use std::collections::BTreeMap;

use opendash_core::registry::{RegistryCatalog, ResourceOwner};
use sqlx::SqlitePool;

use crate::{
    error::{ApiError, ApiResult},
    theme::{Block, Fields, Theme},
};

#[derive(Debug, Clone)]
pub struct BlockDefinition {
    pub fields: Fields,
    pub template: String,
}

/// SQLite is authoritative. A fresh snapshot avoids stale state across restart/install.
pub async fn load_block_catalog(db: &SqlitePool) -> ApiResult<RegistryCatalog<BlockDefinition>> {
    let rows: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id, fields_json, template FROM theme_blocks ORDER BY id")
            .fetch_all(db)
            .await?;
    let mut catalog = RegistryCatalog::new();
    for (id, fields, template) in rows {
        if !opendash_core::id::valid_namespaced_block_id(&id) {
            return Err(ApiError::internal("Invalid stored block ID"));
        }
        let owner = ResourceOwner::Theme(id.split_once('/').unwrap().0.to_owned());
        catalog
            .register(
                id,
                owner,
                BlockDefinition {
                    fields: serde_json::from_str(&fields)?,
                    template,
                },
            )
            .map_err(|e| ApiError::conflict(e.to_string()))?;
    }
    Ok(catalog)
}

pub async fn list_theme_blocks(db: &SqlitePool) -> ApiResult<BTreeMap<String, Fields>> {
    Ok(load_block_catalog(db)
        .await?
        .iter()
        .map(|(id, entry)| (id.clone(), entry.descriptor.fields.clone()))
        .collect())
}

pub async fn insert_theme_block(db: &SqlitePool, theme: &Theme) -> ApiResult<()> {
    let mut catalog = load_block_catalog(db).await?;
    let owner = ResourceOwner::Theme(
        theme
            .descriptor
            .id
            .split_once('/')
            .ok_or_else(|| ApiError::bad_request("Invalid block namespace"))?
            .0
            .into(),
    );
    catalog
        .register(
            theme.descriptor.id.clone(),
            owner,
            BlockDefinition {
                fields: theme.descriptor.fields.clone(),
                template: theme.template.clone(),
            },
        )
        .map_err(|e| ApiError::conflict(e.to_string()))?;
    // Explicit aliases must resolve to an already-installed block with the
    // EXACT field keys. Labels may differ; missing/extra fields cannot be
    // silently dropped. Validate before creating any persistent state.
    if let Some(alias) = &theme.compatible_block_id {
        let source = catalog
            .get(alias)
            .ok_or_else(|| ApiError::bad_request("Alias trỏ đến block chưa được cài đặt."))?;
        if source
            .descriptor
            .fields
            .keys()
            .ne(theme.descriptor.fields.keys())
        {
            return Err(ApiError::bad_request(
                "Field keys của alias không khớp block nguồn.",
            ));
        }
    }
    // The UNIQUE constraint still arbitrates concurrent installs after this snapshot check.
    let result = sqlx::query(
        "INSERT INTO theme_blocks
                     (id, fields_json, template, content_list_template, content_single_template, compatible_block_id)
                     VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&theme.descriptor.id)
    .bind(serde_json::to_string(&theme.descriptor.fields)?)
    .bind(&theme.template)
    .bind(&theme.content_list_template)
    .bind(&theme.content_single_template)
    .bind(&theme.compatible_block_id)
    .execute(db)
    .await;
    match result {
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            Err(ApiError::conflict("ID block đã có trong dữ liệu."))
        }
        result => {
            result?;
            Ok(())
        }
    }
}

/// Explicit active theme controls CPT templates. Without an explicit activation
/// preserve the pre-0011 home_page-selected template for migrated websites.
pub async fn active_content_templates(
    db: &SqlitePool,
) -> ApiResult<(Option<String>, Option<String>)> {
    let templates: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT
            CASE WHEN r.legacy_block_id IS NULL THEN legacy.content_list_template
                 ELSE active.content_list_template END,
            CASE WHEN r.legacy_block_id IS NULL THEN legacy.content_single_template
                 ELSE active.content_single_template END
         FROM theme_active_selection AS a
         LEFT JOIN theme_revisions AS r ON r.revision_id = a.revision_id
         LEFT JOIN theme_blocks AS active ON active.id = r.legacy_block_id
         LEFT JOIN home_page AS p ON p.id = 1
         LEFT JOIN theme_blocks AS legacy ON legacy.id = p.block_id
         WHERE a.id = 1",
    )
    .fetch_optional(db)
    .await?;
    Ok(templates.unwrap_or((None, None)))
}

pub async fn load_theme_block(db: &SqlitePool, block_id: &str) -> ApiResult<(String, Fields)> {
    let catalog = load_block_catalog(db).await?;
    let entry = catalog
        .get(block_id)
        .ok_or_else(|| ApiError::bad_request("Block not found"))?;
    if entry.status != opendash_core::registry::ProviderStatus::Active {
        return Err(ApiError::bad_request("Block provider is inactive"));
    }
    Ok((
        entry.descriptor.template.clone(),
        entry.descriptor.fields.clone(),
    ))
}

pub async fn load_home_page(db: &SqlitePool) -> ApiResult<Option<Block>> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT block_id, values_json FROM home_page WHERE id = 1")
            .fetch_optional(db)
            .await?;
    row.map(|(kind, values)| {
        Ok(Block {
            kind,
            values: serde_json::from_str(&values)?,
        })
    })
    .transpose()
}
