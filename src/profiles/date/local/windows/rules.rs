//! Pure bounded port of .NET 9 TimeZoneInfo's ordinary Windows adjustment rules.
//! Year markers, cross-year transition windows and adjusted endpoints are refused.
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
    transitions: Option<(Transition, Transition)>,
}
impl Rule {
    fn from_tzi(
        row: &Tzi,
        default_bias: i32,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Option<Self>> {
        if row.standard.month == 0 {
            let standard = offset_minutes(-i64::from(row.bias))?;
            return Ok((row.bias != default_bias).then_some(Self {
                start,
                end,
                standard,
                delta: 0,
                transitions: None,
            }));
        }
        let daylight = Transition::from_system_time(&row.daylight)?;
        let standard_transition = Transition::from_system_time(&row.standard)?;
        if daylight == standard_transition {
            return Ok(None);
        }
        let standard = offset_minutes(-i64::from(row.bias))?;
        let delta = -i64::from(row.daylight_bias) * 60;
        ensure!(
            (-43200..=43200).contains(&delta),
            "Windows timezone daylight delta requires unsupported 24-hour rebasing"
        );
        let delta = delta as i32;
        ensure!(
            (-50400..=50400).contains(&(standard + delta)),
            "Windows timezone daylight offset exceeds .NET range"
        );
        let has_daylight = delta != 0 || daylight.time != 0 || standard_transition.time != 10_000;
        Ok(Some(Self {
            start,
            end,
            standard,
            delta,
            transitions: has_daylight.then_some((daylight, standard_transition)),
        }))
    }
    fn get_daylight_time(&self, year: i32) -> Result<(i64, i64)> {
        let (start, end) = self
            .transitions
            .as_ref()
            .context("Windows daylight rule has no transitions")?;
        ensure!(
            !start.marker() && !end.marker(),
            "Windows timezone year markers require adjacent-rule stitching"
        );
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
    fn check_is_dst(start: i64, ticks: i64, end: i64) -> Result<bool> {
        let year = clock(start)?.year();
        let end_year = clock(end)?.year();
        let tick_year = clock(ticks)?.year();
        let end = if end_year == year {
            end
        } else {
            add_years(end, year - end_year)?
        };
        let ticks = if tick_year == year {
            ticks
        } else {
            add_years(ticks, year - tick_year)?
        };
        Ok(if start > end {
            ticks < end || ticks >= start
        } else {
            ticks >= start && ticks < end
        })
    }
    fn get_is_daylight_savings_from_utc(
        &self,
        ticks: i64,
        year: i32,
        rule: &Rule,
    ) -> Result<(bool, bool)> {
        let (start, end) = rule.get_daylight_time(year)?;
        let start = checked_shift(start, -rule.standard)?;
        let end = checked_shift(end, -(rule.standard + rule.delta))?;
        ensure!(
            clock(start)?.year() == year && clock(end)?.year() == year,
            "Windows UTC transition crosses calendar year; rule stitching is unsupported"
        );
        let (ambiguous_start, ambiguous_end) = if rule.delta > 0 {
            (checked_shift(end, -rule.delta)?, end)
        } else {
            (start, checked_shift(start, -rule.delta)?)
        };
        ensure!(
            clock(ambiguous_start)?.year() == clock(ambiguous_end)?.year(),
            "Windows UTC ambiguity crosses calendar year; rule stitching is unsupported"
        );
        let dst = Self::check_is_dst(start, ticks, end)?;
        Ok((
            dst,
            dst && ticks >= ambiguous_start && ticks < ambiguous_end,
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
        ensure!(
            self.rules.is_empty() || (86400 * TICKS..=MAX - 86400 * TICKS).contains(&ticks),
            "Windows endpoint adjustment-rule selection is not implemented"
        );
        let target = checked_shift(ticks, self.base)?;
        let Some(rule) = self.get_adjustment_rule_for_time(target)? else {
            return Ok((self.base, false));
        };
        if rule.transitions.is_none() {
            return Ok((rule.standard, false));
        }
        let (dst, fold) =
            self.get_is_daylight_savings_from_utc(ticks, clock(target)?.year(), rule)?;
        Ok((rule.standard + if dst { rule.delta } else { 0 }, fold))
    }
    fn get_is_daylight_savings(ticks: i64, rule: &Rule, fold: Option<bool>) -> Result<bool> {
        let year = clock(ticks)?.year();
        let (start, end) = rule.get_daylight_time(year)?;
        let (start_check, end_check) = if fold.is_some() {
            (checked_shift(start, rule.delta)?, end)
        } else if rule.delta > 0 {
            (
                checked_shift(start, rule.delta)?,
                checked_shift(end, -rule.delta)?,
            )
        } else {
            (start, end)
        };
        ensure!(
            clock(start_check)?.year() == year && clock(end_check)?.year() == year,
            "Windows local daylight interval crosses calendar year; rule stitching is unsupported"
        );
        let mut dst = Self::check_is_dst(start_check, ticks, end_check)?;
        if dst && let Some(fold) = fold {
            let (ambiguous_start, ambiguous_end) = if rule.delta > 0 {
                (checked_shift(end, -rule.delta)?, end)
            } else {
                (checked_shift(start, rule.delta)?, start)
            };
            ensure!(
                clock(ambiguous_start)?.year() == clock(ambiguous_end)?.year(),
                "Windows local ambiguity crosses calendar year; rule stitching is unsupported"
            );
            if ticks >= ambiguous_start && ticks < ambiguous_end {
                dst = fold;
            }
        }
        Ok(dst)
    }
    fn local_offset(&self, ticks: i64, fold: Option<bool>) -> Result<i32> {
        ensure!(
            self.rules.is_empty() || (86400 * TICKS..=MAX - 86400 * TICKS).contains(&ticks),
            "Windows endpoint local adjustment-rule selection is not implemented"
        );
        let Some(rule) = self.get_adjustment_rule_for_time(ticks)? else {
            return Ok(self.base);
        };
        if rule.transitions.is_none() {
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
