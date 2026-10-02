use super::NumberCultureSnapshot;
use anyhow::{Context, Result, ensure};
use libloading::Library;
use std::ffi::{CString, c_char, c_void};
type Open =
    unsafe extern "C" fn(i32, *const u16, i32, *const c_char, *mut c_void, *mut i32) -> *mut c_void;
type Close = unsafe extern "C" fn(*mut c_void);
type Symbol = unsafe extern "C" fn(*const c_void, i32, *mut u16, i32, *mut i32) -> i32;
type Pattern = unsafe extern "C" fn(*const c_void, i8, *mut u16, i32, *mut i32) -> i32;
type DefaultLocale = unsafe extern "C" fn() -> *const c_char;
struct Format {
    pointer: *mut c_void,
    close: Close,
}
impl Drop for Format {
    fn drop(&mut self) {
        unsafe { (self.close)(self.pointer) };
    }
}
unsafe fn function<T: Copy>(library: &Library, name: &str, suffix: &str) -> Result<T> {
    let symbol = format!("{name}{suffix}\0");
    // Caller supplies the declared ICU C ABI and retains both libraries until
    // all function pointers and native format handles have been discarded.
    Ok(*unsafe { library.get::<T>(symbol.as_bytes()) }?)
}
fn libraries() -> Result<(Library, Library, String)> {
    #[cfg(target_os = "macos")]
    {
        let path = "/usr/lib/libicucore.A.dylib";
        // Fixed system path, no user-authored plugin path.
        return Ok((
            unsafe { Library::new(path) }?,
            unsafe { Library::new(path) }?,
            String::new(),
        ));
    }
    #[cfg(target_os = "linux")]
    {
        // ICU C symbols carry their major ABI version on Linux. Each pair is
        // loaded by the same explicit SONAME; never mix uc/i18n major versions.
        for major in (50..=99).rev() {
            let uc = unsafe { Library::new(format!("libicuuc.so.{major}")) };
            let i18n = unsafe { Library::new(format!("libicui18n.so.{major}")) };
            if let (Ok(uc), Ok(i18n)) = (uc, i18n) {
                return Ok((uc, i18n, format!("_{major}")));
            }
        }
        anyhow::bail!("ICU numeric locale data unavailable (supported system ABI majors 50–99)")
    }
}
fn c_string(pointer: *const c_char) -> Result<String> {
    ensure!(!pointer.is_null(), "ICU returned no default locale");
    let mut bytes = Vec::new();
    for index in 0..=157 {
        // ICU documents a static NUL-terminated default locale identifier.
        let byte = unsafe { *pointer.add(index) } as u8;
        if byte == 0 {
            return String::from_utf8(bytes).context("ICU locale is not UTF-8");
        }
        bytes.push(byte);
    }
    anyhow::bail!("ICU locale identifier exceeds 157 bytes")
}
pub(super) fn current() -> Result<NumberCultureSnapshot> {
    let (uc, i18n, suffix) = libraries()?;
    let default: DefaultLocale = unsafe { function(&uc, "uloc_getDefault", &suffix) }?;
    let mut locale = c_string(unsafe { default() })?;
    #[cfg(target_os = "macos")]
    if locale.is_empty() || locale == "en_US_POSIX" {
        locale = apple_locale()?;
    }
    #[cfg(target_os = "linux")]
    if locale.is_empty() || locale == "en_US_POSIX" {
        return Ok(NumberCultureSnapshot::invariant());
    }
    // .NET obtains base locale rather than honoring arbitrary ICU numbering-
    // system keywords when constructing the default CultureInfo.
    locale.truncate(locale.find('@').unwrap_or(locale.len()));
    let name = CString::new(locale.as_str())?;
    let open: Open = unsafe { function(&i18n, "unum_open", &suffix) }?;
    let close: Close = unsafe { function(&i18n, "unum_close", &suffix) }?;
    let get: Symbol = unsafe { function(&i18n, "unum_getSymbol", &suffix) }?;
    let pattern: Pattern = unsafe { function(&i18n, "unum_toPattern", &suffix) }?;
    let mut error = 0;
    let pointer = unsafe {
        open(
            1,
            std::ptr::null(),
            0,
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut error,
        )
    };
    ensure!(!pointer.is_null(), "ICU decimal formatter unavailable");
    let format = Format { pointer, close };
    ensure!(error <= 0, "ICU decimal formatter failed: {error}");
    let read = |function: Symbol, field| -> Result<String> {
        let mut buffer = [0u16; 128];
        let mut error = 0;
        let count = unsafe {
            function(
                format.pointer,
                field,
                buffer.as_mut_ptr(),
                buffer.len() as i32,
                &mut error,
            )
        };
        ensure!(
            error <= 0 && (0..128).contains(&count),
            "ICU numeric symbol unavailable or too long"
        );
        String::from_utf16(&buffer[..count as usize]).context("ICU symbol is invalid UTF-16")
    };
    let negative = read(get, 6)?;
    let infinity = read(get, 14)?;
    let mut pattern_buffer = [0u16; 512];
    let mut pattern_error = 0;
    let pattern_len = unsafe {
        pattern(
            format.pointer,
            0,
            pattern_buffer.as_mut_ptr(),
            512,
            &mut pattern_error,
        )
    };
    ensure!(
        pattern_error <= 0 && (0..512).contains(&pattern_len),
        "ICU numeric pattern unavailable or too long"
    );
    let pattern = String::from_utf16(&pattern_buffer[..pattern_len as usize])?;
    let negative_pattern = pattern.rsplit(';').next().unwrap_or(&pattern);
    let mut normalized = String::new();
    if !negative_pattern.contains(['-', '(', ')']) {
        normalized.push('-');
    }
    let mut digit = false;
    let mut space = false;
    for ch in negative_pattern.chars() {
        match ch {
            '#' | '0' if !digit => {
                normalized.push('n');
                digit = true;
            }
            ' ' | '\u{a0}' if !space => {
                normalized.push(' ');
                space = true;
            }
            '-' | '(' | ')' => normalized.push(ch),
            _ => {}
        }
    }
    NumberCultureSnapshot {
        locale,
        provider: "system-icu",
        decimal: read(get, 0)?,
        group: read(get, 1)?,
        positive: read(get, 7)?,
        negative_infinity: format!("{negative}{infinity}"),
        negative_space: normalized == "- n",
        negative,
        infinity,
        nan: read(get, 15)?,
    }
    .validate()
}
#[cfg(target_os = "macos")]
fn apple_locale() -> Result<String> {
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFLocaleCopyCurrent() -> *const c_void;
        fn CFLocaleGetIdentifier(locale: *const c_void) -> *const c_void;
        fn CFStringGetCString(
            string: *const c_void,
            buffer: *mut c_char,
            size: isize,
            encoding: u32,
        ) -> u8;
        fn CFRelease(value: *const c_void);
    }
    let locale = unsafe { CFLocaleCopyCurrent() };
    ensure!(!locale.is_null(), "macOS current locale unavailable");
    struct Owned(*const c_void);
    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe { CFRelease(self.0) };
        }
    }
    let owner = Owned(locale);
    let identifier = unsafe { CFLocaleGetIdentifier(owner.0) };
    ensure!(!identifier.is_null(), "macOS locale identifier unavailable");
    let mut buffer = [0i8; 158];
    ensure!(
        unsafe {
            CFStringGetCString(
                identifier,
                buffer.as_mut_ptr(),
                buffer.len() as isize,
                0x08000100,
            )
        } != 0,
        "macOS locale identifier exceeds bound"
    );
    c_string(buffer.as_ptr())
}
