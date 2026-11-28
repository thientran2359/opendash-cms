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

pub(super) fn required_theme_utf8<'a>(
    files: &'a BTreeMap<String, Vec<u8>>,
    path: &str,
) -> Result<&'a str, V2ZipError> {
    let bytes = files.get(path).ok_or(V2ZipError::BadRequest(
        "Thiếu file theme được khai báo trong theme.json.",
    ))?;
    let source = std::str::from_utf8(bytes)
        .map_err(|_| V2ZipError::BadRequest("File theme v1 phải là UTF-8 hợp lệ."))?;
    if source.trim().is_empty() {
        return Err(V2ZipError::BadRequest(
            "File theme v1 được khai báo không được để trống.",
        ));
    }
    Ok(source)
}

pub fn load_theme_files_from_dir(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
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
    walk_theme_dir(root, "", 0, &mut files, &mut seen_lower, &mut decoded_total)?;
    Ok(files)
}

/// Parses and validates `declarations` (and optional top-level `field_groups`) from a V1 theme manifest.
/// Rejects unknown fields, invalid CPT/field schemas, duplicate keys, and any
/// undeclared taxonomy associations and duplicate keys.
pub fn parse_theme_declarations(manifest_json: &str) -> Result<ThemeDeclarations, &'static str> {
    let val: serde_json::Value =
        serde_json::from_str(manifest_json).map_err(|_| "Manifest theme v1 không hợp lệ.")?;
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
    let mut parsed: ThemeDeclarations = serde_json::from_value(raw_decl)
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
        tax.validate()?;
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
            singular_label: ct.singular_label.clone(),
            description: ct.description.clone(),
            settings,
        };
        def.validate()?;
        opendash_core::content::validate_provider_rewrite_base(ct.rewrite_base.as_deref())?;
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
        fg.validate_and_normalize_full(
            opendash_core::field_schema::FieldScopeKind::ExtensionGroupField,
        )?;
    }
    if parsed.options_pages.len() > opendash_core::content::MAX_CUSTOM_OPTIONS_PAGES_PER_PACKAGE {
        return Err("Theme vượt quá số lượng tối đa custom_options_pages cho phép.");
    }
    let mut seen_pages = std::collections::HashSet::new();
    for page in &mut parsed.options_pages {
        if !seen_pages.insert(page.page_key.clone()) {
            return Err("Theme khai báo trùng page_key trong options_pages.");
        }
        page.validate_and_normalize()?;
    }
    Ok(parsed)
}

pub fn cpt_scope_for_target_key<'a>(
    manifest: &'a ThemeManifest,
    target_key: &'a str,
) -> Option<&'a str> {
    if let Some(rest) = target_key.strip_prefix("single:type:") {
        return rest.split(':').next().filter(|s| !s.is_empty());
    }
    if let Some(rest) = target_key.strip_prefix("archive:type:") {
        return (!rest.is_empty()).then_some(rest);
    }
    if let Some(custom_key) = target_key.strip_prefix("single_custom:") {
        return manifest.templates.iter().find_map(|tpl| {
            (tpl.kind == TemplateCandidateKind::SingleCustom
                && tpl.custom_key.as_deref() == Some(custom_key)
                && tpl.allowed_types.len() == 1)
                .then(|| tpl.allowed_types[0].as_str())
        });
    }
    None
}

pub fn declared_fields_for_cpt(
    declarations: &ThemeDeclarations,
    cpt_key: &str,
) -> Vec<opendash_core::field_schema::FieldSpec> {
    let mut fields = Vec::new();
    for ct in &declarations.content_types {
        if ct.key == cpt_key {
            let mut settings = ct.settings.clone().unwrap_or_default();
            if settings.validate_and_normalize().is_ok()
                && let Ok(eff) = opendash_core::content::EffectiveContentSchema::assemble(
                    opendash_core::registry::OwnedDescriptor {
                        owner: opendash_core::registry::ResourceOwner::User,
                        status: opendash_core::registry::ProviderStatus::Active,
                        declaration_state: opendash_core::registry::DeclarationState::Declared,
                        descriptor: opendash_core::content::ContentTypeDefinition {
                            key: ct.key.clone(),
                            label: ct.label.clone(),
                            singular_label: ct.singular_label.clone(),
                            description: ct.description.clone(),
                            settings,
                        },
                    },
                    1,
                    Vec::new(),
                )
            {
                fields.extend(eff.effective_fields.into_values().map(|e| e.definition));
            }
        }
    }
    for fg in &declarations.field_groups {
        if let Ok(norm) = fg.validate_and_normalize_full(
            opendash_core::field_schema::FieldScopeKind::ExtensionGroupField,
        ) && norm.domain == opendash_core::content::FieldGroupDomain::Content
            && opendash_core::content::group_allows_role_for_target(
                &norm.location,
                "content_type",
                cpt_key,
                opendash_core::content::FieldRole::Admin,
            )
        {
            fields.extend(norm.fields);
        }
    }
    fields
}

pub fn taxonomy_scope_for_target_key<'a>(target_key: &'a str) -> Option<&'a str> {
    if let Some(rest) = target_key.strip_prefix("taxonomy:key:") {
        return rest.split(':').next().filter(|s| !s.is_empty());
    }
    None
}

pub fn declared_fields_for_taxonomy(
    declarations: &ThemeDeclarations,
    tax_key: Option<&str>,
) -> Vec<opendash_core::field_schema::FieldSpec> {
    use opendash_core::content::{
        FieldGroupDomain, FieldRole, LocationEvalContext, evaluate_location_rules,
        is_restricted_to_admin_only,
    };
    use opendash_core::field_schema::FieldScopeKind;

    let mut fields = Vec::new();
    for fg in &declarations.field_groups {
        let Ok(norm) = fg.validate_and_normalize_full(FieldScopeKind::ExtensionGroupField) else {
            continue;
        };
        if norm.domain != FieldGroupDomain::Term || is_restricted_to_admin_only(&norm.location) {
            continue;
        }
        let matches = match tax_key {
            Some(key) => evaluate_location_rules(
                &norm.location,
                &LocationEvalContext {
                    taxonomy_term: Some(key.to_owned()),
                    user_role: FieldRole::Editor,
                    ..Default::default()
                },
            ),
            None => true,
        };
        if matches {
            fields.extend(norm.fields);
        }
    }
    fields
}

pub fn declared_custom_options_pages_fields(
    declarations: &ThemeDeclarations,
) -> BTreeMap<String, Vec<opendash_core::field_schema::FieldSpec>> {
    use opendash_core::content::{
        FieldGroupDomain, FieldRole, LocationEvalContext, evaluate_location_rules,
        is_restricted_to_admin_only,
    };
    use opendash_core::field_schema::FieldScopeKind;

    let mut out = BTreeMap::new();
    for page in &declarations.options_pages {
        out.entry(page.page_key.clone()).or_insert_with(Vec::new);
    }
    for fg in &declarations.field_groups {
        let Ok(norm) = fg.validate_and_normalize_full(FieldScopeKind::ExtensionGroupField) else {
            continue;
        };
        if norm.domain != FieldGroupDomain::OptionsPage
            || is_restricted_to_admin_only(&norm.location)
        {
            continue;
        }
        for page in &declarations.options_pages {
            let page_ref = format!("custom:{}", page.page_key);
            if evaluate_location_rules(
                &norm.location,
                &LocationEvalContext {
                    options_page: Some(page_ref),
                    user_role: FieldRole::Editor,
                    ..Default::default()
                },
            ) && let Some(list) = out.get_mut(&page.page_key)
            {
                list.extend(norm.fields.clone());
            }
        }
    }
    out
}

pub fn declared_extra_site_and_theme_options_fields(
    declarations: &ThemeDeclarations,
    theme_id: &str,
) -> (
    Vec<opendash_core::field_schema::FieldSpec>,
    Vec<opendash_core::field_schema::FieldSpec>,
) {
    use opendash_core::content::{
        FieldGroupDomain, FieldRole, LocationEvalContext, evaluate_location_rules,
        is_restricted_to_admin_only,
    };
    use opendash_core::field_schema::FieldScopeKind;

    let mut site_fields = Vec::new();
    let mut theme_fields = Vec::new();
    let theme_ref = format!("theme:{theme_id}");
    for fg in &declarations.field_groups {
        let Ok(norm) = fg.validate_and_normalize_full(FieldScopeKind::ExtensionGroupField) else {
            continue;
        };
        if norm.domain != FieldGroupDomain::OptionsPage
            || is_restricted_to_admin_only(&norm.location)
        {
            continue;
        }
        if evaluate_location_rules(
            &norm.location,
            &LocationEvalContext {
                options_page: Some("site_global".to_owned()),
                user_role: FieldRole::Editor,
                ..Default::default()
            },
        ) {
            site_fields.extend(norm.fields.clone());
        }
        if evaluate_location_rules(
            &norm.location,
            &LocationEvalContext {
                options_page: Some(theme_ref.clone()),
                user_role: FieldRole::Editor,
                ..Default::default()
            },
        ) {
            theme_fields.extend(norm.fields);
        }
    }
    (site_fields, theme_fields)
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
