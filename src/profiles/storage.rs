//! Bounded private profile/settings persistence. Existing ancestors are validated,
//! never chmod'd; new directories and replacement files receive private permissions.
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path},
    time::{Duration, Instant},
};
#[cfg(windows)]
#[path = "storage_windows.rs"]
mod windows;
pub(super) const LIMIT: usize = 16 * 1024 * 1024;
fn ordinary(meta: &fs::Metadata, directory: bool) -> Result<()> {
    ensure!(
        !meta.file_type().is_symlink()
            && if directory {
                meta.is_dir()
            } else {
                meta.is_file()
            },
        "Settings path is not an ordinary {}",
        if directory { "directory" } else { "file" }
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            meta.file_attributes()
                & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
                == 0,
            "Settings path is a reparse point"
        );
    }
    Ok(())
}
fn owner(path: &Path, meta: &fs::Metadata) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            meta.uid() == unsafe { libc::geteuid() },
            "Settings path is not owned by current user"
        );
        let _ = path;
    }
    #[cfg(windows)]
    {
        let _ = meta;
        windows::check_owner(path)?;
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (path, meta);
        anyhow::bail!("Private settings ownership unavailable on this platform");
    }
    Ok(())
}
pub(crate) fn directory(path: &Path, create: bool) -> Result<()> {
    let path = std::path::absolute(path)?;
    ensure!(
        !path.components().any(|v| matches!(v, Component::ParentDir)),
        "Settings directory cannot contain parent traversal"
    );
    for ancestor in path.ancestors().collect::<Vec<_>>().into_iter().rev() {
        match fs::symlink_metadata(ancestor) {
            Ok(meta) => ordinary(&meta, true)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if create {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::DirBuilderExt;
                        fs::DirBuilder::new().mode(0o700).create(ancestor)?;
                    }
                    #[cfg(windows)]
                    windows::create_directory(ancestor)?;
                    #[cfg(not(any(unix, windows)))]
                    anyhow::bail!("Private settings directory creation unavailable");
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    match fs::symlink_metadata(&path) {
        Ok(meta) => {
            owner(&path, &meta)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                ensure!(
                    meta.permissions().mode() & 0o022 == 0,
                    "Settings directory is writable by another user; select a private directory"
                );
            }
        }
        Err(error) if !create && error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
pub(crate) fn exists(path: &Path) -> Result<bool> {
    directory(path.parent().context("Settings path has no parent")?, false)?;
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            ordinary(&meta, false)?;
            owner(path, &meta)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
pub(super) fn read(path: &Path) -> Result<Option<Vec<u8>>> {
    read_bounded(path, LIMIT)
}
pub(crate) fn read_bounded(path: &Path, limit: usize) -> Result<Option<Vec<u8>>> {
    ensure!(limit <= LIMIT, "Private file read exceeds supported bound");
    if !exists(path)? {
        return Ok(None);
    }
    let mut file = crate::paths::open_regular_read(path)?;
    let meta = file.metadata()?;
    ordinary(&meta, false)?;
    owner(path, &meta)?;
    #[cfg(windows)]
    windows::check_file(&file)?;
    ensure!(
        meta.len() <= limit as u64,
        "Private file exceeds configured size limit"
    );
    let mut bytes = Vec::new();
    (&mut file).take(limit as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= limit,
        "Private file exceeds configured size limit"
    );
    Ok(Some(bytes))
}
fn create_file(path: &Path) -> Result<File> {
    #[cfg(windows)]
    {
        windows::create_file(path)
    }
    #[cfg(not(windows))]
    {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        Ok(options.open(path)?)
    }
}
pub(crate) fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() <= LIMIT,
        "Settings/profile exceeds 16 MiB limit"
    );
    let parent = path.parent().context("Settings path has no parent")?;
    directory(parent, true)?;
    exists(path)?;
    let temporary = parent.join(format!(".modsync-settings-{}.tmp", uuid::Uuid::new_v4()));
    let mut created = false;
    let result = (|| {
        let mut file = create_file(&temporary)?;
        created = true;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        // Keep replacement private without altering a pre-existing permissive directory.
        exists(path)?;
        #[cfg(windows)]
        windows::replace(&temporary, path)?;
        #[cfg(not(windows))]
        fs::rename(&temporary, path)?;
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() && created {
        let _ = fs::remove_file(&temporary);
    }
    result
}
pub(crate) fn lock(directory_path: &Path) -> Result<File> {
    directory(directory_path, true)?;
    let path = directory_path.join(".lock");
    let file = match create_file(&path) {
        Ok(file) => file,
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|v| v.kind() == std::io::ErrorKind::AlreadyExists) =>
        {
            ensure!(exists(&path)?, "Settings lock disappeared");
            let mut options = OpenOptions::new();
            options.read(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options.custom_flags(
                    windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
                );
            }
            options.open(&path)?
        }
        Err(error) => return Err(error),
    };
    let meta = file.metadata()?;
    ordinary(&meta, false)?;
    owner(&path, &meta)?;
    #[cfg(windows)]
    windows::check_file(&file)?;
    let started = Instant::now();
    loop {
        match fs2::FileExt::try_lock_exclusive(&file) {
            Ok(()) => return Ok(file),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                ensure!(
                    started.elapsed() < Duration::from_secs(15),
                    "Settings lock timed out"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => return Err(error.into()),
        }
    }
}
