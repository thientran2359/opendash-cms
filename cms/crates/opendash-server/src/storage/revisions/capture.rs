use std::collections::{BTreeMap, HashSet};

use opendash_core::content::ContentMode;
use opendash_core::revisions::{
    CanonicalRevisionPayload, CanonicalRevisionSeo, MAX_REVISIONS_PER_ENTRY, RevisionSource,
    canonical_json_value, canonical_revision_json,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection};

use crate::auth::SessionContext;
use crate::error::{ApiError, ApiResult};
use crate::storage::entries::PluginActionFence;

use super::types::{RevisionActor, actor_from_plugin_fence};

pub(super) struct LoadedEntrySnapshot {
    pub(super) entry_id: i64,
    pub(super) content_type_key: String,
    pub(super) slug: String,
    pub(super) version: i64,
    pub(super) payload: CanonicalRevisionPayload,
}

/// Computes the deterministic 64-hex SHA-256 hash of a canonical revision snapshot.
pub fn compute_revision_content_hash(payload: &CanonicalRevisionPayload) -> String {
    let canonical = canonical_revision_json(payload);
    let digest = Sha256::digest(canonical.as_bytes());
    format!("{digest:x}")
}

pub(super) fn parse_content_mode(raw: &str) -> ApiResult<ContentMode> {
    match raw {
        "editor" => Ok(ContentMode::Editor),
        "builder" => Ok(ContentMode::Builder),
        _ => Err(ApiError::conflict(
            "Chế độ nội dung trong DB không hợp lệ; không thể tạo snapshot lịch sử.",
        )),
    }
}

pub(super) async fn load_entry_canonical_snapshot_in_conn(
    conn: &mut SqliteConnection,
    entry_id: i64,
) -> ApiResult<LoadedEntrySnapshot> {
    let row = sqlx::query(
        "SELECT id, content_type_key, slug, title, values_json, content_mode,
                page_layout_key, custom_template_key, primary_category_term_id,
                parent_id, menu_order, version
           FROM content_entries
          WHERE id = ?",
    )
    .bind(entry_id)
    .fetch_optional(&mut *conn)
    .await?;

    let Some(row) = row else {
        return Err(ApiError::not_found("Không tìm thấy nội dung."));
    };

    let content_type_key: String = row.try_get("content_type_key")?;
    let slug: String = row.try_get("slug")?;
    let title: String = row.try_get("title")?;
    let values_raw: String = row.try_get("values_json")?;
    let content_mode_raw: String = row.try_get("content_mode")?;
    let page_layout_key: Option<String> = row
        .try_get::<Option<String>, _>("page_layout_key")?
        .filter(|s| !s.is_empty());
    let custom_template_key: Option<String> = row
        .try_get::<Option<String>, _>("custom_template_key")?
        .filter(|s| !s.is_empty());
    let primary_category_term_id: Option<i64> = row.try_get("primary_category_term_id")?;
    let parent_id: Option<i64> = row.try_get("parent_id")?;
    let menu_order: i32 = row.try_get("menu_order")?;
    let version: i64 = row.try_get("version")?;

    let content_mode = parse_content_mode(&content_mode_raw)?;
    let parsed_values: BTreeMap<String, Value> =
        serde_json::from_str(&values_raw).map_err(|_| {
            ApiError::conflict("Dữ liệu trường nội dung bị lỗi JSON; không thể tạo snapshot.")
        })?;
    let mut values = BTreeMap::new();
    for (key, value) in parsed_values {
        values.insert(key, canonical_json_value(&value));
    }

    let builder_document_json = if content_type_key == "pattern" {
        let doc_raw: Option<String> = sqlx::query_scalar(
            "SELECT document_json FROM entry_builder_documents WHERE entry_id = ?",
        )
        .bind(entry_id)
        .fetch_optional(&mut *conn)
        .await?;
        match doc_raw {
            Some(raw) => {
                let parsed: Value = serde_json::from_str(&raw).map_err(|_| {
                    ApiError::conflict("Dữ liệu Builder JSON không hợp lệ; không thể tạo snapshot.")
                })?;
                if !parsed.is_object() {
                    return Err(ApiError::conflict(
                        "Dữ liệu Builder không phải JSON object; không thể tạo snapshot.",
                    ));
                }
                Some(serde_json::to_string(&canonical_json_value(&parsed))?)
            }
            None => None,
        }
    } else {
        None
    };

    let mut term_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT term_id FROM entry_terms WHERE entry_id = ? ORDER BY term_id ASC",
    )
    .bind(entry_id)
    .fetch_all(&mut *conn)
    .await?;
    term_ids.sort_unstable();
    term_ids.dedup();

    let seo_row = sqlx::query(
        "SELECT title, description, og_image_id, canonical_url, noindex
           FROM entry_seo
          WHERE entry_id = ?",
    )
    .bind(entry_id)
    .fetch_optional(&mut *conn)
    .await?;
    let seo = seo_row
        .map(|srow| -> Result<CanonicalRevisionSeo, sqlx::Error> {
            Ok(CanonicalRevisionSeo {
                title: srow.try_get("title")?,
                description: srow.try_get("description")?,
                og_image_id: srow.try_get("og_image_id")?,
                canonical_url: srow.try_get("canonical_url")?,
                noindex: srow.try_get::<i64, _>("noindex")? != 0,
            })
        })
        .transpose()?
        .and_then(CanonicalRevisionSeo::into_normalized);

    Ok(LoadedEntrySnapshot {
        entry_id,
        content_type_key,
        slug,
        version,
        payload: CanonicalRevisionPayload {
            title,
            content_mode,
            values,
            builder_document_json,
            term_ids,
            primary_category_term_id,
            page_layout_key,
            custom_template_key,
            seo,
            parent_id,
            menu_order,
        },
    })
}

async fn insert_revision_row_in_tx(
    conn: &mut SqliteConnection,
    snapshot: &LoadedEntrySnapshot,
    revision_number: i64,
    content_hash: &str,
    source: RevisionSource,
    restored_from_revision_number: Option<i64>,
    actor: RevisionActor<'_>,
) -> ApiResult<()> {
    let values_json = serde_json::to_string(&snapshot.payload.values)?;
    let term_ids_json = serde_json::to_string(&snapshot.payload.term_ids)?;
    let seo_json = snapshot
        .payload
        .seo
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    let (author_user_id, author_username) = actor.into_parts();

    sqlx::query(
        "INSERT INTO entry_revisions (
             entry_id, content_type_key, revision_number, content_hash, source,
             restored_from_revision_number, author_user_id, author_username,
             title, slug, content_mode, values_json, builder_document_json,
             term_ids_json, primary_category_term_id, page_layout_key,
             custom_template_key, seo_json, parent_id, menu_order, entry_version
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(snapshot.entry_id)
    .bind(&snapshot.content_type_key)
    .bind(revision_number)
    .bind(content_hash)
    .bind(source.as_str())
    .bind(restored_from_revision_number)
    .bind(author_user_id)
    .bind(&author_username)
    .bind(&snapshot.payload.title)
    .bind(&snapshot.slug)
    .bind(snapshot.payload.content_mode.as_str())
    .bind(&values_json)
    .bind(snapshot.payload.builder_document_json.as_deref())
    .bind(&term_ids_json)
    .bind(snapshot.payload.primary_category_term_id)
    .bind(snapshot.payload.page_layout_key.as_deref())
    .bind(snapshot.payload.custom_template_key.as_deref())
    .bind(seo_json.as_deref())
    .bind(snapshot.payload.parent_id)
    .bind(snapshot.payload.menu_order)
    .bind(snapshot.version)
    .execute(&mut *conn)
    .await?;

    Ok(())
}

async fn prune_old_revisions_in_tx(conn: &mut SqliteConnection, entry_id: i64) -> ApiResult<()> {
    sqlx::query(
        "DELETE FROM entry_revisions
          WHERE entry_id = ?1
            AND id NOT IN (
                SELECT id
                  FROM entry_revisions
                 WHERE entry_id = ?1
                 ORDER BY revision_number DESC
                 LIMIT ?2
            )",
    )
    .bind(entry_id)
    .bind(MAX_REVISIONS_PER_ENTRY as i64)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Captures revision 1 (`source = 'baseline'`) before mutating a legacy entry that has
/// no revision history yet.
pub async fn capture_baseline_if_missing_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
) -> ApiResult<bool> {
    let has_revision: i64 =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM entry_revisions WHERE entry_id = ?)")
            .bind(entry_id)
            .fetch_one(&mut *conn)
            .await?;
    if has_revision != 0 {
        return Ok(false);
    }

    let snapshot = load_entry_canonical_snapshot_in_conn(&mut *conn, entry_id).await?;
    let content_hash = compute_revision_content_hash(&snapshot.payload);
    insert_revision_row_in_tx(
        &mut *conn,
        &snapshot,
        1,
        &content_hash,
        RevisionSource::Baseline,
        None,
        RevisionActor::Baseline,
    )
    .await?;
    Ok(true)
}

/// Captures a new revision if `content_hash` differs from the latest revision and prunes
/// older revisions beyond `MAX_REVISIONS_PER_ENTRY`.
pub async fn capture_entry_revision_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
    source: RevisionSource,
    restored_from_revision_number: Option<i64>,
    actor: RevisionActor<'_>,
) -> ApiResult<Option<i64>> {
    let snapshot = load_entry_canonical_snapshot_in_conn(&mut *conn, entry_id).await?;
    let content_hash = compute_revision_content_hash(&snapshot.payload);

    let latest: Option<(i64, String)> = sqlx::query_as(
        "SELECT revision_number, content_hash
           FROM entry_revisions
          WHERE entry_id = ?
          ORDER BY revision_number DESC
          LIMIT 1",
    )
    .bind(entry_id)
    .fetch_optional(&mut *conn)
    .await?;

    if let Some((_, ref latest_hash)) = latest
        && latest_hash == &content_hash
    {
        return Ok(None);
    }

    let next_rev = latest.map(|(rev, _)| rev + 1).unwrap_or(1);
    insert_revision_row_in_tx(
        &mut *conn,
        &snapshot,
        next_rev,
        &content_hash,
        source,
        restored_from_revision_number,
        actor,
    )
    .await?;
    prune_old_revisions_in_tx(&mut *conn, entry_id).await?;
    Ok(Some(next_rev))
}

pub async fn clear_actor_autosave_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
    user_id: i64,
) -> ApiResult<()> {
    sqlx::query("DELETE FROM entry_autosaves WHERE entry_id = ? AND user_id = ?")
        .bind(entry_id)
        .bind(user_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn clear_all_entry_autosaves_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
) -> ApiResult<()> {
    sqlx::query("DELETE FROM entry_autosaves WHERE entry_id = ?")
        .bind(entry_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn record_entry_revision_and_clear_autosave_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
    source: RevisionSource,
    actor: RevisionActor<'_>,
    clear_autosave_user_id: Option<i64>,
) -> ApiResult<Option<i64>> {
    let captured = capture_entry_revision_in_tx(&mut *conn, entry_id, source, None, actor).await?;
    if let Some(uid) = clear_autosave_user_id {
        clear_actor_autosave_in_tx(&mut *conn, entry_id, uid).await?;
    }
    Ok(captured)
}

/// Captures a create/update revision for `forbid_generic_pattern_write` paths while
/// preserving user autosaves when a plugin/theme `OwnerFence` performs a background save.
pub async fn record_fenced_entry_revision_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
    is_create: bool,
    fence: Option<&PluginActionFence>,
    actor: &SessionContext,
) -> ApiResult<Option<i64>> {
    match fence {
        Some(plugin_fence) => {
            capture_entry_revision_in_tx(
                &mut *conn,
                entry_id,
                RevisionSource::PluginSave,
                None,
                actor_from_plugin_fence(plugin_fence),
            )
            .await
        }
        None => {
            let source = if is_create {
                RevisionSource::Create
            } else {
                RevisionSource::Save
            };
            let clear_uid = if is_create { None } else { Some(actor.user.id) };
            record_entry_revision_and_clear_autosave_in_tx(
                &mut *conn,
                entry_id,
                source,
                RevisionActor::User(actor),
                clear_uid,
            )
            .await
        }
    }
}

pub async fn capture_import_revisions_in_conn(
    conn: &mut SqliteConnection,
    entry_ids: &[i64],
) -> ApiResult<()> {
    let mut seen = HashSet::with_capacity(entry_ids.len());
    for &entry_id in entry_ids {
        if seen.insert(entry_id) {
            capture_entry_revision_in_tx(
                &mut *conn,
                entry_id,
                RevisionSource::Import,
                None,
                RevisionActor::Import,
            )
            .await?;
        }
    }
    Ok(())
}
