//! Local NTFS source contract, with process-interruption acceptance still pending.
//! Native contracts: Microsoft FILE_ID_INFO, GetSecurityInfo, AccessCheck,
//! FILE_BASIC_INFO and FILE_RENAME_INFO. Tree DACLs are never modified.
use super::{Identity, WINDOWS_IMMUTABLE_ATTRIBUTES, WindowsIdentity};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io,
    mem::{offset_of, size_of, size_of_val},
    os::windows::{
        ffi::OsStrExt,
        fs::MetadataExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    ptr,
};
use windows_sys::Win32::{
    Foundation::{
        ERROR_HANDLE_EOF, ERROR_NO_TOKEN, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE,
        LocalFree,
    },
    Security::{
        Authorization::{GetSecurityInfo, SE_FILE_OBJECT},
        *,
    },
    Storage::FileSystem::*,
    System::Threading::{GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken},
};

fn check(ok: i32, message: &str) -> Result<()> {
    ensure!(ok != 0, "{message}: {}", io::Error::last_os_error());
    Ok(())
}

fn wide(path: &Path) -> Result<Vec<u16>> {
    let units: Vec<_> = path.as_os_str().encode_wide().collect();
    ensure!(
        !units.contains(&0) && units.len() < 32767,
        "Invalid Windows maintenance path"
    );
    Ok(units.into_iter().chain(Some(0)).collect())
}

fn open(path: &Path, access: u32) -> Result<File> {
    crate::checkpoint::persistence::qualify(path)?;
    let metadata =
        crate::checkpoint::safe_path(path)?.context("Windows maintenance object disappeared")?;
    ensure!(
        metadata.is_file() || metadata.is_dir(),
        "Windows maintenance refuses special objects"
    );
    let handle = unsafe {
        CreateFileW(
            wide(path)?.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT
                | if metadata.is_dir() {
                    FILE_FLAG_BACKUP_SEMANTICS
                } else {
                    0
                },
            ptr::null_mut(),
        )
    };
    ensure!(
        handle != INVALID_HANDLE_VALUE,
        "Could not open ordinary Windows maintenance object: {}",
        io::Error::last_os_error()
    );
    let file = unsafe { File::from_raw_handle(handle) };
    let opened = file.metadata()?;
    ensure!(
        opened.is_dir() == metadata.is_dir()
            && opened.file_attributes() == metadata.file_attributes()
            && opened.len() == metadata.len(),
        "Windows maintenance object changed during open"
    );
    Ok(file)
}

fn query<T: Default>(file: &File, class: FILE_INFO_BY_HANDLE_CLASS) -> Result<T> {
    let mut value = T::default();
    check(
        unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle(),
                class,
                (&mut value as *mut T).cast(),
                size_of::<T>() as u32,
            )
        },
        "Could not inspect Windows maintenance handle",
    )?;
    Ok(value)
}

fn open_parent(path: &Path) -> Result<File> {
    let file = open(
        path,
        READ_CONTROL
            | FILE_READ_ATTRIBUTES
            | FILE_LIST_DIRECTORY
            | FILE_ADD_FILE
            | FILE_ADD_SUBDIRECTORY
            | FILE_TRAVERSE,
    )?;
    ensure!(
        file.metadata()?.is_dir(),
        "Windows maintenance parent is not a directory"
    );
    Ok(file)
}

fn token() -> Result<OwnedHandle> {
    let mut handle = ptr::null_mut();
    if unsafe {
        OpenThreadToken(
            GetCurrentThread(),
            TOKEN_QUERY | TOKEN_DUPLICATE,
            1,
            &mut handle,
        )
    } == 0
    {
        ensure!(
            io::Error::last_os_error().raw_os_error() == Some(ERROR_NO_TOKEN as i32),
            "Could not inspect effective Windows maintenance token"
        );
        check(
            unsafe {
                OpenProcessToken(
                    GetCurrentProcess(),
                    TOKEN_QUERY | TOKEN_DUPLICATE,
                    &mut handle,
                )
            },
            "Could not inspect Windows process token",
        )?;
    }
    let original = unsafe { OwnedHandle::from_raw_handle(handle) };
    let mut duplicate = ptr::null_mut();
    check(
        unsafe {
            DuplicateToken(
                original.as_raw_handle(),
                SecurityImpersonation,
                &mut duplicate,
            )
        },
        "Could not duplicate effective Windows token for access qualification",
    )?;
    Ok(unsafe { OwnedHandle::from_raw_handle(duplicate) })
}

fn sid_bytes(sid: PSID) -> Result<Vec<u8>> {
    ensure!(
        !sid.is_null() && unsafe { IsValidSid(sid) } != 0,
        "Malformed native owner/group SID"
    );
    let size = unsafe { GetLengthSid(sid) } as usize;
    ensure!(
        (8..=68).contains(&size),
        "Native SID exceeds maintenance bound"
    );
    Ok(unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), size) }.to_vec())
}

fn token_sid(token: &OwnedHandle) -> Result<Vec<u8>> {
    let mut required = 0;
    unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            ptr::null_mut(),
            0,
            &mut required,
        )
    };
    ensure!(
        required >= size_of::<TOKEN_USER>() as u32 && required <= 65536,
        "Windows token SID inventory exceeds bound"
    );
    let mut buffer = vec![0usize; (required as usize).div_ceil(size_of::<usize>())];
    let capacity = (buffer.len() * size_of::<usize>()) as u32;
    check(
        unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                TokenUser,
                buffer.as_mut_ptr().cast(),
                capacity,
                &mut required,
            )
        },
        "Could not obtain effective Windows user SID",
    )?;
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    sid_bytes(user.User.Sid)
}

struct SecurityState {
    descriptor: PSECURITY_DESCRIPTOR,
    owner: Vec<u8>,
    fingerprint: String,
    dacl: *mut ACL,
    control: u16,
}
impl Drop for SecurityState {
    fn drop(&mut self) {
        unsafe { LocalFree(self.descriptor) };
    }
}

fn security(file: &File) -> Result<SecurityState> {
    let mut owner = ptr::null_mut();
    let mut group = ptr::null_mut();
    let mut dacl = ptr::null_mut();
    let mut descriptor = ptr::null_mut();
    let error = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            &mut group,
            &mut dacl,
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    ensure!(
        error == 0 && !descriptor.is_null(),
        "Could not capture Windows owner/group/DACL; security repair is unsupported (error {error})"
    );
    let mut state = SecurityState {
        descriptor,
        owner: Vec::new(),
        fingerprint: String::new(),
        dacl,
        control: 0,
    };
    ensure!(
        unsafe { IsValidSecurityDescriptor(descriptor) } != 0
            && unsafe { GetSecurityDescriptorLength(descriptor) } <= 131072,
        "Invalid or oversized Windows security descriptor"
    );
    state.owner = sid_bytes(owner)?;
    let group = sid_bytes(group)?;
    let mut revision = 0;
    check(
        unsafe { GetSecurityDescriptorControl(descriptor, &mut state.control, &mut revision) },
        "Could not inspect DACL control state",
    )?;
    let mut hash = Sha256::new();
    hash.update(b"modsync-windows-owner-group-dacl-v1\0");
    for sid in [&state.owner, &group] {
        hash.update((sid.len() as u32).to_le_bytes());
        hash.update(sid);
    }
    // Owner/group defaulting and DACL flags only; SACL/format state is not claimed.
    hash.update((state.control & 0x150f).to_le_bytes());
    if dacl.is_null() {
        hash.update([0]);
    } else {
        ensure!(unsafe { IsValidAcl(dacl) } != 0, "Malformed Windows DACL");
        let acl = unsafe { &*dacl };
        ensure!(
            (size_of::<ACL>()..=65535).contains(&(acl.AclSize as usize)),
            "Windows DACL exceeds maintenance bound"
        );
        hash.update([1, acl.AclRevision]);
        hash.update(acl.AceCount.to_le_bytes());
        let start = dacl as usize;
        let end = start
            .checked_add(acl.AclSize as usize)
            .context("DACL address overflow")?;
        for index in 0..u32::from(acl.AceCount) {
            let mut ace = ptr::null_mut();
            check(
                unsafe { GetAce(dacl, index, &mut ace) },
                "Could not enumerate Windows DACL",
            )?;
            ensure!(
                (ace as usize) >= start
                    && (ace as usize)
                        .checked_add(size_of::<ACE_HEADER>())
                        .is_some_and(|n| n <= end),
                "DACL ACE lies outside native allocation"
            );
            let header = unsafe { &*ace.cast::<ACE_HEADER>() };
            ensure!(
                header.AceSize as usize >= size_of::<ACCESS_ALLOWED_ACE>()
                    && (ace as usize)
                        .checked_add(header.AceSize as usize)
                        .is_some_and(|n| n <= end),
                "Malformed DACL ACE size"
            );
            // ACCESS_ALLOWED_ACE_TYPE=0 / ACCESS_DENIED_ACE_TYPE=1, winnt.h.
            ensure!(
                header.AceType <= 1,
                "Object/conditional/custom DACL entries are unsupported; no ACL takeover will be attempted"
            );
            hash.update(header.AceSize.to_le_bytes());
            hash.update(unsafe {
                std::slice::from_raw_parts(ace.cast::<u8>(), header.AceSize as usize)
            });
        }
    }
    state.fingerprint = format!("{:x}", hash.finalize());
    Ok(state)
}

fn access(state: &SecurityState, rights: u32) -> Result<()> {
    let token = token()?;
    ensure!(
        token_sid(&token)? == state.owner,
        "Windows maintenance refuses foreign ownership"
    );
    let mapping = GENERIC_MAPPING {
        GenericRead: FILE_GENERIC_READ,
        GenericWrite: FILE_GENERIC_WRITE,
        GenericExecute: FILE_GENERIC_EXECUTE,
        GenericAll: FILE_ALL_ACCESS,
    };
    let mut privileges = [0usize; 128];
    let mut length = size_of_val(&privileges) as u32;
    let mut granted = 0;
    let mut allowed = 0;
    check(
        unsafe {
            AccessCheck(
                state.descriptor,
                token.as_raw_handle(),
                rights,
                &mapping,
                privileges.as_mut_ptr().cast(),
                &mut length,
                &mut granted,
                &mut allowed,
            )
        },
        "Windows DACL cannot be qualified without takeover",
    )?;
    ensure!(
        allowed != 0
            && granted & rights == rights
            && unsafe { (*privileges.as_ptr().cast::<PRIVILEGE_SET>()).PrivilegeCount } == 0,
        "Existing Windows DACL denies current-owner use or requires privileges; ACL/ownership repair is unsupported"
    );
    Ok(())
}

fn usable_rights(directory: bool) -> u32 {
    READ_CONTROL
        | FILE_READ_ATTRIBUTES
        | FILE_WRITE_ATTRIBUTES
        | FILE_READ_DATA
        | FILE_WRITE_DATA
        | if directory {
            FILE_APPEND_DATA | FILE_EXECUTE
        } else {
            0
        }
}

/// FILE_STREAM_INFO is explicitly 8-byte aligned (Microsoft winbase.h contract).
/// No fallback to default-stream-only inspection: named streams escape the tree
/// content proof. A fixed 64 KiB reply admits only one ordinary unnamed stream.
fn no_alternate_streams(file: &File, metadata: &fs::Metadata) -> Result<()> {
    let mut buffer = vec![0u64; 65536 / size_of::<u64>()];
    let result = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileStreamInfo,
            buffer.as_mut_ptr().cast(),
            65536,
        )
    };
    if result == 0 {
        let error = io::Error::last_os_error();
        ensure!(
            metadata.is_dir() && error.raw_os_error() == Some(ERROR_HANDLE_EOF as i32),
            "NTFS stream inventory unavailable or exceeds 64 KiB; alternate-stream maintenance is unsupported: {error}"
        );
        return Ok(());
    }
    let info = unsafe { &*buffer.as_ptr().cast::<FILE_STREAM_INFO>() };
    let length = info.StreamNameLength as usize;
    ensure!(
        info.NextEntryOffset == 0
            && length <= 65536 - offset_of!(FILE_STREAM_INFO, StreamName)
            && length % 2 == 0,
        "NTFS named/multiple/malformed data streams are unsupported"
    );
    if metadata.is_dir() && length == 0 && info.StreamSize == 0 {
        return Ok(());
    }
    let name = unsafe {
        std::slice::from_raw_parts(ptr::addr_of!(info.StreamName).cast::<u16>(), length / 2)
    };
    ensure!(
        name == [58, 58, 36, 68, 65, 84, 65]
            && info.StreamSize >= 0
            && info.StreamSize as u64 == if metadata.is_dir() { 0 } else { metadata.len() },
        "NTFS alternate named data streams are unsupported; default-stream identity/size changed"
    );
    Ok(())
}

pub(super) fn identity_file(file: &File) -> Result<Identity> {
    let metadata = file.metadata()?;
    let id: FILE_ID_INFO = query(file, FileIdInfo)?;
    let standard: FILE_STANDARD_INFO = query(file, FileStandardInfo)?;
    let basic: FILE_BASIC_INFO = query(file, FileBasicInfo)?;
    ensure!(
        !standard.DeletePending
            && standard.Directory == metadata.is_dir()
            && (standard.Directory || standard.NumberOfLinks == 1),
        "Windows maintenance refuses pending deletion, special objects or hardlinked files"
    );
    let flags = basic.FileAttributes;
    ensure!(
        (metadata.is_file() || metadata.is_dir())
            && flags == metadata.file_attributes()
            && flags
                & !(WINDOWS_IMMUTABLE_ATTRIBUTES | FILE_ATTRIBUTE_READONLY | FILE_ATTRIBUTE_NORMAL)
                == 0
            && (flags & FILE_ATTRIBUTE_NORMAL == 0 || flags == FILE_ATTRIBUTE_NORMAL),
        "Unsupported Windows native attributes; reparse, compressed, encrypted, sparse, offline and unknown profiles are refused"
    );
    no_alternate_streams(file, &metadata)?;
    let state = security(file)?;
    access(&state, usable_rights(metadata.is_dir()))?;
    let native = WindowsIdentity {
        volume_serial: id.VolumeSerialNumber,
        file_id: id.FileId.Identifier,
        owner_sid: state.owner.clone(),
        security_sha256: state.fingerprint.clone(),
        attributes: flags & WINDOWS_IMMUTABLE_ATTRIBUTES,
    };
    Ok(Identity {
        device: native.volume_serial,
        inode: u64::from_le_bytes(native.file_id[..8].try_into().unwrap()),
        owner: 0,
        group: 0,
        native_flags: None,
        windows: Some(native),
    })
}

pub(super) fn identity_at(path: &Path, metadata: &fs::Metadata) -> Result<Identity> {
    let file = open(path, READ_CONTROL | FILE_READ_ATTRIBUTES)?;
    let current = file.metadata()?;
    ensure!(
        current.is_dir() == metadata.is_dir()
            && current.file_attributes() == metadata.file_attributes()
            && current.len() == metadata.len()
            && current.modified()? == metadata.modified()?,
        "Windows maintenance metadata changed before native capture"
    );
    identity_file(&file)
}

pub(super) fn qualify(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    identity_at(path, metadata)?;
    Ok(())
}

pub(super) fn qualify_rename(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    ensure!(
        metadata.is_dir() || !metadata.permissions().readonly(),
        "Readonly file rename is unsupported; run OwnerWritable and create a new lowercase preview: {}",
        path.display()
    );
    let file = open(path, READ_CONTROL | FILE_READ_ATTRIBUTES)?;
    access(&security(&file)?, DELETE | usable_rights(metadata.is_dir()))
}

pub(super) fn qualify_private(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    let file = open(path, READ_CONTROL | FILE_READ_ATTRIBUTES)?;
    ensure!(
        metadata.is_file() && !metadata.permissions().readonly(),
        "Private maintenance metadata must be writable regular files"
    );
    let state = security(&file)?;
    access(&state, usable_rights(false) | DELETE)?;
    ensure!(
        state.control & SE_DACL_PROTECTED != 0 && !state.dacl.is_null(),
        "Maintenance metadata requires a protected owner-only DACL"
    );
    let acl = unsafe { &*state.dacl };
    ensure!(
        acl.AceCount == 1,
        "Maintenance metadata DACL contains foreign access entries"
    );
    let mut raw = ptr::null_mut();
    check(
        unsafe { GetAce(state.dacl, 0, &mut raw) },
        "Could not inspect private metadata ACE",
    )?;
    let ace = unsafe { &*raw.cast::<ACCESS_ALLOWED_ACE>() };
    ensure!(
        ace.Header.AceType == 0
            && ace.Header.AceFlags == 0
            && ace.Mask == FILE_ALL_ACCESS
            && sid_bytes((&ace.SidStart as *const u32).cast_mut().cast())? == state.owner,
        "Maintenance metadata is not protected exclusively for its current owner"
    );
    Ok(())
}

pub(super) fn create_file(path: &Path) -> Result<File> {
    let parent = path
        .parent()
        .context("Private Windows metadata requires a parent")?;
    let metadata = crate::checkpoint::safe_path(parent)?
        .context("Private Windows metadata parent disappeared")?;
    super::writable_directory(parent, &metadata)?;
    let owner = token_sid(&token()?)?;
    let mut owner_buffer = vec![0usize; owner.len().div_ceil(size_of::<usize>())];
    unsafe {
        ptr::copy_nonoverlapping(
            owner.as_ptr(),
            owner_buffer.as_mut_ptr().cast::<u8>(),
            owner.len(),
        )
    };
    let owner_ptr = owner_buffer.as_mut_ptr().cast();
    // ACL is aligned, bounded, and supplied to CREATE_NEW before any bytes exist.
    let acl_size =
        size_of::<ACL>() + size_of::<ACCESS_ALLOWED_ACE>() - size_of::<u32>() + owner.len();
    let mut acl_buffer = vec![0usize; acl_size.div_ceil(size_of::<usize>())];
    let acl = acl_buffer.as_mut_ptr().cast::<ACL>();
    check(
        unsafe { InitializeAcl(acl, acl_size as u32, ACL_REVISION) },
        "Could not initialize private metadata ACL",
    )?;
    check(
        unsafe { AddAccessAllowedAceEx(acl, ACL_REVISION, 0, FILE_ALL_ACCESS, owner_ptr) },
        "Could not grant private metadata access to owner",
    )?;
    let mut descriptor = SECURITY_DESCRIPTOR::default();
    let descriptor_ptr = (&mut descriptor as *mut SECURITY_DESCRIPTOR).cast();
    check(
        unsafe { InitializeSecurityDescriptor(descriptor_ptr, 1) },
        "Could not initialize private metadata descriptor",
    )?;
    check(
        unsafe { SetSecurityDescriptorOwner(descriptor_ptr, owner_ptr, 0) },
        "Could not bind private metadata owner",
    )?;
    check(
        unsafe { SetSecurityDescriptorDacl(descriptor_ptr, 1, acl, 0) },
        "Could not set private metadata DACL",
    )?;
    check(
        unsafe {
            SetSecurityDescriptorControl(descriptor_ptr, SE_DACL_PROTECTED, SE_DACL_PROTECTED)
        },
        "Could not protect private metadata DACL",
    )?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor_ptr,
        bInheritHandle: 0,
    };
    let handle = unsafe {
        CreateFileW(
            wide(path)?.as_ptr(),
            GENERIC_READ | GENERIC_WRITE | DELETE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            &attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_ARCHIVE | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error())
            .context("Could not create protected private maintenance metadata");
    }
    let file = unsafe { File::from_raw_handle(handle) };
    qualify_private(path, &file.metadata()?)?;
    Ok(file)
}

pub(super) fn private_temp(parent: &Path, prefix: &str) -> Result<tempfile::NamedTempFile> {
    for _ in 0..16 {
        let path = parent.join(format!("{prefix}{}", uuid::Uuid::new_v4()));
        let mut temporary = tempfile::TempPath::try_from_path(path)
            .context("Could not resolve a private Windows maintenance path")?;
        // A colliding name is not ours: arm cleanup only after exclusive creation succeeds.
        temporary.disable_cleanup(true);
        match create_file(&temporary) {
            Ok(file) => {
                temporary.disable_cleanup(false);
                return Ok(tempfile::NamedTempFile::from_parts(file, temporary));
            }
            Err(error)
                if error
                    .downcast_ref::<io::Error>()
                    .is_some_and(|e| e.kind() == io::ErrorKind::AlreadyExists) => {}
            Err(error) => return Err(error),
        }
    }
    anyhow::bail!("Could not reserve a private Windows maintenance name")
}

fn rename_handle(
    source: &File,
    parent: &File,
    name: &std::ffi::OsStr,
    replace: bool,
) -> Result<()> {
    let units: Vec<u16> = name.encode_wide().collect();
    ensure!(
        !units.is_empty()
            && units.len() <= 255
            && !units.iter().any(|c| matches!(*c, 0 | 47 | 92 | 58)),
        "Invalid native maintenance destination component"
    );
    let size = offset_of!(FILE_RENAME_INFO, FileName) + (units.len() + 1) * size_of::<u16>();
    let mut buffer = vec![0usize; size.div_ceil(size_of::<usize>())];
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    unsafe {
        (*info).Anonymous.ReplaceIfExists = replace;
        (*info).RootDirectory = parent.as_raw_handle();
        (*info).FileNameLength = (units.len() * size_of::<u16>()) as u32;
        ptr::copy_nonoverlapping(
            units.as_ptr(),
            ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
            units.len(),
        );
    }
    check(
        unsafe {
            SetFileInformationByHandle(
                source.as_raw_handle(),
                FileRenameInfo,
                info.cast(),
                size as u32,
            )
        },
        "Exclusive Windows maintenance rename failed; no overwrite/copy fallback",
    )
}

pub(super) fn rename(from: &Path, to: &Path, expected_parent: &Identity) -> Result<()> {
    ensure!(
        from.parent() == to.parent(),
        "Maintenance rename cannot change its parent"
    );
    let parent_path = from.parent().context("Windows rename parent missing")?;
    let parent = open_parent(parent_path)?;
    ensure!(
        identity_file(&parent)? == *expected_parent,
        "Windows rename parent identity/security changed"
    );
    let metadata =
        crate::checkpoint::safe_path(from)?.context("Windows rename source disappeared")?;
    qualify_rename(from, &metadata)?;
    let before = identity_at(from, &metadata)?;
    let source = open(from, DELETE | READ_CONTROL | FILE_READ_ATTRIBUTES)?;
    ensure!(
        identity_file(&source)? == before,
        "Windows rename source identity/security changed"
    );
    rename_handle(
        &source,
        &parent,
        to.file_name().context("Windows rename target missing")?,
        false,
    )?;
    ensure!(
        identity_file(&source)? == before
            && identity_file(&parent)? == *expected_parent
            && identity_at(to, &source.metadata()?)? == before,
        "Windows rename identity/security/attributes changed"
    );
    crate::checkpoint::persistence::namespace_boundary(parent_path)
}

pub(super) fn publish_checked(
    from: &Path,
    to: &Path,
    replace: bool,
    expected: &Identity,
) -> Result<()> {
    let source_parent = open_parent(from.parent().context("Private source parent missing")?)?;
    let target_parent = open_parent(to.parent().context("Private target parent missing")?)?;
    let source_parent_id = identity_file(&source_parent)?;
    let target_parent_id = identity_file(&target_parent)?;
    ensure!(
        source_parent_id.device == target_parent_id.device
            && expected.device == target_parent_id.device,
        "Windows maintenance publication must remain on the same local NTFS volume"
    );
    if replace {
        let metadata =
            crate::checkpoint::safe_path(to)?.context("Owned replacement metadata disappeared")?;
        qualify_private(to, &metadata)?;
    } else {
        ensure!(
            crate::checkpoint::safe_path(to)?.is_none(),
            "Windows maintenance metadata destination exists"
        );
    }
    let source = open(from, DELETE | READ_CONTROL | FILE_READ_ATTRIBUTES)?;
    qualify_private(from, &source.metadata()?)?;
    ensure!(
        identity_file(&source)? == *expected,
        "Private Windows metadata identity changed before publication"
    );
    rename_handle(
        &source,
        &target_parent,
        to.file_name().context("Private destination name missing")?,
        replace,
    )?;
    ensure!(
        identity_file(&source)? == *expected
            && identity_file(&source_parent)? == source_parent_id
            && identity_file(&target_parent)? == target_parent_id
            && identity_at(to, &source.metadata()?)? == *expected,
        "Windows publication changed identity/security/attributes"
    );
    crate::checkpoint::persistence::namespace_boundary(to.parent().unwrap())?;
    crate::checkpoint::persistence::namespace_boundary(from.parent().unwrap())
}

pub(super) fn chmod(path: &Path, desired: u32, expected: &Identity) -> Result<()> {
    ensure!(desired <= 1, "Malformed Windows readonly state");
    let file = open(
        path,
        READ_CONTROL | FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES,
    )?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && identity_file(&file)? == *expected,
        "Windows readonly target identity/security changed or is a directory"
    );
    if super::mode(&metadata)? == desired {
        return Ok(());
    }
    // An existing writable handle retains its access when readonly is restored.
    // Do not reopen data-write access after setting the readonly attribute.
    let before_flush = if !metadata.permissions().readonly() {
        let writable = open(path, GENERIC_READ | GENERIC_WRITE)?;
        ensure!(
            identity_file(&writable)? == *expected,
            "Windows readonly rollback object changed before opening flush handle"
        );
        Some(writable)
    } else {
        None
    };
    let immutable = expected
        .windows
        .as_ref()
        .context("Windows target lacks native identity")?
        .attributes;
    let flags = immutable
        | if desired == 1 {
            FILE_ATTRIBUTE_READONLY
        } else {
            0
        };
    let basic = FILE_BASIC_INFO {
        FileAttributes: if flags == 0 {
            FILE_ATTRIBUTE_NORMAL
        } else {
            flags
        },
        ..Default::default()
    };
    check(
        unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle(),
                FileBasicInfo,
                (&basic as *const FILE_BASIC_INFO).cast_mut().cast(),
                size_of::<FILE_BASIC_INFO>() as u32,
            )
        },
        "Pinned Windows readonly repair failed",
    )?;
    if let Some(writable) = before_flush {
        writable.sync_all()?;
    } else if desired == 0 {
        let writable = open(path, GENERIC_READ | GENERIC_WRITE)?;
        ensure!(
            identity_file(&writable)? == *expected,
            "Windows readonly repair target changed before flush"
        );
        writable.sync_all()?;
    }
    ensure!(
        identity_file(&file)? == *expected && super::mode(&file.metadata()?)? == desired,
        "Windows readonly repair changed owner/DACL/native flags or failed readback"
    );
    Ok(())
}
