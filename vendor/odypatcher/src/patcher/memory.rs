//! Port of `pykotor/tslpatcher/memory.py`.

use std::collections::BTreeMap;
use std::fmt;

use super::error::{PResult, PatchException};

/// What a `2DAMEMORY` token holds: text, or a GFF field path captured with
/// `!FieldPath` (Python stores a `PureWindowsPath` for the latter).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryValue {
    /// Ordinary text (a row index, label, cell, list index, ...).
    Str(String),
    /// A backslash-joined GFF field path.
    FieldPath(String),
}

impl MemoryValue {
    /// `str(value)`.
    pub fn as_str(&self) -> &str {
        match self {
            MemoryValue::Str(text) | MemoryValue::FieldPath(text) => text,
        }
    }

    /// True for a `!FieldPath` capture.
    pub fn is_field_path(&self) -> bool {
        matches!(self, MemoryValue::FieldPath(_))
    }
}

impl fmt::Display for MemoryValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `PatcherMemory`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PatcherMemory {
    /// `2DAMEMORY#` token → value.
    pub memory_2da: BTreeMap<i32, MemoryValue>,
    /// `StrRef#` token → dialog.tlk index.
    pub memory_str: BTreeMap<i32, i32>,
}

impl PatcherMemory {
    /// Empty memory.
    pub fn new() -> Self {
        Self::default()
    }
}

/// `TokenUsage` and its three concrete classes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenUsage {
    /// `NoTokenUsage(stored)`: a literal, stored as text.
    NoToken(String),
    /// `TokenUsage2DA(token_id)`.
    TwoDA(i32),
    /// `TokenUsageTLK(token_id)`.
    Tlk(i32),
}

impl TokenUsage {
    /// `NoTokenUsage(stored)` from anything printable (`str(stored)`).
    pub fn literal(stored: impl ToString) -> Self {
        TokenUsage::NoToken(stored.to_string())
    }

    /// `value(memory)`; undefined tokens raise `KeyError`.
    pub fn value(&self, memory: &PatcherMemory) -> PResult<MemoryValue> {
        match self {
            TokenUsage::NoToken(stored) => Ok(MemoryValue::Str(stored.clone())),
            TokenUsage::TwoDA(token_id) => {
                memory.memory_2da.get(token_id).cloned().ok_or_else(|| {
                    PatchException::key(format!("2DAMEMORY{token_id} was not defined before use"))
                })
            }
            TokenUsage::Tlk(token_id) => memory
                .memory_str
                .get(token_id)
                .map(|value| MemoryValue::Str(value.to_string()))
                .ok_or_else(|| {
                    PatchException::key(format!("StrRef{token_id} was not defined before use"))
                }),
        }
    }
}
