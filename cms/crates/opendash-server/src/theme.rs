use std::collections::{BTreeMap, BTreeSet};

pub use opendash_core::theme::{Block, BlockDescriptor, Fields};
use opendash_core::{
    builder::BuilderDocument,
    registry::RegistryCatalog,
    theme::{TemplateCandidateKind, ThemeV3Manifest, valid_asset_path},
};
use serde::{Deserialize, Serialize};

use crate::{
    package::{
        V2_MAX_RASTER_BYTES, V2_MAX_TEXT_BYTES, V2_MAX_ZIP_BYTES, V2ZipError,
        ensure_safe_flat_entry, open_bounded_zip, read_bounded_entry, read_v2_zip_entries,
        validate_v2_relative_path,
    },
    storage::theme::valid_v2_theme_id,
};

pub const MAX_ZIP_BYTES: usize = 64 * 1024;

pub struct Theme {
    pub descriptor: BlockDescriptor,
    pub template: String,
    pub content_list_template: Option<String>,
    pub content_single_template: Option<String>,
    /// Explicitly supported block from another installed theme; never rewrites home_page.
    pub compatible_block_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HomepageCompat {
    version: u8,
    accepts: String,
}

impl Theme {
    pub fn from_zip(bytes: &[u8]) -> Result<Self, &'static str> {
        let mut archive = open_bounded_zip(
            bytes,
            MAX_ZIP_BYTES,
            "ZIP vượt quá giới hạn kích thước.",
            "ZIP không hợp lệ.",
        )?;
        if !(2..=5).contains(&archive.len()) {
            return Err(
                "ZIP cần block.json, block.html và chỉ thêm content-list.html/content-single.html/homepage-compat.json.",
            );
        }

        let mut descriptor = None;
        let mut template = None;
        let mut content_list_template = None;
        let mut content_single_template = None;
        let mut homepage_compat = None;
        let mut remaining = MAX_ZIP_BYTES;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|_| "Không đọc được ZIP.")?;
            let name = ensure_safe_flat_entry(
                &entry,
                "Đường dẫn ZIP không an toàn.",
                "ZIP không được chứa symlink.",
                "ZIP không được chứa thư mục con.",
            )?;
            let slot = match name.as_str() {
                "block.json" if descriptor.is_none() => &mut descriptor,
                "block.html" if template.is_none() => &mut template,
                "content-list.html" if content_list_template.is_none() => {
                    &mut content_list_template
                }
                "content-single.html" if content_single_template.is_none() => {
                    &mut content_single_template
                }
                "homepage-compat.json" if homepage_compat.is_none() => &mut homepage_compat,
                _ => return Err("ZIP chứa đường dẫn lạ hoặc mục bị trùng."),
            };
            if entry.size() > remaining as u64 {
                return Err("Nội dung ZIP vượt quá giới hạn kích thước.");
            }
            let raw_bytes = read_bounded_entry(
                &mut entry,
                remaining,
                "Không đọc được nội dung ZIP dạng UTF-8.",
            )?;
            let content = String::from_utf8(raw_bytes)
                .map_err(|_| "Không đọc được nội dung ZIP dạng UTF-8.")?;
            remaining = remaining
                .checked_sub(content.len())
                .ok_or("Nội dung ZIP vượt quá giới hạn kích thước.")?;
            *slot = Some(content);
        }

        let descriptor: BlockDescriptor =
            serde_json::from_str(&descriptor.ok_or("Thiếu block.json.")?)
                .map_err(|_| "block.json không hợp lệ.")?;
        descriptor.validate()?;
        let compatible_block_id = if let Some(raw) = homepage_compat {
            let compat: HomepageCompat =
                serde_json::from_str(&raw).map_err(|_| "homepage-compat.json không hợp lệ.")?;
            if compat.version != 1
                || !opendash_core::id::valid_namespaced_block_id(&compat.accepts)
                || compat.accepts == descriptor.id
            {
                return Err("Alias homepage phải là một block khác có ID hợp lệ (version 1).");
            }
            Some(compat.accepts)
        } else {
            None
        };

        let template = template.ok_or("Thiếu block.html.")?;
        if template.trim().is_empty()
            || crate::template_sec::restricted_environment()
                .template_from_str(&template)
                .is_err()
        {
            return Err("Template block không hợp lệ.");
        }
        for optional in [&content_list_template, &content_single_template] {
            if let Some(template) = optional {
                if template.trim().is_empty()
                    || crate::template_sec::restricted_environment()
                        .template_from_str(template)
                        .is_err()
                {
                    return Err("Template CPT không hợp lệ.");
                }
            }
        }
        Ok(Self {
            descriptor,
            template,
            content_list_template,
            content_single_template,
            compatible_block_id,
        })
    }
}

pub fn render(template: &str, fields: &Fields, block: &Block) -> Result<String, &'static str> {
    block.validate_against_fields(fields)?;
    let context = minijinja::context!(values => &block.values);
    crate::template_sec::render_csec("block", template, context)
        .map_err(|_| "Không render được template block.")
}

/// Shared MiniJinja C-SEC HTML auto-escaping and fuel budget for theme CPT
/// templates and the built-in fallback, matching the Builder's rendering rules.
pub fn render_content(template: &str, data: &serde_json::Value) -> Result<String, &'static str> {
    let context = crate::template_sec::json_to_safe_value(data);
    crate::template_sec::render_csec("content", template, context)
        .map_err(|_| "Không thể render template nội dung.")
}

const ALLOWED_V2_ROLES: &[&str] = &[
    "home",
    "page",
    "header",
    "footer",
    "archive",
    "single",
    "taxonomy_archive",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeV2Manifest {
    pub manifest_version: u8,
    pub theme_id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub templates: BTreeMap<String, String>,
    #[serde(default)]
    pub options: Option<serde_json::Value>,
    #[serde(default)]
    pub menus: Option<serde_json::Value>,
    #[serde(default)]
    pub sample: Option<serde_json::Value>,
    #[serde(default)]
    pub declarations: Option<serde_json::Value>,
    #[serde(default)]
    pub field_groups: Option<serde_json::Value>,
    #[serde(default)]
    pub admin_extensions: Option<opendash_core::admin_extensions::AdminExtensions>,
}

#[derive(Debug, Clone)]
pub struct ThemeV2Package {
    pub manifest: ThemeV2Manifest,
    pub manifest_json: String,
    pub files: BTreeMap<String, Vec<u8>>,
}

/// Parsed, validated theme v3 revision. S01 validates without activating or
/// writing site state; storage/import transition to v3 is reserved for S02.
#[derive(Debug, Clone)]
pub struct ThemeV3Package {
    pub manifest: ThemeV3Manifest,
    pub manifest_json: String,
    pub files: BTreeMap<String, Vec<u8>>,
}

fn v3_entry_cap_and_allowlist(rel_path: &str) -> Result<usize, &'static str> {
    if rel_path == "theme.json" {
        return Ok(V2_MAX_TEXT_BYTES);
    }

    let (parent, name) = match rel_path.rsplit_once('/') {
        Some((parent, name)) => (parent, name),
        None => ("", rel_path),
    };
    if !parent.is_empty()
        && parent.split('/').any(|segment| {
            segment.is_empty()
                || segment.starts_with('.')
                || !segment.bytes().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_')
                })
        })
    {
        return Err("Tên thư mục theme v3 không hợp lệ.");
    }
    let is_html = name.ends_with(".html");
    let is_json = name.ends_with(".json");
    let safe_file_name = name
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.'));
    if !safe_file_name || name.starts_with('.') || name.contains("..") {
        return Err("Tên file theme v3 không hợp lệ.");
    }
    match parent {
        "" | "template-part" | "page-layouts" if is_html => Ok(V2_MAX_TEXT_BYTES),
        "patterns" if is_json => Ok(V2_MAX_TEXT_BYTES),
        _ if rel_path.starts_with("assets/") => {
            if !valid_asset_path(rel_path) {
                return Err("Đường dẫn assets theme v3 không đúng schema D01.");
            }
            let ext = name.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
            match ext {
                "css" | "txt" => Ok(V2_MAX_TEXT_BYTES),
                "png" | "jpg" | "jpeg" | "webp" | "gif" | "woff2" => Ok(V2_MAX_RASTER_BYTES),
                _ => Err("Định dạng assets theme v3 không được cho phép."),
            }
        }
        _ => Err("Gói theme v3 chứa file ngoài root hoặc bốn thư mục được phép."),
    }
}

fn required_v3_utf8<'a>(
    files: &'a BTreeMap<String, Vec<u8>>,
    path: &str,
) -> Result<&'a str, V2ZipError> {
    let bytes = files.get(path).ok_or(V2ZipError::BadRequest(
        "Thiếu file theme được khai báo trong theme.json.",
    ))?;
    let source = std::str::from_utf8(bytes)
        .map_err(|_| V2ZipError::BadRequest("File theme v3 phải là UTF-8 hợp lệ."))?;
    if source.trim().is_empty() {
        return Err(V2ZipError::BadRequest(
            "File theme v3 được khai báo không được để trống.",
        ));
    }
    Ok(source)
}

/// Read a v3 ZIP through the shared bounded, symlink-safe package reader,
/// then validate manifest paths, content AST and C-SEC before any persistence.
/// Keeps the v1/v2 reader untouched until the coordinated S02 cutover.
pub fn parse_theme_zip_v3(bytes: &[u8]) -> Result<ThemeV3Package, V2ZipError> {
    let files = read_v2_zip_entries(bytes, v3_entry_cap_and_allowlist)?;
    let manifest_json = required_v3_utf8(&files, "theme.json")?.to_owned();
    let manifest: ThemeV3Manifest = serde_json::from_str(&manifest_json)
        .map_err(|_| V2ZipError::BadRequest("theme.json không đúng schema theme v3."))?;
    manifest.validate().map_err(V2ZipError::BadRequest)?;
    parse_theme_declarations(&manifest_json).map_err(V2ZipError::BadRequest)?;
    if let Some(extensions) = &manifest.admin_extensions {
        extensions
            .validate("theme", &[])
            .map_err(V2ZipError::BadRequest)?;
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
    let mut parts = BTreeMap::new();
    for (key, path) in &manifest.parts {
        let source = required_v3_utf8(&files, path)?;
        declared.insert(path.clone());
        parts.insert(key.clone(), source.to_string());
    }

    use crate::template_sec::{CsecContentKind, validate_and_compile_csec_composition};
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
        validate_and_compile_csec_composition(&declaration.file, source, &parts, kind)
            .map_err(V2ZipError::BadRequest)?;
    }
    for layout in &manifest.page_layouts {
        let source = required_v3_utf8(&files, &layout.file)?;
        declared.insert(layout.file.clone());
        validate_and_compile_csec_composition(&layout.file, source, &parts, CsecContentKind::Entry)
            .map_err(V2ZipError::BadRequest)?;
    }
    for (key, source) in &parts {
        validate_and_compile_csec_composition(key, source, &parts, CsecContentKind::NonEntry)
            .map_err(V2ZipError::BadRequest)?;
    }
    for pattern in &manifest.patterns {
        let source = required_v3_utf8(&files, &pattern.file)?;
        declared.insert(pattern.file.clone());
        let document = BuilderDocument::from_json(source.as_bytes())
            .map_err(|_| V2ZipError::BadRequest("Theme Pattern JSON không hợp lệ."))?;
        document
            .validate_for_save(&RegistryCatalog::<BlockDescriptor>::new(), None)
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

/// Returns `true` if the ZIP contains `theme.json` (v2), `false` if v1 legacy (`block.json`).
/// Rejects ambiguous packages containing both manifests.
pub fn is_v2_theme_zip(bytes: &[u8]) -> Result<bool, V2ZipError> {
    let mut archive = open_bounded_zip(
        bytes,
        V2_MAX_ZIP_BYTES,
        "Gói theme ZIP vượt quá giới hạn kích thước.",
        "ZIP không hợp lệ.",
    )
    .map_err(|msg| {
        if bytes.len() > V2_MAX_ZIP_BYTES {
            V2ZipError::PayloadTooLarge(msg)
        } else {
            V2ZipError::BadRequest(msg)
        }
    })?;

    let mut has_v2 = false;
    let mut has_v1 = false;
    for idx in 0..archive.len() {
        let entry = archive
            .by_index(idx)
            .map_err(|_| V2ZipError::BadRequest("Không đọc được mục trong ZIP."))?;
        if let Ok(name) = entry.name() {
            if name == "theme.json" {
                has_v2 = true;
            } else if name == "block.json" {
                has_v1 = true;
            }
        }
    }
    if has_v2 && has_v1 {
        return Err(V2ZipError::BadRequest(
            "Gói theme không được chứa đồng thời theme.json (v2) và block.json (v1).",
        ));
    }
    Ok(has_v2)
}

fn v2_entry_cap_and_allowlist(rel_path: &str) -> Result<usize, &'static str> {
    let ext = rel_path
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "json" | "html" | "htm" | "jinja" | "jinja2" | "css" | "txt" | "md" => {
            Ok(V2_MAX_TEXT_BYTES)
        }
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "woff2" => Ok(V2_MAX_RASTER_BYTES),
        _ => Err("Loại file trong gói theme v2 không nằm trong danh sách cho phép."),
    }
}

pub fn parse_theme_zip_v2(bytes: &[u8]) -> Result<ThemeV2Package, V2ZipError> {
    let files = read_v2_zip_entries(bytes, v2_entry_cap_and_allowlist)?;
    let raw_manifest = files.get("theme.json").ok_or(V2ZipError::BadRequest(
        "Thiếu theme.json trong gói theme v2.",
    ))?;
    let manifest_json = std::str::from_utf8(raw_manifest)
        .map_err(|_| V2ZipError::BadRequest("theme.json phải là UTF-8 hợp lệ."))?
        .to_string();
    let manifest: ThemeV2Manifest = serde_json::from_str(&manifest_json)
        .map_err(|_| V2ZipError::BadRequest("theme.json không đúng cấu trúc v2."))?;

    if manifest.manifest_version != 2 {
        return Err(V2ZipError::BadRequest("manifest_version phải bằng 2."));
    }
    if let Some(extensions) = &manifest.admin_extensions {
        extensions
            .validate("theme", &[])
            .map_err(V2ZipError::BadRequest)?;
    }
    if !valid_v2_theme_id(&manifest.theme_id) {
        return Err(V2ZipError::BadRequest(
            "theme_id v2 phải là lowercase kebab-case (1–64 ký tự) và không dùng tiền tố legacy.",
        ));
    }
    if manifest.name.trim().is_empty() || manifest.name.len() > 128 {
        return Err(V2ZipError::BadRequest("Tên theme v2 không hợp lệ."));
    }
    if manifest.version.trim().is_empty()
        || manifest.version.len() > 64
        || !manifest.version.is_ascii()
    {
        return Err(V2ZipError::BadRequest("Phiên bản theme v2 không hợp lệ."));
    }

    for (role, tpl_path) in &manifest.templates {
        if !ALLOWED_V2_ROLES.contains(&role.as_str()) {
            return Err(V2ZipError::BadRequest(
                "Vai trò template trong theme.json không hợp lệ.",
            ));
        }
        let safe_tpl = validate_v2_relative_path(tpl_path).map_err(V2ZipError::BadRequest)?;
        let tpl_bytes = files.get(&safe_tpl).ok_or(V2ZipError::BadRequest(
            "File template khai báo trong theme.json không tồn tại trong ZIP.",
        ))?;
        let tpl_src = std::str::from_utf8(tpl_bytes)
            .map_err(|_| V2ZipError::BadRequest("Template phải là UTF-8 hợp lệ."))?;
        if tpl_src.trim().is_empty() {
            return Err(V2ZipError::BadRequest(
                "Cú pháp template MiniJinja không hợp lệ.",
            ));
        }
        crate::template_sec::validate_template_csec(role, tpl_src)
            .map_err(V2ZipError::BadRequest)?;
    }

    for (rel_path, content) in &files {
        if rel_path.to_ascii_lowercase().ends_with(".css") {
            let css_src = std::str::from_utf8(content)
                .map_err(|_| V2ZipError::BadRequest("File CSS phải là UTF-8 hợp lệ."))?;
            crate::template_sec::validate_safe_css(css_src).map_err(V2ZipError::BadRequest)?;
        }
    }

    Ok(ThemeV2Package {
        manifest,
        manifest_json,
        files,
    })
}

pub fn manifest_has_declarations(manifest_json: &str) -> bool {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(manifest_json) else {
        return false;
    };
    let has_decl = val.get("declarations").is_some_and(|decl| {
        !decl.is_null()
            && !matches!(decl, serde_json::Value::Object(m) if m.is_empty())
            && !matches!(decl, serde_json::Value::Array(a) if a.is_empty())
    });
    let has_top_fg = val.get("field_groups").is_some_and(|fg| {
        !fg.is_null() && !matches!(fg, serde_json::Value::Array(a) if a.is_empty())
    });
    has_decl || has_top_fg
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

/// Parses and validates `declarations` (and optional top-level `field_groups`) from a v2 theme manifest.
/// Rejects unknown fields, invalid CPT/field schemas, duplicate keys, and any
/// undeclared taxonomy associations and duplicate keys.
pub fn parse_theme_declarations(manifest_json: &str) -> Result<ThemeDeclarations, &'static str> {
    let val: serde_json::Value =
        serde_json::from_str(manifest_json).map_err(|_| "Manifest theme v2 không hợp lệ.")?;
    let mut raw_decl = match val.get("declarations").cloned() {
        Some(v) if !v.is_null() => v,
        _ => serde_json::json!({}),
    };
    if let Some(top_fg) = val.get("field_groups").cloned() {
        if !top_fg.is_null() {
            let Some(obj) = raw_decl.as_object_mut() else {
                return Err("Khai báo declarations trong theme.json không đúng định dạng.");
            };
            if obj.contains_key("field_groups") {
                return Err(
                    "Không được khai báo trùng field_groups ở cả cấp gốc và trong declarations của theme.json.",
                );
            }
            obj.insert("field_groups".into(), top_fg);
        }
    }
    let parsed: ThemeDeclarations = serde_json::from_value(raw_decl)
        .map_err(|_| "Khai báo declarations trong theme.json không đúng định dạng.")?;
    let mut seen_keys = std::collections::HashSet::new();
    let declared_taxonomies: std::collections::HashSet<&str> = parsed
        .taxonomy_definitions
        .iter()
        .map(|tax| tax.key.as_str())
        .collect();
    if declared_taxonomies.len() != parsed.taxonomy_definitions.len() {
        return Err("Theme khai báo taxonomy trùng key.");
    }
    for tax in &parsed.taxonomy_definitions {
        opendash_core::content::TaxonomyDefinitionInput {
            key: tax.key.clone(),
            label: tax.label.clone(),
            hierarchical: tax.hierarchical,
            public: tax.public,
        }
        .validate()?;
    }
    for ct in &parsed.content_types {
        if !seen_keys.insert(ct.key.as_str()) {
            return Err("Khóa content_type bị trùng lặp trong declarations.");
        }
        let mut settings = ct.settings.clone().unwrap_or_default();
        settings.validate_and_normalize()?;
        let def = opendash_core::content::ContentTypeDefinition {
            key: ct.key.clone(),
            label: ct.label.clone(),
            fields: ct.fields.clone(),
            settings,
        };
        def.validate()?;
        for tax in &ct.taxonomies {
            if !opendash_core::content::valid_content_type_key(tax) {
                return Err("Theme taxonomy eligibility key không hợp lệ.");
            }
        }
    }
    for elig in &parsed.taxonomy_eligibility {
        if !opendash_core::content::valid_content_type_key(&elig.content_type) {
            return Err("Khóa content_type trong taxonomy_eligibility không hợp lệ.");
        }
        for tax in &elig.taxonomies {
            if !opendash_core::content::valid_content_type_key(tax) {
                return Err("Theme taxonomy eligibility key không hợp lệ.");
            }
        }
    }
    if parsed.field_groups.len() > 16 {
        return Err("Theme chỉ được khai báo tối đa 16 field_groups.");
    }
    let mut seen_groups = std::collections::HashSet::new();
    for fg in &parsed.field_groups {
        if !seen_groups.insert(fg.local_key.as_str()) {
            return Err(
                "duplicate_panel_id: Trùng local_key trong danh sách field_groups của theme.",
            );
        }
        fg.validate_and_normalize()?;
    }
    Ok(parsed)
}

pub fn theme_asset_mime(rel_path: &str) -> Option<&'static str> {
    let ext = rel_path
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())?;
    match ext.as_str() {
        "css" => Some("text/css; charset=utf-8"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "woff2" => Some("font/woff2"),
        _ => None,
    }
}

pub fn theme_asset_url(revision_digest: &str, rel_path: &str) -> String {
    format!("/theme/{revision_digest}/{rel_path}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    fn build_zip(entries: &[(&str, &[u8], CompressionMethod)]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zw = ZipWriter::new(std::io::Cursor::new(&mut buf));
            for (name, data, method) in entries {
                let opts = SimpleFileOptions::default().compression_method(*method);
                zw.start_file(*name, opts).unwrap();
                zw.write_all(data).unwrap();
            }
            zw.finish().unwrap();
        }
        buf
    }

    /// D01 test fixture: all ZIP entries, including empty directory entries,
    /// pass through the production bounded reader before manifest validation.
    fn build_v3_zip(manifest: serde_json::Value, entries: &[(&str, &[u8])]) -> Vec<u8> {
        let manifest_json = serde_json::to_vec(&manifest).unwrap();
        let mut buf = Vec::new();
        {
            let mut zw = ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            zw.start_file("theme.json", opts).unwrap();
            zw.write_all(&manifest_json).unwrap();
            for (path, bytes) in entries {
                if path.ends_with('/') {
                    zw.add_directory(*path, opts).unwrap();
                } else {
                    zw.start_file(*path, opts).unwrap();
                    zw.write_all(bytes).unwrap();
                }
            }
            zw.finish().unwrap();
        }
        buf
    }

    fn minimal_v3_manifest() -> serde_json::Value {
        serde_json::json!({
            "manifest_version": 3,
            "theme_id": "sample-theme",
            "name": "Sample",
            "version": "1.0.0"
        })
    }

    #[test]
    fn parses_valid_v2_theme_and_detects_declarations() {
        let manifest = br#"{
            "manifest_version": 2,
            "theme_id": "editorial-clean",
            "name": "Editorial Clean",
            "version": "1.0.0",
            "templates": { "single": "templates/single.html" },
            "declarations": { "content_types": [{"key": "portfolio"}] }
        }"#;
        let zip = build_zip(&[
            ("theme.json", manifest, CompressionMethod::Deflated),
            (
                "templates/single.html",
                b"<article>{{ entry.title }}</article>",
                CompressionMethod::Deflated,
            ),
            (
                "assets/style.css",
                b"body { color: #111; }",
                CompressionMethod::Deflated,
            ),
        ]);
        assert!(is_v2_theme_zip(&zip).unwrap());
        let pkg = parse_theme_zip_v2(&zip).unwrap();
        assert_eq!(pkg.manifest.theme_id, "editorial-clean");
        assert!(manifest_has_declarations(&pkg.manifest_json));
        assert_eq!(
            theme_asset_mime("assets/style.css"),
            Some("text/css; charset=utf-8")
        );
        assert_eq!(theme_asset_mime("theme.json"), None);
    }

    #[test]
    fn parses_v3_theme_templates_parts_layouts_and_pattern_document() {
        let manifest = br#"{
          "manifest_version": 3,
          "theme_id": "sample-theme", "name": "Sample", "version": "1.0.0",
          "templates": [
            {"kind": "index", "file": "index.html"},
            {"kind": "page", "file": "page.html"}
          ],
          "parts": {"header": "header.html"},
          "page_layouts": [{"key":"canvas", "label":"Canvas", "file":"page-layouts/canvas.html"}],
          "patterns": [{"key":"hero", "title":"Hero", "category":"banner", "file":"patterns/hero.json", "preview_asset":"assets/hero.png"}],
          "assets": ["assets/hero.png"]
        }"#;
        let zip = build_zip(&[
            ("theme.json", manifest, CompressionMethod::Stored),
            (
                "index.html",
                b"<main>{{ od_content() }}</main>",
                CompressionMethod::Stored,
            ),
            (
                "page.html",
                b"<main>{{ od_part(\"header\") }}{{ od_content() }}</main>",
                CompressionMethod::Stored,
            ),
            (
                "header.html",
                b"<header>Header</header>",
                CompressionMethod::Stored,
            ),
            (
                "page-layouts/canvas.html",
                b"<main>{{ od_content() }}</main>",
                CompressionMethod::Stored,
            ),
            (
                "patterns/hero.json",
                br#"{"schema_version":1,"sections":[]}"#,
                CompressionMethod::Stored,
            ),
            (
                "assets/hero.png",
                b"\x89PNG\r\n\x1a\n",
                CompressionMethod::Stored,
            ),
        ]);
        let package = parse_theme_zip_v3(&zip).unwrap();
        assert_eq!(package.manifest.theme_id, "sample-theme");
        assert_eq!(package.files.len(), 7);
        assert!(package.manifest_json.contains("manifest_version"));
    }

    #[test]
    fn v3_d01_accepts_exact_declared_assets_and_raster_preview() {
        let mut manifest = minimal_v3_manifest();
        manifest["assets"] = serde_json::json!([
            "assets/styles.css",
            "assets/images/hero.webp",
            "assets/fonts/body.woff2"
        ]);
        manifest["patterns"] = serde_json::json!([{
            "key": "hero", "title": "Hero", "category": "banner",
            "file": "patterns/hero.json", "preview_asset": "assets/images/hero.webp"
        }]);
        let entries: &[(&str, &[u8])] = &[
            (
                "patterns/hero.json",
                br#"{"schema_version":1,"sections":[]}"#,
            ),
            ("assets/styles.css", b"body { color: #123; }"),
            ("assets/images/hero.webp", b"RIFFdemoWEBP"),
            ("assets/fonts/body.woff2", b"woff2"),
        ];
        let pkg = parse_theme_zip_v3(&build_v3_zip(manifest, entries)).unwrap();
        assert_eq!(pkg.manifest.assets.len(), 3);
        assert_eq!(pkg.files.len(), 5);
    }

    #[test]
    fn v3_d01_rejects_missing_undeclared_and_duplicate_assets() {
        let manifest = minimal_v3_manifest();
        let assets = &[("assets/extra.png", b"image".as_slice())];
        assert!(matches!(
            parse_theme_zip_v3(&build_v3_zip(manifest.clone(), assets)),
            Err(V2ZipError::BadRequest(_))
        ));

        let mut missing = manifest.clone();
        missing["assets"] = serde_json::json!(["assets/missing.png"]);
        assert!(matches!(
            parse_theme_zip_v3(&build_v3_zip(missing, &[])),
            Err(V2ZipError::BadRequest(_))
        ));

        let mut declared = manifest.clone();
        declared["assets"] = serde_json::json!(["assets/extra.png"]);
        // ZipWriter refuses duplicate filenames itself. Build a structurally
        // valid archive with two distinct equal-width names, then substitute
        // the second filename in its local and central directory headers.
        // CRC covers entry contents, not filename bytes.
        let mut duplicate_zip = build_v3_zip(
            declared.clone(),
            &[("assets/extra.png", b"a"), ("assets/other.png", b"b")],
        );
        // Prove the original ZIP is structurally valid and both distinctly
        // named regular members are readable before rewriting its headers.
        let mut original = zip::ZipArchive::new(std::io::Cursor::new(duplicate_zip.as_slice()))
            .expect("pre-rewrite ZIP must open");
        assert_eq!(original.len(), 3);
        for (index, expected_name, expected_bytes) in [
            (1, "assets/extra.png", b"a".as_slice()),
            (2, "assets/other.png", b"b".as_slice()),
        ] {
            let mut member = original
                .by_index(index)
                .expect("regular ZIP member must open");
            assert_eq!(
                member.name().expect("pre-rewrite ZIP member name").as_ref(),
                expected_name
            );
            let mut decoded = Vec::new();
            member
                .read_to_end(&mut decoded)
                .expect("pre-rewrite ZIP member must be readable");
            assert_eq!(decoded, expected_bytes);
        }
        drop(original);
        let from = b"assets/other.png";
        let to = b"assets/extra.png";
        assert_eq!(from.len(), to.len());
        let mut replaced = 0_usize;
        for idx in 0..=(duplicate_zip.len() - from.len()) {
            if duplicate_zip[idx..idx + from.len()] == from[..] {
                duplicate_zip[idx..idx + from.len()].copy_from_slice(to);
                replaced += 1;
            }
        }
        assert_eq!(
            replaced, 2,
            "ZIP local and central names must both be rewritten"
        );
        let duplicate_result = parse_theme_zip_v3(&duplicate_zip);
        assert!(
            matches!(&duplicate_result, Err(V2ZipError::BadRequest(_))),
            "duplicate regular ZIP member must be rejected (any safe layer): {duplicate_result:?}"
        );
        declared["assets"] = serde_json::json!(["assets/extra.png", "assets/extra.png"]);
        assert!(matches!(
            parse_theme_zip_v3(&build_v3_zip(declared, assets)),
            Err(V2ZipError::BadRequest(_))
        ));
    }

    #[test]
    fn v3_d01_rejects_hidden_traversal_unsafe_extension_and_long_paths() {
        let manifest = minimal_v3_manifest();
        let too_long = format!("assets/{}.png", "a".repeat(237));
        for path in [
            "assets/.private/hidden.png",
            "assets/icons.v2/logo.png",
            "assets/a..b.png",
            "assets/../escape.png",
            "../escape.png",
            "assets\\escape.png",
            "assets/evil.svg",
            "assets/evil.js",
            "assets/evil.html",
            "assets/images/.secret.png",
            too_long.as_str(),
        ] {
            let zip = build_v3_zip(manifest.clone(), &[(path, b"not safe")]);
            assert!(
                matches!(parse_theme_zip_v3(&zip), Err(V2ZipError::BadRequest(_))),
                "unexpected acceptance: {path}"
            );
        }
    }

    #[test]
    fn v3_d01_ignores_safe_empty_directories_but_rejects_hidden_children() {
        let manifest = minimal_v3_manifest();
        let empty_dirs = &[
            ("assets/", b"".as_slice()),
            ("assets/images/", b""),
            ("assets/.hidden/", b""),
        ];
        let package = parse_theme_zip_v3(&build_v3_zip(manifest.clone(), empty_dirs)).unwrap();
        assert_eq!(package.files.len(), 1);
        let hidden_file = &[
            ("assets/.hidden/", b"".as_slice()),
            ("assets/.hidden/secret.png", b"hidden"),
        ];
        assert!(matches!(
            parse_theme_zip_v3(&build_v3_zip(manifest, hidden_file)),
            Err(V2ZipError::BadRequest(_))
        ));
    }

    #[test]
    fn v3_d01_rejects_invalid_preview_css_and_oversized_assets() {
        let mut manifest = minimal_v3_manifest();
        manifest["assets"] = serde_json::json!(["assets/styles.css"]);
        manifest["patterns"] = serde_json::json!([{
            "key": "hero", "title": "Hero", "category": "banner",
            "file": "patterns/hero.json", "preview_asset": "assets/styles.css"
        }]);
        let entries: &[(&str, &[u8])] = &[
            (
                "patterns/hero.json",
                br#"{"schema_version":1,"sections":[]}"#,
            ),
            ("assets/styles.css", b"body{}"),
        ];
        assert!(matches!(
            parse_theme_zip_v3(&build_v3_zip(manifest.clone(), entries)),
            Err(V2ZipError::BadRequest(_))
        ));

        manifest.as_object_mut().unwrap().remove("patterns");
        let unsafe_css = &[(
            "assets/styles.css",
            b"@import url(https://evil.test/a.css);".as_slice(),
        )];
        assert!(matches!(
            parse_theme_zip_v3(&build_v3_zip(manifest.clone(), unsafe_css)),
            Err(V2ZipError::BadRequest(_))
        ));

        let giant_css = vec![b'a'; V2_MAX_TEXT_BYTES + 1];
        let giant_zip = build_v3_zip(manifest, &[("assets/styles.css", &giant_css)]);
        assert!(matches!(
            parse_theme_zip_v3(&giant_zip),
            Err(V2ZipError::PayloadTooLarge(_))
        ));
    }

    #[test]
    fn v3_d01_rejects_malformed_declarations_and_field_groups() {
        let manifest = minimal_v3_manifest();
        for bad in [
            serde_json::json!({"declarations":{"content_types":"not an array"}}),
            serde_json::json!({"field_groups":"not an array"}),
            serde_json::json!({"declarations":{"field_groups":[]},"field_groups":[]}),
        ] {
            let mut variant = manifest.clone();
            variant
                .as_object_mut()
                .unwrap()
                .extend(bad.as_object().unwrap().clone());
            assert!(matches!(
                parse_theme_zip_v3(&build_v3_zip(variant, &[])),
                Err(V2ZipError::BadRequest(_))
            ));
        }
    }

    #[test]
    fn rejects_v3_undeclared_templates_and_unsafe_archive_paths() {
        let manifest = br#"{
          "manifest_version":3, "theme_id":"sample-theme", "name":"Sample", "version":"1.0.0",
          "templates":[{"kind":"index", "file":"index.html"}]
        }"#;
        for unwanted in [
            "unlisted.html",
            "../escape.html",
            "assets/inject.js",
            "assets/.private/hidden.png",
            "unknown/foo.html",
        ] {
            let zip = build_zip(&[
                ("theme.json", manifest, CompressionMethod::Stored),
                (
                    "index.html",
                    b"<main>Static</main>",
                    CompressionMethod::Stored,
                ),
                (unwanted, b"should be rejected", CompressionMethod::Stored),
            ]);
            assert!(
                matches!(parse_theme_zip_v3(&zip), Err(V2ZipError::BadRequest(_))),
                "{unwanted}"
            );
        }
    }

    #[test]
    fn rejects_v3_untrusted_template_and_pattern_content() {
        let manifest = br#"{
          "manifest_version":3, "theme_id":"sample-theme", "name":"Sample", "version":"1.0.0",
          "templates":[{"kind":"page", "file":"page.html"}]
        }"#;
        for template in [
            b"<main><script>alert(1)</script>{{ od_content() }}</main>".as_slice(),
            b"<main>{% include 'evil.html' %}{{ od_content() }}</main>".as_slice(),
            b"<main>{{ od_content() }}{{ od_content() }}</main>".as_slice(),
            b"<div title=\"{{ od_content() }}\"></div>".as_slice(),
        ] {
            let zip = build_zip(&[
                ("theme.json", manifest, CompressionMethod::Stored),
                ("page.html", template, CompressionMethod::Stored),
            ]);
            assert!(matches!(
                parse_theme_zip_v3(&zip),
                Err(V2ZipError::BadRequest(_))
            ));
        }

        let manifest_with_pattern = br#"{
          "manifest_version":3, "theme_id":"sample-theme", "name":"Sample", "version":"1.0.0",
          "patterns":[{"key":"hero", "title":"Hero", "category":"banner", "file":"patterns/hero.json"}]
        }"#;
        let zip = build_zip(&[
            ("theme.json", manifest_with_pattern, CompressionMethod::Stored),
            ("patterns/hero.json", br#"{"schema_version":1,"sections":[{"id":"broken","children":[{"id":"broken","children":[]}]}]}"#, CompressionMethod::Stored),
        ]);
        assert!(matches!(
            parse_theme_zip_v3(&zip),
            Err(V2ZipError::BadRequest(_))
        ));
    }

    #[test]
    fn rejects_malicious_v2_zip_corpus() {
        let manifest =
            br#"{"manifest_version":2,"theme_id":"bad-theme","name":"Bad","version":"1.0.0"}"#;

        let traversal = build_zip(&[
            ("theme.json", manifest, CompressionMethod::Stored),
            ("../escape.css", b"body{}", CompressionMethod::Stored),
        ]);
        assert!(matches!(
            parse_theme_zip_v2(&traversal),
            Err(V2ZipError::BadRequest(_))
        ));

        let backslash = build_zip(&[
            ("theme.json", manifest, CompressionMethod::Stored),
            ("assets\\style.css", b"body{}", CompressionMethod::Stored),
        ]);
        assert!(matches!(
            parse_theme_zip_v2(&backslash),
            Err(V2ZipError::BadRequest(_))
        ));

        let collision = build_zip(&[
            ("theme.json", manifest, CompressionMethod::Stored),
            ("assets/app.css", b"a{}", CompressionMethod::Stored),
            ("Assets/App.css", b"b{}", CompressionMethod::Stored),
        ]);
        assert!(matches!(
            parse_theme_zip_v2(&collision),
            Err(V2ZipError::BadRequest(_))
        ));

        let js = build_zip(&[
            ("theme.json", manifest, CompressionMethod::Stored),
            ("assets/evil.js", b"alert(1)", CompressionMethod::Stored),
        ]);
        assert!(matches!(
            parse_theme_zip_v2(&js),
            Err(V2ZipError::BadRequest(_))
        ));

        let big_css = vec![b'a'; V2_MAX_TEXT_BYTES + 1];
        let oversized = build_zip(&[
            ("theme.json", manifest, CompressionMethod::Stored),
            ("assets/big.css", &big_css, CompressionMethod::Stored),
        ]);
        assert!(matches!(
            parse_theme_zip_v2(&oversized),
            Err(V2ZipError::PayloadTooLarge(_))
        ));

        let bomb_payload = vec![b'0'; 200 * 1024];
        let bomb = build_zip(&[
            ("theme.json", manifest, CompressionMethod::Deflated),
            (
                "assets/bomb.css",
                &bomb_payload,
                CompressionMethod::Deflated,
            ),
        ]);
        assert!(matches!(
            parse_theme_zip_v2(&bomb),
            Err(V2ZipError::BadRequest(_))
        ));
    }
}
