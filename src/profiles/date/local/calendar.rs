//! Shared checked DateTime calendar mechanics, independent of timezone policy.
use anyhow::{Context, Result, ensure};
use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};
pub(super) const TICKS: i64 = 10_000_000;
pub(super) const EPOCH: i64 = 621355968000000000;
pub(super) const MAX: i64 = 3155378975999999999;
pub(super) fn clock_ticks(value: NaiveDateTime) -> i64 {
    value.and_utc().timestamp() * TICKS + EPOCH + i64::from(value.nanosecond() / 100)
}
pub(super) fn clock(ticks: i64) -> Result<NaiveDateTime> {
    ensure!((0..=MAX).contains(&ticks), "Date exceeds DateTime range");
    let unix = ticks - EPOCH;
    chrono::DateTime::from_timestamp(
        unix.div_euclid(TICKS),
        (unix.rem_euclid(TICKS) * 100) as u32,
    )
    .map(|value| value.naive_utc())
    .context("Date calendar is unavailable")
}
pub(super) fn checked_shift(ticks: i64, seconds: i32) -> Result<i64> {
    let value = ticks
        .checked_add(i64::from(seconds) * TICKS)
        .context("Date arithmetic overflow")?;
    ensure!(
        (0..=MAX).contains(&value),
        "Date arithmetic exceeds DateTime range"
    );
    Ok(value)
}
pub(super) fn clamped_shift(ticks: i64, seconds: i32) -> i64 {
    (i128::from(ticks) + i128::from(seconds) * i128::from(TICKS)).clamp(0, i128::from(MAX)) as i64
}
pub(super) fn add_years(ticks: i64, years: i32) -> Result<i64> {
    let value = clock(ticks)?;
    let year = value
        .year()
        .checked_add(years)
        .context("Date year overflow")?;
    ensure!(
        (1..=9999).contains(&year),
        "Date year exceeds DateTime range"
    );
    let date = NaiveDate::from_ymd_opt(year, value.month(), value.day())
        .or_else(|| {
            (value.month() == 2 && value.day() == 29)
                .then(|| NaiveDate::from_ymd_opt(year, 2, 28))
                .flatten()
        })
        .context("Date year adjustment failed")?;
    Ok(clock_ticks(date.and_time(value.time())))
}
pub(super) fn month_weekday(year: i32, month: u32, week: u32, weekday: u32) -> Result<NaiveDate> {
    ensure!(
        (1..=9999).contains(&year)
            && (1..=12).contains(&month)
            && (1..=5).contains(&week)
            && weekday <= 6,
        "Invalid month/weekday transition"
    );
    let first =
        NaiveDate::from_ymd_opt(year, month, 1).context("Transition calendar unavailable")?;
    let mut day = 1 + (weekday + 7 - first.weekday().num_days_from_sunday()) % 7 + (week - 1) * 7;
    while NaiveDate::from_ymd_opt(year, month, day).is_none() {
        day -= 7;
    }
    NaiveDate::from_ymd_opt(year, month, day).context("Transition date unavailable")
}
