//! Bounded cleanup text and matching, pinned to .NET 9 ReadAllText and casing.
//! No guide replacement-character-to-underscore policy applies here.
use anyhow::{Result, ensure};
use std::{
    collections::{BTreeSet, VecDeque},
    io::Read,
    sync::atomic::{AtomicBool, Ordering},
};

const BYTE_LIMIT: usize = 8 * 1024 * 1024;
const LINE_LIMIT: usize = 256 * 1024;
const ROW_LIMIT: usize = 100_000;
const WORK_LIMIT: usize = 2_000_000;

pub(super) struct Budget<'a> {
    cancelled: &'a AtomicBool,
    work: usize,
}
impl<'a> Budget<'a> {
    pub(super) fn new(cancelled: &'a AtomicBool) -> Self {
        Self { cancelled, work: 0 }
    }
    pub(super) fn check(&self) -> Result<()> {
        ensure!(!self.cancelled.load(Ordering::Acquire), "Cleanup cancelled");
        Ok(())
    }
    pub(super) fn charge(&mut self, work: usize) -> Result<()> {
        self.check()?;
        self.work = self
            .work
            .checked_add(work)
            .ok_or_else(|| anyhow::anyhow!("Cleanup work overflow"))?;
        ensure!(
            self.work <= WORK_LIMIT,
            "Cleanup exceeds 2 million work units"
        );
        Ok(())
    }
    pub(super) fn text(&mut self, text: &str) -> Result<()> {
        ensure!(text.len() <= LINE_LIMIT, "Cleanup field exceeds 256 KiB");
        for c in text.chars() {
            self.charge(c.len_utf8())?;
        }
        Ok(())
    }
}

// .NET 9 Char.IsWhiteSpace; do not depend on the host's Unicode tables.
fn whitespace(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{0085}' | '\u{00a0}'
        | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}'
        | '\u{202f}' | '\u{205f}' | '\u{3000}')
}
pub(super) fn trim(text: &str) -> &str {
    text.trim_matches(whitespace)
}
pub(super) fn blank(text: &str) -> bool {
    text.chars().all(whitespace)
}

/// A listed CleanList name that no Windows file name can spell: C# `FileExists`
/// on `Path.Combine(target, name)` is false and the field is skipped as missing.
/// A drive prefix (`C:`) is left to the path parser, which refuses its `:` as an unsafe path.
pub(super) fn unnameable(name: &str) -> bool {
    let drive = matches!(name.as_bytes(), [letter, b':', ..] if letter.is_ascii_alphabetic());
    name.contains(['"', '<', '>', '|', '*', '?', '\0'])
        || name.match_indices(':').any(|(at, _)| !(drive && at == 1))
}

/// Path.GetExtension/GetFileNameWithoutExtension on a qualified basename.
pub(super) fn name_parts(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(at) => (
            &name[..at],
            if at + 1 == name.len() {
                ""
            } else {
                &name[at..]
            },
        ),
        None => (name, ""),
    }
}

struct Bytes<R> {
    input: R,
    buffer: [u8; 8192],
    at: usize,
    end: usize,
    raw: usize,
    pending: VecDeque<u8>,
}
impl<R: Read> Bytes<R> {
    fn next(&mut self, budget: &mut Budget<'_>) -> Result<Option<u8>> {
        if let Some(byte) = self.pending.pop_front() {
            return Ok(Some(byte));
        }
        if self.at == self.end {
            budget.check()?;
            let remaining = (BYTE_LIMIT - self.raw + 1).min(self.buffer.len());
            self.end = self.input.read(&mut self.buffer[..remaining])?;
            self.at = 0;
            self.raw += self.end;
            ensure!(self.raw <= BYTE_LIMIT, "CleanList input exceeds 8 MiB");
            budget.check()?;
            if self.end == 0 {
                return Ok(None);
            }
        }
        let byte = self.buffer[self.at];
        self.at += 1;
        Ok(Some(byte))
    }
}
#[derive(Clone, Copy)]
enum Encoding {
    Utf8,
    Utf16(bool),
    Utf32(bool),
}
enum Unit {
    Value(u16),
    Odd,
    End,
}
struct Decoder<R> {
    bytes: Bytes<R>,
    encoding: Encoding,
    high: Option<u16>,
    unit: Option<Unit>,
}
impl<R: Read> Decoder<R> {
    fn new(input: R, budget: &mut Budget<'_>) -> Result<Self> {
        let mut bytes = Bytes {
            input,
            buffer: [0; 8192],
            at: 0,
            end: 0,
            raw: 0,
            pending: VecDeque::new(),
        };
        let mut prefix = Vec::with_capacity(4);
        for _ in 0..4 {
            match bytes.next(budget)? {
                Some(byte) => prefix.push(byte),
                None => break,
            }
        }
        // Read four initial bytes before distinguishing UTF-16LE from UTF-32LE.
        let (encoding, skip) = if prefix.starts_with(&[0xff, 0xfe, 0, 0]) {
            (Encoding::Utf32(false), 4)
        } else if prefix.starts_with(&[0, 0, 0xfe, 0xff]) {
            (Encoding::Utf32(true), 4)
        } else if prefix.starts_with(&[0xff, 0xfe]) {
            (Encoding::Utf16(false), 2)
        } else if prefix.starts_with(&[0xfe, 0xff]) {
            (Encoding::Utf16(true), 2)
        } else if prefix.starts_with(&[0xef, 0xbb, 0xbf]) {
            (Encoding::Utf8, 3)
        } else {
            (Encoding::Utf8, 0)
        };
        bytes.pending.extend(prefix.into_iter().skip(skip));
        Ok(Self {
            bytes,
            encoding,
            high: None,
            unit: None,
        })
    }
    fn read_unit(&mut self, big: bool, budget: &mut Budget<'_>) -> Result<Unit> {
        let Some(a) = self.bytes.next(budget)? else {
            return Ok(Unit::End);
        };
        let Some(b) = self.bytes.next(budget)? else {
            return Ok(Unit::Odd);
        };
        Ok(Unit::Value(if big {
            u16::from_be_bytes([a, b])
        } else {
            u16::from_le_bytes([a, b])
        }))
    }
    fn next(&mut self, budget: &mut Budget<'_>) -> Result<Option<char>> {
        budget.charge(1)?;
        match self.encoding {
            Encoding::Utf8 => {
                let Some(first) = self.bytes.next(budget)? else {
                    return Ok(None);
                };
                if first < 0x80 {
                    return Ok(Some(first as char));
                }
                let length = match first {
                    0xc2..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf4 => 4,
                    _ => return Ok(Some('\u{fffd}')),
                };
                let mut word = [first, 0, 0, 0];
                for (at, slot) in word.iter_mut().enumerate().take(length).skip(1) {
                    let Some(byte) = self.bytes.next(budget)? else {
                        return Ok(Some('\u{fffd}'));
                    };
                    let valid = (0x80..=0xbf).contains(&byte)
                        && (at != 1
                            || match first {
                                0xe0 => byte >= 0xa0,
                                0xed => byte <= 0x9f,
                                0xf0 => byte >= 0x90,
                                0xf4 => byte <= 0x8f,
                                _ => true,
                            });
                    if !valid {
                        // Rune.DecodeFromUtf8 consumes only the valid prefix of an
                        // ill-formed subsequence, then retries the offending byte.
                        self.bytes.pending.push_front(byte);
                        return Ok(Some('\u{fffd}'));
                    }
                    *slot = byte;
                }
                Ok(std::str::from_utf8(&word[..length])?.chars().next())
            }
            Encoding::Utf16(big) => loop {
                let unit = match self.unit.take() {
                    Some(unit) => unit,
                    None => self.read_unit(big, budget)?,
                };
                if let Some(high) = self.high.take() {
                    if let Unit::Value(low @ 0xdc00..=0xdfff) = unit {
                        let scalar =
                            0x10000 + (((high as u32 - 0xd800) << 10) | (low as u32 - 0xdc00));
                        return Ok(char::from_u32(scalar));
                    }
                    self.unit = Some(unit);
                    return Ok(Some('\u{fffd}'));
                }
                match unit {
                    Unit::Value(high @ 0xd800..=0xdbff) => self.high = Some(high),
                    Unit::Value(0xdc00..=0xdfff) | Unit::Odd => return Ok(Some('\u{fffd}')),
                    Unit::Value(unit) => return Ok(char::from_u32(unit as u32)),
                    Unit::End => return Ok(None),
                }
            },
            Encoding::Utf32(big) => {
                let mut word = [0; 4];
                for (at, byte) in word.iter_mut().enumerate() {
                    match self.bytes.next(budget)? {
                        Some(value) => *byte = value,
                        None => return Ok((at != 0).then_some('\u{fffd}')),
                    }
                }
                let scalar = if big {
                    u32::from_be_bytes(word)
                } else {
                    u32::from_le_bytes(word)
                };
                Ok(Some(char::from_u32(scalar).unwrap_or('\u{fffd}')))
            }
        }
    }
}

pub(super) fn read_rows(input: impl Read, budget: &mut Budget<'_>) -> Result<Vec<String>> {
    let mut decoder = Decoder::new(input, budget)?;
    let mut rows = Vec::new();
    let mut row = String::new();
    let mut decoded = 0usize;
    let mut row_count = 0usize;
    while let Some(c) = decoder.next(budget)? {
        decoded += c.len_utf8();
        ensure!(decoded <= BYTE_LIMIT, "Decoded CleanList exceeds 8 MiB");
        if matches!(c, '\r' | '\n') {
            row_count += 1;
            ensure!(row_count <= ROW_LIMIT, "CleanList exceeds 100,000 rows");
            if !row.is_empty() {
                rows.push(std::mem::take(&mut row));
            }
        } else {
            ensure!(
                row.len() + c.len_utf8() <= LINE_LIMIT,
                "CleanList line exceeds 256 KiB"
            );
            row.push(c);
        }
    }
    if !row.is_empty() {
        ensure!(row_count < ROW_LIMIT, "CleanList exceeds 100,000 rows");
        rows.push(row);
    }
    Ok(rows)
}

fn tokens(name: &str, budget: &mut Budget<'_>) -> Result<BTreeSet<String>> {
    const STOP: &[&str] = &[
        "by",
        "for",
        "the",
        "and",
        "of",
        "mod",
        "pack",
        "k1",
        "k2",
        "hd",
        "kotor",
        "kotorn",
        "version",
        "recolored",
        "reskin",
        "retexture",
        "fix",
        "fixes",
    ];
    budget.text(name)?;
    let lower = crate::unicode_case::invariant_lower_key(name);
    let mut result = BTreeSet::new();
    for word in lower.split(|c: char| !c.is_ascii_lowercase() && !c.is_ascii_digit()) {
        budget.charge(word.len() + 1)?;
        if !word.is_empty() && !STOP.contains(&word) {
            result.insert(
                if word.len() > 3 {
                    word.strip_suffix('s').unwrap_or(word)
                } else {
                    word
                }
                .to_owned(),
            );
        }
    }
    Ok(result)
}
struct Selected {
    key: String,
    tokens: BTreeSet<String>,
}
pub(super) struct Matcher {
    selected: Vec<Selected>,
}
impl Matcher {
    pub(super) fn new(selected: &BTreeSet<String>, budget: &mut Budget<'_>) -> Result<Self> {
        let mut prepared = Vec::new();
        for name in selected {
            budget.text(name)?;
            let key = crate::unicode_case::ordinal_key(name);
            let tokens = tokens(name, budget)?;
            prepared.push(Selected { key, tokens });
        }
        Ok(Self { selected: prepared })
    }
    pub(super) fn matches(&self, name: &str, budget: &mut Budget<'_>) -> Result<bool> {
        budget.text(name)?;
        let key = crate::unicode_case::ordinal_key(name);
        if key.starts_with("MANDATORY DELETIONS") {
            return Ok(true);
        }
        let target = tokens(name, budget)?;
        if target.is_empty() {
            return Ok(false);
        }
        for candidate in &self.selected {
            budget.charge(key.len() + candidate.key.len() + 1)?;
            if key.contains(&candidate.key) || candidate.key.contains(&key) {
                return Ok(true);
            }
        }
        for candidate in &self.selected {
            budget.charge(target.len() + candidate.tokens.len() + 1)?;
            let shared = target.intersection(&candidate.tokens).count();
            if shared >= 2 && shared * 5 >= target.len().min(candidate.tokens.len()) * 3 {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
