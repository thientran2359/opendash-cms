//! Site-owned Builder homepage. All persistence, version CAS and media ACL
//! writes stay in this module; preview/open and public snapshot are read-only.
use std::collections::{BTreeMap, BTreeSet};

use opendash_core::{
    builder::{
        BUILDER_SCHEMA_VERSION, BindingSource, BuilderDocument, ColumnNode, ElementContent,
        ElementNode, KnownElement, LegacyBlockProps, RowNode, SectionNode,
    },
    content::{
        ContentFieldType, ContentMode, ContentSupport, EffectiveContentSchema,
        EffectiveFieldStatus, FieldRole, PublicExposure,
    },
    id::valid_namespaced_block_id,
    registry::{DeclarationState, ProviderStatus, RegistryCatalog, ResourceOwner},
    theme::BlockDescriptor,
};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::{
    auth::{Role, SessionContext},
    error::{ApiError, ApiResult},
    hooks::{ContentBeforeSaveVeto, HookDispatcher},
    page::{self, RenderedLayout},
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

pub struct HomeState {
    pub version: i64,
    pub saved: Option<BuilderDocument>,
    pub legacy: Option<Block>,
    pub themes: BTreeMap<String, ThemeDefinition>,
    pub active_id: Option<String>,
    pub active_alias: Option<String>,
    pub active_template: Option<String>,
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

    /// Do not trust prior-state information from the client. This method uses
    /// either persisted JSON or a deterministic, read-only M3 projection.
    pub fn validate(&self, document: &BuilderDocument) -> ApiResult<()> {
        document
            .validate_for_save(&self.descriptors()?, Some(&self.document()))
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
            responsive: None,
            children: vec![RowNode {
                id: format!("legacy-row-{suffix}"),
                responsive: None,
                children: vec![ColumnNode {
                    id: format!("legacy-column-{suffix}"),
                    responsive: None,
                    children: vec![ElementNode {
                        id: format!("legacy-element-{suffix}"),
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
    Ok(HomeState {
        version,
        saved,
        legacy,
        themes,
        active_id,
        active_alias,
        active_template,
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

pub async fn validate_media_refs_in_tx(
    conn: &mut sqlx::SqliteConnection,
    document: &BuilderDocument,
) -> ApiResult<()> {
    for id in image_ids(document)
        .into_iter()
        .chain(poster_image_ids(document))
    {
        if !valid_media_id(id) {
            return Err(ApiError::bad_request("Invalid Builder media ID"));
        }
        let exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM media_assets WHERE id = ? AND kind = 'image' AND lifecycle_state = 'active')",
        )
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
        if exists == 0 {
            return Err(ApiError::bad_request(
                "Image chưa tồn tại trong Media Library.",
            ));
        }
    }
    for id in video_ids(document) {
        if !valid_media_id(id) {
            return Err(ApiError::bad_request("Invalid Builder video ID"));
        }
        let exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM media_assets WHERE id = ? AND kind = 'video' AND lifecycle_state = 'active')",
        )
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
        if exists == 0 {
            return Err(ApiError::bad_request(
                "Video chưa tồn tại trong Media Library.",
            ));
        }
    }
    Ok(())
}

pub async fn check_preview_images(db: &SqlitePool, document: &BuilderDocument) -> ApiResult<()> {
    let mut conn = db.acquire().await?;
    validate_media_refs_in_tx(&mut conn, document).await?;
    for id in image_ids(document) {
        if !valid_media_id(id) {
            return Err(ApiError::bad_request("Invalid Builder media ID"));
        }
        let exists: i64 =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM media_assets WHERE id = ? AND kind = 'image' AND lifecycle_state = 'active')")
                .bind(id)
                .fetch_one(db)
                .await?;
        if exists == 0 {
            return Err(ApiError::bad_request(
                "Image chưa tồn tại trong Media Library.",
            ));
        }
    }
    Ok(())
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
    let videos = video_ids(document);
    let posters = poster_image_ids(document);
    validate_media_refs_in_tx(&mut tx, document).await?;
    let images = image_ids(document);
    for id in &images {
        if !valid_media_id(id) {
            return Err(ApiError::bad_request("Invalid Builder media ID"));
        }
        let exists: i64 =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM media_assets WHERE id = ? AND kind = 'image' AND lifecycle_state = 'active')")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        if exists == 0 {
            return Err(ApiError::bad_request(
                "Image chưa tồn tại trong Media Library.",
            ));
        }
    }
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
    for id in videos {
        sqlx::query(
            "INSERT INTO builder_homepage_video_refs (media_id, ref_role) VALUES (?, 'video')",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    for id in posters {
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
    for id in images {
        sqlx::query("INSERT INTO builder_homepage_media (media_id) VALUES (?)")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok((current.version, rendered))
}

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
    pub document: Option<BuilderDocument>,
}

struct EntryBuilderSnapshot {
    opened: EntryBuilderOpen,
    schema: EffectiveContentSchema,
    title: String,
    slug: String,
    values: BTreeMap<String, Value>,
    term_ids: Vec<i64>,
}

fn actor_field_role(actor: &SessionContext) -> FieldRole {
    match actor.user.role {
        Role::Admin => FieldRole::Admin,
        Role::Editor => FieldRole::Editor,
    }
}

async fn entry_builder_snapshot(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    id: i64,
) -> ApiResult<EntryBuilderSnapshot> {
    // Section still uses its private legacy APIs in S01. Pattern replaces it
    // in S02, so a malformed Section registration must not unlock this path.
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
    let row: Option<(String, String, String, String, i64, Option<String>)> = sqlx::query_as(
        "SELECT e.title, e.slug, e.values_json, e.content_mode, e.version, d.document_json
           FROM content_entries e LEFT JOIN entry_builder_documents d ON d.entry_id = e.id
          WHERE e.id = ? AND e.content_type_key = ?",
    )
    .bind(id)
    .bind(type_key)
    .fetch_optional(&mut **tx)
    .await?;
    let (title, slug, values, mode, version, raw) =
        row.ok_or_else(|| ApiError::not_found("Content entry not found."))?;
    let content_mode = match mode.as_str() {
        "editor" => ContentMode::Editor,
        "builder" => ContentMode::Builder,
        _ => return Err(ApiError::internal("Invalid stored entry content mode.")),
    };
    let document = raw
        .map(|raw| BuilderDocument::from_json(raw.as_bytes()))
        .transpose()
        .map_err(|_| ApiError::internal("Stored entry Builder document is not readable."))?;
    let values = serde_json::from_str(&values)?;
    let term_ids = entries::read_term_ids(tx, id).await?;
    Ok(EntryBuilderSnapshot {
        opened: EntryBuilderOpen {
            entry_id: id,
            type_key: type_key.to_owned(),
            content_mode,
            version,
            document,
        },
        schema,
        title,
        slug,
        values,
        term_ids,
    })
}

pub async fn read_entry_builder(
    db: &SqlitePool,
    _actor: &SessionContext,
    type_key: &str,
    id: i64,
) -> ApiResult<EntryBuilderOpen> {
    let mut tx = db.begin().await?;
    let current = entry_builder_snapshot(&mut tx, type_key, id).await?;
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
    let state = read_home_tx(tx).await?;
    document
        .validate_for_save(&state.descriptors()?, current.opened.document.as_ref())
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
    let bindings = builder_binding::resolve_in_tx_with_route(
        tx,
        document,
        true,
        Some((&current.opened.type_key, &current.slug)),
    )
    .await?;
    let lists = resolve_lists_in_tx(tx, document, &BTreeMap::new()).await?;
    let forms = resolve_contact_forms_in_tx(tx, document).await?;
    let parts = BTreeMap::new();
    let context = shared_preview.then_some(page::BuilderTemplateContext {
        content_html: None,
        parts: &parts,
    });
    page::render_builder_with_template_context(
        state,
        document,
        true,
        &bindings,
        &lists,
        &forms,
        context.as_ref(),
    )
}

/// Shared-template preview uses the selected CPT body inside the caller's
/// read snapshot. A missing saved document is handled by the text renderer.
pub(super) async fn preview_saved_page_body_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    actor: &SessionContext,
    type_key: &str,
    id: i64,
) -> ApiResult<Option<RenderedLayout>> {
    let current = entry_builder_snapshot(tx, type_key, id).await?;
    if current.opened.content_mode != ContentMode::Builder {
        return Ok(None);
    }
    let Some(document) = current.opened.document.as_ref() else {
        return Ok(None);
    };
    let state =
        validate_entry_builder_document(tx, &current, actor_field_role(actor), document).await?;
    Ok(Some(
        render_entry_builder_preview(tx, &current, &state, document, true).await?,
    ))
}

pub async fn preview_entry_builder(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    id: i64,
    document: &BuilderDocument,
) -> ApiResult<RenderedLayout> {
    let mut tx = db.begin().await?;
    let current = entry_builder_snapshot(&mut tx, type_key, id).await?;
    let state =
        validate_entry_builder_document(&mut tx, &current, actor_field_role(actor), document)
            .await?;
    let rendered = render_entry_builder_preview(&mut tx, &current, &state, document, false).await?;
    tx.commit().await?;
    Ok(rendered)
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
    if matches!(mutation, EntryBuilderMutation::Reset) && current.opened.document.is_none() {
        return Err(ApiError::not_found("No saved Content Builder document."));
    }
    Ok(())
}

async fn replace_entry_builder_refs(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
    document: Option<&BuilderDocument>,
) -> ApiResult<()> {
    sqlx::query("DELETE FROM entry_builder_media_refs WHERE entry_id = ?")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    if let Some(document) = document {
        for (role, ids) in [
            ("image", image_ids(document)),
            ("video", video_ids(document)),
            ("poster", poster_image_ids(document)),
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
    let role = actor_field_role(actor);
    // Complete read-only preflight before plugin invocation. No writer lease is
    // held during the bounded, veto-capable hook.
    let mut read_tx = db.begin().await?;
    let before = entry_builder_snapshot(&mut read_tx, type_key, id).await?;
    check_entry_builder_cas(&before, expected_version, &mutation)?;
    if let EntryBuilderMutation::Save(document) = &mutation {
        let state = validate_entry_builder_document(&mut read_tx, &before, role, document).await?;
        render_entry_builder_preview(&mut read_tx, &before, &state, document, false).await?;
    }
    read_tx.commit().await?;
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
                "candidate_values": &before.values,
                "term_ids": &before.term_ids,
                "builder_operation": operation,
            }),
        )
        .await
        .map_err(EntryBuilderMutationError::Veto)?;

    // Same writer reservation and session/role recheck as ordinary entry edits.
    let mut tx = entries::begin_entry_write(db, actor).await?;
    let current = entry_builder_snapshot(&mut tx, type_key, id).await?;
    check_entry_builder_cas(&current, expected_version, &mutation)?;
    if current.schema.schema_token != before.schema.schema_token {
        return Err(ApiError::precondition_failed(
            "Content schema changed during Builder operation.",
        )
        .into());
    }

    let (mode, document) = match mutation {
        EntryBuilderMutation::Save(document) => {
            let state = validate_entry_builder_document(&mut tx, &current, role, document).await?;
            // Enforce the same renderer output/node/list/form limits even when
            // the caller saves directly without ever requesting Preview.
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
                 ON CONFLICT(entry_id) DO UPDATE SET document_json=excluded.document_json, updated_at=excluded.updated_at"
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
