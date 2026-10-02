use super::*;
use anyhow::{Context, Result, ensure};
use std::{
    ffi::{CString, OsStr, c_void},
    path::Path,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
type Ref = *const c_void;
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithCString(
        allocator: Ref,
        text: *const std::ffi::c_char,
        encoding: u32,
    ) -> Ref;
    fn CFStringGetLength(string: Ref) -> isize;
    fn CFStringGetCString(
        string: Ref,
        buffer: *mut std::ffi::c_char,
        size: isize,
        encoding: u32,
    ) -> u8;
    fn CFURLCreateWithString(allocator: Ref, string: Ref, base: Ref) -> Ref;
    fn CFURLCopyFileSystemPath(url: Ref, style: isize) -> Ref;
    fn CFErrorGetCode(error: Ref) -> isize;
    fn CFRelease(value: Ref);
}
// Apple Launch Services declarations: the copy function returns a retained URL;
// the setter returns OSStatus and never receives a command/string for execution.
#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn LSCopyDefaultApplicationURLForURL(url: Ref, roles: u32, error: *mut Ref) -> Ref;
    fn LSSetDefaultHandlerForURLScheme(scheme: Ref, bundle_id: Ref) -> i32;
}
struct Owned(Ref);
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) }
        }
    }
}
fn string(value: &str) -> Result<Owned> {
    let value = CString::new(value)?;
    let result = unsafe { CFStringCreateWithCString(ptr::null(), value.as_ptr(), 0x08000100) };
    ensure!(!result.is_null(), "Create Launch Services string failed");
    Ok(Owned(result))
}
fn string_value(value: Ref) -> Result<String> {
    let length = unsafe { CFStringGetLength(value) };
    ensure!(
        (0..=262144).contains(&length),
        "Launch Services path exceeds bound"
    );
    let mut bytes = vec![0u8; length as usize * 4 + 1];
    let ok = unsafe {
        CFStringGetCString(
            value,
            bytes.as_mut_ptr().cast(),
            bytes.len() as isize,
            0x08000100,
        )
    };
    ensure!(ok != 0, "Decode Launch Services path failed");
    let end = bytes
        .iter()
        .position(|b| *b == 0)
        .context("Launch Services path lacks terminator")?;
    bytes.truncate(end);
    Ok(String::from_utf8(bytes)?)
}
fn selected(scheme: Scheme) -> Result<Option<PathBuf>> {
    let text = string(&format!("{}://modsync-registration.invalid", scheme.name()))?;
    let url = Owned(unsafe { CFURLCreateWithString(ptr::null(), text.0, ptr::null()) });
    ensure!(!url.0.is_null(), "Create protocol query URL failed");
    let mut error = ptr::null();
    let result = Owned(unsafe { LSCopyDefaultApplicationURLForURL(url.0, u32::MAX, &mut error) });
    let error = Owned(error);
    if result.0.is_null() {
        ensure!(
            error.0.is_null() || unsafe { CFErrorGetCode(error.0) } == -10814,
            "Launch Services default query failed"
        );
        return Ok(None);
    }
    let path = Owned(unsafe { CFURLCopyFileSystemPath(result.0, 0) });
    ensure!(
        !path.0.is_null(),
        "Default application has no filesystem path"
    );
    Ok(Some(std::fs::canonicalize(string_value(path.0)?)?))
}
const LSREGISTER: &str = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
struct Bundle {
    path: PathBuf,
    id: String,
}
fn bundle(identity: &Identity, flag: &AtomicBool) -> Result<Bundle> {
    let executable = Path::new(&identity.executable);
    let macos = executable.parent().context("Executable has no parent")?;
    let contents = macos.parent().context("Executable is outside a bundle")?;
    let path = contents
        .parent()
        .context("Executable is outside a bundle")?;
    ensure!(
        macos.file_name().is_some_and(|v| v == "MacOS")
            && contents.file_name().is_some_and(|v| v == "Contents")
            && path.extension().is_some_and(|v| v == "app"),
        "macOS registration requires an installed .app bundle"
    );
    let info = contents.join("Info.plist");
    let original = safe_file(&info)?.context("Bundle Info.plist missing")?;
    let json = command::checked(
        "/usr/bin/plutil",
        &[
            OsStr::new("-convert"),
            OsStr::new("json"),
            OsStr::new("-o"),
            OsStr::new("-"),
            info.as_os_str(),
        ],
        flag,
    )?;
    ensure!(
        safe_file(&info)?.as_deref() == Some(original.as_slice()),
        "Bundle metadata changed while reading"
    );
    let value: serde_json::Value = serde_json::from_str(&json)?;
    ensure!(
        value["ModSyncPackageToken"].as_str() == Some(&identity.package_token)
            && value["ModSyncChannel"].as_str() == Some(&identity.channel),
        "Bundle package/channel token differs from registration request"
    );
    let name = value["CFBundleExecutable"]
        .as_str()
        .context("Bundle executable declaration missing")?;
    ensure!(
        Path::new(name).components().count() == 1
            && name != "."
            && name != ".."
            && std::fs::canonicalize(macos.join(name))? == executable,
        "Bundle executable declaration differs from request"
    );
    let id = value["CFBundleIdentifier"]
        .as_str()
        .filter(|value| {
            !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
        })
        .context("Bundle identifier missing/invalid")?;
    let types = value["CFBundleURLTypes"]
        .as_array()
        .context("Bundle protocol declarations missing")?;
    let schemes: std::collections::BTreeSet<_> = types
        .iter()
        .filter_map(|value| value["CFBundleURLSchemes"].as_array())
        .flatten()
        .filter_map(|value| value.as_str())
        .collect();
    ensure!(
        schemes == ["modsync", "nxm"].into_iter().collect(),
        "Owned bundle must declare exactly modsync and nxm URL schemes"
    );
    Ok(Bundle {
        path: path.into(),
        id: id.into(),
    })
}
fn state(bundle: &Bundle, scheme: Scheme, flag: &AtomicBool) -> Result<SchemeReport> {
    let selected = selected(scheme)?;
    let state = match &selected {
        None => State::Absent,
        Some(path) if path == &bundle.path => State::Owned,
        Some(path) => {
            let info = path.join("Contents/Info.plist");
            match safe_file(&info)? {
                None => State::Foreign,
                Some(before) => {
                    let parsed = command::checked(
                        "/usr/bin/plutil",
                        &[
                            OsStr::new("-convert"),
                            OsStr::new("json"),
                            OsStr::new("-o"),
                            OsStr::new("-"),
                            info.as_os_str(),
                        ],
                        flag,
                    )?;
                    ensure!(
                        safe_file(&info)?.as_deref() == Some(before.as_slice()),
                        "Selected bundle metadata changed while reading"
                    );
                    let value: serde_json::Value = serde_json::from_str(&parsed)?;
                    if value["ModSyncPackageToken"]
                        .as_str()
                        .is_some_and(|v| !v.is_empty())
                        && value["ModSyncChannel"]
                            .as_str()
                            .is_some_and(|v| !v.is_empty())
                    {
                        State::OtherModSync
                    } else {
                        State::Foreign
                    }
                }
            }
        }
    };
    Ok(SchemeReport {
        scheme,
        state,
        handler: selected.map(|path| path.to_string_lossy().into_owned()),
        changed: false,
        message: None,
    })
}
pub(super) fn run(
    operation: &'static str,
    request: &Request,
    identity: &Identity,
    flag: &AtomicBool,
    displayed: Option<&RegistrationReport>,
) -> Result<RegistrationReport> {
    let _lock = if operation == "status" {
        None
    } else {
        Some(mutation_lock(flag)?)
    };
    let bundle = bundle(identity, flag)?;
    let mut entries = Vec::new();
    for scheme in &request.schemes {
        entries.push(state(&bundle, *scheme, flag)?);
    }
    check_confirmed(request, displayed, "macos", &entries)?;
    if operation == "register" && !request.replace {
        ensure!(
            entries
                .iter()
                .all(|entry| matches!(entry.state, State::Absent | State::Owned)),
            "Another protocol handler is selected; explicit --replace required"
        );
    }
    let mut warnings = Vec::new();
    if operation == "register" && entries.iter().any(|entry| entry.state != State::Owned) {
        command::checked(
            LSREGISTER,
            &[OsStr::new("-f"), bundle.path.as_os_str()],
            flag,
        )?;
        for entry in &mut entries {
            ensure!(
                !flag.load(Ordering::Acquire),
                "Protocol registration cancelled"
            );
            if entry.state == State::Owned {
                continue;
            }
            let scheme = string(entry.scheme.name())?;
            let id = string(&bundle.id)?;
            let result = unsafe { LSSetDefaultHandlerForURLScheme(scheme.0, id.0) };
            ensure!(result == 0, "Set Launch Services default failed: {result}");
            *entry = state(&bundle, entry.scheme, flag)?;
            ensure!(
                entry.state == State::Owned,
                "Launch Services selected another bundle location; registration not owned"
            );
            entry.changed = true;
        }
        warnings.push("Launch Services advertises both declared schemes for the bundle; requested defaults were verified independently".into());
    } else if operation == "unregister" && entries.iter().any(|entry| entry.state == State::Owned) {
        ensure!(
            request.schemes.len() == 2 && entries.iter().all(|entry| entry.state == State::Owned),
            "Launch Services unregisters a whole bundle; both schemes must still select this owned bundle"
        );
        for scheme in [Scheme::Modsync, Scheme::Nxm] {
            ensure!(
                state(&bundle, scheme, flag)?.state == State::Owned,
                "Default application changed before bundle unregister"
            );
        }
        command::checked(
            LSREGISTER,
            &[OsStr::new("-u"), bundle.path.as_os_str()],
            flag,
        )?;
        for entry in &mut entries {
            *entry = state(&bundle, entry.scheme, flag)?;
            ensure!(
                entry.state != State::Owned,
                "Launch Services still selects the bundle after unregister"
            );
            entry.changed = true;
        }
        warnings.push("Unregistered the bundle from Launch Services as a whole; no per-scheme-only deletion is claimed".into());
    } else if operation == "unregister" {
        for entry in &mut entries {
            entry.message = Some("Foreign/newer application preserved".into());
        }
    }
    Ok(RegistrationReport {
        operation,
        platform: "macos",
        entries,
        warnings,
    })
}
