//! Current-user ownership and protected DACLs for newly created settings objects.
use anyhow::{Result, ensure};
use std::{
    fs::File,
    io,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    ptr,
};
use windows_sys::Win32::{
    Foundation::{INVALID_HANDLE_VALUE, LocalFree},
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            GetSecurityInfo, SE_FILE_OBJECT,
        },
        EqualSid, GetTokenInformation, OWNER_SECURITY_INFORMATION, SECURITY_ATTRIBUTES,
        TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    Storage::FileSystem::{
        CREATE_NEW, CreateDirectoryW, CreateFileW, FILE_ATTRIBUTE_NORMAL,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, MOVEFILE_REPLACE_EXISTING,
        MOVEFILE_WRITE_THROUGH, MoveFileExW, OPEN_EXISTING,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};
struct Local(*mut std::ffi::c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
struct User(Vec<usize>);
impl User {
    fn current() -> Result<Self> {
        let mut token = ptr::null_mut();
        ensure!(
            unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } != 0,
            "Open settings user identity: {}",
            io::Error::last_os_error()
        );
        let token = unsafe { OwnedHandle::from_raw_handle(token) };
        let mut needed = 0;
        unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                TokenUser,
                ptr::null_mut(),
                0,
                &mut needed,
            );
        }
        ensure!(
            needed > 0 && needed <= 65536,
            "Invalid settings user identity size"
        );
        let mut storage = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
        ensure!(
            unsafe {
                GetTokenInformation(
                    token.as_raw_handle(),
                    TokenUser,
                    storage.as_mut_ptr().cast(),
                    needed,
                    &mut needed,
                )
            } != 0,
            "Read settings user identity"
        );
        Ok(Self(storage))
    }
    fn sid(&self) -> *mut std::ffi::c_void {
        unsafe { (&*self.0.as_ptr().cast::<TOKEN_USER>()).User.Sid }
    }
    fn descriptor(&self) -> Result<Local> {
        let mut raw = ptr::null_mut();
        ensure!(
            unsafe { ConvertSidToStringSidW(self.sid(), &mut raw) } != 0,
            "Render settings owner SID"
        );
        let held = Local(raw.cast());
        let mut length = 0;
        while length < 256 && unsafe { *raw.add(length) } != 0 {
            length += 1;
        }
        ensure!(length < 256, "Settings owner SID exceeds bound");
        let sid = String::from_utf16(unsafe { std::slice::from_raw_parts(raw, length) })?;
        drop(held);
        let sddl: Vec<u16> = format!("O:{sid}D:P(A;OICI;FA;;;{sid})")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut descriptor = ptr::null_mut();
        ensure!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    1,
                    &mut descriptor,
                    ptr::null_mut(),
                )
            } != 0,
            "Create private settings security descriptor"
        );
        Ok(Local(descriptor))
    }
}
fn wide(path: &Path) -> Result<Vec<u16>> {
    let value: Vec<_> = path.as_os_str().encode_wide().collect();
    ensure!(!value.contains(&0), "Settings path contains NUL");
    Ok(value.into_iter().chain(Some(0)).collect())
}
pub(super) fn check_file(file: &File) -> Result<()> {
    let user = User::current()?;
    let mut owner = ptr::null_mut();
    let mut descriptor = ptr::null_mut();
    let result = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    ensure!(result == 0, "Read settings ownership failed: {result}");
    let _descriptor = Local(descriptor);
    ensure!(
        !owner.is_null() && unsafe { EqualSid(owner, user.sid()) } != 0,
        "Settings path is not owned by current user"
    );
    Ok(())
}
pub(super) fn check_owner(path: &Path) -> Result<()> {
    let path = wide(path)?;
    // READ_CONTROL plus metadata access, opening the reparse point itself.
    let raw = unsafe {
        CreateFileW(
            path.as_ptr(),
            0x0002_0000 | FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error().into());
    }
    let file = unsafe { File::from_raw_handle(raw) };
    check_file(&file)
}
pub(super) fn create_directory(path: &Path) -> Result<()> {
    let user = User::current()?;
    let descriptor = user.descriptor()?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    let path = wide(path)?;
    if unsafe { CreateDirectoryW(path.as_ptr(), &attributes) } == 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(())
}
pub(super) fn create_file(path: &Path) -> Result<File> {
    let user = User::current()?;
    let descriptor = user.descriptor()?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    let path = wide(path)?;
    let raw = unsafe {
        CreateFileW(
            path.as_ptr(),
            0x8000_0000 | 0x4000_0000,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            &attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error().into());
    }
    Ok(unsafe { File::from_raw_handle(raw) })
}
pub(super) fn replace(from: &Path, to: &Path) -> Result<()> {
    let from = wide(from)?;
    let to = wide(to)?;
    ensure!(
        unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } != 0,
        "Publish private settings failed: {}",
        io::Error::last_os_error()
    );
    Ok(())
}
