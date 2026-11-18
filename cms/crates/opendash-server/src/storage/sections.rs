//! Private, site-owned Section entries. One canonical content_entries version
//! guards both metadata and Builder AST; Section never enters public routing.
use std::collections::BTreeMap;

use opendash_core::{builder::BuilderDocument, id::valid_kebab_segment_with_limit};
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::{
    error::{ApiError, ApiResult},
    page::{self, RenderedLayout},
    storage::{builder, builder_binding},
};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionInput {
    pub title: String,
    pub slug: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SectionRecord {
    pub id: i64,
    pub content_type_key: &'static str,
    pub title: String,
    pub slug: String,
    pub publication_status: &'static str,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

type Row = (i64, String, String, i64, String, String);

fn decode(row: Row) -> SectionRecord {
    SectionRecord {
        id: row.0,
        content_type_key: "section",
        title: row.1,
        slug: row.2,
        publication_status: "draft",
        version: row.3,
        created_at: row.4,
        updated_at: row.5,
    }
}

fn validate(input: &SectionInput) -> ApiResult<()> {
    let title_len = input.title.chars().count();
    if title_len == 0
        || title_len > 200
        || input.title.trim().is_empty()
        || input.title.chars().any(|c| c.is_control())
    {
        return Err(ApiError::bad_request(
            "Tiêu đề Section không hợp lệ (1–200 ký tự).",
        ));
    }
    if !valid_kebab_segment_with_limit(&input.slug, 64) {
        return Err(ApiError::bad_request("Slug Section không hợp lệ."));
    }
    Ok(())
}

async fn get_in_tx(tx: &mut Transaction<'_, Sqlite>, id: i64) -> ApiResult<SectionRecord> {
    if id <= 0 {
        return Err(ApiError::bad_request("Section ID phải dương."));
    }
    let row: Option<Row> = sqlx::query_as(
        "SELECT id,title,slug,version,created_at,updated_at FROM content_entries
         WHERE id = ? AND content_type_key = 'section' AND publication_status = 'draft'",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(decode)
        .ok_or_else(|| ApiError::not_found("Section không tồn tại."))
}

pub async fn list(db: &SqlitePool) -> ApiResult<Vec<SectionRecord>> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id,title,slug,version,created_at,updated_at FROM content_entries
         WHERE content_type_key='section' AND publication_status='draft'
         ORDER BY id DESC LIMIT 200",
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(decode).collect())
}

pub async fn get(db: &SqlitePool, id: i64) -> ApiResult<SectionRecord> {
    let mut tx = db.begin().await?;
    let row = get_in_tx(&mut tx, id).await?;
    tx.commit().await?;
    Ok(row)
}

async fn reserve_writer(tx: &mut Transaction<'_, Sqlite>) -> ApiResult<()> {
    sqlx::query("UPDATE bootstrap SET initialized=initialized WHERE id=1")
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Return the private Section pins while holding the same writer transaction
/// that will tombstone the media. Routes must never issue SQL directly.
#[derive(Debug)]
pub struct SectionMediaPins {
    pub total_pins: i64,
    pub entry_ids: Vec<i64>,
}

pub async fn media_pins_for_delete_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    media_id: &str,
    expected_generation: i64,
    expected_version: i64,
) -> ApiResult<Option<SectionMediaPins>> {
    reserve_writer(tx).await?;
    let media: Option<(String, i64, i64)> =
        sqlx::query_as("SELECT lifecycle_state,generation,version FROM media_assets WHERE id=?")
            .bind(media_id)
            .fetch_optional(&mut **tx)
            .await?;
    let Some((state, generation, version)) = media else {
        return Err(ApiError::not_found("Không tìm thấy media."));
    };
    if state != "active" {
        return Err(ApiError::conflict("Media đang trong quá trình xóa."));
    }
    if expected_generation != generation || expected_version != version {
        return Err(ApiError::precondition_failed(
            "Phiên bản media đã thay đổi.",
        ));
    }
    let entry_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT entry_id FROM section_document_media_refs WHERE media_id=? ORDER BY entry_id LIMIT 51",
    )
    .bind(media_id)
    .fetch_all(&mut **tx)
    .await?;
    if entry_ids.is_empty() {
        return Ok(None);
    }
    let total_pins: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT entry_id) FROM section_document_media_refs WHERE media_id=?",
    )
    .bind(media_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(Some(SectionMediaPins {
        total_pins,
        entry_ids,
    }))
}

pub async fn create(db: &SqlitePool, input: &SectionInput) -> ApiResult<SectionRecord> {
    validate(input)?;
    let mut tx = db.begin().await?;
    reserve_writer(&mut tx).await?;
    let result = sqlx::query(
        "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
         VALUES('section', ?, ?, '{}', 'draft')",
    )
    .bind(&input.slug)
    .bind(&input.title)
    .execute(&mut *tx)
    .await;
    let id = match result {
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(ApiError::conflict("Slug Section đã tồn tại."));
        }
        other => other?.last_insert_rowid(),
    };
    sqlx::query("INSERT INTO section_documents(entry_id,document_json) VALUES(?, ?)")
        .bind(id)
        .bind(serde_json::to_string(&BuilderDocument::empty())?)
        .execute(&mut *tx)
        .await?;
    let row = get_in_tx(&mut tx, id).await?;
    tx.commit().await?;
    Ok(row)
}

pub async fn update(
    db: &SqlitePool,
    id: i64,
    version: i64,
    input: &SectionInput,
) -> ApiResult<SectionRecord> {
    validate(input)?;
    let mut tx = db.begin().await?;
    reserve_writer(&mut tx).await?;
    let old = get_in_tx(&mut tx, id).await?;
    if old.version != version {
        return Err(ApiError::precondition_failed("Section đã thay đổi."));
    }
    let res = sqlx::query(
        "UPDATE content_entries SET slug=?,title=?,version=version+1,updated_at=datetime('now')
         WHERE id=? AND content_type_key='section' AND version=? AND publication_status='draft'",
    )
    .bind(&input.slug)
    .bind(&input.title)
    .bind(id)
    .bind(version)
    .execute(&mut *tx)
    .await;
    let updated = match res {
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(ApiError::conflict("Slug Section đã tồn tại."));
        }
        other => other?,
    };
    if updated.rows_affected() != 1 {
        return Err(ApiError::precondition_failed("Section đã thay đổi."));
    }
    let updated = get_in_tx(&mut tx, id).await?;
    tx.commit().await?;
    Ok(updated)
}

pub async fn delete(db: &SqlitePool, id: i64, version: i64) -> ApiResult<()> {
    let mut tx = db.begin().await?;
    reserve_writer(&mut tx).await?;
    let old = get_in_tx(&mut tx, id).await?;
    if old.version != version {
        return Err(ApiError::precondition_failed("Section đã thay đổi."));
    }
    // section_documents + private pins cascade in the SAME transaction.
    let affected = sqlx::query(
        "DELETE FROM content_entries WHERE id=? AND version=? AND content_type_key='section'",
    )
    .bind(id)
    .bind(version)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if affected != 1 {
        return Err(ApiError::precondition_failed("Section đã thay đổi."));
    }
    tx.commit().await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct SectionDocumentOpen {
    pub entry_id: i64,
    pub version: i64,
    pub document: BuilderDocument,
}

async fn read_document_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
) -> ApiResult<SectionDocumentOpen> {
    let section = get_in_tx(tx, id).await?;
    let raw: String =
        sqlx::query_scalar("SELECT document_json FROM section_documents WHERE entry_id=?")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| ApiError::internal("Section thiếu tài liệu Builder."))?;
    let document = BuilderDocument::from_json(raw.as_bytes())
        .map_err(|_| ApiError::internal("Section Builder AST hỏng."))?;
    Ok(SectionDocumentOpen {
        entry_id: id,
        version: section.version,
        document,
    })
}

pub async fn read_document(db: &SqlitePool, id: i64) -> ApiResult<SectionDocumentOpen> {
    let mut tx = db.begin().await?;
    let data = read_document_in_tx(&mut tx, id).await?;
    tx.commit().await?;
    Ok(data)
}

async fn validate_document_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    doc: &BuilderDocument,
    prior: &BuilderDocument,
) -> ApiResult<builder::HomeState> {
    let state = builder::read_home_tx(tx).await?;
    doc.validate_for_save(&state.descriptors()?, Some(prior))
        .map_err(|err| ApiError::bad_request(err.to_string()))?;
    // Sections cannot carry RouteContext bindings: there is no public route.
    doc.validate_route_context_for_role(Some("section"), None)
        .map_err(|err| ApiError::bad_request(err.to_string()))?;
    builder::validate_media_refs_in_tx(&mut **tx, doc).await?;
    Ok(state)
}

async fn render_private_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    state: &builder::HomeState,
    doc: &BuilderDocument,
) -> ApiResult<RenderedLayout> {
    let bindings = builder_binding::resolve_in_tx(tx, doc, true).await?;
    let lists = builder::resolve_lists_in_tx(tx, doc, &BTreeMap::new()).await?;
    let forms = builder::resolve_contact_forms_in_tx(tx, doc).await?;
    page::render_builder_with_lists_and_forms(state, doc, true, &bindings, &lists, &forms)
}

pub async fn preview(db: &SqlitePool, id: i64, doc: &BuilderDocument) -> ApiResult<RenderedLayout> {
    let mut tx = db.begin().await?;
    let current = read_document_in_tx(&mut tx, id).await?;
    let state = validate_document_in_tx(&mut tx, doc, &current.document).await?;
    let rendered = render_private_in_tx(&mut tx, &state, doc).await?;
    tx.commit().await?;
    Ok(rendered)
}

#[derive(Debug, Serialize)]
pub struct SectionDocumentSaved {
    #[serde(flatten)]
    pub opened: SectionDocumentOpen,
    #[serde(flatten)]
    pub rendered: RenderedLayout,
}

pub async fn save_document(
    db: &SqlitePool,
    id: i64,
    version: i64,
    doc: &BuilderDocument,
) -> ApiResult<SectionDocumentSaved> {
    let mut tx = db.begin().await?;
    reserve_writer(&mut tx).await?;
    let old = read_document_in_tx(&mut tx, id).await?;
    if old.version != version {
        return Err(ApiError::precondition_failed(
            "Section đã thay đổi; nháp chưa lưu.",
        ));
    }
    let state = validate_document_in_tx(&mut tx, doc, &old.document).await?;
    let rendered = render_private_in_tx(&mut tx, &state, doc).await?;
    let affected = sqlx::query(
        "UPDATE content_entries SET version=version+1,updated_at=datetime('now')
         WHERE id=? AND version=? AND content_type_key='section' AND publication_status='draft'",
    )
    .bind(id)
    .bind(version)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if affected != 1 {
        return Err(ApiError::precondition_failed(
            "Section đã thay đổi đồng thời.",
        ));
    }
    sqlx::query("UPDATE section_documents SET document_json=? WHERE entry_id=?")
        .bind(serde_json::to_string(doc)?)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM section_document_media_refs WHERE entry_id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for (role, ids) in [
        ("image", builder::image_ids(doc)),
        ("video", builder::video_ids(doc)),
        ("poster", builder::poster_image_ids(doc)),
    ] {
        for asset_id in ids {
            sqlx::query(
                "INSERT INTO section_document_media_refs(entry_id,media_id,ref_role) VALUES(?,?,?)",
            )
            .bind(id)
            .bind(asset_id)
            .bind(role)
            .execute(&mut *tx)
            .await?;
        }
    }
    let opened = read_document_in_tx(&mut tx, id).await?;
    tx.commit().await?;
    Ok(SectionDocumentSaved { opened, rendered })
}
