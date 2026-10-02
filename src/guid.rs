//! Character GUID parsing pinned to .NET 9, without guide identity rewriting.
//!
//! Reference: runtime/v9.0.0 System/Guid.cs TryParseGuid/TryParseExactD/X;
//! System/Number.Parsing.cs hexadecimal parsing and Common Number.Parsing.Common.cs
//! TrailingZeros. This implements TryParse success values, not exception details.

use std::iter::Peekable;
use uuid::Uuid;

/// .NET Char.IsWhiteSpace (Unicode 15.0); deliberately independent of Rust tables.
fn whitespace(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{0085}' | '\u{00a0}'
        | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}'
        | '\u{202f}' | '\u{205f}' | '\u{3000}')
}

/// Parse the reference character grammar. Nil is valid; required-ID policy is
/// a caller concern. Work is linear in supplied text with constant extra space.
pub(crate) fn try_parse(text: &str) -> Option<Uuid> {
    let text = text.trim_matches(whitespace);
    // Dispatch uses the original UTF-16 positions, before X removes whitespace.
    // Only the first 32 units are needed to establish the minimum and positions.
    let mut prefix = [0u16; 32];
    let mut units = text.encode_utf16();
    for unit in &mut prefix {
        *unit = units.next()?;
    }
    match prefix[0] {
        0x28 => wrapped(text, b'(', b')'),
        0x7b if prefix[9] == 0x2d => wrapped(text, b'{', b'}'),
        0x7b => x(text),
        _ if prefix[8] == 0x2d => d(text.as_bytes()),
        _ => n(text.as_bytes()),
    }
}

fn wrapped(text: &str, start: u8, end: u8) -> Option<Uuid> {
    let bytes = text.as_bytes();
    if bytes.len() != 38 || bytes[0] != start || bytes[37] != end {
        return None;
    }
    d(&bytes[1..37])
}

fn digit(c: char) -> Option<u32> {
    match c {
        '0'..='9' => Some(c as u32 - '0' as u32),
        'a'..='f' => Some(c as u32 - 'a' as u32 + 10),
        'A'..='F' => Some(c as u32 - 'A' as u32 + 10),
        _ => None,
    }
}

fn strict_hex(bytes: &[u8]) -> Option<u32> {
    bytes.iter().try_fold(0u32, |value, &byte| {
        Some((value << 4) | digit(char::from(byte))?)
    })
}

fn n(bytes: &[u8]) -> Option<Uuid> {
    if bytes.len() != 32 {
        return None;
    }
    let mut result = [0u8; 16];
    for (byte, pair) in result.iter_mut().zip(bytes.chunks_exact(2)) {
        *byte = strict_hex(pair)? as u8;
    }
    Some(Uuid::from_bytes(result))
}

fn fields(a: u32, b: u16, c: u16, tail: [u8; 8]) -> Uuid {
    // Guid constructor fields are displayed in big-endian order. Avoid native
    // Guid memory-layout endianness when constructing UUID's canonical bytes.
    Uuid::from_fields(a, b, c, &tail)
}

fn d(bytes: &[u8]) -> Option<Uuid> {
    if bytes.len() != 36 || [8, 13, 18, 23].iter().any(|&i| bytes[i] != b'-') {
        return None;
    }
    let ordinary = || {
        Some(fields(
            strict_hex(&bytes[..8])?,
            strict_hex(&bytes[9..13])? as u16,
            strict_hex(&bytes[14..18])? as u16,
            {
                let de = (strict_hex(&bytes[19..23])? as u16).to_be_bytes();
                let fg = (strict_hex(&bytes[24..28])? as u16).to_be_bytes();
                let hijk = strict_hex(&bytes[28..36])?.to_be_bytes();
                [
                    de[0], de[1], fg[0], fg[1], hijk[0], hijk[1], hijk[2], hijk[3],
                ]
            },
        ))
    };
    if let Some(value) = ordinary() {
        return Some(value);
    }
    if !bytes.iter().any(|b| matches!(b, b'x' | b'X' | b'+')) {
        return None;
    }
    let part = |slice: &[u8]| compat_hex(slice.iter().copied().map(char::from));
    let de = (part(&bytes[19..23])? as u16).to_be_bytes();
    let fg = (part(&bytes[24..28])? as u16).to_be_bytes();
    // The final numeric field uses Number's hex parser: at least one digit,
    // followed optionally by NULs. Prefixes and whitespace are not accepted.
    let tail = &bytes[28..36];
    let end = tail.iter().position(|&b| b == 0).unwrap_or(tail.len());
    if end == 0 || tail[end..].iter().any(|&b| b != 0) {
        return None;
    }
    let hijk = strict_hex(&tail[..end])?.to_be_bytes();
    Some(fields(
        part(&bytes[..8])?,
        part(&bytes[9..13])? as u16,
        part(&bytes[14..18])? as u16,
        [
            de[0], de[1], fg[0], fg[1], hijk[0], hijk[1], hijk[2], hijk[3],
        ],
    ))
}

/// Guid's numeric compatibility helper accepts an exhausted optional prefix as
/// zero, ignores leading zeroes, and bounds significant digits to eight.
fn compat_hex(chars: impl Iterator<Item = char>) -> Option<u32> {
    let mut chars = chars.peekable();
    if chars.peek() == Some(&'+') {
        chars.next();
    }
    if chars.peek() == Some(&'0') {
        chars.next();
        if matches!(chars.peek(), Some('x' | 'X')) {
            chars.next();
        }
        // Otherwise this consumed only an insignificant leading zero.
    }
    let mut value = 0u32;
    let mut significant = 0u8;
    for c in chars {
        let next = digit(c)?;
        if significant == 0 && next == 0 {
            continue;
        }
        if significant == 8 {
            return None;
        }
        significant += 1;
        value = (value << 4) | next;
    }
    Some(value)
}

fn x_number(chars: &mut Peekable<impl Iterator<Item = char>>, end: char) -> Option<u32> {
    if chars.next()? != '0' || !matches!(chars.next()?, 'x' | 'X') {
        return None;
    }
    let mut nonempty = false;
    let value = compat_hex(std::iter::from_fn(|| {
        if chars.peek() == Some(&end) {
            return None;
        }
        let next = chars.next()?;
        nonempty = true;
        Some(next)
    }))?;
    if !nonempty || chars.next()? != end {
        return None;
    }
    Some(value)
}

fn x(text: &str) -> Option<Uuid> {
    let mut chars = text.chars().filter(|&c| !whitespace(c)).peekable();
    if chars.next()? != '{' {
        return None;
    }
    let a = x_number(&mut chars, ',')?;
    let b = x_number(&mut chars, ',')? as u16;
    let c = x_number(&mut chars, ',')? as u16;
    if chars.next()? != '{' {
        return None;
    }
    let mut tail = [0u8; 8];
    for (index, byte) in tail.iter_mut().enumerate() {
        let value = x_number(&mut chars, if index == 7 { '}' } else { ',' })?;
        *byte = u8::try_from(value).ok()?;
    }
    if chars.next()? != '}' || chars.next().is_some() {
        return None;
    }
    Some(fields(a, b, c, tail))
}

/// Ephemeral comparison identity. Stored and emitted guide spellings stay untouched.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Identity {
    Known(Uuid),
    Opaque(String),
}

/// Required GUID policy remains with callers; opaque legacy identities keep ASCII folding.
pub(crate) fn key_ascii(text: &str) -> Identity {
    try_parse(text).map_or_else(
        || Identity::Opaque(text.to_ascii_lowercase()),
        Identity::Known,
    )
}

pub(crate) fn eq_ascii(left: &str, right: &str) -> bool {
    match (try_parse(left), try_parse(right)) {
        (Some(left), Some(right)) => left == right,
        _ => left.eq_ignore_ascii_case(right),
    }
}
