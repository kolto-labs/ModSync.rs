//! Bounded additional ISO forms accepted by Newtonsoft.Json 13.0.4's
//! DateTimeParser + DateTimeUtils.CreateDateTime. This validates acceptance;
//! persisted profile strings keep their existing authored representation.
use chrono::{Datelike, NaiveDate};

pub(super) fn iso_year(text: &str) -> Option<i32> {
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
    if bytes.get(position) == Some(&b'.') {
        position += 1;
        let start = position;
        while bytes.get(position).is_some_and(u8::is_ascii_digit) {
            if hour == 24 && bytes[position] != b'0' {
                return None;
            }
            position += 1;
        }
        if !(1..=7).contains(&(position - start)) {
            return None;
        }
    }
    match bytes.get(position..)? {
        [] | [b'Z'] | [b'z'] => {}
        zone => {
            if !matches!(zone.first(), Some(b'+' | b'-')) {
                return None;
            }
            // The reference DateTime parser permits two-digit offset hours and
            // minutes through 99, unlike DateTimeOffset's ±14-hour constraint.
            number(position + 1, 2)?;
            match zone.len() {
                3 => {}
                5 => {
                    number(position + 3, 2)?;
                }
                6 if zone[3] == b':' => {
                    number(position + 4, 2)?;
                }
                _ => return None,
            }
        }
    }
    if hour == 24 {
        date = date.succ_opt()?;
    }
    (date.year() <= 9999).then_some(date.year())
}
