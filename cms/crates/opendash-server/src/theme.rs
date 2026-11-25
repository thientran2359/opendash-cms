use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read as _,
    path::Path,
};

pub use opendash_core::theme::{Block, BlockDescriptor, Fields};
use opendash_core::{
    block_schema::{BlockSchemaV1, RegisteredBlockSchema},
    builder::BuilderDocument,
    id::valid_kebab_segment_with_limit,
    registry::{DeclarationState, OwnedDescriptor, ProviderStatus, RegistryCatalog, ResourceOwner},
    theme::{TemplateCandidateKind, ThemeV3Manifest, valid_asset_path},
};
use serde::{Deserialize, Serialize};

#[cfg(test)]
use crate::package::open_bounded_zip;
use crate::package::{
    V2_MAX_DECODED_BYTES, V2_MAX_FILES, V2_MAX_RASTER_BYTES, V2_MAX_TEXT_BYTES, V2_MAX_ZIP_BYTES,
    V2ZipError, read_v2_zip_entries, validate_v2_relative_path,
};
#[cfg(test)]
use crate::storage::theme::valid_v2_theme_id;

pub const MAX_ZIP_BYTES: usize = 64 * 1024;

pub struct Theme {
    pub descriptor: BlockDescriptor,
    pub template: String,
    pub content_list_template: Option<String>,
    pub content_single_template: Option<String>,
    /// Explicitly supported block from another installed theme; never rewrites home_page.
    pub compatible_block_id: Option<String>,
}

#[cfg(test)]
mod s03_package_block_tests {
    use super::*;

    fn block_schema(owner: &str, slug: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "schema_version": 1, "id": format!("{owner}/{slug}"),
            "title": "Banner", "category": "content", "render": "render.html", "fields": []
        }))
        .unwrap()
    }

    fn file_map() -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([
            ("theme.json".into(), br#"{"manifest_version":3,"theme_id":"sample-theme","name":"Sample","version":"1.0.0"}"#.to_vec()),
            ("blocks/banner/block.json".into(), block_schema("sample-theme", "banner")),
            ("blocks/banner/render.html".into(), b"<p>Hello</p>".to_vec()),
        ])
    }

    #[test]
    fn v3_discovers_valid_blocks_from_unlisted_manifest_paths() {
        let parsed = validate_theme_v3_files(file_map()).unwrap();
        assert_eq!(parsed.manifest.theme_id, "sample-theme");
    }

    #[test]
    fn v3_pattern_can_reference_a_block_from_same_theme() {
        let mut files = file_map();
        files.insert("theme.json".into(), br#"{"manifest_version":3,"theme_id":"sample-theme","name":"Sample","version":"1.0.0","patterns":[{"key":"notice","title":"Notice","category":"banner","file":"patterns/notice.json"}]}"#.to_vec());
        let pattern = serde_json::json!({
            "schema_version":1,
            "sections":[{"id":"section-one","children":[{"id":"row-one","children":[{
                "id":"column-one","children":[{
                    "id":"element-one","type":"plugin/block",
                    "props":{"block_id":"sample-theme/banner","values":{}}
                }]
            }]}]}]
        });
        files.insert(
            "patterns/notice.json".into(),
            serde_json::to_vec(&pattern).unwrap(),
        );
        assert!(validate_theme_v3_files(files.clone()).is_ok());

        let mut unknown = pattern;
        unknown["sections"][0]["children"][0]["children"][0]["children"][0]["props"]["block_id"] =
            "other-theme/banner".into();
        files.insert(
            "patterns/notice.json".into(),
            serde_json::to_vec(&unknown).unwrap(),
        );
        assert!(validate_theme_v3_files(files).is_err());
    }

    #[test]
    fn v3_rejects_unpaired_blocks_wrong_slug_and_unsafe_csec() {
        let mut files = file_map();
        files.remove("blocks/banner/render.html");
        assert!(validate_theme_v3_files(files).is_err());

        let mut files = file_map();
        files.insert(
            "blocks/banner/block.json".into(),
            block_schema("sample-theme", "other"),
        );
        assert!(validate_theme_v3_files(files).is_err());

        let mut files = file_map();
        files.insert(
            "blocks/banner/render.html".into(),
            b"<script>alert(1)</script>".to_vec(),
        );
        assert!(validate_theme_v3_files(files).is_err());

        let mut files = file_map();
        files.insert("blocks/Banner/render.html".into(), b"<p>Bad</p>".to_vec());
        assert!(validate_theme_v3_files(files).is_err());
    }

    #[test]
    fn v3_rejects_over_32_blocks_and_invalid_owner() {
        let mut files = file_map();
        for i in 0..33 {
            let slug = format!("notice-{i}");
            files.insert(
                format!("blocks/{slug}/block.json"),
                block_schema("sample-theme", &slug),
            );
            files.insert(format!("blocks/{slug}/render.html"), b"<p>Hi</p>".to_vec());
        }
        assert!(validate_theme_v3_files(files).is_err());

        let mut files = file_map();
        files.insert("theme.json".into(), br#"{"manifest_version":3,"theme_id":"abcdefghijklmnopqrstuvwxyz-abcde","name":"Sample","version":"1.0.0"}"#.to_vec());
        assert!(validate_theme_v3_files(files).is_err());
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
    render_content_with_config(
        template,
        data,
        &crate::template_sec::RenderFormattingConfig::default(),
    )
}

pub fn render_content_with_config(
    template: &str,
    data: &serde_json::Value,
    fmt_cfg: &crate::template_sec::RenderFormattingConfig,
) -> Result<String, &'static str> {
    let context = crate::template_sec::json_to_safe_value(data);
    crate::template_sec::render_csec_with_config("content", template, context, fmt_cfg)
        .map_err(|_| "Không thể render template nội dung.")
}

#[cfg(test)]
const ALLOWED_V2_ROLES: &[&str] = &[
    "home",
    "page",
    "header",
    "footer",
    "archive",
    "single",
    "taxonomy_archive",
];

#[cfg(test)]
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

#[cfg(test)]
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
    if let Some(rest) = rel_path.strip_prefix("blocks/") {
        if let Some((slug, leaf)) = rest.split_once('/')
            && valid_kebab_segment_with_limit(slug, 32)
            && matches!(leaf, "block.json" | "render.html")
        {
            return Ok(V2_MAX_TEXT_BYTES);
        }
        return Err("Đường dẫn blocks/<slug>/ của theme v3 không hợp lệ.");
    }
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
        "patterns" | "locales" if is_json => Ok(V2_MAX_TEXT_BYTES),
        _ if rel_path.starts_with("assets/") => {
            if !valid_asset_path(rel_path) {
                return Err("Đường dẫn assets theme v3 không đúng schema D01.");
            }
            let ext = name.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
            match ext {
                "css" | "txt" | "js" | "mjs" | "svg" => Ok(V2_MAX_TEXT_BYTES),
                "png" | "jpg" | "jpeg" | "webp" | "gif" | "woff2" => Ok(V2_MAX_RASTER_BYTES),
                _ => Err("Định dạng assets theme v3 không được cho phép."),
            }
        }
        _ => Err("Gói theme v3 chứa file ngoài root hoặc bốn thư mục được phép."),
    }
}

/// Decode strict, paired declarative blocks from a validated package file map.
/// This is shared by theme and plugin ingestion, and by theme activation after
/// checking the immutable revision's on-disk file digests.
pub(crate) fn collect_package_blocks(
    files: &BTreeMap<String, Vec<u8>>,
    owner_id: &str,
) -> Result<Vec<(BlockSchemaV1, String)>, &'static str> {
    let mut slugs = BTreeSet::new();
    for path in files.keys().filter(|path| path.starts_with("blocks/")) {
        let Some(rest) = path.strip_prefix("blocks/") else {
            unreachable!()
        };
        let Some((slug, leaf)) = rest.split_once('/') else {
            return Err("Đường dẫn block trong gói không hợp lệ.");
        };
        if !valid_kebab_segment_with_limit(slug, 32)
            || !matches!(leaf, "block.json" | "render.html")
        {
            return Err("Gói chứa đường dẫn block ngoài hợp đồng.");
        }
        slugs.insert(slug.to_owned());
    }
    if slugs.len() > 32 {
        return Err("Mỗi gói chỉ được khai báo tối đa 32 blocks.");
    }
    if !slugs.is_empty() && !valid_kebab_segment_with_limit(owner_id, 32) {
        return Err("Package ID khai báo block phải là kebab-case tối đa 32 ký tự.");
    }

    let mut blocks = Vec::with_capacity(slugs.len());
    for slug in slugs {
        let schema_path = format!("blocks/{slug}/block.json");
        let template_path = format!("blocks/{slug}/render.html");
        let raw_schema =
            std::str::from_utf8(files.get(&schema_path).ok_or("Block thiếu block.json.")?)
                .map_err(|_| "block.json phải là UTF-8 hợp lệ.")?;
        let template = std::str::from_utf8(
            files
                .get(&template_path)
                .ok_or("Block thiếu render.html.")?,
        )
        .map_err(|_| "render.html phải là UTF-8 hợp lệ.")?;
        let schema: BlockSchemaV1 =
            serde_json::from_str(raw_schema).map_err(|_| "block.json không đúng schema v1.")?;
        schema.validate(owner_id)?;
        if schema.id != format!("{owner_id}/{slug}") {
            return Err("ID block không khớp thư mục blocks/<slug>.");
        }
        crate::template_sec::validate_block_template_csec(&schema.id, template, &schema)?;
        blocks.push((schema, template.to_owned()));
    }
    Ok(blocks)
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

pub(crate) const MAX_THEME_LOCALE_FILES: usize = 32;
pub(crate) const MAX_THEME_LOCALE_KEYS: usize = 512;
pub(crate) const MAX_THEME_LOCALE_VALUE_CHARS: usize = 500;

/// Parses and validates a flat theme dictionary `locales/<code>.json`.
pub(crate) fn parse_theme_locale_json(
    raw: &[u8],
) -> Result<BTreeMap<String, String>, &'static str> {
    let text = std::str::from_utf8(raw).map_err(|_| "File từ điển locale phải là UTF-8 hợp lệ.")?;
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "File từ điển locale không phải JSON hợp lệ.")?;
    let obj = value
        .as_object()
        .ok_or("File từ điển locale phải là JSON object phẳng.")?;
    if obj.len() > MAX_THEME_LOCALE_KEYS {
        return Err("File từ điển locale vượt quá giới hạn 512 khóa.");
    }
    let mut dict = BTreeMap::new();
    for (key, val) in obj {
        if !opendash_core::id::valid_snake_key_with_limit(key, 64) {
            return Err("Khóa trong từ điển locale phải là snake_case tối đa 64 ký tự.");
        }
        let s = val
            .as_str()
            .ok_or("Giá trị trong từ điển locale phải là chuỗi phẳng.")?;
        if s.chars().count() > MAX_THEME_LOCALE_VALUE_CHARS {
            return Err("Chuỗi dịch trong từ điển locale vượt quá 500 ký tự.");
        }
        if s.chars().any(|ch| ch.is_control()) {
            return Err("Chuỗi dịch trong từ điển locale không được chứa ký tự điều khiển.");
        }
        dict.insert(key.clone(), s.to_owned());
    }
    Ok(dict)
}

/// Read a v3 ZIP through the shared bounded, symlink-safe package reader,
/// then validate manifest paths, content AST and C-SEC before persistence.
pub fn parse_theme_zip_v3(bytes: &[u8]) -> Result<ThemeV3Package, V2ZipError> {
    let files = read_v2_zip_entries(bytes, v3_entry_cap_and_allowlist)?;
    validate_theme_v3_files(files)
}

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
        validate_v2_relative_path(path).map_err(V2ZipError::BadRequest)?;
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
                    legacy_mode: false,
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
        CsecContentKind, collect_theme_translation_keys,
        validate_and_compile_csec_composition_with_theme_urls,
    };
    let mut used_t_keys = BTreeSet::new();
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
        used_t_keys.extend(collect_theme_translation_keys(source).map_err(V2ZipError::BadRequest)?);
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
        used_t_keys.extend(collect_theme_translation_keys(source).map_err(V2ZipError::BadRequest)?);
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
        used_t_keys.extend(collect_theme_translation_keys(source).map_err(V2ZipError::BadRequest)?);
    }

    let mut locale_dicts = BTreeMap::new();
    let mut seen_lower_locale_paths = BTreeSet::new();
    for (path, raw) in &files {
        if let Some(rest) = path.strip_prefix("locales/") {
            let Some(code) = rest.strip_suffix(".json") else {
                return Err(V2ZipError::BadRequest(
                    "File trong locales/ của theme v3 phải có đuôi .json.",
                ));
            };
            if !seen_lower_locale_paths.insert(path.to_ascii_lowercase()) {
                return Err(V2ZipError::BadRequest(
                    "Theme v3 có file locales/*.json trùng tên hoa/thường.",
                ));
            }
            if code != code.to_ascii_lowercase()
                || !crate::storage::settings::valid_content_language(code)
            {
                return Err(V2ZipError::BadRequest(
                    "Mã ngôn ngữ trong locales/<code>.json không hợp lệ.",
                ));
            }
            if locale_dicts.len() >= MAX_THEME_LOCALE_FILES {
                return Err(V2ZipError::BadRequest(
                    "Theme v3 vượt quá giới hạn 32 file locales/*.json.",
                ));
            }
            let dict = parse_theme_locale_json(raw).map_err(V2ZipError::BadRequest)?;
            locale_dicts.insert(code.to_owned(), dict);
            declared.insert(path.clone());
        }
    }

    if !used_t_keys.is_empty() {
        let Some(def_locale) = manifest.default_locale.as_deref() else {
            return Err(V2ZipError::BadRequest(
                "Theme sử dụng khóa dịch t.<key> bắt buộc khai báo default_locale trong theme.json.",
            ));
        };
        let def_code = def_locale.to_ascii_lowercase();
        let Some(def_dict) = locale_dicts.get(&def_code) else {
            return Err(V2ZipError::BadRequest(
                "Theme thiếu file từ điển locales/<default_locale>.json.",
            ));
        };
        for key in &used_t_keys {
            if !def_dict.contains_key(key) {
                return Err(V2ZipError::BadRequest(
                    "Theme thiếu khóa dịch t.<key> trong từ điển default_locale.",
                ));
            }
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
}

fn zip_err_msg(err: V2ZipError) -> String {
    match err {
        V2ZipError::PayloadTooLarge(msg) | V2ZipError::BadRequest(msg) => msg.to_owned(),
    }
}

fn walk_v3_dir(
    dir: &Path,
    rel_prefix: &str,
    depth: usize,
    files: &mut BTreeMap<String, Vec<u8>>,
    seen_lower: &mut BTreeSet<String>,
    decoded_total: &mut usize,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("Không thể đọc thư mục '{}': {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Lỗi đọc mục thư mục theme: {e}"))?;
        let file_name = entry.file_name();
        let name = file_name.to_str().ok_or_else(|| {
            "Tên file hoặc thư mục trong theme không phải UTF-8 hợp lệ.".to_owned()
        })?;
        if name.starts_with('.') {
            return Err(format!("Không cho phép entry ẩn trong theme: {name}"));
        }
        let rel_path = if rel_prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{rel_prefix}/{name}")
        };
        let entry_path = entry.path();
        let meta = std::fs::symlink_metadata(&entry_path)
            .map_err(|e| format!("Không thể kiểm tra metadata '{rel_path}': {e}"))?;
        let ft = meta.file_type();
        if ft.is_symlink() {
            return Err(format!(
                "Không cho phép symlink bên trong theme: {rel_path}"
            ));
        }
        if ft.is_dir() {
            if depth >= MAX_THEME_DIR_DEPTH {
                return Err(format!(
                    "Thư mục theme vượt quá giới hạn độ sâu tối đa ({MAX_THEME_DIR_DEPTH}): {rel_path}"
                ));
            }
            if !seen_lower.insert(rel_path.to_ascii_lowercase()) {
                return Err(format!(
                    "Đường dẫn theme bị trùng lặp không phân biệt hoa thường: {rel_path}"
                ));
            }
            walk_v3_dir(
                &entry_path,
                &rel_path,
                depth + 1,
                files,
                seen_lower,
                decoded_total,
            )?;
        } else if ft.is_file() {
            if files.len() >= V2_MAX_FILES {
                return Err("Theme v3 vượt quá giới hạn số lượng file.".to_owned());
            }
            if !seen_lower.insert(rel_path.to_ascii_lowercase()) {
                return Err(format!(
                    "Đường dẫn theme bị trùng lặp không phân biệt hoa thường: {rel_path}"
                ));
            }
            if depth == 0 && name.eq_ignore_ascii_case("readme.md") {
                if meta.len() > V2_MAX_TEXT_BYTES as u64 {
                    return Err(format!(
                        "File '{rel_path}' vượt quá giới hạn kích thước ({V2_MAX_TEXT_BYTES} bytes)."
                    ));
                }
                continue;
            }
            validate_v2_relative_path(&rel_path).map_err(str::to_owned)?;
            let cap = v3_entry_cap_and_allowlist(&rel_path).map_err(str::to_owned)?;
            if meta.len() > cap as u64 {
                return Err(format!(
                    "File '{rel_path}' vượt quá giới hạn kích thước ({cap} bytes)."
                ));
            }
            let file = std::fs::File::open(&entry_path)
                .map_err(|e| format!("Không thể mở file '{rel_path}': {e}"))?;
            let mut data = Vec::with_capacity((meta.len() as usize).min(cap));
            file.take((cap + 1) as u64)
                .read_to_end(&mut data)
                .map_err(|e| format!("Không thể đọc file '{rel_path}': {e}"))?;
            if data.len() > cap {
                return Err(format!(
                    "File '{rel_path}' vượt quá giới hạn kích thước ({cap} bytes)."
                ));
            }
            *decoded_total = decoded_total.saturating_add(data.len());
            if *decoded_total > V2_MAX_DECODED_BYTES {
                return Err("Theme v3 vượt quá giới hạn tổng kích thước giải mã 32 MiB.".to_owned());
            }
            files.insert(rel_path, data);
        } else {
            return Err(format!(
                "Không cho phép entry không phải file thường trong theme: {rel_path}"
            ));
        }
    }
    Ok(())
}

pub fn load_v3_files_from_dir(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let meta = std::fs::metadata(root)
        .map_err(|e| format!("Không thể truy cập thư mục theme '{}': {e}", root.display()))?;
    if !meta.is_dir() {
        return Err(format!(
            "Đường dẫn '{}' không phải thư mục.",
            root.display()
        ));
    }
    let mut files = BTreeMap::new();
    let mut seen_lower = BTreeSet::new();
    let mut decoded_total = 0usize;
    walk_v3_dir(root, "", 0, &mut files, &mut seen_lower, &mut decoded_total)?;
    Ok(files)
}

fn build_theme_check_summary(pkg: &ThemeV3Package) -> Result<ThemeCheckSummary, String> {
    let mut templates: Vec<String> = pkg
        .manifest
        .templates
        .iter()
        .map(|t| t.canonical_key().unwrap_or_else(|_| t.file.clone()))
        .collect();
    templates.sort();

    let parts: Vec<String> = pkg.manifest.parts.keys().cloned().collect();

    let mut page_layouts: Vec<String> = pkg
        .manifest
        .page_layouts
        .iter()
        .map(|l| l.key.clone())
        .collect();
    page_layouts.sort();

    let mut patterns: Vec<String> = pkg
        .manifest
        .patterns
        .iter()
        .map(|p| p.key.clone())
        .collect();
    patterns.sort();

    let mut blocks: Vec<String> = collect_package_blocks(&pkg.files, &pkg.manifest.theme_id)
        .map_err(str::to_owned)?
        .into_iter()
        .map(|(schema, _)| schema.id)
        .collect();
    blocks.sort();

    let mut assets = pkg.manifest.assets.clone();
    assets.sort();

    let options_count = crate::storage::settings::parse_theme_options_schema(&pkg.manifest_json)
        .map_err(str::to_owned)?
        .len();

    Ok(ThemeCheckSummary {
        ok: true,
        theme_id: pkg.manifest.theme_id.clone(),
        version: pkg.manifest.version.clone(),
        templates,
        parts,
        page_layouts,
        patterns,
        blocks,
        assets,
        options_count,
    })
}

pub fn check_theme_path(path: &Path) -> Result<ThemeCheckSummary, String> {
    let meta = std::fs::metadata(path).map_err(|e| {
        format!(
            "Không thể truy cập đường dẫn theme '{}': {e}",
            path.display()
        )
    })?;
    let pkg = if meta.is_file() {
        if meta.len() > V2_MAX_ZIP_BYTES as u64 {
            return Err(format!(
                "File ZIP theme vượt quá giới hạn {V2_MAX_ZIP_BYTES} bytes."
            ));
        }
        let file = std::fs::File::open(path)
            .map_err(|e| format!("Không thể mở file ZIP '{}': {e}", path.display()))?;
        let mut bytes = Vec::with_capacity(meta.len() as usize);
        file.take((V2_MAX_ZIP_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("Không thể đọc file ZIP '{}': {e}", path.display()))?;
        if bytes.len() > V2_MAX_ZIP_BYTES {
            return Err(format!(
                "File ZIP theme vượt quá giới hạn {V2_MAX_ZIP_BYTES} bytes."
            ));
        }
        parse_theme_zip_v3(&bytes).map_err(zip_err_msg)?
    } else if meta.is_dir() {
        let files = load_v3_files_from_dir(path)?;
        validate_theme_v3_files(files).map_err(zip_err_msg)?
    } else {
        return Err(format!(
            "Đường dẫn '{}' không phải file hoặc thư mục hợp lệ.",
            path.display()
        ));
    };
    build_theme_check_summary(&pkg)
}

/// Retained only for historical ZIP-parser unit tests during the v3 cutover.
#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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
        "js" | "mjs" => Some("text/javascript; charset=utf-8"),
        "svg" => Some("image/svg+xml"),
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
    fn v3_s01_accepts_declared_utf8_js_modules_and_svg_only_within_text_cap() {
        for (path, source, mime) in [
            (
                "assets/main.js",
                "const title = 'Tiếng Việt';".as_bytes(),
                "text/javascript; charset=utf-8",
            ),
            (
                "assets/module.mjs",
                b"export const enabled = true;".as_slice(),
                "text/javascript; charset=utf-8",
            ),
            (
                "assets/icon.svg",
                br#"<svg xmlns="http://www.w3.org/2000/svg"><title>Icon</title></svg>"#.as_slice(),
                "image/svg+xml",
            ),
        ] {
            let mut manifest = minimal_v3_manifest();
            manifest["assets"] = serde_json::json!([path]);
            let pkg =
                parse_theme_zip_v3(&build_v3_zip(manifest.clone(), &[(path, source)])).unwrap();
            assert_eq!(pkg.files[path], source);
            assert_eq!(theme_asset_mime(path), Some(mime));

            let invalid_utf8 = [0xff, 0xfe];
            assert!(matches!(
                parse_theme_zip_v3(&build_v3_zip(manifest.clone(), &[(path, &invalid_utf8)])),
                Err(V2ZipError::BadRequest(message)) if message.contains("UTF-8")
            ));
            let boundary = vec![b'a'; V2_MAX_TEXT_BYTES];
            assert!(
                parse_theme_zip_v3(&build_v3_zip(manifest.clone(), &[(path, &boundary)])).is_ok()
            );
            let oversized = vec![b'a'; V2_MAX_TEXT_BYTES + 1];
            assert!(matches!(
                parse_theme_zip_v3(&build_v3_zip(manifest, &[(path, &oversized)])),
                Err(V2ZipError::PayloadTooLarge(_))
            ));
        }
    }

    #[test]
    fn v3_s01_rejects_svg_pattern_preview_and_undeclared_active_assets() {
        let mut manifest = minimal_v3_manifest();
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#;
        for (path, source) in [
            ("assets/icon.svg", svg.as_slice()),
            ("assets/main.js", b"let x = 1;".as_slice()),
            ("assets/module.mjs", b"export {};".as_slice()),
        ] {
            assert!(matches!(
                parse_theme_zip_v3(&build_v3_zip(manifest.clone(), &[(path, source)])),
                Err(V2ZipError::BadRequest(_))
            ));
        }
        manifest["assets"] = serde_json::json!(["assets/icon.svg"]);
        manifest["patterns"] = serde_json::json!([{
            "key": "hero", "title": "Hero", "category": "banner",
            "file": "patterns/hero.json", "preview_asset": "assets/icon.svg"
        }]);
        assert!(matches!(
            parse_theme_zip_v3(&build_v3_zip(
                manifest,
                &[
                    (
                        "patterns/hero.json",
                        br#"{"schema_version":1,"sections":[]}"#
                    ),
                    ("assets/icon.svg", svg),
                ],
            )),
            Err(V2ZipError::BadRequest(_))
        ));
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
            "assets/evil.wasm",
            "assets/evil.zip",
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

    #[test]
    fn check_theme_path_validates_dir_zip_and_security_boundaries() {
        use std::io::Write as _;
        use zip::write::SimpleFileOptions;

        let starter_dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src/builtin_themes/opendash-starter");
        let summary = check_theme_path(&starter_dir).expect("opendash-starter must pass");
        assert!(summary.ok);
        assert_eq!(summary.theme_id, "opendash-starter");
        assert!(!summary.templates.is_empty());
        assert!(summary.parts.windows(2).all(|w| w[0] <= w[1]));
        assert!(summary.templates.windows(2).all(|w| w[0] <= w[1]));

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let tmp_root = std::env::temp_dir().join(format!(
            "opendash-check-theme-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&tmp_root).unwrap();

        // Build minimal valid theme dir + test binary zip with non-.zip extension.
        let valid_dir = tmp_root.join("valid-theme");
        std::fs::create_dir_all(valid_dir.join("assets")).unwrap();
        let manifest_bytes = br#"{"manifest_version":3,"theme_id":"cli-sample","name":"CLI Sample","version":"1.0.0","templates":[{"kind":"index","file":"index.html"}],"assets":["assets/main.css"],"options":[{"key":"cta","label":"CTA","type":"text","default":"Hi"}]}"#;
        let index_bytes = b"<main><h1>{{ site.title }}</h1>{% for e in entries %}<a href=\"{{ e.url }}\">{{ e.title }}</a>{% endfor %}</main>";
        let css_bytes = b"body { color: #111; }";
        std::fs::write(valid_dir.join("theme.json"), manifest_bytes).unwrap();
        std::fs::write(valid_dir.join("index.html"), index_bytes).unwrap();
        std::fs::write(valid_dir.join("assets/main.css"), css_bytes).unwrap();
        std::fs::write(valid_dir.join("README.md"), b"# Sample Theme Docs\n").unwrap();

        let dir_summary = check_theme_path(&valid_dir).expect("valid dir passes");
        assert_eq!(dir_summary.theme_id, "cli-sample");
        assert_eq!(dir_summary.options_count, 1);
        assert_eq!(dir_summary.assets, vec!["assets/main.css"]);

        // Subdirectory README.md must still be rejected by offline CLI check.
        let sub_readme_dir = tmp_root.join("sub-readme-theme");
        std::fs::create_dir_all(sub_readme_dir.join("assets")).unwrap();
        std::fs::write(sub_readme_dir.join("theme.json"), manifest_bytes).unwrap();
        std::fs::write(sub_readme_dir.join("index.html"), index_bytes).unwrap();
        std::fs::write(sub_readme_dir.join("assets/main.css"), css_bytes).unwrap();
        std::fs::write(sub_readme_dir.join("assets/README.md"), b"# Not Allowed\n").unwrap();
        assert!(check_theme_path(&sub_readme_dir).is_err());

        // ZIP package without `.zip` extension must still validate via magic/file read.
        let bin_zip_path = tmp_root.join("cli-sample.bundle");
        {
            let f = std::fs::File::create(&bin_zip_path).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            let opts = SimpleFileOptions::default();
            zw.start_file("theme.json", opts).unwrap();
            zw.write_all(manifest_bytes).unwrap();
            zw.start_file("index.html", opts).unwrap();
            zw.write_all(index_bytes).unwrap();
            zw.start_file("assets/main.css", opts).unwrap();
            zw.write_all(css_bytes).unwrap();
            zw.finish().unwrap();
        }
        let zip_summary = check_theme_path(&bin_zip_path).expect("non-.zip zip passes");
        assert_eq!(zip_summary.theme_id, "cli-sample");

        // Hidden file rejection.
        let hidden_dir = tmp_root.join("hidden-theme");
        std::fs::create_dir_all(&hidden_dir).unwrap();
        std::fs::write(hidden_dir.join("theme.json"), manifest_bytes).unwrap();
        std::fs::write(hidden_dir.join(".env"), b"SECRET=1").unwrap();
        assert!(check_theme_path(&hidden_dir).is_err());

        // Depth > MAX_THEME_DIR_DEPTH (16) rejection.
        let deep_dir = tmp_root.join("deep-theme");
        let mut nested = deep_dir.clone();
        for i in 0..=16 {
            nested = nested.join(format!("d{i}"));
        }
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(deep_dir.join("theme.json"), manifest_bytes).unwrap();
        assert!(check_theme_path(&deep_dir).is_err());

        // Oversized text file rejection.
        let big_dir = tmp_root.join("big-theme");
        std::fs::create_dir_all(&big_dir).unwrap();
        std::fs::write(big_dir.join("theme.json"), manifest_bytes).unwrap();
        std::fs::write(
            big_dir.join("index.html"),
            vec![b'x'; V2_MAX_TEXT_BYTES + 1],
        )
        .unwrap();
        assert!(check_theme_path(&big_dir).is_err());

        // Oversized ZIP rejection.
        let big_zip = tmp_root.join("big.zip");
        std::fs::write(&big_zip, vec![0u8; V2_MAX_ZIP_BYTES + 1]).unwrap();
        assert!(check_theme_path(&big_zip).is_err());

        // Internal symlink rejection (Unix).
        #[cfg(unix)]
        {
            let sym_dir = tmp_root.join("sym-theme");
            std::fs::create_dir_all(&sym_dir).unwrap();
            std::fs::write(sym_dir.join("theme.json"), manifest_bytes).unwrap();
            std::os::unix::fs::symlink(valid_dir.join("index.html"), sym_dir.join("index.html"))
                .unwrap();
            assert!(check_theme_path(&sym_dir).is_err());
        }

        // Unsafe C-SEC template rejection.
        let bad_csec = tmp_root.join("bad-csec");
        std::fs::create_dir_all(&bad_csec).unwrap();
        std::fs::write(
            bad_csec.join("theme.json"),
            br#"{"manifest_version":3,"theme_id":"bad-csec","name":"Bad","version":"1.0.0","templates":[{"kind":"index","file":"index.html"}]}"#,
        )
        .unwrap();
        std::fs::write(bad_csec.join("index.html"), b"<script>alert(1)</script>").unwrap();
        assert!(check_theme_path(&bad_csec).is_err());

        let _ = std::fs::remove_dir_all(&tmp_root);
    }
}
