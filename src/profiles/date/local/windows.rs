//! Immutable Windows registry snapshot for the pinned .NET 9 local-zone rules.
//! Unsupported intake/conversion returns to the authored-text warning adapter.
#[path = "windows/rules.rs"]
mod rules;

use anyhow::{Context, Result, ensure};
use chrono::NaiveDateTime;
use std::ptr;
use windows_sys::Win32::{
    Foundation::{
        ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_ITEMS, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS, FILETIME,
        SYSTEMTIME,
    },
    System::{
        Registry::{
            HKEY, HKEY_LOCAL_MACHINE, KEY_READ, REG_BINARY, REG_DWORD, REG_DWORD_BIG_ENDIAN,
            REG_EXPAND_SZ, REG_MULTI_SZ, REG_NONE, REG_QWORD, REG_SZ, RegCloseKey, RegEnumKeyExW,
            RegOpenKeyExW, RegQueryInfoKeyW, RegQueryValueExW,
        },
        Time::{
            DYNAMIC_TIME_ZONE_INFORMATION, GetDynamicTimeZoneInformation, TIME_ZONE_ID_INVALID,
        },
    },
};

const REGISTRY_ROOT: &str = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Time Zones";
const MAX_ANNUAL: u32 = 1024;
const MAX_BYTES: usize = 1024 * 1024;
const MAX_KEYS: usize = 1024;
const MAX_VALUE: u32 = 2048;

#[derive(Default)]
struct Budget {
    bytes: usize,
    calls: usize,
}
impl Budget {
    fn call(&mut self) -> Result<()> {
        self.calls += 1;
        ensure!(
            self.calls <= 32768,
            "Windows timezone discovery exceeds native call bound"
        );
        Ok(())
    }
    fn bytes(&mut self, count: usize) -> Result<()> {
        self.bytes = self
            .bytes
            .checked_add(count)
            .context("Windows timezone snapshot budget overflow")?;
        ensure!(
            self.bytes <= MAX_BYTES,
            "Windows timezone snapshot exceeds 1 MiB"
        );
        Ok(())
    }
}
#[derive(Clone, PartialEq, Eq)]
enum Value {
    Missing,
    Present { kind: u32, bytes: Vec<u8> },
}
impl Value {
    fn tzi(&self) -> Result<Option<Tzi>> {
        let Self::Present { kind, bytes } = self else {
            return Ok(None);
        };
        // Other registry kinds can become byte[] in .NET's general reader;
        // that broader acceptance is outside this qualified intake profile.
        ensure!(
            *kind == REG_BINARY,
            "Non-binary Windows timezone records are unsupported"
        );
        Ok(<[u8; 44]>::try_from(bytes.as_slice()).ok().map(Tzi::bytes))
    }
    fn year(&self) -> Result<Option<i32>> {
        let Self::Present { kind, bytes } = self else {
            return Ok(None);
        };
        ensure!(
            *kind == REG_DWORD,
            "Non-DWORD Windows dynamic timezone years are unsupported"
        );
        Ok(<[u8; 4]>::try_from(bytes.as_slice())
            .ok()
            .map(i32::from_le_bytes))
    }
    fn standard_name(&self) -> Result<Option<Vec<u16>>> {
        let Self::Present { kind, bytes } = self else {
            return Ok(Some(Vec::new()));
        };
        if matches!(
            *kind,
            REG_BINARY | REG_DWORD | REG_DWORD_BIG_ENDIAN | REG_QWORD | REG_MULTI_SZ | REG_NONE
        ) {
            return Ok(None); // GetValue(..., "") as string yields null for these kinds.
        }
        ensure!(
            *kind != REG_EXPAND_SZ,
            "Expanded Windows timezone standard names are unsupported"
        );
        ensure!(
            *kind == REG_SZ,
            "Unknown Windows timezone standard-name type is unsupported"
        );
        // RegistryKey pads an odd final byte and removes exactly one trailing NUL.
        let mut units: Vec<u16> = bytes
            .chunks(2)
            .map(|pair| u16::from_le_bytes([pair[0], *pair.get(1).unwrap_or(&0)]))
            .collect();
        if units.last() == Some(&0) {
            units.pop();
        }
        Ok(Some(units))
    }
}

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
    fn open(parent: HKEY, name: &str, budget: &mut Budget) -> Result<Option<Self>> {
        let mut key = ptr::null_mut();
        budget.call()?;
        let error = unsafe { RegOpenKeyExW(parent, wide(name).as_ptr(), 0, KEY_READ, &mut key) };
        if matches!(error, ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) {
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
    fn stamp(&self, budget: &mut Budget) -> Result<u64> {
        let mut stamp = FILETIME::default();
        budget.call()?;
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
    fn value(&self, name: &str, budget: &mut Budget) -> Result<Value> {
        let name = wide(name);
        let mut size = 0;
        let mut kind = 0;
        budget.call()?;
        let error = unsafe {
            RegQueryValueExW(
                self.0,
                name.as_ptr(),
                ptr::null(),
                &mut kind,
                ptr::null_mut(),
                &mut size,
            )
        };
        if error == ERROR_FILE_NOT_FOUND {
            return Ok(Value::Missing);
        }
        ensure!(
            error == ERROR_SUCCESS,
            "Could not inspect Windows timezone registry value ({error})"
        );
        ensure!(
            size <= MAX_VALUE,
            "Windows timezone registry value exceeds 2048 bytes"
        );
        budget.bytes(size as usize)?;
        let mut bytes = vec![0; size as usize];
        let expected_size = size;
        let expected_kind = kind;
        budget.call()?;
        let data = if bytes.is_empty() {
            ptr::null_mut()
        } else {
            bytes.as_mut_ptr()
        };
        let error = unsafe {
            RegQueryValueExW(
                self.0,
                name.as_ptr(),
                ptr::null(),
                &mut kind,
                data,
                &mut size,
            )
        };
        ensure!(
            error == ERROR_SUCCESS && size == expected_size && kind == expected_kind,
            "Windows timezone registry value changed or became inaccessible ({error})"
        );
        Ok(Value::Present { kind, bytes })
    }
    fn names(&self, budget: &mut Budget) -> Result<Vec<String>> {
        let mut names = Vec::new();
        for index in 0..=MAX_KEYS {
            let mut units = [0u16; 256];
            let mut length = units.len() as u32;
            budget.call()?;
            let error = unsafe {
                RegEnumKeyExW(
                    self.0,
                    index as u32,
                    units.as_mut_ptr(),
                    &mut length,
                    ptr::null(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                )
            };
            if error == ERROR_NO_MORE_ITEMS {
                return Ok(names);
            }
            ensure!(
                error == ERROR_SUCCESS && length > 0 && length <= 255,
                "Windows timezone key enumeration is unavailable or unsupported ({error})"
            );
            ensure!(
                index < MAX_KEYS,
                "Windows timezone discovery exceeds 1024 keys"
            );
            budget.bytes(length as usize * 2)?;
            let name = String::from_utf16(&units[..length as usize])
                .context("Windows timezone registry key contains invalid UTF-16")?;
            ensure!(
                !name.contains(['\\', '/', '\0']) && name != "." && name != "..",
                "Windows timezone registry key is not one bounded component"
            );
            names.push(name);
        }
        anyhow::bail!("Windows timezone enumeration did not terminate within bound")
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
fn native_state() -> Result<Option<NativeState>> {
    let mut value = DYNAMIC_TIME_ZONE_INFORMATION::default();
    let status = unsafe { GetDynamicTimeZoneInformation(&mut value) };
    // Failed API output is not authority. The reference's explicit INVALID
    // branch uses UTC; capture qualifies the repeated return code, not fields.
    if status == TIME_ZONE_ID_INVALID {
        return Ok(None);
    }
    let length = value
        .TimeZoneKeyName
        .iter()
        .position(|&c| c == 0)
        .context("Windows timezone key is not terminated")?;
    let key = String::from_utf16(&value.TimeZoneKeyName[..length])
        .context("Windows timezone key contains invalid UTF-16")?;
    ensure!(
        !key.contains(['\\', '/', '\0']) && key != "." && key != "..",
        "Windows timezone key is not one bounded registry component"
    );
    ensure!(
        value.StandardName.contains(&0) && value.DaylightName.contains(&0),
        "Windows timezone display names are not bounded terminated UTF-16"
    );
    Ok(Some(NativeState {
        key,
        disabled: value.DynamicDaylightTimeDisabled,
        bias: value.Bias,
        standard_bias: value.StandardBias,
        daylight_bias: value.DaylightBias,
        standard: SystemTime::native(value.StandardDate),
        daylight: SystemTime::native(value.DaylightDate),
        standard_name: value.StandardName,
        daylight_name: value.DaylightName,
    }))
}
struct RegistryData {
    default: Tzi,
    annual: Vec<(i32, Tzi)>,
}
#[derive(PartialEq, Eq)]
struct DynamicObservation {
    stamp: u64,
    first: Value,
    last: Value,
    annual: Vec<(i32, Value)>,
}
#[derive(PartialEq, Eq)]
struct ZoneObservation {
    stamp: u64,
    default: Value,
    dynamic: Option<DynamicObservation>,
}
#[derive(PartialEq, Eq)]
struct LoadObservation {
    name: String,
    content: Option<ZoneObservation>,
}
struct DynamicCapture {
    observation: Option<DynamicObservation>,
    records: Option<Vec<(i32, Tzi)>>,
}
fn dynamic_data(parent: HKEY, budget: &mut Budget) -> Result<DynamicCapture> {
    let Some(key) = Key::open(parent, "Dynamic DST", budget)? else {
        return Ok(DynamicCapture {
            observation: None,
            records: Some(Vec::new()),
        });
    };
    let stamp = key.stamp(budget)?;
    let mut observation = DynamicObservation {
        stamp,
        first: key.value("FirstEntry", budget)?,
        last: key.value("LastEntry", budget)?,
        annual: Vec::new(),
    };
    let data = (|| -> Result<Option<Vec<(i32, Tzi)>>> {
        let (Some(first), Some(last)) = (observation.first.year()?, observation.last.year()?)
        else {
            return Ok(None);
        };
        if first == -1 || last == -1 || first > last {
            return Ok(None);
        }
        ensure!(
            first >= 1 && last <= 9999,
            "Windows dynamic timezone years require unsupported out-of-range qualification"
        );
        ensure!(
            last - first < MAX_ANNUAL as i32,
            "Windows timezone exceeds 1024 annual records"
        );
        let mut annual = Vec::new();
        for year in first..=last {
            let value = key.value(&year.to_string(), budget)?;
            let parsed = value.tzi()?;
            observation.annual.push((year, value));
            let Some(parsed) = parsed else {
                return Ok(None);
            };
            annual.push((year, parsed));
        }
        Ok(Some(annual))
    })()?;
    ensure!(
        key.stamp(budget)? == stamp,
        "Windows dynamic timezone changed during capture"
    );
    Ok(DynamicCapture {
        observation: Some(observation),
        records: data,
    })
}
fn registry_data(
    name: &str,
    budget: &mut Budget,
) -> Result<(LoadObservation, Option<RegistryData>)> {
    let Some(zone) = Key::open(
        HKEY_LOCAL_MACHINE,
        &format!("{REGISTRY_ROOT}\\{name}"),
        budget,
    )?
    else {
        return Ok((
            LoadObservation {
                name: name.to_owned(),
                content: None,
            },
            None,
        ));
    };
    let stamp = zone.stamp(budget)?;
    let value = zone.value("TZI", budget)?;
    let parsed = value.tzi()?;
    let (dynamic, data) = if let Some(default) = parsed {
        let dynamic = dynamic_data(zone.0, budget)?;
        (
            dynamic.observation,
            dynamic
                .records
                .map(|annual| RegistryData { default, annual }),
        )
    } else {
        (None, None)
    };
    ensure!(
        zone.stamp(budget)? == stamp,
        "Windows timezone changed during capture"
    );
    Ok((
        LoadObservation {
            name: name.to_owned(),
            content: Some(ZoneObservation {
                stamp,
                default: value,
                dynamic,
            }),
        },
        data,
    ))
}
fn build_registry(data: Option<RegistryData>, disabled: bool) -> Option<rules::Zone> {
    // Only pure constructor/data rejection reaches the reference fallback chain.
    // Operational, unsupported and budget errors never pass through this helper.
    data.and_then(|data| rules::Zone::from_registry(data.default, data.annual, disabled).ok())
}

#[derive(PartialEq, Eq)]
struct MatchObservation {
    stamp: u64,
    record: Value,
    standard_name: Option<Value>,
    matched: bool,
}
#[derive(PartialEq, Eq)]
struct Candidate {
    name: String,
    content: Option<MatchObservation>,
}
fn match_candidate(
    root: HKEY,
    name: &str,
    native: &NativeState,
    budget: &mut Budget,
) -> Result<Candidate> {
    let Some(key) = Key::open(root, name, budget)? else {
        return Ok(Candidate {
            name: name.to_owned(),
            content: None,
        });
    };
    let stamp = key.stamp(budget)?;
    let record = key.value("TZI", budget)?;
    let mut standard_name = None;
    let mut matched = false;
    if let Some(row) = record.tzi()? {
        let standard_equal = row.bias == native.bias
            && row.standard_bias == native.standard_bias
            && row.standard == native.standard;
        let daylight_equal = native.standard == native.daylight
            || (row.daylight_bias == native.daylight_bias && row.daylight == native.daylight);
        if standard_equal && daylight_equal {
            let value = key.value("Std", budget)?;
            let length = native
                .standard_name
                .iter()
                .position(|&c| c == 0)
                .context("Windows native standard name is not terminated")?;
            matched = value
                .standard_name()?
                .is_some_and(|units| units == native.standard_name[..length]);
            standard_name = Some(value);
        }
    }
    ensure!(
        key.stamp(budget)? == stamp,
        "Windows timezone candidate changed during capture"
    );
    Ok(Candidate {
        name: name.to_owned(),
        content: Some(MatchObservation {
            stamp,
            record,
            standard_name,
            matched,
        }),
    })
}
#[derive(PartialEq, Eq)]
struct SearchRoot {
    stamp: u64,
    names: Vec<String>,
    visited: Vec<Candidate>,
    selected: Option<LoadObservation>,
}
#[derive(PartialEq, Eq)]
struct SearchObservation {
    root: Option<SearchRoot>,
}
fn find_registry(
    native: &NativeState,
    budget: &mut Budget,
) -> Result<(SearchObservation, Option<rules::Zone>)> {
    let Some(root) = Key::open(HKEY_LOCAL_MACHINE, REGISTRY_ROOT, budget)? else {
        return Ok((SearchObservation { root: None }, None));
    };
    let stamp = root.stamp(budget)?;
    // Preserve native index order: .NET GetSubKeyNames does not sort the list.
    let names = root.names(budget)?;
    let mut visited = Vec::new();
    let mut selected = None;
    let mut result = None;
    for name in &names {
        let candidate = match_candidate(root.0, name, native, budget)?;
        let matched = candidate
            .content
            .as_ref()
            .is_some_and(|content| content.matched);
        if matched {
            let (load, data) = registry_data(name, budget)?;
            let compared = candidate
                .content
                .as_ref()
                .context("Windows timezone match evidence is unavailable")?;
            let loaded = load
                .content
                .as_ref()
                .context("Selected Windows timezone disappeared during capture")?;
            ensure!(
                compared.stamp == loaded.stamp && compared.record == loaded.default,
                "Selected Windows timezone changed after comparison"
            );
            // Pinned TryCompareTimeZoneInformationToRegistry initializes
            // dstDisabled=false and never changes it. Do not infer it here.
            result = build_registry(data, false);
            selected = Some(load);
        }
        visited.push(candidate);
        // Even a failed full load of the first match proceeds to native fallback,
        // never to a later matching registry key.
        if matched {
            break;
        }
    }
    ensure!(
        root.stamp(budget)? == stamp,
        "Windows timezone namespace changed during capture"
    );
    Ok((
        SearchObservation {
            root: Some(SearchRoot {
                stamp,
                names,
                visited,
                selected,
            }),
        },
        result,
    ))
}
#[derive(PartialEq, Eq)]
enum Branch {
    Direct,
    Matched,
    NativeEnabled,
    NativeDisabled,
    DummyUtc,
}
#[derive(PartialEq, Eq)]
struct Witness {
    direct: Option<LoadObservation>,
    search: Option<SearchObservation>,
    branch: Branch,
}
fn discovery(native: &NativeState, budget: &mut Budget) -> Result<(Witness, rules::Zone)> {
    let direct = if native.key.is_empty() {
        None
    } else {
        let (load, data) = registry_data(&native.key, budget)?;
        if let Some(zone) = build_registry(data, native.disabled) {
            return Ok((
                Witness {
                    direct: Some(load),
                    search: None,
                    branch: Branch::Direct,
                },
                zone,
            ));
        }
        Some(load)
    };
    // Alternative-ID mapping is a separate ICU scope. This conservative shape
    // qualification is not a claim that the failed key is a valid Windows ID.
    ensure!(
        native.key.is_empty() || native.key.contains(' ') || native.key == "UTC",
        "Windows alternate timezone ID lookup is unsupported for this failed key shape"
    );
    let (search, zone) = find_registry(native, budget)?;
    if let Some(zone) = zone {
        return Ok((
            Witness {
                direct,
                search: Some(search),
                branch: Branch::Matched,
            },
            zone,
        ));
    }
    let row = Tzi {
        bias: native.bias,
        standard_bias: native.standard_bias,
        daylight_bias: native.daylight_bias,
        standard: native.standard.clone(),
        daylight: native.daylight.clone(),
    };
    let (branch, zone) = if let Ok(zone) = rules::Zone::from_native(row.clone(), false) {
        (Branch::NativeEnabled, zone)
    } else if let Ok(zone) = rules::Zone::from_native(row, true) {
        (Branch::NativeDisabled, zone)
    } else {
        (Branch::DummyUtc, rules::Zone::utc())
    };
    Ok((
        Witness {
            direct,
            search: Some(search),
            branch,
        },
        zone,
    ))
}

pub(super) struct Snapshot {
    rules: rules::Zone,
}
impl Snapshot {
    pub(super) fn capture() -> Result<Self> {
        let native = native_state()?;
        let Some(state) = native.as_ref() else {
            ensure!(
                native_state()?.is_none(),
                "Windows native timezone status changed during capture"
            );
            return Ok(Self {
                rules: rules::Zone::utc(),
            });
        };
        let mut budget = Budget::default();
        let (witness, zone) = discovery(state, &mut budget)?;
        // Reopen and replay the complete decision once, including failed direct
        // lookup and the ordered search prefix that determined the first match.
        let (current, _) = discovery(state, &mut budget)?;
        ensure!(
            witness == current && native_state()? == native,
            "Windows local timezone changed while taking immutable snapshot"
        );
        Ok(Self { rules: zone })
    }
    pub(super) fn normalize(&self, wall: NaiveDateTime, offset: Option<i32>) -> Result<String> {
        self.rules.normalize(wall, offset)
    }
    pub(super) fn normalize_fallback(&self, wall: NaiveDateTime, offset: i32) -> Result<String> {
        self.rules.normalize_fallback(wall, offset)
    }
}
