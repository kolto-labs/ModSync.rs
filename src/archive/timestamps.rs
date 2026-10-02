//! Decoder timestamp units and timezone handling, separate from file publication.
use anyhow::{Context, Result};
use chrono::{Local, NaiveDate, TimeZone};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(super) fn unix(seconds: u64) -> Result<SystemTime> {
    UNIX_EPOCH
        .checked_add(Duration::from_secs(seconds))
        .context("Archive timestamp is outside the platform time range")
}

pub(super) fn windows(ticks: u64) -> Result<SystemTime> {
    const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;
    let distance = ticks.abs_diff(UNIX_EPOCH_TICKS);
    let duration = Duration::new(
        distance / 10_000_000,
        ((distance % 10_000_000) * 100) as u32,
    );
    if ticks >= UNIX_EPOCH_TICKS {
        UNIX_EPOCH.checked_add(duration)
    } else {
        UNIX_EPOCH.checked_sub(duration)
    }
    .context("Archive FILETIME is outside the platform time range")
}

pub(super) fn zip(entry: &zip::read::ZipFile<'_>) -> Result<Option<SystemTime>> {
    use zip::extra_fields::ExtraField;
    // UTC-bearing extensions take precedence over the timezone-less DOS clock.
    for field in entry.extra_data_fields() {
        if let ExtraField::Ntfs(time) = field {
            return windows(time.mtime()).map(Some);
        }
    }
    for field in entry.extra_data_fields() {
        if let ExtraField::ExtendedTimestamp(time) = field
            && let Some(seconds) = time.mod_time()
        {
            return unix(u64::from(seconds)).map(Some);
        }
    }
    let Some(value) = entry.last_modified() else {
        return Ok(None);
    };
    let date = NaiveDate::from_ymd_opt(
        i32::from(value.year()),
        u32::from(value.month()),
        u32::from(value.day()),
    )
    .and_then(|date| {
        date.and_hms_opt(
            u32::from(value.hour()),
            u32::from(value.minute()),
            u32::from(value.second()),
        )
    })
    .context("Archive DOS timestamp is invalid")?;
    // DOS timestamps are local wall clocks. Prefer the later instant at a
    // fall-back transition. A nonexistent spring-forward time is not invented.
    let local = Local
        .from_local_datetime(&date)
        .latest()
        .context("Archive DOS timestamp does not exist in the local timezone")?;
    let seconds = local.timestamp();
    let result = if seconds >= 0 {
        UNIX_EPOCH.checked_add(Duration::from_secs(seconds as u64))
    } else {
        UNIX_EPOCH.checked_sub(Duration::from_secs(seconds.unsigned_abs()))
    };
    result
        .context("Archive local timestamp is outside the platform time range")
        .map(Some)
}
