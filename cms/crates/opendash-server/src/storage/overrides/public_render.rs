use super::*;

/// The public S02 graph has one budget across Builder and C-SEC segments.
pub struct PublicCompositionBudget {
    pub render_budget: page::RenderBudget,
    pub(in crate::storage) expansions: usize,
    pub(in crate::storage) shared_l10n:
        BTreeMap<String, std::sync::Arc<opendash_core::l10n::Catalog>>,
    pub(in crate::storage) shared_locale_code: String,
    pub(in crate::storage) shared_domain: Option<String>,
    pub(in crate::storage) shared_locale: Option<minijinja::Value>,
}

impl Default for PublicCompositionBudget {
    fn default() -> Self {
        Self {
            render_budget: page::RenderBudget::new(),
            expansions: 0,
            shared_l10n: BTreeMap::new(),
            shared_locale_code: "en".to_owned(),
            shared_domain: None,
            shared_locale: None,
        }
    }
}

impl PublicCompositionBudget {
    pub fn with_render_budget(render_budget: page::RenderBudget) -> Self {
        Self {
            render_budget,
            expansions: 0,
            shared_l10n: BTreeMap::new(),
            shared_locale_code: "en".to_owned(),
            shared_domain: None,
            shared_locale: None,
        }
    }

    /// Pre-computes and shares `Arc`-backed `l10n` catalogs and `locale` MiniJinja values
    /// once per request across header, body and footer segments.
    pub fn set_locale_context(
        &mut self,
        active: Option<&ActiveThemeV3>,
        locale_code: &str,
        locale_ctx: serde_json::Value,
    ) {
        self.set_locale_context_with_registry(
            active,
            Some(crate::l10n::CatalogRegistry::global()),
            locale_code,
            locale_ctx,
        );
    }

    pub fn set_locale_context_with_registry(
        &mut self,
        active: Option<&ActiveThemeV3>,
        registry: Option<&crate::l10n::CatalogRegistry>,
        locale_code: &str,
        locale_ctx: serde_json::Value,
    ) {
        self.shared_locale_code = locale_code.to_owned();
        let domain = active.map(|v| {
            v.manifest
                .text_domain
                .clone()
                .unwrap_or_else(|| v.manifest.theme_id.clone())
        });
        self.shared_domain = domain.clone();
        let mut catalogs = active
            .map(|v| v.l10n_catalog_map(locale_code))
            .unwrap_or_default();
        if let Some(reg) = registry {
            let reg_map = reg.theme_render_catalogs(domain.as_deref().unwrap_or(""), locale_code);
            for (k, v) in reg_map {
                catalogs.insert(k, v);
            }
        }
        self.shared_l10n = catalogs;
        self.shared_locale = Some(template_sec::json_to_safe_value(&locale_ctx));
    }

    pub(in crate::storage) fn charge(&mut self, count: usize) -> ApiResult<()> {
        self.expansions = self
            .expansions
            .checked_add(count)
            .filter(|n| *n <= 32)
            .ok_or_else(|| ApiError::internal("Template parts exceed 32 expansions."))?;
        Ok(())
    }

    pub fn render_theme_with_config(
        &mut self,
        compiled: &template_sec::CompiledCsecComposition,
        context: &serde_json::Value,
        slot: Option<&str>,
        fmt_cfg: &template_sec::RenderFormattingConfig,
    ) -> ApiResult<String> {
        self.charge(compiled.part_expansions())?;
        let owned_cfg;
        let effective_cfg = if fmt_cfg.l10n_catalogs.is_empty() && !self.shared_l10n.is_empty() {
            owned_cfg = template_sec::RenderFormattingConfig {
                timezone: fmt_cfg.timezone.clone(),
                l10n_locale: Some(self.shared_locale_code.clone()),
                l10n_domain: self.shared_domain.clone(),
                l10n_catalogs: self.shared_l10n.clone(),
            };
            &owned_cfg
        } else {
            fmt_cfg
        };
        let html = compiled
            .render_with_config_and_budget(
                template_sec::json_context_with_shared_locale(
                    context,
                    None,
                    self.shared_locale.as_ref(),
                ),
                slot,
                &mut self.render_budget,
                effective_cfg,
            )
            .map_err(ApiError::internal)?;
        let own_bytes = html.len().saturating_sub(slot.map_or(0, str::len));
        self.render_budget
            .charge_html_bytes(own_bytes)
            .map_err(ApiError::internal)?;
        Ok(html)
    }
}

pub(in crate::storage) struct PublicBuilderContext<'a> {
    pub(in crate::storage) state: &'a BuilderRenderContext,
    pub(in crate::storage) active: Option<&'a ActiveThemeV3>,
    pub(in crate::storage) theme_context: &'a serde_json::Value,
    pub(in crate::storage) route_entry: Option<(&'a str, &'a str)>,
    pub(in crate::storage) render_locale: &'a str,
    pub(in crate::storage) cache: &'a page::BlockTemplateCache,
    pub(in crate::storage) url_policy: &'a template_sec::ThemeUrlPolicy,
    pub(in crate::storage) fmt_cfg: &'a template_sec::RenderFormattingConfig,
}

#[allow(clippy::too_many_arguments)]
pub async fn render_resolved_public_builder_with_cache_for_locale(
    db: &SqlitePool,
    record: &TemplateOverrideRecord,
    active: Option<&ActiveThemeV3>,
    theme_context: &serde_json::Value,
    route_entry: Option<(&str, &str)>,
    locale: Option<&str>,
    slot: Option<&str>,
    budget: &mut PublicCompositionBudget,
    cache: &page::BlockTemplateCache,
) -> ApiResult<RenderedLayout> {
    let mut tx = db.begin().await?;
    let effective_locale = settings::resolve_requested_locale_in_conn(&mut *tx, locale).await?;
    let mut state =
        builder::read_render_context_for_locale_tx(&mut tx, Some(&effective_locale), false, None)
            .await?;
    collect_db_docs_and_hydrate_state_in_tx(&mut tx, &mut state, &[&record.document], false)
        .await?;
    let url_policy = active_theme_url_policy(active)?;
    let domain = active.map(|a| {
        a.manifest
            .text_domain
            .clone()
            .unwrap_or_else(|| a.manifest.theme_id.clone())
    });
    let mut l10n_catalogs = active
        .map(|a| a.l10n_catalog_map(&effective_locale))
        .unwrap_or_default();
    for (k, v) in crate::l10n::CatalogRegistry::global()
        .theme_render_catalogs(domain.as_deref().unwrap_or(""), &effective_locale)
    {
        l10n_catalogs.insert(k, v);
    }
    let fmt_cfg = template_sec::RenderFormattingConfig {
        timezone: state.timezone.clone(),
        l10n_locale: Some(effective_locale.clone()),
        l10n_domain: domain,
        l10n_catalogs,
        ..Default::default()
    };
    let context = PublicBuilderContext {
        state: &state,
        active,
        theme_context,
        route_entry,
        render_locale: &effective_locale,
        cache,
        url_policy: &url_policy,
        fmt_cfg: &fmt_cfg,
    };
    let stack = vec![record.canonical_target_key.clone()];
    let rendered = render_public_builder(
        &mut tx,
        &record.document,
        slot,
        0,
        &stack,
        &context,
        budget,
        "ov",
    )
    .await?;
    tx.commit().await?;
    Ok(rendered)
}

pub(in crate::storage) fn render_csec_template_for_preview(
    target_key: &str,
    html: &str,
    active: Option<&ActiveThemeV3>,
    db_schema: Option<&EffectiveContentSchema>,
    theme_context: &serde_json::Value,
    slot_html: Option<&str>,
    budget: &mut PublicCompositionBudget,
    fmt_cfg: &template_sec::RenderFormattingConfig,
) -> ApiResult<String> {
    let kind = csec_content_kind_for_target_key(target_key);
    let empty_parts = BTreeMap::new();
    let parts = active.map(|v| &v.parts).unwrap_or(&empty_parts);
    let url_policy = active_theme_url_policy_for_target(active, Some(target_key), db_schema)?;
    let composition = if let Some(active) = active {
        template_sec::ThemeCompositionCache::global().get_or_compile(
            &active.revision_digest,
            target_key,
            html,
            parts,
            kind,
            &url_policy,
        )
    } else {
        template_sec::validate_and_compile_csec_composition_with_theme_urls(
            target_key,
            html,
            parts,
            kind,
            &url_policy,
        )
        .map(Arc::new)
    }
    .map_err(ApiError::bad_request)?;
    budget.render_theme_with_config(&composition, theme_context, slot_html, fmt_cfg)
}
