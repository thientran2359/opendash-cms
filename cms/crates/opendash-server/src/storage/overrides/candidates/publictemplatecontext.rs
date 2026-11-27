use super::*;

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
