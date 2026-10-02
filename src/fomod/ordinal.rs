//! Versioned simple uppercase keys for .NET 9 ICU ordinal name matching.
//! Data and the two BMP ordinal exceptions are generated from retained UCD 15.0.
#[path = "ordinal/data.rs"]
mod data;

pub(super) fn key(value: &str) -> String {
    value.chars().map(uppercase).collect()
}

fn uppercase(value: char) -> char {
    match data::UPPERCASE.binary_search_by_key(&(value as u32), |(source, _)| *source) {
        Ok(index) => char::from_u32(data::UPPERCASE[index].1).unwrap_or(value),
        Err(_) => value,
    }
}
