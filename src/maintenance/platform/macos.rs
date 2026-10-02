//! Darwin native maintenance source profile. The recovery design covers process
//! interruption only; native execution/interruption, OS crash and power-loss
//! acceptance have not been established.
//! SDK contracts:
//! https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/fcntl.h
//! https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/stat.h
//! https://github.com/apple-oss-distributions/Libc/blob/main/gen/filesec.c
//! https://github.com/apple-oss-distributions/Libc/blob/main/sys/statx_np.c
use super::{Identity, MACOS_ALLOWED_FLAGS, identity, mode, qualify_unix_owner};
use anyhow::{Context, Result, ensure};
use std::{
    ffi::{CString, c_void},
    fs,
    os::{
        fd::AsRawFd,
        unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    },
    path::Path,
    ptr::NonNull,
};

// Apple bsd/sys/fcntl.h defines filesec_property_t::FILESEC_ACL = 5:
// https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/fcntl.h
// This is not the ACL_TYPE_EXTENDED selector defined by sys/acl.h.
const FILESEC_ACL: libc::c_int = 5;
unsafe extern "C" {
    fn filesec_init() -> *mut c_void;
    fn filesec_free(filesec: *mut c_void);
    fn filesec_query_property(
        filesec: *mut c_void,
        property: libc::c_int,
        present: *mut libc::c_int,
    ) -> libc::c_int;
    // sys/stat.h declares fstatx_np with __DARWIN_INODE64(fstatx_np).
    // sys/cdefs.h selects $INODE64 on x86 and no suffix on 64-bit-only ARM:
    // https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/cdefs.h
    // This matches libc::fstat's alias and libc::stat's native layout.
    #[cfg_attr(not(target_arch = "aarch64"), link_name = "fstatx_np$INODE64")]
    fn fstatx_np(fd: libc::c_int, stat: *mut libc::stat, filesec: *mut c_void) -> libc::c_int;
}

struct FileSecurity(NonNull<c_void>);
impl Drop for FileSecurity {
    fn drop(&mut self) {
        // The native object is exclusively owned and released exactly once.
        unsafe { filesec_free(self.0.as_ptr()) };
    }
}

/// Native successful ACL absence is mandatory; query failures never mean no ACL.
fn qualify_pinned(file: &fs::File, path: &Path) -> Result<()> {
    let before = file.metadata()?;
    qualify_unix_owner(path, &before)?;
    let expected = identity(&before)?;
    ensure!(
        expected
            .native_flags
            .is_some_and(|flags| flags & !MACOS_ALLOWED_FLAGS == 0),
        "Maintenance refuses unsupported Darwin native flags: {}",
        path.display()
    );
    let security = FileSecurity(
        NonNull::new(unsafe { filesec_init() })
            .context("Could not allocate native maintenance security query")?,
    );
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    ensure!(
        unsafe { fstatx_np(file.as_raw_fd(), stat.as_mut_ptr(), security.0.as_ptr()) } == 0,
        "Could not qualify native maintenance ACL state: {}",
        std::io::Error::last_os_error()
    );
    // Native stat is initialized only on success. The ACL and metadata describe
    // this pinned handle, and the expected ordinary object is checked again.
    let stat = unsafe { stat.assume_init() };
    let observed = Identity {
        device: stat.st_dev as u64,
        inode: stat.st_ino,
        owner: stat.st_uid,
        group: stat.st_gid,
        native_flags: Some(stat.st_flags),
        windows: None,
    };
    ensure!(
        observed == expected && u32::from(stat.st_mode) & 0o7777 == mode(&before)?,
        "Maintenance pinned native metadata changed"
    );
    let mut present = 0;
    ensure!(
        unsafe { filesec_query_property(security.0.as_ptr(), FILESEC_ACL, &mut present) } == 0,
        "Could not query native maintenance ACL presence: {}",
        std::io::Error::last_os_error()
    );
    ensure!(
        present == 0,
        "Maintenance refuses native extended ACLs: {}",
        path.display()
    );
    let after = file.metadata()?;
    qualify_unix_owner(path, &after)?;
    ensure!(
        identity(&after)? == expected
            && mode(&after)? == mode(&before)?
            && after.is_file() == before.is_file()
            && after.is_dir() == before.is_dir(),
        "Maintenance pinned ACL/metadata qualification changed"
    );
    Ok(())
}

fn open_checked(path: &Path, expected: &fs::Metadata) -> Result<fs::File> {
    ensure!(
        !expected.file_type().is_symlink() && (expected.is_file() || expected.is_dir()),
        "Maintenance refuses links and special files: {}",
        path.display()
    );
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(
            libc::O_NOFOLLOW
                | libc::O_NONBLOCK
                | if expected.is_dir() {
                    libc::O_DIRECTORY
                } else {
                    0
                },
        )
        .open(path)?;
    let actual = file.metadata()?;
    ensure!(
        identity(&actual)? == identity(expected)?
            && mode(&actual)? == mode(expected)?
            && actual.is_file() == expected.is_file()
            && actual.is_dir() == expected.is_dir(),
        "Maintenance pinned object differs from inspected path"
    );
    qualify_pinned(&file, path)?;
    Ok(file)
}

pub(super) fn qualify(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    let _file = open_checked(path, metadata)?;
    Ok(())
}

fn open_parent(path: &Path) -> Result<fs::File> {
    let metadata =
        crate::checkpoint::safe_path(path)?.context("Maintenance namespace parent disappeared")?;
    ensure!(
        metadata.is_dir(),
        "Maintenance namespace parent is not a directory"
    );
    open_checked(path, &metadata)
}

fn basename(path: &Path) -> Result<CString> {
    Ok(CString::new(
        path.file_name()
            .context("Maintenance native name is missing")?
            .as_bytes(),
    )?)
}

/// Descriptor-relative RENAME_EXCL is the native exclusion guarantee for every
/// forward and rollback rename leg. No overwrite/copy fallback is permitted.
pub(super) fn rename(from: &Path, to: &Path, parent_identity: &Identity) -> Result<()> {
    let parent = from.parent().context("Maintenance rename lacks a parent")?;
    ensure!(
        Some(parent) == to.parent(),
        "Maintenance rename parents differ"
    );
    let directory = open_parent(parent)?;
    ensure!(
        identity(&directory.metadata()?)? == *parent_identity,
        "Maintenance pinned namespace parent identity changed"
    );
    let from = basename(from)?;
    let to = basename(to)?;
    ensure!(
        unsafe {
            libc::renameatx_np(
                directory.as_raw_fd(),
                from.as_ptr(),
                directory.as_raw_fd(),
                to.as_ptr(),
                libc::RENAME_EXCL,
            )
        } == 0,
        "Native exclusive maintenance rename failed: {}",
        std::io::Error::last_os_error()
    );
    directory.sync_all()?;
    ensure!(
        identity(&directory.metadata()?)? == *parent_identity,
        "Maintenance namespace parent identity changed after rename"
    );
    Ok(())
}

/// Replacement is used only by the core's verified owned terminal header path.
/// New immutable intents use RENAME_EXCL, and publication stays on one device.
pub(super) fn publish(from: &Path, to: &Path, replace: bool) -> Result<()> {
    let source_parent = open_parent(from.parent().context("Private intent needs a parent")?)?;
    let target_parent = open_parent(to.parent().context("Maintenance intent needs a parent")?)?;
    let source_identity = identity(&source_parent.metadata()?)?;
    let target_identity = identity(&target_parent.metadata()?)?;
    ensure!(
        source_identity.device == target_identity.device,
        "Maintenance intent publication must stay on one filesystem"
    );
    let from = basename(from)?;
    let to = basename(to)?;
    ensure!(
        unsafe {
            libc::renameatx_np(
                source_parent.as_raw_fd(),
                from.as_ptr(),
                target_parent.as_raw_fd(),
                to.as_ptr(),
                if replace { 0 } else { libc::RENAME_EXCL },
            )
        } == 0,
        "Maintenance atomic intent publication failed: {}",
        std::io::Error::last_os_error()
    );
    target_parent.sync_all()?;
    source_parent.sync_all()?;
    ensure!(
        identity(&source_parent.metadata()?)? == source_identity
            && identity(&target_parent.metadata()?)? == target_identity,
        "Maintenance publication parent identity changed"
    );
    Ok(())
}

pub(super) fn chmod(path: &Path, desired: u32, expected: &Identity) -> Result<()> {
    ensure!(
        desired <= 0o7777,
        "Maintenance requested native mode is malformed"
    );
    let metadata =
        crate::checkpoint::safe_path(path)?.context("Maintenance chmod object disappeared")?;
    let file = open_checked(path, &metadata)?;
    ensure!(
        identity(&file.metadata()?)? == *expected,
        "Maintenance chmod object identity changed"
    );
    ensure!(
        unsafe { libc::fchmod(file.as_raw_fd(), desired as libc::mode_t) } == 0,
        "Native maintenance mode repair failed: {}",
        std::io::Error::last_os_error()
    );
    file.sync_all()?;
    qualify_pinned(&file, path)?;
    let observed = file.metadata()?;
    ensure!(
        identity(&observed)? == *expected && mode(&observed)? == desired,
        "Maintenance chmod did not retain all requested mode bits/native flags"
    );
    Ok(())
}
