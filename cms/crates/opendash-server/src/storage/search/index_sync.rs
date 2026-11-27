use std::collections::{BTreeMap, BTreeSet, HashMap};

use opendash_core::{
    block_schema::RegisteredBlockSchema, builder::BuilderDocument, content::EffectiveContentSchema,
    registry::RegistryCatalog, search::extract_searchable_document,
};
use serde_json::Value;
use sqlx::{SqliteConnection, SqlitePool};

use crate::error::ApiResult;

use super::ELIGIBLE_ENTRY_PREDICATE;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchIndexStatus {
    pub indexed_documents: i64,
    pub eligible_published_entries: i64,
    pub in_sync: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchReindexStats {
    pub indexed_documents: i64,
    pub eligible_published_entries: i64,
}

pub(in crate::storage) async fn delete_entry_search_index_in_conn(
    conn: &mut SqliteConnection,
    entry_id: i64,
) -> ApiResult<()> {
    sqlx::query("DELETE FROM entry_search_fts WHERE rowid = ?")
        .bind(entry_id)
        .execute(conn)
        .await?;
    Ok(())
}

pub(in crate::storage) async fn delete_locale_search_index_in_conn(
    conn: &mut SqliteConnection,
    locale: &str,
) -> ApiResult<()> {
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM content_entries WHERE locale = ?")
        .bind(locale)
        .fetch_all(&mut *conn)
        .await?;
    for id in ids {
        delete_entry_search_index_in_conn(&mut *conn, id).await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn upsert_entry_fts_row_in_conn(
    conn: &mut SqliteConnection,
    entry_id: i64,
    title: &str,
    values_json: &str,
    effective_schema: &EffectiveContentSchema,
    legacy_doc: Option<&BuilderDocument>,
    block_schemas: &RegistryCatalog<RegisteredBlockSchema>,
    term_names: &[String],
) -> ApiResult<()> {
    let values: BTreeMap<String, Value> = serde_json::from_str(values_json).unwrap_or_default();
    let doc = extract_searchable_document(
        title,
        &values,
        effective_schema,
        legacy_doc,
        block_schemas,
        term_names,
    );
    delete_entry_search_index_in_conn(&mut *conn, entry_id).await?;
    sqlx::query(
        "INSERT INTO entry_search_fts(rowid, title, summary, body, terms) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(entry_id)
    .bind(&doc.title_folded)
    .bind(&doc.summary_folded)
    .bind(&doc.body_folded)
    .bind(&doc.terms_folded)
    .execute(conn)
    .await?;
    Ok(())
}

async fn load_legacy_builder_docs_in_conn(
    conn: &mut SqliteConnection,
    entry_ids: &[i64],
) -> ApiResult<HashMap<i64, BuilderDocument>> {
    let mut out = HashMap::new();
    for chunk in entry_ids.chunks(500) {
        let ids_json = serde_json::to_string(chunk)?;
        let rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT entry_id, document_json
               FROM entry_builder_documents
              WHERE entry_id IN (SELECT value FROM json_each(?))",
        )
        .bind(ids_json)
        .fetch_all(&mut *conn)
        .await?;
        for (entry_id, raw) in rows {
            if let Ok(doc) = BuilderDocument::from_json(raw.as_bytes()) {
                out.insert(entry_id, doc);
            }
        }
    }
    Ok(out)
}

pub(in crate::storage) async fn sync_entry_search_index_in_conn(
    conn: &mut SqliteConnection,
    entry_id: i64,
) -> ApiResult<()> {
    let sql = format!(
        "SELECT e.content_type_key, e.title, e.values_json
           FROM content_entries e
           JOIN content_types t ON t.key = e.content_type_key
           JOIN site_locales sl ON sl.code = e.locale
          WHERE e.id = ? AND {ELIGIBLE_ENTRY_PREDICATE}"
    );
    let row: Option<(String, String, String)> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(entry_id)
        .fetch_optional(&mut *conn)
        .await?;
    let Some((type_key, title, values_json)) = row else {
        return delete_entry_search_index_in_conn(conn, entry_id).await;
    };

    let Some(effective_schema) =
        crate::storage::content::load_effective_content_schema_in_tx(&mut *conn, &type_key).await?
    else {
        return delete_entry_search_index_in_conn(conn, entry_id).await;
    };

    let block_schemas =
        crate::storage::page::load_unified_block_catalog_in_conn(&mut *conn).await?;
    let mut legacy_docs = load_legacy_builder_docs_in_conn(&mut *conn, &[entry_id]).await?;
    let mut terms_map = crate::storage::public_content::batch_load_public_entry_terms_in_conn(
        &mut *conn,
        &[(entry_id, type_key)],
    )
    .await?;
    let term_names: Vec<String> = terms_map
        .remove(&entry_id)
        .unwrap_or_default()
        .into_iter()
        .map(|t| t.name)
        .collect();

    upsert_entry_fts_row_in_conn(
        conn,
        entry_id,
        &title,
        &values_json,
        &effective_schema,
        legacy_docs.remove(&entry_id).as_ref(),
        &block_schemas,
        &term_names,
    )
    .await
}

pub(in crate::storage) async fn sync_entries_for_type_in_conn(
    conn: &mut SqliteConnection,
    type_key: &str,
) -> ApiResult<()> {
    if matches!(type_key, "pattern" | "section") {
        return Ok(());
    }

    let stale_sql = format!(
        "SELECT d.id
           FROM entry_search_fts_docsize d
           JOIN content_entries e ON e.id = d.id
          WHERE e.content_type_key = ?
            AND NOT EXISTS (
                SELECT 1
                  FROM content_types t
                  JOIN site_locales sl ON sl.code = e.locale
                 WHERE t.key = e.content_type_key
                   AND {ELIGIBLE_ENTRY_PREDICATE}
            )"
    );
    let stale_ids: Vec<i64> = sqlx::query_scalar(sqlx::AssertSqlSafe(stale_sql))
        .bind(type_key)
        .fetch_all(&mut *conn)
        .await?;
    for id in stale_ids {
        delete_entry_search_index_in_conn(&mut *conn, id).await?;
    }

    let Some(effective_schema) =
        crate::storage::content::load_effective_content_schema_in_tx(&mut *conn, type_key).await?
    else {
        return Ok(());
    };
    let block_schemas =
        crate::storage::page::load_unified_block_catalog_in_conn(&mut *conn).await?;

    let select_sql = format!(
        "SELECT e.id, e.title, e.values_json
           FROM content_entries e
           JOIN content_types t ON t.key = e.content_type_key
           JOIN site_locales sl ON sl.code = e.locale
          WHERE e.content_type_key = ?
            AND e.id > ?
            AND {ELIGIBLE_ENTRY_PREDICATE}
          ORDER BY e.id ASC
          LIMIT 200"
    );

    let mut after_id = 0i64;
    loop {
        let rows: Vec<(i64, String, String)> =
            sqlx::query_as(sqlx::AssertSqlSafe(select_sql.clone()))
                .bind(type_key)
                .bind(after_id)
                .fetch_all(&mut *conn)
                .await?;
        if rows.is_empty() {
            break;
        }
        after_id = rows.last().map(|r| r.0).unwrap_or(after_id);
        let pairs: Vec<(i64, String)> = rows
            .iter()
            .map(|(id, _, _)| (*id, type_key.to_owned()))
            .collect();
        let ids: Vec<i64> = rows.iter().map(|(id, _, _)| *id).collect();
        let mut legacy_docs = load_legacy_builder_docs_in_conn(&mut *conn, &ids).await?;
        let mut terms_map = crate::storage::public_content::batch_load_public_entry_terms_in_conn(
            &mut *conn, &pairs,
        )
        .await?;

        for (id, title, values_json) in rows {
            let term_names: Vec<String> = terms_map
                .remove(&id)
                .unwrap_or_default()
                .into_iter()
                .map(|t| t.name)
                .collect();
            upsert_entry_fts_row_in_conn(
                conn,
                id,
                &title,
                &values_json,
                &effective_schema,
                legacy_docs.remove(&id).as_ref(),
                &block_schemas,
                &term_names,
            )
            .await?;
        }
    }
    Ok(())
}

pub(in crate::storage) async fn sync_entries_for_term_in_conn(
    conn: &mut SqliteConnection,
    term_id: i64,
) -> ApiResult<()> {
    let entry_ids: Vec<i64> =
        sqlx::query_scalar("SELECT DISTINCT entry_id FROM entry_terms WHERE term_id = ?")
            .bind(term_id)
            .fetch_all(&mut *conn)
            .await?;
    for entry_id in entry_ids {
        sync_entry_search_index_in_conn(&mut *conn, entry_id).await?;
    }
    Ok(())
}

pub(in crate::storage) async fn collect_type_keys_for_taxonomy_in_conn(
    conn: &mut SqliteConnection,
    taxonomy_key: &str,
) -> ApiResult<BTreeSet<String>> {
    let mut keys = BTreeSet::new();
    let elig: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT content_type_key
           FROM content_type_taxonomy_eligibility
          WHERE taxonomy_kind = ?",
    )
    .bind(taxonomy_key)
    .fetch_all(&mut *conn)
    .await?;
    keys.extend(elig);

    let ovr: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT content_type_key
           FROM content_type_taxonomy_overrides
          WHERE EXISTS (SELECT 1 FROM json_each(taxonomies_json) WHERE value = ?)",
    )
    .bind(taxonomy_key)
    .fetch_all(&mut *conn)
    .await?;
    keys.extend(ovr);

    let linked: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT e.content_type_key
           FROM entry_terms et
           JOIN taxonomy_terms tt ON tt.id = et.term_id
           JOIN content_entries e ON e.id = et.entry_id
          WHERE tt.kind = ?",
    )
    .bind(taxonomy_key)
    .fetch_all(&mut *conn)
    .await?;
    keys.extend(linked);
    Ok(keys)
}

pub(in crate::storage) async fn sync_entries_for_taxonomy_in_conn(
    conn: &mut SqliteConnection,
    taxonomy_key: &str,
) -> ApiResult<()> {
    let type_keys = collect_type_keys_for_taxonomy_in_conn(conn, taxonomy_key).await?;
    for key in type_keys {
        sync_entries_for_type_in_conn(conn, &key).await?;
    }
    Ok(())
}

pub(in crate::storage) async fn sync_builder_entries_in_conn(
    conn: &mut SqliteConnection,
) -> ApiResult<()> {
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id
           FROM content_entries
          WHERE content_type_key NOT IN ('pattern', 'section')
            AND (
                content_mode = 'builder'
                OR values_json LIKE '%<!-- od:%'
                OR EXISTS (SELECT 1 FROM entry_builder_documents d WHERE d.entry_id = content_entries.id)
            )",
    )
    .fetch_all(&mut *conn)
    .await?;
    for id in ids {
        sync_entry_search_index_in_conn(conn, id).await?;
    }
    Ok(())
}

pub async fn rebuild_search_index_in_conn(
    conn: &mut SqliteConnection,
) -> ApiResult<SearchReindexStats> {
    sqlx::query("INSERT INTO entry_search_fts(entry_search_fts) VALUES ('delete-all')")
        .execute(&mut *conn)
        .await?;

    let block_schemas =
        crate::storage::page::load_unified_block_catalog_in_conn(&mut *conn).await?;
    let mut schemas_cache: HashMap<String, Option<EffectiveContentSchema>> = HashMap::new();
    let batch_sql = format!(
        "SELECT e.id, e.content_type_key, e.title, e.values_json
           FROM content_entries e
           JOIN content_types t ON t.key = e.content_type_key
           JOIN site_locales sl ON sl.code = e.locale
          WHERE e.id > ?
            AND {ELIGIBLE_ENTRY_PREDICATE}
          ORDER BY e.id ASC
          LIMIT 200"
    );

    let mut after_id = 0i64;
    let mut indexed_documents = 0i64;

    loop {
        let rows: Vec<(i64, String, String, String)> =
            sqlx::query_as(sqlx::AssertSqlSafe(batch_sql.clone()))
                .bind(after_id)
                .fetch_all(&mut *conn)
                .await?;
        if rows.is_empty() {
            break;
        }
        after_id = rows.last().map(|r| r.0).unwrap_or(after_id);
        let ids: Vec<i64> = rows.iter().map(|r| r.0).collect();
        let pairs: Vec<(i64, String)> = rows.iter().map(|r| (r.0, r.1.clone())).collect();
        let mut legacy_docs = load_legacy_builder_docs_in_conn(&mut *conn, &ids).await?;
        let mut terms_map = crate::storage::public_content::batch_load_public_entry_terms_in_conn(
            &mut *conn, &pairs,
        )
        .await?;

        for (id, type_key, title, values_json) in rows {
            if !schemas_cache.contains_key(&type_key) {
                let loaded = crate::storage::content::load_effective_content_schema_in_tx(
                    &mut *conn, &type_key,
                )
                .await?;
                schemas_cache.insert(type_key.clone(), loaded);
            }
            let Some(Some(effective_schema)) = schemas_cache.get(&type_key) else {
                continue;
            };
            let term_names: Vec<String> = terms_map
                .remove(&id)
                .unwrap_or_default()
                .into_iter()
                .map(|t| t.name)
                .collect();
            let values: BTreeMap<String, Value> =
                serde_json::from_str(&values_json).unwrap_or_default();
            let doc = extract_searchable_document(
                &title,
                &values,
                effective_schema,
                legacy_docs.remove(&id).as_ref(),
                &block_schemas,
                &term_names,
            );
            sqlx::query(
                "INSERT INTO entry_search_fts(rowid, title, summary, body, terms) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(&doc.title_folded)
            .bind(&doc.summary_folded)
            .bind(&doc.body_folded)
            .bind(&doc.terms_folded)
            .execute(&mut *conn)
            .await?;
            indexed_documents += 1;
        }
    }

    Ok(SearchReindexStats {
        indexed_documents,
        eligible_published_entries: indexed_documents,
    })
}

#[allow(dead_code)] // Consumed by S03/S04 Admin & background search_reindex job.
pub async fn rebuild_search_index(db: &SqlitePool) -> ApiResult<SearchReindexStats> {
    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
    let stats = rebuild_search_index_in_conn(&mut tx).await?;
    tx.commit().await?;
    Ok(stats)
}

pub async fn get_search_index_status_in_conn(
    conn: &mut SqliteConnection,
) -> ApiResult<SearchIndexStatus> {
    let fts_sig: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(MIN(id), 0), COALESCE(MAX(id), 0), CAST(COALESCE(SUM(id), 0) AS INTEGER)
           FROM entry_search_fts_docsize",
    )
    .fetch_one(&mut *conn)
    .await?;

    let eligible_sql = format!(
        "SELECT COUNT(*), COALESCE(MIN(e.id), 0), COALESCE(MAX(e.id), 0), CAST(COALESCE(SUM(e.id), 0) AS INTEGER)
           FROM content_entries e
           JOIN content_types t ON t.key = e.content_type_key
           JOIN site_locales sl ON sl.code = e.locale
          WHERE {ELIGIBLE_ENTRY_PREDICATE}"
    );
    let eligible_sig: (i64, i64, i64, i64) = sqlx::query_as(sqlx::AssertSqlSafe(eligible_sql))
        .fetch_one(&mut *conn)
        .await?;

    let mut in_sync = fts_sig == eligible_sig;
    if in_sync {
        let missing_sql = format!(
            "SELECT EXISTS(
                SELECT 1
                  FROM content_entries e
                  JOIN content_types t ON t.key = e.content_type_key
                  JOIN site_locales sl ON sl.code = e.locale
                 WHERE {ELIGIBLE_ENTRY_PREDICATE}
                   AND NOT EXISTS (SELECT 1 FROM entry_search_fts_docsize d WHERE d.id = e.id)
                 LIMIT 1
            )"
        );
        let missing: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(missing_sql))
            .fetch_one(&mut *conn)
            .await?;

        let stale_sql = format!(
            "SELECT EXISTS(
                SELECT 1
                  FROM entry_search_fts_docsize d
                 WHERE NOT EXISTS (
                     SELECT 1
                       FROM content_entries e
                       JOIN content_types t ON t.key = e.content_type_key
                       JOIN site_locales sl ON sl.code = e.locale
                      WHERE e.id = d.id AND {ELIGIBLE_ENTRY_PREDICATE}
                 )
                 LIMIT 1
            )"
        );
        let stale: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(stale_sql))
            .fetch_one(&mut *conn)
            .await?;
        in_sync = !missing && !stale;
    }

    Ok(SearchIndexStatus {
        indexed_documents: fts_sig.0,
        eligible_published_entries: eligible_sig.0,
        in_sync,
    })
}

#[allow(dead_code)] // Consumed by S03/S04 Admin Search Index status screen.
pub async fn get_search_index_status(db: &SqlitePool) -> ApiResult<SearchIndexStatus> {
    let mut conn = db.acquire().await?;
    get_search_index_status_in_conn(&mut conn).await
}

/// Checks FTS5 index consistency on server startup (after migrations and import
/// recovery) and automatically rebuilds when out of sync.
pub async fn ensure_search_index_bootstrapped(db: &SqlitePool) -> ApiResult<bool> {
    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
    let status = get_search_index_status_in_conn(&mut tx).await?;
    if status.in_sync {
        tx.commit().await?;
        return Ok(false);
    }
    rebuild_search_index_in_conn(&mut tx).await?;
    tx.commit().await?;
    Ok(true)
}
