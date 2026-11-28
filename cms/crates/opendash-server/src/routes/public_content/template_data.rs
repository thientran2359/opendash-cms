//! Per-request builder for `TemplateRenderData` across public routes.
//! Avoids N+1 queries by gating adjacent/related/menu lookups on `PublicSurfaceInspection`.

use super::*;

#[derive(Clone)]
pub(super) struct OwnedPublicTemplateData {
    pub(super) site_logo: Option<content_render::HydratedMedia>,
    pub(super) locale: content_render::LocaleRenderContext,
    pub(super) menus_by_slot: BTreeMap<String, crate::storage::menu::MenuViewModel>,
    pub(super) menus_by_key: BTreeMap<String, crate::storage::menu::MenuViewModel>,
    pub(super) breadcrumbs: Vec<content_render::BreadcrumbItem>,
    pub(super) pagination: Option<PaginationView>,
    pub(super) entry: Option<content_render::EntryView>,
    pub(super) entry_schema: Option<opendash_core::content::EffectiveContentSchema>,
    pub(super) entry_body_html: Option<String>,
    pub(super) entry_has_builder_body: bool,
    pub(super) adjacent_entries_global: Option<content_render::AdjacentEntriesView>,
    pub(super) adjacent_entries_same_taxonomy: Option<content_render::AdjacentEntriesView>,
    pub(super) related_entries: Vec<content_render::EntryView>,
    pub(super) archive_entries: Vec<content_render::EntryView>,
    pub(super) content_type: Option<content_render::ArchiveTypeSummary>,
    pub(super) term: Option<content_render::TermRenderSummary>,
}

impl OwnedPublicTemplateData {
    pub(super) fn as_render_data<'a>(
        &'a self,
        site: &'a SiteContext<'a>,
    ) -> content_render::TemplateRenderData<'a> {
        content_render::TemplateRenderData {
            site,
            site_logo: self.site_logo.as_ref(),
            locale: &self.locale,
            menus_by_slot: &self.menus_by_slot,
            menus_by_key: &self.menus_by_key,
            breadcrumbs: &self.breadcrumbs,
            pagination: self.pagination.as_ref(),
            entry: self.entry.as_ref(),
            entry_schema: self.entry_schema.as_ref(),
            entry_body_html: self.entry_body_html.as_deref(),
            entry_has_builder_body: self.entry_has_builder_body,
            adjacent_entries_global: self.adjacent_entries_global.as_ref(),
            adjacent_entries_same_taxonomy: self.adjacent_entries_same_taxonomy.as_ref(),
            related_entries: &self.related_entries,
            archive_entries: &self.archive_entries,
            content_type: self.content_type.as_ref(),
            term: self.term.as_ref(),
            is_preview: false,
        }
    }
}

pub(super) struct PreInspectedSurface {
    pub(super) url_policy: crate::template_sec::ThemeUrlPolicy,
    pub(super) inspection: overrides::PublicSurfaceInspection,
    pub(super) cpt_schema: Option<opendash_core::content::EffectiveContentSchema>,
}

pub(super) async fn inspect_surface_for_resolution(
    db: &SqlitePool,
    resolution: &TemplateResolution,
    active: Option<&theme_storage::ActiveTheme>,
    known_cpt_schema: Option<&opendash_core::content::EffectiveContentSchema>,
    term_schema: Option<&opendash_core::content::EffectiveTermSchema>,
) -> ApiResult<PreInspectedSurface> {
    let cpt_schema = if known_cpt_schema.is_some() {
        known_cpt_schema.cloned()
    } else if let (Some(act), TemplateResolution::ActiveTheme { target_key, .. }) =
        (active, resolution)
        && let Some(cpt) = crate::theme::cpt_scope_for_target_key(&act.manifest, target_key)
    {
        content_storage::load_effective_content_schema(db, cpt).await?
    } else {
        None
    };
    let target_key_ref = match resolution {
        TemplateResolution::ActiveTheme { target_key, .. } => Some(target_key.as_str()),
        _ => None,
    };
    let url_policy = overrides::active_theme_url_policy_for_route(
        db,
        active,
        target_key_ref,
        cpt_schema.as_ref(),
        term_schema,
    )
    .await?;
    let inspection =
        overrides::inspect_public_surfaces_and_precompile(db, resolution, active, &url_policy)
            .await?;
    Ok(PreInspectedSurface {
        url_policy,
        inspection,
        cpt_schema,
    })
}

pub(super) struct BuildPublicTemplateDataInput<'a> {
    pub(super) site_opts: &'a BTreeMap<String, serde_json::Value>,
    pub(super) resolved_locale: &'a str,
    pub(super) locale_target: seo::ThemeLocaleTarget<'a>,
    pub(super) route_kind: &'a str,
    pub(super) route_path: Option<&'a str>,
    pub(super) entry_id: Option<i64>,
    pub(super) entry_type_key: Option<&'a str>,
    pub(super) breadcrumbs: &'a [content_render::BreadcrumbItem],
    pub(super) pagination: &'a PaginationView,
    pub(super) entry: Option<&'a content_render::EntryView>,
    pub(super) entry_schema: Option<&'a opendash_core::content::EffectiveContentSchema>,
    pub(super) entry_body_html: Option<String>,
    pub(super) entry_has_builder_body: bool,
    pub(super) entries: &'a [content_render::EntryView],
    pub(super) content_type: Option<content_render::ArchiveTypeSummary>,
    pub(super) term: Option<content_render::TermRenderSummary>,
    pub(super) menus_by_slot: BTreeMap<String, crate::storage::menu::MenuViewModel>,
    pub(super) inspection: &'a overrides::PublicSurfaceInspection,
    pub(super) related_entries: Vec<content_render::EntryView>,
}

pub(super) async fn build_public_template_data(
    db: &SqlitePool,
    input: BuildPublicTemplateDataInput<'_>,
) -> ApiResult<OwnedPublicTemplateData> {
    let site_logo = input
        .site_opts
        .get("site_logo")
        .and_then(content_render::HydratedMedia::from_value);
    let locale =
        seo::build_locale_render_context(db, Some(input.resolved_locale), input.locale_target)
            .await?;
    let menus_by_key = if input.inspection.requested_menu_keys.is_empty() {
        BTreeMap::new()
    } else {
        let keys: Vec<String> = input
            .inspection
            .requested_menu_keys
            .iter()
            .cloned()
            .collect();
        let mut conn = db.acquire().await?;
        crate::storage::menu::resolve_menus_by_keys_for_path_in_tx(
            &mut conn,
            &keys,
            input.route_path,
            Some(input.resolved_locale),
            input.menus_by_slot.len(),
        )
        .await?
    };

    let (adjacent_entries_global, adjacent_entries_same_taxonomy) =
        if (input.inspection.uses_adjacent_global || input.inspection.uses_adjacent_same_taxonomy)
            && let (Some(eid), Some(tkey)) = (input.entry_id, input.entry_type_key)
        {
            let mut tx = db.begin().await?;
            let adj_global = if input.inspection.uses_adjacent_global {
                Some(
                    public_content::query_adjacent_entries_in_tx(
                        &mut tx,
                        eid,
                        tkey,
                        input.resolved_locale,
                        false,
                        false,
                    )
                    .await?,
                )
            } else {
                None
            };
            let adj_tax = if input.inspection.uses_adjacent_same_taxonomy {
                Some(
                    public_content::query_adjacent_entries_in_tx(
                        &mut tx,
                        eid,
                        tkey,
                        input.resolved_locale,
                        true,
                        false,
                    )
                    .await?,
                )
            } else {
                None
            };
            tx.commit().await?;
            (adj_global, adj_tax)
        } else {
            (None, None)
        };

    let show_pagination = matches!(input.route_kind, "archive" | "taxonomy" | "search")
        || (input.route_kind == "home" && input.entry.is_none());

    Ok(OwnedPublicTemplateData {
        site_logo,
        locale,
        menus_by_slot: input.menus_by_slot,
        menus_by_key,
        breadcrumbs: input.breadcrumbs.to_vec(),
        pagination: show_pagination.then(|| input.pagination.clone()),
        entry: input.entry.cloned(),
        entry_schema: input.entry_schema.cloned(),
        entry_body_html: input.entry_body_html,
        entry_has_builder_body: input.entry_has_builder_body,
        adjacent_entries_global,
        adjacent_entries_same_taxonomy,
        related_entries: input.related_entries,
        archive_entries: input.entries.to_vec(),
        content_type: input.content_type,
        term: input.term,
    })
}
