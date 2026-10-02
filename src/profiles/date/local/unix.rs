//! Immutable .NET-compatible Unix local-time snapshot. Unsupported rules return
//! errors to the profile adapter, which preserves authored text with a warning.
#[path = "posix.rs"]
mod posix;
#[path = "tzif.rs"]
mod tzif;

use anyhow::{Context, Result, ensure};
use chrono::{Datelike, NaiveDateTime, Timelike};
use std::{fs, io::Read, path::PathBuf};

const TICKS: i64 = 10_000_000;
const EPOCH: i64 = 621355968000000000;
const MAX: i64 = 3155378975999999999;
fn clock_ticks(clock: NaiveDateTime) -> i64 {
    clock.and_utc().timestamp() * TICKS + EPOCH + i64::from(clock.nanosecond() / 100)
}
fn clock(ticks: i64) -> Result<NaiveDateTime> {
    ensure!(
        (0..=MAX).contains(&ticks),
        "Local date exceeds DateTime range"
    );
    let unix = ticks - EPOCH;
    chrono::DateTime::from_timestamp(
        unix.div_euclid(TICKS),
        (unix.rem_euclid(TICKS) * 100) as u32,
    )
    .map(|date| date.naive_utc())
    .context("Local date calendar is unavailable")
}
fn shift(ticks: i64, seconds: i32) -> i64 {
    (ticks + i64::from(seconds) * TICKS).clamp(0, MAX)
}
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
    future_start: i64,
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
            future_start: MAX,
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
            future_start: MAX,
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
        result.future_start = boundaries.last().context("Timezone transition missing")?.0;
        result.future = posix::Future::parse(&data.tail, base)?;
        Ok(result)
    }
    fn future_utc(&self, ticks: i64) -> Result<Option<(i32, bool)>> {
        if ticks < self.future_start {
            return Ok(None);
        }
        let Some(future) = &self.future else {
            return Ok(None);
        };
        let year = clock(shift(ticks, self.base))?.year();
        let Some((start, end)) = future.changes(year)? else {
            return Ok(Some((future.standard, false)));
        };
        ensure!(
            (0..=MAX).contains(&start)
                && (0..=MAX).contains(&end)
                && clock(start)?.year() == year
                && clock(end)?.year() == year,
            "Cross-year or boundary future timezone transition needs rule stitching"
        );
        let dst = if start > end {
            ticks < end || ticks >= start
        } else {
            ticks >= start && ticks < end
        };
        let daylight = future
            .daylight
            .context("Timezone daylight offset missing")?;
        let delta = i64::from(daylight - future.standard) * TICKS;
        let ambiguous = dst
            && if delta > 0 {
                ticks >= end - delta && ticks < end
            } else {
                ticks >= start && ticks < start - delta
            };
        Ok(Some((
            if dst { daylight } else { future.standard },
            ambiguous,
        )))
    }
    fn utc_offset(&self, ticks: i64) -> Result<(i32, bool)> {
        ensure!(
            (0..=MAX).contains(&ticks),
            "UTC date exceeds DateTime range"
        );
        // .NET selects endpoint adjustment rules through a separate local-clock
        // lookup near MinValue/MaxValue. Preserve authored text until that path
        // and its clamped ambiguous-DST state are implemented exactly.
        ensure!(
            self.rules.is_empty() || (86400 * TICKS..=MAX - 86400 * TICKS).contains(&ticks),
            "Endpoint timezone adjustment-rule selection is not implemented"
        );
        if let Some(value) = self.future_utc(ticks)? {
            return Ok(value);
        }
        let index = self.rules.partition_point(|rule| rule.start <= ticks);
        let Some(rule) = index.checked_sub(1).and_then(|index| self.rules.get(index)) else {
            return Ok((self.base, false));
        };
        ensure!(ticks <= rule.end, "Timezone rules do not cover UTC date");
        let previous = &self.rules[index.saturating_sub(2)];
        if rule.dst {
            ensure!(
                (0..=MAX).contains(&(rule.start + i64::from(previous.total()) * TICKS))
                    && (0..=MAX).contains(&(rule.end + i64::from(rule.total()) * TICKS)),
                "Clamped historical daylight boundaries need endpoint rule handling"
            );
        }
        let delta = i64::from(rule.delta) * TICKS;
        let ambiguous = rule.dst
            && if delta > 0 {
                ticks >= rule.end - delta && ticks < rule.end
            } else {
                ticks >= rule.start && ticks < rule.start - delta
            };
        Ok((rule.total(), ambiguous))
    }
    fn local_offset(&self, ticks: i64, fold: Option<bool>) -> Result<i32> {
        // Same binary-search interval comparison as CompareAdjustmentRuleToDateTime:
        // a UTC start is converted with the previous rule, an end with this rule.
        let mut low = 0;
        let mut high = self.rules.len();
        while low < high {
            let mid = low + (high - low) / 2;
            let rule = &self.rules[mid];
            let previous = &self.rules[mid.saturating_sub(1)];
            if shift(ticks, -previous.total()) < rule.start {
                high = mid;
                continue;
            }
            if shift(ticks, -rule.total()) > rule.end {
                low = mid + 1;
                continue;
            }
            if mid == self.rules.len() - 1 && self.future.is_some() {
                return self.future_local(ticks, fold);
            }
            if !rule.dst {
                return Ok(rule.standard);
            }
            let start = shift(rule.start, previous.total());
            let end = shift(rule.end, rule.total());
            let delta = i64::from(rule.delta) * TICKS;
            let (start_check, end_check) = if fold.is_some() {
                (start + delta, end)
            } else if delta > 0 {
                (start + delta, end - delta)
            } else {
                (start, end)
            };
            ensure!(
                (0..=MAX).contains(&start_check) && (0..=MAX).contains(&end_check),
                "Historical timezone boundary requires unsupported year-marker handling"
            );
            let mut dst = ticks >= start_check && ticks <= end_check;
            let ambiguous = if delta > 0 {
                ticks >= end - delta && ticks < end
            } else {
                ticks >= start && ticks < start - delta
            };
            if dst
                && ambiguous
                && let Some(fold) = fold
            {
                dst = fold;
            }
            return Ok(rule.standard + if dst { rule.delta } else { 0 });
        }
        Ok(self.base)
    }
    fn future_local(&self, ticks: i64, fold: Option<bool>) -> Result<i32> {
        let future = self
            .future
            .as_ref()
            .context("Timezone future rule missing")?;
        let Some((utc_start, utc_end)) = future.changes(clock(ticks)?.year())? else {
            return Ok(future.standard);
        };
        let daylight = future
            .daylight
            .context("Timezone daylight offset missing")?;
        let start = utc_start + i64::from(future.standard) * TICKS;
        let end = utc_end + i64::from(daylight) * TICKS;
        let delta = i64::from(daylight - future.standard) * TICKS;
        let (start_check, end_check) = if fold.is_some() {
            (start + delta, end)
        } else if delta > 0 {
            (start + delta, end - delta)
        } else {
            (start, end)
        };
        ensure!(
            (0..=MAX).contains(&start_check) && (0..=MAX).contains(&end_check),
            "Future timezone boundary exceeds DateTime range"
        );
        let mut dst = if start_check > end_check {
            ticks < end_check || ticks >= start_check
        } else {
            ticks >= start_check && ticks < end_check
        };
        let ambiguous = if delta > 0 {
            ticks >= end - delta && ticks < end
        } else {
            ticks >= start && ticks < start - delta
        };
        if dst
            && ambiguous
            && let Some(fold) = fold
        {
            dst = fold;
        }
        Ok(if dst { daylight } else { future.standard })
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
