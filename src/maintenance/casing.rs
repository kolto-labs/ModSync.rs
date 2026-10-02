//! .NET ICU invariant simple lowercase with a declared Unicode dataset.
//! Source, generator, semantic digest encoding and license are retained in
//! casing/. Absence of persisted metadata always means legacy ASCII admission.
use super::CasingProfile;
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

#[path = "casing/data.rs"]
mod data;

pub(super) fn current_profile() -> CasingProfile {
    CasingProfile {
        algorithm: data::ALGORITHM.into(),
        unicode_version: data::UNICODE_VERSION.into(),
        source_sha256: data::SOURCE_SHA256.into(),
        table_sha256: data::TABLE_SHA256.into(),
    }
}

fn assigned(value: u32) -> bool {
    let position = data::ASSIGNED.partition_point(|(first, _)| *first <= value);
    position > 0 && value <= data::ASSIGNED[position - 1].1
}

fn verify_tables() -> Result<()> {
    static ADMISSION: OnceLock<std::result::Result<(), String>> = OnceLock::new();
    let admission = ADMISSION.get_or_init(|| {
        let check = || -> Result<()> {
            let mut hash = Sha256::new();
            hash.update(format!(
                "{}\n{}\n0130=0130\n",
                data::ALGORITHM,
                data::UNICODE_VERSION
            ));
            let mut previous = None;
            for &(first, last) in data::ASSIGNED {
                ensure!(
                    first <= last
                        && char::from_u32(first).is_some()
                        && char::from_u32(last).is_some()
                        && !(first <= 0xDFFF && last >= 0xD800)
                        && previous.is_none_or(|end| first > end + 1),
                    "Compiled Unicode assigned ranges are malformed"
                );
                previous = Some(last);
                hash.update(format!("A:{first:06X}:{last:06X}\n"));
            }
            previous = None;
            for &(source, destination) in data::LOWERCASE {
                ensure!(
                    source != 0x0130
                        && source != destination
                        && assigned(source)
                        && assigned(destination)
                        && char::from_u32(source).is_some()
                        && char::from_u32(destination).is_some()
                        && (source <= 0xFFFF) == (destination <= 0xFFFF)
                        && previous.is_none_or(|last| source > last),
                    "Compiled Unicode lowercase pairs are malformed"
                );
                previous = Some(source);
                hash.update(format!("M:{source:06X}:{destination:06X}\n"));
            }
            ensure!(
                format!("{:x}", hash.finalize()) == data::TABLE_SHA256,
                "Compiled Unicode mapping/admission table differs from its declared dataset digest"
            );
            Ok(())
        };
        check().map_err(|error| format!("{error:#}"))
    });
    ensure!(
        admission.is_ok(),
        "{}",
        admission
            .as_ref()
            .err()
            .map(String::as_str)
            .unwrap_or("Unicode admission failed")
    );
    Ok(())
}

pub(super) fn validate_profile(profile: Option<&CasingProfile>) -> Result<()> {
    if let Some(profile) = profile {
        ensure!(
            profile.algorithm == data::ALGORITHM
                && profile.unicode_version == data::UNICODE_VERSION
                && profile.source_sha256 == data::SOURCE_SHA256
                && profile.table_sha256 == data::TABLE_SHA256,
            "Unsupported maintenance lowercase algorithm, Unicode version or dataset digest; recorded operation remains fenced"
        );
        verify_tables()?;
    }
    Ok(())
}

pub(super) fn lowercase(name: &str, profile: Option<&CasingProfile>) -> Result<String> {
    validate_profile(profile)?;
    if profile.is_none() {
        ensure!(
            name.is_ascii(),
            "Recorded legacy maintenance lowercase profile admits ASCII names only"
        );
        return Ok(name.to_ascii_lowercase());
    }
    let mut result = String::with_capacity(name.len());
    for character in name.chars() {
        let scalar = character as u32;
        ensure!(
            assigned(scalar),
            "Maintenance Unicode 15.0 lowercase refuses unassigned scalar U+{scalar:04X}; a newer casing dataset must be explicitly qualified"
        );
        let mapped = match data::LOWERCASE.binary_search_by_key(&scalar, |(source, _)| *source) {
            Ok(position) => data::LOWERCASE[position].1,
            Err(_) => scalar,
        };
        let mapped = char::from_u32(mapped)
            .ok_or_else(|| anyhow::anyhow!("Compiled lowercase mapping is not scalar"))?;
        ensure!(
            mapped.len_utf16() == character.len_utf16(),
            "Lowercase mapping changed UTF16 width"
        );
        result.push(mapped);
    }
    Ok(result)
}
