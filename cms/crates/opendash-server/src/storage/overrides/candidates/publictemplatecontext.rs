use super::*;

/// The route must authenticate publication, provider visibility, archive
/// eligibility and assigned template eligibility before constructing context.
#[derive(Debug, Clone)]
pub enum PublicTemplateContext {
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
    Search,
    Index,
    Part {
        part_key: String,
    },
}

pub(crate) fn is_canvas_page_layout(
    active: Option<&ActiveTheme>,
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
    active: Option<&ActiveTheme>,
    target: &TemplateCandidate,
) -> bool {
    target.kind == TemplateCandidateKind::PageLayout
        && is_canvas_page_layout(active, target.layout_key.as_deref())
}

pub(in crate::storage) fn is_canvas_resolved_key(
    active: Option<&ActiveTheme>,
    target_key: &str,
) -> bool {
    is_canvas_page_layout(active, target_key.strip_prefix("page_layout:"))
}

pub fn build_candidates(context: &PublicTemplateContext) -> ApiResult<Vec<TemplateCandidate>> {
    use TemplateCandidateKind as K;
    let mut result = Vec::new();
    match context {
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
        PublicTemplateContext::Search => {
            result.push(candidate(K::Search));
            result.push(candidate(K::Index));
        }
        PublicTemplateContext::Index => {
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
        K::Search => candidates.push(candidate(K::Index)),
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
        K::Page => "page",
        K::PageLayout => "page_layout",
        K::Single => "single",
        K::SingleCustom => "single_custom",
        K::Archive => "archive",
        K::Taxonomy => "taxonomy",
        K::Singular => "singular",
        K::Index => "index",
        K::NotFound => "not_found",
        K::Search => "search",
        K::Part => "part",
    }
}

const ENTRY_CONTEXT_BLOCK_IDS: &[&str] = &[
    "core/entry-title",
    "core/entry-featured-image",
    "core/entry-meta",
    "core/entry-excerpt",
    "core/entry-terms",
    "core/entry-field",
    "core/entry-related",
    "core/entry-nav",
];

fn collect_parts_and_slots_rec(
    nodes: &[opendash_core::builder::BlockNode],
    schemas: &opendash_core::registry::RegistryCatalog<
        opendash_core::block_schema::RegisteredBlockSchema,
    >,
    parts: &mut Vec<String>,
    slots: &mut usize,
    entry_blocks: &mut usize,
) {
    for node in nodes {
        let Some(entry) = schemas.get(&node.block_id) else {
            continue;
        };
        if entry.status != ProviderStatus::Active
            || entry.declaration_state != DeclarationState::Declared
        {
            continue;
        }
        match node.block_id.as_str() {
            "core/entry-content" => *slots += 1,
            "core/template-part" => {
                if let Some(part_key) = node
                    .props
                    .get("part_key")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    parts.push(part_key.to_owned());
                }
            }
            id => {
                if ENTRY_CONTEXT_BLOCK_IDS.contains(&id) {
                    *entry_blocks += 1;
                }
                parts.extend(
                    entry
                        .descriptor
                        .schema
                        .collect_template_part_keys(&node.props),
                );
            }
        }
        collect_parts_and_slots_rec(&node.children, schemas, parts, slots, entry_blocks);
    }
}

pub(in crate::storage) fn document_slot_scan_with_schemas(
    document: &BuilderDocument,
    schemas: &opendash_core::registry::RegistryCatalog<
        opendash_core::block_schema::RegisteredBlockSchema,
    >,
) -> ApiResult<(Vec<String>, usize, usize)> {
    let mut parts = Vec::new();
    let mut slots = 0;
    let mut entry_blocks = 0;
    collect_parts_and_slots_rec(&document.blocks, schemas, &mut parts, &mut slots, &mut entry_blocks);
    Ok((parts, slots, entry_blocks))
}

pub(in crate::storage) fn document_parts_and_slot_count_with_schemas(
    document: &BuilderDocument,
    schemas: &opendash_core::registry::RegistryCatalog<
        opendash_core::block_schema::RegisteredBlockSchema,
    >,
) -> ApiResult<(Vec<String>, usize)> {
    let (parts, slots, _) = document_slot_scan_with_schemas(document, schemas)?;
    Ok((parts, slots))
}

pub(in crate::storage) fn validate_content_slots(
    target: &TemplateCandidate,
    entry_content_count: usize,
    entry_context_block_count: usize,
) -> ApiResult<()> {
    let valid = match entry_content_rule_for_target(target.kind) {
        EntryContentRule::RequiredOrEntryBlock => {
            entry_content_count <= 1
                && (entry_content_count == 1 || entry_context_block_count >= 1)
        }
        EntryContentRule::OptionalMaxOne => entry_content_count <= 1,
        EntryContentRule::Forbidden => entry_content_count == 0,
    };
    if valid {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "Invalid content slot count for template target.",
        ))
    }
}

fn has_active_pattern_source_rec(
    nodes: &[opendash_core::builder::BlockNode],
    schemas: &opendash_core::registry::RegistryCatalog<
        opendash_core::block_schema::RegisteredBlockSchema,
    >,
) -> bool {
    nodes.iter().any(|node| {
        let Some(entry) = schemas.get(&node.block_id) else {
            return false;
        };
        if entry.status != ProviderStatus::Active
            || entry.declaration_state != DeclarationState::Declared
        {
            return false;
        }
        node.block_id == "core/pattern-ref"
            || entry
                .descriptor
                .schema
                .has_non_empty_pattern_source_value(&node.props)
            || has_active_pattern_source_rec(&node.children, schemas)
    })
}

pub(in crate::storage) fn normalize_template_ast_json(
    state: &BuilderRenderContext,
    document: &BuilderDocument,
) -> ApiResult<String> {
    let schemas = state.unified_block_schemas()?;
    let normalized = builder::strip_document_default_props(document, &schemas);
    String::from_utf8(
        normalized
            .to_json()
            .map_err(|e| ApiError::bad_request(e.to_string()))?,
    )
    .map_err(|_| ApiError::bad_request("Invalid Builder UTF-8 document."))
}

pub(in crate::storage) async fn validate_typed_document_with_context_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    target: &TemplateCandidate,
    document: &BuilderDocument,
    prior: Option<&BuilderDocument>,
    is_preview: bool,
    requested_locale: Option<&str>,
) -> ApiResult<BuilderRenderContext> {
    validate_typed_target_in_tx(tx, target).await?;
    let state =
        builder::read_render_context_for_locale_tx(tx, requested_locale, is_preview, None).await?;
    let schemas = state.unified_block_schemas()?;
    let ctx = BuilderContextTarget::Template {
        kind: target.kind,
        content_type: target.content_type.clone(),
        taxonomy: target.taxonomy.clone(),
        part_key: target.part_key.clone(),
    };
    document
        .validate_for_save(&schemas, &ctx, prior)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    // Shared template overrides have no per-entry entry_pattern_refs ownership or
    // public-media projection, so PatternRef remains entry-scoped.
    if !builder::pattern_ids_with_schemas(document, &schemas).is_empty()
        || has_active_pattern_source_rec(&document.blocks, &schemas)
    {
        return Err(ApiError::bad_request(
            "Reusable Pattern chỉ hỗ trợ Builder của từng entry/Page.",
        ));
    }
    let (_, slots, entry_blocks) = document_slot_scan_with_schemas(document, &schemas)?;
    validate_content_slots(target, slots, entry_blocks)?;
    crate::storage::fields::validate_document_block_refs_in_conn(
        &mut **tx, document, prior, &schemas, None, None,
    )
    .await?;
    builder::validate_media_refs_in_tx(&mut **tx, document).await?;
    Ok(state)
}

/// The same AST and route-binding validation is used by Save and read-only Preview.
pub(in crate::storage) async fn validate_typed_document_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    target: &TemplateCandidate,
    document: &BuilderDocument,
    prior: Option<&BuilderDocument>,
) -> ApiResult<BuilderRenderContext> {
    validate_typed_document_with_context_in_tx(tx, target, document, prior, false, None).await
}
