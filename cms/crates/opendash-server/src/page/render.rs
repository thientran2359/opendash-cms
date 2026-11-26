use super::*;

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

pub type ContactFormResults =
    std::collections::BTreeMap<String, Option<crate::storage::contact::ContactFormRecord>>;

/// Pattern documents are resolved from one SQLite snapshot before rendering.
/// A missing target is deliberately absent, never client-provided HTML.
pub type PatternResults = std::collections::BTreeMap<i64, RenderedLayout>;

pub(super) fn escape_html(value: &str) -> String {
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

pub(super) fn box_or_legacy_px(style: Option<&ResponsiveStyle>) -> Option<BoxSpacing> {
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

pub(super) fn align_tailwind_class(align: TextAlign) -> &'static str {
    match align {
        TextAlign::Start => "text-left",
        TextAlign::Center => "text-center",
        TextAlign::End => "text-right",
        TextAlign::Justify => "text-justify",
    }
}

pub(super) fn push_visibility_rule(target: &mut String, id: &str) {
    if valid_package_id(id) {
        target.push_str(&format!(
            "main[data-builder-version=\"1\"] [data-node-id=\"{id}\"],.od-pattern-content[data-builder-version=\"1\"] [data-node-id=\"{id}\"]{{display:none !important;}}"
        ));
    }
}

pub(super) fn meta_id_attr(meta: Option<&NodeMeta>) -> String {
    match meta
        .and_then(|m| m.html_id.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(id) => format!(" id=\"{}\"", escape_html(id)),
        None => String::new(),
    }
}

pub(super) fn merge_class_attr(
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

/// All bound values are obtained by the storage layer under Preview/public ACL.
/// The renderer never reads SQL or treats rich text as raw markup.
pub fn render_builder_resolved(
    state: &BuilderRenderContext,
    document: &BuilderDocument,
    is_preview: bool,
    bindings: &BindingResults,
) -> ApiResult<RenderedLayout> {
    render_builder_with_lists(state, document, is_preview, bindings, &ListResults::new())
}

/// Query-free renderer: storage has already resolved every list against the
/// same SQLite snapshot as the document/Field Binding sources.
pub fn render_builder_with_lists(
    state: &BuilderRenderContext,
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
    state: &BuilderRenderContext,
    document: &BuilderDocument,
    is_preview: bool,
    bindings: &BindingResults,
    lists: &ListResults,
    forms: &ContactFormResults,
) -> ApiResult<RenderedLayout> {
    render_builder_with_template_context(state, document, is_preview, bindings, lists, forms, None)
}

pub fn render_builder_with_template_context(
    state: &BuilderRenderContext,
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

pub(super) fn editable_attrs(
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
    state: &BuilderRenderContext,
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
    render_plugin_block_csec_with_budget_and_config(
        block_id,
        template_source,
        values,
        budget,
        &crate::template_sec::RenderFormattingConfig::default(),
    )
}

#[cfg(test)]
pub fn render_plugin_block_csec_with_budget_and_config(
    block_id: &str,
    template_source: &str,
    values: &std::collections::BTreeMap<String, serde_json::Value>,
    budget: &mut RenderBudget,
    fmt_cfg: &crate::template_sec::RenderFormattingConfig,
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
    let mut env = crate::template_sec::restricted_environment_with_config(fmt_cfg);
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
