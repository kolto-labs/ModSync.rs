//! Bounded additional ISO forms accepted by Newtonsoft.Json 13.0.4's
//! DateTimeParser + DateTimeUtils.CreateDateTime. This validates acceptance;
//! timezone-independent values serialize at 100 ns precision. Local conversion
//! uses one bounded immutable host timezone snapshot, retaining authored text with
//! warnings when the platform or rule shape is unsupported.
use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};
#[path = "date/fallback.rs"]
pub(super) mod fallback;
#[path = "date/local.rs"]
pub(super) mod local;

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Utc,
    Unspecified,
    LocalRequired { offset_minutes: Option<i32> },
    FallbackLocal { offset_minutes: i32 },
}

pub(super) struct DateValue {
    clock: NaiveDateTime,
    kind: Kind,
}
impl DateValue {
    pub(super) fn normalize(
        self,
        authored: &str,
        key: &str,
        context: &mut local::ContextSnapshot,
    ) -> anyhow::Result<String> {
        let result = match self.kind {
            Kind::LocalRequired { offset_minutes } => context.normalize(self.clock, offset_minutes),
            Kind::FallbackLocal { offset_minutes } => {
                context.normalize_fallback(self.clock, offset_minutes)
            }
            _ => return Ok(self.canonical().unwrap_or_else(|| authored.to_owned())),
        };
        match result {
            Ok(value) => Ok(value),
            Err(error) if error.is::<local::InvalidFallbackDate>() => Err(error),
            Err(error) => {
                context.warn(key, &error);
                Ok(authored.to_owned())
            }
        }
    }
    pub(super) fn canonical(&self) -> Option<String> {
        if matches!(
            self.kind,
            Kind::LocalRequired { .. } | Kind::FallbackLocal { .. }
        ) {
            return None;
        }
        let mut text = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            self.clock.year(),
            self.clock.month(),
            self.clock.day(),
            self.clock.hour(),
            self.clock.minute(),
            self.clock.second()
        );
        let ticks = self.clock.nanosecond() / 100;
        if ticks != 0 {
            text.push('.');
            text.push_str(format!("{ticks:07}").trim_end_matches('0'));
        }
        if matches!(self.kind, Kind::Utc) {
            text.push('Z');
        }
        Some(text)
    }
}

/// DateTimeUtils.TryParseMicrosoftDate/TryReadOffset, not DateTimeOffset rules.
/// Offset-free values normalize to UTC. The numeric zone is ignored by
/// DateTime after selecting Local kind; the shared decode context supplies
/// supported host normalization without changing public string field types.
pub(super) fn microsoft(text: &str) -> Option<DateValue> {
    let body = text.strip_prefix("/Date(")?.strip_suffix(")/")?;
    // The reference searches '+' first, then '-', beginning after the first
    // epoch character so a negative epoch does not become an offset marker.
    let tail = body.get(1..)?;
    let offset = tail.find('+').or_else(|| tail.find('-')).map(|i| i + 1);
    let epoch = offset.map_or(body, |i| &body[..i]);
    let digits = epoch.strip_prefix('-').unwrap_or(epoch);
    if digits.is_empty() || digits.len() > 19 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if let Some(index) = offset {
        // Read the same fixed UTF-16 pairs as Newtonsoft. Its second pair is
        // optional only for +HH; trailing content after +HHmm is ignored.
        // Int32TryParse permits '-' followed by one digit in a two-char pair.
        let mut units = text[6 + index + 1..].encode_utf16();
        let pair = |first: u16, second: u16| {
            (first == u16::from(b'-') || (u16::from(b'0')..=u16::from(b'9')).contains(&first))
                && (u16::from(b'0')..=u16::from(b'9')).contains(&second)
        };
        if !pair(units.next()?, units.next()?) {
            return None;
        }
        let third = units.next()?;
        let fourth = units.next()?;
        if units.next().is_some() && !pair(third, fourth) {
            return None;
        }
    }
    let date = chrono::DateTime::from_timestamp_millis(epoch.parse().ok()?)?;
    (1..=9999).contains(&date.year()).then_some(DateValue {
        clock: date.naive_utc(),
        kind: if offset.is_some() {
            Kind::LocalRequired {
                offset_minutes: None,
            }
        } else {
            Kind::Utc
        },
    })
}

pub(super) fn iso(text: &str) -> Option<DateValue> {
    let bytes = text.as_bytes();
    if !(19..=40).contains(&bytes.len()) {
        return None;
    }
    if bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    let number = |start: usize, count: usize| -> Option<u32> {
        bytes
            .get(start..start + count)?
            .iter()
            .try_fold(0, |value, byte| {
                byte.is_ascii_digit()
                    .then(|| value * 10 + u32::from(byte - b'0'))
            })
    };
    let year = number(0, 4)? as i32;
    if year == 0 {
        return None;
    }
    let mut date = NaiveDate::from_ymd_opt(year, number(5, 2)?, number(8, 2)?)?;
    let hour = number(11, 2)?;
    let minute = number(14, 2)?;
    let second = number(17, 2)?;
    if hour > 24 || minute > 59 || second > 59 || (hour == 24 && (minute != 0 || second != 0)) {
        return None;
    }
    let mut position = 19;
    let mut fraction = 0u32;
    if bytes.get(position) == Some(&b'.') {
        position += 1;
        let start = position;
        while bytes.get(position).is_some_and(u8::is_ascii_digit) {
            if position - start >= 7 || (hour == 24 && bytes[position] != b'0') {
                return None;
            }
            fraction = fraction * 10 + u32::from(bytes[position] - b'0');
            position += 1;
        }
        if !(1..=7).contains(&(position - start)) {
            return None;
        }
        fraction *= 10u32.pow(7 - (position - start) as u32);
    }
    let kind = match bytes.get(position..)? {
        [] => Kind::Unspecified,
        [b'Z'] | [b'z'] => Kind::Utc,
        zone => {
            if !matches!(zone.first(), Some(b'+' | b'-')) {
                return None;
            }
            // The reference DateTime parser permits two-digit offset hours and
            // minutes through 99, unlike DateTimeOffset's ±14-hour constraint.
            let hours = number(position + 1, 2)?;
            let minutes = match zone.len() {
                3 => 0,
                5 => number(position + 3, 2)?,
                6 if zone[3] == b':' => number(position + 4, 2)?,
                _ => return None,
            };
            Kind::LocalRequired {
                offset_minutes: Some(
                    (hours * 60 + minutes) as i32 * if zone[0] == b'-' { -1 } else { 1 },
                ),
            }
        }
    };
    if hour == 24 {
        date = date.succ_opt()?;
    }
    (date.year() <= 9999).then_some(DateValue {
        clock: date.and_hms_nano_opt(hour % 24, minute, second, fraction * 100)?,
        kind,
    })
}

/// Deterministic invariant fallback subset: a fully specified calendar date,
/// optionally with a zone-free clock. No missing-date/current-date inference.
pub(super) fn invariant(text: &str) -> Option<DateValue> {
    let text = text.trim();
    if text.len() == 10 {
        return iso(&format!("{text}T00:00:00"));
    }
    let bytes = text.as_bytes();
    if !(19..=40).contains(&bytes.len()) || !matches!(bytes.get(10), Some(b'T' | b' ')) {
        return None;
    }
    // The framework fallback does not admit ISO 24:00 rollover.
    if bytes.get(11..13)? == b"24" {
        return None;
    }
    let mut candidate = text.to_owned();
    candidate.replace_range(10..11, "T");
    let value = iso(&candidate)?;
    matches!(value.kind, Kind::Unspecified).then_some(value)
}
