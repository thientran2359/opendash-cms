//! Site-owned Builder homepage. All persistence, version CAS and media ACL
//! writes stay in this module; preview/open and public snapshot are read-only.
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use opendash_core::{
    block_schema::RegisteredBlockSchema,
    builder::{
        BUILDER_SCHEMA_VERSION, BindingSource, BuilderDocument, ColumnNode, ElementContent,
        ElementNode, KnownElement, LegacyBlockProps, RowNode, SectionNode,
    },
    builder_blocks::BlockDiagnostic,
    content::{
        ContentFieldType, ContentMode, ContentPublication, ContentSupport, EffectiveContentSchema,
        EffectiveFieldStatus, FieldRole, PublicExposure,
    },
    id::valid_namespaced_block_id,
    registry::{DeclarationState, ProviderStatus, RegistryCatalog, ResourceOwner},
    theme::BlockDescriptor,
};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{QueryBuilder, Sqlite, SqlitePool, Transaction};

use crate::{
    auth::{Role, SessionContext},
    error::{ApiError, ApiResult},
    hooks::{ContentBeforeSaveVeto, HookDispatcher},
    page::{self, RenderAnnotationScope, RenderedLayout},
    storage::media::valid_media_id,
    storage::{
        builder_binding::{self, BindingResults},
        entries::{self, EntryFilters, PublishedListResult, PublishedListSort},
    },
    theme::{Block, Fields},
};

#[derive(Debug, Clone)]
pub struct ThemeDefinition {
    pub fields: Fields,
    pub template: String,
}

#[derive(Default)]
pub struct HomeState {
    pub version: i64,
    pub saved: Option<BuilderDocument>,
    pub legacy: Option<Block>,
    pub themes: BTreeMap<String, ThemeDefinition>,
    pub declared_blocks: BTreeMap<String, crate::storage::page::RegisteredBlockDef>,
    pub active_id: Option<String>,
    pub active_alias: Option<String>,
    pub active_template: Option<String>,
    pub site_options: BTreeMap<String, serde_json::Value>,
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

pub async fn resolve_contact_forms(
    db: &SqlitePool,
    document: &BuilderDocument,
) -> ApiResult<crate::page::ContactFormResults> {
    let mut tx = db.begin().await?;
    let res = resolve_contact_forms_in_tx(&mut tx, document).await?;
    tx.commit().await?;
    Ok(res)
}

pub async fn resolve_lists_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    document: &BuilderDocument,
    pages: &BTreeMap<String, i64>,
) -> ApiResult<ListResults> {
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

impl HomeState {
    pub fn source(&self) -> &'static str {
        if self.saved.is_some() {
            "layout"
        } else if self.legacy.is_some() {
            "legacy"
        } else {
            "empty"
        }
    }

    pub fn document(&self) -> BuilderDocument {
        self.saved.clone().unwrap_or_else(|| {
            self.legacy
                .as_ref()
                .map(project_legacy)
                .unwrap_or_else(BuilderDocument::empty)
        })
    }

    pub fn descriptors(&self) -> ApiResult<RegistryCatalog<BlockDescriptor>> {
        let mut catalog = RegistryCatalog::new();
        for (id, definition) in &self.themes {
            if !valid_namespaced_block_id(id) {
                return Err(ApiError::internal("Invalid installed block ID"));
            }
            let namespace = id.split_once('/').expect("validated namespaced ID").0;
            catalog
                .register(
                    id.clone(),
                    ResourceOwner::Theme(namespace.into()),
                    BlockDescriptor {
                        id: id.clone(),
                        fields: definition.fields.clone(),
                    },
                )
                .map_err(|_| ApiError::internal("Invalid installed block registry"))?;
        }
        Ok(catalog)
    }

    pub fn unified_block_schemas(
        &self,
    ) -> ApiResult<RegistryCatalog<opendash_core::block_schema::RegisteredBlockSchema>> {
        use opendash_core::{block_schema::upcast_legacy_descriptor, registry::OwnedDescriptor};
        let mut catalog = RegistryCatalog::new();
        for (block_id, def) in &self.declared_blocks {
            if !valid_namespaced_block_id(block_id) {
                return Err(ApiError::internal("Invalid declared block ID"));
            }
            catalog.insert_loaded(
                block_id.clone(),
                OwnedDescriptor {
                    owner: def.owner.clone(),
                    status: def.status,
                    declaration_state: def.declaration_state,
                    descriptor: def.schema.clone(),
                },
            );
        }
        for (id, definition) in &self.themes {
            if catalog.get(id).is_some() {
                continue;
            }
            if !valid_namespaced_block_id(id) {
                return Err(ApiError::internal("Invalid installed block ID"));
            }
            let namespace = id.split_once('/').expect("validated namespaced ID").0;
            let legacy_desc = BlockDescriptor {
                id: id.clone(),
                fields: definition.fields.clone(),
            };
            catalog.insert_loaded(
                id.clone(),
                OwnedDescriptor {
                    owner: ResourceOwner::Theme(namespace.into()),
                    status: ProviderStatus::Active,
                    declaration_state: DeclarationState::Declared,
                    descriptor: upcast_legacy_descriptor(&legacy_desc),
                },
            );
        }
        Ok(catalog)
    }

    /// Do not trust prior-state information from the client. This method uses
    /// either persisted JSON or a deterministic, read-only M3 projection.
    pub fn validate(&self, document: &BuilderDocument) -> ApiResult<()> {
        document
            .validate_for_save_with_schemas(&self.unified_block_schemas()?, Some(&self.document()))
            .map_err(|error| ApiError::bad_request(error.to_string()))
    }
}

fn project_legacy(block: &Block) -> BuilderDocument {
    // Stable across GET, theme switches, and restarts. Position and original
    // block identifier provide the seed; field values never affect node IDs.
    let digest = Sha256::digest(format!("0/0/0/0:{}", block.kind).as_bytes());
    let suffix = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    BuilderDocument {
        schema_version: BUILDER_SCHEMA_VERSION,
        sections: vec![SectionNode {
            id: format!("legacy-section-{suffix}"),
            meta: None,
            responsive: None,
            style: None,
            children: vec![RowNode {
                id: format!("legacy-row-{suffix}"),
                meta: None,
                responsive: None,
                children: vec![ColumnNode {
                    id: format!("legacy-column-{suffix}"),
                    meta: None,
                    responsive: None,
                    children: vec![ElementNode {
                        id: format!("legacy-element-{suffix}"),
                        meta: None,
                        content: ElementContent::Known(KnownElement::Legacy(LegacyBlockProps {
                            block_id: block.kind.clone(),
                            values: block.values.clone(),
                        })),
                    }],
                }],
            }],
        }],
    }
}

pub(crate) async fn read_home_tx(tx: &mut Transaction<'_, Sqlite>) -> ApiResult<HomeState> {
    let (version, raw): (i64, Option<String>) =
        sqlx::query_as("SELECT version, document_json FROM builder_homepage WHERE id = 1")
            .fetch_one(&mut **tx)
            .await?;
    let saved = raw
        .map(|raw| BuilderDocument::from_json(raw.as_bytes()))
        .transpose()
        .map_err(|_| ApiError::internal("Stored Builder document is not readable"))?;
    let legacy: Option<(String, String)> =
        sqlx::query_as("SELECT block_id, values_json FROM home_page WHERE id = 1")
            .fetch_optional(&mut **tx)
            .await?;
    let legacy = legacy
        .map(|(kind, raw)| -> Result<Block, serde_json::Error> {
            Ok(Block {
                kind,
                values: serde_json::from_str(&raw)?,
            })
        })
        .transpose()?;
    let definitions: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id, fields_json, template FROM theme_blocks ORDER BY id")
            .fetch_all(&mut **tx)
            .await?;
    let mut themes = BTreeMap::new();
    for (id, fields, template) in definitions {
        themes.insert(
            id,
            ThemeDefinition {
                fields: serde_json::from_str(&fields)?,
                template,
            },
        );
    }
    let (active_id, active_alias, active_template): (
        Option<String>,
        Option<String>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT r.legacy_block_id, t.compatible_block_id, t.template
         FROM theme_active_selection a
         LEFT JOIN theme_revisions r ON r.revision_id = a.revision_id
         LEFT JOIN theme_blocks t ON t.id = r.legacy_block_id
         WHERE a.id = 1",
    )
    .fetch_one(&mut **tx)
    .await?;
    let declared_blocks = crate::storage::page::load_declared_blocks_in_conn(&mut **tx).await?;
    let site_options =
        crate::storage::settings::get_active_merged_public_options_in_conn(&mut **tx, false)
            .await?;
    Ok(HomeState {
        version,
        saved,
        legacy,
        themes,
        declared_blocks,
        active_id,
        active_alias,
        active_template,
        site_options,
    })
}

pub async fn read(db: &SqlitePool) -> ApiResult<HomeState> {
    // All fields and the active theme are drawn from a consistent SQLite read
    // snapshot, even if another request activates a theme during public render.
    let mut tx = db.begin().await?;
    let state = read_home_tx(&mut tx).await?;
    tx.commit().await?;
    Ok(state)
}

/// Public reads document, bound fields and published list rows from *one*
/// SQLite snapshot; the public path cannot mix pre/post-publish revisions.
pub async fn read_public_resolved(
    db: &SqlitePool,
    pages: &BTreeMap<String, i64>,
) -> ApiResult<(HomeState, BindingResults, ListResults)> {
    let mut tx = db.begin().await?;
    let state = read_home_tx(&mut tx).await?;
    let (bindings, lists) = if let Some(document) = &state.saved {
        (
            builder_binding::resolve_in_tx(&mut tx, document, false).await?,
            resolve_lists_in_tx(&mut tx, document, pages).await?,
        )
    } else {
        (BindingResults::new(), ListResults::new())
    };
    tx.commit().await?;
    Ok((state, bindings, lists))
}

/// Admin Preview queries published-only lists even when a draft Post/Page/CPT
/// may be authorized for exact-entry Field Binding.
pub async fn resolve_preview(
    db: &SqlitePool,
    document: &BuilderDocument,
) -> ApiResult<(BindingResults, ListResults)> {
    let mut tx = db.begin().await?;
    let bindings = builder_binding::resolve_in_tx(&mut tx, document, true).await?;
    let lists = resolve_lists_in_tx(&mut tx, document, &BTreeMap::new()).await?;
    tx.commit().await?;
    Ok((bindings, lists))
}

pub async fn legacy_page(db: &SqlitePool) -> ApiResult<Option<Block>> {
    // Single SELECT prevents a /page check against the new source racing with
    // a separate legacy read under a different SQLite snapshot.
    let (raw, kind, values): (Option<String>, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT b.document_json, p.block_id, p.values_json
             FROM builder_homepage b LEFT JOIN home_page p ON p.id = 1
             WHERE b.id = 1",
    )
    .fetch_one(db)
    .await?;
    if raw.is_some() {
        return Err(ApiError::conflict(
            "Homepage đã dùng Builder dạng cây. Hãy mở /api/admin/builder/homepage.",
        ));
    }
    match (kind, values) {
        (Some(kind), Some(raw)) => Ok(Some(Block {
            kind,
            values: serde_json::from_str(&raw)?,
        })),
        _ => Ok(None),
    }
}

pub fn image_ids(document: &BuilderDocument) -> BTreeSet<&str> {
    let mut ids = BTreeSet::new();
    for section in &document.sections {
        if let Some(bg_media) = section
            .style
            .as_ref()
            .and_then(|s| s.background_media_id.as_deref())
            .filter(|id| !id.is_empty())
        {
            ids.insert(bg_media);
        }
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    if let ElementContent::Known(KnownElement::Image(image)) = &element.content {
                        if !image.binding.as_ref().is_some_and(|b| {
                            b.target == opendash_core::builder::BindingTarget::MediaId
                        }) {
                            ids.insert(image.media_id.as_str());
                        }
                    }
                }
            }
        }
    }
    ids
}

pub fn video_ids(document: &BuilderDocument) -> BTreeSet<&str> {
    let mut ids = BTreeSet::new();
    for section in &document.sections {
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    if let ElementContent::Known(KnownElement::Video(video)) = &element.content {
                        ids.insert(video.media_id.as_str());
                    }
                }
            }
        }
    }
    ids
}

pub fn poster_image_ids(document: &BuilderDocument) -> BTreeSet<&str> {
    let mut ids = BTreeSet::new();
    for section in &document.sections {
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    if let ElementContent::Known(KnownElement::Video(video)) = &element.content
                        && let Some(poster) = video.poster_image_id.as_deref()
                    {
                        ids.insert(poster);
                    }
                }
            }
        }
    }
    ids
}

/// Public ACL projection of a Builder document. Unknown and inactive block
/// schemas never contribute media refs; raw saved values remain untouched so
/// the separate delete-pin scanner can still protect their assets.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct DocumentMediaRefs {
    pub image_ids: BTreeSet<String>,
    pub video_ids: BTreeSet<String>,
    pub poster_image_ids: BTreeSet<String>,
    pub any_media_ids: BTreeSet<String>,
}

pub(crate) fn extract_document_media_refs(
    document: &BuilderDocument,
    schemas: &RegistryCatalog<RegisteredBlockSchema>,
) -> DocumentMediaRefs {
    let mut refs = DocumentMediaRefs {
        image_ids: image_ids(document).into_iter().map(str::to_owned).collect(),
        video_ids: video_ids(document).into_iter().map(str::to_owned).collect(),
        poster_image_ids: poster_image_ids(document)
            .into_iter()
            .map(str::to_owned)
            .collect(),
        ..DocumentMediaRefs::default()
    };
    for section in &document.sections {
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    let ElementContent::Known(KnownElement::PluginBlock(props)) = &element.content
                    else {
                        continue;
                    };
                    let Some(entry) = schemas.get(&props.block_id) else {
                        continue;
                    };
                    if entry.status != ProviderStatus::Active
                        || entry.declaration_state != DeclarationState::Declared
                        || entry.descriptor.legacy_mode
                    {
                        continue;
                    }
                    let collected = entry.descriptor.schema.collect_media_ids(&props.values);
                    refs.image_ids.extend(collected.image_ids);
                    refs.video_ids.extend(collected.video_ids);
                    refs.any_media_ids.extend(collected.any_ids);
                }
            }
        }
    }
    refs
}

/// Resolve media-kind 'any' against the same SQLite snapshot as the write.
/// The returned sets are validated, active assets and safe to project into ACL.
pub(crate) async fn resolve_document_media_refs_in_conn(
    conn: &mut sqlx::SqliteConnection,
    document: &BuilderDocument,
) -> ApiResult<DocumentMediaRefs> {
    let schemas = crate::storage::page::load_unified_block_catalog_in_conn(conn).await?;
    let mut refs = extract_document_media_refs(document, &schemas);
    for (expected_kind, ids) in [
        ("image", &refs.image_ids),
        ("image", &refs.poster_image_ids),
        ("video", &refs.video_ids),
    ] {
        for id in ids {
            if !valid_media_id(id) {
                return Err(ApiError::bad_request("Invalid Builder media ID"));
            }
            let exists: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM media_assets WHERE id = ? AND kind = ? AND lifecycle_state = 'active')",
            )
            .bind(id)
            .bind(expected_kind)
            .fetch_one(&mut *conn)
            .await?;
            if exists == 0 {
                return Err(ApiError::bad_request(
                    "Builder media không tồn tại hoặc không đúng loại trong Media Library.",
                ));
            }
        }
    }
    for id in std::mem::take(&mut refs.any_media_ids) {
        if !valid_media_id(&id) {
            return Err(ApiError::bad_request("Invalid Builder media ID"));
        }
        let kind: Option<String> = sqlx::query_scalar(
            "SELECT kind FROM media_assets WHERE id = ? AND lifecycle_state = 'active'",
        )
        .bind(&id)
        .fetch_optional(&mut *conn)
        .await?;
        match kind.as_deref() {
            Some("image") => {
                refs.image_ids.insert(id);
            }
            Some("video") => {
                refs.video_ids.insert(id);
            }
            _ => {
                return Err(ApiError::bad_request(
                    "Builder media không tồn tại hoặc không đúng loại trong Media Library.",
                ));
            }
        }
    }
    Ok(refs)
}

/// A Pattern is a private, single-hop Builder document. References are stored
/// separately so delete guards and public media ACL never parse client JSON.
pub fn pattern_ids(document: &BuilderDocument) -> BTreeSet<i64> {
    let mut ids = BTreeSet::new();
    for section in &document.sections {
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    if let ElementContent::Known(KnownElement::PatternRef(pattern)) =
                        &element.content
                    {
                        ids.insert(pattern.pattern_id);
                    }
                }
            }
        }
    }
    ids
}

fn node_count(document: &BuilderDocument) -> usize {
    document
        .sections
        .iter()
        .map(|section| {
            1 + section
                .children
                .iter()
                .map(|row| {
                    1 + row
                        .children
                        .iter()
                        .map(|col| 1 + col.children.len())
                        .sum::<usize>()
                })
                .sum::<usize>()
        })
        .sum()
}

fn has_nested_pattern_or_list(document: &BuilderDocument) -> bool {
    document.sections.iter().any(|section| {
        section.children.iter().any(|row| {
            row.children.iter().any(|column| {
                column
                    .children
                    .iter()
                    .any(|element| match &element.content {
                        ElementContent::Known(
                            KnownElement::PatternRef(_) | KnownElement::List(_),
                        ) => true,
                        ElementContent::Unresolved(raw) => matches!(
                            raw.get("type").and_then(Value::as_str),
                            Some("core/pattern_ref" | "core/list")
                        ),
                        _ => false,
                    })
            })
        })
    })
}

/// One batched query inside the caller's read/write transaction, maximum 16
/// distinct IDs. Invalid or missing Pattern data is not rendered publicly.
async fn load_pattern_rows_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    ids: &BTreeSet<i64>,
) -> ApiResult<Vec<(i64, String, Option<String>)>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    if ids.len() > 16 || ids.iter().any(|id| *id <= 0) {
        return Err(ApiError::bad_request(
            "Builder exceeds 16 Pattern references.",
        ));
    }
    let mut sql = QueryBuilder::<Sqlite>::new(
        "SELECT e.id, e.slug, d.document_json FROM content_entries e \
         JOIN content_types t ON t.key = e.content_type_key \
         LEFT JOIN entry_builder_documents d ON d.entry_id = e.id \
         WHERE e.content_type_key = 'pattern' AND t.owner_kind = 'core' \
           AND t.provider_status = 'active' AND t.declaration_state = 'declared' \
           AND e.id IN (",
    );
    {
        let mut list = sql.separated(", ");
        for id in ids {
            list.push_bind(id);
        }
        list.push_unseparated(")");
    }
    Ok(sql.build_query_as().fetch_all(&mut **tx).await?)
}

/// Resolve references once for the entire parent AST. Missing IDs (for
/// example after an explicit force-delete) produce safe public omissions and
/// preview diagnostics. A Pattern cannot pull a second Pattern or ContentList.
pub async fn resolve_patterns_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    state: &HomeState,
    document: &BuilderDocument,
    is_preview: bool,
) -> ApiResult<page::PatternResults> {
    let mut budget = page::RenderBudget::new();
    resolve_patterns_in_tx_with_budget(
        tx,
        state,
        document,
        is_preview,
        &mut budget,
        page::BlockTemplateCache::global(),
    )
    .await
}

pub async fn resolve_patterns_in_tx_with_budget(
    tx: &mut Transaction<'_, Sqlite>,
    state: &HomeState,
    document: &BuilderDocument,
    is_preview: bool,
    budget: &mut page::RenderBudget,
    cache: &page::BlockTemplateCache,
) -> ApiResult<page::PatternResults> {
    let ids = pattern_ids(document);
    let rows = load_pattern_rows_in_tx(tx, &ids).await?;
    let mut results = page::PatternResults::new();
    let mut parsed = BTreeMap::new();
    for (id, slug, raw) in rows {
        let Some(raw) = raw else { continue };
        let Ok(doc) = BuilderDocument::from_json(raw.as_bytes()) else {
            continue;
        };
        if has_nested_pattern_or_list(&doc) {
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
    for (id, (slug, doc)) in parsed {
        let bindings = builder_binding::resolve_in_tx_with_route(
            tx,
            &doc,
            is_preview,
            Some(("pattern", &slug)),
        )
        .await?;
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

pub async fn validate_media_refs_in_tx(
    conn: &mut sqlx::SqliteConnection,
    document: &BuilderDocument,
) -> ApiResult<()> {
    resolve_document_media_refs_in_conn(conn, document).await?;
    Ok(())
}

pub async fn check_preview_images(db: &SqlitePool, document: &BuilderDocument) -> ApiResult<()> {
    let mut conn = db.acquire().await?;
    validate_media_refs_in_tx(&mut conn, document).await
}

/// Run a render callback *inside* the SQLite writer transaction before commit.
/// CAS, current theme selection, content validation, media ACL replacement and
/// serialized JSON become visible to public requests together or not at all.
pub async fn save_and_render<T, F>(
    db: &SqlitePool,
    document: &BuilderDocument,
    expected_version: i64,
    render: F,
) -> ApiResult<(i64, T)>
where
    F: FnOnce(&HomeState, &BuilderDocument, &BindingResults, &ListResults) -> ApiResult<T>,
{
    let mut tx = db.begin().await?;
    // First query inside transaction reserves the writer and arbitrates CAS,
    // including races with /save, theme switches, and other Builder writers.
    let updated = sqlx::query(
        "UPDATE builder_homepage SET version = version + 1
         WHERE id = 1 AND version = ? AND version < 9223372036854775807",
    )
    .bind(expected_version)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::precondition_failed(
            "Homepage đã thay đổi ở một tab khác. Giữ bản nháp và tải lại phiên bản.",
        ));
    }
    let current = read_home_tx(&mut tx).await?;
    current.validate(document)?;
    let media_refs = resolve_document_media_refs_in_conn(&mut tx, document).await?;
    let bindings = builder_binding::resolve_in_tx(&mut tx, document, false).await?;
    let lists = resolve_lists_in_tx(&mut tx, document, &BTreeMap::new()).await?;
    let rendered = render(&current, document, &bindings, &lists)?;
    let bytes = document
        .to_json()
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let raw = String::from_utf8(bytes).expect("serde_json always produces UTF-8");
    sqlx::query("UPDATE builder_homepage SET document_json = ? WHERE id = 1")
        .bind(raw)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM builder_homepage_video_refs")
        .execute(&mut *tx)
        .await?;
    for id in media_refs.video_ids {
        sqlx::query(
            "INSERT INTO builder_homepage_video_refs (media_id, ref_role) VALUES (?, 'video')",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    for id in media_refs.poster_image_ids {
        sqlx::query(
            "INSERT INTO builder_homepage_video_refs (media_id, ref_role) VALUES (?, 'poster')",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("DELETE FROM builder_homepage_media")
        .execute(&mut *tx)
        .await?;
    for id in media_refs.image_ids {
        sqlx::query("INSERT INTO builder_homepage_media (media_id) VALUES (?)")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok((current.version, rendered))
}

#[cfg(test)]
#[path = "s03_media_acl_tests.rs"]
mod s03_media_acl_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projection_preserves_ids_fields_and_positions() {
        let block = Block {
            kind: "ocean/hero".into(),
            values: Fields::from([("title".into(), "First".into())]),
        };
        let projected = project_legacy(&block);
        projected
            .validate_for_save(&RegistryCatalog::new(), Some(&projected))
            .unwrap();
        let mut updated = block.clone();
        updated.values.insert("title".into(), "Second".into());
        let different = project_legacy(&updated);
        assert_eq!(projected.sections[0].id, different.sections[0].id);
        assert_eq!(
            projected.sections[0].children[0].children[0].children[0].id,
            different.sections[0].children[0].children[0].children[0].id
        );
    }

    #[tokio::test]
    async fn migrate_demo_entry_builder_docs_on_startup_cleans_non_pattern_and_keeps_pattern() {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();

        let media_id = "5".repeat(64);
        sqlx::query(
            "INSERT INTO media_assets(id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height)
             VALUES(?, 'image', 'demo.png', 'image/png', 'png', 100, 10, 10, 10, 10)",
        )
        .bind(&media_id)
        .execute(&db)
        .await
        .unwrap();

        let doc: BuilderDocument = serde_json::from_value(json!({
            "schema_version": 1,
            "sections": [{"id":"sec-1","children":[{"id":"row-1","children":[{"id":"col-1","children":[
                {"id":"hd-1","type":"core/heading","props":{"level":2,"text":"Demo migrated"}},
                {"id":"im-1","type":"core/image","props":{"media_id":media_id,"alt":"Demo"}}
            ]}]}]}]
        }))
        .unwrap();
        let doc_json = String::from_utf8(doc.to_json().unwrap()).unwrap();

        let page_id = sqlx::query(
            "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, content_mode)
             VALUES('page', 'demo-page', 'Demo Page', '{}', 'draft', 'builder')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query("UPDATE content_entries SET publication_status = 'published' WHERE id = ?")
            .bind(page_id)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO entry_builder_documents(entry_id, document_json, updated_at) VALUES(?, ?, datetime('now'))",
        )
        .bind(page_id)
        .bind(&doc_json)
        .execute(&db)
        .await
        .unwrap();

        let pattern_id = sqlx::query(
            "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, content_mode)
             VALUES('pattern', 'demo-pat', 'Demo Pattern', '{}', 'draft', 'builder')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query(
            "INSERT INTO entry_builder_documents(entry_id, document_json, updated_at) VALUES(?, ?, datetime('now'))",
        )
        .bind(pattern_id)
        .bind(&doc_json)
        .execute(&db)
        .await
        .unwrap();

        let migrated = migrate_demo_entry_builder_docs_on_startup(&db)
            .await
            .unwrap();
        assert_eq!(migrated, 1);
        assert_eq!(
            migrate_demo_entry_builder_docs_on_startup(&db)
                .await
                .unwrap(),
            0
        );

        let page_values: String =
            sqlx::query_scalar("SELECT values_json FROM content_entries WHERE id = ?")
                .bind(page_id)
                .fetch_one(&db)
                .await
                .unwrap();
        let body = serde_json::from_str::<Value>(&page_values).unwrap()["body"]
            .as_str()
            .unwrap()
            .to_owned();
        let parsed =
            opendash_core::builder_blocks::parse_blocks_to_builder_document(&body).unwrap();
        assert_eq!(parsed.document, doc);

        let non_pattern_left: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM entry_builder_documents WHERE entry_id = ?")
                .bind(page_id)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(non_pattern_left, 0);

        let pattern_left: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM entry_builder_documents WHERE entry_id = ?")
                .bind(pattern_id)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(pattern_left, 1);

        let media_refs: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM entry_builder_media_refs WHERE entry_id = ? AND media_id = ?",
        )
        .bind(page_id)
        .bind(&media_id)
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(media_refs, 1);
    }

    #[tokio::test]
    async fn entry_builder_enforces_body_min_edit_role_and_private_exposure_media_acl() {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();

        sqlx::query(
            r#"INSERT INTO content_types(key, definition_json, owner_kind)
               VALUES('secret_doc', '{"key":"secret_doc","label":"Secret","fields":[{"key":"body","label":"Body","type":"rich_text","required":false,"min_view_role":"admin","min_edit_role":"admin","public_exposure":"private"}],"settings":{"public":true,"has_archive":false,"publication":"draft_publish","supports":["editor","builder"]}}', 'user')"#,
        )
        .execute(&db)
        .await
        .unwrap();

        let admin_id = sqlx::query(
            "INSERT INTO users(username, password_hash, role) VALUES('admin1', 'hash', 'admin')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let editor_id = sqlx::query(
            "INSERT INTO users(username, password_hash, role) VALUES('editor1', 'hash', 'editor')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let future = crate::auth::current_time_secs() + 3600;
        sqlx::query("INSERT INTO sessions(token_hash, user_id, csrf_token, expires_at) VALUES('adm_h', ?, 'c1', ?)")
            .bind(admin_id)
            .bind(future)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO sessions(token_hash, user_id, csrf_token, expires_at) VALUES('ed_h', ?, 'c2', ?)")
            .bind(editor_id)
            .bind(future)
            .execute(&db)
            .await
            .unwrap();

        let admin_ctx = SessionContext {
            user: crate::auth::AuthUser {
                id: admin_id,
                username: "admin1".into(),
                role: Role::Admin,
            },
            csrf_token: "c1".into(),
            token_hash: "adm_h".into(),
        };
        let editor_ctx = SessionContext {
            user: crate::auth::AuthUser {
                id: editor_id,
                username: "editor1".into(),
                role: Role::Editor,
            },
            csrf_token: "c2".into(),
            token_hash: "ed_h".into(),
        };

        let media_id = "6".repeat(64);
        sqlx::query(
            "INSERT INTO media_assets(id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height)
             VALUES(?, 'image', 'priv.png', 'image/png', 'png', 100, 10, 10, 10, 10)",
        )
        .bind(&media_id)
        .execute(&db)
        .await
        .unwrap();
        let entry_id = sqlx::query(
            "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status)
             VALUES('secret_doc', 'sec-1', 'Secret 1', '{}', 'published')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();

        assert_eq!(
            read_entry_builder(&db, &editor_ctx, "secret_doc", entry_id)
                .await
                .unwrap_err()
                .0,
            axum::http::StatusCode::FORBIDDEN
        );

        let doc: BuilderDocument = serde_json::from_value(json!({
            "schema_version": 1,
            "sections": [{"id":"sec-1","children":[{"id":"row-1","children":[{"id":"col-1","children":[
                {"id":"im-1","type":"core/image","props":{"media_id":media_id,"alt":"Secret"}}
            ]}]}]}]
        }))
        .unwrap();
        let plugins = std::sync::Arc::new(crate::plugin::PluginManager::new(db.clone(), false));
        let hooks = HookDispatcher::new(plugins);

        match save_entry_builder(&db, &editor_ctx, &hooks, "secret_doc", entry_id, 1, &doc).await {
            Err(EntryBuilderMutationError::Api(err)) => {
                assert_eq!(err.0, axum::http::StatusCode::FORBIDDEN);
            }
            other => panic!("expected 403 forbidden for Editor, got {other:?}"),
        }

        let saved = save_entry_builder(&db, &admin_ctx, &hooks, "secret_doc", entry_id, 1, &doc)
            .await
            .unwrap();
        assert_eq!(saved.version, 2);

        // Private `body` field must NOT grant public media access, while media remains delete-pinned.
        assert!(
            !crate::storage::public_content::media_is_referenced(&db, &media_id)
                .await
                .unwrap()
        );
        let mut conn = db.acquire().await.unwrap();
        let pins = crate::storage::media_refs::collect_pins_in_tx(&mut conn, &media_id)
            .await
            .unwrap();
        assert_eq!(pins.total_pins, 1);
        assert_eq!(pins.pins[0].source_kind, "content_entry");
    }
}

pub fn register_plugin_block_template(
    state: &mut HomeState,
    block_id: &str,
    template_csec: &str,
) -> ApiResult<()> {
    crate::template_sec::validate_template_csec(block_id, template_csec)
        .map_err(ApiError::bad_request)?;
    state.themes.insert(
        block_id.to_owned(),
        ThemeDefinition {
            fields: Fields::default(),
            template: template_csec.to_owned(),
        },
    );
    Ok(())
}

/// Builder documents are site-owned entry content. Their only version is the
/// enclosing content entry CAS token; shared site templates are independent.
#[derive(Debug, Clone, Serialize)]
pub struct EntryBuilderOpen {
    pub entry_id: i64,
    pub type_key: String,
    pub content_mode: ContentMode,
    pub version: i64,
    pub updated_at: String,
    pub document: Option<BuilderDocument>,
    pub publication_status: String,
    pub publication_policy: ContentPublication,
    pub diagnostics: Vec<BlockDiagnostic>,
    pub had_freeform_wrap: bool,
    pub had_recovery: bool,
}

struct EntryBuilderSnapshot {
    opened: EntryBuilderOpen,
    schema: EffectiveContentSchema,
    title: String,
    slug: String,
    values: BTreeMap<String, Value>,
    term_ids: Vec<i64>,
    raw_body: Option<String>,
    raw_body_has_blocks: bool,
    page_layout_key: Option<String>,
    custom_template_key: Option<String>,
}

fn actor_field_role(actor: &SessionContext) -> FieldRole {
    match actor.user.role {
        Role::Admin => FieldRole::Admin,
        Role::Editor => FieldRole::Editor,
    }
}

fn check_entry_body_edit_acl(
    schema: &EffectiveContentSchema,
    actor: &SessionContext,
) -> ApiResult<()> {
    let Some(body_field) = schema.effective_fields.get("body") else {
        return Err(ApiError::forbidden(
            "Loại nội dung không có trường 'body' hợp lệ cho Trình dựng trang.",
        ));
    };
    if body_field.definition.field_type != ContentFieldType::RichText {
        return Err(ApiError::forbidden(
            "Trường 'body' phải có kiểu rich_text để dùng Trình dựng trang.",
        ));
    }
    if actor_field_role(actor) < body_field.definition.min_edit_role {
        return Err(ApiError::forbidden(
            "Bạn không có quyền chỉnh sửa nội dung 'body'.",
        ));
    }
    Ok(())
}

fn body_allows_public_refs(schema: &EffectiveContentSchema) -> bool {
    schema.effective_fields.get("body").is_some_and(|fe| {
        fe.definition.field_type == ContentFieldType::RichText
            && fe.definition.public_exposure == PublicExposure::Public
            && fe.definition.min_view_role == FieldRole::Editor
    })
}

async fn entry_builder_snapshot(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    id: i64,
) -> ApiResult<EntryBuilderSnapshot> {
    // The deprecated Section CPT must never be reopened via the generic API.
    if type_key == "section" {
        return Err(ApiError::forbidden("Content Builder is not enabled."));
    }
    let schema = crate::storage::content::load_effective_content_schema_in_tx(&mut **tx, type_key)
        .await?
        .ok_or_else(|| ApiError::not_found("Content type not found."))?;
    if schema.base.status != ProviderStatus::Active
        || schema.base.declaration_state != DeclarationState::Declared
        || !schema
            .base
            .descriptor
            .settings
            .supports
            .contains(&ContentSupport::Builder)
    {
        return Err(ApiError::forbidden("Content Builder is not enabled."));
    }
    let (
        title,
        slug,
        values_json,
        mode,
        publication_status,
        version,
        updated_at,
        raw_doc,
        page_layout_key,
        custom_template_key,
    ): (
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = if type_key == "pattern" {
        let row: (
            String,
            String,
            String,
            String,
            String,
            i64,
            String,
            Option<String>,
        ) = sqlx::query_as(
            "SELECT e.title, e.slug, e.values_json, e.content_mode, e.publication_status, e.version, e.updated_at, d.document_json
               FROM content_entries e LEFT JOIN entry_builder_documents d ON d.entry_id = e.id
              WHERE e.id = ? AND e.content_type_key = ?",
        )
        .bind(id)
        .bind(type_key)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| ApiError::not_found("Content entry not found."))?;
        (
            row.0, row.1, row.2, row.3, row.4, row.5, row.6, row.7, None, None,
        )
    } else {
        let row: Option<(
            String,
            String,
            String,
            String,
            String,
            i64,
            String,
            Option<String>,
            Option<String>,
        )> = sqlx::query_as(
            "SELECT title, slug, values_json, content_mode, publication_status, version, updated_at,
                    page_layout_key, custom_template_key
               FROM content_entries
              WHERE id = ? AND content_type_key = ?",
        )
        .bind(id)
        .bind(type_key)
        .fetch_optional(&mut **tx)
        .await?;
        let (
            title,
            slug,
            values_json,
            mode,
            publication_status,
            version,
            updated_at,
            page_layout_key,
            custom_template_key,
        ) = row.ok_or_else(|| ApiError::not_found("Content entry not found."))?;
        (
            title,
            slug,
            values_json,
            mode,
            publication_status,
            version,
            updated_at,
            None,
            page_layout_key,
            custom_template_key,
        )
    };
    let content_mode = match mode.as_str() {
        "editor" => ContentMode::Editor,
        "builder" => ContentMode::Builder,
        _ => return Err(ApiError::internal("Invalid stored entry content mode.")),
    };
    let values: BTreeMap<String, Value> = serde_json::from_str(&values_json)?;
    let raw_body = values
        .get("body")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let raw_body_has_blocks = raw_body
        .as_deref()
        .is_some_and(opendash_core::builder_blocks::contains_blocks);
    let (document, diagnostics, had_freeform_wrap, had_recovery) = if type_key == "pattern" {
        let doc = raw_doc
            .map(|raw| BuilderDocument::from_json(raw.as_bytes()))
            .transpose()
            .map_err(|_| ApiError::internal("Stored entry Builder document is not readable."))?;
        (doc, Vec::new(), false, false)
    } else {
        match raw_body.as_deref() {
            Some(raw) if !raw.trim().is_empty() => {
                let parsed = opendash_core::builder_blocks::parse_blocks_lenient_or_recover(raw);
                (
                    Some(parsed.document),
                    parsed.diagnostics,
                    parsed.had_freeform_wrap,
                    parsed.had_recovery,
                )
            }
            _ => (None, Vec::new(), false, false),
        }
    };
    let publication_policy = schema.base.descriptor.settings.publication;
    let term_ids = entries::read_term_ids(tx, id).await?;
    Ok(EntryBuilderSnapshot {
        opened: EntryBuilderOpen {
            entry_id: id,
            type_key: type_key.to_owned(),
            content_mode,
            version,
            updated_at,
            document,
            publication_status,
            publication_policy,
            diagnostics,
            had_freeform_wrap,
            had_recovery,
        },
        schema,
        title,
        slug,
        values,
        term_ids,
        raw_body,
        raw_body_has_blocks,
        page_layout_key,
        custom_template_key,
    })
}

pub async fn read_entry_builder(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    id: i64,
) -> ApiResult<EntryBuilderOpen> {
    if type_key == "pattern" && !matches!(actor.user.role, Role::Admin) {
        return Err(ApiError::forbidden("Pattern Builder requires Admin."));
    }
    let mut tx = db.begin().await?;
    let current = entry_builder_snapshot(&mut tx, type_key, id).await?;
    if type_key != "pattern" {
        check_entry_body_edit_acl(&current.schema, actor)?;
    }
    tx.commit().await?;
    Ok(current.opened)
}

fn visible_builder_field(schema: &EffectiveContentSchema, key: &str, role: FieldRole) -> bool {
    schema.effective_fields.get(key).is_some_and(|field| {
        field.definition.min_view_role <= role
            && field.definition.public_exposure == PublicExposure::Public
            && field.effective_status == EffectiveFieldStatus::Active
    })
}

async fn validate_entry_builder_document(
    tx: &mut Transaction<'_, Sqlite>,
    current: &EntryBuilderSnapshot,
    role: FieldRole,
    document: &BuilderDocument,
) -> ApiResult<HomeState> {
    if current.opened.type_key == "pattern" && role != FieldRole::Admin {
        return Err(ApiError::forbidden("Pattern Builder requires Admin."));
    }
    let state = read_home_tx(tx).await?;
    document
        .validate_for_save_with_schemas(
            &state.unified_block_schemas()?,
            current.opened.document.as_ref(),
        )
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

    if current.opened.type_key == "pattern" && has_nested_pattern_or_list(document) {
        return Err(ApiError::bad_request(
            "Pattern cannot contain PatternRef or ContentList (depth = 1).",
        ));
    }
    let refs = pattern_ids(document);
    let rows = load_pattern_rows_in_tx(tx, &refs).await?;
    let found: BTreeSet<i64> = rows.into_iter().map(|(id, _, _)| id).collect();
    let previous_refs = current
        .opened
        .document
        .as_ref()
        .map(pattern_ids)
        .unwrap_or_default();
    for id in refs {
        if !found.contains(&id) && !previous_refs.contains(&id) {
            return Err(ApiError::bad_request("Referenced Pattern does not exist."));
        }
    }

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
                            if let Some(field) =
                                current.schema.base.descriptor.fields.iter().find(|f| {
                                    f.field_type == ContentFieldType::Image
                                        && f.public_exposure == PublicExposure::Public
                                })
                                && !visible_builder_field(&current.schema, &field.key, role)
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

async fn render_entry_builder_preview(
    tx: &mut Transaction<'_, Sqlite>,
    current: &EntryBuilderSnapshot,
    state: &HomeState,
    document: &BuilderDocument,
    shared_preview: bool,
) -> ApiResult<RenderedLayout> {
    render_entry_builder_preview_with_scope(
        tx,
        current,
        state,
        document,
        shared_preview,
        RenderAnnotationScope::None,
    )
    .await
}

async fn render_entry_builder_preview_with_scope(
    tx: &mut Transaction<'_, Sqlite>,
    current: &EntryBuilderSnapshot,
    state: &HomeState,
    document: &BuilderDocument,
    shared_preview: bool,
    annotation_scope: RenderAnnotationScope,
) -> ApiResult<RenderedLayout> {
    let mut budget = page::RenderBudget::new();
    render_entry_builder_preview_with_budget(
        tx,
        current,
        state,
        document,
        shared_preview,
        annotation_scope,
        &mut budget,
        page::BlockTemplateCache::global(),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn render_entry_builder_preview_with_budget(
    tx: &mut Transaction<'_, Sqlite>,
    current: &EntryBuilderSnapshot,
    state: &HomeState,
    document: &BuilderDocument,
    shared_preview: bool,
    annotation_scope: RenderAnnotationScope,
    budget: &mut page::RenderBudget,
    cache: &page::BlockTemplateCache,
) -> ApiResult<RenderedLayout> {
    let bindings = builder_binding::resolve_in_tx_with_route(
        tx,
        document,
        true,
        Some((&current.opened.type_key, &current.slug)),
    )
    .await?;
    let lists = resolve_lists_in_tx(tx, document, &BTreeMap::new()).await?;
    let forms = resolve_contact_forms_in_tx(tx, document).await?;
    let patterns =
        resolve_patterns_in_tx_with_budget(tx, state, document, true, budget, cache).await?;
    let parts = BTreeMap::new();
    let context = shared_preview.then_some(page::BuilderTemplateContext {
        content_html: None,
        parts: &parts,
    });
    let scope = if current.opened.type_key == "pattern" {
        format!("pat_{}", current.opened.entry_id)
    } else {
        "entry".to_owned()
    };
    page::render_builder_with_budget(
        state,
        document,
        true,
        &bindings,
        &lists,
        &forms,
        context.as_ref(),
        &patterns,
        false,
        annotation_scope,
        &scope,
        budget,
        cache,
    )
}

/// Shared-template preview uses the selected CPT body inside the caller's
/// read snapshot. A missing saved block document is handled by the text renderer.
pub(super) async fn preview_saved_page_body_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    actor: &SessionContext,
    type_key: &str,
    id: i64,
) -> ApiResult<Option<RenderedLayout>> {
    let mut budget = page::RenderBudget::new();
    preview_saved_page_body_in_tx_with_budget(
        tx,
        actor,
        type_key,
        id,
        &mut budget,
        page::BlockTemplateCache::global(),
    )
    .await
}

pub(super) async fn preview_saved_page_body_in_tx_with_budget(
    tx: &mut Transaction<'_, Sqlite>,
    actor: &SessionContext,
    type_key: &str,
    id: i64,
    budget: &mut page::RenderBudget,
    cache: &page::BlockTemplateCache,
) -> ApiResult<Option<RenderedLayout>> {
    preview_saved_page_body_in_tx_with_options_and_budget(
        tx, actor, type_key, id, None, budget, cache,
    )
    .await
}

pub(super) async fn preview_saved_page_body_in_tx_with_options_and_budget(
    tx: &mut Transaction<'_, Sqlite>,
    actor: &SessionContext,
    type_key: &str,
    id: i64,
    site_options_override: Option<&BTreeMap<String, serde_json::Value>>,
    budget: &mut page::RenderBudget,
    cache: &page::BlockTemplateCache,
) -> ApiResult<Option<RenderedLayout>> {
    let current = entry_builder_snapshot(tx, type_key, id).await?;
    if !current.raw_body_has_blocks {
        return Ok(None);
    }
    let Some(document) = current.opened.document.as_ref() else {
        return Ok(None);
    };
    let mut state =
        validate_entry_builder_document(tx, &current, actor_field_role(actor), document).await?;
    if let Some(opts) = site_options_override {
        state.site_options = opts.clone();
    }
    Ok(Some(
        render_entry_builder_preview_with_budget(
            tx,
            &current,
            &state,
            document,
            true,
            RenderAnnotationScope::None,
            budget,
            cache,
        )
        .await?,
    ))
}

pub async fn preview_saved_entry_body(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    id: i64,
) -> ApiResult<Option<RenderedLayout>> {
    let mut tx = db.begin().await?;
    let rendered = preview_saved_page_body_in_tx(&mut tx, actor, type_key, id).await?;
    tx.rollback().await?;
    Ok(rendered)
}

pub async fn preview_entry_builder(
    db: &SqlitePool,
    themes_root: &Path,
    actor: &SessionContext,
    type_key: &str,
    id: i64,
    document: &BuilderDocument,
) -> ApiResult<RenderedLayout> {
    preview_entry_builder_with_cache(
        db,
        themes_root,
        actor,
        type_key,
        id,
        document,
        page::BlockTemplateCache::global(),
    )
    .await
}

pub async fn preview_entry_builder_with_cache(
    db: &SqlitePool,
    themes_root: &Path,
    actor: &SessionContext,
    type_key: &str,
    id: i64,
    document: &BuilderDocument,
    cache: &page::BlockTemplateCache,
) -> ApiResult<RenderedLayout> {
    let mut tx = db.begin().await?;
    let current = entry_builder_snapshot(&mut tx, type_key, id).await?;
    if type_key != "pattern" {
        check_entry_body_edit_acl(&current.schema, actor)?;
    }
    let actor_role = actor_field_role(actor);
    let state = validate_entry_builder_document(&mut tx, &current, actor_role, document).await?;
    let scope = if type_key == "pattern" {
        RenderAnnotationScope::None
    } else {
        RenderAnnotationScope::EntryBodyOnly
    };
    let mut render_budget = page::RenderBudget::new();
    let rendered = render_entry_builder_preview_with_budget(
        &mut tx,
        &current,
        &state,
        document,
        false,
        scope,
        &mut render_budget,
        cache,
    )
    .await?;
    let out = if type_key == "pattern" {
        rendered
    } else {
        crate::storage::overrides::wrap_entry_body_in_preview_shell_in_tx_with_budget(
            &mut tx,
            themes_root,
            actor_role,
            &current.schema,
            id,
            current.page_layout_key.as_deref(),
            current.custom_template_key.as_deref(),
            rendered,
            render_budget,
            cache,
        )
        .await?
    };
    tx.rollback().await?;
    Ok(out)
}

#[derive(Debug)]
pub enum EntryBuilderMutationError {
    Api(ApiError),
    Veto(ContentBeforeSaveVeto),
}

impl From<ApiError> for EntryBuilderMutationError {
    fn from(err: ApiError) -> Self {
        Self::Api(err)
    }
}

impl From<sqlx::Error> for EntryBuilderMutationError {
    fn from(err: sqlx::Error) -> Self {
        Self::Api(err.into())
    }
}

enum EntryBuilderMutation<'a> {
    Save(&'a BuilderDocument),
    Reset,
}

impl EntryBuilderMutation<'_> {
    fn operation(&self) -> &'static str {
        match self {
            Self::Save(_) => "save",
            Self::Reset => "reset",
        }
    }
}

fn check_entry_builder_cas(
    current: &EntryBuilderSnapshot,
    expected_version: i64,
    mutation: &EntryBuilderMutation<'_>,
) -> ApiResult<()> {
    if current.opened.version != expected_version {
        return Err(ApiError::precondition_failed("Entry version changed."));
    }
    if matches!(mutation, EntryBuilderMutation::Reset) {
        let has_builder_doc = if current.opened.type_key == "pattern" {
            current.opened.document.is_some()
        } else {
            current.raw_body_has_blocks
        };
        if !has_builder_doc {
            return Err(ApiError::not_found("No saved Content Builder document."));
        }
    }
    Ok(())
}

async fn replace_entry_builder_refs(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
    document: Option<&BuilderDocument>,
) -> ApiResult<()> {
    sqlx::query("DELETE FROM entry_pattern_refs WHERE source_entry_id = ?")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM entry_builder_media_refs WHERE entry_id = ?")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    if let Some(document) = document {
        for pattern_id in pattern_ids(document) {
            // The target may later be force-deleted; the FK cascade and
            // warning-only renderer safely preserve that historical AST.
            let exists: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM content_entries WHERE id=? AND content_type_key='pattern')",
            )
            .bind(pattern_id)
            .fetch_one(&mut **tx)
            .await?;
            if exists != 0 {
                sqlx::query(
                    "INSERT INTO entry_pattern_refs(source_entry_id,target_pattern_id) VALUES(?,?)",
                )
                .bind(id)
                .bind(pattern_id)
                .execute(&mut **tx)
                .await?;
            }
        }
        let refs = resolve_document_media_refs_in_conn(&mut **tx, document).await?;
        for (role, ids) in [
            ("image", refs.image_ids),
            ("video", refs.video_ids),
            ("poster", refs.poster_image_ids),
        ] {
            for media_id in ids {
                sqlx::query("INSERT INTO entry_builder_media_refs(entry_id, media_id, ref_role) VALUES (?, ?, ?)")
                    .bind(id)
                    .bind(media_id)
                    .bind(role)
                    .execute(&mut **tx)
                    .await?;
            }
        }
    }
    Ok(())
}

async fn mutate_entry_builder(
    db: &SqlitePool,
    actor: &SessionContext,
    hooks: &HookDispatcher,
    type_key: &str,
    id: i64,
    expected_version: i64,
    mutation: EntryBuilderMutation<'_>,
) -> Result<EntryBuilderOpen, EntryBuilderMutationError> {
    if type_key == "pattern" && !matches!(actor.user.role, Role::Admin) {
        return Err(ApiError::forbidden("Pattern Builder requires Admin.").into());
    }
    let role = actor_field_role(actor);
    // Complete read-only preflight before plugin invocation. No writer lease is
    // held during the bounded, veto-capable hook.
    let mut read_tx = db.begin().await?;
    let before = entry_builder_snapshot(&mut read_tx, type_key, id).await?;
    if type_key != "pattern" {
        check_entry_body_edit_acl(&before.schema, actor)?;
    }
    check_entry_builder_cas(&before, expected_version, &mutation)?;
    if let EntryBuilderMutation::Save(document) = &mutation {
        let state = validate_entry_builder_document(&mut read_tx, &before, role, document).await?;
        render_entry_builder_preview(&mut read_tx, &before, &state, document, false).await?;
    }
    read_tx.commit().await?;
    let mut candidate_values = before.values.clone();
    if type_key != "pattern" {
        match &mutation {
            EntryBuilderMutation::Save(document) => {
                let candidate_blocks =
                    opendash_core::builder_blocks::serialize_builder_document_to_blocks(document)
                        .map_err(|err| ApiError::bad_request(err.to_string()))?;
                candidate_values.insert("body".to_owned(), Value::String(candidate_blocks));
            }
            EntryBuilderMutation::Reset => {
                let plain = opendash_core::builder_blocks::extract_visible_text(
                    before.raw_body.as_deref().unwrap_or(""),
                );
                if plain.is_empty() {
                    candidate_values.remove("body");
                } else {
                    candidate_values.insert("body".to_owned(), Value::String(plain));
                }
            }
        }
    }
    let operation = mutation.operation();
    hooks
        .dispatch_content_before_save_for_actor(
            type_key,
            role,
            json!({
                "operation": "update",
                "type_key": type_key,
                "entry_id": id,
                "expected_version": expected_version,
                "schema_token": &before.schema.schema_token,
                "title": &before.title,
                "slug": &before.slug,
                "candidate_values": &candidate_values,
                "term_ids": &before.term_ids,
                "builder_operation": operation,
            }),
        )
        .await
        .map_err(EntryBuilderMutationError::Veto)?;

    // Same writer reservation and session/role recheck as ordinary entry edits.
    let mut tx = entries::begin_entry_write(db, actor).await?;
    let current = entry_builder_snapshot(&mut tx, type_key, id).await?;
    if type_key != "pattern" {
        check_entry_body_edit_acl(&current.schema, actor)?;
    }
    check_entry_builder_cas(&current, expected_version, &mutation)?;
    if current.schema.schema_token != before.schema.schema_token {
        return Err(ApiError::precondition_failed(
            "Content schema changed during Builder operation.",
        )
        .into());
    }

    if type_key == "pattern" {
        let (mode, document) = match mutation {
            EntryBuilderMutation::Save(document) => {
                let state =
                    validate_entry_builder_document(&mut tx, &current, role, document).await?;
                render_entry_builder_preview(&mut tx, &current, &state, document, false).await?;
                let raw = String::from_utf8(
                    document
                        .to_json()
                        .map_err(|err| ApiError::bad_request(err.to_string()))?,
                )
                .map_err(|_| ApiError::bad_request("Invalid Builder JSON encoding."))?;
                sqlx::query(
                    "INSERT INTO entry_builder_documents(entry_id, document_json, updated_at)
                     VALUES (?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                     ON CONFLICT(entry_id) DO UPDATE SET document_json=excluded.document_json, updated_at=excluded.updated_at",
                )
                .bind(id)
                .bind(raw)
                .execute(&mut *tx)
                .await?;
                (ContentMode::Builder, Some(document))
            }
            EntryBuilderMutation::Reset => {
                sqlx::query("DELETE FROM entry_builder_documents WHERE entry_id = ?")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                (ContentMode::Editor, None)
            }
        };
        replace_entry_builder_refs(&mut tx, id, document).await?;
        let mode_db = match mode {
            ContentMode::Editor => "editor",
            ContentMode::Builder => "builder",
        };
        let affected = sqlx::query(
            "UPDATE content_entries SET content_mode=?, version=version+1,
                     updated_at=strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
              WHERE id=? AND content_type_key=? AND version=?",
        )
        .bind(mode_db)
        .bind(id)
        .bind(type_key)
        .bind(expected_version)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if affected != 1 {
            return Err(
                ApiError::precondition_failed("Entry changed during Builder operation.").into(),
            );
        }
    } else {
        let (mode_db, next_values, ref_doc) = match mutation {
            EntryBuilderMutation::Save(document) => {
                let state =
                    validate_entry_builder_document(&mut tx, &current, role, document).await?;
                // Enforce the same renderer output/node/list/form limits even when
                // the caller saves directly without ever requesting Preview.
                render_entry_builder_preview(&mut tx, &current, &state, document, false).await?;
                let body_blocks =
                    opendash_core::builder_blocks::serialize_builder_document_to_blocks(document)
                        .map_err(|err| ApiError::bad_request(err.to_string()))?;
                let body_val = Value::String(body_blocks);
                if let Some(body_field) = current.schema.effective_fields.get("body")
                    && !opendash_core::content::valid_field_value(&body_field.definition, &body_val)
                {
                    return Err(ApiError::bad_request(
                        "Comment block body exceeds RichText validation limit.",
                    )
                    .into());
                }
                let mut values = current.values.clone();
                values.insert("body".to_owned(), body_val);
                let ref_doc = if body_allows_public_refs(&current.schema) {
                    Some(document)
                } else {
                    None
                };
                ("builder", values, ref_doc)
            }
            EntryBuilderMutation::Reset => {
                let plain = opendash_core::builder_blocks::extract_visible_text(
                    current.raw_body.as_deref().unwrap_or(""),
                );
                let mut values = current.values.clone();
                if plain.is_empty() {
                    values.remove("body");
                } else {
                    values.insert("body".to_owned(), Value::String(plain));
                }
                ("editor", values, None)
            }
        };
        let next_values_json = serde_json::to_string(&next_values).map_err(ApiError::from)?;
        if next_values_json.len() > crate::routes::entries::MAX_CONTENT_ENTRY_JSON_BYTES {
            return Err(ApiError::bad_request("Entry JSON exceeds 768 KiB limit.").into());
        }
        let affected = sqlx::query(
            "UPDATE content_entries
                SET values_json=?, content_mode=?, version=version+1,
                    updated_at=strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
              WHERE id=? AND content_type_key=? AND version=?",
        )
        .bind(next_values_json)
        .bind(mode_db)
        .bind(id)
        .bind(type_key)
        .bind(expected_version)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if affected != 1 {
            return Err(
                ApiError::precondition_failed("Entry changed during Builder operation.").into(),
            );
        }
        sqlx::query("DELETE FROM entry_builder_documents WHERE entry_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        replace_entry_builder_refs(&mut tx, id, ref_doc).await?;
    }
    let saved = entry_builder_snapshot(&mut tx, type_key, id).await?;
    tx.commit().await?;
    hooks
        .dispatch_content_after_save_for_actor(
            type_key,
            role,
            json!({
                "operation": "update",
                "entry_id": id,
                "version": saved.opened.version,
                "type_key": type_key,
                "values": &saved.values,
                "term_ids": &saved.term_ids,
                "builder_operation": operation,
            }),
        )
        .await;
    Ok(saved.opened)
}

/// Startup cleanup under `SiteCoordinator` lock: moves any non-pattern rows
/// left in `entry_builder_documents` (e.g. Spec 2 demo rows) into unified
/// `content_entries.values_json["body"]`, re-syncs media/pattern refs, and
/// removes those non-pattern rows from `entry_builder_documents`.
pub async fn migrate_demo_entry_builder_docs_on_startup(db: &SqlitePool) -> ApiResult<usize> {
    let mut tx = db.begin().await?;
    let rows: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT e.id, e.content_type_key, e.values_json, d.document_json
           FROM entry_builder_documents d
           JOIN content_entries e ON e.id = d.entry_id
          WHERE e.content_type_key != 'pattern'
          ORDER BY d.entry_id ASC",
    )
    .fetch_all(&mut *tx)
    .await?;
    let mut migrated = 0usize;
    for (entry_id, type_key, values_json, doc_json) in rows {
        let mut values: BTreeMap<String, Value> =
            serde_json::from_str(&values_json).unwrap_or_default();
        let existing_has_blocks = values
            .get("body")
            .and_then(Value::as_str)
            .is_some_and(opendash_core::builder_blocks::contains_blocks);
        let parsed_doc = BuilderDocument::from_json(doc_json.as_bytes()).ok();
        let mut synced_doc = parsed_doc.clone();
        if !existing_has_blocks {
            if let Some(doc) = parsed_doc.as_ref()
                && let Ok(blocks) =
                    opendash_core::builder_blocks::serialize_builder_document_to_blocks(doc)
            {
                values.insert("body".to_owned(), Value::String(blocks));
                let updated_json = serde_json::to_string(&values)?;
                sqlx::query("UPDATE content_entries SET values_json = ? WHERE id = ?")
                    .bind(updated_json)
                    .bind(entry_id)
                    .execute(&mut *tx)
                    .await?;
            }
        } else if let Some(raw_body) = values.get("body").and_then(Value::as_str) {
            synced_doc = Some(
                opendash_core::builder_blocks::parse_blocks_lenient_or_recover(raw_body).document,
            );
        }
        let allow_public =
            crate::storage::content::load_effective_content_schema_in_tx(&mut *tx, &type_key)
                .await?
                .is_some_and(|schema| body_allows_public_refs(&schema));
        replace_entry_builder_refs(
            &mut tx,
            entry_id,
            if allow_public {
                synced_doc.as_ref()
            } else {
                None
            },
        )
        .await?;
        sqlx::query("DELETE FROM entry_builder_documents WHERE entry_id = ?")
            .bind(entry_id)
            .execute(&mut *tx)
            .await?;
        migrated += 1;
    }
    tx.commit().await?;
    Ok(migrated)
}

pub async fn save_entry_builder(
    db: &SqlitePool,
    actor: &SessionContext,
    hooks: &HookDispatcher,
    type_key: &str,
    id: i64,
    expected_version: i64,
    document: &BuilderDocument,
) -> Result<EntryBuilderOpen, EntryBuilderMutationError> {
    mutate_entry_builder(
        db,
        actor,
        hooks,
        type_key,
        id,
        expected_version,
        EntryBuilderMutation::Save(document),
    )
    .await
}

pub async fn reset_entry_builder(
    db: &SqlitePool,
    actor: &SessionContext,
    hooks: &HookDispatcher,
    type_key: &str,
    id: i64,
    expected_version: i64,
    confirmed: bool,
) -> Result<EntryBuilderOpen, EntryBuilderMutationError> {
    if !confirmed {
        return Err(ApiError::bad_request("Reset requires confirmation.").into());
    }
    mutate_entry_builder(
        db,
        actor,
        hooks,
        type_key,
        id,
        expected_version,
        EntryBuilderMutation::Reset,
    )
    .await
}
