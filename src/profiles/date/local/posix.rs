//! Future rules follow .NET 9 TZif_CreateTransitionTimeFromPosixRule, including
//! its day-stripping transition-time behavior and limited numeric Julian support.
use super::super::calendar::month_weekday;
use super::{TICKS, clock_ticks, minute_delta};
use anyhow::{Context, Result, ensure};
use chrono::NaiveDate;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Day {
    Month(u32, u32, u32),
    Julian(u32),
    Numeric(u32),
}
#[derive(PartialEq, Eq)]
pub(super) struct Change {
    day: Day,
    seconds: i32,
}
pub(super) struct Future {
    pub standard: i32,
    pub daylight: Option<i32>,
    start: Option<Change>,
    end: Option<Change>,
}
fn name(text: &str, at: &mut usize) -> Result<()> {
    let b = text.as_bytes();
    let start = *at;
    if b.get(*at) == Some(&b'<') {
        *at += 1;
        while b.get(*at).is_some_and(|c| *c != b'>') {
            *at += 1;
        }
        ensure!(
            *at > start + 1 && b.get(*at) == Some(&b'>'),
            "Malformed quoted timezone name"
        );
        *at += 1;
    } else {
        while b.get(*at).is_some_and(u8::is_ascii_alphabetic) {
            *at += 1;
        }
        ensure!(*at > start, "Missing timezone name");
    }
    Ok(())
}
fn offset(text: &str, at: &mut usize) -> Result<Option<i32>> {
    let start = *at;
    let b = text.as_bytes();
    if matches!(b.get(*at), Some(b'+' | b'-')) {
        *at += 1;
    }
    while b.get(*at).is_some_and(|c| c.is_ascii_digit() || *c == b':') {
        *at += 1;
    }
    if *at == start {
        return Ok(None);
    }
    let raw = &text[start..*at];
    let sign = if raw.starts_with('-') { -1 } else { 1 };
    let parts = raw
        .trim_start_matches(['+', '-'])
        .split(':')
        .collect::<Vec<_>>();
    ensure!(
        (1..=3).contains(&parts.len()),
        "Unsupported timezone offset"
    );
    let numbers = parts
        .iter()
        .map(|s| s.parse::<i32>())
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(
        numbers[0] <= 167 && numbers.iter().skip(1).all(|v| (0..60).contains(v)),
        "Timezone offset exceeds supported bounds"
    );
    Ok(Some(
        sign * (numbers[0] * 3600
            + numbers.get(1).copied().unwrap_or(0) * 60
            + numbers.get(2).copied().unwrap_or(0)),
    ))
}
fn change(text: &str) -> Result<Option<Change>> {
    let (day, time) = text
        .split_once('/')
        .map_or((text, None), |(d, t)| (d, Some(t)));
    let day = if let Some(month) = day.strip_prefix('M') {
        let n = month
            .split('.')
            .map(str::parse::<u32>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ensure!(
            n.len() == 3 && (1..=12).contains(&n[0]) && (1..=5).contains(&n[1]) && n[2] <= 6,
            "Malformed timezone month rule"
        );
        Day::Month(n[0], n[1], n[2])
    } else if let Some(julian) = day.strip_prefix('J') {
        let n: u32 = julian.parse()?;
        ensure!((1..=365).contains(&n), "Malformed timezone Julian rule");
        Day::Julian(n)
    } else {
        let n: u32 = day.parse()?;
        // The reference ignores this tail and retains its final historical rule.
        if n >= 59 {
            return Ok(None);
        }
        Day::Numeric(n)
    };
    let seconds = if let Some(time) = time {
        let mut at = 0;
        let seconds = offset(time, &mut at)?.context("Missing timezone transition time")?;
        ensure!(at == time.len(), "Unsupported timezone transition suffix");
        seconds.rem_euclid(86400)
    } else {
        7200
    };
    Ok(Some(Change { day, seconds }))
}
impl Future {
    pub fn parse(text: &str, base: i32) -> Result<Option<Self>> {
        if text.is_empty() {
            return Ok(None);
        }
        let mut at = 0;
        name(text, &mut at)?;
        let standard = -offset(text, &mut at)?.context("Missing timezone standard offset")?;
        let standard_delta = minute_delta(standard - base);
        let mut standard = base + standard_delta;
        if at == text.len() {
            // Without daylight transitions, standard is the effective rule offset.
            ensure!(
                (-50400..=50400).contains(&standard),
                "Timezone future effective offset exceeds supported range"
            );
            return Ok(Some(Self {
                standard,
                daylight: None,
                start: None,
                end: None,
            }));
        }
        name(text, &mut at)?;
        let daylight = offset(text, &mut at)?
            .map(|offset| standard + minute_delta(minute_delta(-offset - base) - standard_delta))
            .unwrap_or(standard + 3600);
        let delta = daylight - standard;
        if delta > 12 * 3600 {
            standard += 24 * 3600;
        } else if delta < -12 * 3600 {
            standard -= 24 * 3600;
        }
        ensure!(
            (-23 * 3600..=14 * 3600).contains(&(daylight - standard)),
            "Future daylight delta exceeds adjustment rule range"
        );
        ensure!(
            text.as_bytes().get(at) == Some(&b','),
            "Timezone daylight rules are missing"
        );
        let rules = &text[at + 1..];
        let (start, end) = rules
            .split_once(',')
            .context("Timezone end rule is missing")?;
        let (Some(start), Some(end)) = (change(start)?, change(end)?) else {
            return Ok(None);
        };
        ensure!(
            !start.same_definition(&end),
            "Timezone future transitions are identical"
        );
        // Reference admission checks the effective daylight total. The single
        // day transfer can place standard outside fourteen hours; the delta
        // bound above and this total bound still limit it to [-28h, +37h].
        ensure!(
            (-50400..=50400).contains(&daylight),
            "Timezone future effective offset exceeds supported range"
        );
        Ok(Some(Self {
            standard,
            daylight: Some(daylight),
            start: Some(start),
            end: Some(end),
        }))
    }
    pub fn wall_changes(&self, year: i32) -> Result<Option<(i64, i64)>> {
        let (Some(start), Some(end)) = (&self.start, &self.end) else {
            return Ok(None);
        };
        Ok(Some((start.ticks(year)?, end.ticks(year)?)))
    }
    pub fn markers(&self) -> (bool, bool) {
        (
            self.start.as_ref().is_some_and(Change::marker),
            self.end.as_ref().is_some_and(Change::marker),
        )
    }
}
impl Change {
    fn marker(&self) -> bool {
        self.seconds == 0
            && matches!(
                self.day,
                Day::Julian(1) | Day::Numeric(0) | Day::Month(1, _, _)
            )
    }

    fn same_definition(&self, other: &Self) -> bool {
        self.seconds == other.seconds
            && (self.day == other.day
                || match (self.day, other.day) {
                    // Both reference factories produce the same fixed calendar day.
                    // Numeric rules are admitted only before leap day (0..=58).
                    (Day::Julian(j), Day::Numeric(n)) | (Day::Numeric(n), Day::Julian(j)) => {
                        j == n + 1
                    }
                    _ => false,
                })
    }
    fn ticks(&self, year: i32) -> Result<i64> {
        let date = match self.day {
            Day::Month(month, week, weekday) => month_weekday(year, month, week, weekday)?,
            Day::Julian(n) => {
                let ordinal = if n >= 60 && NaiveDate::from_ymd_opt(year, 2, 29).is_some() {
                    n + 1
                } else {
                    n
                };
                NaiveDate::from_yo_opt(year, ordinal).context("Timezone Julian day")?
            }
            Day::Numeric(n) => {
                NaiveDate::from_yo_opt(year, n + 1).context("Timezone numeric day")?
            }
        };
        Ok(
            clock_ticks(date.and_hms_opt(0, 0, 0).context("Timezone midnight")?)
                + i64::from(self.seconds) * TICKS,
        )
    }
}
