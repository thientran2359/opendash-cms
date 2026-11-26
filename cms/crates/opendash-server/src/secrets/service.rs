use super::*;

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

impl SecretStore {
    pub fn new(smtp_dir: PathBuf) -> Self {
        Self { smtp_dir }
    }

    pub fn for_plugin_dir(dir: PathBuf) -> Self {
        Self { smtp_dir: dir }
    }

    pub fn smtp_dir(&self) -> &Path {
        &self.smtp_dir
    }

    pub(super) fn secret_path(&self, secret_ref: &SecretRef) -> PathBuf {
        self.smtp_dir.join(format!("{}.json", secret_ref.as_str()))
    }

    pub fn ensure_private_dir(&self) -> ApiResult<()> {
        if let Some(secrets_root) = self.smtp_dir.parent() {
            if matches!(
                secrets_root.file_name().and_then(|s| s.to_str()),
                Some("plugins" | "themes")
            ) && let Some(base_secrets) = secrets_root.parent()
            {
                ensure_no_reparse_or_symlink(base_secrets)?;
                fs::create_dir_all(base_secrets)
                    .map_err(|_| ApiError::internal("Không thể tạo thư mục bí mật."))?;
                ensure_no_reparse_or_symlink(base_secrets)?;
                platform::apply_and_verify_dir_acl(base_secrets)?;
            }
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

    pub(super) fn write_secret_bytes(
        &self,
        bytes: &[u8],
        max_file_bytes: usize,
    ) -> ApiResult<SecretRef> {
        if bytes.len() > max_file_bytes {
            return Err(ApiError::bad_request("Tệp bí mật vượt quá giới hạn."));
        }
        self.ensure_private_dir()?;
        let token = generate_random_token();
        let secret_ref = SecretRef::parse(&token)?;
        let final_path = self.secret_path(&secret_ref);
        let tmp_path = self.smtp_dir.join(format!("{}.tmp", secret_ref.as_str()));

        let write_result = (|| -> ApiResult<()> {
            let mut file = create_protected_empty_file(&tmp_path)?;
            platform::verify_file_acl(&tmp_path)?;
            file.write_all(bytes)
                .map_err(|_| ApiError::internal("Không thể ghi tệp bí mật."))?;
            file.sync_all()
                .map_err(|_| ApiError::internal("Không thể đồng bộ tệp bí mật."))?;
            drop(file);
            ensure_no_reparse_or_symlink(&tmp_path)?;
            fs::rename(&tmp_path, &final_path)
                .map_err(|_| ApiError::internal("Không thể hoàn tất tệp bí mật."))?;
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

    pub(super) fn read_secret_bytes(
        &self,
        raw_ref: &str,
        max_file_bytes: usize,
    ) -> ApiResult<Vec<u8>> {
        let secret_ref = SecretRef::parse(raw_ref)?;
        ensure_no_reparse_or_symlink(&self.smtp_dir)?;
        platform::verify_dir_acl(&self.smtp_dir)?;

        let path = self.secret_path(&secret_ref);
        ensure_no_reparse_or_symlink(&path)?;
        let meta = fs::symlink_metadata(&path)
            .map_err(|_| ApiError::internal("Không tìm thấy tệp bí mật."))?;
        if !meta.is_file() || meta.len() > max_file_bytes as u64 {
            return Err(ApiError::internal("Tệp bí mật không hợp lệ."));
        }
        platform::verify_file_acl(&path)?;

        let mut file =
            File::open(&path).map_err(|_| ApiError::internal("Không thể đọc tệp bí mật."))?;
        let mut buf = Vec::with_capacity(meta.len() as usize);
        std::io::Read::by_ref(&mut file)
            .take((max_file_bytes + 1) as u64)
            .read_to_end(&mut buf)
            .map_err(|_| ApiError::internal("Không thể đọc dữ liệu bí mật."))?;
        if buf.len() > max_file_bytes {
            return Err(ApiError::internal("Tệp bí mật vượt quá giới hạn."));
        }
        Ok(buf)
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
        self.write_secret_bytes(&bytes, MAX_SMTP_SECRET_FILE_BYTES)
    }

    pub fn read_smtp_secret(&self, raw_ref: &str) -> ApiResult<String> {
        let buf = self.read_secret_bytes(raw_ref, MAX_SMTP_SECRET_FILE_BYTES)?;
        let payload: SmtpSecretPayload = serde_json::from_slice(&buf)
            .map_err(|_| ApiError::internal("Dữ liệu bí mật SMTP bị hỏng."))?;
        if payload.v != 1 || payload.password.len() > MAX_SMTP_PASSWORD_BYTES {
            return Err(ApiError::internal("Phiên bản bí mật SMTP không hợp lệ."));
        }
        Ok(payload.password)
    }

    pub fn write_plugin_secret(&self, plaintext: &str) -> ApiResult<SecretRef> {
        if plaintext.len() > MAX_PLUGIN_SECRET_BYTES {
            return Err(ApiError::bad_request(
                "Bí mật plugin vượt quá giới hạn 4096 byte UTF-8.",
            ));
        }
        let payload = PluginSecretPayload {
            v: 1,
            secret: plaintext.to_string(),
        };
        let bytes = serde_json::to_vec(&payload)
            .map_err(|_| ApiError::internal("Không thể mã hóa bí mật plugin."))?;
        self.write_secret_bytes(&bytes, MAX_PLUGIN_SECRET_FILE_BYTES)
    }

    pub fn read_plugin_secret(&self, raw_ref: &str) -> ApiResult<String> {
        let buf = self.read_secret_bytes(raw_ref, MAX_PLUGIN_SECRET_FILE_BYTES)?;
        let payload: PluginSecretPayload = serde_json::from_slice(&buf)
            .map_err(|_| ApiError::internal("Dữ liệu bí mật plugin bị hỏng."))?;
        if payload.v != 1 || payload.secret.len() > MAX_PLUGIN_SECRET_BYTES {
            return Err(ApiError::internal("Phiên bản bí mật plugin không hợp lệ."));
        }
        Ok(payload.secret)
    }

    pub fn remove_plugin_secret(&self, raw_ref: &str) {
        self.remove_secret_blob(raw_ref);
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

pub fn ensure_private_directory_tree(dir: &Path) -> ApiResult<()> {
    ensure_no_reparse_or_symlink(dir)?;
    fs::create_dir_all(dir).map_err(|_| ApiError::internal("Không thể tạo thư mục riêng tư."))?;
    ensure_no_reparse_or_symlink(dir)?;
    platform::apply_and_verify_dir_acl(dir)?;
    Ok(())
}

pub fn create_private_file(path: &Path) -> ApiResult<File> {
    create_protected_empty_file(path)
}

pub fn verify_file_acl(path: &Path) -> ApiResult<()> {
    platform::verify_file_acl(path)
}
