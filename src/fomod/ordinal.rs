//! Versioned simple uppercase keys for .NET 9 ICU ordinal name matching.
//! Data and the two BMP ordinal exceptions are generated from retained UCD 15.0.
pub(super) fn key(value: &str) -> String {
    crate::unicode_case::ordinal_key(value)
}
