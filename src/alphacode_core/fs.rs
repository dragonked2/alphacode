use std::path::Path;

/// Set file permissions to owner-only read/write (0o600).
/// On Windows, replaces the DACL with a protected full-control ACE for the
/// current process user so inherited or explicit access for other principals
/// cannot expose secret-bearing files.
pub fn set_permissions_owner_only(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, perms)
    }
    #[cfg(windows)]
    {
        set_windows_acl_owner_only(path, 0)
    }
}

/// Set directory permissions to owner-only read/write/execute (0o700).
/// Windows child objects inherit the same current-user-only access rule.
pub fn set_directory_permissions_owner_only(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o700);
        std::fs::set_permissions(path, perms)
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Security::SUB_CONTAINERS_AND_OBJECTS_INHERIT;
        set_windows_acl_owner_only(path, SUB_CONTAINERS_AND_OBJECTS_INHERIT)
    }
}

#[cfg(windows)]
fn set_windows_acl_owner_only(
    path: &Path,
    inheritance: windows_sys::Win32::Security::ACE_FLAGS,
) -> std::io::Result<()> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_ALL, LocalFree};
    use windows_sys::Win32::Security::Authorization::{
        EXPLICIT_ACCESS_W, SE_FILE_OBJECT, SET_ACCESS, SetEntriesInAclW, SetNamedSecurityInfoW,
        TRUSTEE_IS_SID, TRUSTEE_IS_USER, TRUSTEE_W,
    };
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, GetTokenInformation, PROTECTED_DACL_SECURITY_INFORMATION,
        TOKEN_QUERY, TOKEN_USER, TokenUser,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(std::io::Error::last_os_error());
    }

    let result = (|| {
        let mut needed = 0u32;
        unsafe {
            GetTokenInformation(token, TokenUser, null_mut(), 0, &mut needed);
        }
        if needed == 0 {
            return Err(std::io::Error::last_os_error());
        }

        // TOKEN_USER contains pointer-aligned data followed by its SID. A usize
        // buffer gives the cast the alignment required by the Windows ABI.
        let word = std::mem::size_of::<usize>();
        let mut token_user = vec![0usize; (needed as usize).div_ceil(word)];
        if unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                token_user.as_mut_ptr().cast::<c_void>(),
                needed,
                &mut needed,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        let user = unsafe { &*(token_user.as_ptr().cast::<TOKEN_USER>()) };

        let trustee = TRUSTEE_W {
            pMultipleTrustee: null_mut(),
            MultipleTrusteeOperation: 0,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_USER,
            ptstrName: user.User.Sid.cast::<u16>(),
        };
        let access = EXPLICIT_ACCESS_W {
            grfAccessPermissions: GENERIC_ALL,
            grfAccessMode: SET_ACCESS,
            grfInheritance: inheritance,
            Trustee: trustee,
        };
        let mut acl = null_mut();
        let acl_status = unsafe { SetEntriesInAclW(1, &access, null(), &mut acl) };
        if acl_status != 0 {
            return Err(std::io::Error::from_raw_os_error(acl_status as i32));
        }

        let mut wide_path = path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let security_status = unsafe {
            SetNamedSecurityInfoW(
                wide_path.as_mut_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                acl,
                null(),
            )
        };
        unsafe {
            LocalFree(acl.cast::<c_void>());
        }
        if security_status != 0 {
            return Err(std::io::Error::from_raw_os_error(security_status as i32));
        }
        Ok(())
    })();

    unsafe {
        CloseHandle(token);
    }
    result
}
