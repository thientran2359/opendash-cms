use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_context_capabilities<'a>(
    spec: &'a FieldSpec,
    ctx: FieldContext,
    in_group: bool,
    in_repeater: bool,
    seen_css_vars: &mut BTreeSet<&'a str>,
) -> Result<(), &'static str> {
    let ft = spec.field_type;

    if spec.acl.is_some()
        && !matches!(
            ctx,
            FieldContext::Content | FieldContext::Term | FieldContext::Options
        )
    {
        return Err("acl chỉ hợp lệ trên Content, Term và Options.");
    }
    if spec.declaration_state.is_some()
        && !matches!(
            ctx,
            FieldContext::Content | FieldContext::Term | FieldContext::Options
        )
    {
        return Err("declaration_state chỉ hợp lệ trên Content, Term và Options.");
    }
    if ctx == FieldContext::Options
        && !in_group
        && !in_repeater
        && let Some(acl) = &spec.acl
    {
        let min_view = acl.min_view_role.unwrap_or(FieldRole::Editor);
        if spec.public
            && (min_view == FieldRole::Admin
                || acl.public_exposure == Some(PublicExposure::Private))
        {
            return Err(
                "invalid_field_acl: Trường Options bật public = true phải có min_view_role = editor và không được đặt public_exposure = private.",
            );
        }
        if !spec.public && acl.public_exposure == Some(PublicExposure::Public) {
            return Err(
                "invalid_field_acl: Trường Options tắt public = false không được đặt public_exposure = public.",
            );
        }
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
        if ctx != FieldContext::Content
            || ft != FieldType::TermRef
            || in_group
            || in_repeater
            || spec.taxonomy_from_field.is_some()
        {
            return Err(
                "sync_entry_terms chỉ áp dụng cho trường term_ref top-level trong Content với taxonomy cố định.",
            );
        }
        if spec.effective_acl().1 == FieldRole::Admin {
            return Err(
                "invalid_field_acl: Trường term_ref bật sync_entry_terms không được đặt min_edit_role = admin.",
            );
        }
    }

    Ok(())
}
