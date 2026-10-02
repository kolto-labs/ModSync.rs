//! Bounded full-calendar-date subset of the invariant framework fallback.
//! Specialized Newtonsoft readers always run before this module.
use super::{DateValue, Kind};
use anyhow::{Context, Result, ensure};
use chrono::{Datelike, NaiveDate};

pub(in super::super) fn parse(text: &str) -> Result<Option<DateValue>> {
    ensure!(text.len() <= 4096, "Fallback date exceeds text bound");
    if let Some((date, rest)) = calendar_prefix(text.trim()) {
        return iso_fallback(&format!("{}{rest}", date.format("%Y-%m-%d")));
    }
    iso_fallback(text)
}
// Explicit four-digit-year, fully specified invariant calendar forms only.
// Month names are fixed CalendarData.Invariant data, not a locale guess.
fn calendar_prefix(text: &str) -> Option<(NaiveDate, &str)> {
    if !text.is_ascii() {
        return None;
    }
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    let first = &text[..end];
    let rest = &text[end..];
    let parts = first.split('/').collect::<Vec<_>>();
    if parts.len() == 3 {
        let (year, month, day) = if parts[0].len() == 4 {
            (parts[0], parts[1], parts[2])
        } else if parts[2].len() == 4 {
            (parts[2], parts[0], parts[1])
        } else {
            return None;
        };
        return complete_date(year, month, day).map(|date| (date, rest));
    }
    let mut tokens = text.split_whitespace();
    let first = tokens.next()?;
    let second = tokens.next()?;
    let third = tokens.next()?;
    // Locate the original suffix without reconstructing its clock punctuation.
    let third_at = third.as_ptr() as usize - text.as_ptr() as usize;
    let suffix = &text[third_at + third.len()..];
    let (month, day, year) = if let Some(month) = invariant_month(first) {
        (month, second.strip_suffix(',').unwrap_or(second), third)
    } else {
        (invariant_month(second)?, first, third)
    };
    complete_date(year, &month.to_string(), day).map(|date| (date, suffix))
}
fn complete_date(year: &str, month: &str, day: &str) -> Option<NaiveDate> {
    if year.len() != 4
        || !(1..=2).contains(&month.len())
        || !(1..=2).contains(&day.len())
        || !year
            .bytes()
            .chain(month.bytes())
            .chain(day.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let year = year.parse::<i32>().ok()?;
    if year < 1 {
        return None;
    }
    NaiveDate::from_ymd_opt(year, month.parse().ok()?, day.parse().ok()?)
}
fn invariant_month(text: &str) -> Option<u32> {
    const MONTHS: [(&str, &str); 12] = [
        ("January", "Jan"),
        ("February", "Feb"),
        ("March", "Mar"),
        ("April", "Apr"),
        ("May", "May"),
        ("June", "Jun"),
        ("July", "Jul"),
        ("August", "Aug"),
        ("September", "Sep"),
        ("October", "Oct"),
        ("November", "Nov"),
        ("December", "Dec"),
    ];
    MONTHS
        .iter()
        .position(|(full, short)| {
            text.eq_ignore_ascii_case(full) || text.eq_ignore_ascii_case(short)
        })
        .map(|i| i as u32 + 1)
}
fn iso_fallback(text: &str) -> Result<Option<DateValue>> {
    let text = text.trim();
    let b = text.as_bytes();
    if b.len() < 10
        || b.get(4) != Some(&b'-')
        || b.get(7) != Some(&b'-')
        || !b[..4]
            .iter()
            .chain(&b[5..7])
            .chain(&b[8..10])
            .all(u8::is_ascii_digit)
    {
        return Ok(None);
    }
    // Leave non-ASCII and additional grammar to the explicitly retained legacy
    // fallback until their framework tokenization has a bounded implementation.
    if !text.is_ascii() {
        return Ok(None);
    }
    let number = |s: &str| {
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
            None
        } else {
            s.parse::<u32>().ok()
        }
    };
    let year = number(&text[..4]).context("Invalid fallback year")?;
    let month = number(&text[5..7]).context("Invalid fallback month")?;
    let day = number(&text[8..10]).context("Invalid fallback day")?;
    let Some(date) = NaiveDate::from_ymd_opt(year as i32, month, day).filter(|_| year >= 1) else {
        return Ok(None);
    };
    if b.len() == 10 {
        return Ok(Some(DateValue {
            clock: date.and_hms_opt(0, 0, 0).context("Date midnight")?,
            kind: Kind::Unspecified,
        }));
    }
    if !matches!(b[10], b'T' | b' ') {
        return Ok(None);
    }
    let mut rest = text[11..].trim_start_matches(|c: char| c.is_ascii_whitespace());
    if rest.len() < 5 || rest.as_bytes().get(2) != Some(&b':') {
        return Ok(None);
    }
    let (Some(hour), Some(minute)) = (number(&rest[..2]), number(&rest[3..5])) else {
        return Ok(None);
    };
    rest = &rest[5..];
    let mut second = 0;
    let mut fraction = 0.0_f64;
    if let Some(after) = rest.strip_prefix(':') {
        if after.len() < 2 {
            return Ok(None);
        }
        let Some(value) = number(&after[..2]) else {
            return Ok(None);
        };
        second = value;
        rest = &after[2..];
        if rest.starts_with(['.', ',']) {
            rest = &rest[1..];
            let mut base = 0.1_f64;
            let count = rest.bytes().take_while(u8::is_ascii_digit).count();
            if count == 0 {
                return Ok(None);
            }
            for digit in rest.bytes().take(count) {
                // Keep the source's independent binary64 multiply and add.
                fraction += f64::from(digit - b'0') * base;
                base *= 0.1;
            }
            rest = &rest[count..];
        }
    }
    let Some(clock) = date.and_hms_opt(hour, minute, second) else {
        return Ok(None);
    };
    let ticks = (fraction * 10_000_000.0).round_ties_even() as i64;
    let clock = clock
        .checked_add_signed(chrono::TimeDelta::nanoseconds(ticks * 100))
        .filter(|value| value.year() <= 9999)
        .context("Fallback fraction exceeds DateTime range")?;
    rest = rest.trim_matches(|c: char| c.is_ascii_whitespace());
    let kind = if rest.is_empty() {
        Kind::Unspecified
    } else if rest == "Z" || rest == "z" {
        Kind::Utc
    } else if matches!(rest.as_bytes().first(), Some(b'+' | b'-')) {
        let sign = if rest.starts_with('-') { -1 } else { 1 };
        let body = &rest[1..];
        let (hours, minutes) = if let Some((hours, minutes)) = body.split_once(':') {
            if !(1..=2).contains(&hours.len()) || !(1..=2).contains(&minutes.len()) {
                return Ok(None);
            }
            (hours, minutes)
        } else if (1..=2).contains(&body.len()) {
            (body, "0")
        } else if (3..=4).contains(&body.len()) {
            body.split_at(body.len() - 2)
        } else {
            return Ok(None);
        };
        if !hours
            .bytes()
            .chain(minutes.bytes())
            .all(|b| b.is_ascii_digit())
        {
            return Ok(None);
        }
        let hours = number(hours).context("Invalid fallback offset")?;
        let minutes = number(minutes).context("Invalid fallback offset")?;
        ensure!(
            minutes < 60 && hours * 60 + minutes <= 14 * 60,
            "Fallback date offset exceeds fourteen hours"
        );
        Kind::FallbackLocal {
            offset_minutes: sign * (hours * 60 + minutes) as i32,
        }
    } else {
        return Ok(None);
    };
    Ok(Some(DateValue { clock, kind }))
}
