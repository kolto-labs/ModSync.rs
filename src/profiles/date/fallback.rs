//! Bounded full-calendar-date subset of the invariant framework fallback.
//! Specialized Newtonsoft readers always run before this module.
use super::{DateValue, Kind};
use anyhow::{Context, Result, ensure};
use chrono::{Datelike, NaiveDate};

pub(in super::super) fn parse(text: &str) -> Result<Option<DateValue>> {
    ensure!(text.len() <= 4096, "Fallback date exceeds text bound");
    let text = envelope(text)?;
    let (prefix, text) = weekday_prefix(text)?;
    let (suffix, text) = weekday_suffix(text)?;
    ensure!(
        prefix.is_none() || suffix.is_none(),
        "Fallback weekday is repeated"
    );
    let weekday = prefix.or(suffix);
    let canonical =
        calendar_prefix(text).map(|(date, rest)| format!("{}{rest}", date.format("%Y-%m-%d")));
    let candidate = canonical.as_deref().unwrap_or(text);
    // Weekdays belong to the generic grammar. Do not pass one through
    // the separate ISO-T parser, whose weekday handling is a different route.
    if weekday.is_some() && matches!(candidate.as_bytes().get(10), Some(b'T' | b't')) {
        return Ok(None);
    }
    let parsed = iso_fallback(candidate)?;
    if let (Some(expected), Some(value)) = (weekday, parsed.as_ref()) {
        ensure!(
            value.clock.weekday().num_days_from_sunday() == expected,
            "Fallback weekday does not match rounded calendar date"
        );
    }
    Ok(parsed)
}
fn envelope(text: &str) -> Result<&str> {
    let text = if let Some(at) = text.find('\0') {
        ensure!(
            text[at..].bytes().all(|b| b == 0),
            "Fallback date has nonterminal NUL"
        );
        &text[..at]
    } else {
        text
    };
    let text = text.trim();
    if text.contains('#') {
        ensure!(
            text.len() >= 2
                && text.starts_with('#')
                && text.ends_with('#')
                && !text[1..text.len() - 1].contains('#'),
            "Fallback date hash delimiters must be paired"
        );
        Ok(text[1..text.len() - 1].trim())
    } else {
        Ok(text)
    }
}
fn weekday_name(text: &str) -> Option<u32> {
    const DAYS: [(&str, &str); 7] = [
        ("Sunday", "Sun"),
        ("Monday", "Mon"),
        ("Tuesday", "Tue"),
        ("Wednesday", "Wed"),
        ("Thursday", "Thu"),
        ("Friday", "Fri"),
        ("Saturday", "Sat"),
    ];
    DAYS.iter()
        .position(|(full, short)| {
            text.eq_ignore_ascii_case(full) || text.eq_ignore_ascii_case(short)
        })
        .map(|index| index as u32)
}
fn weekday_prefix(text: &str) -> Result<(Option<u32>, &str)> {
    let length = text.bytes().take_while(u8::is_ascii_alphabetic).count();
    let Some(day) = weekday_name(&text[..length]) else {
        return Ok((None, text));
    };
    let rest = &text[length..];
    let rest = rest.strip_prefix(',').unwrap_or(rest);
    // This slice intentionally admits only an explicit prefix token boundary.
    if !rest.starts_with(|c: char| c.is_ascii_whitespace()) {
        return Ok((None, text));
    }
    let rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let next = rest.bytes().take_while(u8::is_ascii_alphabetic).count();
    ensure!(
        weekday_name(&rest[..next]).is_none(),
        "Fallback weekday is repeated"
    );
    Ok((Some(day), rest))
}
fn weekday_suffix(text: &str) -> Result<(Option<u32>, &str)> {
    let text = text.trim_end_matches(|c: char| c.is_ascii_whitespace());
    let length = text
        .bytes()
        .rev()
        .take_while(u8::is_ascii_alphabetic)
        .count();
    let at = text.len() - length;
    let Some(day) = weekday_name(&text[at..]) else {
        return Ok((None, text));
    };
    let before = &text[..at];
    // Admit only a whitespace-separated terminal token, not general punctuation.
    if !before.ends_with(|c: char| c.is_ascii_whitespace()) {
        return Ok((None, text));
    }
    let before = before.trim_end_matches(|c: char| c.is_ascii_whitespace());
    let previous = before
        .bytes()
        .rev()
        .take_while(u8::is_ascii_alphabetic)
        .count();
    ensure!(
        weekday_name(&before[before.len() - previous..]).is_none(),
        "Fallback weekday is repeated"
    );
    Ok((Some(day), before))
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
    let separator = if first.contains('/') { '/' } else { '-' };
    let parts = first.split(separator).collect::<Vec<_>>();
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
    if !matches!(b[10], b'T' | b't' | b' ') {
        return Ok(None);
    }
    let mut rest = text[11..].trim_start_matches(|c: char| c.is_ascii_whitespace());
    let generic = b[10] == b' ';
    let Some((mut hour, after)) = clock_number(rest, generic) else {
        return Ok(None);
    };
    rest = after;
    let mut minute = 0;
    let has_minute = if let Some(after) = rest.strip_prefix(':') {
        let Some((value, after)) = clock_number(after, generic) else {
            return Ok(None);
        };
        minute = value;
        rest = after;
        true
    } else {
        false
    };
    if !generic && !has_minute {
        return Ok(None);
    }
    let mut second = 0;
    let mut fraction = 0.0_f64;
    if let Some(after) = rest.strip_prefix(':') {
        if !has_minute {
            return Ok(None);
        }
        let Some((value, after)) = clock_number(after, generic) else {
            return Ok(None);
        };
        second = value;
        rest = after;
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
    rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let mark = if generic { time_mark(rest) } else { None };
    if let Some((pm, after)) = mark {
        ensure!(
            hour <= if pm { 23 } else { 12 },
            "Fallback hour is invalid for AM/PM"
        );
        if pm && hour < 12 {
            hour += 12;
        } else if !pm && hour == 12 {
            hour = 0;
        }
        rest = after.trim_start_matches(|c: char| c.is_ascii_whitespace());
        ensure!(
            time_mark(rest).is_none(),
            "Fallback AM/PM marker is repeated"
        );
    } else if !has_minute {
        return Ok(None);
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
    } else if rest == "Z" || rest == "z" || (generic && rest.eq_ignore_ascii_case("GMT")) {
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

fn clock_number(text: &str, generic: bool) -> Option<(u32, &str)> {
    let count = text.bytes().take_while(u8::is_ascii_digit).count();
    if !(if generic { 1..=2 } else { 2..=2 }).contains(&count) {
        return None;
    }
    Some((text[..count].parse().ok()?, &text[count..]))
}
fn time_mark(text: &str) -> Option<(bool, &str)> {
    let name = text.get(..2)?;
    let pm = if name.eq_ignore_ascii_case("PM") {
        true
    } else if name.eq_ignore_ascii_case("AM") {
        false
    } else {
        return None;
    };
    let rest = &text[2..];
    if !rest.is_empty() && !rest.starts_with(|c: char| c.is_ascii_whitespace()) {
        return None;
    }
    Some((pm, rest))
}
