//! Site-owned Builder homepage. All persistence, version CAS and media ACL
//! writes stay in this module; preview/open and public snapshot are read-only.
use std::collections::{BTreeMap, BTreeSet};

use opendash_core::{
    builder::{
        BUILDER_SCHEMA_VERSION, BuilderDocument, ColumnNode, ElementContent, ElementNode,
        KnownElement, LegacyBlockProps, RowNode, SectionNode,
    },
    id::valid_namespaced_block_id,
    registry::{RegistryCatalog, ResourceOwner},
    theme::BlockDescriptor,
};
use sha2::{Digest, Sha256};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::{
    error::{ApiError, ApiResult},
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

/// Only public-safe title and slug leave the list storage resolver.
#[derive(Debug, Clone)]
pub struct ListItem {
    pub title: String,
    pub slug: String,
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
                        PublishedListResult::Page(result) => ResolvedList::Page {
                            page,
                            out_of_range: page > 1 && result.entries.is_empty(),
                            items: result
                                .entries
                                .into_iter()
                                .map(|entry| ListItem {
                                    title: entry.title,
                                    slug: entry.slug,
                                })
                                .collect(),
                            has_next: result.has_next,
                        },
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

pub async fn check_preview_images(db: &SqlitePool, document: &BuilderDocument) -> ApiResult<()> {
    for id in image_ids(document) {
        if !valid_media_id(id) {
            return Err(ApiError::bad_request("Invalid Builder media ID"));
        }
        let exists: i64 =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM media_assets WHERE id = ?)")
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
    let images = image_ids(document);
    for id in &images {
        if !valid_media_id(id) {
            return Err(ApiError::bad_request("Invalid Builder media ID"));
        }
        let exists: i64 =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM media_assets WHERE id = ?)")
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
