use super::*;
use anyhow::{Result, ensure};
use std::{
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
use windows_sys::Win32::{
    Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
    System::Registry::*,
};
struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.0);
        }
    }
}
fn wide(value: &str) -> Result<Vec<u16>> {
    ensure!(!value.contains('\0'), "Registry text contains NUL");
    Ok(value.encode_utf16().chain(Some(0)).collect())
}
fn opened(path: &str, write: bool, create: bool) -> Result<Option<Key>> {
    let path = wide(path)?;
    let mut key = ptr::null_mut();
    let access = if write {
        KEY_READ | KEY_WRITE
    } else {
        KEY_READ
    };
    let error = unsafe {
        if create {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                0,
                ptr::null(),
                0,
                access,
                ptr::null(),
                &mut key,
                ptr::null_mut(),
            )
        } else {
            RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, access, &mut key)
        }
    };
    if error == ERROR_FILE_NOT_FOUND && !create {
        return Ok(None);
    }
    ensure!(
        error == ERROR_SUCCESS,
        "Open per-user protocol registry key failed: {error}"
    );
    Ok(Some(Key(key)))
}
fn read(path: &str, name: &str) -> Result<Option<String>> {
    let Some(key) = opened(path, false, false)? else {
        return Ok(None);
    };
    let name = wide(name)?;
    let mut kind = 0;
    let mut size = 0;
    let error = unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            ptr::null(),
            &mut kind,
            ptr::null_mut(),
            &mut size,
        )
    };
    if error == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    ensure!(
        error == ERROR_SUCCESS && kind == REG_SZ && size <= 65536 && size % 2 == 0,
        "Invalid or oversized protocol registry value"
    );
    let mut bytes = vec![0u16; (size as usize / 2) + 1];
    let error = unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            ptr::null(),
            &mut kind,
            bytes.as_mut_ptr().cast(),
            &mut size,
        )
    };
    ensure!(
        error == ERROR_SUCCESS
            && kind == REG_SZ
            && size % 2 == 0
            && size as usize / 2 < bytes.len(),
        "Protocol registry value changed while reading"
    );
    bytes.truncate(size as usize / 2);
    if bytes.last() == Some(&0) {
        bytes.pop();
    }
    ensure!(
        !bytes.contains(&0),
        "Protocol registry value contains embedded NUL"
    );
    Ok(Some(String::from_utf16(&bytes)?))
}
fn write(path: &str, name: &str, value: &str) -> Result<()> {
    let key = opened(path, true, true)?.expect("created key");
    let name = wide(name)?;
    let value = wide(value)?;
    let error = unsafe {
        RegSetValueExW(
            key.0,
            name.as_ptr(),
            0,
            REG_SZ,
            value.as_ptr().cast(),
            (value.len() * 2) as u32,
        )
    };
    ensure!(
        error == ERROR_SUCCESS,
        "Write per-user protocol registry value failed: {error}"
    );
    Ok(())
}
fn root(scheme: Scheme) -> String {
    format!("Software\\Classes\\{}", scheme.name())
}
fn values(identity: &Identity, scheme: Scheme) -> Vec<(String, String, String)> {
    let root = root(scheme);
    vec![
        (
            root.clone(),
            String::new(),
            format!("URL:{} Protocol", scheme.name()),
        ),
        (root.clone(), "URL Protocol".into(), String::new()),
        (
            format!("{root}\\DefaultIcon"),
            String::new(),
            format!("\"{}\",0", identity.executable),
        ),
        (
            format!("{root}\\shell\\open\\command"),
            String::new(),
            format!("\"{}\" gui --protocol \"%1\"", identity.executable),
        ),
        (
            root,
            "ModSyncOwnership".into(),
            serde_json::to_string(identity).expect("serializable identity"),
        ),
    ]
}
fn selected(scheme: Scheme) -> Result<Option<String>> {
    read(
        &format!(
            "Software\\Microsoft\\Windows\\Shell\\Associations\\UrlAssociations\\{}\\UserChoice",
            scheme.name()
        ),
        "ProgId",
    )
}
fn status(identity: &Identity, scheme: Scheme) -> Result<SchemeReport> {
    let choice = selected(scheme)?;
    let root = root(scheme);
    let owner = read(&root, "ModSyncOwnership")?;
    let handler = read(&format!("{root}\\shell\\open\\command"), "")?;
    let state = if choice
        .as_deref()
        .is_some_and(|value| !value.eq_ignore_ascii_case(scheme.name()))
    {
        State::Foreign
    } else if handler.is_none() {
        State::Absent
    } else if let Some(owner) = owner {
        match serde_json::from_str::<Identity>(&owner) {
            Ok(owner) if owner == *identity => {
                let mut equal = true;
                for (path, name, value) in values(identity, scheme) {
                    equal &= read(&path, &name)?.as_deref() == Some(value.as_str());
                }
                if equal {
                    State::Owned
                } else {
                    State::OtherModSync
                }
            }
            Ok(_) => State::OtherModSync,
            Err(_) => State::Foreign,
        }
    } else {
        State::Foreign
    };
    Ok(SchemeReport {
        scheme,
        state,
        handler: choice.or(handler),
        changed: false,
        message: None,
    })
}
fn exact_shape(path: &str, subkeys: u32, values: u32) -> Result<()> {
    let key = opened(path, false, false)?.context("Owned registry branch missing")?;
    let mut actual_subkeys = 0;
    let mut actual_values = 0;
    let error = unsafe {
        RegQueryInfoKeyW(
            key.0,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null(),
            &mut actual_subkeys,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut actual_values,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    ensure!(
        error == ERROR_SUCCESS && actual_subkeys == subkeys && actual_values == values,
        "Protocol registry contains foreign additions; refusing subtree removal"
    );
    Ok(())
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
    ensure!(
        !identity.executable.contains('"'),
        "Protocol executable cannot contain quotes"
    );
    let mut entries = Vec::new();
    for scheme in &request.schemes {
        entries.push(status(identity, *scheme)?);
    }
    check_confirmed(request, displayed, "windows", &entries)?;
    if operation == "register" && !request.replace {
        ensure!(
            entries
                .iter()
                .all(|entry| matches!(entry.state, State::Owned | State::Absent)),
            "Another protocol handler is selected; explicit --replace is required"
        );
    }
    let mut result = RegistrationReport {
        operation,
        platform: "windows",
        entries: Vec::new(),
        warnings: Vec::new(),
    };
    for mut entry in entries {
        ensure!(
            !flag.load(Ordering::Acquire),
            "Protocol registration cancelled"
        );
        if operation == "register" && entry.state != State::Owned {
            ensure!(
                selected(entry.scheme)?
                    .as_deref()
                    .is_none_or(|value| value.eq_ignore_ascii_case(entry.scheme.name())),
                "Windows UserChoice selects another handler; change the default through Windows Settings before registration can become active"
            );
            for (path, name, value) in values(identity, entry.scheme) {
                write(&path, &name, &value)?;
            }
            entry = status(identity, entry.scheme)?;
            ensure!(
                entry.state == State::Owned,
                "Registry registration did not become the active owned handler"
            );
            entry.changed = true;
        } else if operation == "unregister" && entry.state == State::Owned {
            let root = root(entry.scheme);
            for (path, subkeys, value_count) in [
                (root.clone(), 2, 3),
                (format!("{root}\\DefaultIcon"), 0, 1),
                (format!("{root}\\shell"), 1, 0),
                (format!("{root}\\shell\\open"), 1, 0),
                (format!("{root}\\shell\\open\\command"), 0, 1),
            ] {
                exact_shape(&path, subkeys, value_count)?;
            }
            ensure!(
                status(identity, entry.scheme)?.state == State::Owned,
                "Handler changed before unregister"
            );
            for suffix in [
                "\\shell\\open\\command",
                "\\shell\\open",
                "\\shell",
                "\\DefaultIcon",
                "",
            ] {
                let path = wide(&format!("{root}{suffix}"))?;
                let error = unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, path.as_ptr()) };
                ensure!(
                    error == ERROR_SUCCESS,
                    "Remove owned registry branch failed: {error}"
                );
            }
            entry = status(identity, entry.scheme)?;
            entry.changed = true;
        } else if operation == "unregister" {
            entry.message = Some("Foreign/newer/channel handler preserved".into());
        }
        result.entries.push(entry);
    }
    Ok(result)
}
