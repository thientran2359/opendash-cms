use std::collections::BTreeMap;

use sqlx::SqlitePool;

use crate::{
    error::{ApiError, ApiResult},
    theme::{Block, Fields, Theme},
};

pub async fn list_theme_blocks(db: &SqlitePool) -> ApiResult<BTreeMap<String, Fields>> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT id, fields_json FROM theme_blocks")
        .fetch_all(db)
        .await?;
    let blocks = rows
        .into_iter()
        .map(|(id, fields)| Ok((id, serde_json::from_str(&fields)?)))
        .collect::<Result<_, serde_json::Error>>()?;
    Ok(blocks)
}

pub async fn insert_theme_block(db: &SqlitePool, theme: &Theme) -> ApiResult<()> {
    let result =
        sqlx::query("INSERT INTO theme_blocks (id, fields_json, template) VALUES (?, ?, ?)")
            .bind(&theme.descriptor.id)
            .bind(serde_json::to_string(&theme.descriptor.fields)?)
            .bind(&theme.template)
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

pub async fn load_theme_block(db: &SqlitePool, block_id: &str) -> ApiResult<(String, Fields)> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT template, fields_json FROM theme_blocks WHERE id = ?")
            .bind(block_id)
            .fetch_optional(db)
            .await?;
    let (template, fields) = row.ok_or(ApiError::bad_request("Không tìm thấy block."))?;
    Ok((template, serde_json::from_str(&fields)?))
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

pub async fn save_home_page(db: &SqlitePool, block: &Block) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO home_page (id, block_id, values_json) VALUES (1, ?, ?)
         ON CONFLICT(id) DO UPDATE SET block_id = excluded.block_id, values_json = excluded.values_json",
    )
    .bind(&block.kind)
    .bind(serde_json::to_string(&block.values)?)
    .execute(db)
    .await?;
    Ok(())
}
