use super::*;

/// Shared MiniJinja C-SEC HTML auto-escaping and fuel budget for theme CPT
/// templates and the built-in fallback, matching the Builder's rendering rules.
pub fn render_content_with_config(
    template: &str,
    data: &serde_json::Value,
    fmt_cfg: &crate::template_sec::RenderFormattingConfig,
) -> Result<String, &'static str> {
    let context = crate::template_sec::json_to_safe_value(data);
    crate::template_sec::render_csec_with_config("content", template, context, fmt_cfg)
        .map_err(|_| "Không thể render template nội dung.")
}

pub fn render_default_content_with_config(
    template: &str,
    data: &serde_json::Value,
    fmt_cfg: &crate::template_sec::RenderFormattingConfig,
) -> Result<String, &'static str> {
    let context = crate::template_sec::json_to_safe_value(data);
    crate::template_sec::render_csec_with_images_and_config("content", template, context, fmt_cfg)
        .map_err(|_| "Không thể render template nội dung.")
}

pub(super) fn required_v3_utf8<'a>(
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

/// Parses and validates `declarations` (and optional top-level `field_groups`) from a v3 theme manifest.
/// Rejects unknown fields, invalid CPT/field schemas, duplicate keys, and any
/// undeclared taxonomy associations and duplicate keys.
pub fn parse_theme_declarations(manifest_json: &str) -> Result<ThemeDeclarations, &'static str> {
    let val: serde_json::Value =
        serde_json::from_str(manifest_json).map_err(|_| "Manifest theme v3 không hợp lệ.")?;
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
