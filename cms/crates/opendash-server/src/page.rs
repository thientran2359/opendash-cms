//! Page use cases shared by preview, save and public rendering.
use crate::{
    error::{ApiError, ApiResult},
    hooks::HookDispatcher,
    public_context::CanonicalUrlBuilder,
    storage::{
        builder::{HomeState, ListResults, ResolvedList},
        builder_binding::{BindingResult, BindingResults},
        page::load_theme_block,
        theme::{self as theme_storage, HomeSnapshot, SelectedTemplate},
    },
    theme::{self, Block},
};
use opendash_core::{
    builder::{
        BindingTarget, BoxSpacing, BuilderDocument, ElementContent, KnownElement, NodeMeta,
        Responsive, ResponsiveStyle, SectionWidth, TextAlign, compile_box_spacing_tailwind,
        gap_px_to_tw_step,
    },
    hooks::HookEvent,
    id::valid_package_id,
};
use serde::Serialize;
use serde_json::json;
use sqlx::SqlitePool;

pub use crate::template_sec::RenderBudget;

#[derive(Debug, Clone)]
pub struct ValidatedBlockTemplate {
    pub block_id: String,
    pub source: String,
}

#[derive(Debug, Default)]
pub struct BlockTemplateCache {
    inner: std::sync::RwLock<
        std::collections::BTreeMap<(String, String), std::sync::Arc<ValidatedBlockTemplate>>,
    >,
}

impl BlockTemplateCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn global() -> &'static std::sync::Arc<Self> {
        static GLOBAL: std::sync::OnceLock<std::sync::Arc<BlockTemplateCache>> =
            std::sync::OnceLock::new();
        GLOBAL.get_or_init(|| std::sync::Arc::new(BlockTemplateCache::new()))
    }

    pub fn get_or_validate(
        &self,
        block_id: &str,
        schema: &opendash_core::block_schema::BlockSchemaV1,
        template_source: &str,
    ) -> Result<std::sync::Arc<ValidatedBlockTemplate>, &'static str> {
        let schema_json =
            serde_json::to_string(schema).map_err(|_| "Không thể tuần tự hóa block schema.")?;
        let digest =
            crate::storage::page::compute_block_definition_digest(&schema_json, template_source);
        let key = (block_id.to_owned(), digest);
        if let Some(found) = self
            .inner
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
            .cloned()
        {
            return Ok(found);
        }
        crate::template_sec::validate_block_template_csec(block_id, template_source, schema)?;
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if let Some(found) = guard.get(&key).cloned() {
            return Ok(found);
        }
        if guard.len() >= 256 {
            guard.clear();
        }
        let validated = std::sync::Arc::new(ValidatedBlockTemplate {
            block_id: block_id.to_owned(),
            source: template_source.to_owned(),
        });
        guard.insert(key, validated.clone());
        Ok(validated)
    }
}

pub struct RenderedBlock {
    pub html: String,
    pub visibility: &'static str,
}

#[derive(Debug, Serialize)]
pub struct BuilderDiagnostic {
    pub node_id: String,
    pub code: &'static str,
}

pub type ContactFormResults =
    std::collections::BTreeMap<String, Option<crate::storage::contact::ContactFormRecord>>;

/// Pattern documents are resolved from one SQLite snapshot before rendering.
/// A missing target is deliberately absent, never client-provided HTML.
pub type PatternResults = std::collections::BTreeMap<i64, RenderedLayout>;

#[derive(Debug, Serialize)]
pub struct RenderedLayout {
    #[serde(skip_serializing)]
    pub has_active_contact_form: bool,
    pub html: String,
    pub diagnostics: Vec<BuilderDiagnostic>,
}

fn escape_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

#[derive(Clone, Copy)]
enum ContainerKind {
    Section,
    Row,
    Column,
}

fn push_box_spacing_css(css: &mut String, prop: &str, spacing: Option<BoxSpacing>) {
    let Some(s) = spacing else { return };
    if let Some(v) = s.top {
        css.push_str(&format!("{prop}-top:{v}px;"));
    }
    if let Some(v) = s.right {
        css.push_str(&format!("{prop}-right:{v}px;"));
    }
    if let Some(v) = s.bottom {
        css.push_str(&format!("{prop}-bottom:{v}px;"));
    }
    if let Some(v) = s.left {
        css.push_str(&format!("{prop}-left:{v}px;"));
    }
}

fn box_or_legacy_px(style: Option<&ResponsiveStyle>) -> Option<BoxSpacing> {
    let style = style?;
    if let Some(pad) = style.padding.filter(|b| !b.is_empty()) {
        return Some(pad);
    }
    style.padding_px.map(|px| BoxSpacing {
        top: Some(i16::from(px)),
        right: Some(i16::from(px)),
        bottom: Some(i16::from(px)),
        left: Some(i16::from(px)),
    })
}

fn align_tailwind_class(align: TextAlign) -> &'static str {
    match align {
        TextAlign::Start => "text-left",
        TextAlign::Center => "text-center",
        TextAlign::End => "text-right",
        TextAlign::Justify => "text-justify",
    }
}

struct ContainerRenderPlan {
    tailwind_classes: Vec<String>,
    fallback_desktop_css: String,
    fallback_tablet_css: String,
    fallback_mobile_css: String,
}

fn plan_container_styles(
    responsive: Option<&Responsive>,
    kind: ContainerKind,
    row_section_width: Option<SectionWidth>,
) -> ContainerRenderPlan {
    let mut tailwind_classes = Vec::new();
    let mut fallback_desktop_css = String::new();
    let mut fallback_tablet_css = String::new();
    let mut fallback_mobile_css = String::new();

    if matches!(kind, ContainerKind::Row)
        && let Some(cw) = row_section_width
    {
        match cw {
            SectionWidth::Boxed => {
                tailwind_classes.extend(["w-full", "max-w-7xl", "mx-auto"].map(str::to_owned));
            }
            SectionWidth::Wide => {
                tailwind_classes
                    .extend(["w-full", "max-w-screen-2xl", "mx-auto"].map(str::to_owned));
            }
            SectionWidth::Full => {
                tailwind_classes.extend(["w-full", "max-w-none"].map(str::to_owned));
            }
        }
    }

    let Some(resp) = responsive else {
        return ContainerRenderPlan {
            tailwind_classes,
            fallback_desktop_css,
            fallback_tablet_css,
            fallback_mobile_css,
        };
    };

    let d = resp.desktop.as_ref();
    let t = resp.tablet.as_ref();
    let m = resp.mobile.as_ref();

    let (d_pad, t_pad, m_pad) = (
        box_or_legacy_px(d),
        box_or_legacy_px(t),
        box_or_legacy_px(m),
    );
    if let Some(pad_classes) = compile_box_spacing_tailwind("p", d_pad, t_pad, m_pad, false) {
        tailwind_classes.extend(pad_classes);
    } else {
        push_box_spacing_css(&mut fallback_desktop_css, "padding", d_pad);
        push_box_spacing_css(&mut fallback_tablet_css, "padding", t_pad);
        push_box_spacing_css(&mut fallback_mobile_css, "padding", m_pad);
    }

    let (d_mar, t_mar, m_mar) = (
        d.and_then(|s| s.margin).filter(|b| !b.is_empty()),
        t.and_then(|s| s.margin).filter(|b| !b.is_empty()),
        m.and_then(|s| s.margin).filter(|b| !b.is_empty()),
    );
    if let Some(mar_classes) = compile_box_spacing_tailwind("m", d_mar, t_mar, m_mar, true) {
        tailwind_classes.extend(mar_classes);
    } else {
        push_box_spacing_css(&mut fallback_desktop_css, "margin", d_mar);
        push_box_spacing_css(&mut fallback_tablet_css, "margin", t_mar);
        push_box_spacing_css(&mut fallback_mobile_css, "margin", m_mar);
    }

    if matches!(kind, ContainerKind::Row) && resp.opts_into_grid() {
        tailwind_classes.push("grid".to_owned());
        let d_col = d.and_then(|s| s.columns_per_row);
        let t_col = t.and_then(|s| s.columns_per_row).or(d_col);
        let m_col = m.and_then(|s| s.columns_per_row).or(t_col);
        let m_v = m_col.unwrap_or(1);
        let t_v = t_col.unwrap_or(1);
        let d_v = d_col.unwrap_or(1);
        tailwind_classes.push(format!("grid-cols-{m_v}"));
        if t_v != m_v {
            tailwind_classes.push(format!("md:grid-cols-{t_v}"));
        }
        if d_v != t_v {
            tailwind_classes.push(format!("lg:grid-cols-{d_v}"));
        }

        let d_gap = d.and_then(|s| s.gap_px);
        let t_gap = t.and_then(|s| s.gap_px).or(d_gap);
        let m_gap = m.and_then(|s| s.gap_px).or(t_gap);
        if d_gap.is_some() || t_gap.is_some() || m_gap.is_some() {
            let m_g = m_gap.unwrap_or(0);
            let t_g = t_gap.unwrap_or(0);
            let d_g = d_gap.unwrap_or(0);
            if let (Some(gm), Some(gt), Some(gd)) = (
                gap_px_to_tw_step(m_g),
                gap_px_to_tw_step(t_g),
                gap_px_to_tw_step(d_g),
            ) {
                tailwind_classes.push(format!("gap-{gm}"));
                if gt != gm {
                    tailwind_classes.push(format!("md:gap-{gt}"));
                }
                if gd != gt {
                    tailwind_classes.push(format!("lg:gap-{gd}"));
                }
            } else {
                fallback_desktop_css.push_str(&format!("gap:{}px;", d_gap.unwrap_or(0)));
                if let Some(tg) = t.and_then(|s| s.gap_px) {
                    fallback_tablet_css.push_str(&format!("gap:{tg}px;"));
                }
                if let Some(mg) = m.and_then(|s| s.gap_px) {
                    fallback_mobile_css.push_str(&format!("gap:{mg}px;"));
                }
            }
        }
    }

    if matches!(kind, ContainerKind::Column) {
        let d_al = d.and_then(|s| s.text_align);
        let t_al = t.and_then(|s| s.text_align).or(d_al);
        let m_al = m.and_then(|s| s.text_align).or(t_al);
        if d_al.is_some() || t_al.is_some() || m_al.is_some() {
            let m_cls = align_tailwind_class(m_al.unwrap_or(TextAlign::Start));
            let t_cls = align_tailwind_class(t_al.unwrap_or(TextAlign::Start));
            let d_cls = align_tailwind_class(d_al.unwrap_or(TextAlign::Start));
            tailwind_classes.push(m_cls.to_owned());
            if t_cls != m_cls {
                tailwind_classes.push(format!("md:{t_cls}"));
            }
            if d_cls != t_cls {
                tailwind_classes.push(format!("lg:{d_cls}"));
            }
        }
    }

    ContainerRenderPlan {
        tailwind_classes,
        fallback_desktop_css,
        fallback_tablet_css,
        fallback_mobile_css,
    }
}

fn push_scoped_rule(target: &mut String, id: &str, properties: &str) {
    // These IDs are validated on save; also refuse invalid historical IDs
    // so no untrusted value can enter the CSS selector or style element.
    if !properties.is_empty() && valid_package_id(id) {
        target.push_str(&format!(
            "main[data-builder-version=\"1\"] [data-node-id=\"{id}\"]{{{properties}}}"
        ));
    }
}

fn push_visibility_rule(target: &mut String, id: &str) {
    if valid_package_id(id) {
        target.push_str(&format!(
            "main[data-builder-version=\"1\"] [data-node-id=\"{id}\"],.od-pattern-content[data-builder-version=\"1\"] [data-node-id=\"{id}\"]{{display:none !important;}}"
        ));
    }
}

fn meta_id_attr(meta: Option<&NodeMeta>) -> String {
    match meta
        .and_then(|m| m.html_id.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(id) => format!(" id=\"{}\"", escape_html(id)),
        None => String::new(),
    }
}

fn merge_class_attr(
    base_class: Option<&str>,
    tw_classes: &[String],
    meta: Option<&NodeMeta>,
) -> String {
    let mut tokens: Vec<String> = Vec::new();
    if let Some(base) = base_class.filter(|s| !s.is_empty()) {
        tokens.push(base.to_owned());
    }
    tokens.extend(tw_classes.iter().cloned());
    if let Some(extra) = meta
        .and_then(|m| m.class_name.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        tokens.push(escape_html(extra));
    }
    if tokens.is_empty() {
        String::new()
    } else {
        format!(" class=\"{}\"", tokens.join(" "))
    }
}

/// Emits `<style data-builder-responsive>` only for properties that cannot be represented
/// as static Tailwind utility classes: Section backgrounds, node Visibility, and custom pixel fallbacks.
fn builder_responsive_css(document: &BuilderDocument, is_preview: bool) -> String {
    let mut desktop = String::new();
    let mut tablet = String::new();
    let mut mobile = String::new();
    let mut vis_desktop = String::new();
    let mut vis_tablet = String::new();
    let mut vis_mobile = String::new();
    let mut append_visibility = |id: &str, meta: Option<&NodeMeta>| {
        let Some(vis) = meta.and_then(|m| m.visibility.as_ref()) else {
            return;
        };
        if vis.hide_desktop {
            push_visibility_rule(&mut vis_desktop, id);
        }
        if vis.hide_tablet {
            push_visibility_rule(&mut vis_tablet, id);
        }
        if vis.hide_mobile {
            push_visibility_rule(&mut vis_mobile, id);
        }
    };
    for section in &document.sections {
        if !section.is_synthetic() {
            let plan =
                plan_container_styles(section.responsive.as_ref(), ContainerKind::Section, None);
            push_scoped_rule(&mut desktop, &section.id, &plan.fallback_desktop_css);
            push_scoped_rule(&mut tablet, &section.id, &plan.fallback_tablet_css);
            push_scoped_rule(&mut mobile, &section.id, &plan.fallback_mobile_css);
            append_visibility(&section.id, section.meta.as_ref());
            if let Some(style) = section.style.as_ref()
                && valid_package_id(&section.id)
            {
                let mut sec_css = String::new();
                if let Some(bg_color) = style
                    .background_color
                    .as_deref()
                    .filter(|s| !s.is_empty() && opendash_core::block_schema::valid_block_color(s))
                {
                    sec_css.push_str(&format!("background-color:{bg_color};"));
                }
                if let Some(bg_media) = style
                    .background_media_id
                    .as_deref()
                    .filter(|id| !id.is_empty() && valid_package_id(id))
                {
                    let media_prefix = if is_preview {
                        "/api/admin/media"
                    } else {
                        "/media"
                    };
                    sec_css.push_str(&format!(
                        "background-image:url(\"{media_prefix}/{bg_media}/file\");background-size:cover;background-position:center;"
                    ));
                }
                push_scoped_rule(&mut desktop, &section.id, &sec_css);
            }
        }
        let row_section_width = (!section.is_synthetic())
            .then(|| section.style.as_ref().and_then(|s| s.content_width))
            .flatten();
        for row in &section.children {
            if !row.is_synthetic() {
                let plan = plan_container_styles(
                    row.responsive.as_ref(),
                    ContainerKind::Row,
                    row_section_width,
                );
                push_scoped_rule(&mut desktop, &row.id, &plan.fallback_desktop_css);
                push_scoped_rule(&mut tablet, &row.id, &plan.fallback_tablet_css);
                push_scoped_rule(&mut mobile, &row.id, &plan.fallback_mobile_css);
                append_visibility(&row.id, row.meta.as_ref());
            }
            for column in &row.children {
                if !column.is_synthetic() {
                    let plan = plan_container_styles(
                        column.responsive.as_ref(),
                        ContainerKind::Column,
                        None,
                    );
                    push_scoped_rule(&mut desktop, &column.id, &plan.fallback_desktop_css);
                    push_scoped_rule(&mut tablet, &column.id, &plan.fallback_tablet_css);
                    push_scoped_rule(&mut mobile, &column.id, &plan.fallback_mobile_css);
                    append_visibility(&column.id, column.meta.as_ref());
                }
                for element in &column.children {
                    append_visibility(&element.id, element.meta.as_ref());
                }
            }
        }
    }
    if desktop.is_empty()
        && tablet.is_empty()
        && mobile.is_empty()
        && vis_desktop.is_empty()
        && vis_tablet.is_empty()
        && vis_mobile.is_empty()
    {
        return String::new();
    }
    let mut css = format!("<style data-builder-responsive>{desktop}");
    if !tablet.is_empty() {
        css.push_str(&format!("@media (width < 1024px){{{tablet}}}"));
    }
    if !mobile.is_empty() {
        css.push_str(&format!("@media (width < 768px){{{mobile}}}"));
    }
    if !vis_desktop.is_empty() {
        css.push_str(&format!("@media (width >= 1024px){{{vis_desktop}}}"));
    }
    if !vis_tablet.is_empty() {
        css.push_str(&format!(
            "@media (width >= 768px) and (width < 1024px){{{vis_tablet}}}"
        ));
    }
    if !vis_mobile.is_empty() {
        css.push_str(&format!("@media (width < 768px){{{vis_mobile}}}"));
    }
    css.push_str("</style>");
    css
}

fn format_css_number(n: f64, unit: Option<opendash_core::block_schema::CssUnit>) -> Option<String> {
    if !n.is_finite() {
        return None;
    }
    let suffix = unit.map(|u| u.as_css_suffix()).unwrap_or("");
    if n.fract() == 0.0 && n.abs() < 1e15 {
        Some(format!("{}{suffix}", n as i64))
    } else {
        Some(format!("{n}{suffix}"))
    }
}

/// Emits scoped CSS custom properties (`--od-*`) for all active, declared `block.json` v1
/// `PluginBlock` instances in `document` under `[data-od-node-scope="{scope}-{node_id}"]`.
pub fn serialize_scoped_block_css(
    state: &HomeState,
    document: &BuilderDocument,
    scope: &str,
    is_preview: bool,
) -> ApiResult<String> {
    use opendash_core::{
        block_schema::{
            BlockFieldSpec, MediaFieldKind, valid_block_color, valid_css_select_value,
            valid_css_var_name, valid_sha256_media_id,
        },
        registry::{DeclarationState, ProviderStatus},
    };

    if scope.is_empty()
        || !scope
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(ApiError::bad_request("Scope CSS block không hợp lệ."));
    }

    let mut desktop_rules = String::new();
    let mut tablet_rules = String::new();
    let mut mobile_rules = String::new();

    for section in &document.sections {
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    if !valid_package_id(&element.id) {
                        continue;
                    }
                    let ElementContent::Known(KnownElement::PluginBlock(props)) = &element.content
                    else {
                        continue;
                    };
                    let Some(def) = state.declared_blocks.get(&props.block_id) else {
                        continue;
                    };
                    if def.status != ProviderStatus::Active
                        || def.declaration_state != DeclarationState::Declared
                        || def.schema.legacy_mode
                    {
                        continue;
                    }

                    let node_scope = format!("{scope}-{}", element.id);
                    let mut desktop_props = String::new();
                    let mut tablet_props = String::new();
                    let mut mobile_props = String::new();

                    for field in def.schema.schema.flattened_top_fields() {
                        match field {
                            BlockFieldSpec::Number {
                                key,
                                default,
                                unit,
                                css_var: Some(var_name),
                                responsive,
                                ..
                            }
                            | BlockFieldSpec::Range {
                                key,
                                default,
                                unit,
                                css_var: Some(var_name),
                                responsive,
                                ..
                            } => {
                                if !valid_css_var_name(var_name) {
                                    continue;
                                }
                                let raw = props.values.get(key.as_str());
                                if *responsive && let Some(obj) = raw.and_then(|v| v.as_object()) {
                                    if let Some(d) = obj
                                        .get("desktop")
                                        .and_then(|v| v.as_f64())
                                        .or(*default)
                                        .and_then(|n| format_css_number(n, *unit))
                                    {
                                        desktop_props.push_str(&format!("{var_name}:{d};"));
                                    }
                                    if let Some(t) = obj
                                        .get("tablet")
                                        .and_then(|v| v.as_f64())
                                        .and_then(|n| format_css_number(n, *unit))
                                    {
                                        tablet_props.push_str(&format!("{var_name}:{t};"));
                                    }
                                    if let Some(m) = obj
                                        .get("mobile")
                                        .and_then(|v| v.as_f64())
                                        .and_then(|n| format_css_number(n, *unit))
                                    {
                                        mobile_props.push_str(&format!("{var_name}:{m};"));
                                    }
                                } else if let Some(d) = raw
                                    .and_then(|v| v.as_f64())
                                    .or(*default)
                                    .and_then(|n| format_css_number(n, *unit))
                                {
                                    desktop_props.push_str(&format!("{var_name}:{d};"));
                                }
                            }
                            BlockFieldSpec::Select {
                                key,
                                default,
                                css_var: Some(var_name),
                                responsive,
                                ..
                            }
                            | BlockFieldSpec::RadioButtons {
                                key,
                                default,
                                css_var: Some(var_name),
                                responsive,
                                ..
                            } => {
                                if !valid_css_var_name(var_name) {
                                    continue;
                                }
                                let raw = props.values.get(key.as_str());
                                if *responsive && let Some(obj) = raw.and_then(|v| v.as_object()) {
                                    if let Some(d) = obj
                                        .get("desktop")
                                        .and_then(|v| v.as_str())
                                        .or(default.as_deref())
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
                                    .or(default.as_deref())
                                    .filter(|s| valid_css_select_value(s))
                                {
                                    desktop_props.push_str(&format!("{var_name}:{d};"));
                                }
                            }
                            BlockFieldSpec::Spacing {
                                key,
                                default,
                                unit,
                                css_var,
                                responsive,
                                ..
                            } => {
                                let css_unit =
                                    unit.unwrap_or(opendash_core::block_schema::CssUnit::Px);
                                let raw = props.values.get(key.as_str());
                                if let Some(var_name) = css_var {
                                    if !valid_css_var_name(var_name) {
                                        continue;
                                    }
                                    let parse_spacing = |v: &serde_json::Value| {
                                        serde_json::from_value::<
                                            opendash_core::block_schema::SpacingValue,
                                        >(v.clone())
                                        .ok()
                                        .and_then(|sv| sv.format_css(css_unit))
                                    };
                                    if *responsive
                                        && let Some(obj) = raw.and_then(|v| v.as_object())
                                        && !(obj.contains_key("top")
                                            || obj.contains_key("right")
                                            || obj.contains_key("bottom")
                                            || obj.contains_key("left"))
                                    {
                                        if let Some(d) =
                                            obj.get("desktop").and_then(&parse_spacing).or_else(
                                                || default.and_then(|sv| sv.format_css(css_unit)),
                                            )
                                        {
                                            desktop_props.push_str(&format!("{var_name}:{d};"));
                                        }
                                        if let Some(t) = obj.get("tablet").and_then(&parse_spacing)
                                        {
                                            tablet_props.push_str(&format!("{var_name}:{t};"));
                                        }
                                        if let Some(m) = obj.get("mobile").and_then(&parse_spacing)
                                        {
                                            mobile_props.push_str(&format!("{var_name}:{m};"));
                                        }
                                    } else if let Some(d) = raw
                                        .and_then(&parse_spacing)
                                        .or_else(|| default.and_then(|sv| sv.format_css(css_unit)))
                                    {
                                        desktop_props.push_str(&format!("{var_name}:{d};"));
                                    }
                                } else if crate::template_sec::spacing_field_tailwind_classes(
                                    raw,
                                    *default,
                                    *responsive,
                                    css_unit,
                                )
                                .is_none()
                                {
                                    let (d_sv, t_sv, m_sv) =
                                        crate::template_sec::extract_spacing_breakpoint_values(
                                            raw,
                                            *default,
                                            *responsive,
                                        );
                                    if let Some(d_css) = d_sv.and_then(|sv| sv.format_css(css_unit))
                                    {
                                        desktop_rules.push_str(&format!(
                                            "[data-od-node-scope=\"{node_scope}\"] [data-od-spacing=\"{key}\"]{{padding:{d_css};}}"
                                        ));
                                    }
                                    if let Some(t_css) = t_sv.and_then(|sv| sv.format_css(css_unit))
                                    {
                                        tablet_rules.push_str(&format!(
                                            "[data-od-node-scope=\"{node_scope}\"] [data-od-spacing=\"{key}\"]{{padding:{t_css};}}"
                                        ));
                                    }
                                    if let Some(m_css) = m_sv.and_then(|sv| sv.format_css(css_unit))
                                    {
                                        mobile_rules.push_str(&format!(
                                            "[data-od-node-scope=\"{node_scope}\"] [data-od-spacing=\"{key}\"]{{padding:{m_css};}}"
                                        ));
                                    }
                                }
                            }
                            BlockFieldSpec::Color {
                                key,
                                default,
                                css_var: Some(var_name),
                                ..
                            } => {
                                if !valid_css_var_name(var_name) {
                                    continue;
                                }
                                if let Some(c) = props
                                    .values
                                    .get(key.as_str())
                                    .and_then(|v| v.as_str())
                                    .or(default.as_deref())
                                    .filter(|s| !s.is_empty() && valid_block_color(s))
                                {
                                    desktop_props.push_str(&format!("{var_name}:{c};"));
                                }
                            }
                            BlockFieldSpec::Media {
                                key,
                                media_kind: MediaFieldKind::Image,
                                default,
                                css_var: Some(var_name),
                                ..
                            } => {
                                if !valid_css_var_name(var_name) {
                                    continue;
                                }
                                if let Some(id) = props
                                    .values
                                    .get(key.as_str())
                                    .and_then(|v| v.as_str())
                                    .or(default.as_deref())
                                    .filter(|s| !s.is_empty() && valid_sha256_media_id(s))
                                {
                                    let url_path = if is_preview {
                                        format!("/api/admin/media/{id}/file")
                                    } else {
                                        format!("/media/{id}/file")
                                    };
                                    desktop_props
                                        .push_str(&format!("{var_name}:url(\"{url_path}\");"));
                                }
                            }
                            _ => {}
                        }
                    }

                    if !desktop_props.is_empty() {
                        desktop_rules.push_str(&format!(
                            "[data-od-node-scope=\"{node_scope}\"]{{{desktop_props}}}"
                        ));
                    }
                    if !tablet_props.is_empty() {
                        tablet_rules.push_str(&format!(
                            "[data-od-node-scope=\"{node_scope}\"]{{{tablet_props}}}"
                        ));
                    }
                    if !mobile_props.is_empty() {
                        mobile_rules.push_str(&format!(
                            "[data-od-node-scope=\"{node_scope}\"]{{{mobile_props}}}"
                        ));
                    }
                }
            }
        }
    }

    if desktop_rules.is_empty() && tablet_rules.is_empty() && mobile_rules.is_empty() {
        return Ok(String::new());
    }

    let mut css_body = desktop_rules;
    if !tablet_rules.is_empty() {
        css_body.push_str(&format!("@media (width < 1024px){{{tablet_rules}}}"));
    }
    if !mobile_rules.is_empty() {
        css_body.push_str(&format!("@media (width < 768px){{{mobile_rules}}}"));
    }
    crate::template_sec::validate_safe_scoped_block_css(&css_body, is_preview)
        .map_err(ApiError::bad_request)?;
    Ok(format!("<style data-od-block-styles>{css_body}</style>"))
}

/// Template selection comes from one storage snapshot. Unsupported legacy
/// nodes are invisible in public markup but remain intact in Builder JSON.
/// Only renderer-known core elements can add public media references.
#[cfg(test)]
pub fn render_builder(
    state: &HomeState,
    document: &BuilderDocument,
    is_preview: bool,
) -> ApiResult<RenderedLayout> {
    render_builder_resolved(state, document, is_preview, &BindingResults::new())
}

/// All bound values are obtained by the storage layer under Preview/public ACL.
/// The renderer never reads SQL or treats rich text as raw markup.
pub fn render_builder_resolved(
    state: &HomeState,
    document: &BuilderDocument,
    is_preview: bool,
    bindings: &BindingResults,
) -> ApiResult<RenderedLayout> {
    render_builder_with_lists(state, document, is_preview, bindings, &ListResults::new())
}

/// Query-free renderer: storage has already resolved every list against the
/// same SQLite snapshot as the document/Field Binding sources.
pub fn render_builder_with_lists(
    state: &HomeState,
    document: &BuilderDocument,
    is_preview: bool,
    bindings: &BindingResults,
    lists: &ListResults,
) -> ApiResult<RenderedLayout> {
    render_builder_with_lists_and_forms(
        state,
        document,
        is_preview,
        bindings,
        lists,
        &ContactFormResults::new(),
    )
}

pub fn render_builder_with_lists_and_forms(
    state: &HomeState,
    document: &BuilderDocument,
    is_preview: bool,
    bindings: &BindingResults,
    lists: &ListResults,
    forms: &ContactFormResults,
) -> ApiResult<RenderedLayout> {
    render_builder_with_template_context(state, document, is_preview, bindings, lists, forms, None)
}

/// The S02 public resolver supplies already sanitized entry HTML and verified
/// active theme parts. Ordinary page Builder calls have no template context.
pub struct BuilderTemplateContext<'a> {
    pub content_html: Option<&'a str>,
    pub parts: &'a std::collections::BTreeMap<String, String>,
}

pub fn render_builder_with_template_context(
    state: &HomeState,
    document: &BuilderDocument,
    is_preview: bool,
    bindings: &BindingResults,
    lists: &ListResults,
    forms: &ContactFormResults,
    context: Option<&BuilderTemplateContext<'_>>,
) -> ApiResult<RenderedLayout> {
    render_builder_with_patterns(
        state,
        document,
        is_preview,
        bindings,
        lists,
        forms,
        context,
        &PatternResults::new(),
        false,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderAnnotationScope {
    #[default]
    None,
    TemplateAll,
    EntryBodyOnly,
}

fn editable_attrs(
    scope: RenderAnnotationScope,
    is_preview: bool,
    pattern_document: bool,
    kind: &str,
) -> String {
    if !is_preview || pattern_document || scope == RenderAnnotationScope::None {
        return String::new();
    }
    format!(" data-od-editable=\"1\" data-od-kind=\"{kind}\"")
}

/// Render an entry or a trusted, one-hop Pattern body. Only the storage layer
/// can populate `patterns`, so incoming AST never carries referenced markup.
pub fn render_builder_with_patterns(
    state: &HomeState,
    document: &BuilderDocument,
    is_preview: bool,
    bindings: &BindingResults,
    lists: &ListResults,
    forms: &ContactFormResults,
    context: Option<&BuilderTemplateContext<'_>>,
    patterns: &PatternResults,
    pattern_document: bool,
) -> ApiResult<RenderedLayout> {
    render_builder_with_annotations(
        state,
        document,
        is_preview,
        bindings,
        lists,
        forms,
        context,
        patterns,
        pattern_document,
        RenderAnnotationScope::None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn render_builder_with_annotations(
    state: &HomeState,
    document: &BuilderDocument,
    is_preview: bool,
    bindings: &BindingResults,
    lists: &ListResults,
    forms: &ContactFormResults,
    context: Option<&BuilderTemplateContext<'_>>,
    patterns: &PatternResults,
    pattern_document: bool,
    annotation_scope: RenderAnnotationScope,
) -> ApiResult<RenderedLayout> {
    let scope = if pattern_document { "pat_0" } else { "entry" };
    let mut budget = RenderBudget::new();
    render_builder_with_budget(
        state,
        document,
        is_preview,
        bindings,
        lists,
        forms,
        context,
        patterns,
        pattern_document,
        annotation_scope,
        scope,
        &mut budget,
        BlockTemplateCache::global(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn render_builder_with_budget(
    state: &HomeState,
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
    budget: &mut RenderBudget,
    cache: &BlockTemplateCache,
) -> ApiResult<RenderedLayout> {
    render_builder_with_budget_and_base_path(
        state,
        document,
        is_preview,
        bindings,
        lists,
        forms,
        context,
        patterns,
        pattern_document,
        annotation_scope,
        scope,
        "/",
        budget,
        cache,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn render_builder_with_budget_and_base_path(
    state: &HomeState,
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
    let mut html = builder_responsive_css(document, is_preview);
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
                            let path = if is_preview {
                                "/api/admin/media"
                            } else {
                                "/media"
                            };
                            Some(format!(
                                "<img src=\"{path}/{id}/file\" alt=\"{alt}\" loading=\"lazy\">",
                                id = escape_html(&props.media_id),
                                alt = escape_html(&props.alt)
                            ))
                        }
                        ElementContent::Known(KnownElement::Video(props)) => {
                            let path = if is_preview {
                                "/api/admin/media"
                            } else {
                                "/media"
                            };
                            let mut attrs = format!(
                                "src=\"{path}/{id}/file\" preload=\"metadata\" playsinline",
                                id = escape_html(&props.media_id),
                            );
                            if let Some(poster) = props.poster_image_id.as_deref() {
                                attrs.push_str(&format!(
                                    " poster=\"{path}/{poster}/file\"",
                                    poster = escape_html(poster)
                                ));
                            }
                            if props.controls {
                                attrs.push_str(" controls");
                            }
                            if props.muted {
                                attrs.push_str(" muted");
                            }
                            Some(format!("<video {attrs}></video>"))
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
                                        l10n,
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
                                    let render_res = cache
                                        .get_or_validate(
                                            &props.block_id,
                                            &def.schema.schema,
                                            &def.template,
                                        )
                                        .and_then(|compiled| {
                                            crate::template_sec::render_block_csec_with_options_and_budget(
                                                &compiled.block_id,
                                                &compiled.source,
                                                &def.schema.schema,
                                                &props.values,
                                                Some(&state.site_options),
                                                is_preview,
                                                budget,
                                            )
                                        });
                                    match render_res {
                                        Ok(rendered_inner) => {
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
                            } else if let Some(template_src) = state
                                .themes
                                .get(&props.block_id)
                                .map(|d| d.template.as_str())
                            {
                                match render_plugin_block_csec_with_budget(
                                    &props.block_id,
                                    template_src,
                                    &props.values,
                                    budget,
                                ) {
                                    Ok(rendered_inner) => {
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
                        ElementContent::Known(KnownElement::Legacy(props)) => {
                            match state.themes.get(&props.block_id) {
                                None => {
                                    diagnostics.push(BuilderDiagnostic {
                                        node_id: element.id.clone(),
                                        code: "descriptor_missing",
                                    });
                                    None
                                }
                                Some(definition) => {
                                    let selected = theme_storage::select_template(
                                        &props.block_id,
                                        definition.template.clone(),
                                        state.active_id.as_deref(),
                                        state.active_alias.as_deref(),
                                        state.active_template.clone(),
                                    );
                                    if let Some(template) = selected.template {
                                        let block = Block {
                                            kind: props.block_id.clone(),
                                            values: props.values.clone(),
                                        };
                                        if block
                                            .validate_against_fields(&definition.fields)
                                            .is_err()
                                        {
                                            diagnostics.push(BuilderDiagnostic {
                                                node_id: element.id.clone(),
                                                code: "legacy_fields_invalid",
                                            });
                                            None
                                        } else {
                                            Some(
                                                theme::render(
                                                    &template,
                                                    &definition.fields,
                                                    &block,
                                                )
                                                .map_err(ApiError::bad_request)?,
                                            )
                                        }
                                    } else {
                                        diagnostics.push(BuilderDiagnostic {
                                            node_id: element.id.clone(),
                                            code: "unsupported_by_theme",
                                        });
                                        None
                                    }
                                }
                            }
                        }
                        ElementContent::Unresolved(_) => {
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

fn render_contact_form_html(
    node_id: &str,
    form: &crate::storage::contact::ContactFormRecord,
    submit_label: &str,
    is_preview: bool,
    l10n: &crate::content_render::CoreL10n,
) -> String {
    use opendash_core::contact::FormKind;
    let mut out = String::new();
    if is_preview {
        out.push_str(
            "<div class=\"builder-contact-form builder-contact-preview\"><fieldset disabled>",
        );
    } else {
        let nonce = crate::auth::generate_random_token();
        out.push_str(&format!(
            "<form class=\"builder-contact-form\" method=\"post\" action=\"/api/public/contact/{}\">",
            form.id
        ));
        out.push_str(&format!(
            "<input type=\"hidden\" name=\"form_version\" value=\"{}\">",
            form.version
        ));
        out.push_str(&format!(
            "<input type=\"hidden\" name=\"nonce\" value=\"{nonce}\">"
        ));
        out.push_str(&format!(
            "<div class=\"opendash-hp\" hidden><label>{}<input type=\"text\" name=\"honeypot\" tabindex=\"-1\" autocomplete=\"off\"></label></div>",
            l10n.select_empty
        ));
    }

    for field in &form.schema.fields {
        let input_id = format!("cf_{}_{}", escape_html(node_id), escape_html(&field.key));
        let field_name = escape_html(&field.key);
        let label = escape_html(&field.label);
        let req_attr = if field.required { " required" } else { "" };
        out.push_str("<div class=\"builder-contact-field\">");
        match field.kind {
            FormKind::Checkbox => {
                out.push_str(&format!(
                    "<label for=\"{input_id}\"><input id=\"{input_id}\" type=\"checkbox\" name=\"{field_name}\" value=\"true\"{req_attr}> {label}</label>"
                ));
            }
            FormKind::Textarea => {
                out.push_str(&format!(
                    "<label for=\"{input_id}\">{label}</label><textarea id=\"{input_id}\" name=\"{field_name}\" maxlength=\"4000\"{req_attr}></textarea>"
                ));
            }
            FormKind::Select => {
                out.push_str(&format!(
                    "<label for=\"{input_id}\">{label}</label><select id=\"{input_id}\" name=\"{field_name}\"{req_attr}>"
                ));
                if !field.required {
                    out.push_str(&format!(
                        "<option value=\"\">{}</option>",
                        l10n.select_placeholder
                    ));
                }
                if let Some(options) = &field.options {
                    for opt in options {
                        let eopt = escape_html(opt);
                        out.push_str(&format!("<option value=\"{eopt}\">{eopt}</option>"));
                    }
                }
                out.push_str("</select>");
            }
            FormKind::Email => {
                out.push_str(&format!(
                    "<label for=\"{input_id}\">{label}</label><input id=\"{input_id}\" type=\"email\" name=\"{field_name}\" maxlength=\"254\"{req_attr}>"
                ));
            }
            FormKind::Text => {
                out.push_str(&format!(
                    "<label for=\"{input_id}\">{label}</label><input id=\"{input_id}\" type=\"text\" name=\"{field_name}\" maxlength=\"254\"{req_attr}>"
                ));
            }
        }
        out.push_str("</div>");
    }

    let btn_label = escape_html(submit_label.trim());
    if is_preview {
        out.push_str(&format!(
            "<button type=\"button\" disabled>{btn_label}</button></fieldset></div>"
        ));
    } else {
        out.push_str(&format!(
            "<button type=\"submit\">{btn_label}</button></form>"
        ));
    }
    out
}

/// Canonical link: only currently resolved node IDs with valid nondefault pages
/// survive. Unknown query parameters, duplicates and invalid values are dropped.
fn list_page_href(base_path: &str, lists: &ListResults, node_id: &str, target: i64) -> String {
    let mut pages = std::collections::BTreeMap::new();
    for (id, current) in lists {
        let ResolvedList::Page {
            page, out_of_range, ..
        } = current
        else {
            continue;
        };
        let effective = if id == node_id {
            target
        } else if *out_of_range {
            1
        } else {
            *page
        };
        if (2..=100).contains(&effective) {
            pages.insert(id.clone(), effective);
        }
    }
    CanonicalUrlBuilder::list_pages(base_path, &pages)
}

pub fn render_legacy_state(state: &HomeState) -> ApiResult<Option<RenderedBlock>> {
    let Some(block) = &state.legacy else {
        return Ok(None);
    };
    let definition = state
        .themes
        .get(&block.kind)
        .ok_or_else(|| ApiError::internal("Missing legacy theme descriptor"))?;
    let selected = theme_storage::select_template(
        &block.kind,
        definition.template.clone(),
        state.active_id.as_deref(),
        state.active_alias.as_deref(),
        state.active_template.clone(),
    );
    render_selected(block, &definition.fields, selected).map(Some)
}

pub fn layout_hook_payload(document: &BuilderDocument, version: i64) -> serde_json::Value {
    let elements: Vec<_> = document
        .sections
        .iter()
        .flat_map(|s| &s.children)
        .flat_map(|r| &r.children)
        .flat_map(|c| &c.children)
        .collect();
    if let [only] = elements.as_slice() {
        if let ElementContent::Known(KnownElement::Legacy(props)) = &only.content {
            return json!({"block_type": props.block_id});
        }
    }
    json!({"block_type": "core/layout", "layout_version": version})
}

pub async fn dispatch_saved_hook(hooks: &HookDispatcher, payload: serde_json::Value) {
    let report = hooks.dispatch(HookEvent::PageAfterSave, payload).await;
    for outcome in &report.outcomes {
        if let Some(error) = &outcome.error {
            eprintln!(
                "Saved page; hook {} failed ({}): {error}",
                outcome.handler_id, report.envelope.correlation_id
            );
        }
    }
}

fn render_selected(
    block: &Block,
    fields: &theme::Fields,
    selected: SelectedTemplate,
) -> ApiResult<RenderedBlock> {
    // Always validate against the author's original block schema, including when
    // the active theme does not support it. Incompatible does not mean invalid.
    block
        .validate_against_fields(fields)
        .map_err(ApiError::bad_request)?;
    let html = match selected.template {
        Some(template) => theme::render(&template, fields, block).map_err(ApiError::bad_request)?,
        None => "<main></main>".to_string(), // Safe empty content; never borrow a stale template.
    };
    Ok(RenderedBlock {
        html,
        visibility: selected.visibility,
    })
}

pub fn render_saved(snapshot: HomeSnapshot) -> ApiResult<RenderedBlock> {
    render_selected(
        &snapshot.block,
        &snapshot.original_fields,
        snapshot.selected,
    )
}

pub async fn render_block(db: &SqlitePool, block: &Block) -> ApiResult<RenderedBlock> {
    let (original_template, fields) = load_theme_block(db, &block.kind).await?;
    let (active_id, active_alias, active_template) = theme_storage::active_selection(db).await?;
    let selected = theme_storage::select_template(
        &block.kind,
        original_template,
        active_id.as_deref(),
        active_alias.as_deref(),
        active_template,
    );
    render_selected(block, &fields, selected)
}

pub async fn save_page(
    db: &SqlitePool,
    hooks: &HookDispatcher,
    block: &Block,
) -> ApiResult<RenderedBlock> {
    // A converted homepage must reject *every* legacy write with 409,
    // including requests whose legacy descriptor no longer resolves.
    // The transaction in save_home_page_and_render repeats the guard under
    // the writer lock to close the race with a concurrent Builder Save.
    crate::storage::builder::legacy_page(db).await?;
    // The source schema is immutable once a package is installed. Validate
    // before acquiring a SQLite write lock; render + homepage write observe
    // one active-theme version inside the same write transaction.
    let (original_template, fields) = load_theme_block(db, &block.kind).await?;
    block
        .validate_against_fields(&fields)
        .map_err(ApiError::bad_request)?;
    let rendered =
        theme_storage::save_home_page_and_render(db, block, original_template, |selected| {
            render_selected(block, &fields, selected)
        })
        .await?;
    // The SQLite autocommit has completed. Notifications cannot invalidate this save.
    dispatch_saved_hook(hooks, json!({ "block_type": block.kind })).await;
    Ok(rendered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        plugin::PluginManager,
        storage::{
            builder::{read as builder_read, save_and_render as builder_save_and_render},
            page::{insert_theme_block, load_home_page},
        },
        theme::{BlockDescriptor, Fields, Theme},
    };
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[tokio::test]
    async fn save_commits_before_hooks_and_keeps_data_after_handler_failure() {
        let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        let fields = Fields::from([("title".into(), "Title".into())]);
        insert_theme_block(
            &db,
            &Theme {
                descriptor: BlockDescriptor {
                    id: "sample/banner".into(),
                    fields,
                },
                template: "<h1>{{ values.title }}</h1>".into(),
                content_list_template: None,
                content_single_template: None,
                compatible_block_id: None,
            },
        )
        .await
        .unwrap();
        let block = Block {
            kind: "sample/banner".into(),
            values: Fields::from([("title".into(), "Saved title".into())]),
        };
        let observed = Arc::new(AtomicBool::new(false));
        let mut hooks = HookDispatcher::new(Arc::new(PluginManager::new(db.clone(), false)));
        let observed_by_handler = observed.clone();
        let handler_db = db.clone();
        hooks
            .register_handler(
                "core/check_commit",
                HookEvent::PageAfterSave,
                1,
                move |event| {
                    let db = handler_db.clone();
                    let observed = observed_by_handler.clone();
                    Box::pin(async move {
                        assert_eq!(
                            load_home_page(&db).await.unwrap().unwrap().values["title"],
                            "Saved title"
                        );
                        assert_eq!(event.payload["block_type"], "sample/banner");
                        observed.store(true, Ordering::SeqCst);
                        Err("expected handler failure".into())
                    })
                },
            )
            .unwrap();
        hooks
            .register_handler("core/panic", HookEvent::PageAfterSave, 2, |_| {
                Box::pin(async { panic!("expected handler panic") })
            })
            .unwrap();
        // Preview does not write or dispatch.
        assert!(
            render_block(&db, &block)
                .await
                .unwrap()
                .html
                .contains("Saved title")
        );
        assert!(load_home_page(&db).await.unwrap().is_none());
        assert!(!observed.load(Ordering::SeqCst));
        assert!(
            save_page(&db, &hooks, &block)
                .await
                .unwrap()
                .html
                .contains("Saved title")
        );
        assert!(observed.load(Ordering::SeqCst));
        assert_eq!(load_home_page(&db).await.unwrap().unwrap(), block);
        let invalid = Block {
            kind: "missing/block".into(),
            values: block.values.clone(),
        };
        assert!(save_page(&db, &hooks, &invalid).await.is_err());
        assert_eq!(load_home_page(&db).await.unwrap().unwrap(), block);
    }

    #[tokio::test]
    async fn builder_save_commits_before_hook_failure_and_never_exposes_draft() {
        let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        let draft = BuilderDocument::from_json(
            br#"{"schema_version":1,"sections":[{"id":"section-root","children":[{"id":"row-root","children":[{"id":"column-root","children":[{"id":"heading-root","type":"core/text","props":{"text":"Committed"}}]}]}]}]}"#,
        ).unwrap();
        let observed = Arc::new(AtomicBool::new(false));
        let mut hooks = HookDispatcher::new(Arc::new(PluginManager::new(db.clone(), false)));
        let observed_copy = observed.clone();
        let observer_db = db.clone();
        hooks
            .register_handler(
                "core/builder_commit_check",
                HookEvent::PageAfterSave,
                1,
                move |event| {
                    let observed = observed_copy.clone();
                    let db = observer_db.clone();
                    Box::pin(async move {
                        let committed = builder_read(&db).await.unwrap();
                        assert_eq!(committed.version, 2);
                        assert!(committed.saved.is_some());
                        assert_eq!(
                            event.payload,
                            json!({"block_type": "core/layout", "layout_version": 2})
                        );
                        observed.store(true, Ordering::SeqCst);
                        Err("expected hook failure after commit".to_string())
                    })
                },
            )
            .unwrap();

        let (version, rendered) =
            builder_save_and_render(&db, &draft, 1, |current, document, bindings, _lists| {
                render_builder_resolved(current, document, false, bindings)
            })
            .await
            .unwrap();
        assert_eq!(version, 2);
        assert!(rendered.html.contains("Committed"));
        assert!(!observed.load(Ordering::SeqCst));
        dispatch_saved_hook(&hooks, layout_hook_payload(&draft, version)).await;
        assert!(observed.load(Ordering::SeqCst));
        assert_eq!(builder_read(&db).await.unwrap().saved, Some(draft.clone()));
        let failure =
            builder_save_and_render(&db, &draft, 1, |current, document, bindings, _lists| {
                render_builder_resolved(current, document, false, bindings)
            })
            .await
            .unwrap_err();
        assert_eq!(failure.0, axum::http::StatusCode::PRECONDITION_FAILED);
        assert_eq!(builder_read(&db).await.unwrap().version, 2);
    }

    #[test]
    fn single_legacy_node_uses_original_m3_hook_block_type() {
        let doc = BuilderDocument::from_json(
            br#"{"schema_version":1,"sections":[{"id":"section-root","children":[{"id":"row-root","children":[{"id":"column-root","children":[{"id":"block-root","type":"legacy/block","props":{"block_id":"sample/banner","values":{"title":"Secret draft"}}}]}]}]}]}"#,
        ).unwrap();
        assert_eq!(
            layout_hook_payload(&doc, 42),
            json!({"block_type": "sample/banner"})
        );
        let empty = BuilderDocument::empty();
        assert_eq!(
            layout_hook_payload(&empty, 3),
            json!({"block_type": "core/layout", "layout_version": 3})
        );
    }
}

pub fn render_plugin_block_csec(
    block_id: &str,
    template_source: &str,
    values: &std::collections::BTreeMap<String, serde_json::Value>,
) -> ApiResult<String> {
    let mut budget = RenderBudget::new();
    render_plugin_block_csec_with_budget(block_id, template_source, values, &mut budget)
}

pub fn render_plugin_block_csec_with_budget(
    block_id: &str,
    template_source: &str,
    values: &std::collections::BTreeMap<String, serde_json::Value>,
    budget: &mut RenderBudget,
) -> ApiResult<String> {
    if budget.fuel_remaining == 0 {
        return Err(ApiError::bad_request("Đã hết ngân sách fuel render trang."));
    }
    let node_fuel = budget
        .fuel_remaining
        .min(crate::template_sec::MAX_BLOCK_FUEL_PER_NODE);
    crate::template_sec::validate_template_csec(block_id, template_source)
        .map_err(ApiError::bad_request)?;
    let mut ctx_map = std::collections::BTreeMap::new();
    for (k, v) in values {
        ctx_map.insert(k.clone(), crate::template_sec::json_to_safe_value(v));
    }
    let ctx = minijinja::Value::from(ctx_map);
    let mut env = crate::template_sec::restricted_environment();
    env.set_fuel(Some(node_fuel));
    env.add_template(block_id, template_source)
        .map_err(|_| ApiError::bad_request("Không thể biên dịch template."))?;
    let captured = env
        .get_template(block_id)
        .map_err(|_| ApiError::bad_request("Không tìm thấy template."))?
        .render_captured(ctx)
        .map_err(|_| ApiError::bad_request("Lỗi thực thi template (fuel/recursion/undefined)."))?;
    let (consumed, _) = captured
        .state()
        .fuel_levels()
        .ok_or_else(|| ApiError::bad_request("Thiếu bộ theo dõi fuel."))?;
    budget.fuel_remaining = budget.fuel_remaining.saturating_sub(consumed);
    let rendered = captured.output().to_owned();
    if rendered.len() > crate::template_sec::MAX_RENDERED_HTML_BYTES {
        return Err(ApiError::bad_request(
            "HTML render vượt quá giới hạn 512 KiB.",
        ));
    }
    crate::template_sec::validate_template_csec(block_id, &rendered)
        .map_err(ApiError::bad_request)?;
    budget
        .charge_html_bytes(rendered.len())
        .map_err(ApiError::bad_request)?;
    Ok(rendered)
}

#[cfg(test)]
mod m6_s05_tests {
    use super::*;
    use opendash_core::builder::{
        BuilderDocument, ColumnNode, ElementContent, ElementNode, KnownElement, PluginBlockProps,
        RowNode, SectionNode,
    };
    use std::collections::BTreeMap;

    #[test]
    fn plugin_block_csec_escapes_values_blocks_xss_and_falls_back_safely() {
        // 1. C-SEC rejects dangerous template constructs
        assert!(
            render_plugin_block_csec(
                "acme-plugin/hero",
                "<script>alert(1)</script>",
                &BTreeMap::new()
            )
            .is_err()
        );

        // 2. C-SEC auto-escapes prop values
        let mut vals = BTreeMap::new();
        vals.insert(
            "title".into(),
            serde_json::Value::String("<script>xss()</script>Hello".into()),
        );
        let ok_html = render_plugin_block_csec(
            "acme-plugin/hero",
            "<div class=\"card\">{{ title }}</div>",
            &vals,
        )
        .expect("safe C-SEC template renders");
        assert!(!ok_html.contains("<script>"));
        assert!(ok_html.contains("&lt;script&gt;xss()"));
        assert!(ok_html.contains("Hello"));

        // 3. Builder render with active plugin block vs disabled plugin block fallback
        let doc = BuilderDocument {
            schema_version: 1,
            sections: vec![SectionNode {
                id: "sec-1".into(),
                meta: None,
                responsive: None,
                style: None,
                children: vec![RowNode {
                    id: "row-1".into(),
                    meta: None,
                    responsive: None,
                    children: vec![ColumnNode {
                        id: "col-1".into(),
                        meta: None,
                        responsive: None,
                        children: vec![ElementNode {
                            id: "el-1".into(),
                            meta: None,
                            content: ElementContent::Known(KnownElement::PluginBlock(
                                PluginBlockProps {
                                    block_id: "acme-plugin/hero".into(),
                                    values: vals,
                                },
                            )),
                        }],
                    }],
                }],
            }],
        };

        let mut active_state = HomeState {
            active_id: None,
            active_alias: None,
            active_template: None,
            themes: BTreeMap::new(),
            saved: Some(doc.clone()),
            legacy: None,
            version: 1,
            ..Default::default()
        };
        crate::storage::builder::register_plugin_block_template(
            &mut active_state,
            "acme-plugin/hero",
            "<section class=\"hero\">{{ title }}</section>",
        )
        .expect("register plugin block template");

        let rendered_active = render_builder_with_lists(
            &active_state,
            &doc,
            false,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .expect("active render");
        assert!(
            rendered_active
                .html
                .contains("data-block-id=\"acme-plugin/hero\"")
        );
        assert!(rendered_active.html.contains("&lt;script&gt;xss()"));
        assert!(rendered_active.html.contains("Hello"));
        assert!(rendered_active.diagnostics.is_empty());

        // Disabled plugin: block absent from state.themes -> safe fallback without error
        let disabled_state = HomeState {
            active_id: None,
            active_alias: None,
            active_template: None,
            themes: BTreeMap::new(),
            saved: Some(doc.clone()),
            legacy: None,
            version: 1,
            ..Default::default()
        };
        let rendered_disabled = render_builder_with_lists(
            &disabled_state,
            &doc,
            false,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .expect("disabled fallback render must not fail");
        assert!(rendered_disabled.html.contains("builder-block-unavailable"));
        assert_eq!(
            rendered_disabled.diagnostics[0].code,
            "plugin_block_unavailable"
        );
    }

    #[test]
    fn builder_list_uses_snapshot_canonical_href_not_legacy_slug() {
        let props: opendash_core::builder::ListProps =
            serde_json::from_value(serde_json::json!({"type_key": "post"})).unwrap();
        let doc = BuilderDocument {
            schema_version: 1,
            sections: vec![SectionNode {
                id: "sec-1".into(),
                meta: None,
                responsive: None,
                style: None,
                children: vec![RowNode {
                    id: "row-1".into(),
                    meta: None,
                    responsive: None,
                    children: vec![ColumnNode {
                        id: "col-1".into(),
                        meta: None,
                        responsive: None,
                        children: vec![ElementNode {
                            id: "list-1".into(),
                            meta: None,
                            content: ElementContent::Known(KnownElement::List(props)),
                        }],
                    }],
                }],
            }],
        };
        let state = HomeState {
            version: 1,
            saved: Some(doc.clone()),
            legacy: None,
            themes: BTreeMap::new(),
            active_id: None,
            active_alias: None,
            active_template: None,
            ..Default::default()
        };
        let mut lists = ListResults::new();
        lists.insert(
            "list-1".into(),
            ResolvedList::Page {
                page: 1,
                items: vec![
                    crate::storage::builder::ListItem {
                        entry_id: 17,
                        title: "<unsafe>".into(),
                        slug: "old-slug".into(),
                        href: Some("/blog/new-slug".into()),
                    },
                    crate::storage::builder::ListItem {
                        entry_id: 18,
                        title: "Unlinked".into(),
                        slug: "hidden".into(),
                        href: None,
                    },
                ],
                has_next: false,
                out_of_range: false,
            },
        );
        let rendered =
            render_builder_with_lists(&state, &doc, false, &BTreeMap::new(), &lists).unwrap();
        assert!(rendered.html.contains("/blog/new-slug"));
        assert!(!rendered.html.contains("/old-slug"));
        assert!(!rendered.html.contains("href=\"/hidden\""));
        assert!(!rendered.html.contains("href=\"/content/post/hidden\""));
        assert!(rendered.html.contains("&lt;unsafe&gt;"));
        assert!(!rendered.html.contains("<unsafe>"));
    }

    #[test]
    fn preview_annotation_scope_isolates_public_pattern_parts_and_slots() {
        use opendash_core::builder::{
            ColumnNode, ContentSlotProps, ElementNode, HeadingProps, PatternRefProps, RowNode,
            SectionNode, TemplatePartProps,
        };
        let state = HomeState {
            version: 1,
            saved: None,
            legacy: None,
            themes: BTreeMap::new(),
            active_id: None,
            active_alias: None,
            active_template: None,
            ..Default::default()
        };
        let pattern_doc = BuilderDocument {
            schema_version: 1,
            sections: vec![SectionNode {
                id: "psec-1".into(),
                meta: None,
                responsive: None,
                style: None,
                children: vec![RowNode {
                    id: "prow-1".into(),
                    meta: None,
                    responsive: None,
                    children: vec![ColumnNode {
                        id: "pcol-1".into(),
                        meta: None,
                        responsive: None,
                        children: vec![ElementNode {
                            id: "phead-1".into(),
                            meta: None,
                            content: ElementContent::Known(KnownElement::Heading(HeadingProps {
                                level: 3,
                                text: "Inner pattern".into(),
                                binding: None,
                            })),
                        }],
                    }],
                }],
            }],
        };
        // Pattern AST is always rendered with pattern_document = true and scope None.
        let inner_pattern = render_builder_with_annotations(
            &state,
            &pattern_doc,
            true,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            None,
            &BTreeMap::new(),
            true,
            RenderAnnotationScope::TemplateAll,
        )
        .unwrap();
        assert!(!inner_pattern.html.contains("data-od-editable"));
        assert!(!inner_pattern.html.contains("data-od-kind"));

        let outer_doc = BuilderDocument {
            schema_version: 1,
            sections: vec![SectionNode {
                id: "sec-1".into(),
                meta: None,
                responsive: None,
                style: None,
                children: vec![RowNode {
                    id: "row-1".into(),
                    meta: None,
                    responsive: None,
                    children: vec![ColumnNode {
                        id: "col-1".into(),
                        meta: None,
                        responsive: None,
                        children: vec![
                            ElementNode {
                                id: "part-1".into(),
                                meta: None,
                                content: ElementContent::Known(KnownElement::TemplatePart(
                                    TemplatePartProps {
                                        part_key: "header".into(),
                                    },
                                )),
                            },
                            ElementNode {
                                id: "pat-1".into(),
                                meta: None,
                                content: ElementContent::Known(KnownElement::PatternRef(
                                    PatternRefProps { pattern_id: 7 },
                                )),
                            },
                            ElementNode {
                                id: "slot-1".into(),
                                meta: None,
                                content: ElementContent::Known(KnownElement::ContentSlot(
                                    ContentSlotProps {},
                                )),
                            },
                        ],
                    }],
                }],
            }],
        };
        let mut patterns = PatternResults::new();
        patterns.insert(7, inner_pattern);
        let mut parts = BTreeMap::new();
        parts.insert(
            "header".into(),
            "<section data-node-id=\"hdr-sec\"><p>Header</p></section>".into(),
        );
        let ctx = BuilderTemplateContext {
            content_html: Some("<main><p data-node-id=\"entry-inner\">Entry</p></main>"),
            parts: &parts,
        };

        // Public render (is_preview = false) and Scope::None emit zero data-od-editable attributes.
        let public_out = render_builder_with_annotations(
            &state,
            &outer_doc,
            false,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            Some(&ctx),
            &patterns,
            false,
            RenderAnnotationScope::TemplateAll,
        )
        .unwrap();
        assert!(!public_out.html.contains("data-od-editable"));
        assert!(!public_out.html.contains("data-od-kind"));

        let unscoped_preview = render_builder_with_patterns(
            &state,
            &outer_doc,
            true,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            Some(&ctx),
            &patterns,
            false,
        )
        .unwrap();
        assert!(!unscoped_preview.html.contains("data-od-editable"));

        // Scoped preview annotates only outer template nodes, never embedded pattern/part/slot internals.
        let scoped = render_builder_with_annotations(
            &state,
            &outer_doc,
            true,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            Some(&ctx),
            &patterns,
            false,
            RenderAnnotationScope::TemplateAll,
        )
        .unwrap();
        assert!(scoped.html.contains(
            "<section data-node-id=\"sec-1\" data-od-editable=\"1\" data-od-kind=\"section\">"
        ));
        assert!(scoped.html.contains(
            "<div class=\"builder-row\" data-node-id=\"row-1\" data-od-editable=\"1\" data-od-kind=\"row\">"
        ));
        assert!(scoped.html.contains(
            "<div class=\"builder-column\" data-node-id=\"col-1\" data-od-editable=\"1\" data-od-kind=\"column\">"
        ));
        assert!(scoped.html.contains(
            "<div class=\"builder-element\" data-node-id=\"pat-1\" data-od-editable=\"1\" data-od-kind=\"element\">"
        ));
        assert!(scoped.html.contains(
            "<section data-node-id=\"psec-1\"><div class=\"builder-row\" data-node-id=\"prow-1\">"
        ));
        assert!(scoped.html.contains("<section data-node-id=\"hdr-sec\">"));
        assert!(scoped.html.contains("<p data-node-id=\"entry-inner\">"));
    }

    #[test]
    fn renders_all_five_core_blocks_with_scoped_css_responsive_breakpoints_and_budget_isolation() {
        let state = HomeState {
            version: 1,
            declared_blocks: crate::storage::page::embedded_core_block_defs().clone(),
            ..Default::default()
        };
        let media_id = "d".repeat(64);
        let doc = BuilderDocument {
            schema_version: 1,
            sections: vec![SectionNode {
                id: "sec-core".into(),
                meta: None,
                responsive: None,
                style: None,
                children: vec![RowNode {
                    id: "row-core".into(),
                    meta: None,
                    responsive: None,
                    children: vec![ColumnNode {
                        id: "col-core".into(),
                        meta: None,
                        responsive: None,
                        children: vec![
                            ElementNode {
                                id: "sp-1".into(),
                                meta: None,
                                content: ElementContent::Known(KnownElement::PluginBlock(
                                    PluginBlockProps {
                                        block_id: "core/spacer".into(),
                                        values: BTreeMap::from([(
                                            "height".into(),
                                            json!({ "desktop": 64, "tablet": 40, "mobile": 24 }),
                                        )]),
                                    },
                                )),
                            },
                            ElementNode {
                                id: "div-1".into(),
                                meta: None,
                                content: ElementContent::Known(KnownElement::PluginBlock(
                                    PluginBlockProps {
                                        block_id: "core/divider".into(),
                                        values: BTreeMap::from([
                                            ("line_style".into(), json!("dashed")),
                                            ("thickness".into(), json!(2)),
                                            (
                                                "width_pct".into(),
                                                json!({ "desktop": 80, "mobile": 100 }),
                                            ),
                                            ("color".into(), json!("var(--od-token-border)")),
                                        ]),
                                    },
                                )),
                            },
                            ElementNode {
                                id: "ban-1".into(),
                                meta: None,
                                content: ElementContent::Known(KnownElement::PluginBlock(
                                    PluginBlockProps {
                                        block_id: "core/banner".into(),
                                        values: BTreeMap::from([
                                            ("bg_media".into(), json!(media_id)),
                                            ("overlay_color".into(), json!("#00000080")),
                                            (
                                                "min_height".into(),
                                                json!({ "desktop": 480, "tablet": 360 }),
                                            ),
                                            ("focus_x".into(), json!(30)),
                                            ("focus_y".into(), json!(70)),
                                            (
                                                "align".into(),
                                                json!({ "desktop": "left", "mobile": "center" }),
                                            ),
                                            ("heading".into(), json!("Khuyến mãi Mùa Thu")),
                                            (
                                                "body".into(),
                                                json!(
                                                    "<p>Giảm <strong>50%</strong><script>evil()</script></p>"
                                                ),
                                            ),
                                            ("show_cta".into(), json!(true)),
                                            ("cta_label".into(), json!("Mua ngay")),
                                            ("cta_url".into(), json!("/cua-hang")),
                                        ]),
                                    },
                                )),
                            },
                            ElementNode {
                                id: "card-1".into(),
                                meta: None,
                                content: ElementContent::Known(KnownElement::PluginBlock(
                                    PluginBlockProps {
                                        block_id: "core/card".into(),
                                        values: BTreeMap::from([
                                            ("layout".into(), json!("horizontal")),
                                            ("image".into(), json!(media_id)),
                                            ("image_alt".into(), json!("Ảnh sản phẩm")),
                                            ("title".into(), json!("Thẻ VIP")),
                                            (
                                                "body".into(),
                                                json!("<p>Quyền lợi <em>đặc biệt</em></p>"),
                                            ),
                                            ("bg_color".into(), json!("#f8fafc")),
                                            ("show_cta".into(), json!(true)),
                                            ("cta_label".into(), json!("Đăng ký")),
                                            ("cta_url".into(), json!("#vip")),
                                        ]),
                                    },
                                )),
                            },
                            ElementNode {
                                id: "acc-1".into(),
                                meta: None,
                                content: ElementContent::Known(KnownElement::PluginBlock(
                                    PluginBlockProps {
                                        block_id: "core/accordion".into(),
                                        values: BTreeMap::from([
                                            ("style".into(), json!("bordered")),
                                            ("open_first".into(), json!(true)),
                                            (
                                                "items".into(),
                                                json!([
                                                    { "title": "Câu hỏi 1", "content": "<p>Trả lời <b>1</b></p>", "open": false },
                                                    { "title": "Câu hỏi 2", "content": "<p>Trả lời <i>2</i></p>", "open": false }
                                                ]),
                                            ),
                                        ]),
                                    },
                                )),
                            },
                        ],
                    }],
                }],
            }],
        };

        state.validate(&doc).expect("all 5 core blocks validate");

        let mut budget = RenderBudget::new();
        let cache = BlockTemplateCache::new();
        let public_rendered = render_builder_with_budget(
            &state,
            &doc,
            false,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            None,
            &BTreeMap::new(),
            false,
            RenderAnnotationScope::None,
            "entry",
            &mut budget,
            &cache,
        )
        .unwrap();

        assert!(public_rendered.diagnostics.is_empty());
        assert!(
            public_rendered
                .html
                .contains("<style data-od-block-styles>")
        );
        assert!(
            public_rendered
                .html
                .contains("[data-od-node-scope=\"entry-sp-1\"]{--od-spacer-h:64px;}")
        );
        assert!(public_rendered.html.contains(
            "@media (width < 1024px){[data-od-node-scope=\"entry-sp-1\"]{--od-spacer-h:40px;}"
        ));
        assert!(public_rendered.html.contains(
            "@media (width < 768px){[data-od-node-scope=\"entry-sp-1\"]{--od-spacer-h:24px;}"
        ));
        assert!(
            public_rendered
                .html
                .contains(&format!("--od-banner-bg:url(\"/media/{media_id}/file\");"))
        );
        assert!(
            public_rendered
                .html
                .contains("--od-banner-fx:30%;--od-banner-fy:70%;")
        );
        assert!(
            public_rendered
                .html
                .contains("<p>Giảm <strong>50%</strong></p>")
        );
        assert!(!public_rendered.html.contains("<script>"));
        assert!(
            public_rendered
                .html
                .contains("<details class=\"od-accordion-item\" open>")
        );
        assert!(
            public_rendered
                .html
                .contains("<details class=\"od-accordion-item\">")
        );

        // Preview uses /api/admin/media/{id}/file in both CSS and <img>
        let mut preview_budget = RenderBudget::new();
        let preview_rendered = render_builder_with_budget(
            &state,
            &doc,
            true,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            None,
            &BTreeMap::new(),
            false,
            RenderAnnotationScope::EntryBodyOnly,
            "pat_42",
            &mut preview_budget,
            &cache,
        )
        .unwrap();
        assert!(
            preview_rendered
                .html
                .contains("[data-od-node-scope=\"pat_42-ban-1\"]")
        );
        assert!(preview_rendered.html.contains(&format!(
            "--od-banner-bg:url(\"/api/admin/media/{media_id}/file\");"
        )));
        let decoded_preview =
            opendash_core::richtext::decode_html5_entities(&preview_rendered.html);
        assert!(decoded_preview.contains(&format!("src=\"/api/admin/media/{media_id}/file\"")));

        // Exhausted fuel isolates block with plugin_block_render_error instead of failing request
        let mut zero_fuel_budget = RenderBudget {
            fuel_remaining: 0,
            html_bytes_used: 0,
            ..RenderBudget::default()
        };
        let isolated = render_builder_with_budget(
            &state,
            &doc,
            false,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            None,
            &BTreeMap::new(),
            false,
            RenderAnnotationScope::None,
            "entry",
            &mut zero_fuel_budget,
            &cache,
        )
        .expect("exhausted fuel must isolate plugin blocks without HTTP 500");
        assert_eq!(isolated.diagnostics.len(), 5);
        assert!(
            isolated
                .diagnostics
                .iter()
                .all(|d| d.code == "plugin_block_render_error")
        );
    }

    #[test]
    fn renders_node_meta_attributes_visibility_and_omits_synthetic_wrappers() {
        use opendash_core::builder::{
            ColumnNode, ElementContent, ElementNode, HeadingProps, KnownElement, NodeMeta,
            NodeVisibility, RowNode, SectionNode,
        };

        let state = HomeState::default();
        let cache = BlockTemplateCache::new();
        let doc = BuilderDocument {
            schema_version: 1,
            sections: vec![SectionNode {
                id: "sec-syn".into(),
                meta: Some(NodeMeta {
                    synthetic: true,
                    ..Default::default()
                }),
                responsive: None,
                style: None,
                children: vec![RowNode {
                    id: "row-syn".into(),
                    meta: Some(NodeMeta {
                        synthetic: true,
                        ..Default::default()
                    }),
                    responsive: None,
                    children: vec![ColumnNode {
                        id: "col-syn".into(),
                        meta: Some(NodeMeta {
                            synthetic: true,
                            ..Default::default()
                        }),
                        responsive: None,
                        children: vec![ElementNode {
                            id: "el-hero".into(),
                            meta: Some(NodeMeta {
                                name: Some("Tiêu đề Hero".into()),
                                html_id: Some("hero-anchor".into()),
                                class_name: Some("py-8 text-center".into()),
                                visibility: Some(NodeVisibility {
                                    hide_desktop: false,
                                    hide_tablet: true,
                                    hide_mobile: false,
                                }),
                                synthetic: false,
                            }),
                            content: ElementContent::Known(KnownElement::Heading(HeadingProps {
                                level: 2,
                                text: "Hello UX Builder".into(),
                                binding: None,
                            })),
                        }],
                    }],
                }],
            }],
        };

        let mut budget = RenderBudget::new();
        let rendered = render_builder_with_budget(
            &state,
            &doc,
            false,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            None,
            &BTreeMap::new(),
            false,
            RenderAnnotationScope::None,
            "entry",
            &mut budget,
            &cache,
        )
        .unwrap();

        assert!(!rendered.html.contains("<section"));
        assert!(!rendered.html.contains("builder-row"));
        assert!(!rendered.html.contains("builder-column"));
        assert!(rendered.html.contains("id=\"hero-anchor\""));
        assert!(
            rendered
                .html
                .contains("class=\"builder-element py-8 text-center\"")
        );
        assert!(
            rendered
                .html
                .contains("@media (width >= 768px) and (width < 1024px)")
        );
        assert!(
            rendered
                .html
                .contains("[data-node-id=\"el-hero\"]{display:none !important;}")
        );
    }

    #[test]
    fn renders_section_style_box_spacing_rich_text_button_flags_and_new_schema_fields() {
        use opendash_core::{
            block_schema::BlockSchemaV1,
            builder::{
                BoxSpacing, ButtonProps, ColumnNode, ElementContent, ElementNode, KnownElement,
                PluginBlockProps, Responsive, ResponsiveStyle, RowNode, SectionNode, SectionStyle,
                SectionWidth, TextAlign, TextProps,
            },
            registry::{DeclarationState, ProviderStatus, ResourceOwner},
        };

        let bg_id = "f".repeat(64);
        let schema: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "demo/ux-card",
            "title": "UX Card",
            "category": "content",
            "render": "render.html",
            "fields": [
                {
                    "type": "radio_buttons",
                    "key": "align",
                    "label": "Align",
                    "options": [{ "value": "left", "label": "Trái" }, { "value": "center", "label": "Giữa" }],
                    "css_var": "--od-card-align",
                    "responsive": true
                },
                {
                    "type": "spacing",
                    "key": "pad",
                    "label": "Padding",
                    "unit": "px",
                    "css_var": "--od-card-pad",
                    "responsive": true
                },
                { "type": "link", "key": "cta", "label": "CTA" }
            ]
        }))
        .unwrap();
        let template = "<div class=\"ux-card\"><a href=\"{{ cta_url }}\" target=\"{{ cta_target }}\" rel=\"{{ cta_rel }}\">Go</a></div>".to_string();
        crate::template_sec::validate_block_template_csec("demo/ux-card", &template, &schema)
            .expect("C-SEC template using Link synthetic attrs must pass validation");

        let mut declared = crate::storage::page::embedded_core_block_defs().clone();
        declared.insert(
            "demo/ux-card".into(),
            crate::storage::page::RegisteredBlockDef {
                block_id: "demo/ux-card".into(),
                owner: ResourceOwner::Theme("demo".into()),
                status: ProviderStatus::Active,
                declaration_state: DeclarationState::Declared,
                schema: opendash_core::block_schema::RegisteredBlockSchema {
                    schema: schema.clone(),
                    legacy_mode: false,
                },
                template,
                revision_digest: "rev-demo-1".into(),
            },
        );
        let state = HomeState {
            version: 1,
            declared_blocks: declared,
            ..Default::default()
        };

        let doc = BuilderDocument {
            schema_version: 1,
            sections: vec![SectionNode {
                id: "sec-ux".into(),
                meta: None,
                responsive: Some(Responsive {
                    desktop: Some(ResponsiveStyle {
                        padding: Some(BoxSpacing {
                            top: Some(40),
                            right: Some(16),
                            bottom: Some(40),
                            left: Some(16),
                        }),
                        margin: Some(BoxSpacing {
                            top: Some(0),
                            bottom: Some(24),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                style: Some(SectionStyle {
                    background_color: Some("#0f172a".into()),
                    background_media_id: Some(bg_id.clone()),
                    content_width: Some(SectionWidth::Boxed),
                }),
                children: vec![RowNode {
                    id: "row-ux".into(),
                    meta: None,
                    responsive: Some(Responsive {
                        desktop: Some(ResponsiveStyle {
                            columns_per_row: Some(3),
                            gap_px: Some(24),
                            ..Default::default()
                        }),
                        mobile: Some(ResponsiveStyle {
                            columns_per_row: Some(1),
                            gap_px: Some(16),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    children: vec![ColumnNode {
                        id: "col-ux".into(),
                        meta: None,
                        responsive: Some(Responsive {
                            desktop: Some(ResponsiveStyle {
                                text_align: Some(TextAlign::Justify),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        children: vec![
                            ElementNode {
                                id: "el-rich".into(),
                                meta: None,
                                content: ElementContent::Known(KnownElement::Text(TextProps {
                                    text:
                                        "<p>Hello <strong>Rich</strong><script>bad()</script></p>"
                                            .into(),
                                    rich: true,
                                    binding: None,
                                })),
                            },
                            ElementNode {
                                id: "el-btn".into(),
                                meta: None,
                                content: ElementContent::Known(KnownElement::Button(ButtonProps {
                                    label: "Visit".into(),
                                    url: "https://opendash.vn".into(),
                                    new_tab: true,
                                    nofollow: true,
                                    binding: None,
                                })),
                            },
                            ElementNode {
                                id: "el-ux".into(),
                                meta: None,
                                content: ElementContent::Known(KnownElement::PluginBlock(
                                    PluginBlockProps {
                                        block_id: "demo/ux-card".into(),
                                        values: BTreeMap::from([
                                            (
                                                "align".into(),
                                                json!({ "desktop": "center", "mobile": "left" }),
                                            ),
                                            (
                                                "pad".into(),
                                                json!({ "desktop": { "top": 20, "right": 12, "bottom": 20, "left": 12 } }),
                                            ),
                                            (
                                                "cta".into(),
                                                json!({ "url": "https://opendash.vn/docs", "new_tab": true, "nofollow": false }),
                                            ),
                                        ]),
                                    },
                                )),
                            },
                        ],
                    }],
                }],
            },
            SectionNode {
                id: "sec-custom".into(),
                meta: None,
                responsive: Some(Responsive {
                    desktop: Some(ResponsiveStyle {
                        padding: Some(BoxSpacing {
                            top: Some(37),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                style: None,
                children: vec![RowNode {
                    id: "row-custom".into(),
                    meta: None,
                    responsive: Some(Responsive {
                        desktop: Some(ResponsiveStyle {
                            columns_per_row: Some(2),
                            gap_px: Some(19),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    children: vec![],
                }],
            }],
        };

        let mut budget = RenderBudget::new();
        let cache = BlockTemplateCache::new();
        let preview = render_builder_with_budget(
            &state,
            &doc,
            true,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            None,
            &BTreeMap::new(),
            false,
            RenderAnnotationScope::EntryBodyOnly,
            "entry",
            &mut budget,
            &cache,
        )
        .unwrap();

        assert!(preview.html.contains("background-color:#0f172a;"));
        assert!(preview.html.contains(&format!(
            "background-image:url(\"/api/admin/media/{bg_id}/file\");"
        )));
        assert!(preview.html.contains("class=\"px-4 py-10 mt-0 mb-6\""));
        assert!(
            preview
                .html
                .contains("class=\"builder-row w-full max-w-7xl mx-auto grid grid-cols-1 md:grid-cols-3 gap-4 md:gap-6\"")
        );
        assert!(
            preview
                .html
                .contains("class=\"builder-column text-justify\"")
        );
        assert!(!preview.html.contains("padding-top:40px;"));
        assert!(!preview.html.contains("text-align:justify;"));
        assert!(preview.html.contains("padding-top:37px;"));
        assert!(preview.html.contains("gap:19px;"));
        assert!(
            preview
                .html
                .contains("class=\"builder-row grid grid-cols-2\"")
        );
        assert!(
            preview
                .html
                .contains("<div class=\"od-rich-text\"><p>Hello <strong>Rich</strong></p></div>")
        );
        assert!(!preview.html.contains("<script>"));
        assert!(
            preview
                .html
                .contains("target=\"_blank\" rel=\"noopener noreferrer nofollow\"")
        );
        assert!(preview.html.contains("--od-card-align:center;"));
        assert!(preview.html.contains("--od-card-pad:20px 12px 20px 12px;"));
        assert!(
            preview
                .html
                .contains("target=\"_blank\" rel=\"noopener noreferrer\">Go</a>")
        );
    }

    #[test]
    fn renders_social_follow_block_from_site_options_for_public_and_preview() {
        use opendash_core::builder::{
            ColumnNode, ElementContent, ElementNode, KnownElement, PluginBlockProps, RowNode,
            SectionNode,
        };

        let doc = BuilderDocument {
            schema_version: 1,
            sections: vec![SectionNode {
                id: "sec-sf".into(),
                meta: None,
                responsive: None,
                style: None,
                children: vec![RowNode {
                    id: "row-sf".into(),
                    meta: None,
                    responsive: None,
                    children: vec![ColumnNode {
                        id: "col-sf".into(),
                        meta: None,
                        responsive: None,
                        children: vec![ElementNode {
                            id: "sf-1".into(),
                            meta: None,
                            content: ElementContent::Known(KnownElement::PluginBlock(
                                PluginBlockProps {
                                    block_id: "core/social-follow".into(),
                                    values: BTreeMap::from([
                                        ("heading".into(), json!("Theo dõi")),
                                        ("show_facebook".into(), json!(true)),
                                        ("show_youtube".into(), json!(false)),
                                        ("display_style".into(), json!("pill")),
                                        ("open_new_tab".into(), json!(true)),
                                    ]),
                                },
                            )),
                        }],
                    }],
                }],
            }],
        };

        let cache = BlockTemplateCache::new();
        let state_with_links = HomeState {
            version: 1,
            declared_blocks: crate::storage::page::embedded_core_block_defs().clone(),
            site_options: BTreeMap::from([
                (
                    "social_facebook".into(),
                    json!("https://facebook.com/opendash"),
                ),
                (
                    "social_youtube".into(),
                    json!("https://youtube.com/@opendash"),
                ),
            ]),
            ..Default::default()
        };
        let mut budget = RenderBudget::new();
        let rendered = render_builder_with_budget(
            &state_with_links,
            &doc,
            false,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            None,
            &BTreeMap::new(),
            false,
            RenderAnnotationScope::None,
            "entry",
            &mut budget,
            &cache,
        )
        .unwrap();
        assert!(
            rendered.diagnostics.is_empty(),
            "{:?}",
            rendered.diagnostics
        );
        let decoded_html = opendash_core::richtext::decode_html5_entities(&rendered.html);
        assert!(
            decoded_html.contains("href=\"https://facebook.com/opendash\""),
            "{decoded_html}"
        );
        assert!(!decoded_html.contains("youtube.com/@opendash"));
        assert!(decoded_html.contains("target=\"_blank\" rel=\"noopener noreferrer\""));

        // Empty global links: public omits output, preview shows configuration hint.
        let state_empty = HomeState {
            version: 1,
            declared_blocks: crate::storage::page::embedded_core_block_defs().clone(),
            ..Default::default()
        };
        let mut pub_budget = RenderBudget::new();
        let pub_empty = render_builder_with_budget(
            &state_empty,
            &doc,
            false,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            None,
            &BTreeMap::new(),
            false,
            RenderAnnotationScope::None,
            "entry",
            &mut pub_budget,
            &cache,
        )
        .unwrap();
        assert!(!pub_empty.html.contains("od-social-follow"));

        let mut prev_budget = RenderBudget::new();
        let prev_empty = render_builder_with_budget(
            &state_empty,
            &doc,
            true,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            None,
            &BTreeMap::new(),
            false,
            RenderAnnotationScope::EntryBodyOnly,
            "entry",
            &mut prev_budget,
            &cache,
        )
        .unwrap();
        assert!(prev_empty.html.contains("od-social-follow-empty"));
        assert!(prev_empty.html.contains("Tùy chỉnh giao diện"));
    }
}
