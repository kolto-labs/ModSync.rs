use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    os::windows::ffi::OsStrExt,
    os::windows::{
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    ptr,
};
use windows_sys::Win32::{
    Foundation::{HANDLE, LocalFree},
    Security::{
        ACCESS_ALLOWED_ACE, ACL,
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            GetSecurityInfo, SE_FILE_OBJECT, SE_KERNEL_OBJECT, SE_OBJECT_TYPE, SetSecurityInfo,
        },
        DACL_SECURITY_INFORMATION, GetAce, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
        GetTokenInformation, IsValidSid, OWNER_SECURITY_INFORMATION,
        PROTECTED_DACL_SECURITY_INFORMATION, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES, TOKEN_GROUPS,
        TOKEN_INFORMATION_CLASS, TOKEN_QUERY, TOKEN_USER, TokenLogonSid, TokenSessionId, TokenUser,
    },
    Storage::FileSystem::{
        CreateDirectoryW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

/// Principals that already administer every local volume and cannot be excluded by
/// any ACL a user could set: SYSTEM, BUILTIN\Administrators and the Windows
/// servicing account NT SERVICE\TrustedInstaller, which owns `C:\` and
/// `C:\Program Files*` on standard installations.
const TRUSTED_SYSTEM: [&str; 3] = [
    "S-1-5-18",
    "S-1-5-32-544",
    "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464",
];
fn trusted_system(sid: &str) -> bool {
    TRUSTED_SYSTEM.contains(&sid)
}
pub(super) struct Local(pub *mut std::ffi::c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Identity {
    pub user: String,
    pub logon: String,
    pub session: u32,
}
fn token_value(token: HANDLE, class: TOKEN_INFORMATION_CLASS) -> Result<Vec<usize>> {
    let mut needed = 0;
    unsafe {
        GetTokenInformation(token, class, ptr::null_mut(), 0, &mut needed);
    }
    ensure!(needed > 0 && needed <= 65536, "Invalid job identity size");
    let mut data = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
    ensure!(
        unsafe { GetTokenInformation(token, class, data.as_mut_ptr().cast(), needed, &mut needed) }
            != 0,
        "Cannot read job identity"
    );
    Ok(data)
}
fn sid_text(sid: *mut std::ffi::c_void) -> Result<String> {
    ensure!(
        !sid.is_null() && unsafe { IsValidSid(sid) } != 0,
        "Invalid owner SID"
    );
    let mut raw = ptr::null_mut();
    ensure!(
        unsafe { ConvertSidToStringSidW(sid, &mut raw) } != 0,
        "Cannot render job identity"
    );
    let _held = Local(raw.cast());
    let mut len = 0;
    while len < 256 && unsafe { *raw.add(len) } != 0 {
        len += 1;
    }
    ensure!(len < 256, "Job identity exceeds bound");
    Ok(String::from_utf16(unsafe {
        std::slice::from_raw_parts(raw, len)
    })?)
}
impl Identity {
    pub fn current() -> Result<Self> {
        let mut raw = ptr::null_mut();
        ensure!(
            unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } != 0,
            "Cannot open job owner identity"
        );
        let token = unsafe { OwnedHandle::from_raw_handle(raw) };
        let user = token_value(token.as_raw_handle(), TokenUser)?;
        let logon = token_value(token.as_raw_handle(), TokenLogonSid)?;
        let session = token_value(token.as_raw_handle(), TokenSessionId)?;
        let groups = unsafe { &*logon.as_ptr().cast::<TOKEN_GROUPS>() };
        ensure!(
            groups.GroupCount == 1,
            "Job recovery requires one logon SID"
        );
        Ok(Self {
            user: sid_text(unsafe { (*user.as_ptr().cast::<TOKEN_USER>()).User.Sid })?,
            logon: sid_text(groups.Groups[0].Sid)?,
            session: unsafe { *session.as_ptr().cast::<u32>() },
        })
    }
    pub fn descriptor(&self) -> Result<Local> {
        // No inheritable or generic grants to other users. SYSTEM can still administer
        // this object using its normal OS privileges; it is not an untrusted principal.
        sddl(&format!("O:{}D:P(A;;GA;;;{})", self.user, self.user))
    }
    /// The private recovery directory: owned by and inheritable only to this user and
    /// the trusted system principals, never to the drive's ordinary user groups.
    fn directory_descriptor(&self) -> Result<Local> {
        let user = &self.user;
        sddl(&format!(
            "O:{user}D:P(A;OICI;FA;;;{user})(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)"
        ))
    }
}
fn sddl(text: &str) -> Result<Local> {
    let text: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let mut descriptor = ptr::null_mut();
    ensure!(
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                1,
                &mut descriptor,
                ptr::null_mut(),
            )
        } != 0,
        "Cannot create job security descriptor"
    );
    Ok(Local(descriptor))
}
fn inspect(
    handle: HANDLE,
    object: SE_OBJECT_TYPE,
    identity: &Identity,
    strict: bool,
    ancestor: bool,
) -> Result<()> {
    let mut owner = ptr::null_mut();
    let mut acl = ptr::null_mut();
    let mut descriptor = ptr::null_mut();
    let result = unsafe {
        GetSecurityInfo(
            handle,
            object,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            &mut acl,
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    ensure!(
        result == 0,
        "Cannot inspect process-fence security ({result})"
    );
    let _held = Local(descriptor);
    let owner = sid_text(owner)?;
    ensure!(
        owner == identity.user || (ancestor && trusted_system(&owner)),
        "Process-fence namespace has an untrusted owner"
    );
    ensure!(
        !acl.is_null(),
        "Process-fence object has no restrictive DACL"
    );
    let mut control = 0;
    let mut revision = 0;
    ensure!(
        unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } != 0,
        "Cannot inspect process-fence DACL control"
    );
    if strict {
        ensure!(
            control & SE_DACL_PROTECTED != 0,
            "Process-fence DACL is not protected"
        );
    }
    let count = unsafe { (*acl).AceCount };
    ensure!(count > 0 && count <= 256, "Invalid process-fence ACL size");
    let mut owner_allowed = false;
    for index in 0..u32::from(count) {
        let mut raw = ptr::null_mut();
        ensure!(
            unsafe { GetAce(acl, index, &mut raw) } != 0,
            "Cannot inspect process-fence ACE"
        );
        let ace = unsafe { &*raw.cast::<ACCESS_ALLOWED_ACE>() };
        ensure!(
            ace.Header.AceType == 0
                && usize::from(ace.Header.AceSize) >= std::mem::size_of::<ACCESS_ALLOWED_ACE>(),
            "Unsupported process-fence ACE"
        );
        // INHERIT_ONLY grants do not authorize mutation of this existing ancestor.
        if ancestor && ace.Header.AceFlags & 0x08 != 0 {
            continue;
        }
        let sid = sid_text((&ace.SidStart as *const u32).cast_mut().cast())?;
        if strict {
            ensure!(
                ace.Header.AceFlags == 0 && sid == identity.user,
                "Unexpected process-fence grant"
            );
        }
        if sid == identity.user {
            owner_allowed = true;
        } else if !trusted_system(&sid) {
            // Other readers on the pre-existing recovery directory are harmless;
            // write/delete/ownership grants would permit record replacement.
            // Ancestor add-file/add-directory rights alone do not replace an existing
            // protected child. Deletion, ACL/owner changes and attribute writes do.
            let mutate = 0x1000_0000
                | 0x4000_0000
                | 0x000d_0000
                | if ancestor { 0x0000_0140 } else { 0x0000_0156 };
            ensure!(
                ace.Mask & mutate == 0,
                "Recovery directory grants mutation to another principal"
            );
        }
    }
    ensure!(
        ancestor || owner_allowed,
        "Process-fence owner lacks an explicit grant"
    );
    Ok(())
}
pub(super) fn check_job(handle: HANDLE, identity: &Identity) -> Result<()> {
    inspect(handle, SE_KERNEL_OBJECT, identity, true, false)
}
pub(super) fn check_file(file: &File, identity: &Identity) -> Result<()> {
    inspect(file.as_raw_handle(), SE_FILE_OBJECT, identity, true, false)
}
pub(super) fn check_directory(path: &Path, identity: &Identity) -> Result<()> {
    use std::os::windows::fs::MetadataExt;
    let file = File::options()
        .access_mode(0x0002_0080)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_dir() && meta.file_attributes() & 0x400 == 0,
        "Unsafe recovery directory"
    );
    inspect(file.as_raw_handle(), SE_FILE_OBJECT, identity, false, false)
}
fn open_ancestor(path: &Path, access: u32) -> Result<File> {
    use std::os::windows::fs::MetadataExt;
    let file = File::options()
        .access_mode(access)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_dir() && meta.file_attributes() & 0x400 == 0,
        "Unsafe recovery ancestor"
    );
    Ok(file)
}
/// The recovery directory itself, which ModSync creates privately: its owner and
/// every grant that could rename, delete or re-permission it are judged.
pub(super) fn check_ancestor(path: &Path, identity: &Identity) -> Result<()> {
    let file = open_ancestor(path, 0x0002_0080)?;
    inspect(file.as_raw_handle(), SE_FILE_OBJECT, identity, false, true)
}
/// A directory above the recovery directory. These are exactly the ancestors of the
/// game root (the recovery directory is its sibling), so anyone who can rename or
/// re-permission one already controls which tree the game path names. Their grants
/// cannot make the fence stronger than the game root itself and are not judged;
/// Windows routinely gives Users or Authenticated Users modify rights there (drive
/// roots, Steam libraries). The owner must still be the current user or a trusted
/// system principal, which rejects a path placed under another user's folder.
pub(super) fn check_outer_ancestor(path: &Path, identity: &Identity) -> Result<()> {
    let file = open_ancestor(path, 0x0002_0080)?;
    let owner = owner_of(file.as_raw_handle())?;
    ensure!(
        owner == identity.user || trusted_system(&owner),
        "Process-fence namespace has an untrusted owner"
    );
    Ok(())
}
fn owner_of(handle: HANDLE) -> Result<String> {
    let mut owner = ptr::null_mut();
    let mut descriptor = ptr::null_mut();
    let result = unsafe {
        GetSecurityInfo(
            handle,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    ensure!(
        result == 0,
        "Cannot inspect process-fence security ({result})"
    );
    let _held = Local(descriptor);
    sid_text(owner)
}
fn dacl_protected(handle: HANDLE) -> Result<bool> {
    let mut descriptor = ptr::null_mut();
    let result = unsafe {
        GetSecurityInfo(
            handle,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    ensure!(
        result == 0,
        "Cannot inspect recovery directory security ({result})"
    );
    let held = Local(descriptor);
    let mut control = 0;
    let mut revision = 0;
    ensure!(
        unsafe { GetSecurityDescriptorControl(held.0, &mut control, &mut revision) } != 0,
        "Cannot inspect recovery directory DACL control"
    );
    Ok(control & SE_DACL_PROTECTED != 0)
}
/// Creates the recovery directory with a protected private DACL instead of the
/// parent's inherited grants. An existing directory this user owns that still only
/// carries inherited grants (left by an older build) is made private the same way.
/// Any other existing directory is left untouched for the fence checks to judge.
/// Only called while acquiring the root locks, never from validation.
pub(super) fn create_private_directory(path: &Path, identity: &Identity) -> Result<()> {
    let descriptor = identity.directory_descriptor()?;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    ensure!(
        !wide[..wide.len() - 1].contains(&0),
        "Recovery directory path contains NUL"
    );
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    if unsafe { CreateDirectoryW(wide.as_ptr(), &attributes) } != 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if error.kind() != std::io::ErrorKind::AlreadyExists {
        return Err(error).context("Cannot create private recovery directory");
    }
    {
        let file = open_ancestor(path, 0x0002_0080)?;
        if owner_of(file.as_raw_handle())? != identity.user || dacl_protected(file.as_raw_handle())?
        {
            return Ok(());
        }
    }
    // READ_CONTROL | WRITE_DAC | FILE_READ_ATTRIBUTES; the owner always holds WRITE_DAC.
    let file = open_ancestor(path, 0x0006_0080)?;
    ensure!(
        owner_of(file.as_raw_handle())? == identity.user,
        "Recovery directory owner changed"
    );
    let mut present = 0;
    let mut defaulted = 0;
    let mut acl: *mut ACL = ptr::null_mut();
    ensure!(
        unsafe { GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut acl, &mut defaulted) }
            != 0
            && present != 0
            && !acl.is_null(),
        "Invalid recovery directory security descriptor"
    );
    let result = unsafe {
        SetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            acl,
            ptr::null_mut(),
        )
    };
    ensure!(
        result == 0,
        "Cannot make recovery directory private ({result})"
    );
    Ok(())
}
pub(super) fn protect_new_file(path: &Path, identity: &Identity) -> Result<()> {
    use std::os::windows::fs::MetadataExt;
    let file = File::options()
        .access_mode(0x0006_0080)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file() && meta.file_attributes() & 0x400 == 0,
        "Unsafe process-fence temporary"
    );
    let descriptor = identity.descriptor()?;
    let mut present = 0;
    let mut defaulted = 0;
    let mut acl: *mut ACL = ptr::null_mut();
    ensure!(
        unsafe { GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut acl, &mut defaulted) }
            != 0
            && present != 0
            && !acl.is_null(),
        "Invalid process-fence security descriptor"
    );
    let result = unsafe {
        SetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            acl,
            ptr::null_mut(),
        )
    };
    ensure!(
        result == 0,
        "Cannot protect process-fence record ({result})"
    );
    check_file(&file, identity).context("Verify new process-fence record security")
}
