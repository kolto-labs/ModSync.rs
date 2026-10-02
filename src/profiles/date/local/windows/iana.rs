//! Qualified default-system ICU mapping for the .NET 9 alternative-ID branch.
//! Dynamic C exports avoid windows-sys' static legacy icuin.dll dependency.
use anyhow::{Context, Result, ensure};
use std::{os::windows::ffi::OsStrExt, ptr};
use windows_sys::Win32::{
    Foundation::{FreeLibrary, HMODULE},
    System::{
        LibraryLoader::{
            GetModuleFileNameW, GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
        },
        SystemInformation::GetSystemDirectoryW,
    },
};

type Version = unsafe extern "C" fn(*mut u8);
type DataVersion = unsafe extern "C" fn(*mut u8, *mut i32);
type TextVersion = unsafe extern "C" fn(*mut i32) -> *const u8;
type Directory = unsafe extern "C" fn() -> *const u8;
type Convert = unsafe extern "C" fn(*const u16, i32, *mut u16, i32, *mut i32) -> i32;

#[derive(PartialEq, Eq)]
struct Configuration {
    invariant: Option<Vec<u16>>,
    nls: Option<Vec<u16>>,
    app_local: Option<Vec<u16>>,
    data: Option<Vec<u16>>,
}
fn setting(name: &str) -> Result<Option<Vec<u16>>> {
    std::env::var_os(name)
        .map(|value| {
            let units: Vec<_> = value.encode_wide().take(257).collect();
            ensure!(
                units.len() <= 256,
                "Windows ICU configuration exceeds its bound"
            );
            Ok(units)
        })
        .transpose()
}
fn enabled(value: Option<&[u16]>) -> Result<bool> {
    let Some(value) = value else { return Ok(false) };
    let value =
        String::from_utf16(value).context("Windows ICU configuration is not qualified UTF-16")?;
    if value.is_empty() || value == "0" || value.eq_ignore_ascii_case("false") {
        return Ok(false);
    }
    ensure!(
        value == "1" || value.eq_ignore_ascii_case("true"),
        "Windows ICU configuration requires an explicit boolean"
    );
    Ok(true)
}
impl Configuration {
    fn read() -> Result<Self> {
        Ok(Self {
            invariant: setting("DOTNET_SYSTEM_GLOBALIZATION_INVARIANT")?,
            nls: setting("DOTNET_SYSTEM_GLOBALIZATION_USENLS")?,
            app_local: setting("DOTNET_SYSTEM_GLOBALIZATION_APPLOCALICU")?,
            data: setting("ICU_DATA")?,
        })
    }
    fn disabled(&self) -> Result<bool> {
        let invariant = enabled(self.invariant.as_deref())?;
        let nls = enabled(self.nls.as_deref())?;
        Ok(invariant || nls)
    }
    fn qualify_system(&self) -> Result<()> {
        ensure!(
            self.app_local.as_ref().is_none_or(Vec::is_empty),
            "App-local ICU timezone mapping is unsupported"
        );
        ensure!(
            self.data.as_ref().is_none_or(Vec::is_empty),
            "Custom ICU data timezone mapping is unsupported"
        );
        Ok(())
    }
}

struct Module(HMODULE);
impl Drop for Module {
    fn drop(&mut self) {
        unsafe { FreeLibrary(self.0) };
    }
}
impl Module {
    fn system() -> Result<(Self, Vec<u16>)> {
        let name: Vec<u16> = "icu.dll".encode_utf16().chain(Some(0)).collect();
        let handle =
            unsafe { LoadLibraryExW(name.as_ptr(), ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32) };
        ensure!(
            !handle.is_null(),
            "Combined System32 ICU is unavailable; legacy or other globalization profiles are unsupported"
        );
        let module = Self(handle);
        let mut path = [0u16; 1024];
        let length =
            unsafe { GetModuleFileNameW(handle, path.as_mut_ptr(), path.len() as u32) } as usize;
        ensure!(
            length > 0 && length < path.len(),
            "Loaded ICU module path is unavailable or exceeds its bound"
        );
        let mut directory = [0u16; 1024];
        let count =
            unsafe { GetSystemDirectoryW(directory.as_mut_ptr(), directory.len() as u32) } as usize;
        ensure!(
            count > 0 && count < directory.len(),
            "System32 path is unavailable or exceeds its bound"
        );
        let mut expected = directory[..count].to_vec();
        if expected.last() != Some(&(b'\\' as u16)) {
            expected.push(b'\\' as u16);
        }
        expected.extend("icu.dll".encode_utf16());
        // The SDK path is preserved; only ASCII case in the bounded path is ignored.
        let fold = |unit: u16| {
            if (b'A' as u16..=b'Z' as u16).contains(&unit) {
                unit + 32
            } else {
                unit
            }
        };
        ensure!(
            path[..length]
                .iter()
                .copied()
                .map(fold)
                .eq(expected.iter().copied().map(fold)),
            "Loaded ICU module is outside the qualified System32 profile"
        );
        Ok((module, path[..length].to_vec()))
    }
    fn symbol(&self, name: &'static [u8]) -> Option<unsafe extern "system" fn() -> isize> {
        unsafe { GetProcAddress(self.0, name.as_ptr()) }
    }
}
struct Icu {
    _module: Module,
    path: Vec<u16>,
    version: Version,
    data_version: DataVersion,
    cldr_version: DataVersion,
    tz_version: TextVersion,
    directory: Directory,
    convert: Option<Convert>,
}
impl Icu {
    fn load() -> Result<Self> {
        let (module, path) = Module::system()?;
        // ICU's SDK U_EXPORT2 is __cdecl. Each transmute is the named header signature.
        let version = unsafe {
            std::mem::transmute::<unsafe extern "system" fn() -> isize, Version>(
                module
                    .symbol(b"u_getVersion\0")
                    .context("ICU version export is unavailable")?,
            )
        };
        let data_version = unsafe {
            std::mem::transmute::<unsafe extern "system" fn() -> isize, DataVersion>(
                module
                    .symbol(b"u_getDataVersion\0")
                    .context("ICU data-version export is unavailable")?,
            )
        };
        let cldr_version = unsafe {
            std::mem::transmute::<unsafe extern "system" fn() -> isize, DataVersion>(
                module
                    .symbol(b"ulocdata_getCLDRVersion\0")
                    .context("ICU CLDR-version export is unavailable")?,
            )
        };
        let tz_version = unsafe {
            std::mem::transmute::<unsafe extern "system" fn() -> isize, TextVersion>(
                module
                    .symbol(b"ucal_getTZDataVersion\0")
                    .context("ICU timezone-version export is unavailable")?,
            )
        };
        let directory = unsafe {
            std::mem::transmute::<unsafe extern "system" fn() -> isize, Directory>(
                module
                    .symbol(b"u_getDataDirectory\0")
                    .context("ICU data-directory export is unavailable")?,
            )
        };
        let convert = module
            .symbol(b"ucal_getWindowsTimeZoneID\0")
            .map(|symbol| unsafe {
                std::mem::transmute::<unsafe extern "system" fn() -> isize, Convert>(symbol)
            });
        Ok(Self {
            _module: module,
            path,
            version,
            data_version,
            cldr_version,
            tz_version,
            directory,
            convert,
        })
    }
    fn metadata(&self, calls: &mut usize) -> Result<Metadata> {
        let mut version = [0; 4];
        count(calls)?;
        unsafe { (self.version)(version.as_mut_ptr()) };
        let mut data = [0; 4];
        let mut status = 0;
        count(calls)?;
        unsafe { (self.data_version)(data.as_mut_ptr(), &mut status) };
        ensure!(status <= 0, "ICU data-version query failed ({status})");
        let mut cldr = [0; 4];
        status = 0;
        count(calls)?;
        unsafe { (self.cldr_version)(cldr.as_mut_ptr(), &mut status) };
        ensure!(status <= 0, "ICU CLDR-version query failed ({status})");
        status = 0;
        count(calls)?;
        let text = unsafe { (self.tz_version)(&mut status) };
        ensure!(status <= 0, "ICU timezone-version query failed ({status})");
        let tz = bounded_text(text, 64)?;
        ensure!(!tz.is_empty(), "ICU timezone version is unavailable");
        count(calls)?;
        let directory = bounded_text(unsafe { (self.directory)() }, 256)?;
        ensure!(
            directory.is_empty(),
            "ICU custom data directory is unsupported"
        );
        Ok(Metadata {
            path: self.path.clone(),
            version,
            data,
            cldr,
            tz,
            directory,
        })
    }
}
fn count(calls: &mut usize) -> Result<()> {
    *calls += 1;
    ensure!(
        *calls <= 48,
        "Windows ICU capture exceeds native call bound"
    );
    Ok(())
}
fn bounded_text(text: *const u8, maximum: usize) -> Result<Vec<u8>> {
    ensure!(!text.is_null(), "ICU returned unavailable bounded text");
    let mut bytes = Vec::new();
    // SDK APIs return valid terminated C strings; enforce a bounded scan.
    for index in 0..=maximum {
        let byte = unsafe { *text.add(index) };
        if byte == 0 {
            return Ok(bytes);
        }
        ensure!(index < maximum, "ICU text exceeds capture bound");
        bytes.push(byte);
    }
    unreachable!()
}
#[derive(PartialEq, Eq)]
struct Metadata {
    path: Vec<u16>,
    version: [u8; 4],
    data: [u8; 4],
    cldr: [u8; 4],
    tz: Vec<u8>,
    directory: Vec<u8>,
}
#[derive(PartialEq, Eq)]
enum Conversion {
    Disabled,
    MissingExport,
    NoMap { status: i32 },
    Mapped { status: i32, id: String },
}
#[derive(PartialEq, Eq)]
pub(super) struct Observation {
    configuration: Configuration,
    metadata: Option<Metadata>,
    conversion: Conversion,
}
impl Observation {
    pub(super) fn mapped_id(&self) -> Option<&str> {
        match &self.conversion {
            Conversion::Mapped { id, .. } => Some(id),
            _ => None,
        }
    }
}
#[derive(Default)]
pub(super) struct Mapper {
    icu: Option<Icu>,
    calls: usize,
}
impl Mapper {
    pub(super) fn map(&mut self, id: &str) -> Result<Observation> {
        ensure!(
            !id.contains(['\\', '\n', '\r', '\0']),
            "Windows alternative timezone ID is unsupported"
        );
        let input: Vec<_> = id.encode_utf16().collect();
        ensure!(
            input.len() <= 255,
            "Windows alternative timezone ID exceeds its bound"
        );
        let configuration = Configuration::read()?;
        if configuration.disabled()? {
            ensure!(
                configuration == Configuration::read()?,
                "Windows ICU configuration changed during mapping"
            );
            return Ok(Observation {
                configuration,
                metadata: None,
                conversion: Conversion::Disabled,
            });
        }
        configuration.qualify_system()?;
        if self.icu.is_none() {
            self.icu = Some(Icu::load()?);
        }
        let icu = self
            .icu
            .as_ref()
            .context("Pinned ICU mapping module is unavailable")?;
        let metadata = icu.metadata(&mut self.calls)?;
        let conversion = if let Some(convert) = icu.convert {
            let input: Vec<_> = input.into_iter().chain(Some(0)).collect();
            let mut output = [0u16; 100];
            let mut status = 0;
            count(&mut self.calls)?;
            let length = unsafe {
                convert(
                    input.as_ptr(),
                    -1,
                    output.as_mut_ptr(),
                    output.len() as i32,
                    &mut status,
                )
            };
            ensure!(
                status <= 0 && (0..=100).contains(&length),
                "ICU timezone mapping failed or exceeds its fixed output bound ({status})"
            );
            if length == 0 {
                Conversion::NoMap { status }
            } else {
                let id = String::from_utf16(&output[..length as usize])
                    .context("Mapped Windows timezone ID is not qualified UTF-16")?;
                ensure!(
                    !id.contains(['\\', '/', '\0', '\n', '\r']) && id != "." && id != "..",
                    "Mapped Windows timezone ID is not a qualified registry component"
                );
                Conversion::Mapped { status, id }
            }
        } else {
            Conversion::MissingExport
        };
        ensure!(
            metadata == icu.metadata(&mut self.calls)? && configuration == Configuration::read()?,
            "Windows ICU mapping authority changed during capture"
        );
        Ok(Observation {
            configuration,
            metadata: Some(metadata),
            conversion,
        })
    }
}
