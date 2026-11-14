use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    auth::generate_random_token,
    error::{ApiError, ApiResult},
};

pub const MAX_SMTP_PASSWORD_BYTES: usize = 1024;
pub const MAX_SMTP_SECRET_FILE_BYTES: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRef(String);

impl SecretRef {
    pub fn parse(raw: &str) -> ApiResult<Self> {
        if raw.len() != 64 || !raw.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return Err(ApiError::bad_request("SecretRef không hợp lệ."));
        }
        Ok(Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SmtpSecretPayload {
    v: u32,
    password: String,
}

#[derive(Debug, Clone)]
pub struct SecretStore {
    smtp_dir: PathBuf,
}

impl SecretStore {
    pub fn new(smtp_dir: PathBuf) -> Self {
        Self { smtp_dir }
    }

    pub fn smtp_dir(&self) -> &Path {
        &self.smtp_dir
    }

    fn secret_path(&self, secret_ref: &SecretRef) -> PathBuf {
        self.smtp_dir.join(format!("{}.json", secret_ref.as_str()))
    }

    pub fn ensure_private_dir(&self) -> ApiResult<()> {
        if let Some(secrets_root) = self.smtp_dir.parent() {
            ensure_no_reparse_or_symlink(secrets_root)?;
            fs::create_dir_all(secrets_root)
                .map_err(|_| ApiError::internal("Không thể tạo thư mục bí mật."))?;
            ensure_no_reparse_or_symlink(secrets_root)?;
            platform::apply_and_verify_dir_acl(secrets_root)?;
        }
        ensure_no_reparse_or_symlink(&self.smtp_dir)?;
        fs::create_dir_all(&self.smtp_dir)
            .map_err(|_| ApiError::internal("Không thể tạo thư mục bí mật SMTP."))?;
        ensure_no_reparse_or_symlink(&self.smtp_dir)?;
        platform::apply_and_verify_dir_acl(&self.smtp_dir)?;
        Ok(())
    }

    pub fn verify_native_acl(&self) -> ApiResult<()> {
        self.ensure_private_dir()?;
        let probe_name = format!(".acl-probe-{}.tmp", generate_random_token());
        let probe_path = self.smtp_dir.join(probe_name);
        let file = create_protected_empty_file(&probe_path)?;
        drop(file);
        let verify_res = platform::verify_file_acl(&probe_path);
        let _ = fs::remove_file(&probe_path);
        verify_res
    }

    pub fn write_smtp_secret(&self, password: &str) -> ApiResult<SecretRef> {
        if password.len() > MAX_SMTP_PASSWORD_BYTES {
            return Err(ApiError::bad_request(
                "Mật khẩu SMTP vượt quá giới hạn 1024 byte UTF-8.",
            ));
        }
        let payload = SmtpSecretPayload {
            v: 1,
            password: password.to_string(),
        };
        let bytes = serde_json::to_vec(&payload)
            .map_err(|_| ApiError::internal("Không thể mã hóa bí mật SMTP."))?;
        if bytes.len() > MAX_SMTP_SECRET_FILE_BYTES {
            return Err(ApiError::bad_request(
                "Tệp bí mật SMTP vượt quá giới hạn 2 KiB.",
            ));
        }

        self.ensure_private_dir()?;
        let token = generate_random_token();
        let secret_ref = SecretRef::parse(&token)?;
        let final_path = self.secret_path(&secret_ref);
        let tmp_path = self.smtp_dir.join(format!("{}.tmp", secret_ref.as_str()));

        let write_result = (|| -> ApiResult<()> {
            let mut file = create_protected_empty_file(&tmp_path)?;
            platform::verify_file_acl(&tmp_path)?;
            file.write_all(&bytes)
                .map_err(|_| ApiError::internal("Không thể ghi tệp bí mật SMTP."))?;
            file.sync_all()
                .map_err(|_| ApiError::internal("Không thể đồng bộ tệp bí mật SMTP."))?;
            drop(file);
            ensure_no_reparse_or_symlink(&tmp_path)?;
            fs::rename(&tmp_path, &final_path)
                .map_err(|_| ApiError::internal("Không thể hoàn tất tệp bí mật SMTP."))?;
            platform::verify_file_acl(&final_path)?;
            Ok(())
        })();

        if write_result.is_err() {
            let _ = fs::remove_file(&tmp_path);
            let _ = fs::remove_file(&final_path);
        }
        write_result?;
        Ok(secret_ref)
    }

    pub fn read_smtp_secret(&self, raw_ref: &str) -> ApiResult<String> {
        let secret_ref = SecretRef::parse(raw_ref)?;
        ensure_no_reparse_or_symlink(&self.smtp_dir)?;
        platform::verify_dir_acl(&self.smtp_dir)?;

        let path = self.secret_path(&secret_ref);
        ensure_no_reparse_or_symlink(&path)?;
        let meta = fs::symlink_metadata(&path)
            .map_err(|_| ApiError::internal("Không tìm thấy tệp bí mật SMTP."))?;
        if !meta.is_file() || meta.len() > MAX_SMTP_SECRET_FILE_BYTES as u64 {
            return Err(ApiError::internal("Tệp bí mật SMTP không hợp lệ."));
        }
        platform::verify_file_acl(&path)?;

        let mut file = File::open(&path)
            .map_err(|_| ApiError::internal("Không thể đọc tệp bí mật SMTP."))?;
        let mut buf = Vec::with_capacity(meta.len() as usize);
        std::io::Read::by_ref(&mut file)
            .take((MAX_SMTP_SECRET_FILE_BYTES + 1) as u64)
            .read_to_end(&mut buf)
            .map_err(|_| ApiError::internal("Không thể đọc dữ liệu bí mật SMTP."))?;
        if buf.len() > MAX_SMTP_SECRET_FILE_BYTES {
            return Err(ApiError::internal("Tệp bí mật SMTP vượt quá giới hạn."));
        }
        let payload: SmtpSecretPayload = serde_json::from_slice(&buf)
            .map_err(|_| ApiError::internal("Dữ liệu bí mật SMTP bị hỏng."))?;
        if payload.v != 1 || payload.password.len() > MAX_SMTP_PASSWORD_BYTES {
            return Err(ApiError::internal("Phiên bản bí mật SMTP không hợp lệ."));
        }
        Ok(payload.password)
    }

    pub fn remove_secret_blob(&self, raw_ref: &str) {
        if let Ok(secret_ref) = SecretRef::parse(raw_ref) {
            let path = self.secret_path(&secret_ref);
            if ensure_no_reparse_or_symlink(&path).is_ok() {
                let _ = fs::remove_file(path);
            }
        }
    }
}

fn ensure_no_reparse_or_symlink(path: &Path) -> ApiResult<()> {
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
                    return Err(ApiError::internal(
                        "Từ chối reparse point tại kho bí mật.",
                    ));
                }
            }
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(ApiError::internal("Không thể kiểm tra metadata kho bí mật.")),
    }
}

fn create_protected_empty_file(path: &Path) -> ApiResult<File> {
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
mod platform {
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
mod platform {
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt, path::Path, ptr};

    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, LocalFree},
        Security::{
            ACL, ACL_SIZE_INFORMATION, ACCESS_ALLOWED_ACE, AclSizeInformation,
            Authorization::{
                EXPLICIT_ACCESS_W, GetNamedSecurityInfoW, SET_ACCESS, SE_FILE_OBJECT,
                SetEntriesInAclW, SetNamedSecurityInfoW, TRUSTEE_IS_SID, TRUSTEE_IS_USER,
                TRUSTEE_W,
            },
            CreateWellKnownSid, DACL_SECURITY_INFORMATION, EqualSid, GetAce,
            GetAclInformation, GetSecurityDescriptorControl, GetTokenInformation,
            NO_INHERITANCE, PROTECTED_DACL_SECURITY_INFORMATION,
            SECURITY_DESCRIPTOR_CONTROL, SE_DACL_PROTECTED, TOKEN_QUERY, TOKEN_USER,
            TokenUser, WinLocalSystemSid,
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
            if CreateWellKnownSid(WinLocalSystemSid, ptr::null_mut(), buf.as_mut_ptr().cast(), &mut size) == 0 {
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
                    let sid_ptr = (&ace.SidStart as *const u32).cast::<std::ffi::c_void>().cast_mut();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_store_enforces_acl_roundtrip_and_rejects_bad_ref() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("opendash-secret-test-{nanos}"));
        let store = SecretStore::new(dir.clone());
        store.verify_native_acl().expect("native ACL probe");

        let r = store.write_smtp_secret("p@ssw0rd!").expect("write secret");
        assert_eq!(r.as_str().len(), 64);
        assert_eq!(store.read_smtp_secret(r.as_str()).unwrap(), "p@ssw0rd!");
        assert!(SecretRef::parse("../escape").is_err());
        assert!(SecretRef::parse("short").is_err());
        assert!(store.write_smtp_secret(&"a".repeat(1025)).is_err());

        store.remove_secret_blob(r.as_str());
        assert!(store.read_smtp_secret(r.as_str()).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
