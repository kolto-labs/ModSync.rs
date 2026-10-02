//! Bounded TZif intake. Integer arithmetic is checked before every section read.
use anyhow::{Context, Result, ensure};

pub(super) struct Type {
    pub offset: i32,
    pub dst: bool,
}
pub(super) struct Data {
    pub transitions: Vec<(i64, usize)>,
    pub types: Vec<Type>,
    pub tail: String,
}
struct Header {
    version: u8,
    utc: usize,
    standard: usize,
    leaps: usize,
    transitions: usize,
    types: usize,
    names: usize,
}
fn word(bytes: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(
        bytes
            .get(at..at + 4)
            .context("Truncated TZif integer")?
            .try_into()?,
    ))
}
fn header(bytes: &[u8], at: usize) -> Result<Header> {
    let bytes = bytes.get(at..at + 44).context("Truncated TZif header")?;
    ensure!(&bytes[..4] == b"TZif", "Invalid timezone signature");
    ensure!(
        matches!(bytes[4], 0 | b'2' | b'3'),
        "Unsupported TZif version"
    );
    let h = Header {
        version: bytes[4],
        utc: word(bytes, 20)? as usize,
        standard: word(bytes, 24)? as usize,
        leaps: word(bytes, 28)? as usize,
        transitions: word(bytes, 32)? as usize,
        types: word(bytes, 36)? as usize,
        names: word(bytes, 40)? as usize,
    };
    ensure!(
        h.transitions <= 65536
            && (1..=256).contains(&h.types)
            && h.names <= 65536
            && h.leaps <= 65536,
        "Timezone count exceeds supported snapshot bounds"
    );
    ensure!(
        (h.utc == 0 || h.utc == h.types) && (h.standard == 0 || h.standard == h.types),
        "Invalid TZif indicator counts"
    );
    Ok(h)
}
fn size(h: &Header, width: usize) -> usize {
    // Header bounds make these products safely smaller than usize on supported targets.
    h.transitions * (width + 1) + h.types * 6 + h.names + h.leaps * (width + 4) + h.standard + h.utc
}
pub(super) fn parse(bytes: &[u8]) -> Result<Data> {
    ensure!(bytes.len() <= 1024 * 1024, "Timezone exceeds 1 MiB");
    let first = header(bytes, 0)?;
    let (h, start, width) = if first.version == 0 {
        (first, 44, 4)
    } else {
        let at = 44 + size(&first, 4);
        (header(bytes, at)?, at + 44, 8)
    };
    let end = start + size(&h, width);
    ensure!(end <= bytes.len(), "Truncated timezone data");
    let indices = start + h.transitions * width;
    let types_at = indices + h.transitions;
    let names_at = types_at + h.types * 6;
    let mut types = Vec::with_capacity(h.types);
    for i in 0..h.types {
        let at = types_at + i * 6;
        let offset = word(bytes, at)? as i32;
        ensure!(
            offset.abs_diff(0) <= 7 * 86400,
            "Unsupported extreme TZif offset"
        );
        ensure!(
            bytes[at + 4] <= 1 && (bytes[at + 5] as usize) < h.names,
            "Invalid timezone type"
        );
        let abbreviation = &bytes[names_at + bytes[at + 5] as usize..names_at + h.names];
        ensure!(
            abbreviation.contains(&0),
            "Unterminated timezone abbreviation"
        );
        types.push(Type {
            offset,
            dst: bytes[at + 4] != 0,
        });
    }
    let mut transitions = Vec::with_capacity(h.transitions);
    for i in 0..h.transitions {
        let at = start + i * width;
        let time = if width == 8 {
            i64::from_be_bytes(bytes[at..at + 8].try_into()?)
        } else {
            (word(bytes, at)? as i32) as i64
        };
        ensure!(
            transitions
                .last()
                .is_none_or(|(previous, _)| *previous < time),
            "Timezone transitions are not strictly ordered"
        );
        let kind = bytes[indices + i] as usize;
        ensure!(
            kind < types.len(),
            "Timezone transition type is out of range"
        );
        transitions.push((time, kind));
    }
    let tail = if width == 8 && bytes.len() > end {
        let raw = &bytes[end..];
        ensure!(
            raw.len() <= 4098 && raw.first() == Some(&b'\n') && raw.last() == Some(&b'\n'),
            "Unsupported timezone future rule framing"
        );
        let raw = &raw[1..raw.len() - 1];
        ensure!(
            raw.is_ascii() && !raw.contains(&b'\n') && !raw.contains(&0),
            "Invalid timezone future rule"
        );
        std::str::from_utf8(raw)?.to_owned()
    } else {
        String::new()
    };
    Ok(Data {
        transitions,
        types,
        tail,
    })
}
