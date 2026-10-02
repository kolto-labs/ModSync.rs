use super::*;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
fn safe_file(path: &Path) -> Result<Option<Vec<u8>>> {
    let bytes = super::safe_file(path)?;
    if bytes.is_some() {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            fs::symlink_metadata(path)?.uid() == unsafe { libc::geteuid() },
            "Per-user protocol artifact is not owned by current user"
        );
    }
    Ok(bytes)
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    schema_version: u32,
    identity: Identity,
    scheme: Scheme,
    previous: Option<String>,
}
struct Paths {
    applications: PathBuf,
    config: PathBuf,
    records: PathBuf,
}
impl Paths {
    fn resolve() -> Result<Self> {
        let data = dirs::data_local_dir().context("Cannot locate user application data")?;
        let config = dirs::config_dir().context("Cannot locate user configuration")?;
        ensure!(
            data.is_absolute() && config.is_absolute(),
            "Protocol directories must be absolute"
        );
        for root in [&data, &config] {
            safe_directory(root, false)?;
        }
        Ok(Self {
            applications: data.join("applications"),
            records: data.join("ModSync/protocol-registration"),
            config,
        })
    }
}
fn safe_directory(path: &Path, create: bool) -> Result<()> {
    for ancestor in path.ancestors().collect::<Vec<_>>().into_iter().rev() {
        match fs::symlink_metadata(ancestor) {
            Ok(meta) => ensure!(
                meta.is_dir() && !meta.file_type().is_symlink(),
                "Unsafe protocol directory: {}",
                ancestor.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !create {
                    continue;
                }
                fs::create_dir(ancestor)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    if let Ok(meta) = fs::symlink_metadata(path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            ensure!(
                meta.uid() == unsafe { libc::geteuid() },
                "Protocol directory is not owned by current user"
            );
        }
    }
    Ok(())
}
fn file_id(identity: &Identity, scheme: Scheme) -> String {
    let hash = format!(
        "{:x}",
        Sha256::digest(format!("{}\0{}", identity.package_token, identity.channel))
    );
    format!("modsync-{}-{}.desktop", &hash[..20], scheme.name())
}
fn query(scheme: Scheme, flag: &AtomicBool) -> Result<Option<String>> {
    let mime = format!("x-scheme-handler/{}", scheme.name());
    let value = command::checked(
        "xdg-mime",
        &[
            OsStr::new("query"),
            OsStr::new("default"),
            OsStr::new(&mime),
        ],
        flag,
    )?;
    if value.is_empty() {
        return Ok(None);
    }
    ensure!(
        !value.contains(['\n', '\r', '/', '\\']) && value.len() <= 512,
        "Invalid default desktop handler response"
    );
    Ok(Some(value))
}
fn identity_from(bytes: &[u8]) -> Option<Identity> {
    std::str::from_utf8(bytes)
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("X-ModSync-Identity="))
        .and_then(|text| serde_json::from_str(text).ok())
}
fn state(
    paths: &Paths,
    identity: &Identity,
    scheme: Scheme,
    flag: &AtomicBool,
) -> Result<SchemeReport> {
    let handler = query(scheme, flag)?;
    let expected = file_id(identity, scheme);
    let state = match &handler {
        None => State::Absent,
        Some(handler) if handler == &expected => {
            let bytes = safe_file(&paths.applications.join(handler))?;
            match bytes.as_deref().and_then(identity_from) {
                Some(owner)
                    if &owner == identity
                        && bytes.as_deref() == Some(desktop(identity, scheme)?.as_bytes()) =>
                {
                    State::Owned
                }
                Some(_) => State::OtherModSync,
                None => State::Foreign,
            }
        }
        Some(handler)
            if safe_file(&paths.applications.join(handler))?
                .as_deref()
                .and_then(identity_from)
                .is_some() =>
        {
            State::OtherModSync
        }
        Some(_) => State::Foreign,
    };
    Ok(SchemeReport {
        scheme,
        state,
        handler,
        changed: false,
        message: None,
    })
}
fn desktop(identity: &Identity, scheme: Scheme) -> Result<String> {
    let entry = super::super::linux_desktop_entry(Path::new(&identity.executable))?;
    let entry = entry
        .lines()
        .map(|line| {
            if line.starts_with("MimeType=") {
                format!("MimeType=x-scheme-handler/{};", scheme.name())
            } else {
                line.into()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!(
        "{entry}\nX-ModSync-Identity={}\n",
        serde_json::to_string(identity)?
    ))
}
fn lock(paths: &Paths, flag: &AtomicBool) -> Result<fs::File> {
    safe_directory(&paths.records, true)?;
    let path = paths.records.join(".lock");
    if let Ok(meta) = fs::symlink_metadata(&path) {
        ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "Unsafe registration lock"
        );
    }
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    ensure!(
        file.metadata()?.is_file(),
        "Registration lock is not regular"
    );
    let started = std::time::Instant::now();
    loop {
        ensure!(
            !flag.load(Ordering::Acquire),
            "Protocol registration cancelled"
        );
        match fs2::FileExt::try_lock_exclusive(&file) {
            Ok(()) => break,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                ensure!(
                    started.elapsed() < std::time::Duration::from_secs(15),
                    "Protocol registration lock timed out"
                );
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(file)
}
fn refresh(paths: &Paths, flag: &AtomicBool) -> Result<()> {
    command::checked(
        "update-desktop-database",
        &[paths.applications.as_os_str()],
        flag,
    )?;
    Ok(())
}
pub(super) fn run(
    operation: &'static str,
    request: &Request,
    identity: &Identity,
    flag: &AtomicBool,
    displayed: Option<&RegistrationReport>,
) -> Result<RegistrationReport> {
    let paths = Paths::resolve()?;
    safe_directory(&paths.applications, false)?;
    safe_directory(&paths.records, false)?;
    let _lock = if operation == "status" {
        None
    } else {
        Some(lock(&paths, flag)?)
    };
    let mut entries = Vec::new();
    for scheme in &request.schemes {
        entries.push(state(&paths, identity, *scheme, flag)?);
    }
    check_confirmed(request, displayed, "linux", &entries)?;
    if operation == "register" && !request.replace {
        ensure!(
            entries
                .iter()
                .all(|entry| matches!(entry.state, State::Absent | State::Owned)),
            "Another protocol handler is selected; explicit --replace is required"
        );
    }
    let mut report = RegistrationReport {
        operation,
        platform: "linux",
        entries: Vec::new(),
        warnings: Vec::new(),
    };
    for mut entry in entries {
        ensure!(
            !flag.load(Ordering::Acquire),
            "Protocol registration cancelled"
        );
        let filename = file_id(identity, entry.scheme);
        let target = paths.applications.join(&filename);
        let record = paths.records.join(format!("{filename}.json"));
        if operation == "register" && entry.state != State::Owned {
            safe_directory(&paths.applications, true)?;
            let previous = entry.handler.clone().filter(|handler| handler != &filename);
            let receipt = Receipt {
                schema_version: 1,
                identity: identity.clone(),
                scheme: entry.scheme,
                previous,
            };
            if let Some(bytes) = safe_file(&target)? {
                let owner = identity_from(&bytes);
                ensure!(
                    request.replace || owner.as_ref() == Some(identity),
                    "Existing desktop file has another owner; explicit --replace required"
                );
            }
            crate::recovery::atomic_write(&record, &serde_json::to_vec_pretty(&receipt)?)?;
            crate::recovery::atomic_write(&target, desktop(identity, entry.scheme)?.as_bytes())?;
            let mime = format!("x-scheme-handler/{}", entry.scheme.name());
            command::checked(
                "xdg-mime",
                &[
                    OsStr::new("default"),
                    OsStr::new(&filename),
                    OsStr::new(&mime),
                ],
                flag,
            )?;
            refresh(&paths, flag)?;
            entry = state(&paths, identity, entry.scheme, flag)?;
            ensure!(
                entry.state == State::Owned,
                "Registration command did not select the owned handler"
            );
            entry.changed = true;
        } else if operation == "unregister"
            && safe_file(&target)?.as_deref() == Some(desktop(identity, entry.scheme)?.as_bytes())
        {
            let was_selected = entry.state == State::Owned;
            let selected_before = entry.handler.clone();
            let bytes = safe_file(&record)?
                .context("Owned registration has no receipt; refusing destructive unregister")?;
            let receipt: Receipt = serde_json::from_slice(&bytes)?;
            ensure!(
                receipt.schema_version == 1
                    && receipt.identity == *identity
                    && receipt.scheme == entry.scheme,
                "Registration receipt belongs to another package/executable"
            );
            ensure!(
                state(&paths, identity, entry.scheme, flag)?.handler == selected_before,
                "Handler changed before unregister"
            );
            if let Some(previous) = receipt.previous.filter(|_| was_selected) {
                ensure!(
                    !previous.is_empty()
                        && previous.len() <= 512
                        && !previous.contains(['\n', '\r', '/', '\\']),
                    "Invalid previous handler receipt"
                );
                let mime = format!("x-scheme-handler/{}", entry.scheme.name());
                command::checked(
                    "xdg-mime",
                    &[
                        OsStr::new("default"),
                        OsStr::new(&previous),
                        OsStr::new(&mime),
                    ],
                    flag,
                )?;
            }
            remove_associations(&paths, entry.scheme, &filename)?;
            ensure!(
                safe_file(&target)?.as_deref() == Some(desktop(identity, entry.scheme)?.as_bytes()),
                "Desktop ownership changed before removal"
            );
            fs::remove_file(&target)?;
            refresh(&paths, flag)?;
            entry = state(&paths, identity, entry.scheme, flag)?;
            ensure!(
                entry.handler.as_deref() != Some(filename.as_str()),
                "Owned association remains after unregister"
            );
            fs::remove_file(record)?;
            entry.changed = true;
        } else if operation == "unregister" {
            entry.message = Some(
                "Association is not owned by this package/channel/executable; preserved".into(),
            );
        }
        report.entries.push(entry);
    }
    Ok(report)
}
fn remove_associations(paths: &Paths, scheme: Scheme, filename: &str) -> Result<()> {
    let mime = format!("x-scheme-handler/{}", scheme.name());
    for directory in [&paths.config, &paths.applications] {
        safe_directory(directory, false)?;
        let items = match fs::read_dir(directory) {
            Ok(items) => items,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        for (index, item) in items.enumerate() {
            ensure!(
                index < 10_000,
                "User MIME directory exceeds inventory bound"
            );
            let path = item?.path();
            if !path
                .file_name()
                .and_then(|v| v.to_str())
                .is_some_and(|v| v == "mimeapps.list" || v.ends_with("-mimeapps.list"))
            {
                continue;
            }
            let Some(bytes) = safe_file(&path)? else {
                continue;
            };
            let original = std::str::from_utf8(&bytes).context("User MIME file is not UTF-8")?;
            let mut changed = false;
            let mut result = String::new();
            for line in original.split_inclusive('\n') {
                let body = line.trim_end_matches(['\n', '\r']);
                if let Some((key, value)) = body.split_once('=')
                    && key.trim() == mime
                {
                    let retained: Vec<_> = value
                        .split(';')
                        .filter(|entry| !entry.is_empty() && *entry != filename)
                        .collect();
                    if retained.len() != value.split(';').filter(|entry| !entry.is_empty()).count()
                    {
                        changed = true;
                        if !retained.is_empty() {
                            result.push_str(&format!("{key}={};\n", retained.join(";")));
                        }
                        continue;
                    }
                }
                result.push_str(line);
            }
            if changed {
                ensure!(
                    safe_file(&path)?.as_deref() == Some(bytes.as_slice()),
                    "User MIME file changed during unregister"
                );
                crate::recovery::atomic_write(&path, result.as_bytes())?;
            }
        }
    }
    Ok(())
}
