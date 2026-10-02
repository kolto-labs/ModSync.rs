//! Optional Boolean getter for represented structured-guide values.
use anyhow::{Result, ensure};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

const MAX_TEXT: usize = 4096;

// .NET 9 Char.IsWhiteSpace (Unicode White_Space), plus Boolean.Parse's NUL.
// Do not silently extend this grammar through a platform locale or Rust trim.
fn boundary(character: char) -> bool {
    matches!(character, '\0' | '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{0085}'
        | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}'
        | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}')
}

pub(super) fn optional(value: &Value) -> Result<bool> {
    Ok(match value {
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => {
            ensure!(value.len() <= MAX_TEXT, "Boolean text exceeds intake bound");
            // False and failed conversion both return the optional getter default.
            crate::unicode_case::ordinal_eq(value.trim_matches(boundary), "true")
        }
        Value::Null | Value::Array(_) | Value::Object(_) => false,
    })
}

pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    optional(&Value::deserialize(d)?).map_err(serde::de::Error::custom)
}
