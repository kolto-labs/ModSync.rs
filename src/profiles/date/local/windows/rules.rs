//! Pure port of the pinned .NET 9 Windows adjustment-rule operations.
//! Capture qualification and authored-text preservation remain in the adapters.
use super::super::calendar::{
    MAX, TICKS, add_years, checked_shift, clamped_shift, clock, clock_ticks, month_weekday,
};
use super::{SystemTime, Tzi};
use anyhow::{Context, Result, ensure};
use chrono::{Datelike, NaiveDate, NaiveDateTime};

#[derive(Clone, PartialEq, Eq)]
enum Day {
    Fixed(u32),
    Floating { week: u32, weekday: u32 },
}
#[derive(Clone, PartialEq, Eq)]
struct Transition {
    month: u32,
    day: Day,
    time: i64,
}
impl Transition {
    fn from_system_time(value: &SystemTime) -> Result<Self> {
        ensure!(
            (1..=12).contains(&value.month)
                && value.hour < 24
                && value.minute < 60
                && value.second < 60
                && value.millis < 1000,
            "Windows timezone transition calendar/clock is invalid"
        );
        let day = if value.year == 0 {
            ensure!(
                (1..=5).contains(&value.day) && value.weekday < 7,
                "Windows floating timezone transition is invalid"
            );
            Day::Floating {
                week: u32::from(value.day),
                weekday: u32::from(value.weekday),
            }
        } else {
            // The reference discards the native year for fixed transitions.
            ensure!(
                (1..=31).contains(&value.day),
                "Windows fixed timezone transition day is invalid"
            );
            Day::Fixed(u32::from(value.day))
        };
        Ok(Self {
            month: u32::from(value.month),
            day,
            time: (i64::from(value.hour) * 3600
                + i64::from(value.minute) * 60
                + i64::from(value.second))
                * TICKS
                + i64::from(value.millis) * 10_000,
        })
    }
    fn marker(&self) -> bool {
        // TransitionTime's floating constructor stores Day=1 regardless of Week.
        self.month == 1
            && self.time < TICKS
            && matches!(self.day, Day::Fixed(1) | Day::Floating { .. })
    }
    fn transition_time_to_date_time(&self, year: i32) -> Result<i64> {
        let date = match self.day {
            Day::Fixed(mut day) => loop {
                if let Some(date) = NaiveDate::from_ymd_opt(year, self.month, day) {
                    break date;
                }
                ensure!(day > 1, "Windows fixed transition calendar is unavailable");
                day -= 1;
            },
            Day::Floating { week, weekday } => month_weekday(year, self.month, week, weekday)?,
        };
        Ok(clock_ticks(
            date.and_hms_opt(0, 0, 0)
                .context("Windows transition midnight is unavailable")?,
        ) + self.time)
    }
}
struct Rule {
    start: NaiveDate,
    end: NaiveDate,
    standard: i32,
    delta: i32,
    transitions: (Transition, Transition),
    has_daylight: bool,
}
impl Rule {
    fn from_tzi(
        row: &Tzi,
        default_bias: i32,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Option<Self>> {
        if row.standard.month == 0 {
            if row.bias == default_bias {
                return Ok(None);
            }
            let standard = offset_minutes(-i64::from(row.bias))?;
            // Adjacent-year marker helpers inspect these sentinels even though
            // this rule has no daylight saving. Retain the source representation.
            return Ok(Some(Self {
                start,
                end,
                standard,
                delta: 0,
                transitions: (
                    Transition {
                        month: 1,
                        day: Day::Fixed(1),
                        time: 0,
                    },
                    Transition {
                        month: 1,
                        day: Day::Fixed(1),
                        time: 10_000,
                    },
                ),
                has_daylight: false,
            }));
        }
        let daylight = Transition::from_system_time(&row.daylight)?;
        let standard_transition = Transition::from_system_time(&row.standard)?;
        if daylight == standard_transition {
            return Ok(None);
        }
        let mut standard = -i64::from(row.bias) * 60;
        let mut delta = -i64::from(row.daylight_bias) * 60;
        // AdjustmentRule.AdjustDaylightDeltaToExpectedRange performs one transfer,
        // before validation. Preserve the effective daylight offset.
        if delta > 43200 {
            delta -= 86400;
            standard += 86400;
        } else if delta < -43200 {
            delta += 86400;
            standard -= 86400;
        }
        ensure!(
            (-23 * 3600..=14 * 3600).contains(&delta),
            "Windows timezone daylight delta exceeds .NET adjustment-rule range"
        );
        let has_daylight = delta != 0 || daylight.time != 0 || standard_transition.time != 10_000;
        ensure!(
            (-50400..=50400).contains(&(standard + if has_daylight { delta } else { 0 })),
            "Windows timezone daylight offset exceeds .NET range"
        );
        Ok(Some(Self {
            start,
            end,
            standard: i32::try_from(standard)
                .context("Windows standard offset exceeds storage range")?,
            delta: delta as i32,
            transitions: (daylight, standard_transition),
            has_daylight,
        }))
    }
    fn get_daylight_time(&self, year: i32) -> Result<(i64, i64)> {
        let (start, end) = &self.transitions;
        Ok((
            start.transition_time_to_date_time(year)?,
            end.transition_time_to_date_time(year)?,
        ))
    }
}
fn offset_minutes(minutes: i64) -> Result<i32> {
    let seconds = minutes
        .checked_mul(60)
        .context("Windows timezone bias overflow")?;
    ensure!(
        (-50400..=50400).contains(&seconds),
        "Windows timezone offset exceeds .NET range"
    );
    Ok(seconds as i32)
}
fn date(year: i32, month: u32, day: u32) -> Result<NaiveDate> {
    NaiveDate::from_ymd_opt(year, month, day).context("Windows timezone rule date is invalid")
}
fn midnight(year: i32, month: u32, day: u32) -> Result<i64> {
    ensure!(
        (1..=9999).contains(&year),
        "Windows rule year exceeds DateTime range"
    );
    Ok(clock_ticks(
        date(year, month, day)?
            .and_hms_opt(0, 0, 0)
            .context("Windows rule midnight is unavailable")?,
    ))
}

pub(super) struct Zone {
    base: i32,
    rules: Vec<Rule>,
}
struct LocalValue {
    ticks: i64,
    ambiguous_dst: bool,
}
impl Zone {
    pub(super) fn from_registry(
        default: Tzi,
        annual: Vec<(i32, Tzi)>,
        disabled: bool,
    ) -> Result<Self> {
        let base = offset_minutes(-i64::from(default.bias))?;
        let mut rules = Vec::new();
        let first_date = date(1, 1, 1)?;
        let last_date = date(9999, 12, 31)?;
        if annual.is_empty() {
            if let Some(rule) = Rule::from_tzi(&default, default.bias, first_date, last_date)? {
                rules.push(rule);
            }
        } else {
            let count = annual.len();
            for (index, (year, row)) in annual.iter().enumerate() {
                let start = if index == 0 {
                    first_date
                } else {
                    date(*year, 1, 1)?
                };
                let end = if index + 1 == count {
                    last_date
                } else {
                    date(*year, 12, 31)?
                };
                if let Some(rule) = Rule::from_tzi(row, default.bias, start, end)? {
                    rules.push(rule);
                }
            }
        }
        // Successful registry loading with DST disabled removes every adjustment.
        if disabled {
            rules.clear();
        }
        Ok(Self { base, rules })
    }
    fn get_adjustment_rule_for_time(&self, ticks: i64) -> Result<Option<&Rule>> {
        let day = clock(ticks)?.date();
        let index = self.rules.partition_point(|rule| rule.start <= day);
        Ok(index
            .checked_sub(1)
            .and_then(|index| self.rules.get(index))
            .filter(|rule| day <= rule.end))
    }
    fn check_is_dst(
        start: i64,
        mut ticks: i64,
        mut end: i64,
        ignore_year_adjustment: bool,
    ) -> Result<bool> {
        if !ignore_year_adjustment {
            let year = clock(start)?.year();
            let end_year = clock(end)?.year();
            let tick_year = clock(ticks)?.year();
            if end_year != year {
                end = add_years(end, year - end_year)?;
            }
            if tick_year != year {
                ticks = add_years(ticks, year - tick_year)?;
            }
        }
        Ok(if start > end {
            ticks < end || ticks >= start
        } else {
            ticks >= start && ticks < end
        })
    }
    fn try_get_start_of_dst_if_year_end_with_dst(&self, year: i32) -> Result<Option<i64>> {
        let Some(rule) = self.get_adjustment_rule_for_time(midnight(year, 12, 31)?)? else {
            return Ok(None);
        };
        let (start, end) = rule.get_daylight_time(year)?;
        if rule.transitions.1.marker() || start > end {
            return Ok(Some(checked_shift(start, -rule.standard)?));
        }
        Ok(None)
    }
    fn try_get_end_of_dst_if_year_start_with_dst(&self, year: i32) -> Result<Option<i64>> {
        let Some(rule) = self.get_adjustment_rule_for_time(midnight(year, 1, 1)?)? else {
            return Ok(None);
        };
        if rule.transitions.0.marker() {
            let end = if rule.transitions.1.marker() {
                // The reference helper uses Dec 31 midnight, not next Jan 1 minus a tick.
                midnight(year, 12, 31)?
            } else {
                rule.get_daylight_time(year)?.1
            };
            return Ok(Some(checked_shift(end, -(rule.standard + rule.delta))?));
        }
        let (start, end) = rule.get_daylight_time(year)?;
        if end < start && !rule.transitions.1.marker() {
            return Ok(Some(checked_shift(end, -(rule.standard + rule.delta))?));
        }
        Ok(None)
    }
    fn ambiguity_contains(ticks: i64, start: i64, end: i64) -> Result<bool> {
        if ticks >= start && ticks < end {
            return Ok(true);
        }
        if clock(start)?.year() != clock(end)?.year() {
            // Only these optional AddYears retries swallow DateTime range errors.
            for years in [1, -1] {
                if let (Ok(start), Ok(end)) = (add_years(start, years), add_years(end, years))
                    && ticks >= start
                    && ticks < end
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    fn get_is_daylight_savings_from_utc(
        &self,
        ticks: i64,
        year: i32,
        rule: &Rule,
    ) -> Result<(bool, bool)> {
        let (start, end) = rule.get_daylight_time(year)?;
        let mut ignore_year_adjustment = false;
        let start = if rule.transitions.0.marker() && year > 1 {
            if let Some(start) = self.try_get_start_of_dst_if_year_end_with_dst(year - 1)? {
                ignore_year_adjustment = true;
                start
            } else {
                checked_shift(midnight(year, 1, 1)?, -rule.standard)?
            }
        } else {
            checked_shift(start, -rule.standard)?
        };
        let end = if rule.transitions.1.marker() && year < 9999 {
            if let Some(end) = self.try_get_end_of_dst_if_year_start_with_dst(year + 1)? {
                ignore_year_adjustment = true;
                end
            } else {
                checked_shift(midnight(year + 1, 1, 1)? - 1, -(rule.standard + rule.delta))?
            }
        } else {
            checked_shift(end, -(rule.standard + rule.delta))?
        };
        let (ambiguous_start, ambiguous_end) = if rule.delta > 0 {
            (checked_shift(end, -rule.delta)?, end)
        } else {
            (start, checked_shift(start, -rule.delta)?)
        };
        let dst = Self::check_is_dst(start, ticks, end, ignore_year_adjustment)?;
        Ok((
            dst,
            dst && Self::ambiguity_contains(ticks, ambiguous_start, ambiguous_end)?,
        ))
    }
    fn get_utc_offset_from_utc(&self, ticks: i64) -> Result<(i32, bool)> {
        ensure!(
            (0..=MAX).contains(&ticks),
            "UTC date exceeds DateTime range"
        );
        if self.rules.is_empty() {
            return Ok((self.base, false));
        }
        // GetUtcOffsetFromUtc selects endpoint rules through local date-only
        // lookup; ordinary dates instead use the global base-adjusted target.
        let target = if ticks > midnight(9999, 12, 31)? {
            MAX
        } else if ticks < 86400 * TICKS {
            0
        } else {
            checked_shift(ticks, self.base)?
        };
        let Some(rule) = self.get_adjustment_rule_for_time(target)? else {
            return Ok((self.base, false));
        };
        if !rule.has_daylight {
            return Ok((rule.standard, false));
        }
        let (dst, fold) =
            self.get_is_daylight_savings_from_utc(ticks, clock(target)?.year(), rule)?;
        Ok((rule.standard + if dst { rule.delta } else { 0 }, fold))
    }
    fn get_is_daylight_savings(ticks: i64, rule: &Rule, fold: Option<bool>) -> Result<bool> {
        let year = clock(ticks)?.year();
        let (start, end) = rule.get_daylight_time(year)?;
        let marker_start = rule.transitions.0.marker();
        let marker_end = rule.transitions.1.marker();
        let (start_check, end_check) = if fold.is_some() {
            (
                if marker_start {
                    midnight(year, 1, 1)?
                } else {
                    checked_shift(start, rule.delta)?
                },
                if marker_end {
                    midnight(year + 1, 1, 1)? - 1
                } else {
                    end
                },
            )
        } else {
            (
                if marker_start {
                    midnight(year, 1, 1)?
                } else if rule.delta > 0 {
                    checked_shift(start, rule.delta)?
                } else {
                    start
                },
                if marker_end {
                    midnight(year + 1, 1, 1)? - 1
                } else if rule.delta > 0 {
                    checked_shift(end, -rule.delta)?
                } else {
                    end
                },
            )
        };
        let mut dst = Self::check_is_dst(start_check, ticks, end_check, false)?;
        if dst
            && let Some(fold) = fold
            && Self::get_is_ambiguous_time(ticks, rule, start, end)?
        {
            dst = fold;
        }
        Ok(dst)
    }
    fn get_is_ambiguous_time(ticks: i64, rule: &Rule, start: i64, end: i64) -> Result<bool> {
        if rule.delta == 0 {
            return Ok(false);
        }
        let (start, end) = if rule.delta > 0 {
            if rule.transitions.1.marker() {
                return Ok(false);
            }
            (checked_shift(end, -rule.delta)?, end)
        } else {
            if rule.transitions.0.marker() {
                return Ok(false);
            }
            (checked_shift(start, rule.delta)?, start)
        };
        Self::ambiguity_contains(ticks, start, end)
    }
    fn local_offset(&self, ticks: i64, fold: Option<bool>) -> Result<i32> {
        let Some(rule) = self.get_adjustment_rule_for_time(ticks)? else {
            return Ok(self.base);
        };
        if !rule.has_daylight {
            return Ok(rule.standard);
        }
        Ok(rule.standard
            + if Self::get_is_daylight_savings(ticks, rule, fold)? {
                rule.delta
            } else {
                0
            })
    }
    fn to_local(&self, ticks: i64) -> Result<LocalValue> {
        let (offset, fold) = self.get_utc_offset_from_utc(ticks)?;
        let shifted = ticks + i64::from(offset) * TICKS;
        Ok(LocalValue {
            ticks: clamped_shift(ticks, offset),
            ambiguous_dst: (0..=MAX).contains(&shifted) && fold,
        })
    }
    pub(super) fn normalize(&self, wall: NaiveDateTime, offset: Option<i32>) -> Result<String> {
        let wall_ticks = clock_ticks(wall);
        let local = if let Some(offset) = offset {
            let utc = wall_ticks - i64::from(offset) * 60 * TICKS;
            if (0..=MAX).contains(&utc) {
                self.to_local(utc)?
            } else {
                let adjustment = self.local_offset(wall_ticks, None)?;
                LocalValue {
                    ticks: (utc + i64::from(adjustment) * TICKS).clamp(0, MAX),
                    ambiguous_dst: false,
                }
            }
        } else {
            self.to_local(wall_ticks)?
        };
        self.serialize_local(local)
    }
    pub(super) fn normalize_fallback(&self, wall: NaiveDateTime, offset: i32) -> Result<String> {
        // DateTimeParse.AdjustTimeZoneToLocal(full date, bTimeOnly=false) is
        // distinct from Newtonsoft's fast ISO path: its result must not clamp.
        let original = clock_ticks(wall);
        let mut ticks = original - i64::from(offset) * 60 * TICKS;
        let mut fold = false;
        if original < 86400 * TICKS {
            ticks += i64::from(self.local_offset(original, None)?) * TICKS;
            if ticks < 0 {
                ticks += 86400 * TICKS;
            }
        } else if !(0..=MAX).contains(&ticks) {
            ticks += i64::from(self.local_offset(original, None)?) * TICKS;
        } else {
            let (adjustment, ambiguous) = self.get_utc_offset_from_utc(ticks)?;
            ticks += i64::from(adjustment) * TICKS;
            fold = ambiguous;
        }
        if !(0..=MAX).contains(&ticks) {
            return Err(super::super::InvalidFallbackDate.into());
        }
        self.serialize_local(LocalValue {
            ticks,
            ambiguous_dst: fold,
        })
    }
    fn serialize_local(&self, local: LocalValue) -> Result<String> {
        let offset = self.local_offset(local.ticks, Some(local.ambiguous_dst))?;
        let mut text = super::super::super::DateValue {
            clock: clock(local.ticks)?,
            kind: super::super::super::Kind::Unspecified,
        }
        .canonical()
        .context("Windows local clock serialization failed")?;
        let minutes = offset / 60;
        text.push_str(&format!(
            "{}{:02}:{:02}",
            if minutes < 0 { '-' } else { '+' },
            minutes.abs() / 60,
            minutes.abs() % 60
        ));
        Ok(text)
    }
}
