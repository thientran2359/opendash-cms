use super::*;

pub(crate) use crate::{
    error::{ApiError, ApiResult},
    public_context::CanonicalUrlBuilder,
    storage::{
        builder::{BuilderRenderContext, ListResults, ResolvedList},
        builder_binding::{BindingResult, BindingResults},
    },
};

pub(crate) use opendash_core::{
    builder::{
        BindingTarget, BoxSpacing, BuilderDocument, ElementContent, KnownElement, NodeMeta,
        Responsive, ResponsiveStyle, SectionWidth, TextAlign, compile_box_spacing_tailwind,
        gap_px_to_tw_step,
    },
    id::valid_package_id,
};

pub(crate) use serde::Serialize;

pub use crate::template_sec::RenderBudget;

#[derive(Debug, Clone)]
pub struct ValidatedBlockTemplate {
    pub block_id: String,
    pub source: String,
}

#[derive(Debug, Default)]
pub struct BlockTemplateCache {
    pub(crate) inner: std::sync::RwLock<
        std::collections::BTreeMap<(String, String), std::sync::Arc<ValidatedBlockTemplate>>,
    >,
}

#[derive(Debug, Serialize)]
pub struct BuilderDiagnostic {
    pub node_id: String,
    pub code: &'static str,
}

#[derive(Debug, Serialize)]
pub struct RenderedLayout {
    #[serde(skip_serializing)]
    pub has_active_contact_form: bool,
    pub html: String,
    pub diagnostics: Vec<BuilderDiagnostic>,
}

#[derive(Clone, Copy)]
pub(crate) enum ContainerKind {
    Section,
    Row,
    Column,
}

pub(crate) struct ContainerRenderPlan {
    pub(crate) tailwind_classes: Vec<String>,
    pub(crate) fallback_desktop_css: String,
    pub(crate) fallback_tablet_css: String,
    pub(crate) fallback_mobile_css: String,
}

pub(crate) fn push_scoped_rule(target: &mut String, id: &str, properties: &str) {
    // These IDs are validated on save; also refuse invalid historical IDs
    // so no untrusted value can enter the CSS selector or style element.
    if !properties.is_empty() && valid_package_id(id) {
        target.push_str(&format!(
            "main[data-builder-version=\"1\"] [data-node-id=\"{id}\"]{{{properties}}}"
        ));
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn collect_scoped_css_for_fields(
    fields: &[opendash_core::field_schema::FieldSpec],
    values: &std::collections::BTreeMap<String, serde_json::Value>,
    path_prefix: Option<&str>,
    node_scope: &str,
    is_preview: bool,
    hydration_media: &std::collections::BTreeMap<String, crate::storage::fields::HydratedMediaRow>,
    desktop_props: &mut String,
    tablet_props: &mut String,
    mobile_props: &mut String,
    desktop_rules: &mut String,
    tablet_rules: &mut String,
    mobile_rules: &mut String,
) {
    use opendash_core::field_schema::{
        FieldType, MediaFieldKind, SpacingProperty, SpacingValue, valid_color_value,
        valid_css_select_value, valid_css_var_name, valid_sha256_media_id,
    };

    for spec in fields {
        if spec.is_ui_only() || spec.field_type == FieldType::Secret {
            continue;
        }
        let field_path = match path_prefix {
            Some(prefix) => format!("{prefix}.{}", spec.key),
            None => spec.key.clone(),
        };
        match spec.field_type {
            FieldType::Group => {
                let child_values: std::collections::BTreeMap<String, serde_json::Value> = values
                    .get(&spec.key)
                    .and_then(serde_json::Value::as_object)
                    .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .unwrap_or_default();
                collect_scoped_css_for_fields(
                    &spec.fields,
                    &child_values,
                    Some(&field_path),
                    node_scope,
                    is_preview,
                    hydration_media,
                    desktop_props,
                    tablet_props,
                    mobile_props,
                    desktop_rules,
                    tablet_rules,
                    mobile_rules,
                );
            }
            FieldType::Number | FieldType::Range => {
                let Some(var_name) = spec.css_var.as_deref() else {
                    continue;
                };
                if !valid_css_var_name(var_name) {
                    continue;
                }
                let default_num = spec
                    .default
                    .as_ref()
                    .and_then(serde_json::Value::as_f64)
                    .or(if spec.field_type == FieldType::Range {
                        spec.min
                    } else {
                        None
                    });
                let raw = values.get(&spec.key);
                if spec.responsive
                    && let Some(obj) = raw.and_then(|v| v.as_object())
                {
                    if let Some(d) = obj
                        .get("desktop")
                        .and_then(|v| v.as_f64())
                        .or(default_num)
                        .and_then(|n| format_css_number(n, spec.unit))
                    {
                        desktop_props.push_str(&format!("{var_name}:{d};"));
                    }
                    if let Some(t) = obj
                        .get("tablet")
                        .and_then(|v| v.as_f64())
                        .and_then(|n| format_css_number(n, spec.unit))
                    {
                        tablet_props.push_str(&format!("{var_name}:{t};"));
                    }
                    if let Some(m) = obj
                        .get("mobile")
                        .and_then(|v| v.as_f64())
                        .and_then(|n| format_css_number(n, spec.unit))
                    {
                        mobile_props.push_str(&format!("{var_name}:{m};"));
                    }
                } else if let Some(d) = raw
                    .and_then(|v| v.as_f64())
                    .or(default_num)
                    .and_then(|n| format_css_number(n, spec.unit))
                {
                    desktop_props.push_str(&format!("{var_name}:{d};"));
                }
            }
            FieldType::Select | FieldType::RadioButtons => {
                let Some(var_name) = spec.css_var.as_deref() else {
                    continue;
                };
                if !valid_css_var_name(var_name) {
                    continue;
                }
                let default_str = spec.default.as_ref().and_then(serde_json::Value::as_str);
                let raw = values.get(&spec.key);
                if spec.responsive
                    && let Some(obj) = raw.and_then(|v| v.as_object())
                {
                    if let Some(d) = obj
                        .get("desktop")
                        .and_then(|v| v.as_str())
                        .or(default_str)
                        .filter(|s| valid_css_select_value(s))
                    {
                        desktop_props.push_str(&format!("{var_name}:{d};"));
                    }
                    if let Some(t) = obj
                        .get("tablet")
                        .and_then(|v| v.as_str())
                        .filter(|s| valid_css_select_value(s))
                    {
                        tablet_props.push_str(&format!("{var_name}:{t};"));
                    }
                    if let Some(m) = obj
                        .get("mobile")
                        .and_then(|v| v.as_str())
                        .filter(|s| valid_css_select_value(s))
                    {
                        mobile_props.push_str(&format!("{var_name}:{m};"));
                    }
                } else if let Some(d) = raw
                    .and_then(|v| v.as_str())
                    .or(default_str)
                    .filter(|s| valid_css_select_value(s))
                {
                    desktop_props.push_str(&format!("{var_name}:{d};"));
                }
            }
            FieldType::Spacing => {
                let css_unit = spec.effective_unit();
                let spacing_prop = spec.effective_spacing_property();
                let default_sv = spec
                    .default
                    .as_ref()
                    .and_then(|v| serde_json::from_value::<SpacingValue>(v.clone()).ok());
                let raw = values.get(&spec.key);
                let (d_sv, t_sv, m_sv) = crate::template_sec::extract_spacing_breakpoint_values(
                    raw,
                    default_sv,
                    spec.responsive,
                );
                if let Some(var_name) = spec.css_var.as_deref() {
                    if !valid_css_var_name(var_name) {
                        continue;
                    }
                    if let Some(d_css) = d_sv.and_then(|sv| sv.format_css(css_unit)) {
                        desktop_props.push_str(&format!("{var_name}:{d_css};"));
                    }
                    if let Some(t_css) = t_sv.and_then(|sv| sv.format_css(css_unit)) {
                        tablet_props.push_str(&format!("{var_name}:{t_css};"));
                    }
                    if let Some(m_css) = m_sv.and_then(|sv| sv.format_css(css_unit)) {
                        mobile_props.push_str(&format!("{var_name}:{m_css};"));
                    }
                } else if crate::template_sec::spacing_field_tailwind_classes(
                    raw,
                    default_sv,
                    spec.responsive,
                    css_unit,
                    spacing_prop,
                )
                .is_none()
                {
                    let css_prop = match spacing_prop {
                        SpacingProperty::Padding => "padding",
                        SpacingProperty::Margin => "margin",
                    };
                    if let Some(d_css) = d_sv.and_then(|sv| sv.format_css(css_unit)) {
                        desktop_rules.push_str(&format!(
                            "[data-od-node-scope=\"{node_scope}\"] [data-od-spacing=\"{field_path}\"]{{{css_prop}:{d_css};}}"
                        ));
                    }
                    if let Some(t_css) = t_sv.and_then(|sv| sv.format_css(css_unit)) {
                        tablet_rules.push_str(&format!(
                            "[data-od-node-scope=\"{node_scope}\"] [data-od-spacing=\"{field_path}\"]{{{css_prop}:{t_css};}}"
                        ));
                    }
                    if let Some(m_css) = m_sv.and_then(|sv| sv.format_css(css_unit)) {
                        mobile_rules.push_str(&format!(
                            "[data-od-node-scope=\"{node_scope}\"] [data-od-spacing=\"{field_path}\"]{{{css_prop}:{m_css};}}"
                        ));
                    }
                }
            }
            FieldType::Color => {
                let Some(var_name) = spec.css_var.as_deref() else {
                    continue;
                };
                if !valid_css_var_name(var_name) {
                    continue;
                }
                let default_str = spec.default.as_ref().and_then(serde_json::Value::as_str);
                if let Some(c) = values
                    .get(&spec.key)
                    .and_then(|v| v.as_str())
                    .or(default_str)
                    .filter(|s| !s.is_empty() && valid_color_value(s))
                {
                    desktop_props.push_str(&format!("{var_name}:{c};"));
                }
            }
            FieldType::Media if spec.effective_media_kind() == MediaFieldKind::Image => {
                let Some(var_name) = spec.css_var.as_deref() else {
                    continue;
                };
                if !valid_css_var_name(var_name) {
                    continue;
                }
                let default_str = spec.default.as_ref().and_then(serde_json::Value::as_str);
                if let Some(id) = values
                    .get(&spec.key)
                    .and_then(|v| v.as_str())
                    .or(default_str)
                    .filter(|s| !s.is_empty() && valid_sha256_media_id(s))
                {
                    let url_path = super::media_render::versioned_media_file_url(
                        id,
                        hydration_media,
                        is_preview,
                    );
                    desktop_props.push_str(&format!("{var_name}:url(\"{url_path}\");"));
                }
            }
            _ => {}
        }
    }
}

/// Emits scoped CSS custom properties (`--od-*`) for all active, declared `block.json` v1
/// `PluginBlock` instances in `document` under `[data-od-node-scope="{scope}-{node_id}"]`.
/// The S02 public resolver supplies already sanitized entry HTML and verified
/// active theme parts. Ordinary page Builder calls have no template context.
pub struct BuilderTemplateContext<'a> {
    pub content_html: Option<&'a str>,
    pub parts: &'a std::collections::BTreeMap<String, String>,
    pub search: Option<BuilderSearchData<'a>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderAnnotationScope {
    #[default]
    None,
    TemplateAll,
    EntryBodyOnly,
}

#[allow(clippy::too_many_arguments)]
pub fn render_builder_with_budget_and_base_path(
    state: &BuilderRenderContext,
    document: &BuilderDocument,
    is_preview: bool,
    bindings: &BindingResults,
    lists: &ListResults,
    forms: &ContactFormResults,
    context: Option<&BuilderTemplateContext<'_>>,
    patterns: &PatternResults,
    pattern_document: bool,
    annotation_scope: RenderAnnotationScope,
    scope: &str,
    list_base_path: &str,
    budget: &mut RenderBudget,
    cache: &BlockTemplateCache,
) -> ApiResult<RenderedLayout> {
    // Keep the original v1 main/section child structure intact; the optional
    // stylesheet precedes the root and every selector is scoped to that root.
    let mut html = builder_responsive_css(document, is_preview, &state.hydration_media);
    let block_css = serialize_scoped_block_css(state, document, scope, is_preview)?;
    html.push_str(&block_css);
    if pattern_document {
        html.push_str("<div class=\"od-pattern-content\" data-builder-version=\"1\">");
    } else {
        html.push_str("<main data-builder-version=\"1\">");
    }
    let mut already_charged_bytes = 0usize;
    let mut diagnostics = Vec::new();
    let mut has_active_contact_form = false;
    let sec_attrs = editable_attrs(annotation_scope, is_preview, pattern_document, "section");
    let row_attrs = editable_attrs(annotation_scope, is_preview, pattern_document, "row");
    let col_attrs = editable_attrs(annotation_scope, is_preview, pattern_document, "column");
    let el_attrs = editable_attrs(annotation_scope, is_preview, pattern_document, "element");
    for section in &document.sections {
        if !section.is_synthetic() {
            let sec_id_attr = meta_id_attr(section.meta.as_ref());
            let sec_plan =
                plan_container_styles(section.responsive.as_ref(), ContainerKind::Section, None);
            let sec_class_attr =
                merge_class_attr(None, &sec_plan.tailwind_classes, section.meta.as_ref());
            html.push_str(&format!(
                "<section{sec_id_attr}{sec_class_attr} data-node-id=\"{}\"{sec_attrs}>",
                escape_html(&section.id)
            ));
        }
        let row_section_width = (!section.is_synthetic())
            .then(|| section.style.as_ref().and_then(|s| s.content_width))
            .flatten();
        for row in &section.children {
            if !row.is_synthetic() {
                let row_id_attr = meta_id_attr(row.meta.as_ref());
                let row_plan = plan_container_styles(
                    row.responsive.as_ref(),
                    ContainerKind::Row,
                    row_section_width,
                );
                let row_class_attr = merge_class_attr(
                    Some("builder-row"),
                    &row_plan.tailwind_classes,
                    row.meta.as_ref(),
                );
                html.push_str(&format!(
                    "<div{row_id_attr}{row_class_attr} data-node-id=\"{}\"{row_attrs}>",
                    escape_html(&row.id)
                ));
            }
            for column in &row.children {
                if !column.is_synthetic() {
                    let col_id_attr = meta_id_attr(column.meta.as_ref());
                    let col_plan = plan_container_styles(
                        column.responsive.as_ref(),
                        ContainerKind::Column,
                        None,
                    );
                    let col_class_attr = merge_class_attr(
                        Some("builder-column"),
                        &col_plan.tailwind_classes,
                        column.meta.as_ref(),
                    );
                    html.push_str(&format!(
                        "<div{col_id_attr}{col_class_attr} data-node-id=\"{}\"{col_attrs}>",
                        escape_html(&column.id)
                    ));
                }
                for element in &column.children {
                    let mut materialized = None;
                    if let ElementContent::Known(known) = &element.content {
                        if let Some(binding) = known.binding() {
                            let value = match bindings.get(&element.id) {
                                Some(BindingResult::Value(value)) => value,
                                result => {
                                    diagnostics.push(BuilderDiagnostic {
                                        node_id: element.id.clone(),
                                        code: match result {
                                            Some(BindingResult::Incompatible) => {
                                                "binding_incompatible"
                                            }
                                            Some(BindingResult::UrlRejected) => {
                                                "binding_url_rejected"
                                            }
                                            _ => "binding_unavailable",
                                        },
                                    });
                                    continue;
                                }
                            };
                            let mut resolved = known.clone();
                            match (&mut resolved, binding.target) {
                                (KnownElement::Heading(props), BindingTarget::Text) => {
                                    props.text = value.clone()
                                }
                                (KnownElement::Text(props), BindingTarget::Text) => {
                                    props.text = value.clone()
                                }
                                (KnownElement::Image(props), BindingTarget::MediaId) => {
                                    props.media_id = value.clone()
                                }
                                (KnownElement::Image(props), BindingTarget::Alt) => {
                                    props.alt = value.clone()
                                }
                                (KnownElement::Button(props), BindingTarget::Label) => {
                                    props.label = value.clone()
                                }
                                (KnownElement::Button(props), BindingTarget::Url) => {
                                    props.url = value.clone()
                                }
                                _ => {
                                    diagnostics.push(BuilderDiagnostic {
                                        node_id: element.id.clone(),
                                        code: "binding_incompatible",
                                    });
                                    continue;
                                }
                            }
                            materialized = Some(ElementContent::Known(resolved));
                        }
                    }
                    let rendered = match materialized.as_ref().unwrap_or(&element.content) {
                        ElementContent::Known(KnownElement::Heading(props)) => Some(format!(
                            "<h{level}>{text}</h{level}>",
                            level = props.level,
                            text = escape_html(&props.text)
                        )),
                        ElementContent::Known(KnownElement::Text(props)) => {
                            if props.rich && props.binding.is_none() {
                                let clean = opendash_core::richtext::sanitize_richtext(&props.text)
                                    .unwrap_or_else(|_| escape_html(&props.text));
                                Some(format!("<div class=\"od-rich-text\">{clean}</div>"))
                            } else {
                                Some(format!("<p>{}</p>", escape_html(&props.text)))
                            }
                        }
                        ElementContent::Known(KnownElement::Image(props)) => {
                            Some(super::media_render::render_builder_image_html(
                                props,
                                &state.hydration_media,
                                is_preview,
                            ))
                        }
                        ElementContent::Known(KnownElement::Video(props)) => {
                            Some(super::media_render::render_builder_video_html(
                                props,
                                &state.hydration_media,
                                is_preview,
                            ))
                        }
                        ElementContent::Known(KnownElement::Button(props)) => {
                            let target_attr = if props.new_tab {
                                " target=\"_blank\""
                            } else {
                                ""
                            };
                            let rel_attr = match (props.new_tab, props.nofollow) {
                                (true, true) => "noopener noreferrer nofollow",
                                (false, true) => "nofollow",
                                _ => "noopener noreferrer",
                            };
                            Some(format!(
                                "<a href=\"{}\"{target_attr} rel=\"{rel_attr}\">{}</a>",
                                escape_html(&props.url),
                                escape_html(&props.label)
                            ))
                        }
                        ElementContent::Known(KnownElement::List(props)) => {
                            let l10n = crate::content_render::core_l10n(&budget.locale);
                            match lists.get(&element.id) {
                                Some(ResolvedList::MissingConfig) => {
                                    diagnostics.push(BuilderDiagnostic {
                                        node_id: element.id.clone(),
                                        code: "list_config_unavailable",
                                    });
                                    Some(format!(
                                        "<p class=\"builder-list-empty\">{}</p>",
                                        l10n.list_unavailable
                                    ))
                                }
                                Some(ResolvedList::Page {
                                    items,
                                    page,
                                    has_next,
                                    out_of_range,
                                }) => {
                                    let mut list_html = format!(
                                        "<div class=\"builder-list\" data-list-type=\"{}\">",
                                        escape_html(&props.type_key),
                                    );
                                    if *out_of_range {
                                        list_html.push_str(&format!("<p>{}</p>", l10n.page_empty));
                                        list_html.push_str(&format!(
                                            "<a class=\"builder-list-first\" href=\"{}\">{}</a>",
                                            escape_html(&list_page_href(
                                                list_base_path,
                                                lists,
                                                &element.id,
                                                1
                                            )),
                                            l10n.back_to_first_page,
                                        ));
                                    } else if items.is_empty() {
                                        list_html.push_str(&format!(
                                            "<p class=\"builder-list-empty\">{}</p>",
                                            l10n.empty_content,
                                        ));
                                    } else {
                                        list_html.push_str("<ul>");
                                        for item in items {
                                            // Never manufacture a legacy link; the
                                            // href comes from the snapshot resolver.
                                            if let Some(path) = &item.href {
                                                list_html.push_str(&format!(
                                                    "<li><a href=\"{}\">{}</a></li>",
                                                    escape_html(&path),
                                                    escape_html(&item.title),
                                                ));
                                            } else {
                                                list_html.push_str(&format!(
                                                    "<li>{}</li>",
                                                    escape_html(&item.title),
                                                ));
                                            }
                                        }
                                        list_html.push_str("</ul>");
                                        if !is_preview && *page > 1 {
                                            list_html.push_str(&format!(
                                                "<a class=\"builder-list-prev\" href=\"{}\">{}</a>",
                                                escape_html(&list_page_href(
                                                    list_base_path,
                                                    lists,
                                                    &element.id,
                                                    page - 1
                                                )),
                                                l10n.list_prev,
                                            ));
                                        }
                                        if !is_preview && *has_next && *page < 100 {
                                            list_html.push_str(&format!(
                                                "<a class=\"builder-list-next\" href=\"{}\">{}</a>",
                                                escape_html(&list_page_href(
                                                    list_base_path,
                                                    lists,
                                                    &element.id,
                                                    page + 1
                                                )),
                                                l10n.list_next,
                                            ));
                                        }
                                    }
                                    list_html.push_str("</div>");
                                    Some(list_html)
                                }
                                None => {
                                    diagnostics.push(BuilderDiagnostic {
                                        node_id: element.id.clone(),
                                        code: "list_unavailable",
                                    });
                                    None
                                }
                            }
                        }
                        ElementContent::Known(KnownElement::ContactForm(props)) => {
                            let l10n = crate::content_render::core_l10n(&budget.locale);
                            match forms.get(&element.id).and_then(|opt| opt.as_ref()) {
                                Some(form) if form.status == "active" => {
                                    let rendered = render_contact_form_html(
                                        &element.id,
                                        form,
                                        &props.submit_label,
                                        is_preview,
                                        &l10n,
                                    );
                                    if !is_preview {
                                        has_active_contact_form = true;
                                    }
                                    Some(rendered)
                                }
                                _ => {
                                    diagnostics.push(BuilderDiagnostic {
                                        node_id: element.id.clone(),
                                        code: "contact_form_unavailable",
                                    });
                                    Some(format!(
                                        "<p class=\"builder-contact-unavailable\">{}</p>",
                                        l10n.contact_unavailable
                                    ))
                                }
                            }
                        }
                        ElementContent::Known(KnownElement::PluginBlock(props)) => {
                            use opendash_core::registry::{DeclarationState, ProviderStatus};
                            let node_scope = format!("{scope}-{}", element.id);
                            if let Some(def) = state.declared_blocks.get(&props.block_id) {
                                if def.status != ProviderStatus::Active
                                    || def.declaration_state != DeclarationState::Declared
                                {
                                    diagnostics.push(BuilderDiagnostic {
                                        node_id: element.id.clone(),
                                        code: "plugin_block_unavailable",
                                    });
                                    Some(format!(
                                        "<div class=\"builder-block-unavailable\" data-block-id=\"{}\"></div>",
                                        escape_html(&props.block_id)
                                    ))
                                } else {
                                    let hydration_ctx = crate::field_hydrate::HydrationContext {
                                        media: &state.hydration_media,
                                        entries: &state.hydration_entries,
                                        terms: &state.hydration_terms,
                                        is_preview,
                                        tag_media_provenance: true,
                                    };
                                    let mut local_rt =
                                        crate::template_sec::RichTextNonceTable::default();
                                    let render_res = cache
                                        .get_or_validate(
                                            &props.block_id,
                                            &def.schema.schema,
                                            &def.template,
                                        )
                                        .and_then(|compiled| {
                                            let hydrated =
                                                crate::field_hydrate::hydrate_field_values(
                                                    &def.schema.schema.fields,
                                                    &props.values,
                                                    &hydration_ctx,
                                                    Some(&mut local_rt),
                                                    crate::field_hydrate::HydrationVisibility::All,
                                                )?;
                                            let fmt_cfg =
                                                crate::template_sec::RenderFormattingConfig {
                                                    timezone: state.timezone.clone(),
                                                    ..Default::default()
                                                };
                                            let menus_opt =
                                                (!state.menus.is_empty()).then_some(&state.menus);
                                            crate::template_sec::render_block_csec_with_inputs(
                                                &compiled.block_id,
                                                &compiled.source,
                                                crate::template_sec::BlockRenderInputs {
                                                    hydrated_props: &hydrated,
                                                    local_richtext_nonces: Some(local_rt),
                                                    merged_options: Some(&state.options),
                                                    site_options: Some(&state.site_options),
                                                    menus: menus_opt,
                                                    is_preview,
                                                    fmt_cfg: &fmt_cfg,
                                                },
                                                budget,
                                            )
                                        });
                                    match render_res {
                                        Ok(rendered_inner) => {
                                            if !is_preview
                                                && def
                                                    .schema
                                                    .schema
                                                    .has_active_contact_form(&props.values)
                                            {
                                                has_active_contact_form = true;
                                            }
                                            already_charged_bytes = already_charged_bytes
                                                .saturating_add(rendered_inner.len());
                                            Some(format!(
                                                "<div class=\"builder-plugin-block\" data-block-id=\"{}\" data-od-node-scope=\"{}\">{}</div>",
                                                escape_html(&props.block_id),
                                                escape_html(&node_scope),
                                                rendered_inner
                                            ))
                                        }
                                        Err(_) => {
                                            if budget.html_bytes_used
                                                > crate::template_sec::MAX_RENDERED_HTML_BYTES
                                            {
                                                return Err(ApiError::bad_request(
                                                    "Builder template composition exceeds 512 KiB.",
                                                ));
                                            }
                                            diagnostics.push(BuilderDiagnostic {
                                                node_id: element.id.clone(),
                                                code: "plugin_block_render_error",
                                            });
                                            Some(format!(
                                                "<div class=\"builder-block-unavailable\" data-block-id=\"{}\"></div>",
                                                escape_html(&props.block_id)
                                            ))
                                        }
                                    }
                                }
                            } else {
                                diagnostics.push(BuilderDiagnostic {
                                    node_id: element.id.clone(),
                                    code: "plugin_block_unavailable",
                                });
                                Some(format!(
                                    "<div class=\"builder-block-unavailable\" data-block-id=\"{}\"></div>",
                                    escape_html(&props.block_id)
                                ))
                            }
                        }
                        ElementContent::Known(KnownElement::SearchForm(props)) => {
                            let rendered = render_search_form_block(
                                state,
                                props,
                                context,
                                is_preview,
                                &budget.locale,
                            );
                            Some(rendered)
                        }
                        ElementContent::Known(KnownElement::SearchResults(props)) => {
                            render_search_results_block(
                                &element.id,
                                props,
                                context,
                                is_preview,
                                &budget.locale,
                                &mut diagnostics,
                            )
                        }
                        ElementContent::Known(KnownElement::ContentSlot(_)) => {
                            match context.and_then(|ctx| ctx.content_html) {
                                Some(slot_html) => {
                                    already_charged_bytes =
                                        already_charged_bytes.saturating_add(slot_html.len());
                                    Some(slot_html.to_owned())
                                }
                                None => {
                                    diagnostics.push(BuilderDiagnostic {
                                        node_id: element.id.clone(),
                                        code: "template_context_unavailable",
                                    });
                                    None
                                }
                            }
                        }
                        ElementContent::Known(KnownElement::PatternRef(props)) => {
                            match patterns.get(&props.pattern_id) {
                                Some(pattern) => {
                                    has_active_contact_form |= pattern.has_active_contact_form;
                                    already_charged_bytes =
                                        already_charged_bytes.saturating_add(pattern.html.len());
                                    Some(format!(
                                        "<div class=\"od-pattern-ref\" data-pattern-id=\"{}\">{}</div>",
                                        props.pattern_id, pattern.html,
                                    ))
                                }
                                None => {
                                    diagnostics.push(BuilderDiagnostic {
                                        node_id: element.id.clone(),
                                        code: "pattern_missing",
                                    });
                                    if is_preview {
                                        let l10n = crate::content_render::core_l10n(&budget.locale);
                                        Some(format!(
                                            "<div class=\"od-pattern-missing\" role=\"status\">{}</div>",
                                            l10n.pattern_missing
                                        ))
                                    } else {
                                        None
                                    }
                                }
                            }
                        }
                        ElementContent::Known(KnownElement::TemplatePart(props)) => {
                            match context.and_then(|ctx| ctx.parts.get(&props.part_key)) {
                                Some(part_html) => {
                                    already_charged_bytes =
                                        already_charged_bytes.saturating_add(part_html.len());
                                    Some(part_html.clone())
                                }
                                None => {
                                    diagnostics.push(BuilderDiagnostic {
                                        node_id: element.id.clone(),
                                        code: "template_part_unavailable",
                                    });
                                    None
                                }
                            }
                        }
                        ElementContent::Known(KnownElement::Legacy(_))
                        | ElementContent::Unresolved(_) => {
                            diagnostics.push(BuilderDiagnostic {
                                node_id: element.id.clone(),
                                code: "descriptor_missing",
                            });
                            None
                        }
                    };
                    if let Some(fragment) = rendered {
                        if html.len().saturating_add(fragment.len())
                            > crate::template_sec::MAX_RENDERED_HTML_BYTES
                        {
                            return Err(ApiError::bad_request(
                                "Builder template composition exceeds 512 KiB.",
                            ));
                        }
                        let el_id_attr = meta_id_attr(element.meta.as_ref());
                        let el_class_attr =
                            merge_class_attr(Some("builder-element"), &[], element.meta.as_ref());
                        html.push_str(&format!(
                            "<div{el_id_attr}{el_class_attr} data-node-id=\"{}\"{el_attrs}>{fragment}</div>",
                            escape_html(&element.id)
                        ));
                    }
                }
                if !column.is_synthetic() {
                    html.push_str("</div>");
                }
            }
            if !row.is_synthetic() {
                html.push_str("</div>");
            }
        }
        if !section.is_synthetic() {
            html.push_str("</section>");
        }
    }
    html.push_str(if pattern_document {
        "</div>"
    } else {
        "</main>"
    });
    if html.len() > crate::template_sec::MAX_RENDERED_HTML_BYTES {
        return Err(ApiError::bad_request(
            "Builder template composition exceeds 512 KiB.",
        ));
    }
    let own_bytes = html.len().saturating_sub(already_charged_bytes);
    budget
        .charge_html_bytes(own_bytes)
        .map_err(ApiError::bad_request)?;
    Ok(RenderedLayout {
        html,
        diagnostics,
        has_active_contact_form,
    })
}
