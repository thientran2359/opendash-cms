//! Local Dev Live-Edit overlay (`--dev-theme`, `--dev-plugin`) with strict safety guards.
//!
//! Reads only declared contract files from validated local directories, rejects symlinks,
//! hidden files (`.env`, etc.), and `node_modules`, disables itself immediately if runtime
//! Safe Mode activates, and bumps `SiteCacheEpoch` whenever a file fingerprint changes.

use super::*;

pub(super) use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, RwLock},
    time::UNIX_EPOCH,
};

pub(super) use opendash_core::{
    admin_extensions::AdminExtensions,
    backup::PortableRuntimeConfigV1,
    block_schema::BlockSchema,
    plugin::PluginManifest,
    theme::{TemplateCandidateKind, ThemeManifest, valid_theme_asset_path},
};

pub(super) use sha2::{Digest, Sha256};

pub(super) use crate::{
    config::RecoveryState,
    package::{
        V2_MAX_DECODED_BYTES, V2_MAX_FILES, V2_MAX_TEXT_BYTES, validate_theme_relative_path,
    },
    secrets::ensure_no_reparse_or_symlink,
};

pub(super) const MAX_DEV_MANIFEST_BYTES: usize = 64 * 1024;

pub(super) const MAX_DEV_ENTRYPOINT_BYTES: usize = 2 * 1024 * 1024;

pub(super) const MAX_DEV_SCAN_DEPTH: usize = 4;

#[derive(Debug, Clone)]
pub struct DevThemeOverlay {
    pub root: PathBuf,
    pub theme_id: String,
}

#[derive(Debug, Clone)]
pub struct DevPluginOverlay {
    pub root: PathBuf,
    pub plugin_id: String,
}

#[derive(Debug, Clone)]
pub struct DevThemeSnapshot {
    pub manifest: ThemeManifest,
    pub manifest_json: String,
    pub revision_digest: String,
    pub templates: BTreeMap<String, String>,
    pub parts: BTreeMap<String, String>,
    pub page_layouts: BTreeMap<String, String>,
    pub l10n_catalogs: BTreeMap<String, std::sync::Arc<opendash_core::l10n::Catalog>>,
    pub blocks: Vec<(BlockSchema, String)>,
    pub functions_js: Option<Vec<u8>>,
    pub admin_extensions: Option<AdminExtensions>,
}

#[derive(Debug, Clone)]
pub struct DevPluginSnapshot {
    pub manifest: PluginManifest,
    pub manifest_json: String,
    pub admin_extensions: Option<AdminExtensions>,
    pub code_bytes: Vec<u8>,
    pub blocks: Vec<(BlockSchema, String)>,
    pub revision_digest: String,
}

pub struct DevOverlayConfig {
    pub(super) dev_theme: Option<DevThemeOverlay>,
    pub(super) dev_plugins: Vec<DevPluginOverlay>,
    pub(super) recovery: RecoveryState,
    pub(super) last_fingerprints: Mutex<BTreeMap<String, String>>,
}

pub(super) fn compute_dev_fingerprint(entries: &BTreeMap<String, (u64, u128)>) -> String {
    let mut hasher = Sha256::new();
    for (rel, (len, mtime_nanos)) in entries {
        hasher.update(rel.as_bytes());
        hasher.update(b"\0");
        hasher.update(len.to_le_bytes());
        hasher.update(b"\0");
        hasher.update(mtime_nanos.to_le_bytes());
        hasher.update(b"\n");
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(68);
    hex.push_str("dev-");
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{b:02x}");
    }
    hex
}

pub fn read_dev_theme_package(canonical_root: &Path) -> Result<DevThemeSnapshot, String> {
    let mut fp_entries: BTreeMap<String, (u64, u128)> = BTreeMap::new();
    let mut decoded_total = 0usize;
    let mut file_count = 0usize;

    let mut add_read = |rel: &str, cap: usize| -> Result<Vec<u8>, String> {
        file_count += 1;
        if file_count > V2_MAX_FILES {
            return Err("Theme dev vượt quá giới hạn số lượng file.".to_owned());
        }
        let (bytes, len, mtime) = read_bounded_contract_file(canonical_root, rel, cap)?;
        decoded_total = decoded_total.saturating_add(bytes.len());
        if decoded_total > V2_MAX_DECODED_BYTES {
            return Err("Theme dev vượt quá giới hạn tổng kích thước 32 MiB.".to_owned());
        }
        fp_entries.insert(rel.to_owned(), (len, mtime));
        Ok(bytes)
    };

    let manifest_bytes = add_read("theme.json", V2_MAX_TEXT_BYTES)?;
    let manifest_json = String::from_utf8(manifest_bytes)
        .map_err(|_| "theme.json phải là UTF-8 hợp lệ.".to_owned())?;
    let manifest: ThemeManifest = serde_json::from_str(&manifest_json)
        .map_err(|_| "theme.json v1 không đúng định dạng JSON.".to_owned())?;
    manifest.validate().map_err(str::to_owned)?;
    let url_policy =
        crate::storage::settings::theme_url_policy(&manifest_json).map_err(str::to_owned)?;
    let declarations =
        crate::theme::parse_theme_declarations(&manifest_json).map_err(str::to_owned)?;

    let theme_action_ids: Vec<String> = manifest
        .functions
        .as_ref()
        .map(|f| f.actions.iter().map(|a| a.id.clone()).collect())
        .unwrap_or_default();
    if let Some(ref ext) = manifest.admin_extensions {
        ext.validate("theme", &theme_action_ids)
            .map_err(str::to_owned)?;
    }

    // Optional functions.js symmetry
    let func_path = canonical_root.join("functions.js");
    let has_func_file = fs::symlink_metadata(&func_path).is_ok();
    if manifest.functions.is_some() != has_func_file {
        return Err(
            "Theme dev phải khai báo 'functions' trong theme.json tương ứng với functions.js."
                .to_owned(),
        );
    }
    let functions_js = if has_func_file {
        let cap =
            crate::theme::theme_entry_cap_and_allowlist("functions.js").map_err(str::to_owned)?;
        let bytes = add_read("functions.js", cap)?;
        let src = std::str::from_utf8(&bytes)
            .map_err(|_| "File functions.js phải là UTF-8 hợp lệ.".to_owned())?;
        crate::plugin::js_runtime::validate_js_module_syntax(src)
            .map_err(|e| format!("File functions.js có lỗi cú pháp JavaScript: {e}"))?;
        Some(bytes)
    } else {
        None
    };

    // Parts
    let mut parts = BTreeMap::new();
    for (key, path) in &manifest.parts {
        validate_theme_relative_path(path).map_err(str::to_owned)?;
        let cap = crate::theme::theme_entry_cap_and_allowlist(path).map_err(str::to_owned)?;
        let bytes = add_read(path, cap)?;
        let src = String::from_utf8(bytes)
            .map_err(|_| format!("Template part '{path}' không phải UTF-8 hợp lệ."))?;
        parts.insert(key.clone(), src);
    }

    // Templates
    use crate::template_sec::{
        CsecContentKind, validate_and_compile_csec_composition_with_theme_urls,
    };
    let mut templates = BTreeMap::new();
    for decl in &manifest.templates {
        validate_theme_relative_path(&decl.file).map_err(str::to_owned)?;
        let cap = crate::theme::theme_entry_cap_and_allowlist(&decl.file).map_err(str::to_owned)?;
        let bytes = add_read(&decl.file, cap)?;
        let src = String::from_utf8(bytes)
            .map_err(|_| format!("Template '{}' không phải UTF-8 hợp lệ.", decl.file))?;
        let canonical_key = decl.canonical_key().map_err(str::to_owned)?;
        let cpt_policy;
        let tpl_policy = match crate::theme::cpt_scope_for_target_key(&manifest, &canonical_key) {
            Some(cpt) => {
                cpt_policy = url_policy.with_cpt_entry_fields(
                    &crate::theme::declared_fields_for_cpt(&declarations, cpt),
                );
                &cpt_policy
            }
            None => &url_policy,
        };
        let kind = match decl.kind {
            TemplateCandidateKind::Page
            | TemplateCandidateKind::PageLayout
            | TemplateCandidateKind::Single
            | TemplateCandidateKind::SingleCustom
            | TemplateCandidateKind::Singular => CsecContentKind::Entry,
            TemplateCandidateKind::Index => CsecContentKind::Index,
            _ => CsecContentKind::NonEntry,
        };
        validate_and_compile_csec_composition_with_theme_urls(
            &decl.file, &src, &parts, kind, tpl_policy,
        )
        .map_err(str::to_owned)?;
        let key = decl.canonical_key().map_err(str::to_owned)?;
        templates.insert(key, src);
    }

    // Page layouts
    let mut page_layouts = BTreeMap::new();
    for layout in &manifest.page_layouts {
        validate_theme_relative_path(&layout.file).map_err(str::to_owned)?;
        let cap =
            crate::theme::theme_entry_cap_and_allowlist(&layout.file).map_err(str::to_owned)?;
        let bytes = add_read(&layout.file, cap)?;
        let src = String::from_utf8(bytes)
            .map_err(|_| format!("Page layout '{}' không phải UTF-8 hợp lệ.", layout.file))?;
        validate_and_compile_csec_composition_with_theme_urls(
            &layout.file,
            &src,
            &parts,
            CsecContentKind::Entry,
            &url_policy,
        )
        .map_err(str::to_owned)?;
        page_layouts.insert(layout.key.clone(), src);
    }

    for (key, src) in &parts {
        validate_and_compile_csec_composition_with_theme_urls(
            key,
            src,
            &parts,
            CsecContentKind::NonEntry,
            &url_policy,
        )
        .map_err(str::to_owned)?;
    }

    // Patterns (fingerprint + sanity check)
    for pattern in &manifest.patterns {
        validate_theme_relative_path(&pattern.file).map_err(str::to_owned)?;
        let cap =
            crate::theme::theme_entry_cap_and_allowlist(&pattern.file).map_err(str::to_owned)?;
        let _ = add_read(&pattern.file, cap)?;
    }

    // Blocks under blocks/<slug>/{block.json, render.html}
    let mut block_rel_paths = Vec::new();
    scan_contract_subdir_files(canonical_root, "blocks", 0, &mut block_rel_paths)?;
    let mut block_files = BTreeMap::new();
    for rel in block_rel_paths {
        validate_theme_relative_path(&rel).map_err(str::to_owned)?;
        let cap = crate::theme::theme_entry_cap_and_allowlist(&rel).map_err(str::to_owned)?;
        let bytes = add_read(&rel, cap)?;
        block_files.insert(rel, bytes);
    }
    let blocks = crate::theme::collect_package_blocks(&block_files, &manifest.theme_id)
        .map_err(str::to_owned)?;

    // Languages under languages/*.{pot,po,mo}
    let mut lang_rel_paths = Vec::new();
    scan_contract_subdir_files(canonical_root, "languages", 0, &mut lang_rel_paths)?;
    let mut l10n_catalogs = BTreeMap::new();
    let domain = manifest
        .text_domain
        .as_deref()
        .unwrap_or(&manifest.theme_id);
    for rel in lang_rel_paths {
        validate_theme_relative_path(&rel).map_err(str::to_owned)?;
        let cap = crate::theme::theme_entry_cap_and_allowlist(&rel).map_err(str::to_owned)?;
        let bytes = add_read(&rel, cap)?;
        if let Some(stem) = rel
            .strip_prefix("languages/")
            .and_then(|r| r.strip_suffix(".mo"))
            && let Some(raw_loc) = stem.strip_prefix(&format!("{domain}-"))
            && let Ok(canon) = opendash_core::l10n::canonicalize_locale_code(raw_loc)
            && let Ok(cat) = opendash_core::l10n::parse_mo(&bytes)
        {
            l10n_catalogs.insert(canon, std::sync::Arc::new(cat));
        } else if let Some(stem) = rel
            .strip_prefix("languages/")
            .and_then(|r| r.strip_suffix(".po"))
            && let Some(raw_loc) = stem.strip_prefix(&format!("{domain}-"))
            && let Ok(canon) = opendash_core::l10n::canonicalize_locale_code(raw_loc)
            && !l10n_catalogs.contains_key(&canon)
            && let Ok(text) = std::str::from_utf8(&bytes)
            && let Ok(cat) = opendash_core::l10n::parse_po(text)
        {
            l10n_catalogs.insert(canon, std::sync::Arc::new(cat));
        }
    }

    drop(add_read);

    // Declared assets (verify existence, no symlink, record mtime/len in fingerprint)
    for asset_path in &manifest.assets {
        validate_theme_relative_path(asset_path).map_err(str::to_owned)?;
        if !valid_theme_asset_path(asset_path) {
            return Err(format!("Đường dẫn asset không hợp lệ: '{asset_path}'."));
        }
        let resolved = safe_resolve_under_root(canonical_root, asset_path)?;
        let meta = fs::symlink_metadata(&resolved)
            .map_err(|e| format!("Lỗi đọc metadata asset '{asset_path}': {e}"))?;
        let mtime_nanos = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        fp_entries.insert(asset_path.clone(), (meta.len(), mtime_nanos));
    }

    let admin_extensions = manifest.admin_extensions.clone();
    let revision_digest = compute_dev_fingerprint(&fp_entries);

    Ok(DevThemeSnapshot {
        manifest,
        manifest_json,
        revision_digest,
        templates,
        parts,
        page_layouts,
        l10n_catalogs,
        blocks,
        functions_js,
        admin_extensions,
    })
}
