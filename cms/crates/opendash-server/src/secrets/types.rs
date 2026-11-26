use super::*;

pub(super) use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub(super) use serde::{Deserialize, Serialize};

pub(super) use crate::{
    auth::generate_random_token,
    error::{ApiError, ApiResult},
};

pub const MAX_SMTP_PASSWORD_BYTES: usize = 1024;

pub const MAX_SMTP_SECRET_FILE_BYTES: usize = 2048;

pub const MAX_PLUGIN_SECRET_BYTES: usize = 4096;

pub const MAX_PLUGIN_SECRET_FILE_BYTES: usize = 8192;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRef(pub(super) String);

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct SmtpSecretPayload {
    pub(super) v: u32,
    pub(super) password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct PluginSecretPayload {
    pub(super) v: u32,
    pub(super) secret: String,
}

#[derive(Debug, Clone)]
pub struct SecretStore {
    pub(super) smtp_dir: PathBuf,
}

pub fn ensure_no_reparse_or_symlink(path: &Path) -> ApiResult<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(ApiError::internal("Từ chối symlink tại kho bí mật."));
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
                if (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
                    return Err(ApiError::internal("Từ chối reparse point tại kho bí mật."));
                }
            }
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(ApiError::internal(
            "Không thể kiểm tra metadata kho bí mật.",
        )),
    }
}

pub(super) fn create_protected_empty_file(path: &Path) -> ApiResult<File> {
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let file = opts
        .open(path)
        .map_err(|_| ApiError::internal("Không thể tạo tệp bí mật mới."))?;
    #[cfg(windows)]
    {
        if let Err(err) = platform::apply_and_verify_file_acl(path) {
            drop(file);
            let _ = fs::remove_file(path);
            return Err(err);
        }
    }
    #[cfg(unix)]
    {
        if let Err(err) = platform::verify_file_acl(path) {
            drop(file);
            let _ = fs::remove_file(path);
            return Err(err);
        }
    }
    Ok(file)
}

#[cfg(unix)]
pub(super) mod platform {
    use std::{fs, os::unix::fs::PermissionsExt, path::Path};

    use crate::error::{ApiError, ApiResult};

    pub fn apply_and_verify_dir_acl(path: &Path) -> ApiResult<()> {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| ApiError::internal("Không thể đặt quyền 0700 cho thư mục bí mật."))?;
        verify_dir_acl(path)
    }

    pub fn verify_dir_acl(path: &Path) -> ApiResult<()> {
        let mode = fs::symlink_metadata(path)
            .map_err(|_| ApiError::internal("Không thể đọc quyền thư mục bí mật."))?
            .permissions()
            .mode();
        if (mode & 0o077) != 0 {
            return Err(ApiError::internal(
                "Quyền thư mục bí mật quá rộng (yêu cầu 0700).",
            ));
        }
        Ok(())
    }

    pub fn verify_file_acl(path: &Path) -> ApiResult<()> {
        let mode = fs::symlink_metadata(path)
            .map_err(|_| ApiError::internal("Không thể đọc quyền tệp bí mật."))?
            .permissions()
            .mode();
        if (mode & 0o177) != 0 {
            return Err(ApiError::internal(
                "Quyền tệp bí mật quá rộng (yêu cầu 0600).",
            ));
        }
        Ok(())
    }
}

#[cfg(windows)]
pub(super) mod platform {
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt, path::Path, ptr};

    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, LocalFree},
        Security::{
            ACCESS_ALLOWED_ACE, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
            Authorization::{
                EXPLICIT_ACCESS_W, GetNamedSecurityInfoW, SE_FILE_OBJECT, SET_ACCESS,
                SetEntriesInAclW, SetNamedSecurityInfoW, TRUSTEE_IS_SID, TRUSTEE_IS_USER,
                TRUSTEE_W,
            },
            CreateWellKnownSid, DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetAclInformation,
            GetSecurityDescriptorControl, GetTokenInformation, NO_INHERITANCE,
            PROTECTED_DACL_SECURITY_INFORMATION, SE_DACL_PROTECTED, SECURITY_DESCRIPTOR_CONTROL,
            TOKEN_QUERY, TOKEN_USER, TokenUser, WinLocalSystemSid,
        },
        Storage::FileSystem::FILE_ALL_ACCESS,
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };

    use crate::error::{ApiError, ApiResult};

    fn to_wide_null(path: &Path) -> Vec<u16> {
        OsStr::new(path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    struct CurrentUserSid {
        buf: Vec<u8>,
        sid: *mut std::ffi::c_void,
    }

    fn current_user_sid() -> ApiResult<CurrentUserSid> {
        unsafe {
            let mut token: HANDLE = ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err(ApiError::internal("Không thể mở process token."));
            }
            let mut needed: u32 = 0;
            let _ = GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut needed);
            if needed == 0 {
                CloseHandle(token);
                return Err(ApiError::internal("Không thể lấy kích thước TokenUser."));
            }
            let mut buf = vec![0u8; needed as usize];
            let ok = GetTokenInformation(
                token,
                TokenUser,
                buf.as_mut_ptr().cast(),
                needed,
                &mut needed,
            );
            CloseHandle(token);
            if ok == 0 {
                return Err(ApiError::internal("Không thể đọc TokenUser SID."));
            }
            let user = &*(buf.as_ptr().cast::<TOKEN_USER>());
            let sid = user.User.Sid;
            Ok(CurrentUserSid { buf, sid })
        }
    }

    fn system_sid() -> ApiResult<Vec<u8>> {
        unsafe {
            let mut size: u32 = 68;
            let mut buf = vec![0u8; size as usize];
            if CreateWellKnownSid(
                WinLocalSystemSid,
                ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut size,
            ) == 0
            {
                return Err(ApiError::internal("Không thể tạo SYSTEM SID."));
            }
            Ok(buf)
        }
    }

    fn apply_protected_dacl(path: &Path) -> ApiResult<()> {
        let user = current_user_sid()?;
        let _keep_buf = &user.buf;
        let mut sys = system_sid()?;
        let wide = to_wide_null(path);

        unsafe {
            let entries = [
                EXPLICIT_ACCESS_W {
                    grfAccessPermissions: FILE_ALL_ACCESS,
                    grfAccessMode: SET_ACCESS,
                    grfInheritance: NO_INHERITANCE,
                    Trustee: TRUSTEE_W {
                        pMultipleTrustee: ptr::null_mut(),
                        MultipleTrusteeOperation: 0,
                        TrusteeForm: TRUSTEE_IS_SID,
                        TrusteeType: TRUSTEE_IS_USER,
                        ptstrName: user.sid.cast(),
                    },
                },
                EXPLICIT_ACCESS_W {
                    grfAccessPermissions: FILE_ALL_ACCESS,
                    grfAccessMode: SET_ACCESS,
                    grfInheritance: NO_INHERITANCE,
                    Trustee: TRUSTEE_W {
                        pMultipleTrustee: ptr::null_mut(),
                        MultipleTrusteeOperation: 0,
                        TrusteeForm: TRUSTEE_IS_SID,
                        TrusteeType: TRUSTEE_IS_USER,
                        ptstrName: sys.as_mut_ptr().cast(),
                    },
                },
            ];

            let mut new_dacl: *mut ACL = ptr::null_mut();
            let err = SetEntriesInAclW(
                entries.len() as u32,
                entries.as_ptr(),
                ptr::null_mut(),
                &mut new_dacl,
            );
            if err != ERROR_SUCCESS || new_dacl.is_null() {
                return Err(ApiError::internal("SetEntriesInAclW thất bại."));
            }

            let set_err = SetNamedSecurityInfoW(
                wide.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                new_dacl,
                ptr::null_mut(),
            );
            LocalFree(new_dacl.cast());
            if set_err != ERROR_SUCCESS {
                return Err(ApiError::internal("SetNamedSecurityInfoW thất bại."));
            }
        }
        Ok(())
    }

    fn verify_protected_dacl(path: &Path) -> ApiResult<()> {
        let user = current_user_sid()?;
        let _keep_buf = &user.buf;
        let mut sys = system_sid()?;
        let wide = to_wide_null(path);

        unsafe {
            let mut dacl: *mut ACL = ptr::null_mut();
            let mut sd: *mut std::ffi::c_void = ptr::null_mut();
            let err = GetNamedSecurityInfoW(
                wide.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut dacl,
                ptr::null_mut(),
                &mut sd,
            );
            if err != ERROR_SUCCESS || sd.is_null() || dacl.is_null() {
                if !sd.is_null() {
                    LocalFree(sd);
                }
                return Err(ApiError::internal("Không thể đọc DACL bảo vệ."));
            }

            let res = (|| -> ApiResult<()> {
                let mut control: SECURITY_DESCRIPTOR_CONTROL = 0;
                let mut revision: u32 = 0;
                if GetSecurityDescriptorControl(sd, &mut control, &mut revision) == 0 {
                    return Err(ApiError::internal("Không thể đọc control flags DACL."));
                }
                if (control & SE_DACL_PROTECTED) == 0 {
                    return Err(ApiError::internal("DACL chưa được bật PROTECTED_DACL."));
                }

                let mut info = ACL_SIZE_INFORMATION {
                    AceCount: 0,
                    AclBytesInUse: 0,
                    AclBytesFree: 0,
                };
                if GetAclInformation(
                    dacl,
                    (&mut info as *mut ACL_SIZE_INFORMATION).cast(),
                    std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
                    AclSizeInformation,
                ) == 0
                {
                    return Err(ApiError::internal("Không thể đọc ACL_SIZE_INFORMATION."));
                }
                if info.AceCount == 0 {
                    return Err(ApiError::internal("DACL rỗng không hợp lệ."));
                }
                for i in 0..info.AceCount {
                    let mut ace_ptr: *mut std::ffi::c_void = ptr::null_mut();
                    if GetAce(dacl, i, &mut ace_ptr) == 0 || ace_ptr.is_null() {
                        return Err(ApiError::internal("Không thể đọc ACE trong DACL."));
                    }
                    let ace = &*(ace_ptr.cast::<ACCESS_ALLOWED_ACE>());
                    // Only ACCESS_ALLOWED_ACE (type 0) is allowed
                    if ace.Header.AceType != 0 {
                        return Err(ApiError::internal("Phát hiện ACE không hợp lệ trong DACL."));
                    }
                    let sid_ptr = (&ace.SidStart as *const u32)
                        .cast::<std::ffi::c_void>()
                        .cast_mut();
                    let is_user = EqualSid(sid_ptr, user.sid) != 0;
                    let is_sys = EqualSid(sid_ptr, sys.as_mut_ptr().cast()) != 0;
                    if !is_user && !is_sys {
                        return Err(ApiError::internal(
                            "DACL chứa SID ngoài service identity và SYSTEM.",
                        ));
                    }
                }
                Ok(())
            })();

            LocalFree(sd);
            res
        }
    }

    pub fn apply_and_verify_dir_acl(path: &Path) -> ApiResult<()> {
        apply_protected_dacl(path)?;
        verify_protected_dacl(path)
    }

    pub fn verify_dir_acl(path: &Path) -> ApiResult<()> {
        verify_protected_dacl(path)
    }

    pub fn apply_and_verify_file_acl(path: &Path) -> ApiResult<()> {
        apply_protected_dacl(path)?;
        verify_protected_dacl(path)
    }

    pub fn verify_file_acl(path: &Path) -> ApiResult<()> {
        verify_protected_dacl(path)
    }
}
