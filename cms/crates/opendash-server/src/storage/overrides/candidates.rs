//! Site-owned typed templates. Candidate specificity wins before source priority:
//! for each candidate, site-owned Builder AST precedes active theme v3 HTML.

use super::*;

pub(super) use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};

pub(super) use opendash_core::{
    builder::{BuilderDocument, ElementContent, KnownElement},
    content::{
        ContentSupport, ContentTypeDefinition, EffectiveContentSchema, FieldRole,
        valid_content_type_key,
    },
    registry::{DeclarationState, ProviderStatus},
    theme::{
        EnqueueCondition, PageLayoutChrome, ResolvedEnqueuedAssets, TemplateCandidate,
        TemplateCandidateKind, resolve_enqueued_assets,
    },
};

pub(super) use serde::{Deserialize, Serialize};

pub(super) use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

pub(super) use crate::{
    auth::{Role, SessionContext},
    content_render::{self, EntryView, PaginationView, PublicThemeContextInput, SiteContext},
    error::{ApiError, ApiResult},
    page::{self, RenderAnnotationScope, RenderedLayout},
    storage::{
        builder::{self, BuilderRenderContext, check_preview_images, resolve_contact_forms_in_tx},
        builder_binding, content, entries, menu, permalinks, public_content, settings, theme,
        theme::ActiveThemeV3,
    },
    template_sec::{self, PreviewCompositionBudget},
};

/// The route must authenticate publication, provider visibility, archive
/// eligibility and assigned template eligibility before constructing context.
#[derive(Debug, Clone)]
pub enum PublicTemplateContext {
    Home {
        page_layout_key: Option<String>,
    },
    Page {
        page_layout_key: Option<String>,
    },
    Single {
        content_type: String,
        entry_id: i64,
        slug: String,
        custom_template_key: Option<String>,
    },
    Archive {
        content_type: String,
    },
    Taxonomy {
        taxonomy: String,
        term_id: i64,
        slug: String,
    },
    NotFound,
    Part {
        part_key: String,
    },
}

pub(crate) fn is_canvas_page_layout(
    active: Option<&ActiveThemeV3>,
    layout_key: Option<&str>,
) -> bool {
    let Some(layout_key) = layout_key else {
        return false;
    };
    active
        .and_then(|theme| {
            theme
                .manifest
                .page_layouts
                .iter()
                .find(|decl| decl.key == layout_key)
        })
        .map_or(layout_key == "canvas", |decl| {
            decl.chrome == PageLayoutChrome::None
        })
}

pub(in crate::storage) fn is_canvas_target(
    active: Option<&ActiveThemeV3>,
    target: &TemplateCandidate,
) -> bool {
    target.kind == TemplateCandidateKind::PageLayout
        && is_canvas_page_layout(active, target.layout_key.as_deref())
}

pub(in crate::storage) fn is_canvas_resolved_key(
    active: Option<&ActiveThemeV3>,
    target_key: &str,
) -> bool {
    is_canvas_page_layout(active, target_key.strip_prefix("page_layout:"))
}

pub fn build_candidates(context: &PublicTemplateContext) -> ApiResult<Vec<TemplateCandidate>> {
    use TemplateCandidateKind as K;
    let mut result = Vec::new();
    match context {
        PublicTemplateContext::Home { page_layout_key } => {
            result.push(candidate(K::Home));
            if let Some(layout_key) = page_layout_key {
                let mut layout = candidate(K::PageLayout);
                layout.layout_key = Some(layout_key.clone());
                result.push(layout);
            }
            result.push(candidate(K::Page));
            result.push(candidate(K::Index));
        }
        PublicTemplateContext::Page { page_layout_key } => {
            if let Some(layout_key) = page_layout_key {
                let mut layout = candidate(K::PageLayout);
                layout.layout_key = Some(layout_key.clone());
                result.push(layout);
            }
            result.push(candidate(K::Page));
            result.push(candidate(K::Index));
        }
        PublicTemplateContext::Single {
            content_type,
            entry_id,
            slug,
            custom_template_key,
        } => {
            if let Some(custom_key) = custom_template_key {
                let mut custom = candidate(K::SingleCustom);
                custom.custom_key = Some(custom_key.clone());
                result.push(custom);
            }
            let mut specific = candidate(K::Single);
            specific.content_type = Some(content_type.clone());
            specific.entry_id = Some(*entry_id);
            result.push(specific);
            let mut slugged = candidate(K::Single);
            slugged.content_type = Some(content_type.clone());
            slugged.selector_slug = Some(slug.clone());
            result.push(slugged);
            let mut typed = candidate(K::Single);
            typed.content_type = Some(content_type.clone());
            result.push(typed);
            result.push(candidate(K::Single));
            result.push(candidate(K::Singular));
            result.push(candidate(K::Index));
        }
        PublicTemplateContext::Archive { content_type } => {
            let mut typed = candidate(K::Archive);
            typed.content_type = Some(content_type.clone());
            result.push(typed);
            result.push(candidate(K::Archive));
            result.push(candidate(K::Index));
        }
        PublicTemplateContext::Taxonomy {
            taxonomy,
            term_id,
            slug,
        } => {
            let mut specific = candidate(K::Taxonomy);
            specific.taxonomy = Some(taxonomy.clone());
            specific.term_id = Some(*term_id);
            result.push(specific);
            let mut slugged = candidate(K::Taxonomy);
            slugged.taxonomy = Some(taxonomy.clone());
            slugged.selector_slug = Some(slug.clone());
            result.push(slugged);
            let mut typed = candidate(K::Taxonomy);
            typed.taxonomy = Some(taxonomy.clone());
            result.push(typed);
            result.push(candidate(K::Taxonomy));
            result.push(candidate(K::Archive));
            result.push(candidate(K::Index));
        }
        PublicTemplateContext::NotFound => {
            result.push(candidate(K::NotFound));
            result.push(candidate(K::Index));
        }
        PublicTemplateContext::Part { part_key } => {
            let mut part = candidate(K::Part);
            part.part_key = Some(part_key.clone());
            result.push(part);
        }
    }
    for item in &result {
        item.canonical_key().map_err(ApiError::bad_request)?;
    }
    Ok(result)
}

/// For the Admin reset API, reconstruct the fallback chain from a stored
/// target. ID-scoped targets recover the saved slug from SQLite so resetting
/// an ID override can correctly reveal a more specific slug theme template.
/// Callers with a fully authorized public route context may instead pass
/// `build_candidates` for that context to `reset_typed_override`.
pub async fn build_reset_candidate_chain(
    db: &SqlitePool,
    target: &TemplateCandidate,
) -> ApiResult<Vec<TemplateCandidate>> {
    let mut conn = db.acquire().await?;
    build_reset_candidate_chain_in_conn(&mut conn, target).await
}

pub(in crate::storage) async fn build_reset_candidate_chain_for_locale_in_conn(
    conn: &mut SqliteConnection,
    target: &TemplateCandidate,
    locale: Option<&str>,
) -> ApiResult<Vec<TemplateCandidate>> {
    use TemplateCandidateKind as K;
    let mut candidates = vec![target.clone()];
    match target.kind {
        K::Home => {
            let effective_locale = settings::resolve_requested_locale_in_conn(conn, locale).await?;
            let layout: Option<String> = sqlx::query_scalar(
                "SELECT e.page_layout_key FROM site_reading_settings AS reading
                   JOIN content_entries AS e ON e.id = reading.homepage_page_id
                  WHERE reading.locale = ?
                    AND e.content_type_key='page'
                    AND e.locale = reading.locale
                    AND e.trashed_at IS NULL
                    AND e.publication_status='published'",
            )
            .bind(&effective_locale)
            .fetch_optional(&mut *conn)
            .await?
            .flatten();
            if let Some(layout_key) = layout {
                let mut layout_candidate = candidate(K::PageLayout);
                layout_candidate.layout_key = Some(layout_key);
                candidates.push(layout_candidate);
            }
            candidates.push(candidate(K::Page));
            candidates.push(candidate(K::Index));
        }
        K::PageLayout => {
            candidates.push(candidate(K::Page));
            candidates.push(candidate(K::Index));
        }
        K::Page => candidates.push(candidate(K::Index)),
        K::Single => {
            if let (Some(entry_id), Some(content_type)) =
                (target.entry_id, target.content_type.as_deref())
            {
                let slug: Option<String> = sqlx::query_scalar(
                    "SELECT slug FROM content_entries WHERE id=? AND content_type_key=? AND trashed_at IS NULL",
                )
                .bind(entry_id)
                .bind(content_type)
                .fetch_optional(&mut *conn)
                .await?;
                if let Some(selector_slug) = slug {
                    let mut slugged = candidate(K::Single);
                    slugged.content_type = Some(content_type.to_owned());
                    slugged.selector_slug = Some(selector_slug);
                    candidates.push(slugged);
                }
            }
            if let Some(content_type) = &target.content_type {
                let mut typed = candidate(K::Single);
                typed.content_type = Some(content_type.clone());
                candidates.push(typed);
            }
            candidates.push(candidate(K::Single));
            candidates.push(candidate(K::Singular));
            candidates.push(candidate(K::Index));
        }
        K::SingleCustom => {
            candidates.push(candidate(K::Single));
            candidates.push(candidate(K::Singular));
            candidates.push(candidate(K::Index));
        }
        K::Singular => candidates.push(candidate(K::Index)),
        K::Archive => {
            candidates.push(candidate(K::Archive));
            candidates.push(candidate(K::Index));
        }
        K::Taxonomy => {
            if let (Some(term_id), Some(taxonomy)) = (target.term_id, target.taxonomy.as_deref()) {
                let slug: Option<String> =
                    sqlx::query_scalar("SELECT slug FROM taxonomy_terms WHERE id=? AND kind=?")
                        .bind(term_id)
                        .bind(taxonomy)
                        .fetch_optional(&mut *conn)
                        .await?;
                if let Some(selector_slug) = slug {
                    let mut slugged = candidate(K::Taxonomy);
                    slugged.taxonomy = Some(taxonomy.to_owned());
                    slugged.selector_slug = Some(selector_slug);
                    candidates.push(slugged);
                }
            }
            if let Some(taxonomy) = &target.taxonomy {
                let mut typed = candidate(K::Taxonomy);
                typed.taxonomy = Some(taxonomy.clone());
                candidates.push(typed);
            }
            candidates.push(candidate(K::Taxonomy));
            candidates.push(candidate(K::Archive));
            candidates.push(candidate(K::Index));
        }
        K::NotFound => candidates.push(candidate(K::Index)),
        K::Index | K::Part => {}
    }
    let mut seen = std::collections::BTreeSet::new();
    candidates.retain(|candidate| match candidate.canonical_key() {
        Ok(key) => seen.insert(key),
        // Keep malformed candidates for the explicit error pass below.
        Err(_) => true,
    });
    for candidate in &candidates {
        candidate.canonical_key().map_err(ApiError::bad_request)?;
    }
    Ok(candidates)
}

pub(in crate::storage) fn template_kind_str(kind: TemplateCandidateKind) -> &'static str {
    use TemplateCandidateKind as K;
    match kind {
        K::Home => "home",
        K::Page => "page",
        K::PageLayout => "page_layout",
        K::Single => "single",
        K::SingleCustom => "single_custom",
        K::Archive => "archive",
        K::Taxonomy => "taxonomy",
        K::Singular => "singular",
        K::Index => "index",
        K::NotFound => "not_found",
        K::Part => "part",
    }
}

pub(in crate::storage) fn document_parts_and_slot_count_with_schemas(
    document: &BuilderDocument,
    schemas: &opendash_core::registry::RegistryCatalog<
        opendash_core::block_schema::RegisteredBlockSchema,
    >,
) -> ApiResult<(Vec<String>, usize)> {
    use opendash_core::registry::{DeclarationState, ProviderStatus};
    let mut parts = Vec::new();
    let mut slots = 0;
    for section in &document.sections {
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    match &element.content {
                        ElementContent::Known(KnownElement::ContentSlot(_)) => slots += 1,
                        ElementContent::Known(KnownElement::TemplatePart(props)) => {
                            parts.push(props.part_key.clone());
                        }
                        ElementContent::Known(KnownElement::PluginBlock(props)) => {
                            if let Some(entry) = schemas.get(&props.block_id)
                                && entry.status == ProviderStatus::Active
                                && entry.declaration_state == DeclarationState::Declared
                            {
                                parts.extend(
                                    entry
                                        .descriptor
                                        .schema
                                        .collect_template_part_keys(&props.values),
                                );
                            }
                        }
                        ElementContent::Unresolved(value)
                            if matches!(
                                value.get("type").and_then(serde_json::Value::as_str),
                                Some("core/content_slot" | "core/template_part")
                            ) =>
                        {
                            return Err(ApiError::bad_request(
                                "Malformed content slot or template part block.",
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    Ok((parts, slots))
}

pub(in crate::storage) fn validate_content_slots(
    target: &TemplateCandidate,
    slots: usize,
) -> ApiResult<()> {
    use TemplateCandidateKind as K;
    let valid = match target.kind {
        K::Home | K::Page | K::PageLayout | K::Single | K::SingleCustom | K::Singular => slots == 1,
        K::Index => slots <= 1,
        K::Archive | K::Taxonomy | K::NotFound | K::Part => slots == 0,
    };
    if valid {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "Invalid content slot count for template target.",
        ))
    }
}

/// The same AST and route-binding validation is used by Save and read-only Preview.
pub(in crate::storage) async fn validate_typed_document_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    target: &TemplateCandidate,
    document: &BuilderDocument,
    prior: Option<&BuilderDocument>,
) -> ApiResult<BuilderRenderContext> {
    use opendash_core::registry::{DeclarationState, ProviderStatus};
    let scoped_fields = validate_typed_target_in_tx(tx, target).await?;
    let state = builder::read_render_context_tx(tx).await?;
    let schemas = state.unified_block_schemas()?;
    document
        .validate_for_save_with_schemas(&schemas, prior)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    // S02 stores synchronized Pattern references per content entry through
    // entry_pattern_refs. Shared template overrides have no such ownership or
    // public-media projection, so accepting a PatternRef here would silently
    // hide its markup on public pages. Keep these blocks entry-scoped.
    let has_block_pattern = document.sections.iter().any(|s| {
        s.children.iter().any(|r| {
            r.children.iter().any(|c| {
                c.children.iter().any(|el| {
                    if let ElementContent::Known(KnownElement::PluginBlock(props)) = &el.content
                        && let Some(entry) = schemas.get(&props.block_id)
                        && entry.status == ProviderStatus::Active
                        && entry.declaration_state == DeclarationState::Declared
                    {
                        entry
                            .descriptor
                            .schema
                            .has_non_empty_pattern_source_value(&props.values)
                    } else {
                        false
                    }
                })
            })
        })
    });
    if !builder::pattern_ids_with_schemas(document, &schemas).is_empty() || has_block_pattern {
        return Err(ApiError::bad_request(
            "Reusable Pattern chỉ hỗ trợ Builder của từng entry/Page.",
        ));
    }
    let route_role = match target.kind {
        TemplateCandidateKind::Single
        | TemplateCandidateKind::SingleCustom
        | TemplateCandidateKind::Singular => Some("single"),
        TemplateCandidateKind::Home
        | TemplateCandidateKind::Page
        | TemplateCandidateKind::PageLayout => Some("page"),
        _ => None,
    };
    document
        .validate_route_context_for_role(route_role, scoped_fields.as_deref())
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let (_, slots) = document_parts_and_slot_count_with_schemas(document, &schemas)?;
    validate_content_slots(target, slots)?;
    crate::storage::fields::validate_document_block_refs_in_conn(
        &mut **tx,
        document,
        prior,
        &schemas,
        None,
        scoped_fields.as_deref(),
    )
    .await?;
    builder::validate_media_refs_in_tx(&mut **tx, document).await?;
    Ok(state)
}

pub(in crate::storage) async fn render_preview_part(
    tx: &mut Transaction<'_, Sqlite>,
    key: &str,
    ctx: &PreviewRenderContext<'_>,
    budget: &mut PreviewCompositionBudget,
) -> ApiResult<Option<RenderedLayout>> {
    match ctx.graph.get(key) {
        Some(PreviewPartSource::Builder(doc)) => {
            let scope = part_scope_prefix(key, budget.expansions);
            Ok(Some(
                Box::pin(render_preview_builder(
                    tx,
                    doc,
                    None,
                    ctx,
                    budget,
                    RenderAnnotationScope::None,
                    &scope,
                ))
                .await?,
            ))
        }
        Some(PreviewPartSource::Theme(source)) => {
            // Flatten only contiguous Theme-to-Theme edges. Builder boundaries
            // use the server snapshot rather than the surrounding lexical scope.
            let mut theme_sources = BTreeMap::new();
            let mut boundaries = BTreeSet::new();
            let mut todo =
                template_sec::preview_csec_part_keys_with_theme_urls(key, source, ctx.url_policy)
                    .map_err(ApiError::bad_request)?;
            while let Some(child) = todo.pop() {
                match ctx.graph.get(&child) {
                    Some(PreviewPartSource::Theme(child_source)) => {
                        if !theme_sources.contains_key(&child) {
                            todo.extend(
                                template_sec::preview_csec_part_keys_with_theme_urls(
                                    &child,
                                    child_source,
                                    ctx.url_policy,
                                )
                                .map_err(ApiError::bad_request)?,
                            );
                            theme_sources.insert(child, child_source.clone());
                        }
                    }
                    _ => {
                        boundaries.insert(child);
                    }
                }
            }
            let mut trusted = BTreeMap::new();
            let mut diagnostics = Vec::new();
            for child in boundaries {
                if let Some(mut rendered) =
                    Box::pin(render_preview_part(tx, &child, ctx, budget)).await?
                {
                    trusted.insert(child, rendered.html);
                    diagnostics.append(&mut rendered.diagnostics);
                } else {
                    diagnostics.push(page::BuilderDiagnostic {
                        node_id: child.clone(),
                        code: "template_part_unavailable",
                    });
                    trusted.insert(child, String::new());
                }
            }
            let html = template_sec::render_preview_csec_fragment_with_theme_urls_and_config(
                key,
                source,
                template_sec::json_to_safe_value(ctx.theme_context),
                &theme_sources,
                &trusted,
                budget,
                ctx.url_policy,
                ctx.fmt_cfg,
            )
            .map_err(ApiError::bad_request)?;
            Ok(Some(RenderedLayout {
                html,
                diagnostics,
                has_active_contact_form: false,
            }))
        }
        _ => Ok(None),
    }
}

pub(in crate::storage) fn build_placeholder_preview_theme_context(
    target: &TemplateCandidate,
    active: Option<&ActiveThemeV3>,
    site: &SiteContext<'_>,
    site_options: &BTreeMap<String, serde_json::Value>,
    options: &BTreeMap<String, serde_json::Value>,
    menus: &BTreeMap<String, menu::MenuViewModel>,
) -> ApiResult<serde_json::Value> {
    use TemplateCandidateKind as K;
    let route_kind = route_kind_for_candidate(target.kind);
    let theme_assets = active.map(ActiveThemeV3::asset_url_map).unwrap_or_default();
    let empty_entries = Vec::new();
    let pagination = PaginationView::empty();

    let archive_url: Option<&str> = match target.kind {
        K::Single | K::SingleCustom | K::Singular | K::Index | K::Archive => Some("/posts"),
        K::Home | K::Page | K::PageLayout | K::Taxonomy | K::NotFound | K::Part => None,
    };

    let placeholder_entry = matches!(
        target.kind,
        K::Home | K::Page | K::PageLayout | K::Single | K::SingleCustom | K::Singular | K::Index
    )
    .then(|| {
        let ct_key = target.content_type.as_deref().unwrap_or(
            if matches!(target.kind, K::Home | K::Page | K::PageLayout) {
                "page"
            } else {
                "post"
            },
        );
        build_placeholder_preview_entry(ct_key)
    });

    let ct_value: Option<serde_json::Value> = if matches!(
        target.kind,
        K::Home | K::Page | K::PageLayout | K::Single | K::SingleCustom | K::Singular | K::Index
    ) {
        let ct_key = target.content_type.as_deref().unwrap_or(
            if matches!(target.kind, K::Home | K::Page | K::PageLayout) {
                "page"
            } else {
                "post"
            },
        );
        let label = if matches!(target.kind, K::Home | K::Page | K::PageLayout) {
            "Trang"
        } else {
            "Bài viết"
        };
        Some(serde_json::json!({ "key": ct_key, "label": label }))
    } else if target.kind == K::Archive {
        let ct_key = target.content_type.as_deref().unwrap_or("post");
        Some(serde_json::json!({ "key": ct_key, "label": ct_key }))
    } else {
        None
    };

    let taxonomy_value: Option<serde_json::Value> = if target.kind == K::Taxonomy {
        let tax_key = target.taxonomy.as_deref().unwrap_or("category");
        Some(serde_json::json!({
            "key": tax_key,
            "id": 0,
            "slug": "sample-term",
            "name": "Chuyên mục mẫu",
        }))
    } else {
        None
    };

    let breadcrumbs = match target.kind {
        K::Home => content_render::home_breadcrumbs(
            content_render::DEFAULT_BREADCRUMB_HOME_LABEL,
            Some(content_render::DEFAULT_BREADCRUMB_HOME_URL),
        ),
        K::Archive => {
            let ct_key = target.content_type.as_deref().unwrap_or("post");
            content_render::archive_breadcrumbs(
                content_render::DEFAULT_BREADCRUMB_HOME_LABEL,
                Some(content_render::DEFAULT_BREADCRUMB_HOME_URL),
                ct_key,
                archive_url.map(str::to_owned),
            )
        }
        K::Taxonomy => content_render::taxonomy_breadcrumbs(
            content_render::DEFAULT_BREADCRUMB_HOME_LABEL,
            Some(content_render::DEFAULT_BREADCRUMB_HOME_URL),
            Vec::new(),
            "Chuyên mục mẫu",
            None,
        ),
        K::NotFound => content_render::not_found_breadcrumbs(
            content_render::DEFAULT_BREADCRUMB_HOME_LABEL,
            Some(content_render::DEFAULT_BREADCRUMB_HOME_URL),
            content_render::DEFAULT_BREADCRUMB_NOT_FOUND_LABEL,
        ),
        K::Part => Vec::new(),
        _ => {
            let ct_key = target.content_type.as_deref().unwrap_or("page");
            content_render::single_breadcrumbs(
                content_render::DEFAULT_BREADCRUMB_HOME_LABEL,
                Some(content_render::DEFAULT_BREADCRUMB_HOME_URL),
                ct_key,
                if matches!(target.kind, K::Page | K::PageLayout) {
                    "Trang"
                } else {
                    "Bài viết"
                },
                archive_url.is_some(),
                archive_url.map(str::to_owned),
                None,
                placeholder_entry
                    .as_ref()
                    .map(|e| e.title.as_str())
                    .unwrap_or("Nội dung xem trước"),
                placeholder_entry.as_ref().and_then(|e| e.url.clone()),
            )
        }
    };

    content_render::build_public_theme_context(PublicThemeContextInput {
        site,
        site_options,
        options,
        theme_assets: &theme_assets,
        menus,
        route_kind,
        route_path: None,
        breadcrumbs: &breadcrumbs,
        pagination: &pagination,
        archive_url,
        previous_url: None,
        next_url: None,
        entries: &empty_entries,
        related_entries: &empty_entries,
        recent_posts: &empty_entries,
        taxonomy: taxonomy_value.as_ref(),
        content_type: ct_value.as_ref(),
        entry: placeholder_entry.as_ref(),
    })
}

pub(in crate::storage) async fn resolve_preview_entry(
    tx: &mut Transaction<'_, Sqlite>,
    target: &TemplateCandidate,
    sample: Option<&TemplatePreviewEntry>,
    active: Option<&ActiveThemeV3>,
    preview_locale: &str,
    actor_role: FieldRole,
) -> ApiResult<Option<(entries::ContentEntryRecord, EffectiveContentSchema)>> {
    use TemplateCandidateKind as K;
    let selected = if let Some(sample) = sample {
        if sample.entry_id <= 0 || !valid_content_type_key(&sample.type_key) {
            return Err(ApiError::bad_request("Invalid preview entry."));
        }
        Some((sample.type_key.clone(), sample.entry_id))
    } else if target.kind == K::Single {
        match (&target.content_type, target.entry_id, &target.selector_slug) {
            (Some(t), Some(id), _) => Some((t.clone(), id)),
            (Some(t), None, Some(slug)) => {
                let id: i64 = sqlx::query_scalar(
                    "SELECT id FROM content_entries WHERE content_type_key = ? AND slug = ? AND locale = ? AND trashed_at IS NULL",
                )
                .bind(t)
                .bind(slug)
                .bind(preview_locale)
                .fetch_optional(&mut **tx)
                .await?
                .ok_or_else(|| ApiError::not_found("Preview entry not found."))?;
                Some((t.clone(), id))
            }
            _ => None,
        }
    } else {
        None
    };
    let Some((type_key, id)) = selected else {
        return Ok(None);
    };
    if type_key == "pattern" {
        return Err(ApiError::not_found("Preview entry not found."));
    }
    let valid = match target.kind {
        K::Home | K::Page | K::PageLayout => type_key == "page",
        K::Single => {
            type_key != "page"
                && target.content_type.as_ref().is_none_or(|t| t == &type_key)
                && target.entry_id.is_none_or(|target_id| target_id == id)
        }
        K::SingleCustom => {
            type_key != "page"
                && active
                    .and_then(|a| {
                        a.manifest.templates.iter().find(|d| {
                            d.kind == K::SingleCustom && d.custom_key == target.custom_key
                        })
                    })
                    .is_none_or(|d| d.allowed_types.contains(&type_key))
        }
        K::Singular => type_key != "page",
        K::Index => true,
        _ => false,
    };
    if !valid {
        return Err(ApiError::bad_request(
            "Preview entry does not match template target.",
        ));
    }
    let mut entry = entries::get_entry_in_tx(tx, &type_key, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Preview entry not found."))?;
    if target.kind == K::Single
        && target
            .selector_slug
            .as_ref()
            .is_some_and(|s| s != &entry.slug)
    {
        return Err(ApiError::bad_request(
            "Preview entry slug does not match template target.",
        ));
    }
    let schema = content::load_effective_content_schema_in_tx(tx, &type_key)
        .await?
        .filter(|s| {
            s.base.status == ProviderStatus::Active
                && s.base.declaration_state == DeclarationState::Declared
        })
        .ok_or_else(|| ApiError::not_found("Preview entry type not available."))?;
    entries::filter_entry_values_for_role(&mut entry, &schema, actor_role);
    Ok(Some((entry, schema)))
}

pub async fn inspect_public_surfaces_and_precompile(
    db: &SqlitePool,
    resolution: &TemplateResolution,
    active: Option<&ActiveThemeV3>,
    url_policy: &template_sec::ThemeUrlPolicy,
) -> ApiResult<PublicSurfaceInspection> {
    let mut tx = db.begin().await?;
    let schemas = crate::storage::page::load_unified_block_catalog_in_conn(&mut *tx).await?;
    let mut precompiled_body = None;
    let mut precompiled_shell = PrecompiledPublicShell::default();
    let mut uses_related_entries = false;
    let mut uses_recent_posts = false;

    let body_canvas_layout = match resolution {
        TemplateResolution::DbOverride(record) => {
            let stack = vec![record.canonical_target_key.clone()];
            inspect_db_document_surfaces_in_tx(
                &mut tx,
                &record.document,
                0,
                &stack,
                active,
                url_policy,
                &schemas,
                &mut uses_related_entries,
                &mut uses_recent_posts,
            )
            .await?;
            is_canvas_target(active, &record.target)
        }
        TemplateResolution::ActiveTheme { target_key, html } => {
            let empty_parts = BTreeMap::new();
            let parts = active.map(|a| &a.parts).unwrap_or(&empty_parts);
            let compiled = if let Some(active) = active {
                template_sec::ThemeCompositionCache::global().get_or_compile(
                    &active.revision_digest,
                    target_key,
                    html,
                    parts,
                    csec_content_kind_for_target_key(target_key),
                    url_policy,
                )
            } else {
                template_sec::validate_and_compile_csec_composition_with_theme_urls(
                    target_key,
                    html,
                    parts,
                    csec_content_kind_for_target_key(target_key),
                    url_policy,
                )
                .map(Arc::new)
            }
            .map_err(ApiError::internal)?;
            uses_related_entries |= compiled.uses_related_entries;
            uses_recent_posts |= compiled.uses_recent_posts;
            precompiled_body = Some(compiled);
            is_canvas_resolved_key(active, target_key)
        }
        TemplateResolution::CoreFallback => false,
    };

    let mut shell_part_present = false;
    if !body_canvas_layout {
        let (has_header, compiled_header) = inspect_part_slot_in_tx(
            &mut tx,
            "header",
            0,
            &[],
            active,
            url_policy,
            &schemas,
            &mut uses_related_entries,
            &mut uses_recent_posts,
        )
        .await?;
        let (has_footer, compiled_footer) = inspect_part_slot_in_tx(
            &mut tx,
            "footer",
            0,
            &[],
            active,
            url_policy,
            &schemas,
            &mut uses_related_entries,
            &mut uses_recent_posts,
        )
        .await?;
        shell_part_present = has_header || has_footer;
        precompiled_shell.header = compiled_header;
        precompiled_shell.footer = compiled_footer;
    }

    tx.commit().await?;
    let has_shell_or_theme =
        !matches!(resolution, TemplateResolution::CoreFallback) || shell_part_present;
    Ok(PublicSurfaceInspection {
        precompiled_body,
        precompiled_shell,
        uses_related_entries,
        uses_recent_posts,
        has_shell_or_theme,
    })
}

pub(in crate::storage) fn target_requires_and_forbids_slot(
    kind: TemplateCandidateKind,
) -> (bool, bool) {
    use TemplateCandidateKind as K;
    let requires = matches!(
        kind,
        K::Home | K::Page | K::PageLayout | K::Single | K::SingleCustom | K::Singular
    );
    let forbids = matches!(kind, K::Archive | K::Taxonomy | K::NotFound | K::Part);
    (requires, forbids)
}

pub async fn list_studio_targets(
    db: &SqlitePool,
    themes_root: &Path,
) -> ApiResult<StudioTargetsResponse> {
    use TemplateCandidateKind as K;
    let active = theme::load_active_theme_v3(db, themes_root).await?;
    let overrides_list = list_overrides(db).await?;
    let overrides_map: BTreeMap<String, &TemplateOverrideRecord> = overrides_list
        .iter()
        .map(|row| (row.canonical_target_key.clone(), row))
        .collect();
    let content_types = content::list_content_types_with_meta(db).await?;

    let mut items = Vec::new();
    let mut seen = BTreeSet::new();

    for (part_key, label) in [("header", "Header"), ("footer", "Footer")] {
        let mut target = candidate(K::Part);
        target.part_key = Some(part_key.to_owned());
        push_studio_target_item(
            &mut items,
            &mut seen,
            &overrides_map,
            active.as_ref(),
            target,
            label.to_owned(),
            "parts",
        )?;
    }
    if let Some(ref theme) = active {
        for part_key in theme.manifest.parts.keys() {
            if part_key == "header" || part_key == "footer" {
                continue;
            }
            let mut target = candidate(K::Part);
            target.part_key = Some(part_key.clone());
            push_studio_target_item(
                &mut items,
                &mut seen,
                &overrides_map,
                active.as_ref(),
                target,
                format!("Part: {part_key}"),
                "parts",
            )?;
        }
    }

    for (kind, label) in [
        (K::Home, "Trang chủ (Home)"),
        (K::Page, "Trang mặc định (Page)"),
        (K::Single, "Chi tiết bài viết (Single)"),
        (K::Archive, "Danh sách lưu trữ (Archive)"),
        (K::Index, "Index dự phòng (Index)"),
    ] {
        push_studio_target_item(
            &mut items,
            &mut seen,
            &overrides_map,
            active.as_ref(),
            candidate(kind),
            label.to_owned(),
            "core",
        )?;
    }

    for ct in content_types {
        if ct.base.status != ProviderStatus::Active
            || ct.base.declaration_state != DeclarationState::Declared
        {
            continue;
        }
        let d = &ct.base.descriptor;
        if d.key == "pattern" || d.key == "section" || d.key == "page" || !d.settings.public {
            continue;
        }
        let mut single_target = candidate(K::Single);
        single_target.content_type = Some(d.key.clone());
        push_studio_target_item(
            &mut items,
            &mut seen,
            &overrides_map,
            active.as_ref(),
            single_target,
            format!("Chi tiết: {}", d.label),
            "content_types",
        )?;
        if d.settings.has_archive {
            let mut archive_target = candidate(K::Archive);
            archive_target.content_type = Some(d.key.clone());
            push_studio_target_item(
                &mut items,
                &mut seen,
                &overrides_map,
                active.as_ref(),
                archive_target,
                format!("Lưu trữ: {}", d.label),
                "content_types",
            )?;
        }
    }

    if let Some(ref theme) = active {
        for layout in &theme.manifest.page_layouts {
            let mut target = candidate(K::PageLayout);
            target.layout_key = Some(layout.key.clone());
            push_studio_target_item(
                &mut items,
                &mut seen,
                &overrides_map,
                active.as_ref(),
                target,
                format!("Bố cục trang: {}", layout.label),
                "page_layouts",
            )?;
        }
        for tpl in &theme.manifest.templates {
            if tpl.kind == K::SingleCustom
                && let Some(ck) = &tpl.custom_key
            {
                let mut target = candidate(K::SingleCustom);
                target.custom_key = Some(ck.clone());
                let label = tpl
                    .label
                    .clone()
                    .unwrap_or_else(|| format!("Mẫu tùy chỉnh: {ck}"));
                push_studio_target_item(
                    &mut items,
                    &mut seen,
                    &overrides_map,
                    active.as_ref(),
                    target,
                    label,
                    "custom_templates",
                )?;
            }
        }
    }

    for row in &overrides_list {
        push_studio_target_item(
            &mut items,
            &mut seen,
            &overrides_map,
            active.as_ref(),
            row.target.clone(),
            row.canonical_target_key.clone(),
            "overrides",
        )?;
    }

    Ok(StudioTargetsResponse {
        active_theme_id: active.as_ref().map(|a| a.manifest.theme_id.clone()),
        active_theme_name: active.as_ref().map(|a| a.manifest.name.clone()),
        items,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn preview_studio_with_cache_and_locale(
    db: &SqlitePool,
    themes_root: &Path,
    actor: &SessionContext,
    target: &TemplateCandidate,
    document: Option<&BuilderDocument>,
    preview_entry: Option<&TemplatePreviewEntry>,
    preview_annotations: bool,
    transient: Option<&TransientStudioOverrides>,
    preview_locale: Option<&str>,
    cache: &page::BlockTemplateCache,
) -> ApiResult<RenderedLayout> {
    use TemplateCandidateKind as K;
    let key = target.canonical_key().map_err(ApiError::bad_request)?;
    let mut tx = db.begin().await?;
    let effective_locale =
        settings::resolve_requested_locale_in_conn(&mut *tx, preview_locale).await?;
    validate_typed_target_in_tx(&mut tx, target).await?;
    let active = theme::load_active_theme_v3_in_conn(&mut tx, themes_root).await?;
    let url_policy = active_theme_url_policy(active.as_ref())?;
    let mut state =
        builder::read_render_context_for_locale_tx(&mut tx, Some(&effective_locale), true, None)
            .await?;
    let mut options_richtext_nonces = template_sec::RichTextNonceTable::default();
    let (general, pub_settings, site_options, options, menus) = build_preview_context_in_tx(
        &mut tx,
        active.as_ref(),
        transient,
        &effective_locale,
        Some(&mut options_richtext_nonces),
    )
    .await?;
    state.site_options = site_options.clone();
    state.options = options.clone();
    for (slot_key, view) in &menus {
        state
            .menus
            .insert(slot_key.clone(), serde_json::to_value(view)?);
    }
    if let Some(slot_ov) = transient.and_then(|t| t.menu_slots.as_ref()) {
        for (slot_key, maybe_id) in slot_ov {
            if maybe_id.is_none() {
                state.menus.remove(slot_key);
            }
        }
    }
    state.timezone = general.timezone.clone();
    let fmt_cfg = template_sec::RenderFormattingConfig {
        timezone: state.timezone.clone(),
    };
    let site = SiteContext {
        title: &general.site_title,
        tagline: &general.tagline,
        timezone: &general.timezone,
        origin: normalized_origin(pub_settings.public_origin.as_deref()),
        current_year: content_render::current_year_in_timezone(&general.timezone),
    };

    let default_home_entry: Option<TemplatePreviewEntry> = if preview_entry.is_none()
        && target.kind == K::Home
    {
        let reading =
            settings::get_reading_in_conn_for_locale(&mut *tx, Some(&effective_locale)).await?;
        match reading.homepage_page_id {
            Some(id) => {
                let exists: Option<i64> = sqlx::query_scalar(
                        "SELECT id FROM content_entries WHERE id = ? AND content_type_key = 'page' AND locale = ? AND trashed_at IS NULL",
                    )
                    .bind(id)
                    .bind(&effective_locale)
                    .fetch_optional(&mut *tx)
                    .await?;
                exists.map(|entry_id| TemplatePreviewEntry {
                    type_key: "page".to_owned(),
                    entry_id,
                })
            }
            None => None,
        }
    } else {
        None
    };
    let effective_sample = preview_entry.or(default_home_entry.as_ref());
    let actor_role = actor_field_role(actor);
    let entry = resolve_preview_entry(
        &mut tx,
        target,
        effective_sample,
        active.as_ref(),
        &effective_locale,
        actor_role,
    )
    .await?;

    let mut diagnostics = Vec::new();
    let mut has_active_contact_form = false;
    let mut route = None;
    let mut shared_rb = page::RenderBudget::new();
    shared_rb.richtext_nonces.extend(options_richtext_nonces);
    let (mut theme_context, body) = if let Some((record, effective_schema)) = &entry {
        route = Some((record.content_type_key.as_str(), record.slug.as_str()));
        let built = build_entry_preview_context_in_tx(
            &mut tx,
            record,
            effective_schema,
            actor_role,
            active.as_ref(),
            &site,
            &site_options,
            &options,
            &menus,
            route_kind_for_candidate(target.kind),
        )
        .await?;
        let definition = &effective_schema.base.descriptor;
        let raw_body_has_blocks = record
            .values
            .get("body")
            .and_then(serde_json::Value::as_str)
            .is_some_and(opendash_core::builder_blocks::contains_blocks);
        let body = if raw_body_has_blocks
            && definition.settings.supports(ContentSupport::Builder)
            && record.content_type_key != "pattern"
        {
            if let Some(mut rendered) =
                builder::preview_saved_page_body_in_tx_with_options_and_budget(
                    &mut tx,
                    actor,
                    &record.content_type_key,
                    record.id,
                    Some(&site_options),
                    Some(&options),
                    Some(general.timezone.as_str()),
                    &mut shared_rb,
                    cache,
                )
                .await?
            {
                diagnostics.append(&mut rendered.diagnostics);
                Some(rendered.html)
            } else {
                None
            }
        } else {
            None
        }
        .map(Ok)
        .unwrap_or_else(|| {
            let html = content_render::render_single_with_context_for_locale(
                definition,
                &built.view,
                None,
                &site,
                built.archive_url.as_deref(),
                &built.breadcrumbs,
                &fmt_cfg,
                &effective_locale,
            )?;
            crate::document::normalize_public_document(&html)
                .map(|doc| doc.body_fragment)
                .map_err(|_| ApiError::bad_request("Invalid preview body."))
        })?;
        (built.theme_context, body)
    } else {
        let theme_context = build_placeholder_preview_theme_context(
            target,
            active.as_ref(),
            &site,
            &site_options,
            &options,
            &menus,
        )?;
        (
            theme_context,
            "<div data-template-preview-placeholder>Preview content</div>".to_owned(),
        )
    };
    if body.len() > template_sec::MAX_RENDERED_HTML_BYTES {
        return Err(ApiError::bad_request("Preview output exceeds 512 KiB."));
    }

    let slot_fragment = crate::document::normalize_public_document_with_options(&body, true)
        .map(|parts| {
            let mut f = parts.authorized_styles.concat();
            f.push_str(&parts.body_fragment);
            f
        })
        .unwrap_or_else(|_| body.clone());

    let scope = if preview_annotations {
        RenderAnnotationScope::TemplateAll
    } else {
        RenderAnnotationScope::None
    };
    let mut preview_budget = PreviewCompositionBudget::with_render_budget(shared_rb.clone());
    let mut public_budget = PublicCompositionBudget::with_render_budget(shared_rb);
    let preview_locale_ctx = crate::storage::seo::build_theme_locale_context_in_conn(
        &mut tx,
        Some(&effective_locale),
        crate::storage::seo::ThemeLocaleTarget::None,
    )
    .await?;
    let preview_t_map = active
        .as_ref()
        .map(|a| a.merged_translations(&effective_locale))
        .unwrap_or_default();
    if let Some(obj) = theme_context.as_object_mut() {
        obj.insert(
            "t".to_owned(),
            serde_json::to_value(&preview_t_map).unwrap_or_else(|_| serde_json::json!({})),
        );
        obj.insert("locale".to_owned(), preview_locale_ctx.clone());
    }
    public_budget.set_locale_context(active.as_ref(), &effective_locale, preview_locale_ctx);
    let mut canvas_layout = is_canvas_target(active.as_ref(), target);
    let main_scope = if target.kind == K::Part {
        part_scope_prefix(target.part_key.as_deref().unwrap_or("part"), 0)
    } else {
        "ov".to_owned()
    };

    enum PreviewMainPlan<'a> {
        DraftDoc(&'a BuilderDocument),
        Resolved(TemplateResolution),
    }
    let main_plan = if let Some(doc) = document {
        let existing = get_typed_override_on_connection(&mut tx, &key).await?;
        validate_typed_document_in_tx(&mut tx, target, doc, existing.as_ref().map(|r| &r.document))
            .await?;
        PreviewMainPlan::DraftDoc(doc)
    } else {
        let chain = build_reset_candidate_chain_for_locale_in_conn(
            &mut tx,
            target,
            Some(&effective_locale),
        )
        .await?;
        let resolved = resolve_template_2axis_on_connection(
            &mut tx,
            &chain,
            active.as_ref().map(|a| &a.templates),
            active.as_ref().map(|a| &a.parts),
            active.as_ref().map(|a| &a.page_layouts),
        )
        .await?;
        match &resolved {
            TemplateResolution::DbOverride(record) => {
                canvas_layout = is_canvas_target(active.as_ref(), &record.target);
            }
            TemplateResolution::ActiveTheme { target_key, .. } => {
                canvas_layout = is_canvas_resolved_key(active.as_ref(), target_key);
            }
            TemplateResolution::CoreFallback => {
                canvas_layout = false;
            }
        }
        PreviewMainPlan::Resolved(resolved)
    };

    let mut shell_part_docs = Vec::new();
    if target.kind != K::Part && !canvas_layout {
        for slot_key in ["header", "footer"] {
            let part_target = TemplateCandidate {
                part_key: Some(slot_key.to_owned()),
                ..candidate(K::Part)
            };
            let canonical = part_target.canonical_key().map_err(ApiError::bad_request)?;
            if let Some(row) = get_typed_override_on_connection(&mut *tx, &canonical).await? {
                shell_part_docs.push(row.document);
            }
        }
    }
    {
        let mut hydrate_roots: Vec<&BuilderDocument> = shell_part_docs.iter().collect();
        match &main_plan {
            PreviewMainPlan::DraftDoc(doc) => hydrate_roots.push(doc),
            PreviewMainPlan::Resolved(TemplateResolution::DbOverride(record)) => {
                hydrate_roots.push(&record.document);
            }
            _ => {}
        }
        if !hydrate_roots.is_empty() {
            collect_db_docs_and_hydrate_state_in_tx(&mut tx, &mut state, &hydrate_roots, true)
                .await?;
        }
    }

    let main_html = match main_plan {
        PreviewMainPlan::DraftDoc(doc) => {
            let graph =
                preview_part_graph(&mut tx, target, doc, active.as_ref(), &mut preview_budget)
                    .await?;
            let ctx = PreviewRenderContext {
                state: &state,
                graph: &graph,
                theme_context: &theme_context,
                route,
                render_locale: &effective_locale,
                cache,
                url_policy: &url_policy,
                fmt_cfg: &fmt_cfg,
            };
            let mut rendered = render_preview_builder(
                &mut tx,
                doc,
                Some(&slot_fragment),
                &ctx,
                &mut preview_budget,
                scope,
                &main_scope,
            )
            .await?;
            diagnostics.append(&mut rendered.diagnostics);
            has_active_contact_form |= rendered.has_active_contact_form;
            rendered.html
        }
        PreviewMainPlan::Resolved(resolved) => match resolved {
            TemplateResolution::DbOverride(record) => {
                let graph = preview_part_graph(
                    &mut tx,
                    &record.target,
                    &record.document,
                    active.as_ref(),
                    &mut preview_budget,
                )
                .await?;
                let ctx = PreviewRenderContext {
                    state: &state,
                    graph: &graph,
                    theme_context: &theme_context,
                    route,
                    render_locale: &effective_locale,
                    cache,
                    url_policy: &url_policy,
                    fmt_cfg: &fmt_cfg,
                };
                let slot = if target.kind == K::Part {
                    None
                } else {
                    Some(slot_fragment.as_str())
                };
                let mut rendered = render_preview_builder(
                    &mut tx,
                    &record.document,
                    slot,
                    &ctx,
                    &mut preview_budget,
                    scope,
                    &main_scope,
                )
                .await?;
                diagnostics.append(&mut rendered.diagnostics);
                has_active_contact_form |= rendered.has_active_contact_form;
                rendered.html
            }
            TemplateResolution::ActiveTheme { target_key, html } => {
                let slot = if target.kind == K::Part {
                    None
                } else {
                    Some(slot_fragment.as_str())
                };
                let out = render_csec_template_for_preview(
                    &target_key,
                    &html,
                    active.as_ref(),
                    &theme_context,
                    slot,
                    &mut public_budget,
                    &fmt_cfg,
                )?;
                preview_budget.sync_from_render_budget(&public_budget.render_budget);
                out
            }
            TemplateResolution::CoreFallback => {
                if target.kind == K::Part {
                    String::new()
                } else {
                    format!(
                        "<main><h1>{}</h1><p>{}</p>{}</main>",
                        escape_preview_text(&general.site_title),
                        escape_preview_text(&general.tagline),
                        slot_fragment
                    )
                }
            }
        },
    };

    let (header_html, footer_html) = if target.kind != K::Part && !canvas_layout {
        let hdr = render_preview_named_part_in_tx(
            &mut tx,
            "header",
            &state,
            active.as_ref(),
            &theme_context,
            route,
            &effective_locale,
            &mut preview_budget,
            cache,
        )
        .await?;
        let ftr = render_preview_named_part_in_tx(
            &mut tx,
            "footer",
            &state,
            active.as_ref(),
            &theme_context,
            route,
            &effective_locale,
            &mut preview_budget,
            cache,
        )
        .await?;
        if let Some(ref h) = hdr {
            has_active_contact_form |= h.has_active_contact_form;
        }
        if let Some(ref f) = ftr {
            has_active_contact_form |= f.has_active_contact_form;
        }
        (hdr.map(|r| r.html), ftr.map(|r| r.html))
    } else {
        (None, None)
    };

    let (_, bcp47_tag, dir) =
        settings::get_locale_bcp47_and_dir_in_conn(&mut *tx, Some(&effective_locale)).await?;
    let seo = crate::seo::SeoView::for_noindex_page_with_dir(&bcp47_tag, &dir, &general.site_title);
    let enqueued = preview_enqueued_styles(active.as_ref(), preview_condition(target.kind));
    let full_html = content_render::compose_document_with_seo_with_assets(
        header_html.as_deref(),
        &main_html,
        footer_html.as_deref(),
        &seo,
        &[],
        &enqueued,
        true,
    )?;
    let full_html = if !preview_budget.hook_nonces.is_empty() {
        let clear: Vec<(String, String)> = preview_budget
            .hook_nonces
            .entries()
            .iter()
            .map(|h| (h.nonce.clone(), String::new()))
            .collect();
        content_render::apply_render_hooks_to_document(full_html, "", &[], &[], "", &clear)?
    } else {
        full_html
    };
    let full_html =
        content_render::finalize_document_icons(full_html, &preview_budget.icon_nonces)?;
    let full_html =
        content_render::finalize_document_richtext(full_html, &preview_budget.richtext_nonces)?;
    tx.rollback().await?;
    Ok(RenderedLayout {
        has_active_contact_form,
        html: full_html,
        diagnostics,
    })
}

#[allow(clippy::too_many_arguments)]
pub(in crate::storage) async fn wrap_entry_body_in_preview_shell_in_tx_with_budget(
    tx: &mut Transaction<'_, Sqlite>,
    themes_root: &Path,
    actor_role: FieldRole,
    schema: &EffectiveContentSchema,
    entry_id: i64,
    page_layout_key: Option<&str>,
    custom_template_key: Option<&str>,
    entry_rendered: RenderedLayout,
    mut entry_render_budget: page::RenderBudget,
    cache: &page::BlockTemplateCache,
) -> ApiResult<RenderedLayout> {
    let mut record = entries::get_entry_in_tx(tx, &schema.base.descriptor.key, entry_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Content entry not found."))?;
    entries::filter_entry_values_for_role(&mut record, schema, actor_role);
    let active = theme::load_active_theme_v3_in_conn(&mut **tx, themes_root).await?;
    let url_policy = active_theme_url_policy(active.as_ref())?;
    let mut state =
        builder::read_render_context_for_locale_tx(tx, Some(&record.locale), true, None).await?;
    let mut options_richtext_nonces = template_sec::RichTextNonceTable::default();
    let (general, pub_settings, site_options, options, menus) = build_preview_context_in_tx(
        tx,
        active.as_ref(),
        None,
        &record.locale,
        Some(&mut options_richtext_nonces),
    )
    .await?;
    entry_render_budget
        .richtext_nonces
        .extend(options_richtext_nonces);
    state.site_options = site_options.clone();
    state.options = options.clone();
    for (slot_key, view) in &menus {
        state
            .menus
            .insert(slot_key.clone(), serde_json::to_value(view)?);
    }
    state.timezone = general.timezone.clone();
    let fmt_cfg = template_sec::RenderFormattingConfig {
        timezone: state.timezone.clone(),
    };
    let site = SiteContext {
        title: &general.site_title,
        tagline: &general.tagline,
        timezone: &general.timezone,
        origin: normalized_origin(pub_settings.public_origin.as_deref()),
        current_year: content_render::current_year_in_timezone(&general.timezone),
    };
    let shell_route_kind = if schema.base.descriptor.key == "page" {
        "page"
    } else {
        "single"
    };
    let built = build_entry_preview_context_in_tx(
        tx,
        &record,
        schema,
        actor_role,
        active.as_ref(),
        &site,
        &site_options,
        &options,
        &menus,
        shell_route_kind,
    )
    .await?;
    let mut theme_context = built.theme_context;
    let ctx_target = if schema.base.descriptor.key == "page" {
        PublicTemplateContext::Page {
            page_layout_key: page_layout_key.map(ToOwned::to_owned),
        }
    } else {
        PublicTemplateContext::Single {
            content_type: schema.base.descriptor.key.clone(),
            entry_id: record.id,
            slug: record.slug.clone(),
            custom_template_key: custom_template_key.map(ToOwned::to_owned),
        }
    };
    let candidates = build_candidates(&ctx_target)?;
    let resolution = resolve_template_2axis_on_connection(
        &mut **tx,
        &candidates,
        active.as_ref().map(|a| &a.templates),
        active.as_ref().map(|a| &a.parts),
        active.as_ref().map(|a| &a.page_layouts),
    )
    .await?;
    let slot_fragment =
        crate::document::normalize_public_document_with_options(&entry_rendered.html, true)
            .map(|parts| {
                let mut f = parts.authorized_styles.concat();
                f.push_str(&parts.body_fragment);
                f
            })
            .unwrap_or_else(|_| entry_rendered.html.clone());

    let mut preview_budget =
        PreviewCompositionBudget::with_render_budget(entry_render_budget.clone());
    let mut public_budget = PublicCompositionBudget::with_render_budget(entry_render_budget);
    let preview_locale_ctx = crate::storage::seo::build_theme_locale_context_in_conn(
        &mut **tx,
        Some(&record.locale),
        crate::storage::seo::ThemeLocaleTarget::Entry {
            entry_id: record.id,
        },
    )
    .await?;
    let preview_t_map = active
        .as_ref()
        .map(|a| a.merged_translations(&record.locale))
        .unwrap_or_default();
    if let Some(obj) = theme_context.as_object_mut() {
        obj.insert(
            "t".to_owned(),
            serde_json::to_value(&preview_t_map).unwrap_or_else(|_| serde_json::json!({})),
        );
        obj.insert("locale".to_owned(), preview_locale_ctx.clone());
    }
    public_budget.set_locale_context(active.as_ref(), &record.locale, preview_locale_ctx);
    let route = Some((record.content_type_key.as_str(), record.slug.as_str()));
    let mut diagnostics = entry_rendered.diagnostics;
    let mut has_active_contact_form = entry_rendered.has_active_contact_form;
    let mut canvas_layout = match &resolution {
        TemplateResolution::DbOverride(override_rec) => {
            is_canvas_target(active.as_ref(), &override_rec.target)
        }
        TemplateResolution::ActiveTheme { target_key, .. } => {
            is_canvas_resolved_key(active.as_ref(), target_key)
        }
        TemplateResolution::CoreFallback => false,
    };
    let mut shell_part_docs = Vec::new();
    if !canvas_layout {
        for slot_key in ["header", "footer"] {
            let part_target = TemplateCandidate {
                part_key: Some(slot_key.to_owned()),
                ..candidate(TemplateCandidateKind::Part)
            };
            let canonical = part_target.canonical_key().map_err(ApiError::internal)?;
            if let Some(row) = get_typed_override_on_connection(&mut **tx, &canonical).await? {
                shell_part_docs.push(row.document);
            }
        }
    }
    {
        let mut hydrate_roots: Vec<&BuilderDocument> = shell_part_docs.iter().collect();
        if let TemplateResolution::DbOverride(override_rec) = &resolution {
            hydrate_roots.push(&override_rec.document);
        }
        if !hydrate_roots.is_empty() {
            collect_db_docs_and_hydrate_state_in_tx(tx, &mut state, &hydrate_roots, true).await?;
        }
    }

    let template_html = match resolution {
        TemplateResolution::DbOverride(override_rec) => {
            canvas_layout = is_canvas_target(active.as_ref(), &override_rec.target);
            let graph = preview_part_graph(
                tx,
                &override_rec.target,
                &override_rec.document,
                active.as_ref(),
                &mut preview_budget,
            )
            .await?;
            let ctx = PreviewRenderContext {
                state: &state,
                graph: &graph,
                theme_context: &theme_context,
                route,
                render_locale: &record.locale,
                cache,
                url_policy: &url_policy,
                fmt_cfg: &fmt_cfg,
            };
            let mut tpl_rendered = render_preview_builder(
                tx,
                &override_rec.document,
                Some(&slot_fragment),
                &ctx,
                &mut preview_budget,
                RenderAnnotationScope::None,
                "ov",
            )
            .await?;
            diagnostics.append(&mut tpl_rendered.diagnostics);
            has_active_contact_form |= tpl_rendered.has_active_contact_form;
            tpl_rendered.html
        }
        TemplateResolution::ActiveTheme { target_key, html } => {
            canvas_layout = is_canvas_resolved_key(active.as_ref(), &target_key);
            let out = render_csec_template_for_preview(
                &target_key,
                &html,
                active.as_ref(),
                &theme_context,
                Some(&slot_fragment),
                &mut public_budget,
                &fmt_cfg,
            )?;
            preview_budget.sync_from_render_budget(&public_budget.render_budget);
            out
        }
        TemplateResolution::CoreFallback => slot_fragment,
    };

    let (header_html, footer_html) = if !canvas_layout {
        let hdr = render_preview_named_part_in_tx(
            tx,
            "header",
            &state,
            active.as_ref(),
            &theme_context,
            route,
            &record.locale,
            &mut preview_budget,
            cache,
        )
        .await?;
        let ftr = render_preview_named_part_in_tx(
            tx,
            "footer",
            &state,
            active.as_ref(),
            &theme_context,
            route,
            &record.locale,
            &mut preview_budget,
            cache,
        )
        .await?;
        (hdr.map(|r| r.html), ftr.map(|r| r.html))
    } else {
        (None, None)
    };

    let (_, bcp47_tag, dir) =
        settings::get_locale_bcp47_and_dir_in_conn(&mut **tx, Some(&record.locale)).await?;
    let seo = crate::seo::SeoView::for_noindex_page_with_dir(&bcp47_tag, &dir, &record.title);
    let condition = if record.content_type_key == "page" {
        EnqueueCondition::Page
    } else {
        EnqueueCondition::Single
    };
    let enqueued = preview_enqueued_styles(active.as_ref(), condition);
    let full_html = content_render::compose_document_with_seo_with_assets(
        header_html.as_deref(),
        &template_html,
        footer_html.as_deref(),
        &seo,
        &[],
        &enqueued,
        true,
    )?;
    let full_html = if !preview_budget.hook_nonces.is_empty() {
        let clear: Vec<(String, String)> = preview_budget
            .hook_nonces
            .entries()
            .iter()
            .map(|h| (h.nonce.clone(), String::new()))
            .collect();
        content_render::apply_render_hooks_to_document(full_html, "", &[], &[], "", &clear)?
    } else {
        full_html
    };
    let full_html =
        content_render::finalize_document_icons(full_html, &preview_budget.icon_nonces)?;
    let full_html =
        content_render::finalize_document_richtext(full_html, &preview_budget.richtext_nonces)?;
    Ok(RenderedLayout {
        has_active_contact_form,
        html: full_html,
        diagnostics,
    })
}
