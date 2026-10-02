use super::Capability;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Identity {
    pub device: u64,
    pub inode: u64,
    pub owner: u32,
    pub group: u32,
}

pub(super) fn capability() -> Capability {
    Capability {
        supported: cfg!(target_os = "linux"),
        platform: std::env::consts::OS.into(),
        durability: "Linux file and directory fsync; native no-replace rename; power-loss and native-platform acceptance pending".into(),
        restrictions: vec![
            "ASCII names only for lowercase; Unicode mapping is not qualified against .NET".into(),
            "Current-owner ordinary objects only; no symlinks, hardlinked files, POSIX ACLs or foreign ownership".into(),
            "Readable files/searchable directories required before preview; no permission changes to inspect unreadable trees".into(),
            "File security capabilities are refused; SGID objects require membership in the preserved group".into(),
            "Windows ACL/identity-preserving readonly mutation and other Unix rename/ACL profiles are unsupported".into(),
            "No permission takeover, chmod 777, concurrent external editing or automatic forward resume".into(),
        ],
    }
}

pub(super) fn require_capability() -> Result<()> {
    ensure!(
        capability().supported,
        "Maintenance mutation is unsupported on this platform: ACL/identity and native no-replace rename profile is not qualified"
    );
    Ok(())
}

pub(super) fn identity(metadata: &fs::Metadata) -> Result<Identity> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(Identity {
            device: metadata.dev(),
            inode: metadata.ino(),
            owner: metadata.uid(),
            group: metadata.gid(),
        })
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        anyhow::bail!("Stable maintenance object identity is unsupported on this platform")
    }
}

pub(super) fn mode(metadata: &fs::Metadata) -> Result<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Ok(metadata.permissions().mode() & 0o7777)
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        anyhow::bail!("Maintenance permission profile is unsupported")
    }
}

pub(super) fn qualify(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    require_capability()?;
    ensure!(
        !metadata.file_type().is_symlink() && (metadata.is_file() || metadata.is_dir()),
        "Maintenance refuses links and special files: {}",
        path.display()
    );
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
        ensure!(
            metadata.uid() == unsafe { libc::geteuid() },
            "Maintenance refuses foreign ownership: {}",
            path.display()
        );
        ensure!(
            !metadata.is_file() || metadata.nlink() == 1,
            "Maintenance refuses hardlinked files: {}",
            path.display()
        );
        if mode(metadata)? & 0o2000 != 0 && metadata.gid() != unsafe { libc::getegid() } {
            let count = unsafe { libc::getgroups(0, std::ptr::null_mut()) };
            ensure!(
                (0..=65536).contains(&count),
                "Could not qualify supplementary groups for SGID preservation"
            );
            let mut groups = vec![0 as libc::gid_t; count as usize];
            let actual = unsafe { libc::getgroups(count, groups.as_mut_ptr()) };
            ensure!(
                actual == count && groups.contains(&metadata.gid()),
                "Maintenance cannot preserve SGID for a foreign group"
            );
        }
        let path_bytes = std::ffi::CString::new(path.as_os_str().as_bytes())?;
        let size = unsafe { libc::llistxattr(path_bytes.as_ptr(), std::ptr::null_mut(), 0) };
        if size < 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ENOTSUP) {
                return Err(error).context("Could not qualify maintenance ACL state");
            }
        } else {
            ensure!(
                size <= 1024 * 1024,
                "Maintenance xattr inventory exceeds bound"
            );
            let mut names = vec![0u8; size as usize];
            if size > 0 {
                let actual = unsafe {
                    libc::llistxattr(path_bytes.as_ptr(), names.as_mut_ptr().cast(), names.len())
                };
                ensure!(
                    actual >= 0 && actual as usize == names.len(),
                    "Maintenance ACL inventory changed"
                );
                ensure!(
                    !names.split(|b| *b == 0).any(|name| matches!(
                        name,
                        b"system.posix_acl_access"
                            | b"system.posix_acl_default"
                            | b"security.NTACL"
                            | b"security.capability"
                    )),
                    "Maintenance refuses extended ACLs/security capabilities: {}",
                    path.display()
                );
            }
        }
    }
    Ok(())
}

/// Kernel no-replace operation, including directories. A preflight existence
/// check is not a substitute for this atomic namespace guarantee.
pub(super) fn rename(from: &Path, to: &Path, parent_identity: &Identity) -> Result<()> {
    require_capability()?;
    #[cfg(target_os = "linux")]
    {
        use std::os::{
            fd::AsRawFd,
            unix::{ffi::OsStrExt, fs::OpenOptionsExt},
        };
        let parent = from.parent().context("Maintenance rename lacks a parent")?;
        ensure!(
            Some(parent) == to.parent(),
            "Maintenance rename parents differ"
        );
        crate::checkpoint::safe_path(parent)?.context("Maintenance rename parent disappeared")?;
        let directory = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
            .open(parent)?;
        ensure!(
            identity(&directory.metadata()?)? == *parent_identity,
            "Maintenance pinned namespace parent identity changed"
        );
        let from = std::ffi::CString::new(
            from.file_name()
                .context("Maintenance source name missing")?
                .as_bytes(),
        )?;
        let to = std::ffi::CString::new(
            to.file_name()
                .context("Maintenance target name missing")?
                .as_bytes(),
        )?;
        let result = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                directory.as_raw_fd(),
                from.as_ptr(),
                directory.as_raw_fd(),
                to.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        ensure!(
            result == 0,
            "Native no-replace maintenance rename failed: {}",
            std::io::Error::last_os_error()
        );
        directory.sync_all()?;
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (from, to, parent_identity);
        anyhow::bail!("Native no-replace maintenance rename is unsupported")
    }
}

/// Publish a private flushed intent atomically; use native no-replace for a new
/// intent and replacement only for its verified owned terminal header.
pub(super) fn publish(from: &Path, to: &Path, replace: bool) -> Result<()> {
    require_capability()?;
    #[cfg(target_os = "linux")]
    {
        use std::os::{
            fd::AsRawFd,
            unix::{ffi::OsStrExt, fs::OpenOptionsExt},
        };
        let open_parent = |path: &Path| -> Result<fs::File> {
            let metadata = crate::checkpoint::safe_path(path)?
                .context("Maintenance publication parent is missing")?;
            qualify(path, &metadata)?;
            let file = fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
                .open(path)?;
            ensure!(
                identity(&file.metadata()?)? == identity(&metadata)?,
                "Maintenance publication parent identity changed"
            );
            Ok(file)
        };
        let source_parent = open_parent(
            from.parent()
                .context("Maintenance private intent needs a parent")?,
        )?;
        let target_parent = open_parent(to.parent().context("Maintenance intent needs a parent")?)?;
        ensure!(
            identity(&source_parent.metadata()?)?.device
                == identity(&target_parent.metadata()?)?.device,
            "Maintenance intent publication must stay on one filesystem"
        );
        let from = std::ffi::CString::new(
            from.file_name()
                .context("Maintenance private intent name missing")?
                .as_bytes(),
        )?;
        let to = std::ffi::CString::new(
            to.file_name()
                .context("Maintenance intent name missing")?
                .as_bytes(),
        )?;
        let result = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                source_parent.as_raw_fd(),
                from.as_ptr(),
                target_parent.as_raw_fd(),
                to.as_ptr(),
                if replace { 0 } else { libc::RENAME_NOREPLACE },
            )
        };
        ensure!(
            result == 0,
            "Maintenance atomic intent publication failed: {}",
            std::io::Error::last_os_error()
        );
        target_parent.sync_all()?;
        source_parent.sync_all()?;
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (from, to, replace);
        anyhow::bail!("Maintenance atomic intent publication is unsupported")
    }
}

pub(super) fn chmod(path: &Path, desired: u32, expected: &Identity) -> Result<()> {
    require_capability()?;
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let metadata =
            crate::checkpoint::safe_path(path)?.context("Maintenance chmod object disappeared")?;
        qualify(path, &metadata)?;
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(
                libc::O_NOFOLLOW
                    | libc::O_NONBLOCK
                    | if metadata.is_dir() {
                        libc::O_DIRECTORY
                    } else {
                        0
                    },
            )
            .open(path)?;
        ensure!(
            identity(&file.metadata()?)? == *expected,
            "Maintenance chmod object identity changed"
        );
        file.set_permissions(fs::Permissions::from_mode(desired))?;
        file.sync_all()?;
        ensure!(
            mode(&file.metadata()?)? == desired,
            "Maintenance chmod did not retain all requested mode bits"
        );
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (path, desired, expected);
        anyhow::bail!("Owner-writable maintenance is unsupported on this platform")
    }
}
