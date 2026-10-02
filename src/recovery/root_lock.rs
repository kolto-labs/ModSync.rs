//! One-root coordination shared by acquisition and paired recovery locks.
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    path::{Component, Path, PathBuf},
};

fn identity(file: &File) -> Result<(u64, u64)> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata()?;
        Ok((metadata.dev(), metadata.ino()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        ensure!(
            unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) } != 0,
            "Could not read root-lock object identity: {}",
            std::io::Error::last_os_error()
        );
        Ok((
            u64::from(information.dwVolumeSerialNumber),
            (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
        ))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = file;
        anyhow::bail!("Root-lock identity is unsupported on this platform")
    }
}

fn directory(path: &Path) -> Result<File> {
    ensure!(
        crate::checkpoint::safe_path(path)?.is_some_and(|metadata| metadata.is_dir()),
        "Acquisition root must be an existing ordinary directory"
    );
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        };
        options.custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    ensure!(
        file.metadata()?.is_dir(),
        "Root-lock handle is not a directory"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            file.metadata()?.file_attributes()
                & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
                == 0,
            "Root-lock handle is a reparse point"
        );
    }
    Ok(file)
}

pub(super) fn open(root: &Path) -> Result<File> {
    let original = directory(root)?;
    let hash = format!("{:x}", Sha256::digest(root.to_string_lossy().as_bytes()));
    let path = root
        .parent()
        .context("A filesystem root cannot be locked")?
        .join(format!(".modsync-lock-{}", &hash[..16]));
    if let Some(metadata) = crate::checkpoint::safe_path(&path)? {
        ensure!(metadata.is_file(), "Lock path is not a regular file");
    }
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
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
    let file = options.open(&path)?;
    ensure!(
        file.metadata()?.is_file(),
        "Lock handle is not a regular file"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            file.metadata()?.file_attributes()
                & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
                == 0,
            "Lock handle is a reparse point"
        );
    }
    file.try_lock_exclusive()
        .context("Another ModSync process is using this game or mod directory")?;
    let current = crate::paths::open_regular_read(&path)?;
    ensure!(
        identity(&file)? == identity(&current)?,
        "Lock pathname was replaced during acquisition"
    );
    ensure!(
        identity(&original)? == identity(&directory(root)?)?,
        "Root directory was replaced during lock acquisition"
    );
    Ok(file)
}

/// Exclusive coordination with the mod half of the normal install/recovery lock.
/// Acquisition creates only a sibling lock file, never a fabricated game root.
pub struct ModRootLock {
    root: PathBuf,
    directory: File,
    _lock: File,
}
impl ModRootLock {
    pub fn acquire(root: &Path) -> Result<Self> {
        Self::acquire_impl(root, false)
    }
    pub(crate) fn acquire_maintenance(root: &Path) -> Result<Self> {
        Self::acquire_impl(root, true)
    }
    fn acquire_impl(root: &Path, maintenance: bool) -> Result<Self> {
        ensure!(
            !root.as_os_str().is_empty()
                && !root
                    .components()
                    .any(|part| matches!(part, Component::ParentDir)),
            "Mod root is empty or contains parent traversal"
        );
        let absolute = std::path::absolute(root)?;
        let observed = directory(&absolute)?;
        let root = fs::canonicalize(&absolute)?;
        ensure!(
            root.parent().is_some() && root.to_str().is_some(),
            "Mod root must be a non-root UTF-8 directory"
        );
        let captured = directory(&root)?;
        ensure!(
            identity(&observed)? == identity(&captured)?,
            "Mod root changed during canonicalization"
        );
        if !maintenance {
            crate::maintenance::ensure_clear_mods(&root)?;
        }
        let lock = open(&root)?;
        ensure!(
            identity(&captured)? == identity(&directory(&root)?)?,
            "Mod root changed during acquisition"
        );
        if !maintenance {
            crate::maintenance::ensure_clear_mods(&root)?;
        }
        Ok(Self {
            root,
            directory: captured,
            _lock: lock,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub(crate) fn guards(&self, root: &Path) -> bool {
        self.root == root
    }
    /// Verify the canonical root and ordinary existing ancestors; the final
    /// destination may be absent. Does not create or publish anything.
    pub fn contains_destination(&self, path: &Path) -> Result<()> {
        ensure!(
            identity(&self.directory)? == identity(&directory(&self.root)?)?,
            "Locked mod root was replaced"
        );
        let path = std::path::absolute(path)?;
        let relative = path
            .strip_prefix(&self.root)
            .context("Destination is outside the locked mod root")?;
        ensure!(
            !relative.as_os_str().is_empty()
                && relative
                    .components()
                    .all(|part| matches!(part, Component::Normal(_))),
            "Destination must be strictly beneath the locked mod root"
        );
        crate::checkpoint::safe_path(&path)?;
        Ok(())
    }
}
