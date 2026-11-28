use super::*;

impl PluginManager {
    pub fn new(db_pool: SqlitePool, allow_loopback: bool) -> Self {
        let (plugins_root, themes_root, secrets_root) = match crate::paths::SitePaths::from_env() {
            Ok(p) => (p.plugins_dir(), p.themes_dir(), p.secrets_dir()),
            Err(_) => (
                std::env::temp_dir().join("opendash_plugins_default"),
                std::env::temp_dir().join("opendash_themes_default"),
                std::env::temp_dir().join("opendash_secrets_default"),
            ),
        };
        Self::with_roots(db_pool, allow_loopback, plugins_root, secrets_root)
            .with_themes_root(themes_root)
    }
}

impl PluginManager {
    pub fn with_plugins_root(
        db_pool: SqlitePool,
        allow_loopback: bool,
        plugins_root: PathBuf,
    ) -> Self {
        let secrets_root = crate::paths::SitePaths::from_env()
            .map(|p| p.secrets_dir())
            .unwrap_or_else(|_| {
                plugins_root
                    .parent()
                    .unwrap_or(&plugins_root)
                    .join("database")
                    .join("secrets")
            });
        let themes_root = plugins_root
            .parent()
            .unwrap_or(&plugins_root)
            .join("themes");
        Self::with_roots(db_pool, allow_loopback, plugins_root, secrets_root)
            .with_themes_root(themes_root)
    }
}

impl PluginManager {
    pub fn with_roots(
        db_pool: SqlitePool,
        allow_loopback: bool,
        plugins_root: PathBuf,
        secrets_root: PathBuf,
    ) -> Self {
        let themes_root = plugins_root
            .parent()
            .unwrap_or(&plugins_root)
            .join("themes");
        // Layout dirs created lazily; startup quarantine runs once in main.rs.
        Self {
            db_pool,
            runtime: PluginRuntime::with_secrets_root(secrets_root.clone()),
            active_action_leases: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashSet::new(),
            )),
            confirmation_store: Arc::new(LifecycleConfirmationStore::new()),
            allow_loopback,
            plugins_root,
            themes_root,
            secrets_root,
            manifest_cache: Arc::new(RwLock::new(None)),
            active_theme_cache: Arc::new(RwLock::new(None)),
            recovery: crate::config::RecoveryState::new(false),
        }
    }
}

impl PluginManager {
    pub fn with_themes_root(mut self, themes_root: PathBuf) -> Self {
        self.themes_root = themes_root;
        self
    }
}

impl PluginManager {
    #[allow(dead_code)]
    pub fn themes_root(&self) -> &std::path::Path {
        &self.themes_root
    }
}

impl PluginManager {
    pub fn engine(&self) -> &wasmi::Engine {
        self.runtime.engine()
    }
}

impl PluginManager {
    pub async fn invalidate_cache(&self) {
        *self.manifest_cache.write().await = None;
    }
}

impl PluginManager {
    pub async fn invalidate_active_theme_cache(&self) {
        *self.active_theme_cache.write().await = None;
    }
}

impl PluginManager {
    pub(super) async fn ensure_active_theme_cache(
        &self,
    ) -> Result<Option<CachedActiveTheme>, String> {
        if let Some(dev) = crate::dev_mode::DevOverlayConfig::global()
            && dev.dev_theme_id().is_some()
        {
            return match dev.load_dev_theme() {
                Ok(Some(snap)) => {
                    let Some(runtime_manifest) = snap.manifest.to_runtime_manifest() else {
                        return Ok(None);
                    };
                    let Some(js_bytes) = snap.functions_js else {
                        return Ok(None);
                    };
                    let db_sel = crate::storage::theme::get_active_selection(&self.db_pool)
                        .await
                        .map_err(|e| e.1.to_string())?;
                    let revision_id =
                        if db_sel.theme_id.as_deref() == Some(snap.manifest.theme_id.as_str()) {
                            db_sel
                                .revision_id
                                .unwrap_or_else(|| format!("dev:{}", snap.manifest.theme_id))
                        } else {
                            format!("dev:{}", snap.manifest.theme_id)
                        };
                    Ok(Some(CachedActiveTheme {
                        theme_id: snap.manifest.theme_id,
                        revision_id,
                        revision_digest: snap.revision_digest,
                        selection_version: db_sel.version,
                        manifest: runtime_manifest,
                        js_bytes,
                    }))
                }
                Ok(None) => Ok(None),
                Err(err) => {
                    eprintln!("Dev theme functions load failed: {err}");
                    Ok(None)
                }
            };
        }
        {
            let guard = self.active_theme_cache.read().await;
            if let Some(ref cached) = *guard {
                return Ok(cached.clone());
            }
        }
        let snap_opt = crate::storage::theme::load_active_theme_functions_snapshot(&self.db_pool)
            .await
            .map_err(|e| e.1.to_string())?;
        let built = match snap_opt {
            None => None,
            Some(snap) => {
                let parsed: opendash_core::theme::ThemeManifest =
                    match serde_json::from_str(&snap.manifest_json) {
                        Ok(m) => m,
                        Err(e) => {
                            eprintln!("Invalid active theme manifest {}: {e}", snap.theme_id);
                            *self.active_theme_cache.write().await = Some(None);
                            return Ok(None);
                        }
                    };
                match parsed.to_runtime_manifest() {
                    None => None,
                    Some(runtime_manifest) => {
                        let rev_dir = self
                            .themes_root
                            .join("revisions")
                            .join(&snap.revision_digest);
                        let marker = rev_dir.join(".complete");
                        let marker_ok = std::fs::read_to_string(&marker)
                            .ok()
                            .is_some_and(|s| s.trim() == snap.revision_digest);
                        let func_path = rev_dir.join("functions.js");
                        let no_symlink = crate::secrets::ensure_no_reparse_or_symlink(&rev_dir)
                            .is_ok()
                            && crate::secrets::ensure_no_reparse_or_symlink(&func_path).is_ok();
                        let expected_hex =
                            snap.file_digests.get("functions.js").map(String::as_str);
                        let js_bytes_opt = if marker_ok && no_symlink {
                            std::fs::read(&func_path).ok().filter(|bytes| {
                                expected_hex == Some(sha256_hex(bytes).as_str())
                                    && std::str::from_utf8(bytes).is_ok()
                            })
                        } else {
                            None
                        };
                        js_bytes_opt.map(|js_bytes| CachedActiveTheme {
                            theme_id: snap.theme_id,
                            revision_id: snap.revision_id,
                            revision_digest: snap.revision_digest,
                            selection_version: snap.selection_version,
                            manifest: runtime_manifest,
                            js_bytes,
                        })
                    }
                }
            }
        };
        *self.active_theme_cache.write().await = Some(built.clone());
        Ok(built)
    }
}

impl PluginManager {
    pub async fn active_theme_manifest(&self) -> Option<(String, String, i64, PluginManifest)> {
        let cached = self.ensure_active_theme_cache().await.ok()??;
        Some((
            cached.theme_id,
            cached.revision_id,
            cached.selection_version,
            cached.manifest,
        ))
    }
}

impl PluginManager {
    pub async fn cached_owner_manifest(&self, owner: &RuntimeOwner) -> Option<PluginManifest> {
        match owner.kind {
            ExtensionOwnerKind::Plugin => self.cached_plugin_manifest(&owner.id).await,
            ExtensionOwnerKind::Theme => {
                let cached = self.ensure_active_theme_cache().await.ok()??;
                (cached.theme_id == owner.id).then_some(cached.manifest)
            }
        }
    }
}

impl PluginManager {
    pub(super) async fn ensure_manifest_cache(
        &self,
    ) -> Result<BTreeMap<String, CachedActivePlugin>, String> {
        let dev_overlay = crate::dev_mode::DevOverlayConfig::global();
        if dev_overlay.is_none() {
            let guard = self.manifest_cache.read().await;
            if let Some(ref map) = *guard {
                return Ok(map.clone());
            }
        }
        let rows = plugin_storage::load_all_enabled_plugins_for_cache(&self.db_pool)
            .await
            .map_err(|e| e.to_string())?;
        let mut built = BTreeMap::new();
        for row in rows {
            if row.wasm_blob.is_empty() && row.revision_digest.is_none() {
                // Dev-only placeholder synced via /api/admin/dev/sync; only active when mounted via --dev-plugin.
                continue;
            }
            match PluginManifest::from_json_str(&row.manifest_json) {
                Ok(manifest) if manifest.id == row.id => {
                    built.insert(
                        row.id,
                        CachedActivePlugin {
                            manifest,
                            wasm_blob: row.wasm_blob,
                            revision_generation: row.revision_generation,
                            revision_digest: row.revision_digest,
                        },
                    );
                }
                Ok(_) => {}
                Err(error) => {
                    eprintln!("Invalid cached manifest {}: {error}", row.id);
                }
            }
        }
        if let Some(dev) = dev_overlay {
            for snap in dev.load_all_dev_plugins_best_effort(self.runtime.engine()) {
                let db_gen = built
                    .get(&snap.manifest.id)
                    .map(|existing| existing.revision_generation)
                    .unwrap_or(0);
                built.insert(
                    snap.manifest.id.clone(),
                    CachedActivePlugin {
                        manifest: snap.manifest,
                        wasm_blob: snap.code_bytes,
                        revision_generation: db_gen,
                        revision_digest: Some(snap.revision_digest),
                    },
                );
            }
            return Ok(built);
        }
        *self.manifest_cache.write().await = Some(built.clone());
        Ok(built)
    }
}

impl PluginManager {
    pub async fn cached_plugin_manifest(&self, id: &str) -> Option<PluginManifest> {
        let cache = self.ensure_manifest_cache().await.ok()?;
        cache.get(id).map(|item| item.manifest.clone())
    }
}

impl PluginManager {
    pub fn with_recovery_state(mut self, recovery: crate::config::RecoveryState) -> Self {
        self.recovery = recovery;
        self
    }
}

impl PluginManager {
    pub fn plugins_root(&self) -> &std::path::Path {
        &self.plugins_root
    }
}

impl PluginManager {
    pub fn secrets_root(&self) -> &std::path::Path {
        &self.secrets_root
    }
}

impl PluginManager {
    #[allow(dead_code)]
    pub fn plugin_secrets_dir(&self, plugin_id: &str) -> PathBuf {
        self.secrets_root.join("plugins").join(plugin_id)
    }
}

impl PluginManager {
    pub fn runtime(&self) -> &PluginRuntime {
        &self.runtime
    }
}

impl PluginManager {
    pub fn allow_loopback(&self) -> bool {
        self.allow_loopback
    }
}

impl PluginManager {
    pub async fn disable_plugin_cas(
        &self,
        id: &str,
        expected_cas_version: Option<i64>,
    ) -> Result<i64, plugin_storage::PluginStorageError> {
        let cas =
            plugin_storage::set_plugin_enabled_cas(&self.db_pool, id, false, expected_cas_version)
                .await?;
        self.invalidate_cache().await;
        Ok(cas)
    }
}

impl PluginManager {
    pub async fn enable_plugin_cas(
        &self,
        id: &str,
        expected_cas_version: Option<i64>,
    ) -> Result<i64, plugin_storage::PluginStorageError> {
        let cas =
            plugin_storage::set_plugin_enabled_cas(&self.db_pool, id, true, expected_cas_version)
                .await?;
        self.invalidate_cache().await;
        Ok(cas)
    }
}

impl PluginManager {
    pub async fn list_plugins(&self) -> Result<Vec<PluginSummary>, String> {
        plugin_storage::list_plugins(&self.db_pool).await
    }
}

impl PluginManager {
    pub async fn get_plugin(&self, id: &str) -> Result<Option<PluginSummary>, String> {
        plugin_storage::get_plugin(&self.db_pool, id).await
    }
}

impl PluginManager {
    pub async fn disable_plugin(&self, id: &str) -> Result<(), String> {
        plugin_storage::set_plugin_enabled(&self.db_pool, id, false).await?;
        self.invalidate_cache().await;
        Ok(())
    }
}

impl PluginManager {
    pub async fn enable_plugin(&self, id: &str) -> Result<(), String> {
        plugin_storage::set_plugin_enabled(&self.db_pool, id, true).await?;
        self.invalidate_cache().await;
        Ok(())
    }
}

impl PluginManager {
    #[allow(dead_code)]
    pub async fn record_hook_failure(&self, id: &str, correlation_id: &str, message: &str) {
        self.record_owner_hook_failure(&RuntimeOwner::plugin(id), correlation_id, message)
            .await;
    }
}

impl PluginManager {
    pub async fn record_owner_hook_failure(
        &self,
        owner: &RuntimeOwner,
        correlation_id: &str,
        message: &str,
    ) {
        plugin_storage::record_owner_execution_result(
            &self.db_pool,
            owner.kind,
            &owner.id,
            correlation_id,
            "error",
            message,
            Some(message),
        )
        .await;
    }
}
