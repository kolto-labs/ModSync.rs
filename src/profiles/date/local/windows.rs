//! Immutable Windows registry snapshot for the pinned .NET 9 local-zone rules.
//! Unsupported intake/conversion returns to the authored-text warning adapter.
#[path = "windows/rules.rs"]
mod rules;

use anyhow::{Context, Result, ensure};
use chrono::NaiveDateTime;
use std::ptr;
use windows_sys::Win32::{
    Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, FILETIME, SYSTEMTIME},
    System::{
        Registry::{
            HKEY, HKEY_LOCAL_MACHINE, KEY_READ, REG_BINARY, REG_DWORD, RegCloseKey, RegOpenKeyExW,
            RegQueryInfoKeyW, RegQueryValueExW,
        },
        Time::{
            DYNAMIC_TIME_ZONE_INFORMATION, GetDynamicTimeZoneInformation, TIME_ZONE_ID_INVALID,
        },
    },
};

const REGISTRY_ROOT: &str = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Time Zones";
const MAX_ANNUAL: u32 = 1024;
const MAX_BYTES: usize = 1024 * 1024;

struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
impl Key {
    fn open(parent: HKEY, name: &str) -> Result<Option<Self>> {
        let mut key = ptr::null_mut();
        let error = unsafe { RegOpenKeyExW(parent, wide(name).as_ptr(), 0, KEY_READ, &mut key) };
        if error == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        ensure!(
            error == ERROR_SUCCESS,
            "Could not open Windows timezone registry key ({error})"
        );
        ensure!(
            !key.is_null(),
            "Windows timezone registry handle is unavailable"
        );
        Ok(Some(Self(key)))
    }
    fn stamp(&self) -> Result<u64> {
        let mut stamp = FILETIME::default();
        let error = unsafe {
            RegQueryInfoKeyW(
                self.0,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                &mut stamp,
            )
        };
        ensure!(
            error == ERROR_SUCCESS,
            "Could not inspect Windows timezone registry key ({error})"
        );
        Ok(u64::from(stamp.dwLowDateTime) | u64::from(stamp.dwHighDateTime) << 32)
    }
    fn value<const N: usize>(
        &self,
        name: &str,
        expected_type: u32,
        budget: &mut usize,
    ) -> Result<[u8; N]> {
        *budget = budget
            .checked_add(N)
            .context("Windows timezone snapshot budget overflow")?;
        ensure!(
            *budget <= MAX_BYTES,
            "Windows timezone snapshot exceeds 1 MiB"
        );
        let mut bytes = [0; N];
        let mut size = N as u32;
        let mut kind = 0;
        let error = unsafe {
            RegQueryValueExW(
                self.0,
                wide(name).as_ptr(),
                ptr::null(),
                &mut kind,
                bytes.as_mut_ptr(),
                &mut size,
            )
        };
        ensure!(
            error == ERROR_SUCCESS && kind == expected_type && size == N as u32,
            "Windows timezone registry value is unavailable or has unsupported type/length ({error})"
        );
        Ok(bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SystemTime {
    year: u16,
    month: u16,
    weekday: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    millis: u16,
}
impl SystemTime {
    fn native(value: SYSTEMTIME) -> Self {
        Self {
            year: value.wYear,
            month: value.wMonth,
            weekday: value.wDayOfWeek,
            day: value.wDay,
            hour: value.wHour,
            minute: value.wMinute,
            second: value.wSecond,
            millis: value.wMilliseconds,
        }
    }
    fn bytes(bytes: &[u8]) -> Self {
        let at = |i| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
        Self {
            year: at(0),
            month: at(2),
            weekday: at(4),
            day: at(6),
            hour: at(8),
            minute: at(10),
            second: at(12),
            millis: at(14),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Tzi {
    bias: i32,
    standard_bias: i32,
    daylight_bias: i32,
    standard: SystemTime,
    daylight: SystemTime,
}
impl Tzi {
    fn bytes(bytes: [u8; 44]) -> Self {
        let at = |i| i32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
        Self {
            bias: at(0),
            standard_bias: at(4),
            daylight_bias: at(8),
            standard: SystemTime::bytes(&bytes[12..28]),
            daylight: SystemTime::bytes(&bytes[28..44]),
        }
    }
}
#[derive(PartialEq, Eq)]
struct NativeState {
    key: String,
    disabled: bool,
    bias: i32,
    standard_bias: i32,
    daylight_bias: i32,
    standard: SystemTime,
    daylight: SystemTime,
    standard_name: [u16; 32],
    daylight_name: [u16; 32],
}
fn native_state() -> Result<NativeState> {
    let mut value = DYNAMIC_TIME_ZONE_INFORMATION::default();
    let status = unsafe { GetDynamicTimeZoneInformation(&mut value) };
    ensure!(
        status != TIME_ZONE_ID_INVALID,
        "Configured Windows local timezone is unavailable"
    );
    let length = value
        .TimeZoneKeyName
        .iter()
        .position(|&c| c == 0)
        .context("Windows timezone key is not terminated")?;
    ensure!(
        length > 0,
        "Windows timezone key fallback lookup is unsupported"
    );
    let key = String::from_utf16(&value.TimeZoneKeyName[..length])
        .context("Windows timezone key contains invalid UTF-16")?;
    ensure!(
        !key.contains(['\\', '/', '\0']) && key != "." && key != "..",
        "Windows timezone key is not one bounded registry component"
    );
    Ok(NativeState {
        key,
        disabled: value.DynamicDaylightTimeDisabled,
        bias: value.Bias,
        standard_bias: value.StandardBias,
        daylight_bias: value.DaylightBias,
        standard: SystemTime::native(value.StandardDate),
        daylight: SystemTime::native(value.DaylightDate),
        standard_name: value.StandardName,
        daylight_name: value.DaylightName,
    })
}
#[derive(PartialEq, Eq)]
struct RegistryData {
    default: Tzi,
    annual: Vec<(i32, Tzi)>,
}
fn registry_data(name: &str, budget: &mut usize) -> Result<RegistryData> {
    let zone = Key::open(HKEY_LOCAL_MACHINE, &format!("{REGISTRY_ROOT}\\{name}"))?
        .context("Configured Windows timezone registry key is missing; fallback unsupported")?;
    let before = zone.stamp()?;
    let default = Tzi::bytes(zone.value("TZI", REG_BINARY, budget)?);
    let dynamic = Key::open(zone.0, "Dynamic DST")?;
    let mut annual = Vec::new();
    if let Some(dynamic) = &dynamic {
        let dynamic_before = dynamic.stamp()?;
        let first = u32::from_le_bytes(dynamic.value("FirstEntry", REG_DWORD, budget)?);
        let last = u32::from_le_bytes(dynamic.value("LastEntry", REG_DWORD, budget)?);
        ensure!(
            first >= 1 && first <= last && last <= 9999,
            "Windows dynamic timezone years are outside DateTime range"
        );
        ensure!(
            last - first < MAX_ANNUAL,
            "Windows timezone exceeds 1024 annual records"
        );
        for year in first..=last {
            annual.push((
                year as i32,
                Tzi::bytes(dynamic.value(&year.to_string(), REG_BINARY, budget)?),
            ));
        }
        ensure!(
            dynamic.stamp()? == dynamic_before,
            "Windows dynamic timezone changed during capture"
        );
    }
    ensure!(
        zone.stamp()? == before,
        "Windows timezone changed during capture"
    );
    Ok(RegistryData { default, annual })
}

pub(super) struct Snapshot {
    rules: rules::Zone,
}
impl Snapshot {
    pub(super) fn capture() -> Result<Self> {
        let native = native_state()?;
        let mut budget = 0;
        let data = registry_data(&native.key, &mut budget)?;
        // Reopen and reread once: detect key replacement as well as changes to pinned keys.
        let current = registry_data(&native.key, &mut budget)?;
        ensure!(
            data == current && native_state()? == native,
            "Windows local timezone changed while taking immutable snapshot"
        );
        Ok(Self {
            rules: rules::Zone::from_registry(data.default, data.annual, native.disabled)?,
        })
    }
    pub(super) fn normalize(&self, wall: NaiveDateTime, offset: Option<i32>) -> Result<String> {
        self.rules.normalize(wall, offset)
    }
}
