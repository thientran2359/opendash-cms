//! Public CPT archive/single routes and referenced-only media delivery.
//! Legacy CPT Save stays live; core post/page draft data is never served here.

use super::*;

pub(super) use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(super) use axum::{
    Extension,
    extract::{Path, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
};

pub(super) use opendash_core::content::{ContentTypeDefinition, FieldRole};

pub(super) use opendash_core::theme::{
    EnqueueCondition, ResolvedEnqueuedAssets, resolve_enqueued_assets,
};

pub(super) use sqlx::SqlitePool;

pub(super) use crate::{
    content_render::{self, PaginationView, PublicThemeContextInput, SiteContext},
    error::{ApiError, ApiResult},
    media::MediaStore,
    public_context::{
        ARCHIVE_SIZE, CanonicalUrlBuilder, MAX_ARCHIVE_PAGE, PublicQuery, PublicRequestContext,
        QueryScope, TaxonomyKind, parse_query,
    },
    routes::themes::ThemeStoreRoot,
    storage::{
        content as content_storage,
        entries::EntryFilters,
        media,
        overrides::{self, PublicTemplateContext, TemplateResolution},
        permalinks, public_content,
        seo::{self, ReadingPageState},
        settings, theme as theme_storage,
    },
};

#[derive(Clone)]
pub(crate) struct PublicRenderServices {
    pub hooks: Arc<crate::hooks::HookDispatcher>,
    pub render_cache: Arc<crate::cache::ExtensionRenderCache>,
    pub origin_host: String,
}

pub(super) struct PublicRouteContext<'a> {
    pub(super) route_kind: &'a str,
    pub(super) route_path: Option<&'a str>,
    pub(super) resolved_locale: &'a str,
    pub(super) locale_target: seo::ThemeLocaleTarget<'a>,
    pub(super) entry_id: Option<i64>,
    pub(super) entry_type_key: Option<&'a str>,
    pub(super) entry_schema: Option<&'a opendash_core::content::EffectiveContentSchema>,
    pub(super) entry_body_html: Option<&'a str>,
    pub(super) entry_has_builder_body: bool,
    pub(super) custom_options: &'a BTreeMap<String, serde_json::Value>,
    pub(super) breadcrumbs: &'a [content_render::BreadcrumbItem],
    pub(super) pagination: &'a content_render::PaginationView,
    pub(super) archive_url: Option<&'a str>,
    pub(super) previous_url: Option<&'a str>,
    pub(super) next_url: Option<&'a str>,
    pub(super) entries: &'a [content_render::EntryView],
    pub(super) taxonomy: Option<&'a serde_json::Value>,
    pub(super) term: Option<&'a content_render::PublicTermDetailView>,
    pub(super) term_schema: Option<&'a opendash_core::content::EffectiveTermSchema>,
    pub(super) term_summary: Option<content_render::TermRenderSummary>,
    pub(super) content_type: Option<&'a serde_json::Value>,
    pub(super) archive_type: Option<content_render::ArchiveTypeSummary>,
    pub(super) entry: Option<&'a content_render::EntryView>,
    pub(super) list_pages: Option<&'a BTreeMap<String, i64>>,
    pub(super) search: &'a content_render::SearchView,
}

pub struct ResolvedPublicHtml {
    pub html: String,
    pub has_active_contact_form: bool,
    pub needs_blocks_js: bool,
    pub used_entry_content: bool,
    /// Page Layouts own their canvas; wrapping them in the site's default
    /// header/footer would violate the layout choice.
    pub canvas_layout: bool,
    pub theme_context: serde_json::Value,
    pub precompiled_shell: overrides::PrecompiledPublicShell,
    pub(super) template_data: Option<template_data::OwnedPublicTemplateData>,
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn render_resolved_public_template_html(
    db: &SqlitePool,
    resolution: TemplateResolution,
    active: Option<&theme_storage::ActiveTheme>,
    site_ctx: &SiteContext<'_>,
    site_opts: &BTreeMap<String, serde_json::Value>,
    theme_opts: &BTreeMap<String, serde_json::Value>,
    locale: Option<&str>,
    fallback_html: &str,
    route_entry: Option<(&str, &str)>,
    route_ctx: PublicRouteContext<'_>,
    budget: &mut overrides::PublicCompositionBudget,
    cache: &crate::page::BlockTemplateCache,
) -> ApiResult<ResolvedPublicHtml> {
    render_resolved_public_template_html_with_surface(
        db,
        resolution,
        active,
        site_ctx,
        site_opts,
        theme_opts,
        locale,
        fallback_html,
        route_entry,
        route_ctx,
        None,
        budget,
        cache,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn render_resolved_public_template_html_with_surface(
    db: &SqlitePool,
    resolution: TemplateResolution,
    active: Option<&theme_storage::ActiveTheme>,
    site_ctx: &SiteContext<'_>,
    site_opts: &BTreeMap<String, serde_json::Value>,
    theme_opts: &BTreeMap<String, serde_json::Value>,
    locale: Option<&str>,
    fallback_html: &str,
    route_entry: Option<(&str, &str)>,
    route_ctx: PublicRouteContext<'_>,
    pre_inspected: Option<template_data::PreInspectedSurface>,
    budget: &mut overrides::PublicCompositionBudget,
    cache: &crate::page::BlockTemplateCache,
) -> ApiResult<ResolvedPublicHtml> {
    let pre = match pre_inspected {
        Some(p) => p,
        None => {
            template_data::inspect_surface_for_resolution(
                db,
                &resolution,
                active,
                route_ctx.entry_schema,
                route_ctx.term_schema,
            )
            .await?
        }
    };
    let template_data::PreInspectedSurface {
        url_policy,
        inspection,
        cpt_schema: _,
    } = pre;

    if !inspection.has_shell_or_theme && matches!(&resolution, TemplateResolution::CoreFallback) {
        return Ok(ResolvedPublicHtml {
            html: fallback_html.to_owned(),
            has_active_contact_form: false,
            needs_blocks_js: false,
            used_entry_content: true,
            canvas_layout: false,
            theme_context: serde_json::Value::Null,
            precompiled_shell: overrides::PrecompiledPublicShell::default(),
            template_data: None,
        });
    }

    let menus =
        crate::storage::menu::resolve_active_slot_menus_for_path(db, route_ctx.route_path, locale)
            .await?;
    let related_entries = if inspection.uses_related_entries
        && let (Some(eid), Some(tkey)) = (route_ctx.entry_id, route_ctx.entry_type_key)
    {
        if matches!(&resolution, TemplateResolution::DbOverride(_)) {
            let mut tx = db.begin().await?;
            let rows = public_content::query_related_entries_in_tx(
                &mut tx,
                eid,
                tkey,
                inspection.related_limit.max(1),
                false,
                &inspection.loop_projection,
            )
            .await?;
            tx.commit().await?;
            rows
        } else {
            public_content::query_related_entries(db, eid, tkey, 6.max(inspection.related_limit))
                .await?
        }
    } else {
        Vec::new()
    };
    let owned_td = template_data::build_public_template_data(
        db,
        template_data::BuildPublicTemplateDataInput {
            site_opts,
            resolved_locale: route_ctx.resolved_locale,
            locale_target: route_ctx.locale_target,
            route_kind: route_ctx.route_kind,
            route_path: route_ctx.route_path,
            entry_id: route_ctx.entry_id,
            entry_type_key: route_ctx.entry_type_key,
            breadcrumbs: route_ctx.breadcrumbs,
            pagination: route_ctx.pagination,
            entry: route_ctx.entry,
            entry_schema: route_ctx.entry_schema,
            entry_body_html: route_ctx.entry_body_html.map(str::to_owned),
            entry_has_builder_body: route_ctx.entry_has_builder_body,
            entries: route_ctx.entries,
            content_type: route_ctx.archive_type.clone(),
            term: route_ctx.term_summary.clone(),
            menus_by_slot: menus.clone(),
            inspection: &inspection,
            related_entries: related_entries.clone(),
        },
    )
    .await?;
    let exclude_post_id = (route_ctx.entry_type_key == Some("post"))
        .then_some(route_ctx.entry_id)
        .flatten();
    let recent_posts = if inspection.uses_recent_posts {
        public_content::query_recent_posts(db, exclude_post_id, locale, 6).await?
    } else {
        Vec::new()
    };
    let theme_assets = active
        .map(theme_storage::ActiveTheme::asset_url_map)
        .unwrap_or_default();
    let context = content_render::build_public_theme_context(PublicThemeContextInput {
        site: site_ctx,
        site_options: site_opts,
        options: theme_opts,
        custom_options: route_ctx.custom_options,
        theme_assets: &theme_assets,
        menus: &menus,
        route_kind: route_ctx.route_kind,
        route_path: route_ctx.route_path,
        breadcrumbs: route_ctx.breadcrumbs,
        pagination: route_ctx.pagination,
        archive_url: route_ctx.archive_url,
        previous_url: route_ctx.previous_url,
        next_url: route_ctx.next_url,
        entries: route_ctx.entries,
        related_entries: &related_entries,
        recent_posts: &recent_posts,
        taxonomy: route_ctx.taxonomy,
        term: route_ctx.term,
        content_type: route_ctx.content_type,
        entry: route_ctx.entry,
        search: route_ctx.search,
    })?;
    let fmt_cfg = crate::template_sec::RenderFormattingConfig {
        timezone: site_ctx.timezone.to_owned(),
        ..Default::default()
    };

    // A fallback renderer can return a complete HTML document. Only its
    // normalized body fragment and authorized styles may enter a typed slot;
    // injecting a second document envelope would invalidate the whole page.
    let slot_html = if matches!(&resolution, TemplateResolution::CoreFallback) {
        String::new()
    } else {
        match crate::document::normalize_public_document_with_options(fallback_html, true) {
            Ok(parts) => {
                let mut fragment = parts.authorized_styles.concat();
                fragment.push_str(&parts.body_fragment);
                fragment
            }
            Err(_) => {
                // Keep the existing outer normalizer's safe diagnostic
                // fallback rather than injecting malformed content.
                return Ok(ResolvedPublicHtml {
                    html: fallback_html.to_owned(),
                    has_active_contact_form: false,
                    needs_blocks_js: false,
                    used_entry_content: true,
                    canvas_layout: false,
                    theme_context: context,
                    precompiled_shell: inspection.precompiled_shell,
                    template_data: Some(owned_td),
                });
            }
        }
    };
    match resolution {
        TemplateResolution::CoreFallback => Ok(ResolvedPublicHtml {
            html: fallback_html.to_owned(),
            has_active_contact_form: false,
            needs_blocks_js: false,
            used_entry_content: true,
            canvas_layout: false,
            theme_context: context,
            precompiled_shell: inspection.precompiled_shell,
            template_data: Some(owned_td),
        }),
        TemplateResolution::DbOverride(record) => {
            let canvas_layout = record.target.kind
                == opendash_core::theme::TemplateCandidateKind::PageLayout
                && overrides::is_canvas_page_layout(active, record.target.layout_key.as_deref());
            let empty_pages = BTreeMap::new();
            let search_data =
                route_ctx
                    .search
                    .is_search_route
                    .then_some(crate::page::BuilderSearchData {
                        view: route_ctx.search,
                        entries: route_ctx.entries,
                        pagination: route_ctx.pagination,
                    });
            let td_ref = owned_td.as_render_data(site_ctx);
            let rendered = overrides::render_resolved_public_builder_with_template_data_for_locale(
                db,
                &record,
                active,
                &context,
                route_entry,
                locale,
                Some(&slot_html),
                route_ctx.list_pages.unwrap_or(&empty_pages),
                route_ctx.route_path.unwrap_or("/"),
                search_data,
                Some(&td_ref),
                budget,
                cache,
            )
            .await?;
            Ok(ResolvedPublicHtml {
                html: rendered.html,
                has_active_contact_form: rendered.has_active_contact_form,
                needs_blocks_js: rendered.needs_blocks_js,
                used_entry_content: rendered.used_entry_content,
                canvas_layout,
                theme_context: context,
                precompiled_shell: inspection.precompiled_shell,
                template_data: Some(owned_td),
            })
        }
        TemplateResolution::ActiveTheme { target_key, html } => {
            let compiled = match inspection.precompiled_body {
                Some(compiled) => compiled,
                None => {
                    use crate::template_sec::CsecContentKind;
                    let kind = if target_key == "index" {
                        CsecContentKind::Index
                    } else if matches!(target_key.as_str(), "page" | "singular" | "single")
                        || target_key.starts_with("single:")
                        || target_key.starts_with("single_custom:")
                        || target_key.starts_with("page_layout:")
                    {
                        CsecContentKind::Entry
                    } else {
                        CsecContentKind::NonEntry
                    };
                    let empty_parts = BTreeMap::new();
                    let parts = active.map(|v| &v.parts).unwrap_or(&empty_parts);
                    if let Some(active) = active {
                        crate::template_sec::ThemeCompositionCache::global().get_or_compile(
                            &active.revision_digest,
                            &target_key,
                            &html,
                            parts,
                            kind,
                            &url_policy,
                        )
                    } else {
                        crate::template_sec::validate_and_compile_csec_composition_with_theme_urls(
                            &target_key,
                            &html,
                            parts,
                            kind,
                            &url_policy,
                        )
                        .map(Arc::new)
                    }
                    .map_err(ApiError::internal)?
                }
            };
            let html =
                budget.render_theme_with_config(&compiled, &context, Some(&slot_html), &fmt_cfg)?;
            Ok(ResolvedPublicHtml {
                html,
                has_active_contact_form: false,
                needs_blocks_js: false,
                used_entry_content: compiled.has_entry_content(),
                canvas_layout: overrides::is_canvas_page_layout(
                    active,
                    target_key.strip_prefix("page_layout:"),
                ),
                theme_context: context,
                precompiled_shell: inspection.precompiled_shell,
                template_data: Some(owned_td),
            })
        }
    }
}
