//! Windows native user locale with user overrides. This identifies NLS data;
//! it does not claim to reproduce a separately configured .NET ICU dataset.
use super::NumberCultureSnapshot;
use anyhow::{Context, Result, ensure};
use windows_sys::Win32::Globalization::*;
pub(super) fn current() -> Result<NumberCultureSnapshot> {
    let mut name = [0u16; 85];
    let count = unsafe { GetUserDefaultLocaleName(name.as_mut_ptr(), name.len() as i32) };
    ensure!(
        count > 0 && count <= name.len() as i32,
        "Windows user locale unavailable"
    );
    let locale =
        String::from_utf16(&name[..count as usize - 1]).context("Invalid Windows locale name")?;
    let read = |field| -> Result<String> {
        let mut buffer = [0u16; 128];
        let count = unsafe {
            GetLocaleInfoEx(
                name.as_ptr(),
                field,
                buffer.as_mut_ptr(),
                buffer.len() as i32,
            )
        };
        ensure!(
            count > 0 && count <= buffer.len() as i32,
            "Windows numeric symbol unavailable or too long"
        );
        String::from_utf16(&buffer[..count as usize - 1]).context("Invalid Windows numeric symbol")
    };
    let positive = read(LOCALE_SPOSITIVESIGN)?;
    NumberCultureSnapshot {
        locale,
        provider: "windows-nls-user-overrides",
        decimal: read(LOCALE_SDECIMAL)?,
        group: read(LOCALE_STHOUSAND)?,
        positive: if positive.is_empty() {
            "+".into()
        } else {
            positive
        },
        negative: read(LOCALE_SNEGATIVESIGN)?,
        infinity: read(LOCALE_SPOSINFINITY)?,
        negative_infinity: read(LOCALE_SNEGINFINITY)?,
        nan: read(LOCALE_SNAN)?,
        negative_space: read(LOCALE_INEGNUMBER)? == "2",
    }
    .validate()
}
