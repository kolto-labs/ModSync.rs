//! Bounded ActionType intake from the pinned structured-guide loader.
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

const NAMES: [&str; 12] = [
    "Unset",
    "Extract",
    "Execute",
    "Patcher",
    "Move",
    "Copy",
    "Rename",
    "Delete",
    "DelDuplicate",
    "Choose",
    "Run",
    "CleanList",
];
const MAX_TEXT: usize = 4096;
const MAX_ITEMS: usize = 1024;

pub(super) struct Intake {
    pub raw_choose: bool,
    pub canonical: String,
}
pub(super) fn unset() -> String {
    "Unset".into()
}
pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    intake(&Value::deserialize(d)?)
        .map(|value| value.canonical)
        .map_err(serde::de::Error::custom)
}
pub(super) fn intake(value: &Value) -> Result<Intake> {
    let text = getter_text(value)?;
    let raw_choose = text
        .as_deref()
        .is_some_and(|text| crate::unicode_case::ordinal_eq(text, "Choose"));
    let parsed = text.as_deref().and_then(parse).unwrap_or(0);
    let canonical = usize::try_from(parsed)
        .ok()
        .and_then(|index| NAMES.get(index))
        .map_or_else(|| parsed.to_string(), |name| (*name).into());
    Ok(Intake {
        raw_choose,
        canonical,
    })
}
fn numeric(value: &serde_json::Number) -> Result<Option<i32>> {
    if let Some(value) = value.as_i64() {
        return Ok(i32::try_from(value).ok());
    }
    if let Some(value) = value.as_u64() {
        return Ok(i32::try_from(value).ok());
    }
    if crate::json_native::wide_integer_lexeme(value).is_some() {
        // Beyond Int64/u64 and integer-shaped: no native-type evidence;
        // falls through to the Unset default in `intake`.
        return Ok(None);
    }
    let Some(value) = value.as_f64() else {
        // as_f64 only returns None here for a non-finite decimal/exponent
        // lexeme; refuse rather than silently defaulting to Unset.
        bail!("Non-finite numeric value is not a valid Action");
    };
    Ok((value.is_finite()
        && value.fract() == 0.0
        && value >= f64::from(i32::MIN)
        && value <= f64::from(i32::MAX))
    .then_some(value as i32))
}
fn getter_text(value: &Value) -> Result<Option<String>> {
    match value {
        Value::String(text) => {
            ensure!(text.len() <= MAX_TEXT, "Action text exceeds intake bound");
            Ok(Some(text.clone()))
        }
        Value::Number(value) => Ok(numeric(value)?.map(|value| value.to_string())),
        Value::Null | Value::Bool(_) | Value::Object(_) => Ok(None),
        Value::Array(items) => {
            ensure!(
                items.len() <= MAX_ITEMS,
                "Action collection exceeds intake bound"
            );
            let mut text = String::new();
            let mut count = 0;
            let mut invalid = false;
            for item in items {
                // GetValue consolidates precisely one collection layer.
                if let Value::Array(nested) = item {
                    for item in nested {
                        append(item, &mut text, &mut count, &mut invalid)?;
                    }
                } else {
                    append(item, &mut text, &mut count, &mut invalid)?;
                }
            }
            Ok((!invalid && !text.is_empty()).then_some(text))
        }
    }
}
fn append(value: &Value, text: &mut String, count: &mut usize, invalid: &mut bool) -> Result<()> {
    *count += 1;
    ensure!(
        *count <= MAX_ITEMS,
        "Action collection exceeds intake bound"
    );
    let number;
    let item = match value {
        Value::Null => return Ok(()),
        Value::String(value) => {
            ensure!(value.len() <= MAX_TEXT, "Action text exceeds intake bound");
            if value.trim().is_empty() {
                return Ok(());
            }
            value.as_str()
        }
        Value::Number(value) => {
            let Some(value) = numeric(value)? else {
                *invalid = true;
                return Ok(());
            };
            // Collection ToString uses CurrentCulture, whose negative sign may
            // differ from invariant Enum parsing. Do not fabricate its text.
            if value < 0 {
                bail!(
                    "Negative numeric Action collection items require unsupported culture-dependent conversion"
                );
            }
            number = value.to_string();
            &number
        }
        Value::Bool(_) | Value::Object(_) | Value::Array(_) => {
            *invalid = true;
            return Ok(());
        }
    };
    ensure!(
        text.len() + item.len() + usize::from(!text.is_empty()) * 2 <= MAX_TEXT,
        "Joined Action text exceeds intake bound"
    );
    if !text.is_empty() {
        text.push_str(", ");
    }
    text.push_str(item);
    Ok(())
}
fn parse(raw: &str) -> Option<i32> {
    if ["TSLPatcher", "HoloPatcher"]
        .iter()
        .any(|name| crate::unicode_case::ordinal_eq(raw, name))
    {
        return Some(3);
    }
    let value = raw.trim_start_matches(char::is_whitespace);
    if value.is_empty() {
        return None;
    }
    if value.as_bytes()[0].is_ascii_digit() || value.starts_with(['+', '-']) {
        // Numeric trailing whitespace precedes a terminal NUL run. Leading
        // Unicode whitespace was handled by Enum, not by integer parsing.
        let value = value
            .trim_end_matches('\0')
            .trim_end_matches(|c: char| matches!(c, '\t'..='\r' | ' '));
        let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        return value.parse::<i32>().ok();
    }
    let mut result = 0_i32;
    for token in value.split(',') {
        let token = token.trim();
        let index = NAMES
            .iter()
            .position(|name| crate::unicode_case::ordinal_eq(token, name))?;
        result |= index as i32;
    }
    Some(result)
}
