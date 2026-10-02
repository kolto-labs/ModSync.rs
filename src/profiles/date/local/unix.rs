//! Immutable .NET-compatible Unix local-time snapshot. Unsupported rules return
//! errors to the profile adapter, which preserves authored text with a warning.
#[path = "posix.rs"]
mod posix;
#[path = "tzif.rs"]
mod tzif;

use super::calendar::{
    EPOCH, MAX, TICKS, add_years, checked_shift, clamped_shift as shift, clock, clock_ticks,
};
use anyhow::{Context, Result, ensure};
use chrono::{Datelike, NaiveDate, NaiveDateTime};
use std::{fs, io::Read, path::PathBuf};

fn minute_delta(seconds: i32) -> i32 {
    if seconds % 60 == 0 {
        seconds
    } else {
        (seconds % 86400) / 60 * 60
    }
}
fn normalize(offset: i32, base: i32) -> i32 {
    (base + minute_delta(offset - base)).clamp(-50400, 50400)
}

fn date_ticks(year: i32, month: u32, day: u32) -> Result<i64> {
    ensure!(
        (1..=9999).contains(&year),
        "Date year exceeds DateTime range"
    );
    Ok(clock_ticks(
        NaiveDate::from_ymd_opt(year, month, day)
            .context("Date unavailable")?
            .and_hms_opt(0, 0, 0)
            .context("Date midnight unavailable")?,
    ))
}
fn check_dst(
    start: i64,
    mut time: i64,
    mut end: i64,
    ignore_year: bool,
    historical: bool,
) -> Result<bool> {
    let start_year = clock(start)?.year();
    let end_year = clock(end)?.year();
    if !ignore_year && !historical {
        if start_year != end_year {
            end = add_years(end, start_year - end_year)?;
        }
        let year = clock(time)?.year();
        if start_year != year {
            time = add_years(time, start_year - year)?;
        }
    }
    Ok(if start > end {
        time < end || time >= start
    } else if historical {
        time >= start && time <= end
    } else {
        time >= start && time < end
    })
}
fn ambiguous_range(time: i64, start: i64, end: i64) -> Result<bool> {
    let start_year = clock(start)?.year();
    let end_year = clock(end)?.year();
    if time >= start && time < end {
        return Ok(true);
    }
    if start_year != end_year {
        for years in [1, -1] {
            if let (Ok(start), Ok(end)) = (add_years(start, years), add_years(end, years))
                && time >= start
                && time < end
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[derive(Clone, Copy)]
struct Rule {
    start: i64,
    end: i64,
    standard: i32,
    delta: i32,
    dst: bool,
}
impl Rule {
    fn total(&self) -> i32 {
        self.standard + self.delta
    }
}
pub(super) struct Snapshot {
    base: i32,
    rules: Vec<Rule>,
    future: Option<posix::Future>,
}
struct LocalValue {
    ticks: i64,
    ambiguous_dst: bool,
}

impl Snapshot {
    pub(super) fn capture() -> Result<Self> {
        {
            let tz = std::env::var_os("TZ");
            let directory = std::env::var_os("TZDIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/usr/share/zoneinfo"));
            let paths = if let Some(tz) = tz {
                let tz = tz.to_str().context("Non-UTF8 TZ snapshot is unsupported")?;
                let tz = tz.strip_prefix(':').unwrap_or(tz);
                if tz.is_empty() {
                    return Ok(Self::utc());
                }
                ensure!(tz.len() <= 4096, "Timezone path exceeds snapshot bound");
                let path = PathBuf::from(tz);
                vec![if path.is_absolute() {
                    path
                } else {
                    directory.join(path)
                }]
            } else {
                vec![PathBuf::from("/etc/localtime"), directory.join("localtime")]
            };
            let mut selected = None;
            for path in paths {
                match fs::canonicalize(&path) {
                    Ok(path) => {
                        selected = Some(path);
                        break;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => {
                        return Err(error).context("Could not resolve local timezone snapshot");
                    }
                }
            }
            let path =
                selected.context("Local timezone snapshot is missing; authored date retained")?;
            let mut file = crate::paths::open_regular_read(&path)?;
            let before = file.metadata()?;
            ensure!(
                before.len() > 0 && before.len() <= 1024 * 1024,
                "Timezone snapshot exceeds 1 MiB or is empty"
            );
            let mut bytes = Vec::new();
            file.by_ref()
                .take(1024 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            let after = file.metadata()?;
            let current = crate::paths::open_regular_read(&path)?.metadata()?;
            use std::os::unix::fs::MetadataExt;
            ensure!(
                bytes.len() as u64 == before.len()
                    && after.len() == before.len()
                    && after.modified()? == before.modified()?
                    && current.dev() == before.dev()
                    && current.ino() == before.ino()
                    && current.len() == before.len()
                    && current.modified()? == before.modified()?,
                "Timezone file changed while taking snapshot"
            );
            Self::from_data(tzif::parse(&bytes)?, chrono::Utc::now().timestamp())
        }
    }
    fn utc() -> Self {
        Self {
            base: 0,
            rules: Vec::new(),
            future: None,
        }
    }
    fn from_data(data: tzif::Data, now: i64) -> Result<Self> {
        let mut base = 0;
        if data.transitions.is_empty() {
            for kind in &data.types {
                if !kind.dst {
                    base = kind.offset;
                }
            }
        } else {
            for &(at, index) in &data.transitions {
                if at > now {
                    break;
                }
                if !data.types[index].dst {
                    base = data.types[index].offset;
                }
            }
        }
        base = minute_delta(base);
        ensure!(
            (-50400..=50400).contains(&base),
            "Timezone base offset exceeds .NET range"
        );
        let mut result = Self {
            base,
            rules: Vec::new(),
            future: None,
        };
        if data.transitions.is_empty() {
            return Ok(result);
        }
        let early = data.types.iter().position(|kind| !kind.dst).unwrap_or(0);
        let mut boundaries = Vec::new();
        for (at, index) in data.transitions {
            let ticks = (i128::from(at) * i128::from(TICKS) + i128::from(EPOCH))
                .clamp(0, i128::from(MAX)) as i64;
            if ticks == 0 {
                continue;
            }
            ensure!(
                boundaries
                    .last()
                    .is_none_or(|(previous, _)| *previous < ticks),
                "Clamped timezone transitions overlap"
            );
            boundaries.push((ticks, index));
        }
        // Source drops pre-MinValue transitions before constructing early rules.
        if boundaries.is_empty() {
            anyhow::bail!("Timezone has no supported in-range transition rules");
        }
        let make = |start, end, index: usize| {
            let kind = &data.types[index];
            let total = normalize(kind.offset, base);
            let mut delta = if kind.dst {
                minute_delta(kind.offset - base)
            } else {
                0
            };
            // AdjustmentRule normalizes extreme daylight deltas by one day,
            // moving the opposite day into BaseUtcOffsetDelta (total unchanged).
            if delta > 12 * 3600 {
                delta -= 24 * 3600;
            } else if delta < -12 * 3600 {
                delta += 24 * 3600;
            }
            Rule {
                start,
                end,
                standard: total - delta,
                delta,
                dst: kind.dst,
            }
        };
        result.rules.push(make(0, boundaries[0].0 - 1, early));
        for (position, &(at, index)) in boundaries.iter().enumerate() {
            let end = boundaries
                .get(position + 1)
                .map_or(MAX, |(next, _)| next - 1);
            result.rules.push(make(at, end, index));
        }
        ensure!(
            result
                .rules
                .iter()
                .all(|rule| (-23 * 3600..=14 * 3600).contains(&rule.delta)),
            "Historical daylight delta exceeds .NET adjustment rule range"
        );
        result.future = posix::Future::parse(&data.tail, base)?;
        Ok(result)
    }
    fn effective(&self, index: usize) -> Rule {
        let mut rule = self.rules[index];
        if index + 1 == self.rules.len()
            && let Some(future) = &self.future
        {
            rule.standard = future.standard;
            rule.delta = future.daylight.map_or(0, |value| value - future.standard);
            rule.dst = future.daylight.is_some();
        }
        rule
    }
    fn recurring(&self, index: usize) -> bool {
        index + 1 == self.rules.len()
            && self
                .future
                .as_ref()
                .is_some_and(|future| future.daylight.is_some())
    }
    fn markers(&self, index: usize) -> (bool, bool) {
        if self.recurring(index) {
            self.future
                .as_ref()
                .map_or((false, false), posix::Future::markers)
        } else {
            (false, false)
        }
    }
    fn select(&self, ticks: i64, utc: bool) -> Option<usize> {
        let mut low = 0;
        let mut high = self.rules.len().checked_sub(1)?;
        while low <= high {
            let mid = low + (high - low) / 2;
            let rule = self.effective(mid);
            let previous = self.effective(mid.saturating_sub(1));
            let start_compare = if utc {
                ticks
            } else {
                shift(ticks, -previous.total())
            };
            if start_compare < rule.start {
                high = mid.checked_sub(1)?;
                continue;
            }
            let end_compare = if utc {
                ticks
            } else {
                shift(ticks, -rule.total())
            };
            if end_compare > rule.end {
                low = mid + 1;
            } else {
                return Some(mid);
            }
        }
        None
    }
    fn daylight(&self, index: usize, year: i32) -> Result<(i64, i64)> {
        if self.recurring(index) {
            return self
                .future
                .as_ref()
                .context("Future rule unavailable")?
                .wall_changes(year)?
                .context("Future transitions unavailable");
        }
        let rule = self.effective(index);
        let previous = self.effective(index.saturating_sub(1));
        Ok((
            shift(rule.start, previous.total()),
            shift(rule.end, rule.total()),
        ))
    }
    fn previous_season_start(&self, year: i32) -> Result<Option<i64>> {
        let at = date_ticks(year, 12, 31)?;
        let Some(index) = self.select(at, false) else {
            return Ok(None);
        };
        let (start, end) = self.daylight(index, year)?;
        if self.markers(index).1 || start > end {
            Ok(Some(checked_shift(start, -self.effective(index).standard)?))
        } else {
            Ok(None)
        }
    }
    fn next_season_end(&self, year: i32) -> Result<Option<i64>> {
        let at = date_ticks(year, 1, 1)?;
        let Some(index) = self.select(at, false) else {
            return Ok(None);
        };
        let rule = self.effective(index);
        let (start_marker, end_marker) = self.markers(index);
        if start_marker && end_marker {
            return Ok(Some(checked_shift(
                date_ticks(year, 12, 31)?,
                -rule.total(),
            )?));
        }
        let (start, end) = self.daylight(index, year)?;
        if start_marker || (end < start && !end_marker) {
            Ok(Some(checked_shift(end, -rule.total())?))
        } else {
            Ok(None)
        }
    }
    fn utc_offset(&self, ticks: i64) -> Result<(i32, bool)> {
        ensure!(
            (0..=MAX).contains(&ticks),
            "UTC date exceeds DateTime range"
        );
        let (index, year) = if ticks > date_ticks(9999, 12, 31)? {
            (self.select(MAX, false), 9999)
        } else if ticks < 86400 * TICKS {
            (self.select(0, false), 1)
        } else {
            (
                self.select(ticks, true),
                clock(checked_shift(ticks, self.base)?)?.year(),
            )
        };
        let Some(index) = index else {
            return Ok((self.base, false));
        };
        let rule = self.effective(index);
        if !rule.dst {
            return Ok((rule.standard, false));
        }
        let (wall_start, wall_end) = self.daylight(index, year)?;
        let (start_marker, end_marker) = self.markers(index);
        let start_offset = if self.recurring(index) {
            rule.standard
        } else {
            self.effective(index.saturating_sub(1)).total()
        };
        let mut ignore_year = false;
        let start = if start_marker && year > 1 {
            if let Some(value) = self.previous_season_start(year - 1)? {
                ignore_year = true;
                value
            } else {
                checked_shift(date_ticks(year, 1, 1)?, -start_offset)?
            }
        } else {
            checked_shift(wall_start, -start_offset)?
        };
        let end = if end_marker && year < 9999 {
            if let Some(value) = self.next_season_end(year + 1)? {
                ignore_year = true;
                value
            } else {
                checked_shift(date_ticks(year + 1, 1, 1)? - 1, -rule.total())?
            }
        } else {
            checked_shift(wall_end, -rule.total())?
        };
        let dst = check_dst(start, ticks, end, ignore_year, !self.recurring(index))?;
        let delta = i64::from(rule.delta) * TICKS;
        let (amb_start, amb_end) = if delta > 0 {
            (end - delta, end)
        } else {
            (start, start - delta)
        };
        let ambiguous = dst && ambiguous_range(ticks, amb_start, amb_end)?;
        Ok((rule.standard + if dst { rule.delta } else { 0 }, ambiguous))
    }
    fn local_offset(&self, ticks: i64, fold: Option<bool>) -> Result<i32> {
        let Some(index) = self.select(ticks, false) else {
            return Ok(self.base);
        };
        let rule = self.effective(index);
        if !rule.dst {
            return Ok(rule.standard);
        }
        let year = clock(ticks)?.year();
        let (start, end) = self.daylight(index, year)?;
        let (start_marker, end_marker) = self.markers(index);
        let delta = i64::from(rule.delta) * TICKS;
        let start_check = if start_marker {
            date_ticks(year, 1, 1)?
        } else if fold.is_some() || delta > 0 {
            start + delta
        } else {
            start
        };
        let end_check = if end_marker {
            date_ticks(year + 1, 1, 1)? - 1
        } else if fold.is_none() && delta > 0 {
            end - delta
        } else {
            end
        };
        let mut dst = check_dst(start_check, ticks, end_check, false, !self.recurring(index))?;
        if dst && let Some(fold) = fold {
            let ambiguous = if delta > 0 {
                !end_marker && ambiguous_range(ticks, end - delta, end)?
            } else {
                !start_marker && ambiguous_range(ticks, start + delta, start)?
            };
            if ambiguous {
                dst = fold;
            }
        }
        Ok(rule.standard + if dst { rule.delta } else { 0 })
    }
    fn to_local(&self, ticks: i64) -> Result<LocalValue> {
        let (offset, fold) = self.utc_offset(ticks)?;
        let shifted = ticks + i64::from(offset) * TICKS;
        Ok(LocalValue {
            ticks: shifted.clamp(0, MAX),
            ambiguous_dst: (0..=MAX).contains(&shifted) && fold,
        })
    }
}

impl Snapshot {
    pub(super) fn normalize(&self, wall: NaiveDateTime, offset: Option<i32>) -> Result<String> {
        let wall_ticks = clock_ticks(wall);
        let local = if let Some(offset) = offset {
            let utc = wall_ticks - i64::from(offset) * 60 * TICKS;
            if (0..=MAX).contains(&utc) {
                self.to_local(utc)?
            } else {
                // Newtonsoft fast ISO uses the original Unspecified clock's
                // local offset only in the out-of-range UTC branch.
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
            let (adjustment, ambiguous) = self.utc_offset(ticks)?;
            ticks += i64::from(adjustment) * TICKS;
            fold = ambiguous;
        }
        if !(0..=MAX).contains(&ticks) {
            return Err(super::InvalidFallbackDate.into());
        }
        self.serialize_local(LocalValue {
            ticks,
            ambiguous_dst: fold,
        })
    }
    fn serialize_local(&self, local: LocalValue) -> Result<String> {
        let offset = self.local_offset(local.ticks, Some(local.ambiguous_dst))?;
        let clock = clock(local.ticks)?;
        let mut text = super::super::DateValue {
            clock,
            kind: super::super::Kind::Unspecified,
        }
        .canonical()
        .context("Local clock serialization failed")?;
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
