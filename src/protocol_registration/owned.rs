//! Explicit per-user ownership for protocol handlers. Status never registers.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::atomic::AtomicBool};
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "command.rs"]
mod command;
#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod linux;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod macos;
#[cfg(windows)]
#[path = "windows.rs"]
mod windows;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    Modsync,
    Nxm,
}
impl Scheme {
    pub fn name(self) -> &'static str {
        match self {
            Self::Modsync => "modsync",
            Self::Nxm => "nxm",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Request {
    pub executable: PathBuf,
    pub package_token: String,
    pub channel: String,
    pub schemes: Vec<Scheme>,
    pub replace: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    Absent,
    Owned,
    OtherModSync,
    Foreign,
    Unsupported,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemeReport {
    pub scheme: Scheme,
    pub state: State,
    pub handler: Option<String>,
    pub changed: bool,
    pub message: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrationReport {
    pub operation: &'static str,
    pub platform: &'static str,
    pub entries: Vec<SchemeReport>,
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Identity {
    package_token: String,
    channel: String,
    executable: String,
}
impl Request {
    fn identity(&self) -> Result<Identity> {
        for value in [&self.package_token, &self.channel] {
            ensure!(
                !value.is_empty()
                    && value.len() <= 128
                    && value
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
                "Package token/channel must be 1–128 ASCII letters, digits, dots, underscores or hyphens"
            );
        }
        ensure!(
            !self.schemes.is_empty()
                && self.schemes.len() <= 2
                && (self.schemes.len() != 2 || self.schemes[0] != self.schemes[1]),
            "Choose each protocol scheme at most once"
        );
        ensure!(
            self.executable
                .to_str()
                .is_none_or(|value| !value.chars().any(char::is_control)),
            "Protocol executable contains control characters"
        );
        let executable =
            std::fs::canonicalize(&self.executable).context("Resolve registration executable")?;
        crate::paths::open_regular_read(&executable)?;
        ensure!(
            executable
                .to_str()
                .is_none_or(|value| !value.chars().any(char::is_control)),
            "Resolved protocol executable contains control characters"
        );
        Ok(Identity {
            package_token: self.package_token.clone(),
            channel: self.channel.clone(),
            executable: executable
                .to_str()
                .context("Protocol executable is not UTF-8")?
                .into(),
        })
    }
}
pub fn status(request: &Request, cancelled: &AtomicBool) -> Result<RegistrationReport> {
    dispatch("status", request, cancelled, None)
}
pub fn register(request: &Request, cancelled: &AtomicBool) -> Result<RegistrationReport> {
    dispatch("register", request, cancelled, None)
}
pub fn unregister(request: &Request, cancelled: &AtomicBool) -> Result<RegistrationReport> {
    dispatch("unregister", request, cancelled, None)
}
/// Register only if every displayed scheme/state/handler still matches under
/// the platform mutation lock. External applications need not honor our lock;
/// this is a checked snapshot, not an atomic compare-and-swap with the OS.
/// Handler identity is the displayed string (desktop ID, app path, or Windows
/// handler choice/command), not a content fingerprint of the target's files or
/// registry values. Changes behind an unchanged displayed identity are not detected.
pub fn register_confirmed(
    request: &Request,
    displayed: &RegistrationReport,
    cancelled: &AtomicBool,
) -> Result<RegistrationReport> {
    dispatch("register", request, cancelled, Some(displayed))
}

fn check_confirmed(
    request: &Request,
    displayed: Option<&RegistrationReport>,
    platform: &str,
    current: &[SchemeReport],
) -> Result<()> {
    let Some(displayed) = displayed else {
        return Ok(());
    };
    ensure!(
        displayed.platform == platform
            && displayed.entries.len() == request.schemes.len()
            && current.len() == request.schemes.len(),
        "Protocol confirmation does not match the requested platform/schemes; refresh status"
    );
    for scheme in &request.schemes {
        let previous: Vec<_> = displayed
            .entries
            .iter()
            .filter(|entry| entry.scheme == *scheme)
            .collect();
        let observed: Vec<_> = current
            .iter()
            .filter(|entry| entry.scheme == *scheme)
            .collect();
        ensure!(
            previous.len() == 1 && observed.len() == 1,
            "Protocol confirmation contains missing or duplicate schemes; refresh status"
        );
        ensure!(
            previous[0].state == observed[0].state && previous[0].handler == observed[0].handler,
            "Protocol handler changed after confirmation; refresh status and confirm again"
        );
    }
    Ok(())
}

fn dispatch(
    operation: &'static str,
    request: &Request,
    cancelled: &AtomicBool,
    displayed: Option<&RegistrationReport>,
) -> Result<RegistrationReport> {
    let identity = request.identity()?;
    #[cfg(target_os = "linux")]
    {
        linux::run(operation, request, &identity, cancelled, displayed)
    }
    #[cfg(windows)]
    {
        windows::run(operation, request, &identity, cancelled, displayed)
    }
    #[cfg(target_os = "macos")]
    {
        macos::run(operation, request, &identity, cancelled, displayed)
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        let _ = (identity, cancelled);
        ensure!(
            displayed.is_none(),
            "Confirmed registration is unavailable on this platform"
        );
        Ok(RegistrationReport {
            operation,
            platform: std::env::consts::OS,
            entries: request
                .schemes
                .iter()
                .map(|scheme| SchemeReport {
                    scheme: *scheme,
                    state: State::Unsupported,
                    handler: None,
                    changed: false,
                    message: Some("Protocol registration is unavailable on this platform".into()),
                })
                .collect(),
            warnings: Vec::new(),
        })
    }
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn safe_file(path: &std::path::Path) -> Result<Option<Vec<u8>>> {
    use std::io::Read;
    match std::fs::symlink_metadata(path) {
        Ok(meta) => ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "Protocol artifact is not a regular file"
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    crate::paths::open_regular_read(path)?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 1024 * 1024, "Protocol artifact exceeds 1MiB");
    Ok(Some(bytes))
}

// Serialize cooperating installations before ownership comparisons. External OS
// tools do not honor this lock, so each backend also rechecks before removal.
#[cfg(any(windows, target_os = "macos"))]
fn mutation_lock(flag: &AtomicBool) -> Result<std::fs::File> {
    use std::{
        fs,
        sync::atomic::Ordering,
        time::{Duration, Instant},
    };
    let directory = dirs::data_local_dir()
        .context("Cannot locate per-user data")?
        .join("ModSync/protocol-registration");
    ensure!(
        directory.is_absolute(),
        "Protocol lock directory must be absolute"
    );
    for path in directory.ancestors().collect::<Vec<_>>().into_iter().rev() {
        match fs::symlink_metadata(path) {
            Ok(meta) => {
                ensure!(
                    meta.is_dir() && !meta.file_type().is_symlink(),
                    "Unsafe protocol lock directory"
                );
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    ensure!(
                        meta.file_attributes()
                            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
                            == 0,
                        "Protocol lock directory is a reparse point"
                    );
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(path)?,
            Err(error) => return Err(error.into()),
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            fs::metadata(&directory)?.uid() == unsafe { libc::geteuid() },
            "Protocol lock directory is not owned by current user"
        );
    }
    let path = directory.join(".lock");
    match fs::symlink_metadata(&path) {
        Ok(meta) => ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "Unsafe protocol lock"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
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
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    ensure!(metadata.is_file(), "Protocol lock is not regular");
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes()
                & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
                == 0,
            "Protocol lock is a reparse point"
        );
    }
    let started = Instant::now();
    loop {
        ensure!(
            !flag.load(Ordering::Acquire),
            "Protocol registration cancelled"
        );
        match fs2::FileExt::try_lock_exclusive(&file) {
            Ok(()) => return Ok(file),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                ensure!(
                    started.elapsed() < Duration::from_secs(15),
                    "Protocol registration lock timed out"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => return Err(error.into()),
        }
    }
}
