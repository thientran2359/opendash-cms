use super::*;
#[allow(clippy::too_many_arguments)]

pub(crate) fn validate_single_field_spec<'a>(
    spec: &'a FieldSpec,
    ctx: FieldContext,
    top_map: &BTreeMap<&'a str, &'a FieldSpec>,
    sibling_map: &BTreeMap<&'a str, &'a FieldSpec>,
    group_depth: usize,
    in_repeater: bool,
    seen_css_vars: &mut BTreeSet<&'a str>,
    total_block_repeater_max: &mut usize,
    leaf_data_count: &mut usize,
) -> Result<(), &'static str> {
    let ft = spec.field_type;
    let in_group = group_depth > 0;

    if ft == FieldType::Accordion && spec.endpoint {
        if !spec.label.is_empty() {
            validate_label(&spec.label)?;
        }
    } else {
        validate_label(&spec.label)?;
    }

    validate_optional_plain_text(
        spec.instructions.as_deref(),
        MAX_INSTRUCTIONS_LEN,
        true,
        "Hướng dẫn field tối đa 400 ký tự và không chứa ký tự điều khiển.",
    )?;
    validate_optional_plain_text(
        spec.placeholder.as_deref(),
        MAX_PLACEHOLDER_LEN,
        false,
        "Placeholder field tối đa 120 ký tự.",
    )?;
    validate_optional_plain_text(
        spec.prepend.as_deref(),
        MAX_AFFIX_LEN,
        false,
        "Tiền tố prepend tối đa 32 ký tự.",
    )?;
    validate_optional_plain_text(
        spec.append.as_deref(),
        MAX_AFFIX_LEN,
        false,
        "Hậu tố append tối đa 32 ký tự.",
    )?;

    if let Some(w) = spec.width
        && !matches!(w, 25 | 33 | 50 | 66 | 75 | 100)
    {
        return Err("width chỉ chấp nhận một trong {25, 33, 50, 66, 75, 100}.");
    }

    if (spec.prepend.is_some() || spec.append.is_some())
        && !matches!(
            ft,
            FieldType::Text | FieldType::Number | FieldType::Email | FieldType::Url
        )
    {
        return Err("prepend/append chỉ áp dụng cho text, number, email, url.");
    }

    // Context-only capability checks
    if (spec.acl.is_some() || spec.declaration_state.is_some())
        && !matches!(ctx, FieldContext::Content | FieldContext::Term)
    {
        return Err("acl và declaration_state chỉ hợp lệ trên Content và Term.");
    }
    if spec.show_in_list && (ctx != FieldContext::Content || !ft.is_scalar_listable()) {
        return Err("show_in_list chỉ hợp lệ trên trường scalar của Content.");
    }
    if spec.public && (ctx != FieldContext::Options || ft.is_ui_only()) {
        return Err("Thuộc tính public chỉ hợp lệ trên trường dữ liệu Options.");
    }
    if spec.translatable && (ctx != FieldContext::Options || ft.is_ui_only()) {
        return Err("Thuộc tính translatable chỉ hợp lệ trên trường dữ liệu Options ở Đợt 1.");
    }
    if spec.responsive {
        if ctx != FieldContext::StudioBlock || in_group || in_repeater {
            return Err("responsive chỉ hỗ trợ ở top-level của StudioBlock.");
        }
        let allowed_resp =
            matches!(
                ft,
                FieldType::Number | FieldType::Range | FieldType::RadioButtons | FieldType::Spacing
            ) || (ft == FieldType::Select && !spec.multiple && spec.options_source.is_none());
        if !allowed_resp {
            return Err("Kiểu trường này không hỗ trợ responsive.");
        }
    }
    if spec.css_var.is_some() {
        if ctx != FieldContext::StudioBlock || in_repeater {
            return Err("css_var chỉ hỗ trợ ở StudioBlock (không đặt trong repeater).");
        }
        let allowed_css = match ft {
            FieldType::Number
            | FieldType::Range
            | FieldType::RadioButtons
            | FieldType::Color
            | FieldType::Spacing => true,
            FieldType::Select => !spec.multiple && spec.options_source.is_none(),
            FieldType::Media => spec.effective_media_kind() == MediaFieldKind::Image,
            _ => false,
        };
        if !allowed_css {
            return Err("Kiểu trường hoặc cấu hình hiện tại không hỗ trợ css_var.");
        }
        collect_css_var(spec.css_var.as_deref(), seen_css_vars)?;
    }

    if spec.unit.is_some() {
        let allowed_unit = ft == FieldType::Spacing
            || (ctx == FieldContext::StudioBlock
                && matches!(ft, FieldType::Number | FieldType::Range));
        if !allowed_unit {
            return Err("unit chỉ áp dụng cho spacing hoặc number/range trên StudioBlock.");
        }
    }
    if spec.property.is_some() && ft != FieldType::Spacing {
        return Err("Thuộc tính property chỉ áp dụng cho spacing.");
    }
    if spec.options_source.is_some()
        && (ft != FieldType::Select || ctx != FieldContext::StudioBlock || in_repeater)
    {
        return Err("options_source chỉ áp dụng cho select ngoài repeater trên StudioBlock.");
    }
    if spec.taxonomy_from_field.is_some()
        && (ft != FieldType::TermRef || ctx != FieldContext::StudioBlock || in_repeater)
    {
        return Err(
            "taxonomy_from_field chỉ áp dụng cho term_ref ngoài repeater trên StudioBlock.",
        );
    }
    if spec.sync_entry_terms {
        return Err("sync_entry_terms được kích hoạt ở Đợt 2 (ACF Content Structure).");
    }

    // Strict unused attribute hygiene checks
    if spec.rows.is_some() && ft != FieldType::Textarea {
        return Err("Thuộc tính rows chỉ áp dụng cho textarea.");
    }
    if (spec.min_chars.is_some() || spec.max_chars.is_some())
        && !matches!(ft, FieldType::Text | FieldType::Textarea | FieldType::Email)
    {
        return Err("min_chars/max_chars chỉ áp dụng cho text, textarea, email.");
    }
    if (spec.max_bytes.is_some() || spec.toolbar.is_some()) && ft != FieldType::Richtext {
        return Err("max_bytes/toolbar chỉ áp dụng cho richtext.");
    }
    if (spec.min.is_some() || spec.max.is_some() || spec.step.is_some())
        && !matches!(
            ft,
            FieldType::Number | FieldType::Range | FieldType::Spacing
        )
    {
        return Err("min/max/step chỉ áp dụng cho number, range, spacing.");
    }
    if (spec.min_date.is_some() || spec.max_date.is_some()) && ft != FieldType::Date {
        return Err("min_date/max_date chỉ áp dụng cho date.");
    }
    if !spec.options.is_empty()
        && !matches!(
            ft,
            FieldType::Select | FieldType::Checkbox | FieldType::RadioButtons
        )
    {
        return Err("Chỉ select, checkbox, radio_buttons mới được khai báo options.");
    }
    if (spec.display.is_some() || spec.allow_null) && ft != FieldType::Select {
        return Err("display/allow_null chỉ áp dụng cho select.");
    }
    if spec.multiple
        && !matches!(
            ft,
            FieldType::Select | FieldType::EntryRef | FieldType::TermRef
        )
    {
        return Err("multiple chỉ áp dụng cho select, entry_ref, term_ref.");
    }
    if (spec.on_label.is_some() || spec.off_label.is_some()) && ft != FieldType::Boolean {
        return Err("on_label/off_label chỉ áp dụng cho boolean.");
    }
    if spec.allow_alpha && ft != FieldType::Color {
        return Err("allow_alpha chỉ áp dụng cho color.");
    }
    if spec.media_kind.is_some() && !matches!(ft, FieldType::Media | FieldType::Gallery) {
        return Err("media_kind chỉ áp dụng cho media hoặc gallery.");
    }
    if (spec.min_items.is_some() || spec.max_items.is_some())
        && !matches!(
            ft,
            FieldType::Gallery
                | FieldType::Checkbox
                | FieldType::EntryRef
                | FieldType::TermRef
                | FieldType::Repeater
        )
    {
        return Err("min_items/max_items không áp dụng cho kiểu field này.");
    }
    if !spec.target_types.is_empty() && ft != FieldType::EntryRef {
        return Err("target_types chỉ áp dụng cho entry_ref.");
    }
    if (spec.taxonomy.is_some() || spec.picker_style.is_some()) && ft != FieldType::TermRef {
        return Err("taxonomy/picker_style chỉ áp dụng cho term_ref.");
    }
    if !spec.fields.is_empty() && !ft.is_container() {
        return Err("Chỉ group và repeater mới được chứa fields con.");
    }
    if spec.layout.is_some()
        && !matches!(
            ft,
            FieldType::Checkbox | FieldType::RadioButtons | FieldType::Repeater
        )
    {
        return Err("layout chỉ áp dụng cho checkbox, radio_buttons hoặc repeater.");
    }
    if (spec.item_label_field.is_some() || spec.button_label.is_some()) && ft != FieldType::Repeater
    {
        return Err("item_label_field/button_label chỉ áp dụng cho repeater.");
    }
    if spec.placement.is_some() && ft != FieldType::Tab {
        return Err("placement chỉ áp dụng cho tab.");
    }
    if (spec.open_by_default || spec.multi_expand || spec.endpoint) && ft != FieldType::Accordion {
        return Err("open_by_default/multi_expand/endpoint chỉ áp dụng cho accordion.");
    }
    if (spec.tone.is_some() || spec.body.is_some()) && ft != FieldType::Message {
        return Err("tone/body chỉ áp dụng cho message.");
    }

    // M7-S09 #1, #2, #4 when acl explicitly provided
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

    // UI-only placement checks
    if ft.is_ui_only() {
        if in_repeater || group_depth >= MAX_GROUP_DEPTH {
            return Err("UI layout field không được đặt trong repeater hoặc group cấp 2.");
        }
        if spec.required {
            return Err("UI layout field không được đặt required.");
        }
        if matches!(spec.default, Some(ref v) if !v.is_null()) {
            return Err("UI layout field không được đặt default.");
        }
    }

    // Per-type structural & default checks
    match ft {
        FieldType::Tab | FieldType::Accordion => {}
        FieldType::Message => {
            let Some(body) = spec.body.as_deref() else {
                return Err("Field message bắt buộc có body.");
            };
            if body.trim().is_empty()
                || body.chars().count() > MAX_MESSAGE_BODY_CHARS
                || body.chars().any(|c| c.is_control() && c != '\n')
                || body.contains('<')
                || body.contains('>')
            {
                return Err(
                    "Nội dung message phải là văn bản thuần không rỗng, <= 1000 ký tự và không chứa HTML.",
                );
            }
        }
        FieldType::Text => {
            *leaf_data_count += 1;
            let max_c = spec.max_chars.unwrap_or(DEFAULT_TEXT_MAX_CHARS);
            if max_c == 0 || max_c > MAX_TEXT_CHARS {
                return Err("max_chars của text phải nằm trong 1..=2000.");
            }
            if let Some(min_c) = spec.min_chars
                && min_c > max_c
            {
                return Err("min_chars của text lớn hơn max_chars.");
            }
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định text phải là chuỗi.");
                };
                if s.chars().any(char::is_control) {
                    return Err("Giá trị mặc định text không được chứa ký tự điều khiển.");
                }
                let len = s.chars().count();
                if len > max_c || (!s.is_empty() && spec.min_chars.is_some_and(|mn| len < mn)) {
                    return Err("Giá trị mặc định text vượt giới hạn độ dài.");
                }
            }
        }
        FieldType::Textarea => {
            *leaf_data_count += 1;
            if let Some(rows) = spec.rows
                && !(MIN_TEXTAREA_ROWS..=MAX_TEXTAREA_ROWS).contains(&rows)
            {
                return Err("rows của textarea phải nằm trong 2..=16.");
            }
            let max_c = spec.max_chars.unwrap_or(DEFAULT_TEXTAREA_MAX_CHARS);
            if max_c == 0 || max_c > MAX_TEXTAREA_CHARS {
                return Err("max_chars của textarea phải nằm trong 1..=100000.");
            }
            if let Some(min_c) = spec.min_chars
                && min_c > max_c
            {
                return Err("min_chars của textarea lớn hơn max_chars.");
            }
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định textarea phải là chuỗi.");
                };
                let len = s.chars().count();
                if len > max_c || (!s.is_empty() && spec.min_chars.is_some_and(|mn| len < mn)) {
                    return Err("Giá trị mặc định textarea vượt giới hạn độ dài.");
                }
            }
        }
        FieldType::Number => {
            *leaf_data_count += 1;
            if let Some(mn) = spec.min
                && !mn.is_finite()
            {
                return Err("min của number phải hữu hạn.");
            }
            if let Some(mx) = spec.max
                && !mx.is_finite()
            {
                return Err("max của number phải hữu hạn.");
            }
            if let (Some(mn), Some(mx)) = (spec.min, spec.max)
                && mn > mx
            {
                return Err("number có min lớn hơn max.");
            }
            if let Some(st) = spec.step
                && (!st.is_finite() || st <= 0.0)
            {
                return Err("step của number phải hữu hạn và > 0.");
            }
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(n) = def.as_f64().filter(|f| f.is_finite()) else {
                    return Err("Giá trị mặc định number phải là số hữu hạn.");
                };
                if spec.min.is_some_and(|mn| n < mn) || spec.max.is_some_and(|mx| n > mx) {
                    return Err("Giá trị mặc định number nằm ngoài [min, max].");
                }
            }
        }
        FieldType::Range => {
            *leaf_data_count += 1;
            let (Some(mn), Some(mx)) = (spec.min, spec.max) else {
                return Err("Field range bắt buộc khai báo cả min và max.");
            };
            if !mn.is_finite() || !mx.is_finite() || mn > mx {
                return Err("Khoảng min/max của range không hợp lệ.");
            }
            if let Some(st) = spec.step
                && (!st.is_finite() || st <= 0.0)
            {
                return Err("step của range phải hữu hạn và > 0.");
            }
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(n) = def.as_f64().filter(|f| f.is_finite()) else {
                    return Err("Giá trị mặc định range phải là số hữu hạn.");
                };
                if n < mn || n > mx {
                    return Err("Giá trị mặc định range nằm ngoài [min, max].");
                }
            }
        }
        FieldType::Email => {
            *leaf_data_count += 1;
            let max_c = spec.max_chars.unwrap_or(MAX_EMAIL_CHARS);
            if max_c == 0 || max_c > MAX_EMAIL_CHARS {
                return Err("max_chars của email phải nằm trong 1..=254.");
            }
            if let Some(min_c) = spec.min_chars
                && min_c > max_c
            {
                return Err("min_chars của email lớn hơn max_chars.");
            }
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định email phải là chuỗi.");
                };
                if !s.is_empty() && (!valid_email_address(s) || s.chars().count() > max_c) {
                    return Err("Email mặc định không hợp lệ.");
                }
            }
        }
        FieldType::Url => {
            *leaf_data_count += 1;
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định url phải là chuỗi.");
                };
                if !s.is_empty() && !valid_safe_url(s) {
                    return Err("URL mặc định không hợp lệ.");
                }
            }
        }
        FieldType::Secret => {
            *leaf_data_count += 1;
            if in_group || in_repeater {
                return Err("Field secret chỉ được đặt ở top-level của OwnerSettings.");
            }
            if matches!(spec.default, Some(ref v) if !v.is_null()) {
                return Err("Field secret không được đặt default khác null.");
            }
        }
        FieldType::Richtext => {
            *leaf_data_count += 1;
            let is_core_entry_body =
                ctx == FieldContext::Content && !in_group && !in_repeater && spec.key == "body";
            let cap = if is_core_entry_body {
                MAX_BUILDER_BLOCK_BODY_BYTES
            } else {
                MAX_RICHTEXT_OUTPUT_BYTES
            };
            let limit = spec.max_bytes.unwrap_or(cap);
            if limit == 0 || limit > cap {
                return Err("max_bytes của richtext vượt giới hạn cho phép.");
            }
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định richtext phải là chuỗi.");
                };
                if !s.is_empty() {
                    let cleaned = sanitize_richtext(s)?;
                    if cleaned.len() > limit {
                        return Err("Giá trị mặc định richtext vượt quá max_bytes sau làm sạch.");
                    }
                }
            }
        }
        FieldType::Media => {
            *leaf_data_count += 1;
            if matches!(spec.default, Some(ref v) if !v.is_null()) {
                return Err("Field media không được phép khai báo default.");
            }
        }
        FieldType::Gallery => {
            *leaf_data_count += 1;
            if spec.effective_media_kind() == MediaFieldKind::File {
                return Err("Field gallery chỉ hỗ trợ media_kind: image | video | any.");
            }
            let max_i = spec.max_items.unwrap_or(MAX_GALLERY_ITEMS);
            if max_i == 0 || max_i > MAX_GALLERY_ITEMS {
                return Err("max_items của gallery phải nằm trong 1..=50.");
            }
            if let Some(min_i) = spec.min_items
                && min_i > max_i
            {
                return Err("min_items của gallery lớn hơn max_items.");
            }
            if matches!(spec.default, Some(ref v) if !v.is_null()) {
                return Err("Field gallery không được phép khai báo default.");
            }
        }
        FieldType::Link => {
            *leaf_data_count += 1;
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let link: LinkValue = serde_json::from_value(def.clone())
                    .map_err(|_| "Giá trị mặc định link không đúng cấu trúc.")?;
                if link.label.chars().count() > MAX_LINK_LABEL_CHARS
                    || link.label.chars().any(char::is_control)
                {
                    return Err(
                        "Nhãn link mặc định vượt quá 160 ký tự hoặc chứa ký tự điều khiển.",
                    );
                }
                if !link.url.is_empty() && !valid_safe_url(&link.url) {
                    return Err("URL mặc định trong field link không hợp lệ.");
                }
            }
        }
        FieldType::Select => {
            *leaf_data_count += 1;
            if spec.options_source.is_some() {
                if !spec.options.is_empty() {
                    return Err("options và options_source trên select loại trừ lẫn nhau.");
                }
                if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                    if spec.multiple {
                        let Some(arr) = def.as_array() else {
                            return Err("Giá trị mặc định select multiple phải là mảng.");
                        };
                        for item in arr {
                            let Some(s) = item.as_str() else {
                                return Err("Phần tử mặc định select multiple phải là chuỗi.");
                            };
                            if s.is_empty() || s.len() > 128 || s.chars().any(char::is_control) {
                                return Err("Phần tử mặc định options_source không hợp lệ.");
                            }
                        }
                    } else {
                        let Some(s) = def.as_str() else {
                            return Err("Giá trị mặc định select đơn phải là chuỗi.");
                        };
                        if s.len() > 128 || s.chars().any(char::is_control) {
                            return Err("Giá trị mặc định options_source không hợp lệ.");
                        }
                    }
                }
            } else {
                validate_option_list(
                    &spec.options,
                    MAX_SELECT_OPTIONS,
                    spec.css_var.is_some(),
                    true,
                )?;
                let opt_vals: BTreeSet<&str> =
                    spec.options.iter().map(|o| o.value.as_str()).collect();
                if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                    if spec.multiple {
                        let Some(arr) = def.as_array() else {
                            return Err("Giá trị mặc định select multiple phải là mảng.");
                        };
                        let mut seen = BTreeSet::new();
                        for item in arr {
                            let Some(s) = item.as_str() else {
                                return Err("Phần tử mặc định select phải là chuỗi.");
                            };
                            if !opt_vals.contains(s) || !seen.insert(s) {
                                return Err("Giá trị mặc định select multiple không hợp lệ.");
                            }
                        }
                    } else {
                        let Some(s) = def.as_str() else {
                            return Err("Giá trị mặc định select phải là chuỗi.");
                        };
                        if !(s.is_empty() && (spec.allow_null || !spec.required))
                            && !opt_vals.contains(s)
                        {
                            return Err("Giá trị mặc định select không nằm trong options.");
                        }
                    }
                }
            }
        }
        FieldType::Checkbox => {
            *leaf_data_count += 1;
            let _ = spec.checkbox_layout()?;
            validate_option_list(&spec.options, MAX_SELECT_OPTIONS, false, false)?;
            let max_i = spec.max_items.unwrap_or(spec.options.len());
            if max_i == 0 || max_i > spec.options.len() {
                return Err("max_items của checkbox vượt quá số options.");
            }
            if let Some(min_i) = spec.min_items
                && min_i > max_i
            {
                return Err("min_items của checkbox lớn hơn max_items.");
            }
            let opt_vals: BTreeSet<&str> = spec.options.iter().map(|o| o.value.as_str()).collect();
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(arr) = def.as_array() else {
                    return Err("Giá trị mặc định checkbox phải là mảng.");
                };
                let mut seen = BTreeSet::new();
                for item in arr {
                    let Some(s) = item.as_str() else {
                        return Err("Phần tử mặc định checkbox phải là chuỗi.");
                    };
                    if !opt_vals.contains(s) || !seen.insert(s) {
                        return Err(
                            "Giá trị mặc định checkbox không có trong options hoặc bị trùng.",
                        );
                    }
                }
                if arr.len() > max_i {
                    return Err("Giá trị mặc định checkbox vượt max_items.");
                }
                if !arr.is_empty() && spec.min_items.is_some_and(|mn| arr.len() < mn) {
                    return Err("Giá trị mặc định checkbox ít hơn min_items.");
                }
            }
        }
        FieldType::RadioButtons => {
            *leaf_data_count += 1;
            let _ = spec.radio_layout()?;
            validate_option_list(
                &spec.options,
                MAX_RADIO_OPTIONS,
                spec.css_var.is_some(),
                true,
            )?;
            let opt_vals: BTreeSet<&str> = spec.options.iter().map(|o| o.value.as_str()).collect();
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định radio_buttons phải là chuỗi.");
                };
                if !(s.is_empty() && !spec.required) && !opt_vals.contains(s) {
                    return Err("Giá trị mặc định radio_buttons không có trong options.");
                }
            }
        }
        FieldType::Boolean => {
            *leaf_data_count += 1;
            validate_optional_plain_text(
                spec.on_label.as_deref(),
                MAX_ON_OFF_LABEL_CHARS,
                false,
                "on_label tối đa 64 ký tự.",
            )?;
            validate_optional_plain_text(
                spec.off_label.as_deref(),
                MAX_ON_OFF_LABEL_CHARS,
                false,
                "off_label tối đa 64 ký tự.",
            )?;
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null())
                && !def.is_boolean()
            {
                return Err("Giá trị mặc định boolean phải là true hoặc false.");
            }
        }
        FieldType::Color => {
            *leaf_data_count += 1;
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định color phải là chuỗi.");
                };
                if !s.is_empty() && !valid_color_value(s) {
                    return Err("Màu mặc định không hợp lệ.");
                }
            }
        }
        FieldType::Icon => {
            *leaf_data_count += 1;
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định icon phải là chuỗi.");
                };
                if !s.is_empty() && lookup_core_icon_paths(s).is_none() {
                    return Err("Icon mặc định không thuộc danh mục CORE_ICONS.");
                }
            }
        }
        FieldType::Date => {
            *leaf_data_count += 1;
            if let Some(mn) = spec.min_date.as_deref()
                && !valid_iso_date(mn)
            {
                return Err("min_date phải là ngày YYYY-MM-DD hợp lệ.");
            }
            if let Some(mx) = spec.max_date.as_deref()
                && !valid_iso_date(mx)
            {
                return Err("max_date phải là ngày YYYY-MM-DD hợp lệ.");
            }
            if let (Some(mn), Some(mx)) = (spec.min_date.as_deref(), spec.max_date.as_deref())
                && mn > mx
            {
                return Err("min_date không được lớn hơn max_date.");
            }
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định date phải là chuỗi.");
                };
                if !s.is_empty() {
                    if !valid_iso_date(s) {
                        return Err("Giá trị mặc định date phải có định dạng YYYY-MM-DD.");
                    }
                    if spec.min_date.as_deref().is_some_and(|mn| s < mn)
                        || spec.max_date.as_deref().is_some_and(|mx| s > mx)
                    {
                        return Err("Giá trị mặc định date nằm ngoài [min_date, max_date].");
                    }
                }
            }
        }
        FieldType::Datetime => {
            *leaf_data_count += 1;
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định datetime phải là chuỗi.");
                };
                if !s.is_empty() && !valid_iso_datetime_utc(s) {
                    return Err("Giá trị mặc định datetime phải là ISO-8601 UTC hợp lệ.");
                }
            }
        }
        FieldType::Time => {
            *leaf_data_count += 1;
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định time phải là chuỗi.");
                };
                if !s.is_empty() && !valid_iso_time(s) {
                    return Err("Giá trị mặc định time phải là HH:MM hoặc HH:MM:SS.");
                }
            }
        }
        FieldType::Spacing => {
            *leaf_data_count += 1;
            let prop = spec.effective_spacing_property();
            let lo = spec.min.unwrap_or(match prop {
                SpacingProperty::Padding => 0.0,
                SpacingProperty::Margin => -128.0,
            });
            let hi = spec.max.unwrap_or(256.0);
            let min_bound = match prop {
                SpacingProperty::Padding => 0.0,
                SpacingProperty::Margin => -256.0,
            };
            if !lo.is_finite() || !hi.is_finite() || lo < min_bound || hi > 256.0 || lo > hi {
                return Err("Khoảng min/max của spacing không hợp lệ.");
            }
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let val: SpacingValue = serde_json::from_value(def.clone())
                    .map_err(|_| "Giá trị mặc định spacing không hợp lệ.")?;
                for side in val.sides().into_iter().flatten() {
                    if !side.is_finite() || side < lo || side > hi {
                        return Err("Giá trị mặc định spacing nằm ngoài [min, max].");
                    }
                }
            }
        }
        FieldType::EntryRef => {
            *leaf_data_count += 1;
            if spec.target_types.is_empty() || spec.target_types.len() > MAX_ENTRY_REF_TYPES {
                return Err("entry_ref yêu cầu target_types có từ 1..=16 phần tử.");
            }
            if spec.target_types.iter().any(|t| t == "*") {
                if ctx != FieldContext::StudioBlock || spec.target_types.len() != 1 {
                    return Err(
                        "target_types wildcard [\"*\"] chỉ hợp lệ ở StudioBlock và cấm trộn cùng CPT cụ thể.",
                    );
                }
            } else {
                let mut seen = BTreeSet::new();
                for t in &spec.target_types {
                    if t == "pattern" || !valid_snake_key(t) || !seen.insert(t.as_str()) {
                        return Err("target_types chứa CPT không hợp lệ hoặc bị trùng.");
                    }
                }
            }
            let max_i = spec.max_items.unwrap_or(MAX_ENTRY_REF_ITEMS);
            if max_i == 0 || max_i > MAX_ENTRY_REF_ITEMS {
                return Err("max_items của entry_ref phải nằm trong 1..=50.");
            }
            if let Some(min_i) = spec.min_items
                && min_i > max_i
            {
                return Err("min_items của entry_ref lớn hơn max_items.");
            }
            if !spec.multiple
                && (spec.min_items.is_some_and(|n| n > 1) || spec.max_items.is_some_and(|n| n > 1))
            {
                return Err("entry_ref đơn không cho min_items/max_items > 1.");
            }
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                if spec.multiple {
                    let Some(arr) = def.as_array() else {
                        return Err("Giá trị mặc định entry_ref multiple phải là mảng số nguyên.");
                    };
                    if arr.len() > max_i {
                        return Err("Giá trị mặc định entry_ref vượt max_items.");
                    }
                    let mut seen = BTreeSet::new();
                    for item in arr {
                        let Some(id) = item.as_i64().filter(|n| *n > 0) else {
                            return Err("ID mặc định entry_ref phải là số nguyên dương.");
                        };
                        if !seen.insert(id) {
                            return Err("ID mặc định entry_ref không được trùng lặp.");
                        }
                    }
                } else if def.as_i64().is_none_or(|n| n <= 0) {
                    return Err("Giá trị mặc định entry_ref đơn phải là số nguyên dương.");
                }
            }
        }
        FieldType::TermRef => {
            *leaf_data_count += 1;
            if ctx == FieldContext::StudioBlock {
                match (
                    spec.taxonomy.as_deref(),
                    spec.taxonomy_from_field.as_deref(),
                ) {
                    (Some(tax), None) => {
                        if !valid_snake_key(tax) {
                            return Err("Khóa taxonomy của term_ref không hợp lệ.");
                        }
                    }
                    (None, Some(src_field)) => {
                        if in_repeater || !valid_snake_key(src_field) || src_field == spec.key {
                            return Err("taxonomy_from_field không hợp lệ hoặc tự tham chiếu.");
                        }
                        let target = lookup_condition_target(src_field, sibling_map, top_map)
                            .ok_or(
                                "taxonomy_from_field phải trỏ tới một field select đơn tồn tại.",
                            )?;
                        if target.field_type != FieldType::Select
                            || target.multiple
                            || target.responsive
                        {
                            return Err(
                                "taxonomy_from_field bắt buộc trỏ tới field select đơn không-responsive.",
                            );
                        }
                    }
                    (Some(_), Some(_)) => {
                        return Err(
                            "taxonomy và taxonomy_from_field trên term_ref loại trừ lẫn nhau.",
                        );
                    }
                    (None, None) => {
                        return Err(
                            "term_ref trên StudioBlock bắt buộc khai báo taxonomy hoặc taxonomy_from_field.",
                        );
                    }
                }
            } else {
                if spec.taxonomy_from_field.is_some() {
                    return Err("taxonomy_from_field chỉ hợp lệ ở StudioBlock.");
                }
                let Some(tax) = spec.taxonomy.as_deref() else {
                    return Err("term_ref bắt buộc khai báo taxonomy.");
                };
                if !valid_snake_key(tax) {
                    return Err("Khóa taxonomy của term_ref không hợp lệ.");
                }
            }
            let max_i = spec.max_items.unwrap_or(MAX_TERM_REF_ITEMS);
            if max_i == 0 || max_i > MAX_TERM_REF_ITEMS {
                return Err("max_items của term_ref phải nằm trong 1..=50.");
            }
            if let Some(min_i) = spec.min_items
                && min_i > max_i
            {
                return Err("min_items của term_ref lớn hơn max_items.");
            }
            if !spec.multiple
                && (spec.min_items.is_some_and(|n| n > 1) || spec.max_items.is_some_and(|n| n > 1))
            {
                return Err("term_ref đơn không cho min_items/max_items > 1.");
            }
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                if spec.multiple {
                    let Some(arr) = def.as_array() else {
                        return Err("Giá trị mặc định term_ref multiple phải là mảng số nguyên.");
                    };
                    if arr.len() > max_i {
                        return Err("Giá trị mặc định term_ref vượt max_items.");
                    }
                    let mut seen = BTreeSet::new();
                    for item in arr {
                        let Some(id) = item.as_i64().filter(|n| *n > 0) else {
                            return Err("ID mặc định term_ref phải là số nguyên dương.");
                        };
                        if !seen.insert(id) {
                            return Err("ID mặc định term_ref không được trùng lặp.");
                        }
                    }
                } else if def.as_i64().is_none_or(|n| n <= 0) {
                    return Err("Giá trị mặc định term_ref đơn phải là số nguyên dương.");
                }
            }
        }
        FieldType::Group => {
            if spec.fields.is_empty() {
                return Err("Field group phải có ít nhất 1 field con.");
            }
            let next_depth = group_depth + 1;
            if next_depth > MAX_GROUP_DEPTH {
                return Err("group chỉ hỗ trợ độ sâu tối đa 2 cấp.");
            }
            if in_repeater && next_depth > 1 {
                return Err("Trong repeater chỉ cho phép group phẳng 1 cấp.");
            }
            validate_scope_specs(
                &spec.fields,
                ctx,
                top_map,
                next_depth,
                in_repeater,
                seen_css_vars,
                total_block_repeater_max,
                leaf_data_count,
            )?;
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(obj) = def.as_object() else {
                    return Err("Giá trị mặc định group phải là JSON object.");
                };
                let map: BTreeMap<String, Value> =
                    obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                validate_map_against_specs(
                    &spec.fields,
                    ctx,
                    &map,
                    None,
                    &spec.fields,
                    &map,
                    None,
                    true,
                    true,
                )?;
            }
        }
        FieldType::Repeater => {
            if spec.fields.is_empty() {
                return Err("Field repeater phải có ít nhất 1 field con.");
            }
            if in_repeater {
                return Err("V1 không hỗ trợ repeater lồng bên trong repeater.");
            }
            if group_depth >= MAX_GROUP_DEPTH {
                return Err("Không được đặt repeater bên trong group cấp 2.");
            }
            let _ = spec.repeater_layout()?;
            validate_optional_plain_text(
                spec.button_label.as_deref(),
                MAX_BUTTON_LABEL_CHARS,
                false,
                "button_label tối đa 64 ký tự.",
            )?;
            let max_allowed = if ctx == FieldContext::StudioBlock {
                MAX_BLOCK_REPEATER_ITEMS
            } else {
                MAX_REPEATER_ITEMS
            };
            let max_items = spec.max_items.unwrap_or(max_allowed);
            if max_items == 0 || max_items > max_allowed {
                return Err("max_items của repeater vượt giới hạn cho phép.");
            }
            if let Some(min_i) = spec.min_items
                && min_i > max_items
            {
                return Err("min_items của repeater lớn hơn max_items.");
            }
            if ctx == FieldContext::StudioBlock {
                *total_block_repeater_max += max_items;
            }
            if let Some(item_label) = spec.item_label_field.as_deref() {
                let has_target = spec.fields.iter().any(|sub| {
                    sub.key == item_label && !sub.is_ui_only() && !sub.field_type.is_container()
                });
                if !has_target {
                    return Err(
                        "item_label_field phải trỏ tới một trường nguyên thủy tồn tại trong repeater.",
                    );
                }
            }
            validate_scope_specs(
                &spec.fields,
                ctx,
                top_map,
                group_depth,
                true,
                seen_css_vars,
                total_block_repeater_max,
                leaf_data_count,
            )?;
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(arr) = def.as_array() else {
                    return Err("Giá trị mặc định repeater phải là mảng.");
                };
                if arr.len() > max_items {
                    return Err("Giá trị mặc định repeater vượt max_items.");
                }
                if !arr.is_empty() && spec.min_items.is_some_and(|mn| arr.len() < mn) {
                    return Err("Giá trị mặc định repeater ít hơn min_items.");
                }
                for item in arr {
                    let Some(obj) = item.as_object() else {
                        return Err("Phần tử mặc định repeater phải là JSON object.");
                    };
                    let map: BTreeMap<String, Value> =
                        obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                    validate_map_against_specs(
                        &spec.fields,
                        ctx,
                        &map,
                        None,
                        &spec.fields,
                        &map,
                        None,
                        true,
                        true,
                    )?;
                }
            }
        }
    }

    if let Some(conds) = spec.conditions.as_ref() {
        validate_conditions(conds, spec, ctx, sibling_map, top_map)?;
    }

    Ok(())
}
