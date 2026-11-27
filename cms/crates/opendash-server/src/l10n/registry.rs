use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use opendash_core::l10n::{
    Catalog, L10nError, MAX_MO_BYTES, MAX_PO_BYTES, MAX_SITE_LANGUAGE_FILES,
    canonicalize_locale_code, compile_po_to_mo, parse_mo,
};
use sha2::{Digest, Sha256};

use crate::cache::SiteCacheEpoch;
use crate::paths::SitePaths;
use crate::secrets::ensure_no_reparse_or_symlink;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum L10nScope {
    Core,
    Theme,
    Plugin,
}

impl L10nScope {
    pub fn dir_name(self) -> &'static str {
        match self {
            L10nScope::Core => "core",
            L10nScope::Theme => "themes",
            L10nScope::Plugin => "plugins",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CatalogKey {
    pub scope: L10nScope,
    pub domain: String,
    pub locale: String,
}

#[derive(Debug, Clone)]
pub struct LoadedCatalog {
    pub catalog: Arc<Catalog>,
    pub sha256_etag: String,
    pub source: CatalogSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSource {
    SiteOverride,
    DevOverlay,
    PackageBundled,
    EmbeddedCore,
}

#[derive(Debug, Clone)]
pub struct PreSerializedCatalog {
    pub etag: String,
    pub json_bytes: Arc<Vec<u8>>,
}

#[derive(Debug, Default)]
pub struct CatalogSnapshot {
    pub catalogs: HashMap<CatalogKey, LoadedCatalog>,
    pub admin_json_cache: HashMap<String, PreSerializedCatalog>,
}

/// Thread-safe, zero-disk-I/O-on-read catalog registry with deterministic SHA-256 ETags.
pub struct CatalogRegistry {
    snapshot: RwLock<Arc<CatalogSnapshot>>,
    reload_lock: Mutex<()>,
}

impl Default for CatalogRegistry {
    fn default() -> Self {
        Self::new_empty()
    }
}

impl CatalogRegistry {
    pub fn global() -> &'static Arc<CatalogRegistry> {
        static GLOBAL: OnceLock<Arc<CatalogRegistry>> = OnceLock::new();
        GLOBAL.get_or_init(|| Arc::new(CatalogRegistry::new_empty()))
    }

    pub fn new_empty() -> Self {
        Self {
            snapshot: RwLock::new(Arc::new(CatalogSnapshot::default())),
            reload_lock: Mutex::new(()),
        }
    }

    pub fn snapshot(&self) -> Arc<CatalogSnapshot> {
        Arc::clone(&self.snapshot.read().unwrap_or_else(|e| e.into_inner()))
    }

    /// Resolves a catalog following precedence:
    /// exact locale -> parent language tag (e.g. `vi-vn` -> `vi`).
    pub fn resolve(&self, scope: L10nScope, domain: &str, locale: &str) -> Option<LoadedCatalog> {
        let canonical = canonicalize_locale_code(locale).ok()?;
        let snap = self.snapshot();
        let exact = CatalogKey {
            scope,
            domain: domain.to_owned(),
            locale: canonical.clone(),
        };
        if let Some(found) = snap.catalogs.get(&exact) {
            return Some(found.clone());
        }
        if let Some((parent, _)) = canonical.split_once('-') {
            let parent_key = CatalogKey {
                scope,
                domain: domain.to_owned(),
                locale: parent.to_owned(),
            };
            if let Some(found) = snap.catalogs.get(&parent_key) {
                return Some(found.clone());
            }
        }
        None
    }

    /// Reloads `<site_root>/languages/` overrides, merges bundled/embedded/dev catalogs,
    /// pre-serializes Admin JSON payloads, swaps snapshot atomically, and bumps `SiteCacheEpoch`.
    pub fn reload_from_site_paths(
        &self,
        paths: &SitePaths,
        embedded_core: BTreeMap<String, Vec<u8>>,
        bundled_packages: BTreeMap<(L10nScope, String, String), Vec<u8>>,
        dev_overlays: BTreeMap<(L10nScope, String, String), Vec<u8>>,
    ) -> Result<(), L10nError> {
        let _guard = self.reload_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut catalogs: HashMap<CatalogKey, LoadedCatalog> = HashMap::new();

        for (locale, mo_bytes) in embedded_core {
            if let Ok(canon) = canonicalize_locale_code(&locale)
                && let Ok(cat) = parse_mo(&mo_bytes)
            {
                catalogs.insert(
                    CatalogKey {
                        scope: L10nScope::Core,
                        domain: "opendash".to_owned(),
                        locale: canon,
                    },
                    LoadedCatalog {
                        catalog: Arc::new(cat),
                        sha256_etag: format!("\"{}\"", sha256_hex(&mo_bytes)),
                        source: CatalogSource::EmbeddedCore,
                    },
                );
            }
        }

        for ((scope, domain, locale), mo_bytes) in bundled_packages {
            if let Ok(canon) = canonicalize_locale_code(&locale)
                && let Ok(cat) = parse_mo(&mo_bytes)
            {
                catalogs.insert(
                    CatalogKey {
                        scope,
                        domain,
                        locale: canon,
                    },
                    LoadedCatalog {
                        catalog: Arc::new(cat),
                        sha256_etag: format!("\"{}\"", sha256_hex(&mo_bytes)),
                        source: CatalogSource::PackageBundled,
                    },
                );
            }
        }

        for ((scope, domain, locale), mo_bytes) in dev_overlays {
            if let Ok(canon) = canonicalize_locale_code(&locale)
                && let Ok(cat) = parse_mo(&mo_bytes)
            {
                catalogs.insert(
                    CatalogKey {
                        scope,
                        domain,
                        locale: canon,
                    },
                    LoadedCatalog {
                        catalog: Arc::new(cat),
                        sha256_etag: format!("\"{}\"", sha256_hex(&mo_bytes)),
                        source: CatalogSource::DevOverlay,
                    },
                );
            }
        }

        load_site_overrides(paths, &mut catalogs)?;

        let mut admin_json_cache = HashMap::new();
        for (key, loaded) in &catalogs {
            if key.scope == L10nScope::Core && key.domain == "opendash" {
                let bytes =
                    serialize_admin_catalog_json(&key.locale, &loaded.sha256_etag, &loaded.catalog);
                admin_json_cache.insert(
                    key.locale.clone(),
                    PreSerializedCatalog {
                        etag: loaded.sha256_etag.clone(),
                        json_bytes: Arc::new(bytes),
                    },
                );
            }
        }

        let new_snap = Arc::new(CatalogSnapshot {
            catalogs,
            admin_json_cache,
        });
        *self.snapshot.write().unwrap_or_else(|e| e.into_inner()) = new_snap;
        SiteCacheEpoch::global().bump();
        Ok(())
    }

    /// Reloads `<site_root>/languages/` overrides together with the embedded Core `vi.mo` pack.
    pub fn reload_site_defaults(
        &self,
        paths: &SitePaths,
        bundled_packages: BTreeMap<(L10nScope, String, String), Vec<u8>>,
        dev_overlays: BTreeMap<(L10nScope, String, String), Vec<u8>>,
    ) -> Result<(), L10nError> {
        let mut embedded = BTreeMap::new();
        embedded.insert("vi".to_owned(), crate::l10n::EMBEDDED_VI_MO.to_vec());
        self.reload_from_site_paths(paths, embedded, bundled_packages, dev_overlays)
    }

    /// Builds a `{domain}` catalog map for Theme v3 C-SEC rendering at `locale`,
    /// combining Theme catalogs (with site overrides) and Core `"opendash"` fallbacks.
    pub fn theme_render_catalogs(
        &self,
        theme_domain: &str,
        locale: &str,
    ) -> BTreeMap<String, Arc<Catalog>> {
        let mut out = BTreeMap::new();
        if let Some(core_cat) = self.resolve(L10nScope::Core, "opendash", locale) {
            out.insert("opendash".to_owned(), core_cat.catalog);
        }
        if !theme_domain.is_empty()
            && let Some(theme_cat) = self.resolve(L10nScope::Theme, theme_domain, locale)
        {
            out.insert(theme_domain.to_owned(), theme_cat.catalog);
        }
        out
    }

    /// Reloads `<site_root>/languages/`, embedded Core `vi.mo`, and bundled `languages/*.{mo,po}`
    /// from the currently active Theme v3 revision and enabled Plugin revisions.
    pub async fn reload_site_with_active_packages(
        &self,
        db: &sqlx::SqlitePool,
        paths: &SitePaths,
    ) -> Result<(), L10nError> {
        let mut bundled = BTreeMap::new();
        if let Ok(sel) = crate::storage::theme::get_active_selection(db).await
            && let Some(rev_id) = sel.revision_id
        {
            let lang_dir = paths
                .themes_dir()
                .join("revisions")
                .join(&rev_id)
                .join("languages");
            collect_dir_l10n_packages(&lang_dir, L10nScope::Theme, None, &mut bundled);
        }
        if let Ok(plugins) = crate::storage::plugin::list_enabled_plugin_revision_digests(db).await
        {
            for (_plugin_id, domain, digest) in plugins {
                if !digest.is_empty() {
                    let lang_dir = paths
                        .plugins_dir()
                        .join("revisions")
                        .join(&digest)
                        .join("languages");
                    collect_dir_l10n_packages(
                        &lang_dir,
                        L10nScope::Plugin,
                        Some(&domain),
                        &mut bundled,
                    );
                }
            }
        }
        self.reload_site_defaults(paths, bundled, BTreeMap::new())
    }
}

fn collect_dir_l10n_packages(
    dir: &Path,
    scope: L10nScope,
    forced_domain: Option<&str>,
    out: &mut BTreeMap<(L10nScope, String, String), Vec<u8>>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut po_fallback: Vec<(String, String, Vec<u8>)> = Vec::new();
    for ent in entries.flatten() {
        let path = ent.path();
        if !path.is_file() {
            continue;
        }
        let fname = ent.file_name().to_string_lossy().to_string();
        if let Some(stem) = fname.strip_suffix(".mo") {
            if let Some((parsed_domain, locale)) = parse_pack_stem(scope, stem) {
                let domain = forced_domain.unwrap_or(&parsed_domain).to_owned();
                if let Ok(bytes) = fs::read(&path) {
                    out.insert((scope, domain, locale), bytes);
                }
            }
        } else if let Some(stem) = fname.strip_suffix(".po")
            && let Some((parsed_domain, locale)) = parse_pack_stem(scope, stem)
        {
            let domain = forced_domain.unwrap_or(&parsed_domain).to_owned();
            if let Ok(bytes) = fs::read(&path) {
                po_fallback.push((domain, locale, bytes));
            }
        }
    }
    for (domain, locale, po_bytes) in po_fallback {
        let key = (scope, domain, locale);
        if !out.contains_key(&key)
            && let Ok(text) = std::str::from_utf8(&po_bytes)
            && let Ok(mo_bytes) = compile_po_to_mo(text)
        {
            out.insert(key, mo_bytes);
        }
    }
}

fn load_site_overrides(
    paths: &SitePaths,
    out: &mut HashMap<CatalogKey, LoadedCatalog>,
) -> Result<(), L10nError> {
    let lang_root = paths.languages_dir();
    if !lang_root.exists() {
        return Ok(());
    }
    ensure_no_reparse_or_symlink(&lang_root)
        .map_err(|_| L10nError::new("Thư mục languages/ chứa symlink không hợp lệ."))?;

    let mut total_files = 0usize;
    for scope in [L10nScope::Core, L10nScope::Theme, L10nScope::Plugin] {
        let dir = lang_root.join(scope.dir_name());
        if !dir.exists() {
            continue;
        }
        ensure_no_reparse_or_symlink(&dir)
            .map_err(|_| L10nError::new("Thư mục con languages/ chứa symlink không hợp lệ."))?;
        let entries =
            fs::read_dir(&dir).map_err(|_| L10nError::new("Không đọc được thư mục languages/."))?;
        for entry in entries {
            let entry = entry.map_err(|_| L10nError::new("Lỗi đọc mục trong languages/."))?;
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.starts_with(".tmp-") {
                continue;
            }
            let path = entry.path();
            ensure_no_reparse_or_symlink(&path)
                .map_err(|_| L10nError::new("File ngôn ngữ là symlink không hợp lệ."))?;
            if !path.is_file() {
                continue;
            }
            total_files += 1;
            if total_files > MAX_SITE_LANGUAGE_FILES {
                return Err(L10nError::new("Thư mục languages/ vượt quá 128 file."));
            }
            let Some(stem) = fname.strip_suffix(".mo") else {
                continue;
            };
            let Some((domain, locale)) = parse_pack_stem(scope, stem) else {
                continue;
            };
            let meta = fs::metadata(&path)
                .map_err(|_| L10nError::new("Không đọc được metadata file .mo."))?;
            if meta.len() as usize > MAX_MO_BYTES {
                continue;
            }
            let bytes = fs::read(&path)
                .map_err(|_| L10nError::new("Không đọc được file .mo từ languages/."))?;
            if let Ok(cat) = parse_mo(&bytes) {
                out.insert(
                    CatalogKey {
                        scope,
                        domain,
                        locale,
                    },
                    LoadedCatalog {
                        catalog: Arc::new(cat),
                        sha256_etag: format!("\"{}\"", sha256_hex(&bytes)),
                        source: CatalogSource::SiteOverride,
                    },
                );
            }
        }
    }
    Ok(())
}

/// Parses `<domain>-<canonical_locale>` stems safely.
pub fn parse_pack_stem(scope: L10nScope, stem: &str) -> Option<(String, String)> {
    if scope == L10nScope::Core {
        let rest = stem.strip_prefix("opendash-")?;
        let locale = canonicalize_locale_code(rest).ok()?;
        return Some(("opendash".to_owned(), locale));
    }
    // Try splitting from right for `<domain>-<lang>` or `<domain>-<lang>-<region>`
    let parts: Vec<&str> = stem.split('-').collect();
    for split_at in [parts.len().saturating_sub(2), parts.len().saturating_sub(1)] {
        if split_at == 0 || split_at >= parts.len() {
            continue;
        }
        let domain = parts[..split_at].join("-");
        let raw_locale = parts[split_at..].join("-");
        if opendash_core::l10n::validate_extension_text_domain(&domain).is_ok()
            && let Ok(locale) = canonicalize_locale_code(&raw_locale)
        {
            return Some((domain, locale));
        }
    }
    None
}

/// Validates a `.mo` or `.po` file payload during restore preflight.
pub fn validate_language_pack_bytes(is_po: bool, bytes: &[u8]) -> Result<(), L10nError> {
    if is_po {
        if bytes.len() > MAX_PO_BYTES {
            return Err(L10nError::new("File .po vượt quá giới hạn 2 MiB."));
        }
        let text = std::str::from_utf8(bytes)
            .map_err(|_| L10nError::new("File .po phải là UTF-8 hợp lệ."))?;
        let _ = compile_po_to_mo(text)?;
        Ok(())
    } else {
        let _ = parse_mo(bytes)?;
        Ok(())
    }
}

fn serialize_admin_catalog_json(locale: &str, etag: &str, catalog: &Catalog) -> Vec<u8> {
    let mut entries = serde_json::Map::new();
    let mut context_entries: HashMap<String, serde_json::Map<String, serde_json::Value>> =
        HashMap::new();
    let mut plural_entries = serde_json::Map::new();
    let mut context_plural_entries: HashMap<String, serde_json::Map<String, serde_json::Value>> =
        HashMap::new();

    for ((ctx, msgid), entry) in catalog.entries() {
        if let Some(plural) = &entry.msgid_plural {
            let key = format!("{msgid}\u{0000}{plural}");
            let arr = serde_json::Value::Array(
                entry
                    .translations
                    .iter()
                    .map(|s| serde_json::Value::String(s.clone()))
                    .collect(),
            );
            match ctx {
                None => {
                    plural_entries.insert(key, arr);
                }
                Some(c) => {
                    context_plural_entries
                        .entry(c.clone())
                        .or_default()
                        .insert(key, arr);
                }
            }
        } else if let Some(first) = entry.translations.first() {
            let val = serde_json::Value::String(first.clone());
            match ctx {
                None => {
                    entries.insert(msgid.clone(), val);
                }
                Some(c) => {
                    context_entries
                        .entry(c.clone())
                        .or_default()
                        .insert(msgid.clone(), val);
                }
            }
        }
    }

    let ctx_obj: serde_json::Map<String, serde_json::Value> = context_entries
        .into_iter()
        .map(|(k, v)| (k, serde_json::Value::Object(v)))
        .collect();
    let ctx_plural_obj: serde_json::Map<String, serde_json::Value> = context_plural_entries
        .into_iter()
        .map(|(k, v)| (k, serde_json::Value::Object(v)))
        .collect();

    serde_json::to_vec(&serde_json::json!({
        "locale": locale,
        "etag": etag,
        "nplurals": catalog.plural_rule().nplurals,
        "plural_forms": catalog.plural_rule().raw_header,
        "entries": entries,
        "context_entries": ctx_obj,
        "plural_entries": plural_entries,
        "context_plural_entries": ctx_plural_obj,
    }))
    .unwrap_or_else(|_| b"{}".to_vec())
}

fn sha256_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}
