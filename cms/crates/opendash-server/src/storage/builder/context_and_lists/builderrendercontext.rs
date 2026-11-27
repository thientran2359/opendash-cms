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
        }
    }
}

/// Only public-safe display text and a canonical, storage-resolved href leave
/// the list resolver. IDs remain stable when a slug or permalink changes.
#[derive(Debug, Clone)]
pub struct ListItem {
    pub entry_id: i64,
    pub title: String,
    pub slug: String,
    pub href: Option<String>,
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

/// Use the same SQLite snapshot as the saved document/field bindings.
pub async fn resolve_contact_forms_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    document: &BuilderDocument,
) -> ApiResult<crate::page::ContactFormResults> {
    let mut results = crate::page::ContactFormResults::new();
    for section in &document.sections {
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    if let ElementContent::Known(KnownElement::ContactForm(props)) =
                        &element.content
                    {
                        let rec =
                            crate::storage::contact::get_form_in_conn(&mut *tx, props.form_id)
                                .await?;
                        results.insert(element.id.clone(), rec);
                    }
                }
            }
        }
    }
    Ok(results)
}

pub async fn resolve_lists_in_tx_for_locale(
    tx: &mut Transaction<'_, Sqlite>,
    document: &BuilderDocument,
    pages: &BTreeMap<String, i64>,
    render_locale: Option<&str>,
) -> ApiResult<ListResults> {
    let active_locale = match render_locale {
        Some(raw) => {
            crate::storage::settings::resolve_requested_locale_in_conn(&mut **tx, Some(raw)).await?
        }
        None => crate::storage::settings::get_default_locale_code_in_conn(&mut **tx).await?,
    };
    let mut results = ListResults::new();
    for section in &document.sections {
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    let ElementContent::Known(KnownElement::List(props)) = &element.content else {
                        continue;
                    };
                    // Defensive cap even for corrupt historical stored data.
                    if results.len() >= 8 {
                        return Err(ApiError::bad_request("Builder vượt giới hạn 8 list."));
                    }
                    let page = pages
                        .get(&element.id)
                        .copied()
                        .filter(|p| (1..=100).contains(p))
                        .unwrap_or(1);
                    let filters = EntryFilters {
                        category_id: props.category_id,
                        tag_id: props.tag_id,
                        taxonomy_key: props.taxonomy_key.clone(),
                        term_id: props.term_id,
                        include_descendants: props.include_descendants,
                        locale: Some(active_locale.clone()),
                    };
                    let sort = match props.sort {
                        opendash_core::builder::ListSort::IdDesc => PublishedListSort::IdDesc,
                        opendash_core::builder::ListSort::IdAsc => PublishedListSort::IdAsc,
                    };
                    let result = entries::list_published_page_in_tx(
                        tx,
                        &props.type_key,
                        page,
                        i64::from(props.page_size),
                        sort,
                        filters,
                    )
                    .await?;
                    let rendered = match result {
                        PublishedListResult::Page(result) => {
                            // Keep every URL lookup in the very same read
                            // snapshot as the list page and published guards.
                            let mut items = Vec::with_capacity(result.entries.len());
                            for entry in result.entries {
                                let href = if crate::storage::seo::resolve_public_entry_path_in_tx(
                                    &mut **tx, entry.id,
                                )
                                .await?
                                .is_some()
                                {
                                    crate::storage::permalinks::canonical_entry_path_in_tx(
                                        &mut **tx, entry.id,
                                    )
                                    .await?
                                } else {
                                    None
                                };
                                items.push(ListItem {
                                    entry_id: entry.id,
                                    title: entry.title,
                                    slug: entry.slug,
                                    href,
                                });
                            }
                            ResolvedList::Page {
                                page,
                                out_of_range: page > 1 && items.is_empty(),
                                items,
                                has_next: result.has_next,
                            }
                        }
                        PublishedListResult::MissingConfig(_) => ResolvedList::MissingConfig,
                    };
                    results.insert(element.id.clone(), rendered);
                }
            }
        }
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
    Ok(BuilderRenderContext {
        declared_blocks,
        site_options,
        options,
        menus,
        timezone,
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
    let ids = pattern_ids_with_schemas(document, &schemas);
    let rows = load_pattern_rows_in_tx(tx, &ids).await?;
    let mut results = page::PatternResults::new();
    let mut parsed = BTreeMap::new();
    for (id, slug, raw) in rows {
        let Some(raw) = raw else { continue };
        let Ok(doc) = BuilderDocument::from_json(raw.as_bytes()) else {
            continue;
        };
        if has_nested_pattern_or_list_with_schemas(&doc, &schemas) {
            continue;
        }
        parsed.insert(id, (slug, doc));
    }
    let mut expanded_count = node_count(document);
    for section in &document.sections {
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    if let ElementContent::Known(KnownElement::PatternRef(props)) = &element.content
                    {
                        if let Some((_, doc)) = parsed.get(&props.pattern_id) {
                            expanded_count = expanded_count.saturating_add(node_count(doc));
                        }
                    }
                }
            }
        }
    }
    if expanded_count > 1024 {
        return Err(ApiError::bad_request(
            "Expanded Pattern exceeds 1024 Builder nodes.",
        ));
    }
    let pattern_doc_refs: Vec<&BuilderDocument> = parsed.values().map(|(_, doc)| doc).collect();
    if !pattern_doc_refs.is_empty() {
        hydrate_render_context_for_documents_in_conn(
            &mut **tx,
            state,
            &pattern_doc_refs,
            is_preview,
        )
        .await?;
    }
    for (id, (slug, doc)) in parsed {
        let bindings = builder_binding::resolve_in_tx_with_route(
            tx,
            &doc,
            is_preview,
            Some(("pattern", &slug)),
        )
        .await?;
        hydrate_binding_media_in_conn(&mut **tx, state, &doc, &bindings).await?;
        let forms = resolve_contact_forms_in_tx(tx, &doc).await?;
        let scope = format!("pat_{id}");
        let rendered = page::render_builder_with_budget(
            state,
            &doc,
            is_preview,
            &bindings,
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
        results.insert(id, rendered);
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
    document
        .validate_for_save_with_schemas(&schemas, current.opened.document.as_ref())
        .map_err(|err| ApiError::bad_request(err.to_string()))?;

    // Validate this entry's field keys using its effective field ACL.
    let allowed: Vec<String> = current
        .schema
        .effective_fields
        .keys()
        .filter(|key| visible_builder_field(&current.schema, key, role))
        .cloned()
        .collect();
    // Content Builder is always a singular route; only the Page CPT uses
    // the Page role. Other CPTs bind to their own singular route context.
    let route_role = if current.opened.type_key == "page" {
        "page"
    } else {
        "single"
    };
    document
        .validate_route_context_for_role(Some(route_role), Some(&allowed))
        .map_err(|err| ApiError::bad_request(err.to_string()))?;

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

    let mut exact_schemas = BTreeMap::new();
    for section in &document.sections {
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    let ElementContent::Known(known) = &element.content else {
                        continue;
                    };
                    if matches!(
                        known,
                        KnownElement::ContentSlot(_) | KnownElement::TemplatePart(_)
                    ) {
                        return Err(ApiError::bad_request(
                            "Template-only element in Content Builder.",
                        ));
                    }
                    let Some(binding) = known.binding() else {
                        continue;
                    };
                    match &binding.source {
                        BindingSource::RouteContext { field_key } if field_key == "cover_url" => {
                            // The existing resolver picks the first public base image.
                            if let Some(entry) =
                                current.schema.effective_fields.values().find(|e| {
                                    e.definition.field_type
                                        == opendash_core::field_schema::FieldType::Media
                                        && e.definition.effective_acl().2 == PublicExposure::Public
                                })
                                && !visible_builder_field(
                                    &current.schema,
                                    &entry.definition.key,
                                    role,
                                )
                            {
                                return Err(ApiError::forbidden(
                                    "Builder cover field is not accessible.",
                                ));
                            }
                        }
                        BindingSource::Field { key } => {
                            // Exact-entry bindings may reference other content types.
                            // Missing/inactive sources keep normal unavailable fallback;
                            // private or actor-inaccessible fields must not be rendered.
                            if !exact_schemas.contains_key(&binding.type_key) {
                                let schema =
                                    crate::storage::content::load_effective_content_schema_in_tx(
                                        &mut **tx,
                                        &binding.type_key,
                                    )
                                    .await?;
                                exact_schemas.insert(binding.type_key.clone(), schema);
                            }
                            if let Some(Some(schema)) = exact_schemas.get(&binding.type_key)
                                && schema.effective_fields.contains_key(key)
                                && !visible_builder_field(schema, key, role)
                            {
                                return Err(ApiError::forbidden(
                                    "Builder binding field is not accessible.",
                                ));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    validate_media_refs_in_tx(&mut **tx, document).await?;
    Ok(state)
}
