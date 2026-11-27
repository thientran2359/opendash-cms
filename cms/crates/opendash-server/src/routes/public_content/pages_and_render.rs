use super::*;

pub(super) fn normalized_origin(public_origin: Option<&str>) -> &str {
    public_origin
        .map(|s| s.trim().trim_end_matches('/'))
        .unwrap_or("")
}

pub(crate) fn extract_origin_host(headers: &HeaderMap) -> String {
    headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_ascii_lowercase())
        .unwrap_or_default()
}

fn strip_media_provenance(val: serde_json::Value) -> serde_json::Value {
    match val {
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(strip_media_provenance).collect())
        }
        serde_json::Value::Object(mut map) => {
            map.remove(crate::template_sec::MEDIA_PROVENANCE_KEY);
            for v in map.values_mut() {
                let taken = std::mem::replace(v, serde_json::Value::Null);
                *v = strip_media_provenance(taken);
            }
            serde_json::Value::Object(map)
        }
        other => other,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn apply_public_render_hooks(
    html: String,
    services: Option<&PublicRenderServices>,
    route_kind: &str,
    route_path: &str,
    canonical_query: &str,
    is_not_found: bool,
    locale: &str,
    theme_context: &serde_json::Value,
    budget: &overrides::PublicCompositionBudget,
) -> ApiResult<String> {
    let slots = budget.render_budget.hook_nonces.entries();
    let Some(services) = services else {
        if slots.is_empty() {
            return Ok(html);
        }
        let cleared: Vec<(String, String)> = slots
            .iter()
            .map(|s| (s.nonce.clone(), String::new()))
            .collect();
        return content_render::apply_render_hooks_to_document(html, "", &[], &[], "", &cleared);
    };
    let ctx = crate::hooks::RenderHookContext {
        route_kind,
        route_path,
        canonical_query,
        is_not_found,
        locale,
        origin_host: &services.origin_host,
        site: strip_media_provenance(
            theme_context
                .get("site")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        ),
        options: strip_media_provenance(
            theme_context
                .get("options")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        ),
        entry: theme_context
            .get("entry")
            .cloned()
            .map(strip_media_provenance),
        entries: theme_context
            .get("entries")
            .cloned()
            .map(strip_media_provenance),
        principal: opendash_core::hooks::ExecutionPrincipal::Public,
    };
    let output = services
        .hooks
        .dispatch_render_hooks(&ctx, slots, &services.render_cache)
        .await;
    content_render::apply_render_hooks_to_document(
        html,
        &output.head_html,
        &output.head_styles,
        &output.head_scripts,
        &output.footer_html,
        &output.slot_replacements,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn resolve_public_template_html(
    db: &SqlitePool,
    candidates: &[opendash_core::theme::TemplateCandidate],
    active: Option<&theme_storage::ActiveThemeV3>,
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
    let resolution = overrides::resolve_template_2axis(
        db,
        candidates,
        active.map(|v| &v.templates),
        active.map(|v| &v.parts),
        active.map(|v| &v.page_layouts),
    )
    .await?;
    render_resolved_public_template_html(
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
        budget,
        cache,
    )
    .await
}

pub(super) fn permanent_redirect(location: String) -> Response {
    (
        StatusCode::MOVED_PERMANENTLY,
        [(header::LOCATION, location)],
        "",
    )
        .into_response()
}

#[allow(dead_code)]
pub(crate) async fn render_single_entry_response(
    db: &SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    entry: crate::storage::entries::ContentEntryRecord,
    canonical_path_override: Option<String>,
) -> ApiResult<Response> {
    render_single_entry_response_with_query(
        db,
        themes_root,
        entry,
        canonical_path_override,
        None,
        None,
    )
    .await
}

pub(crate) async fn render_single_entry_response_with_query(
    db: &SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    entry: crate::storage::entries::ContentEntryRecord,
    canonical_path_override: Option<String>,
    raw_query: Option<&str>,
    services: Option<&PublicRenderServices>,
) -> ApiResult<Response> {
    render_entry_response_with_cache(
        db,
        themes_root,
        entry,
        canonical_path_override,
        false,
        raw_query,
        crate::page::BlockTemplateCache::global(),
        services,
    )
    .await
}

pub(crate) async fn render_homepage_entry_response_for_locale(
    db: &SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    entry: crate::storage::entries::ContentEntryRecord,
    canonical_path: String,
    raw_query: Option<&str>,
    services: Option<&PublicRenderServices>,
) -> ApiResult<Response> {
    if entry.content_type_key != "page" {
        return render_not_found_response_for_locale(db, themes_root, None, services).await;
    }
    render_entry_response_with_cache(
        db,
        themes_root,
        entry,
        Some(canonical_path),
        true,
        raw_query,
        crate::page::BlockTemplateCache::global(),
        services,
    )
    .await
}

pub(super) async fn require_term_kind(
    db: &SqlitePool,
    term_id: i64,
    expected: Option<TaxonomyKind>,
) -> ApiResult<String> {
    // The storage ACL helper hides private/withdrawn/provider-inactive terms.
    // Routes must not issue SQL or infer visibility from a bare term row.
    if public_content::canonical_term_by_id(db, term_id)
        .await?
        .is_none()
    {
        return Err(ApiError::not_found("Không tìm thấy taxonomy."));
    }
    let term = crate::storage::taxonomy::get_term(db, term_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy taxonomy."))?;
    let kind = term.kind;
    if expected.is_some_and(|requested| requested.as_str() != kind) {
        return Err(ApiError::not_found("Không tìm thấy taxonomy."));
    }
    Ok(kind)
}

/// Redirect a historical term path directly to its current canonical URL,
/// retaining only the typed pagination/filter query (never untrusted keys).
pub(crate) async fn redirect_canonical_taxonomy(
    db: &SqlitePool,
    term_id: i64,
    canonical_base: String,
    raw_query: Option<&str>,
) -> ApiResult<Response> {
    let kind = require_term_kind(db, term_id, None).await?;
    let scope = QueryScope::TaxonomyArchive(if kind == "tag" {
        TaxonomyKind::Tag
    } else {
        TaxonomyKind::Category
    });
    let query = parse_query(raw_query, scope).map_err(ApiError::bad_request)?;
    Ok(permanent_redirect(CanonicalUrlBuilder::with_query(
        canonical_base,
        query,
        scope,
    )))
}
