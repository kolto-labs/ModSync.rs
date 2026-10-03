//! Namespace ordering is capability-specific; Windows does not claim directory fsync.
use super::read::safe_path;
use anyhow::{Context, Result, ensure};
use std::{fs, path::Path};

pub(super) const UNIX_ORIGIN: &str = "rust";
pub(super) const WINDOWS_ORIGIN: &str = "rust-windows-process-interruption-v1";
pub(super) fn known_origin(origin: &str) -> bool {
    matches!(origin, UNIX_ORIGIN | WINDOWS_ORIGIN)
}
pub(super) fn writer_origin() -> Result<&'static str> {
    #[cfg(unix)]
    {
        Ok(UNIX_ORIGIN)
    }
    #[cfg(windows)]
    {
        Ok(WINDOWS_ORIGIN)
    }
    #[cfg(not(any(unix, windows)))]
    {
        anyhow::bail!("Checkpoint persistence is unsupported on this platform")
    }
}
pub(super) fn durability(origin: &str) -> Result<&'static str> {
    match origin {
        UNIX_ORIGIN => Ok("file-and-directory-fsync; native power-loss acceptance pending"),
        WINDOWS_ORIGIN => Ok(
            "windows-process-interruption-v1; flushed-file-data and same-volume namespace operations; no OS-crash or power-loss guarantee; native acceptance pending",
        ),
        _ => anyhow::bail!("Unknown checkpoint persistence capability"),
    }
}
pub(crate) fn namespace_boundary(path: &Path) -> Result<()> {
    writer_origin()?;
    ensure!(
        safe_path(path)?.is_some_and(|m| m.is_dir()),
        "Unsafe checkpoint namespace boundary"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
            .open(path)?
            .sync_all()
            .with_context(|| format!("Checkpoint directory fsync failed: {}", path.display()))?;
    }
    #[cfg(windows)]
    {
        windows::directory(path)?;
    }
    Ok(())
}
pub(crate) fn qualify(path: &Path) -> Result<()> {
    writer_origin()?;
    #[cfg(windows)]
    {
        windows::qualify(path)?;
    }
    #[cfg(not(windows))]
    let _ = path;
    Ok(())
}
/// Bind participant authority to canonical spelling, including missing suffixes.
pub(super) fn require_canonical_location(path: &Path) -> Result<()> {
    #[cfg(windows)]
    ensure!(
        windows::canonical_location(path)? == path,
        "Windows checkpoint participant must use its canonical path spelling"
    );
    #[cfg(not(windows))]
    let _ = path;
    Ok(())
}
/// The caller still owns the writable handle; flush before permissions/close.
pub(crate) fn flush_file(file: &fs::File) -> Result<()> {
    Ok(file.sync_all()?)
}

pub(crate) fn publish_temp(
    temporary: tempfile::NamedTempFile,
    path: &Path,
    replace: bool,
) -> Result<()> {
    flush_file(temporary.as_file())?;
    #[cfg(windows)]
    {
        let temporary = temporary.into_temp_path(); // close writer before namespace change
        rename(&temporary, path, replace)?;
        // TempPath drop only attempts its now-missing old name.
    }
    #[cfg(not(windows))]
    {
        if replace {
            temporary.persist(path).map_err(|e| e.error)?;
        } else {
            temporary.persist_noclobber(path).map_err(|e| e.error)?;
        }
    }
    Ok(())
}
pub(crate) fn rename(from: &Path, to: &Path, replace: bool) -> Result<()> {
    #[cfg(windows)]
    {
        windows::rename(from, to, replace)?;
    }
    #[cfg(not(windows))]
    {
        if !replace {
            ensure!(
                safe_path(to)?.is_none(),
                "Checkpoint destination already exists"
            );
        }
        fs::rename(from, to)?;
    }
    Ok(())
}
/// Only invoke after an intent authorizes mutation of this exact owned file.
pub(crate) fn writable_file(path: &Path) -> Result<()> {
    #[cfg(windows)]
    if let Some(metadata) = safe_path(path)? {
        ensure!(metadata.is_file(), "Owned writable target changed type");
        if metadata.permissions().readonly() {
            let mut permissions = metadata.permissions();
            // Windows-only: clears FILE_ATTRIBUTE_READONLY; no Unix mode bits exist here.
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            fs::set_permissions(path, permissions)?;
        }
    }
    #[cfg(not(windows))]
    let _ = path;
    Ok(())
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{
        io,
        os::windows::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
        },
        path::{Component, PathBuf, Prefix},
    };
    use windows_sys::Win32::Storage::FileSystem::*;
    fn wide(path: &Path) -> Result<Vec<u16>> {
        let units: Vec<u16> = path.as_os_str().encode_wide().collect();
        ensure!(
            !units.contains(&0) && units.len() < 32767,
            "Invalid Windows checkpoint path"
        );
        Ok(units.into_iter().chain(Some(0)).collect())
    }
    pub(super) fn directory(path: &Path) -> Result<()> {
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_dir() && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
            "Checkpoint namespace handle is not an ordinary directory"
        );
        Ok(())
    }
    fn existing(path: &Path) -> Result<PathBuf> {
        safe_path(path)?;
        for ancestor in path.ancestors() {
            match fs::symlink_metadata(ancestor) {
                Ok(_) => return Ok(fs::canonicalize(ancestor)?),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        anyhow::bail!("Checkpoint location has no existing ancestor")
    }
    pub(super) fn canonical_location(path: &Path) -> Result<PathBuf> {
        ensure!(
            path.is_absolute(),
            "Checkpoint participant must be absolute"
        );
        safe_path(path)?;
        let mut missing = Vec::new();
        let mut ancestor = path;
        loop {
            match fs::symlink_metadata(ancestor) {
                Ok(metadata) => {
                    ensure!(
                        missing.is_empty() || metadata.is_dir(),
                        "Participant creation ancestor is a file"
                    );
                    let mut canonical = fs::canonicalize(ancestor)?;
                    for suffix in missing.into_iter().rev() {
                        canonical.push(suffix);
                    }
                    return Ok(canonical);
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    missing.push(
                        ancestor
                            .file_name()
                            .context("Participant lacks existing root")?
                            .to_os_string(),
                    );
                    ancestor = ancestor
                        .parent()
                        .context("Participant lacks existing ancestor")?;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
    fn volume(path: &Path) -> Result<Vec<u16>> {
        let path = existing(path)?;
        ensure!(
            matches!(path.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))),
            "Checkpoint Windows profile requires a local disk path, not UNC/device/provider paths"
        );
        let mut root = vec![0u16; 32768];
        ensure!(
            unsafe {
                GetVolumePathNameW(wide(&path)?.as_ptr(), root.as_mut_ptr(), root.len() as u32)
            } != 0,
            "Cannot determine checkpoint volume: {}",
            io::Error::last_os_error()
        );
        let end = root
            .iter()
            .position(|v| *v == 0)
            .context("Unterminated volume path")?;
        root.truncate(end + 1);
        ensure!(
            unsafe { GetDriveTypeW(root.as_ptr()) } == 3, /* DRIVE_FIXED */
            "Checkpoint Windows profile requires a fixed local NTFS volume"
        );
        let mut filesystem = [0u16; 64];
        ensure!(
            unsafe {
                GetVolumeInformationW(
                    root.as_ptr(),
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    filesystem.as_mut_ptr(),
                    filesystem.len() as u32,
                )
            } != 0,
            "Cannot qualify checkpoint filesystem: {}",
            io::Error::last_os_error()
        );
        let end = filesystem
            .iter()
            .position(|v| *v == 0)
            .context("Unterminated filesystem name")?;
        ensure!(
            String::from_utf16(&filesystem[..end])? == "NTFS",
            "Checkpoint Windows profile only supports local NTFS"
        );
        Ok(root)
    }
    pub(super) fn qualify(path: &Path) -> Result<()> {
        ensure!(
            path.is_absolute(),
            "Checkpoint Windows location must be absolute"
        );
        volume(path)?;
        // Validate every existing creation ancestor, without creating anything.
        for ancestor in path.ancestors() {
            if let Some(metadata) = safe_path(ancestor)?
                && metadata.is_dir()
            {
                directory(ancestor)?;
            }
        }
        Ok(())
    }
    pub(super) fn rename(from: &Path, to: &Path, replace: bool) -> Result<()> {
        qualify(from)?;
        qualify(to)?;
        ensure!(
            volume(from)? == volume(to)?,
            "Checkpoint namespace move must remain on one volume"
        );
        let source = fs::canonicalize(from)?;
        let target = fs::canonicalize(to.parent().context("Publication needs parent")?)?
            .join(to.file_name().context("Publication needs filename")?);
        let flags = MOVEFILE_WRITE_THROUGH
            | if replace {
                MOVEFILE_REPLACE_EXISTING
            } else {
                0
            };
        if unsafe { MoveFileExW(wide(&source)?.as_ptr(), wide(&target)?.as_ptr(), flags) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        Ok(())
    }
}
