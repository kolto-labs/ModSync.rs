//! Pinned .NET 9 ICU scalar casing shared by name and merge consumers.
//! Each retained generated table is compiled once. Unmapped scalars preserve
//! identity; maintenance separately enforces its recorded assigned-scalar policy.
#[path = "maintenance/casing/data.rs"]
mod lowercase_data;
#[path = "fomod/ordinal/data.rs"]
mod ordinal_data;

// Metadata and table references for the maintenance policy's existing digest
// verifier. These aliases neither copy tables nor change persisted descriptors.
pub(crate) const ALGORITHM: &str = lowercase_data::ALGORITHM;
pub(crate) const UNICODE_VERSION: &str = lowercase_data::UNICODE_VERSION;
pub(crate) const SOURCE_SHA256: &str = lowercase_data::SOURCE_SHA256;
pub(crate) const TABLE_SHA256: &str = lowercase_data::TABLE_SHA256;
pub(crate) const ASSIGNED: &[(u32, u32)] = lowercase_data::ASSIGNED;
pub(crate) const LOWERCASE: &[(u32, u32)] = lowercase_data::LOWERCASE;

pub(crate) fn invariant_lower_key(value: &str) -> String {
    value.chars().map(invariant_lower_scalar).collect()
}

fn invariant_lower_scalar(value: char) -> char {
    match LOWERCASE.binary_search_by_key(&(value as u32), |(source, _)| *source) {
        Ok(index) => char::from_u32(LOWERCASE[index].1).unwrap_or(value),
        Err(_) => value,
    }
}

pub(crate) fn ordinal_key(value: &str) -> String {
    value.chars().map(ordinal_scalar).collect()
}

pub(crate) fn ordinal_eq(left: &str, right: &str) -> bool {
    left == right
        || left
            .chars()
            .map(ordinal_scalar)
            .eq(right.chars().map(ordinal_scalar))
}

fn ordinal_scalar(value: char) -> char {
    match ordinal_data::UPPERCASE.binary_search_by_key(&(value as u32), |(source, _)| *source) {
        Ok(index) => char::from_u32(ordinal_data::UPPERCASE[index].1).unwrap_or(value),
        Err(_) => value,
    }
}
