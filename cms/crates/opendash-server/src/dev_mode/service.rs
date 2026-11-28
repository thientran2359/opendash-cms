use super::*;

pub(super) static GLOBAL_DEV_OVERLAY: OnceLock<RwLock<Option<Arc<DevOverlayConfig>>>> =
    OnceLock::new();

pub(super) fn global_slot() -> &'static RwLock<Option<Arc<DevOverlayConfig>>> {
    GLOBAL_DEV_OVERLAY.get_or_init(|| RwLock::new(None))
}

/// Validates startup safety guards before activating Dev Live-Edit Mode.
pub fn validate_startup_guards(
    has_dev_flags: bool,
    portable: &PortableRuntimeConfigV1,
    allow_dev_remote: bool,
) -> Result<(), String> {
    if !has_dev_flags {
        return Ok(());
    }
    if portable.recovery_safe_mode {
        return Err(
            "Không thể khởi chạy Dev Mode (--dev-theme/--dev-plugin) khi recovery_safe_mode đang bật."
                .to_owned(),
        );
    }
    if portable.trusted_public_origin.is_some() && !allow_dev_remote {
        return Err(
            "Từ chối khởi chạy Dev Mode khi trusted_public_origin đã được cấu hình; chỉ dùng cục bộ hoặc truyền thêm --allow-dev-remote."
                .to_owned(),
        );
    }
    Ok(())
}

impl DevOverlayConfig {
    pub fn init_global(cfg: Arc<Self>) {
        let mut guard = global_slot().write().unwrap_or_else(|e| e.into_inner());
        *guard = Some(cfg);
        crate::page::BlockTemplateCache::global().clear();
        crate::cache::SiteCacheEpoch::global().bump();
    }

    #[allow(dead_code)]
    pub fn clear_global() {
        let mut guard = global_slot().write().unwrap_or_else(|e| e.into_inner());
        *guard = None;
        crate::page::BlockTemplateCache::global().clear();
        crate::cache::SiteCacheEpoch::global().bump();
    }

    /// Returns active dev overlay config, or `None` if unconfigured or if runtime Safe Mode is enabled.
    pub fn global() -> Option<Arc<Self>> {
        let cfg = global_slot()
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()?;
        if cfg.recovery.is_safe_mode() {
            return None;
        }
        Some(cfg)
    }

    pub fn load_and_validate(
        theme_dir: Option<PathBuf>,
        plugin_dirs: Vec<PathBuf>,
        recovery: RecoveryState,
        engine: &wasmi::Engine,
    ) -> Result<Self, String> {
        let dev_theme = match theme_dir {
            Some(dir) => {
                let canonical = validate_dev_root_dir(&dir)?;
                let snap = read_dev_theme_package(&canonical)?;
                Some(DevThemeOverlay {
                    root: canonical,
                    theme_id: snap.manifest.theme_id,
                })
            }
            None => None,
        };

        let mut dev_plugins = Vec::with_capacity(plugin_dirs.len());
        let mut seen_ids = BTreeSet::new();
        for dir in plugin_dirs {
            let canonical = validate_dev_root_dir(&dir)?;
            let snap = read_dev_plugin_package(&canonical, engine)?;
            if !seen_ids.insert(snap.manifest.id.clone()) {
                return Err(format!(
                    "Trùng plugin_id '{}' trong danh sách --dev-plugin.",
                    snap.manifest.id
                ));
            }
            dev_plugins.push(DevPluginOverlay {
                root: canonical,
                plugin_id: snap.manifest.id,
            });
        }

        Ok(Self {
            dev_theme,
            dev_plugins,
            recovery,
            last_fingerprints: Mutex::new(BTreeMap::new()),
        })
    }

    pub fn dev_theme_id(&self) -> Option<&str> {
        self.dev_theme.as_ref().map(|t| t.theme_id.as_str())
    }

    pub fn dev_theme_root(&self) -> Option<&Path> {
        self.dev_theme.as_ref().map(|t| t.root.as_path())
    }

    pub fn dev_plugin_ids(&self) -> Vec<String> {
        self.dev_plugins
            .iter()
            .map(|p| p.plugin_id.clone())
            .collect()
    }

    pub fn has_dev_plugin(&self, plugin_id: &str) -> bool {
        self.dev_plugins.iter().any(|p| p.plugin_id == plugin_id)
    }

    pub(super) fn record_fingerprint(&self, scope: &str, digest: &str) {
        let mut map = self
            .last_fingerprints
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let changed = map.get(scope).map(String::as_str) != Some(digest);
        if changed {
            let had_prev = map.insert(scope.to_owned(), digest.to_owned()).is_some();
            if had_prev {
                crate::page::BlockTemplateCache::global().clear();
                crate::cache::SiteCacheEpoch::global().bump();
            }
        }
    }

    pub fn load_dev_theme(&self) -> Result<Option<DevThemeSnapshot>, String> {
        if self.recovery.is_safe_mode() {
            return Ok(None);
        }
        let Some(ref theme) = self.dev_theme else {
            return Ok(None);
        };
        let snap = read_dev_theme_package(&theme.root)?;
        self.record_fingerprint(&format!("theme:{}", theme.theme_id), &snap.revision_digest);
        Ok(Some(snap))
    }

    pub fn read_dev_theme_asset(&self, rel_path: &str) -> Result<Vec<u8>, String> {
        if self.recovery.is_safe_mode() {
            return Err("Safe mode đang bật.".to_owned());
        }
        let Some(ref theme) = self.dev_theme else {
            return Err("Không có dev theme đang gắn.".to_owned());
        };
        read_dev_theme_asset_from_root(&theme.root, rel_path)
    }

    pub fn read_dev_theme_pattern(&self, rel_path: &str) -> Result<Vec<u8>, String> {
        if self.recovery.is_safe_mode() {
            return Err("Safe mode đang bật.".to_owned());
        }
        let Some(ref theme) = self.dev_theme else {
            return Err("Không có dev theme đang gắn.".to_owned());
        };
        read_bounded_contract_file(&theme.root, rel_path, V2_MAX_TEXT_BYTES)
            .map(|(bytes, _, _)| bytes)
    }

    pub fn load_dev_plugin_by_id(
        &self,
        plugin_id: &str,
        engine: &wasmi::Engine,
    ) -> Result<Option<DevPluginSnapshot>, String> {
        if self.recovery.is_safe_mode() {
            return Ok(None);
        }
        let Some(entry) = self.dev_plugins.iter().find(|p| p.plugin_id == plugin_id) else {
            return Ok(None);
        };
        let snap = read_dev_plugin_package(&entry.root, engine)?;
        if snap.manifest.id != entry.plugin_id {
            return Err(format!(
                "manifest.json của dev plugin '{}' đổi id thành '{}'.",
                entry.plugin_id, snap.manifest.id
            ));
        }
        self.record_fingerprint(
            &format!("plugin:{}", entry.plugin_id),
            &snap.revision_digest,
        );
        Ok(Some(snap))
    }

    pub fn load_all_dev_plugins(
        &self,
        engine: &wasmi::Engine,
    ) -> Result<Vec<DevPluginSnapshot>, String> {
        if self.recovery.is_safe_mode() {
            return Ok(Vec::new());
        }
        let mut out = Vec::with_capacity(self.dev_plugins.len());
        for entry in &self.dev_plugins {
            let snap = read_dev_plugin_package(&entry.root, engine)?;
            if snap.manifest.id != entry.plugin_id {
                return Err(format!(
                    "manifest.json của dev plugin '{}' đổi id thành '{}'.",
                    entry.plugin_id, snap.manifest.id
                ));
            }
            self.record_fingerprint(
                &format!("plugin:{}", entry.plugin_id),
                &snap.revision_digest,
            );
            out.push(snap);
        }
        Ok(out)
    }

    pub fn load_all_dev_plugins_best_effort(
        &self,
        engine: &wasmi::Engine,
    ) -> Vec<DevPluginSnapshot> {
        if self.recovery.is_safe_mode() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for entry in &self.dev_plugins {
            match read_dev_plugin_package(&entry.root, engine) {
                Ok(snap) if snap.manifest.id == entry.plugin_id => {
                    self.record_fingerprint(
                        &format!("plugin:{}", entry.plugin_id),
                        &snap.revision_digest,
                    );
                    out.push(snap);
                }
                Ok(_) => {}
                Err(err) => {
                    eprintln!("Dev plugin '{}' read skipped: {err}", entry.plugin_id);
                }
            }
        }
        out
    }
}

pub(super) fn validate_dev_root_dir(dir: &Path) -> Result<PathBuf, String> {
    let abs = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| format!("Không xác định được thư mục hiện tại: {e}"))?
            .join(dir)
    };
    ensure_no_reparse_or_symlink(&abs).map_err(|_| {
        format!(
            "Từ chối symlink hoặc reparse point tại thư mục dev: {}",
            abs.display()
        )
    })?;
    let meta = fs::symlink_metadata(&abs)
        .map_err(|e| format!("Không thể truy cập thư mục dev '{}': {e}", abs.display()))?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(format!(
            "Đường dẫn dev '{}' phải là thư mục thực (không phải symlink).",
            abs.display()
        ));
    }
    fs::canonicalize(&abs)
        .map_err(|e| format!("Không thể chuẩn hóa thư mục dev '{}': {e}", abs.display()))
}

/// Resolves a relative contract path safely within `canonical_root`, rejecting
/// hidden segments (`.env`, `.git`, `.complete`, ...), `node_modules`, traversal,
/// and any symlink/reparse point on every segment.
pub fn safe_resolve_under_root(canonical_root: &Path, rel_path: &str) -> Result<PathBuf, String> {
    if rel_path.is_empty()
        || !rel_path.is_ascii()
        || rel_path.starts_with('/')
        || rel_path.ends_with('/')
        || rel_path.contains('\\')
        || rel_path.contains(':')
        || rel_path.contains('\0')
    {
        return Err(format!("Đường dẫn dev không hợp lệ: '{rel_path}'."));
    }

    let mut cur = canonical_root.to_path_buf();
    for seg in rel_path.split('/') {
        if seg.is_empty()
            || seg == "."
            || seg == ".."
            || seg.starts_with('.')
            || seg.eq_ignore_ascii_case("node_modules")
        {
            return Err(format!(
                "Từ chối đường dẫn ẩn/không an toàn trong thư mục dev: '{rel_path}'."
            ));
        }
        cur.push(seg);
        ensure_no_reparse_or_symlink(&cur).map_err(|_| {
            format!("Từ chối symlink/reparse point trong thư mục dev: '{rel_path}'.")
        })?;
    }

    let meta = fs::symlink_metadata(&cur)
        .map_err(|_| format!("Thiếu tệp hợp đồng trong thư mục dev: '{rel_path}'."))?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(format!(
            "Tệp hợp đồng trong thư mục dev không hợp lệ hoặc là symlink: '{rel_path}'."
        ));
    }
    let resolved = fs::canonicalize(&cur)
        .map_err(|_| format!("Không thể xác minh đường dẫn thực của '{rel_path}'."))?;
    if !resolved.starts_with(canonical_root) {
        return Err(format!(
            "Đường dẫn dev thoát khỏi thư mục gốc được phép: '{rel_path}'."
        ));
    }
    Ok(resolved)
}

pub(super) fn read_bounded_contract_file(
    canonical_root: &Path,
    rel_path: &str,
    max_bytes: usize,
) -> Result<(Vec<u8>, u64, u128), String> {
    let path = safe_resolve_under_root(canonical_root, rel_path)?;
    let meta = fs::symlink_metadata(&path)
        .map_err(|e| format!("Không đọc được metadata '{rel_path}': {e}"))?;
    if meta.len() > max_bytes as u64 {
        return Err(format!(
            "Tệp dev '{rel_path}' vượt quá giới hạn kích thước ({max_bytes} bytes)."
        ));
    }
    let mtime_nanos = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let file =
        fs::File::open(&path).map_err(|e| format!("Không mở được tệp dev '{rel_path}': {e}"))?;
    let mut data = Vec::with_capacity((meta.len() as usize).min(max_bytes));
    file.take((max_bytes + 1) as u64)
        .read_to_end(&mut data)
        .map_err(|e| format!("Lỗi đọc tệp dev '{rel_path}': {e}"))?;
    if data.len() > max_bytes {
        return Err(format!(
            "Tệp dev '{rel_path}' vượt quá giới hạn kích thước ({max_bytes} bytes)."
        ));
    }
    let len = data.len() as u64;
    Ok((data, len, mtime_nanos))
}

pub(super) fn scan_contract_subdir_files(
    canonical_root: &Path,
    subdir: &str,
    depth: usize,
    out: &mut Vec<String>,
) -> Result<(), String> {
    let dir_path = canonical_root.join(subdir);
    let meta = match fs::symlink_metadata(&dir_path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("Không đọc được thư mục dev '{subdir}': {e}")),
    };
    if meta.file_type().is_symlink() {
        return Err(format!("Từ chối symlink trong thư mục dev: '{subdir}'."));
    }
    ensure_no_reparse_or_symlink(&dir_path)
        .map_err(|_| format!("Từ chối reparse point trong thư mục dev: '{subdir}'."))?;
    if !meta.is_dir() {
        return Err(format!("Đường dẫn dev '{subdir}' không phải thư mục."));
    }
    if depth > MAX_DEV_SCAN_DEPTH {
        return Err(format!("Thư mục dev '{subdir}' vượt quá độ sâu cho phép."));
    }

    let entries =
        fs::read_dir(&dir_path).map_err(|e| format!("Không đọc được thư mục '{subdir}': {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Lỗi duyệt thư mục '{subdir}': {e}"))?;
        let fname = entry.file_name();
        let Some(name) = fname.to_str() else {
            return Err(format!("Tên file trong '{subdir}' không phải UTF-8."));
        };
        // Ignore `.git` and `.DS_Store`, reject any other hidden file or `node_modules`.
        if name == ".git" || name == ".DS_Store" {
            continue;
        }
        if name.starts_with('.') || name.eq_ignore_ascii_case("node_modules") {
            return Err(format!(
                "Từ chối tệp/thư mục ẩn hoặc bị cấm trong hợp đồng dev: '{subdir}/{name}'."
            ));
        }
        let rel = format!("{subdir}/{name}");
        let entry_path = entry.path();
        ensure_no_reparse_or_symlink(&entry_path)
            .map_err(|_| format!("Từ chối symlink trong thư mục dev: '{rel}'."))?;
        let emeta = fs::symlink_metadata(&entry_path)
            .map_err(|e| format!("Lỗi đọc metadata '{rel}': {e}"))?;
        if emeta.file_type().is_symlink() {
            return Err(format!("Từ chối symlink trong thư mục dev: '{rel}'."));
        }
        if emeta.is_dir() {
            scan_contract_subdir_files(canonical_root, &rel, depth + 1, out)?;
        } else if emeta.is_file() {
            out.push(rel);
        }
    }
    Ok(())
}

pub fn read_dev_theme_asset_from_root(
    canonical_root: &Path,
    rel_path: &str,
) -> Result<Vec<u8>, String> {
    let safe_rel = validate_theme_relative_path(rel_path).map_err(str::to_owned)?;
    if !valid_theme_asset_path(&safe_rel) {
        return Err("Đường dẫn asset không hợp lệ.".to_owned());
    }
    let (manifest_bytes, _, _) =
        read_bounded_contract_file(canonical_root, "theme.json", V2_MAX_TEXT_BYTES)?;
    let manifest_json =
        String::from_utf8(manifest_bytes).map_err(|_| "theme.json không hợp lệ.".to_owned())?;
    let manifest: ThemeManifest =
        serde_json::from_str(&manifest_json).map_err(|_| "theme.json không hợp lệ.".to_owned())?;
    if !manifest.assets.iter().any(|a| a == &safe_rel) {
        return Err("Asset chưa được khai báo trong theme.json.".to_owned());
    }
    let cap = crate::theme::theme_entry_cap_and_allowlist(&safe_rel).map_err(str::to_owned)?;
    let (bytes, _, _) = read_bounded_contract_file(canonical_root, &safe_rel, cap)?;
    if safe_rel.ends_with(".css") {
        let css = std::str::from_utf8(&bytes)
            .map_err(|_| "Theme CSS phải là UTF-8 hợp lệ.".to_owned())?;
        crate::template_sec::validate_safe_css(css).map_err(str::to_owned)?;
    } else if safe_rel.ends_with(".js") || safe_rel.ends_with(".mjs") || safe_rel.ends_with(".svg")
    {
        std::str::from_utf8(&bytes).map_err(|_| "Theme JS/SVG phải là UTF-8 hợp lệ.".to_owned())?;
    }
    Ok(bytes)
}

pub fn read_dev_plugin_package(
    canonical_root: &Path,
    engine: &wasmi::Engine,
) -> Result<DevPluginSnapshot, String> {
    let mut fp_entries: BTreeMap<String, (u64, u128)> = BTreeMap::new();

    let (manifest_bytes, m_len, m_mtime) =
        read_bounded_contract_file(canonical_root, "manifest.json", MAX_DEV_MANIFEST_BYTES)?;
    fp_entries.insert("manifest.json".to_owned(), (m_len, m_mtime));
    let manifest_json = String::from_utf8(manifest_bytes)
        .map_err(|_| "manifest.json không phải UTF-8 hợp lệ.".to_owned())?;
    let manifest = PluginManifest::from_json_str(&manifest_json).map_err(str::to_owned)?;

    let raw: serde_json::Value = serde_json::from_str(&manifest_json)
        .map_err(|_| "manifest.json không đúng JSON.".to_owned())?;
    let admin_extensions = match raw.get("admin_extensions") {
        Some(value) => Some(
            serde_json::from_value::<AdminExtensions>(value.clone())
                .map_err(|_| "admin_extensions không đúng cấu trúc.".to_owned())?,
        ),
        None => None,
    };

    let entrypoint = manifest.entrypoint.as_str();
    let (code_bytes, c_len, c_mtime) =
        read_bounded_contract_file(canonical_root, entrypoint, MAX_DEV_ENTRYPOINT_BYTES)?;
    fp_entries.insert(entrypoint.to_owned(), (c_len, c_mtime));

    match entrypoint {
        "plugin.js" => {
            let src = std::str::from_utf8(&code_bytes)
                .map_err(|_| "File plugin.js phải là UTF-8 hợp lệ.".to_owned())?;
            crate::plugin::js_runtime::validate_js_module_syntax(src)
                .map_err(|e| format!("File plugin.js có lỗi cú pháp JavaScript: {e}"))?;
        }
        _ => {
            crate::plugin::package::validate_wasm_module_bytes(engine, &code_bytes)
                .map_err(str::to_owned)?;
        }
    }

    let mut block_rel_paths = Vec::new();
    scan_contract_subdir_files(canonical_root, "blocks", 0, &mut block_rel_paths)?;
    let mut block_files = BTreeMap::new();
    for rel in block_rel_paths {
        validate_theme_relative_path(&rel).map_err(str::to_owned)?;
        let cap = crate::plugin::package::plugin_entry_max_bytes(&rel)
            .ok_or_else(|| format!("Tệp block plugin không được hỗ trợ: '{rel}'."))?;
        let (bytes, len, mtime) = read_bounded_contract_file(canonical_root, &rel, cap)?;
        fp_entries.insert(rel.clone(), (len, mtime));
        block_files.insert(rel, bytes);
    }
    let blocks =
        crate::theme::collect_package_blocks(&block_files, &manifest.id).map_err(str::to_owned)?;

    let revision_digest = compute_dev_fingerprint(&fp_entries);
    Ok(DevPluginSnapshot {
        manifest,
        manifest_json,
        admin_extensions,
        code_bytes,
        blocks,
        revision_digest,
    })
}
