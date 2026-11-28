use super::*;

#[derive(Clone)]
pub struct BuilderRenderContext {
    pub declared_blocks: BTreeMap<String, crate::storage::page::RegisteredBlockDef>,
    pub site_options: BTreeMap<String, serde_json::Value>,
    pub options: BTreeMap<String, serde_json::Value>,
    pub menus: BTreeMap<String, serde_json::Value>,
    pub hydration_media: BTreeMap<String, crate::storage::fields::HydratedMediaRow>,
    pub hydration_entries: BTreeMap<i64, crate::storage::fields::HydratedEntryRefRow>,
    pub hydration_terms: BTreeMap<i64, crate::storage::fields::HydratedTermRefRow>,
    pub timezone: String,
    pub locale_search_url: String,
}

impl Default for BuilderRenderContext {
    fn default() -> Self {
        Self {
            declared_blocks: BTreeMap::new(),
            site_options: BTreeMap::new(),
            options: BTreeMap::new(),
            menus: BTreeMap::new(),
            hydration_media: BTreeMap::new(),
            hydration_entries: BTreeMap::new(),
            hydration_terms: BTreeMap::new(),
            timezone: "UTC".to_owned(),
            locale_search_url: "/".to_owned(),
        }
    }
}

/// Only public-safe display text, canonical storage-resolved URLs, and opted-in
/// hydrated card metadata leave the list resolver. IDs remain stable across slug edits.
#[derive(Debug, Clone, Default)]
pub struct ListItem {
    pub entry_id: i64,
    pub title: String,
    pub slug: String,
    pub href: Option<String>,
    pub cover_url: Option<String>,
    pub published_at: Option<String>,
    pub excerpt: Option<String>,
    pub terms: Vec<super::ListItemTerm>,
}

#[derive(Debug)]
pub enum ResolvedList {
    Page {
        page: i64,
        items: Vec<ListItem>,
        has_next: bool,
        out_of_range: bool,
    },
    MissingConfig,
}

pub type ListResults = BTreeMap<String, ResolvedList>;

fn bool_prop(props: &BTreeMap<String, Value>, key: &str, default: bool) -> bool {
    props.get(key).and_then(Value::as_bool).unwrap_or(default)
}

/// Use the same SQLite snapshot as the saved document.
pub async fn resolve_contact_forms_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    document: &BuilderDocument,
) -> ApiResult<crate::page::ContactFormResults> {
    let mut nodes: Vec<(String, Option<i64>)> = Vec::new();
    document.walk_blocks(|node, _| {
        if node.block_id == "core/contact-form" {
            nodes.push((
                node.id.clone(),
                parse_positive_i64_prop(node.props.get("form_id")),
            ));
        }
    });
    let mut results = crate::page::ContactFormResults::new();
    for (node_id, fid_opt) in nodes {
        let rec = match fid_opt {
            Some(form_id) => crate::storage::contact::get_form_in_conn(&mut *tx, form_id).await?,
            None => None,
        };
        results.insert(node_id, rec);
    }
    Ok(results)
}

pub async fn resolve_lists_in_tx_for_locale(
    tx: &mut Transaction<'_, Sqlite>,
    document: &BuilderDocument,
    pages: &BTreeMap<String, i64>,
    render_locale: Option<&str>,
) -> ApiResult<ListResults> {
    resolve_lists_in_tx_for_locale_inner(tx, document, pages, render_locale, None).await
}

/// Request-scoped variant that charges each list resolution against `RenderBudget`.
pub async fn resolve_lists_in_tx_for_locale_with_budget(
    tx: &mut Transaction<'_, Sqlite>,
    document: &BuilderDocument,
    pages: &BTreeMap<String, i64>,
    render_locale: Option<&str>,
    budget: &mut crate::template_sec::RenderBudget,
) -> ApiResult<ListResults> {
    resolve_lists_in_tx_for_locale_inner(tx, document, pages, render_locale, Some(budget)).await
}

fn collect_list_nodes_rec<'a>(
    nodes: &'a [opendash_core::builder::BlockNode],
    out: &mut Vec<&'a opendash_core::builder::BlockNode>,
) {
    for node in nodes {
        if node.block_id == "core/list" {
            out.push(node);
        }
        collect_list_nodes_rec(&node.children, out);
    }
}

async fn resolve_lists_in_tx_for_locale_inner(
    tx: &mut Transaction<'_, Sqlite>,
    document: &BuilderDocument,
    pages: &BTreeMap<String, i64>,
    render_locale: Option<&str>,
    mut budget: Option<&mut crate::template_sec::RenderBudget>,
) -> ApiResult<ListResults> {
    let active_locale = match render_locale {
        Some(raw) => {
            crate::storage::settings::resolve_requested_locale_in_conn(&mut **tx, Some(raw)).await?
        }
        None => crate::storage::settings::get_default_locale_code_in_conn(&mut **tx).await?,
    };
    let mut list_nodes = Vec::new();
    collect_list_nodes_rec(&document.blocks, &mut list_nodes);
    let mut results = ListResults::new();
    for node in list_nodes {
        if let Some(b) = budget.as_deref_mut() {
            b.charge_builder_list().map_err(ApiError::bad_request)?;
        }
        // Defensive cap even for corrupt historical stored data.
        if results.len() >= 8 {
            return Err(ApiError::bad_request("Builder vượt giới hạn 8 list."));
        }
        let type_key = node
            .props
            .get("type_key")
            .and_then(Value::as_str)
            .unwrap_or("post");
        let taxonomy_key = node
            .props
            .get("taxonomy_key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let term_id = parse_positive_i64_prop(node.props.get("term_id"));
        let include_descendants = bool_prop(&node.props, "include_descendants", false);
        let sort = match node.props.get("sort").and_then(Value::as_str) {
            Some("id_asc") => PublishedListSort::IdAsc,
            _ => PublishedListSort::IdDesc,
        };
        let page_size = node
            .props
            .get("page_size")
            .and_then(Value::as_i64)
            .unwrap_or(6)
            .clamp(1, 24);
        let extra_offset = node
            .props
            .get("offset")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .clamp(0, 200);
        let hydration_opts = super::ListHydrationOptions {
            show_image: bool_prop(&node.props, "show_image", true),
            show_date: bool_prop(&node.props, "show_date", true),
            show_excerpt: bool_prop(&node.props, "show_excerpt", true),
            show_taxonomy: bool_prop(&node.props, "show_taxonomy", true),
        };
        let page = pages
            .get(&node.id)
            .copied()
            .filter(|p| (1..=100).contains(p))
            .unwrap_or(1);
        let raw_filters = EntryFilters {
            category_id: None,
            tag_id: None,
            taxonomy_key,
            term_id,
            include_descendants,
            locale: Some(active_locale.clone()),
        };
        let filters = match super::resolve_list_filters_for_locale_in_tx(
            tx,
            type_key,
            raw_filters,
            &active_locale,
        )
        .await?
        {
            super::LocalizedListFilterOutcome::Ready(resolved) => resolved,
            super::LocalizedListFilterOutcome::MissingConfig => {
                results.insert(node.id.clone(), ResolvedList::MissingConfig);
                continue;
            }
            super::LocalizedListFilterOutcome::EmptyDueToMissingTranslation => {
                results.insert(
                    node.id.clone(),
                    ResolvedList::Page {
                        page,
                        out_of_range: page > 1,
                        items: Vec::new(),
                        has_next: false,
                    },
                );
                continue;
            }
        };
        let result = entries::list_published_page_with_offset_in_tx(
            tx,
            type_key,
            page,
            page_size,
            extra_offset,
            sort,
            filters,
        )
        .await?;
        let rendered = match result {
            PublishedListResult::Page(result) => {
                let items =
                    super::hydrate_list_items_in_tx(tx, type_key, &result.entries, hydration_opts)
                        .await?;
                ResolvedList::Page {
                    page,
                    out_of_range: page > 1 && items.is_empty(),
                    items,
                    has_next: result.has_next,
                }
            }
            PublishedListResult::MissingConfig(_) => ResolvedList::MissingConfig,
        };
        results.insert(node.id.clone(), rendered);
    }
    Ok(results)
}

pub(crate) async fn read_render_context_for_locale_tx(
    tx: &mut Transaction<'_, Sqlite>,
    requested_locale: Option<&str>,
    is_preview: bool,
    current_path: Option<&str>,
) -> ApiResult<BuilderRenderContext> {
    let declared_blocks = crate::storage::page::load_declared_blocks_in_conn(&mut **tx).await?;
    let (site_options, options) =
        crate::storage::settings::get_active_merged_public_options_in_conn_for_locale(
            &mut **tx,
            is_preview,
            requested_locale,
        )
        .await?;
    let menus = crate::storage::menu::resolve_all_menus_for_locale_and_path_in_conn(
        &mut **tx,
        requested_locale,
        current_path,
    )
    .await?;
    let timezone = crate::storage::settings::get_general_in_conn(&mut **tx)
        .await?
        .timezone;
    let locale_search_url =
        crate::storage::seo::resolve_locale_search_url_in_conn(&mut **tx, requested_locale).await?;
    Ok(BuilderRenderContext {
        declared_blocks,
        site_options,
        options,
        menus,
        timezone,
        locale_search_url,
        ..Default::default()
    })
}

pub(crate) async fn read_render_context_tx(
    tx: &mut Transaction<'_, Sqlite>,
) -> ApiResult<BuilderRenderContext> {
    read_render_context_for_locale_tx(tx, None, false, None).await
}

pub(crate) async fn hydrate_render_context_for_documents_in_conn(
    conn: &mut SqliteConnection,
    state: &mut BuilderRenderContext,
    docs: &[&BuilderDocument],
    is_preview: bool,
) -> ApiResult<()> {
    let mut media_ids = BTreeSet::new();
    let mut entry_ids = BTreeSet::new();
    let mut term_ids = BTreeSet::new();
    for doc in docs {
        collect_builder_document_media_ids(
            doc,
            &state.declared_blocks,
            &mut media_ids,
            &mut entry_ids,
            &mut term_ids,
        );
    }

    let missing_entries: BTreeSet<i64> = entry_ids
        .into_iter()
        .filter(|id| !state.hydration_entries.contains_key(id))
        .collect();
    if !missing_entries.is_empty() {
        state.hydration_entries.extend(
            crate::storage::fields::load_entry_ref_hydration_map_in_conn(
                conn,
                &missing_entries,
                is_preview,
                None,
            )
            .await?,
        );
    }

    let missing_terms: BTreeSet<i64> = term_ids
        .into_iter()
        .filter(|id| !state.hydration_terms.contains_key(id))
        .collect();
    if !missing_terms.is_empty() {
        let loaded_terms = crate::storage::fields::load_term_ref_hydration_map_in_conn(
            conn,
            &missing_terms,
            is_preview,
            None,
        )
        .await?;
        for term in loaded_terms.values() {
            if let Some(fid) = &term.featured_image_media_id {
                media_ids.insert(fid.clone());
            }
        }
        state.hydration_terms.extend(loaded_terms);
    }

    let missing_media: BTreeSet<String> = media_ids
        .into_iter()
        .filter(|id| !state.hydration_media.contains_key(id))
        .collect();
    if !missing_media.is_empty() {
        state.hydration_media.extend(
            crate::storage::fields::load_media_hydration_map_in_conn(conn, &missing_media).await?,
        );
    }

    Ok(())
}

/// Resolve references once for the entire parent AST. Missing IDs (for
/// example after an explicit force-delete) produce safe public omissions and
/// preview diagnostics. A Pattern cannot pull a second Pattern or ContentList.
pub async fn resolve_patterns_in_tx_with_budget(
    tx: &mut Transaction<'_, Sqlite>,
    state: &mut BuilderRenderContext,
    document: &BuilderDocument,
    is_preview: bool,
    budget: &mut page::RenderBudget,
    cache: &page::BlockTemplateCache,
) -> ApiResult<page::PatternResults> {
    let schemas = state.unified_block_schemas()?;
    let instances = pattern_ref_instances_with_schemas(document, &schemas);
    if instances.len() > opendash_core::builder::MAX_BUILDER_PATTERN_INSTANCES {
        return Err(ApiError::bad_request(
            "Builder exceeds 16 Pattern references.",
        ));
    }
    let ids: BTreeSet<i64> = instances.iter().map(|(_, pid)| *pid).collect();
    let rows = load_pattern_rows_in_tx(tx, &ids).await?;
    let mut results = page::PatternResults::new();
    let mut parsed = BTreeMap::new();
    for (id, _slug, raw) in rows {
        let Some(raw) = raw else { continue };
        let Ok(doc) = BuilderDocument::from_json(raw.as_bytes()) else {
            continue;
        };
        if has_nested_pattern_or_list_with_schemas(&doc, &schemas) {
            continue;
        }
        parsed.insert(id, doc);
    }
    let mut expanded_count = node_count(document);
    for (_, pid) in &instances {
        if let Some(doc) = parsed.get(pid) {
            expanded_count = expanded_count.saturating_add(node_count(doc));
        }
    }
    if expanded_count > opendash_core::builder::MAX_EXPANDED_BUILDER_NODES {
        return Err(ApiError::bad_request(
            "Expanded Pattern exceeds 1024 Builder nodes.",
        ));
    }
    let pattern_doc_refs: Vec<&BuilderDocument> = parsed.values().collect();
    if !pattern_doc_refs.is_empty() {
        hydrate_render_context_for_documents_in_conn(
            &mut **tx,
            state,
            &pattern_doc_refs,
            is_preview,
        )
        .await?;
    }
    for (node_id, pid) in instances {
        let Some(doc) = parsed.get(&pid) else {
            continue;
        };
        let forms = resolve_contact_forms_in_tx(tx, doc).await?;
        let scope = format!("pat-{pid}-{node_id}");
        let rendered = page::render_builder_with_budget(
            state,
            doc,
            is_preview,
            &ListResults::new(),
            &forms,
            None,
            &page::PatternResults::new(),
            true,
            RenderAnnotationScope::None,
            &scope,
            budget,
            cache,
        )?;
        results.insert(node_id, rendered);
    }
    Ok(results)
}

pub(in crate::storage) async fn validate_entry_builder_document(
    tx: &mut Transaction<'_, Sqlite>,
    current: &EntryBuilderSnapshot,
    role: FieldRole,
    document: &BuilderDocument,
) -> ApiResult<BuilderRenderContext> {
    if current.opened.type_key == "pattern" && role != FieldRole::Admin {
        return Err(ApiError::forbidden("Pattern Builder requires Admin."));
    }
    let state =
        read_render_context_for_locale_tx(tx, Some(current.locale.as_str()), false, None).await?;
    let schemas = state.unified_block_schemas()?;
    let ctx = if current.opened.type_key == "pattern" {
        BuilderContextTarget::Pattern
    } else {
        BuilderContextTarget::EntryBody {
            content_type: current.opened.type_key.clone(),
        }
    };
    document
        .validate_for_save(&schemas, &ctx, current.opened.document.as_ref())
        .map_err(|err| ApiError::bad_request(err.to_string()))?;

    let allowed: Vec<String> = current
        .schema
        .effective_fields
        .keys()
        .filter(|key| visible_builder_field(&current.schema, key, role))
        .cloned()
        .collect();

    if current.opened.type_key == "pattern"
        && has_nested_pattern_or_list_with_schemas(document, &schemas)
    {
        return Err(ApiError::bad_request(
            "Pattern cannot contain PatternRef or ContentList (depth = 1).",
        ));
    }
    let refs = pattern_ids_with_schemas(document, &schemas);
    let rows = load_pattern_rows_in_tx(tx, &refs).await?;
    let found: BTreeSet<i64> = rows.into_iter().map(|(id, _, _)| id).collect();
    let previous_refs = current
        .opened
        .document
        .as_ref()
        .map(|prev_doc| pattern_ids_with_schemas(prev_doc, &schemas))
        .unwrap_or_default();
    for id in refs {
        if !found.contains(&id) && !previous_refs.contains(&id) {
            return Err(ApiError::bad_request("Referenced Pattern does not exist."));
        }
    }

    crate::storage::fields::validate_document_block_refs_in_conn(
        &mut **tx,
        document,
        current.opened.document.as_ref(),
        &schemas,
        None,
        Some(&allowed),
    )
    .await?;

    validate_media_refs_in_tx(&mut **tx, document).await?;
    Ok(state)
}
