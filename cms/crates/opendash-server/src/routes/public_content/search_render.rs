use axum::{
    Extension,
    http::{HeaderValue, header},
    response::Response,
};
use opendash_core::theme::EnqueueCondition;
use sqlx::SqlitePool;

use crate::{
    content_render::{self, EntryView, PaginationView, SearchView, SiteContext},
    error::ApiResult,
    public_context::{MAX_SEARCH_PAGE, SEARCH_PAGE_SIZE, build_search_url, parse_search_query},
    routes::{
        search::{SearchRateLimiter, rate_limited_ssr_response},
        themes::ThemeStoreRoot,
    },
    storage::{
        overrides::{self, PublicTemplateContext},
        search::{PublicSearchQueryInput, query_public_search},
        seo, settings, theme as theme_storage,
    },
};

use super::{
    PublicRenderServices, PublicRouteContext, apply_public_render_hooks, enqueued_for_route,
    normalized_origin, resolve_public_template_html,
};

#[allow(clippy::too_many_arguments)]
pub(crate) async fn render_search_response_for_locale(
    db: &SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    locale: &str,
    raw_query: Option<&str>,
    services: Option<&PublicRenderServices>,
    search_limiter: Option<&SearchRateLimiter>,
    peer_key: &str,
) -> ApiResult<Response> {
    let parsed = parse_search_query(raw_query).map_err(crate::error::ApiError::bad_request)?;

    if let Some(limiter) = search_limiter
        && let Err(_retry) = limiter.check(peer_key)
    {
        return Ok(rate_limited_ssr_response(
            _retry,
            "Quá nhiều yêu cầu tìm kiếm, vui lòng thử lại sau.",
        ));
    }

    let active = match themes_root {
        Some(Extension(root)) => theme_storage::load_active_theme_v3(db, &root.0).await?,
        None => None,
    };
    let site = settings::get_general_for_locale(db, Some(locale)).await?;
    let pub_settings = settings::get_public(db).await?;
    let mut richtext_nonces = crate::template_sec::RichTextNonceTable::default();
    let (site_opts, opts) = settings::get_active_public_theme_options_for_locale_with_nonces(
        db,
        Some(locale),
        Some(&mut richtext_nonces),
    )
    .await?;
    let (resolved_locale, bcp47_tag, dir) =
        settings::get_locale_bcp47_and_dir(db, Some(locale)).await?;
    let output = query_public_search(
        db,
        &PublicSearchQueryInput {
            raw_query: Some(&parsed.query),
            locale: Some(&resolved_locale),
            content_type: parsed.content_type.as_deref(),
            page: parsed.page,
            per_page: SEARCH_PAGE_SIZE,
        },
    )
    .await?;

    let raw_for_alt = (!output.query.is_empty()).then_some(output.query.as_str());
    let locale_ctx = seo::build_theme_locale_context(
        db,
        Some(&resolved_locale),
        seo::ThemeLocaleTarget::Search {
            raw_search_query: raw_for_alt,
        },
    )
    .await?;
    let search_url = locale_ctx
        .get("search_url")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("/")
        .to_owned();
    let home_url = locale_ctx
        .get("home_url")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("/")
        .to_owned();

    let search_view = SearchView::from_public_output(&output, &search_url);
    let query_arg = if output.has_query {
        output.query.as_str()
    } else {
        ""
    };
    let total_pages = if output.total_items == 0 {
        0
    } else {
        (((output.total_items - 1) / output.per_page.max(1)) + 1).min(MAX_SEARCH_PAGE)
    };
    let previous_url = (output.page > 1 && total_pages > 0).then(|| {
        build_search_url(
            &search_url,
            Some(query_arg),
            output.content_type.as_deref(),
            output.page - 1,
        )
    });
    let next_url = (output.page < total_pages).then(|| {
        build_search_url(
            &search_url,
            Some(query_arg),
            output.content_type.as_deref(),
            output.page + 1,
        )
    });
    let pagination = PaginationView::compute_with_max_pages(
        output.page,
        output.per_page,
        output.total_items,
        MAX_SEARCH_PAGE,
        previous_url.clone(),
        next_url.clone(),
        |p| {
            Some(build_search_url(
                &search_url,
                Some(query_arg),
                output.content_type.as_deref(),
                p,
            ))
        },
    );
    let entries: Vec<EntryView> = output.hits.into_iter().map(|hit| hit.view).collect();
    let site_ctx = SiteContext {
        title: &site.site_title,
        tagline: &site.tagline,
        timezone: &site.timezone,
        origin: normalized_origin(pub_settings.public_origin.as_deref()),
        current_year: content_render::current_year_in_timezone(&site.timezone),
    };
    let fmt_cfg = crate::template_sec::RenderFormattingConfig {
        timezone: site.timezone.clone(),
        ..Default::default()
    };
    let mut budget = overrides::PublicCompositionBudget::default();
    budget.render_budget.richtext_nonces.extend(richtext_nonces);
    let custom_opts = crate::storage::options_pages::load_active_custom_options_public(
        db,
        Some(&resolved_locale),
        false,
        Some(&mut budget.render_budget.richtext_nonces),
    )
    .await?;
    budget.set_locale_context(active.as_ref(), &resolved_locale, locale_ctx);

    let l10n = content_render::core_l10n(&resolved_locale);
    let breadcrumbs =
        content_render::search_breadcrumbs(&l10n.home_label, Some(&home_url), &l10n.search_title);
    let fallback_html = content_render::render_default_search_html(
        &site_ctx,
        &search_view,
        &entries,
        &pagination,
        &breadcrumbs,
        &resolved_locale,
    );
    let candidates = overrides::build_candidates(&PublicTemplateContext::Search)?;
    let cache = crate::page::BlockTemplateCache::global();
    let body = resolve_public_template_html(
        db,
        &candidates,
        active.as_ref(),
        &site_ctx,
        &site_opts,
        &opts,
        Some(&resolved_locale),
        &fallback_html,
        None,
        PublicRouteContext {
            route_kind: "search",
            route_path: Some(&search_url),
            entry_id: None,
            entry_type_key: None,
            custom_options: &custom_opts,
            breadcrumbs: &breadcrumbs,
            pagination: &pagination,
            archive_url: None,
            previous_url: previous_url.as_deref(),
            next_url: next_url.as_deref(),
            entries: &entries,
            taxonomy: None,
            term: None,
            term_schema: None,
            content_type: None,
            entry: None,
            list_pages: None,
            search: &search_view,
        },
        &mut budget,
        cache,
    )
    .await?;

    let (header_html, footer_html, shell_has_form) =
        if body.canvas_layout || body.theme_context.is_null() {
            (None, None, false)
        } else {
            overrides::resolve_and_render_header_footer_with_context_for_locale(
                db,
                active.as_ref(),
                &body.theme_context,
                &body.precompiled_shell,
                Some(&resolved_locale),
                &fmt_cfg,
                &mut budget,
                cache,
            )
            .await?
        };
    let page_title = if search_view.has_query {
        format!(
            "{} — {} | {}",
            l10n.search_title, search_view.query, site.site_title
        )
    } else {
        format!("{} | {}", l10n.search_title, site.site_title)
    };
    let seo_view = crate::seo::SeoView::for_search_page(&bcp47_tag, &dir, &page_title);
    let enqueued = enqueued_for_route(active.as_ref(), EnqueueCondition::Search);
    let composed = content_render::compose_document_with_seo_with_assets(
        header_html.as_deref(),
        &body.html,
        footer_html.as_deref(),
        &seo_view,
        &["od-route-search"],
        &enqueued,
        active.is_some(),
    )?;
    let composed = apply_public_render_hooks(
        composed,
        services,
        "search",
        &search_url,
        raw_query.unwrap_or(""),
        false,
        &resolved_locale,
        &body.theme_context,
        &budget,
    )
    .await?;
    let html = content_render::finalize_document_richtext(
        content_render::finalize_document_icons(composed, &budget.render_budget.icon_nonces)?,
        &budget.render_budget.richtext_nonces,
    )?;
    let mut response = crate::routes::public::rendered_layout_with_form_csp(
        html,
        body.has_active_contact_form || shell_has_form,
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::HeaderName::from_static("x-robots-tag"),
        HeaderValue::from_static("noindex, follow"),
    );
    Ok(response)
}
