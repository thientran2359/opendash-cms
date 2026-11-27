use super::*;

pub(super) use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read as _,
    path::Path,
};

pub(super) use opendash_core::{
    block_schema::{BlockSchemaV1, RegisteredBlockSchema},
    builder::BuilderDocument,
    id::valid_kebab_segment_with_limit,
    registry::{DeclarationState, OwnedDescriptor, ProviderStatus, RegistryCatalog, ResourceOwner},
    theme::{TemplateCandidateKind, ThemeV3Manifest, valid_asset_path},
};

pub(super) use serde::{Deserialize, Serialize};

pub(super) use crate::package::{
    V2_MAX_DECODED_BYTES, V2_MAX_FILES, V2_MAX_RASTER_BYTES, V2_MAX_TEXT_BYTES, V2_MAX_ZIP_BYTES,
    V2ZipError, read_v2_zip_entries, validate_theme_relative_path,
};

pub const MAX_ZIP_BYTES: usize = 64 * 1024;

/// Parsed, validated theme v3 revision. S01 validates without activating or
/// writing site state; storage/import transition to v3 is reserved for S02.
#[derive(Debug, Clone)]
pub struct ThemeV3Package {
    pub manifest: ThemeV3Manifest,
    pub manifest_json: String,
    pub files: BTreeMap<String, Vec<u8>>,
}

// The approved v3 active asset cap is 512 KiB; retain legacy text caps.
pub(super) const V3_MAX_ACTIVE_ASSET_BYTES: usize = 512 * 1024;

pub(crate) const MAX_THEME_LOCALE_FILES: usize = 32;

/// Revalidate direct storage callers with the same manifest, path and
/// composition guarantees as the bounded ZIP parser.
pub(crate) fn validate_theme_v3_files(
    files: BTreeMap<String, Vec<u8>>,
) -> Result<ThemeV3Package, V2ZipError> {
    if files.len() > V2_MAX_FILES {
        return Err(V2ZipError::PayloadTooLarge(
            "Theme v3 vượt quá giới hạn file.",
        ));
    }
    let mut decoded_total = 0usize;
    for (path, data) in &files {
        validate_theme_relative_path(path).map_err(V2ZipError::BadRequest)?;
        let cap = v3_entry_cap_and_allowlist(path).map_err(V2ZipError::BadRequest)?;
        decoded_total = decoded_total.saturating_add(data.len());
        if data.len() > cap || decoded_total > V2_MAX_DECODED_BYTES {
            return Err(V2ZipError::PayloadTooLarge(
                "Theme v3 vượt quá giới hạn kích thước file.",
            ));
        }
    }
    let manifest_json = required_v3_utf8(&files, "theme.json")?.to_owned();
    let manifest: ThemeV3Manifest = serde_json::from_str(&manifest_json)
        .map_err(|_| V2ZipError::BadRequest("theme.json không đúng schema theme v3."))?;
    manifest.validate().map_err(V2ZipError::BadRequest)?;
    // Validate schema even for packages declaring no HTML templates; defaults
    // and sample option values must agree before any revision is installed.
    let option_schema = crate::storage::settings::parse_theme_options_schema(&manifest_json)
        .map_err(V2ZipError::BadRequest)?;
    if let Some(raw_sample) = manifest.sample.as_ref() {
        let sample: crate::storage::import::ThemeSampleManifest =
            serde_json::from_value(raw_sample.clone())
                .map_err(|_| V2ZipError::BadRequest("Theme sample có cấu trúc không hợp lệ."))?;
        crate::storage::import::validate_sample_option_values(&sample, &option_schema)
            .map_err(V2ZipError::BadRequest)?;
    }
    let url_policy = crate::storage::settings::theme_url_policy(&manifest_json)
        .map_err(V2ZipError::BadRequest)?;
    let blocks =
        collect_package_blocks(&files, &manifest.theme_id).map_err(V2ZipError::BadRequest)?;
    parse_theme_declarations(&manifest_json).map_err(V2ZipError::BadRequest)?;
    let theme_action_ids: Vec<String> = manifest
        .functions
        .as_ref()
        .map(|f| f.actions.iter().map(|a| a.id.clone()).collect())
        .unwrap_or_default();
    if let Some(extensions) = &manifest.admin_extensions {
        extensions
            .validate("theme", &theme_action_ids)
            .map_err(V2ZipError::BadRequest)?;
    }

    // Symmetry check for optional root functions.js
    let has_functions_file = files.contains_key("functions.js");
    if manifest.functions.is_some() != has_functions_file {
        return Err(V2ZipError::BadRequest(
            "Theme v3 phải khai báo 'functions' trong theme.json tương ứng với file functions.js ở root.",
        ));
    }
    if let Some(func_bytes) = files.get("functions.js") {
        let src = std::str::from_utf8(func_bytes)
            .map_err(|_| V2ZipError::BadRequest("File functions.js phải là UTF-8 hợp lệ."))?;
        crate::plugin::js_runtime::validate_js_module_syntax(src)
            .map_err(|_| V2ZipError::BadRequest("File functions.js có lỗi cú pháp JavaScript."))?;
    }

    // D01: every regular asset member must be explicitly listed in the
    // manifest, and every declared member must be present in the bounded ZIP.
    // The shared package reader has already rejected symlinks, duplicate
    // regular paths and unsafe traversal before we compare these exact sets.
    let declared_assets: BTreeSet<&str> = manifest.assets.iter().map(String::as_str).collect();
    let actual_assets: BTreeSet<&str> = files
        .keys()
        .filter(|path| path.starts_with("assets/"))
        .map(String::as_str)
        .collect();
    if declared_assets != actual_assets {
        return Err(V2ZipError::BadRequest(
            "Theme v3 assets khai báo không khớp file thực tế trong ZIP.",
        ));
    }

    // Only HTML and patterns explicitly permitted by the v3 manifest may
    // enter the package. Unreferenced executable templates are rejected.
    let mut declared = BTreeSet::from(["theme.json".to_string()]);
    if has_functions_file {
        declared.insert("functions.js".to_string());
    }
    let mut block_catalog = RegistryCatalog::<RegisteredBlockSchema>::new();
    for block in crate::storage::page::embedded_core_block_defs().values() {
        block_catalog.insert_loaded(
            block.block_id.clone(),
            OwnedDescriptor {
                owner: ResourceOwner::Core,
                status: ProviderStatus::Active,
                declaration_state: DeclarationState::Declared,
                descriptor: block.schema.clone(),
            },
        );
    }
    for (schema, _) in &blocks {
        declared.insert(format!(
            "blocks/{}/block.json",
            schema.id.split_once('/').unwrap().1
        ));
        declared.insert(format!(
            "blocks/{}/render.html",
            schema.id.split_once('/').unwrap().1
        ));
        block_catalog.insert_loaded(
            schema.id.clone(),
            OwnedDescriptor {
                owner: ResourceOwner::Theme(manifest.theme_id.clone()),
                status: ProviderStatus::Active,
                declaration_state: DeclarationState::Declared,
                descriptor: RegisteredBlockSchema {
                    schema: schema.clone(),
                },
            },
        );
    }
    let mut parts = BTreeMap::new();
    for (key, path) in &manifest.parts {
        let source = required_v3_utf8(&files, path)?;
        declared.insert(path.clone());
        parts.insert(key.clone(), source.to_string());
    }

    use crate::template_sec::{
        CsecContentKind, validate_and_compile_csec_composition_with_theme_urls,
    };
    if let Some(td) = manifest.text_domain.as_deref() {
        opendash_core::l10n::validate_extension_text_domain(td).map_err(V2ZipError::BadRequest)?;
    }

    for declaration in &manifest.templates {
        let source = required_v3_utf8(&files, &declaration.file)?;
        declared.insert(declaration.file.clone());
        let kind = match declaration.kind {
            TemplateCandidateKind::Home
            | TemplateCandidateKind::Page
            | TemplateCandidateKind::PageLayout
            | TemplateCandidateKind::Single
            | TemplateCandidateKind::SingleCustom
            | TemplateCandidateKind::Singular => CsecContentKind::Entry,
            TemplateCandidateKind::Index => CsecContentKind::Index,
            _ => CsecContentKind::NonEntry,
        };
        validate_and_compile_csec_composition_with_theme_urls(
            &declaration.file,
            source,
            &parts,
            kind,
            &url_policy,
        )
        .map_err(V2ZipError::BadRequest)?;
    }
    for layout in &manifest.page_layouts {
        let source = required_v3_utf8(&files, &layout.file)?;
        declared.insert(layout.file.clone());
        validate_and_compile_csec_composition_with_theme_urls(
            &layout.file,
            source,
            &parts,
            CsecContentKind::Entry,
            &url_policy,
        )
        .map_err(V2ZipError::BadRequest)?;
    }
    for (key, source) in &parts {
        validate_and_compile_csec_composition_with_theme_urls(
            key,
            source,
            &parts,
            CsecContentKind::NonEntry,
            &url_policy,
        )
        .map_err(V2ZipError::BadRequest)?;
    }

    let mut lang_files_count = 0usize;
    let mut seen_lower_lang_paths = BTreeSet::new();
    for (path, raw) in &files {
        if path.starts_with("locales/") {
            return Err(V2ZipError::BadRequest(
                "Theme v3 không còn hỗ trợ locales/*.json; hãy dùng languages/*.{pot,po,mo}.",
            ));
        }
        if path.starts_with("languages/") {
            if !seen_lower_lang_paths.insert(path.to_ascii_lowercase()) {
                return Err(V2ZipError::BadRequest(
                    "Theme v3 có file languages/ trùng tên hoa/thường.",
                ));
            }
            if lang_files_count >= MAX_THEME_LOCALE_FILES {
                return Err(V2ZipError::BadRequest(
                    "Theme v3 vượt quá giới hạn 32 file languages/.",
                ));
            }
            if path.ends_with(".mo") {
                opendash_core::l10n::parse_mo(raw).map_err(|_| {
                    V2ZipError::BadRequest("File .mo trong languages/ không hợp lệ.")
                })?;
            } else if path.ends_with(".po") {
                let text = std::str::from_utf8(raw).map_err(|_| {
                    V2ZipError::BadRequest("File .po trong languages/ phải là UTF-8.")
                })?;
                opendash_core::l10n::compile_po_to_mo(text).map_err(|_| {
                    V2ZipError::BadRequest("File .po trong languages/ không hợp lệ.")
                })?;
            } else if path.ends_with(".pot") {
                let text = std::str::from_utf8(raw).map_err(|_| {
                    V2ZipError::BadRequest("File .pot trong languages/ phải là UTF-8.")
                })?;
                opendash_core::l10n::PotDocument::parse(text).map_err(|_| {
                    V2ZipError::BadRequest("File .pot trong languages/ không hợp lệ.")
                })?;
            } else {
                return Err(V2ZipError::BadRequest(
                    "File trong languages/ của theme v3 phải có đuôi .pot, .po hoặc .mo.",
                ));
            }
            lang_files_count += 1;
            declared.insert(path.clone());
        }
    }
    for pattern in &manifest.patterns {
        let source = required_v3_utf8(&files, &pattern.file)?;
        declared.insert(pattern.file.clone());
        let document = BuilderDocument::from_json(source.as_bytes())
            .map_err(|_| V2ZipError::BadRequest("Theme Pattern JSON không hợp lệ."))?;
        document
            .validate_for_save_with_schemas(&block_catalog, None)
            .map_err(|_| V2ZipError::BadRequest("Theme Pattern Builder AST không hợp lệ."))?;
        if let Some(path) = &pattern.preview_asset {
            let is_raster = [".png", ".jpg", ".jpeg", ".webp", ".gif"]
                .iter()
                .any(|ext| path.ends_with(ext));
            if !is_raster || !declared_assets.contains(path.as_str()) || !files.contains_key(path) {
                return Err(V2ZipError::BadRequest(
                    "Theme Pattern preview_asset phải là ảnh raster đã khai báo và tồn tại.",
                ));
            }
            declared.insert(path.clone());
        }
    }
    for (path, raw) in &files {
        if path.starts_with("assets/") {
            // Assets must be assets, never silently interpreted as template
            // executable content. CSS is validated under the same C-SEC rules.
            if path.ends_with(".css") {
                let css = std::str::from_utf8(raw)
                    .map_err(|_| V2ZipError::BadRequest("Theme v3 CSS phải là UTF-8 hợp lệ."))?;
                crate::template_sec::validate_safe_css(css).map_err(V2ZipError::BadRequest)?;
            } else if path.ends_with(".js") || path.ends_with(".mjs") || path.ends_with(".svg") {
                std::str::from_utf8(raw)
                    .map_err(|_| V2ZipError::BadRequest("Theme v3 JS/SVG phải là UTF-8 hợp lệ."))?;
            }
        } else if !declared.contains(path) {
            return Err(V2ZipError::BadRequest(
                "Gói theme v3 có file chưa được khai báo trong theme.json.",
            ));
        }
    }

    Ok(ThemeV3Package {
        manifest,
        manifest_json,
        files,
    })
}

pub const MAX_THEME_DIR_DEPTH: usize = 16;

#[derive(Debug, Clone, Serialize)]
pub struct ThemeCheckSummary {
    pub ok: bool,
    pub theme_id: String,
    pub version: String,
    pub templates: Vec<String>,
    pub parts: Vec<String>,
    pub page_layouts: Vec<String>,
    pub patterns: Vec<String>,
    pub blocks: Vec<String>,
    pub assets: Vec<String>,
    pub options_count: usize,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub has_functions: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeDeclarations {
    #[serde(default)]
    pub content_types: Vec<ThemeContentTypeDeclaration>,
    #[serde(default)]
    pub taxonomy_eligibility: Vec<ThemeTaxonomyEligibilityDeclaration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub taxonomy_definitions: Vec<opendash_core::content::TaxonomyDeclaration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub field_groups: Vec<opendash_core::content::ManifestFieldGroupInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeContentTypeDeclaration {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub fields: Vec<opendash_core::content::ContentFieldDefinition>,
    #[serde(default)]
    pub taxonomies: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<opendash_core::content::ContentTypeSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rewrite_base: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeTaxonomyEligibilityDeclaration {
    pub content_type: String,
    #[serde(default)]
    pub taxonomies: Vec<String>,
}
