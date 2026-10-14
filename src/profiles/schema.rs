//! Persisted profile schema and explicit legacy Rust identity compatibility.
#[path = "date.rs"]
mod date_compat;
use super::{Profile, Selection, unset_date, without_bom};
use anyhow::{Context, Result, ensure};
use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub const MAX_PROFILE_BYTES: usize = 16 * 1024 * 1024;
const IDENTITY_MARKER: &str = "_modsyncRustIdentityFormat";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IdentityFormat {
    ReferenceGuids,
    LegacyOpaqueIds,
}
#[derive(Clone, Debug)]
pub struct DecodedProfile {
    pub profile: Profile,
    pub identity_format: IdentityFormat,
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug, Default)]
pub struct ProfileListing {
    pub profiles: Vec<Profile>,
    pub warnings: Vec<String>,
}

/// Canonicalize the D, N, B, P and X Guid forms accepted by the reference.
pub(super) fn guid(text: &str) -> Option<String> {
    crate::guid::try_parse(text).map(|id| id.to_string())
}
fn field<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    object
        .iter()
        .rev()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, v)| v)
}
fn text(value: Option<&Value>, field_name: &str) -> Result<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        // Newtonsoft permits scalar-to-string conversion for string properties.
        Some(Value::Bool(value)) => Ok(Some(if *value { "True" } else { "False" }.into())),
        Some(Value::Number(value)) => Ok(Some(value.to_string())),
        _ => anyhow::bail!("Profile {field_name} must be text or null"),
    }
}
fn boolean(value: Option<&Value>) -> Result<bool> {
    match value {
        None => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(Value::String(value)) if value.trim().eq_ignore_ascii_case("true") => Ok(true),
        Some(Value::String(value)) if value.trim().eq_ignore_ascii_case("false") => Ok(false),
        Some(Value::Number(value)) if value.is_i64() || value.is_u64() => Ok(value
            .as_i64()
            .map_or_else(|| value.as_u64() != Some(0), |v| v != 0)),
        _ => anyhow::bail!("Profile isSelected must be a Boolean"),
    }
}
fn date(
    value: Option<&Value>,
    key: &str,
    context: &mut date_compat::local::ContextSnapshot,
) -> Result<String> {
    let Some(value) = value else {
        return Ok(unset_date());
    };
    let value = value
        .as_str()
        .with_context(|| format!("Profile {key} must be a DateTime string"))?;
    ensure!(
        value.len() <= 4096,
        "Profile {key} exceeds the date text bound"
    );
    // Fast parser dispatch uses the original text. Trimming Microsoft syntax
    // here would admit input the framework invariant fallback does not parse.
    if let Some(parsed) = date_compat::iso(value).or_else(|| date_compat::microsoft(value)) {
        return parsed.normalize(value, key, context);
    }
    if let Some(parsed) = date_compat::fallback::parse(value)? {
        return parsed.normalize(value, key, context);
    }
    if let Some(parsed) = date_compat::invariant(value) {
        return parsed
            .canonical()
            .context("Invariant date subset unexpectedly requires local normalization");
    }
    // Retain existing bounded fallback acceptance without claiming its broad
    // grammar/rounding or host-local serialization is fully reproduced.
    let candidate = value.trim();
    let year = chrono::DateTime::parse_from_rfc3339(candidate)
        .ok()
        .filter(|d| d.nanosecond() < 1_000_000_000)
        .map(|d| d.year())
        .or_else(|| {
            NaiveDateTime::parse_from_str(candidate, "%Y-%m-%dT%H:%M:%S%.f")
                .ok()
                .filter(|d| d.nanosecond() < 1_000_000_000)
                .map(|d| d.year())
        })
        .or_else(|| {
            NaiveDateTime::parse_from_str(candidate, "%Y-%m-%d %H:%M:%S%.f")
                .ok()
                .filter(|d| d.nanosecond() < 1_000_000_000)
                .map(|d| d.year())
        })
        .or_else(|| {
            NaiveDate::parse_from_str(candidate, "%Y-%m-%d")
                .ok()
                .map(|d| d.year())
        });
    ensure!(
        year.is_some_and(|year| (1..=9999).contains(&year)),
        "Profile {key} is not a supported reference DateTime"
    );
    Ok(value.into())
}

/// Result of the reference guide metadata intake
/// `DateTime.TryParse(text, InvariantCulture, DateTimeStyles.None)`.
pub(crate) enum InvariantDate {
    /// Parsed; `round_trip` is the reference `{0:O}` rendering (seven
    /// fraction digits, host offset only for Local kind).
    Parsed { round_trip: String },
    /// The reference parse fails, so the reference never stores a value.
    Rejected,
    /// Outside the bounded model (or the local-zone provider): keep the
    /// authored value and report why.
    Unsupported(String),
}

/// Shared invariant `DateTime.TryParse` route with `DateTimeStyles.None`,
/// reusing the bounded profile fallback grammar and local-zone provider.
pub(crate) fn invariant_try_parse(text: &str) -> InvariantDate {
    if text.len() > 4096 {
        return InvariantDate::Unsupported("date text exceeds the 4096-byte bound".into());
    }
    // Every reference date/time grammar needs a numeric token; null, empty,
    // whitespace and word-only input fail TryParse.
    if !text.bytes().any(|b| b.is_ascii_digit()) {
        return InvariantDate::Rejected;
    }
    // A lone (signed) integer token is neither a date nor a clock.
    let trimmed = text.trim();
    let digits = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
    if digits.bytes().all(|b| b.is_ascii_digit()) {
        return InvariantDate::Rejected;
    }
    let value = match date_compat::fallback::parse(text) {
        // Typed fallback errors are reference parse failures.
        Err(_) => return InvariantDate::Rejected,
        Ok(None) => {
            return InvariantDate::Unsupported(
                "date form is outside the bounded invariant DateTime grammar".into(),
            );
        }
        Ok(Some(value)) => value.without_roundtrip_kind(),
    };
    let mut context = date_compat::local::ContextSnapshot::new();
    match value.normalize(text, "lastModified", &mut context) {
        Err(error) if error.is::<date_compat::local::InvalidFallbackDate>() => {
            InvariantDate::Rejected
        }
        Err(error) => InvariantDate::Unsupported(format!("{error:#}")),
        Ok(_) if !context.warnings.is_empty() => {
            InvariantDate::Unsupported(context.warnings.join("; "))
        }
        Ok(canonical) => InvariantDate::Parsed {
            round_trip: round_trip_pattern(&canonical),
        },
    }
}

/// Widen the canonical 100 ns serializer output to the `O` pattern, which
/// always writes seven fraction digits.
fn round_trip_pattern(canonical: &str) -> String {
    let (clock, rest) = canonical.split_at(19.min(canonical.len()));
    let (digits, zone) = match rest.strip_prefix('.') {
        Some(rest) => rest.split_at(rest.bytes().take_while(u8::is_ascii_digit).count()),
        None => ("", rest),
    };
    format!("{clock}.{digits:0<7}{zone}")
}
fn identity(value: &str, opaque: &mut bool) -> Result<String> {
    if let Some(guid) = guid(value) {
        return Ok(guid);
    }
    ensure!(
        !value.trim().is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control),
        "Invalid opaque Rust profile identity"
    );
    *opaque = true;
    Ok(value.into())
}

impl Profile {
    /// The compatibility reader preserves historic Rust text identities and reports
    /// their non-interoperability. It never silently discards a selection entry.
    pub fn decode(bytes: &[u8]) -> Result<DecodedProfile> {
        ensure!(
            bytes.len() <= MAX_PROFILE_BYTES,
            "Profile exceeds 16 MiB limit"
        );
        let value: Value = serde_json::from_slice(without_bom(bytes))?;
        let object = value.as_object().context("Profile must be a JSON object")?;
        let marked = match field(object, IDENTITY_MARKER) {
            None => false,
            Some(Value::String(value)) if value == "opaque-v1" => true,
            _ => anyhow::bail!("Unknown Rust profile identity format"),
        };
        let mut opaque = false;
        let mut selections = BTreeMap::new();
        if let Some(value) = field(object, "componentSelections") {
            let values = value
                .as_object()
                .context("Profile componentSelections must be an object")?;
            ensure!(
                values.len() <= 100_000,
                "Too many profile component selections"
            );
            for (key, value) in values {
                let key = identity(key, &mut opaque)?;
                let selection = value
                    .as_object()
                    .context("Profile component selection must be an object")?;
                let mut options = Vec::new();
                if let Some(value) = field(selection, "selectedOptionGuids") {
                    let values = value
                        .as_array()
                        .context("Profile selectedOptionGuids must be an array")?;
                    ensure!(
                        values.len() <= 100_000,
                        "Too many profile option selections"
                    );
                    for value in values {
                        options.push(identity(
                            value
                                .as_str()
                                .context("Profile option identity must be text")?,
                            &mut opaque,
                        )?);
                    }
                }
                // Guid dictionary spelling aliases overwrite in authored order,
                // matching Newtonsoft's dictionary reader.
                selections.insert(
                    key,
                    Selection {
                        is_selected: boolean(field(selection, "isSelected"))?,
                        selected_option_guids: options,
                    },
                );
            }
        }
        let mut date_context = date_compat::local::ContextSnapshot::new();
        let profile = Profile {
            name: text(field(object, "name"), "name")?.unwrap_or_default(),
            kotor_directory: text(field(object, "kotorDirectory"), "kotorDirectory")?,
            mod_directory: text(field(object, "modDirectory"), "modDirectory")?,
            instruction_file_path: text(
                field(object, "instructionFilePath"),
                "instructionFilePath",
            )?,
            component_selections: selections,
            created_utc: date(field(object, "createdUtc"), "createdUtc", &mut date_context)?,
            last_used_utc: date(
                field(object, "lastUsedUtc"),
                "lastUsedUtc",
                &mut date_context,
            )?,
        };
        let identity_format = if opaque {
            IdentityFormat::LegacyOpaqueIds
        } else {
            IdentityFormat::ReferenceGuids
        };
        let mut warnings = if opaque {
            vec![format!(
                "Profile '{}' contains {} Rust text selection identities; they are preserved, but C# Guid-based profiles cannot load them.{}",
                profile.name,
                if marked { "marked" } else { "unmarked legacy" },
                if marked {
                    ""
                } else {
                    " Saving adds an explicit Rust identity-format marker."
                }
            )]
        } else {
            Vec::new()
        };
        warnings.extend(date_context.warnings);
        Ok(DecodedProfile {
            profile,
            identity_format,
            warnings,
        })
    }
    pub fn decode_reference(bytes: &[u8]) -> Result<Self> {
        let decoded = Self::decode(bytes)?;
        ensure!(
            decoded.identity_format == IdentityFormat::ReferenceGuids,
            "Profile contains non-GUID Rust identities and is not C# profile compatible"
        );
        Ok(decoded.profile)
    }
    pub(super) fn persisted_value(&self) -> Result<Value> {
        let mut value = serde_json::to_value(self)?;
        let decoded = Self::decode(&serde_json::to_vec(&value)?)?;
        // Serialize normalized GUIDs and supported pure dates. Never replace
        // opaque identities with new UUIDs or approximate local date conversion.
        value = serde_json::to_value(&decoded.profile)?;
        if decoded.identity_format == IdentityFormat::LegacyOpaqueIds {
            value[IDENTITY_MARKER] = "opaque-v1".into();
        }
        Ok(value)
    }
}
