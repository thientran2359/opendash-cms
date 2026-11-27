use super::*;
#[allow(clippy::too_many_arguments)]

pub fn validate_field_value(
    spec: &FieldSpec,
    ctx: FieldContext,
    raw_val: &Value,
    visible: bool,
    prev_visible: bool,
    upgrade_exempt: bool,
    prev_val: Option<&Value>,
    root_specs: &[FieldSpec],
    root_values: &BTreeMap<String, Value>,
    prev_root_values: Option<&BTreeMap<String, Value>>,
    in_root: bool,
) -> Result<(), &'static str> {
    if spec.responsive && ctx == FieldContext::StudioBlock {
        if let Some(obj) = raw_val.as_object() {
            for k in obj.keys() {
                if !matches!(k.as_str(), "desktop" | "tablet" | "mobile") {
                    return Err("Giá trị responsive chỉ chấp nhận desktop, tablet, mobile.");
                }
            }
            let mut has_non_empty = false;
            for vp in obj.values() {
                if !vp.is_null() {
                    if !vp.as_str().is_some_and(|s| s.trim().is_empty()) {
                        has_non_empty = true;
                    }
                    validate_non_responsive_value(
                        spec,
                        ctx,
                        vp,
                        false,
                        prev_visible,
                        false,
                        prev_val,
                        root_specs,
                        root_values,
                        prev_root_values,
                        in_root,
                    )?;
                }
            }
            if visible && !upgrade_exempt && spec.effective_required() && !has_non_empty {
                return Err("Thiếu giá trị cho trường bắt buộc.");
            }
            return Ok(());
        }
    }
    validate_non_responsive_value(
        spec,
        ctx,
        raw_val,
        visible,
        prev_visible,
        upgrade_exempt,
        prev_val,
        root_specs,
        root_values,
        prev_root_values,
        in_root,
    )
}

/// Checks `COMPAT-1..4` transitions between existing and incoming `FieldSpec` trees.
pub fn check_schema_compatibility(
    existing: &[FieldSpec],
    incoming: &[FieldSpec],
    has_existing_data: bool,
) -> Result<(), &'static str> {
    let old_data: BTreeMap<&str, &FieldSpec> = existing
        .iter()
        .filter(|s| !s.is_ui_only())
        .map(|s| (s.key.as_str(), s))
        .collect();
    let new_data: BTreeMap<&str, &FieldSpec> = incoming
        .iter()
        .filter(|s| !s.is_ui_only())
        .map(|s| (s.key.as_str(), s))
        .collect();

    // Unconditional COMPAT-4 check on incoming ACL/default invariants
    check_compat4_recursive(incoming)?;

    if !has_existing_data {
        return Ok(());
    }

    for (&old_key, &old_spec) in &old_data {
        let Some(&new_spec) = new_data.get(old_key) else {
            return Err(
                "cannot_remove_field_with_data: Không thể xóa hoặc di chuyển khóa field khi đã có dữ liệu.",
            );
        };
        let compatible_type = old_spec.field_type == new_spec.field_type
            || (old_spec.field_type == FieldType::Text
                && new_spec.field_type == FieldType::Textarea);
        if !compatible_type {
            return Err(
                "incompatible_schema_change: Không thể đổi kiểu dữ liệu (chỉ cho phép nới text -> textarea).",
            );
        }
        if matches!(
            old_spec.field_type,
            FieldType::Select | FieldType::EntryRef | FieldType::TermRef
        ) && old_spec.multiple != new_spec.multiple
        {
            return Err("incompatible_schema_change: Không thể lật multiple khi đã có dữ liệu.");
        }
        if matches!(old_spec.field_type, FieldType::Media | FieldType::Gallery) {
            let old_k = old_spec.effective_media_kind();
            let new_k = new_spec.effective_media_kind();
            let kind_ok = old_k == new_k || new_k == MediaFieldKind::Any;
            if !kind_ok {
                return Err(
                    "incompatible_schema_change: Không thể thu hẹp media_kind khi đã có dữ liệu.",
                );
            }
        }
        if matches!(
            old_spec.field_type,
            FieldType::Select | FieldType::Checkbox | FieldType::RadioButtons
        ) && old_spec.options_source.is_none()
        {
            let new_vals: BTreeSet<&str> =
                new_spec.options.iter().map(|o| o.value.as_str()).collect();
            for old_opt in &old_spec.options {
                if !new_vals.contains(old_opt.value.as_str()) {
                    return Err(
                        "incompatible_schema_change: Không thể xóa option đang khai báo khi đã có dữ liệu.",
                    );
                }
            }
        }
        if !old_spec.effective_required() && new_spec.effective_required() {
            return Err(
                "incompatible_schema_change: Không thể đổi trường tùy chọn thành required khi đã có dữ liệu.",
            );
        }
        if !old_spec.fields.is_empty() && !new_spec.fields.is_empty() {
            check_schema_compatibility(&old_spec.fields, &new_spec.fields, true)?;
        }
    }

    for (&new_key, &new_spec) in &new_data {
        if !old_data.contains_key(new_key) && new_spec.effective_required() {
            return Err(
                "incompatible_schema_change: Không thể thêm trường required mới khi đã có dữ liệu.",
            );
        }
    }

    Ok(())
}

pub(crate) fn check_compat4_recursive(specs: &[FieldSpec]) -> Result<(), &'static str> {
    for spec in specs {
        if spec.is_ui_only() {
            continue;
        }
        if let Some(acl) = &spec.acl {
            let min_view = acl.min_view_role.unwrap_or(FieldRole::Editor);
            let min_edit = acl.min_edit_role.unwrap_or(FieldRole::Editor);
            let pub_exp = acl.public_exposure.unwrap_or(PublicExposure::Public);
            if min_edit < min_view {
                return Err("invalid_field_acl: min_edit_role không được thấp hơn min_view_role.");
            }
            if pub_exp == PublicExposure::Public && min_view != FieldRole::Editor {
                return Err(
                    "invalid_field_acl: Trường có min_view_role = admin không được đặt public_exposure = public.",
                );
            }
            if spec.effective_required() && min_edit == FieldRole::Admin {
                let valid = spec
                    .default
                    .as_ref()
                    .is_some_and(|v| is_non_empty_json_value(spec, v));
                if !valid {
                    return Err(
                        "admin_required_field_missing_default: Trường required với min_edit_role = admin bắt buộc có default hợp lệ, khác rỗng.",
                    );
                }
            }
        }
        if !spec.fields.is_empty() {
            check_compat4_recursive(&spec.fields)?;
        }
    }
    Ok(())
}

pub fn collect_media_ids(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
) -> CollectedMediaIds {
    let mut out = CollectedMediaIds::default();
    collect_media_recursive(specs, values, &mut out);
    out
}

pub(crate) fn collect_media_recursive(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    out: &mut CollectedMediaIds,
) {
    for spec in specs {
        let Some(val) = values.get(spec.key.as_str()) else {
            continue;
        };
        match spec.field_type {
            FieldType::Media => {
                if let Some(id) = val.as_str()
                    && valid_sha256_media_id(id)
                {
                    push_collected_media(out, spec.effective_media_kind(), id);
                }
            }
            FieldType::Gallery => {
                if let Some(arr) = val.as_array() {
                    let kind = spec.effective_media_kind();
                    for item in arr.iter().take(MAX_GALLERY_ITEMS) {
                        if let Some(id) = item.as_str()
                            && valid_sha256_media_id(id)
                        {
                            push_collected_media(out, kind, id);
                        }
                    }
                }
            }
            FieldType::Group => {
                if let Some(obj) = val.as_object() {
                    let map: BTreeMap<String, Value> =
                        obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                    collect_media_recursive(&spec.fields, &map, out);
                }
            }
            FieldType::Repeater => {
                if let Some(arr) = val.as_array() {
                    for item in arr.iter().take(MAX_REPEATER_ITEMS) {
                        if let Some(obj) = item.as_object() {
                            let map: BTreeMap<String, Value> =
                                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                            collect_media_recursive(&spec.fields, &map, out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Collects media IDs only from fields with `public_exposure == Public`
/// (recursing strictly into nested `Group` and `Repeater` trees so private nested fields are never leaked,
/// while preserving access for withdrawn fields when `include_withdrawn_public == true`).
pub fn collect_public_media_ids(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    include_withdrawn_public: bool,
) -> CollectedMediaIds {
    let mut out = CollectedMediaIds::default();
    collect_public_media_recursive(specs, values, include_withdrawn_public, &mut out);
    out
}

pub(crate) fn collect_public_media_recursive(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    include_withdrawn_public: bool,
    out: &mut CollectedMediaIds,
) {
    for spec in specs {
        if spec.is_ui_only() {
            continue;
        }
        if spec.is_withdrawn() && !include_withdrawn_public {
            continue;
        }
        let (_, _, exposure) = spec.effective_acl();
        if exposure != PublicExposure::Public {
            continue;
        }
        let Some(val) = values.get(spec.key.as_str()) else {
            continue;
        };
        match spec.field_type {
            FieldType::Media => {
                if let Some(id) = val.as_str()
                    && valid_sha256_media_id(id)
                {
                    push_collected_media(out, spec.effective_media_kind(), id);
                }
            }
            FieldType::Gallery => {
                if let Some(arr) = val.as_array() {
                    let kind = spec.effective_media_kind();
                    for item in arr.iter().take(MAX_GALLERY_ITEMS) {
                        if let Some(id) = item.as_str()
                            && valid_sha256_media_id(id)
                        {
                            push_collected_media(out, kind, id);
                        }
                    }
                }
            }
            FieldType::Group => {
                if let Some(obj) = val.as_object() {
                    let map: BTreeMap<String, Value> =
                        obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                    collect_public_media_recursive(
                        &spec.fields,
                        &map,
                        include_withdrawn_public,
                        out,
                    );
                }
            }
            FieldType::Repeater => {
                if let Some(arr) = val.as_array() {
                    for item in arr.iter().take(MAX_REPEATER_ITEMS) {
                        if let Some(obj) = item.as_object() {
                            let map: BTreeMap<String, Value> =
                                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                            collect_public_media_recursive(
                                &spec.fields,
                                &map,
                                include_withdrawn_public,
                                out,
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollectedRelationalRefs {
    pub entry_ids: BTreeSet<i64>,
    pub term_ids: BTreeSet<i64>,
}

pub fn collect_relational_refs(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
) -> CollectedRelationalRefs {
    let mut out = CollectedRelationalRefs::default();
    collect_relational_recursive(specs, values, &mut out);
    out
}

pub(crate) fn collect_relational_recursive(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    out: &mut CollectedRelationalRefs,
) {
    for spec in specs {
        let Some(val) = values.get(spec.key.as_str()) else {
            continue;
        };
        match spec.field_type {
            FieldType::EntryRef => {
                if spec.multiple {
                    if let Some(arr) = val.as_array() {
                        for id in arr
                            .iter()
                            .take(MAX_ENTRY_REF_ITEMS)
                            .filter_map(Value::as_i64)
                        {
                            if id > 0 {
                                out.entry_ids.insert(id);
                            }
                        }
                    }
                } else if let Some(id) = val.as_i64().filter(|n| *n > 0) {
                    out.entry_ids.insert(id);
                }
            }
            FieldType::TermRef => {
                if spec.multiple {
                    if let Some(arr) = val.as_array() {
                        for id in arr
                            .iter()
                            .take(MAX_TERM_REF_ITEMS)
                            .filter_map(Value::as_i64)
                        {
                            if id > 0 {
                                out.term_ids.insert(id);
                            }
                        }
                    }
                } else if let Some(id) = val.as_i64().filter(|n| *n > 0) {
                    out.term_ids.insert(id);
                }
            }
            FieldType::Group => {
                if let Some(obj) = val.as_object() {
                    let map: BTreeMap<String, Value> =
                        obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                    collect_relational_recursive(&spec.fields, &map, out);
                }
            }
            FieldType::Repeater => {
                if let Some(arr) = val.as_array() {
                    for item in arr.iter().take(MAX_REPEATER_ITEMS) {
                        if let Some(obj) = item.as_object() {
                            let map: BTreeMap<String, Value> =
                                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                            collect_relational_recursive(&spec.fields, &map, out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn collect_options_source_values(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    source: SelectOptionsSource,
) -> Vec<String> {
    let mut out = Vec::new();
    collect_options_source_values_recursive(specs, values, source, &mut out);
    out
}

pub(crate) fn collect_options_source_values_recursive(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    source: SelectOptionsSource,
    out: &mut Vec<String>,
) {
    for spec in specs {
        if spec.is_ui_only() || spec.field_type == FieldType::Secret {
            continue;
        }
        match spec.field_type {
            FieldType::Select if spec.options_source == Some(source) => {
                let candidate = values.get(spec.key.as_str()).or(spec.default.as_ref());
                match candidate {
                    Some(Value::String(s)) => {
                        let trimmed = s.trim();
                        if !trimmed.is_empty() {
                            out.push(trimmed.to_owned());
                        }
                    }
                    Some(Value::Array(arr)) if spec.multiple => {
                        for item in arr {
                            if let Some(s) = item.as_str() {
                                let trimmed = s.trim();
                                if !trimmed.is_empty() {
                                    out.push(trimmed.to_owned());
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            FieldType::Group => {
                let child_map: BTreeMap<String, Value> = values
                    .get(spec.key.as_str())
                    .and_then(Value::as_object)
                    .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .unwrap_or_default();
                collect_options_source_values_recursive(&spec.fields, &child_map, source, out);
            }
            _ => {}
        }
    }
}

pub fn extract_visible_text(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    out: &mut Vec<String>,
) {
    for spec in specs {
        if spec.is_ui_only() || spec.field_type == FieldType::Secret {
            continue;
        }
        let Some(val) = values.get(spec.key.as_str()) else {
            continue;
        };
        match spec.field_type {
            FieldType::Text | FieldType::Textarea => {
                if let Some(s) = val.as_str() {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        out.push(trimmed.to_owned());
                    }
                }
            }
            FieldType::Richtext => {
                if let Some(s) = val.as_str() {
                    let plain = strip_richtext_to_plain(s);
                    let trimmed = plain.trim();
                    if !trimmed.is_empty() {
                        out.push(trimmed.to_owned());
                    }
                }
            }
            FieldType::Link => {
                if let Ok(link) = serde_json::from_value::<LinkValue>(val.clone()) {
                    let trimmed = link.label.trim();
                    if !trimmed.is_empty() {
                        out.push(trimmed.to_owned());
                    }
                }
            }
            FieldType::Group => {
                if let Some(obj) = val.as_object() {
                    let map: BTreeMap<String, Value> =
                        obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                    extract_visible_text(&spec.fields, &map, out);
                }
            }
            FieldType::Repeater => {
                if let Some(arr) = val.as_array() {
                    for item in arr.iter().take(MAX_REPEATER_ITEMS) {
                        if let Some(obj) = item.as_object() {
                            let map: BTreeMap<String, Value> =
                                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                            extract_visible_text(&spec.fields, &map, out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn default_value_for_field_spec(spec: &FieldSpec) -> Value {
    if let Some(v) = spec.default.as_ref().filter(|v| !v.is_null()) {
        return v.clone();
    }
    match spec.field_type {
        FieldType::Text
        | FieldType::Textarea
        | FieldType::Email
        | FieldType::Url
        | FieldType::Richtext
        | FieldType::Color
        | FieldType::Icon
        | FieldType::Date
        | FieldType::Datetime
        | FieldType::Time => Value::String(String::new()),
        FieldType::Number => serde_json::json!(0),
        FieldType::Range => spec
            .min
            .map(canonicalize_f64)
            .map(|n| serde_json::json!(n))
            .unwrap_or_else(|| serde_json::json!(0)),
        FieldType::Boolean => Value::Bool(false),
        FieldType::Select => {
            if spec.multiple {
                Value::Array(Vec::new())
            } else if spec.allow_null || spec.options_source.is_some() {
                Value::String(String::new())
            } else {
                spec.options
                    .first()
                    .map(|o| Value::String(o.value.clone()))
                    .unwrap_or_else(|| Value::String(String::new()))
            }
        }
        FieldType::RadioButtons => spec
            .options
            .first()
            .map(|o| Value::String(o.value.clone()))
            .unwrap_or_else(|| Value::String(String::new())),
        FieldType::Checkbox | FieldType::Gallery | FieldType::Repeater => Value::Array(Vec::new()),
        FieldType::Spacing => serde_json::json!({}),
        FieldType::Link => {
            serde_json::json!({ "url": "", "label": "", "new_tab": false, "nofollow": false })
        }
        FieldType::Group => {
            let mut map = serde_json::Map::new();
            for sub in &spec.fields {
                if !sub.is_ui_only() {
                    map.insert(sub.key.clone(), default_value_for_field_spec(sub));
                }
            }
            Value::Object(map)
        }
        FieldType::Media
        | FieldType::EntryRef
        | FieldType::TermRef
        | FieldType::Secret
        | FieldType::Tab
        | FieldType::Accordion
        | FieldType::Message => Value::Null,
    }
}

/// Global Site Options using UI-only `Accordion` primitives so all 12 data option keys
/// live flat at the root (`social_facebook`, `site_logo`, `brand_primary_color`, etc.).
pub fn global_site_option_fields() -> Vec<FieldSpec> {
    let acc_social = {
        let mut s = FieldSpec::new(FieldType::Accordion, "acc_social_links", "Mạng xã hội");
        s.open_by_default = true;
        s.multi_expand = true;
        s
    };
    let acc_brand = {
        let mut s = FieldSpec::new(
            FieldType::Accordion,
            "acc_brand_contact",
            "Thương hiệu & Liên hệ chung",
        );
        s.open_by_default = true;
        s.multi_expand = true;
        s
    };
    let url_spec = |key: &str, label: &str, ph: &str| {
        let mut s = FieldSpec::new(FieldType::Url, key, label);
        s.public = true;
        s.placeholder = Some(ph.to_owned());
        s
    };

    let mut logo = FieldSpec::new(FieldType::Media, "site_logo", "Logo website");
    logo.public = true;
    logo.media_kind = Some(MediaFieldKind::Image);

    let mut brand_color = FieldSpec::new(FieldType::Color, "brand_primary_color", "Màu chủ đạo");
    brand_color.public = true;
    brand_color.default = Some(Value::String("#2563eb".to_owned()));

    let mut phone = FieldSpec::new(FieldType::Text, "contact_phone", "Số điện thoại");
    phone.public = true;
    phone.max_chars = Some(40);
    phone.placeholder = Some("0901 234 567".to_owned());

    let mut addr = FieldSpec::new(FieldType::Textarea, "contact_address", "Địa chỉ");
    addr.public = true;
    addr.max_chars = Some(300);

    let mut footer = FieldSpec::new(FieldType::Text, "footer_note", "Ghi chú chân trang");
    footer.public = true;
    footer.max_chars = Some(240);
    footer.placeholder = Some("© 2026 OpenDash".to_owned());

    vec![
        acc_social,
        url_spec(
            "social_facebook",
            "Facebook",
            "https://facebook.com/yourpage",
        ),
        url_spec(
            "social_youtube",
            "YouTube",
            "https://youtube.com/@yourchannel",
        ),
        url_spec("social_tiktok", "TikTok", "https://tiktok.com/@yourhandle"),
        url_spec(
            "social_instagram",
            "Instagram",
            "https://instagram.com/yourhandle",
        ),
        url_spec("social_x", "X (Twitter)", "https://x.com/yourhandle"),
        url_spec(
            "social_linkedin",
            "LinkedIn",
            "https://linkedin.com/in/yourprofile",
        ),
        url_spec("social_email", "Email liên hệ", "mailto:hello@example.com"),
        acc_brand,
        logo,
        brand_color,
        phone,
        addr,
        footer,
    ]
}
