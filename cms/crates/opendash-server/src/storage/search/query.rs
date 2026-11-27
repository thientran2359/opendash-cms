use std::collections::{BTreeMap, BTreeSet, HashMap};

use opendash_core::{
    builder::BuilderDocument,
    content::{EffectiveContentSchema, FieldRole, valid_content_type_key},
    search::{
        SnippetSegment, build_structured_snippet, extract_searchable_document,
        sanitize_search_query,
    },
};
use sqlx::{FromRow, SqliteConnection, SqlitePool};

use crate::{
    content_render::{EntryView, entry_view_with_context},
    error::{ApiError, ApiResult},
    storage::entries::ContentEntryRecord,
};

use super::ELIGIBLE_ENTRY_PREDICATE;

pub const SEARCH_SNIPPET_MAX_CHARS: usize = 220;

pub struct PublicSearchQueryInput<'a> {
    pub raw_query: Option<&'a str>,
    pub locale: Option<&'a str>,
    pub content_type: Option<&'a str>,
    pub page: i64,
    pub per_page: i64,
}

#[derive(Debug, Clone)]
pub struct PublicSearchHit {
    pub view: EntryView,
    pub type_label: String,
    pub snippet_segments: Vec<SnippetSegment>,
    pub rank: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PublicSearchFacet {
    pub type_key: String,
    pub label: String,
    pub count: i64,
    pub is_active: bool,
}

#[derive(Debug, Clone)]
pub struct PublicSearchOutput {
    pub locale: String,
    pub query: String,
    pub has_query: bool,
    pub tokens: Vec<String>,
    pub content_type: Option<String>,
    pub page: i64,
    pub per_page: i64,
    pub total_items: i64,
    pub facets: Vec<PublicSearchFacet>,
    pub hits: Vec<PublicSearchHit>,
}

#[derive(FromRow)]
struct SearchEntryDbRow {
    id: i64,
    content_type_key: String,
    type_label: String,
    locale: String,
    translation_group_id: String,
    slug: String,
    title: String,
    values_json: String,
    publication_status: String,
    version: i64,
    created_at: String,
    updated_at: String,
    published_at: Option<String>,
    page_layout_key: Option<String>,
    custom_template_key: Option<String>,
    content_mode: String,
    primary_category_term_id: Option<i64>,
    rank_score: f64,
}

pub async fn query_public_search(
    db: &SqlitePool,
    input: &PublicSearchQueryInput<'_>,
) -> ApiResult<PublicSearchOutput> {
    let mut conn = db.acquire().await?;
    query_public_search_in_conn(&mut conn, input).await
}

pub async fn query_public_search_in_conn(
    conn: &mut SqliteConnection,
    input: &PublicSearchQueryInput<'_>,
) -> ApiResult<PublicSearchOutput> {
    let page = input.page.max(1);
    let per_page = input.per_page.clamp(1, 20);
    let resolved_locale =
        crate::storage::settings::resolve_public_locale_in_conn(&mut *conn, input.locale).await?;

    let normalized_type = input
        .content_type
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);
    if let Some(ref tk) = normalized_type
        && !valid_content_type_key(tk)
    {
        return Err(ApiError::bad_request(
            "Khóa loại nội dung tìm kiếm không hợp lệ.",
        ));
    }

    let sanitized = sanitize_search_query(input.raw_query.unwrap_or(""))
        .map_err(|err| ApiError::bad_request(err.to_string()))?;

    let Some(match_expr) = sanitized.fts_match_expr.as_deref() else {
        return Ok(PublicSearchOutput {
            locale: resolved_locale,
            query: sanitized.display_query,
            has_query: !sanitized.is_empty,
            tokens: sanitized.tokens,
            content_type: normalized_type,
            page,
            per_page,
            total_items: 0,
            facets: Vec::new(),
            hits: Vec::new(),
        });
    };

    let facet_sql = format!(
        "SELECT t.key,
                COALESCE(json_extract(t.definition_json, '$.label'), t.key) AS label,
                COUNT(*) AS cnt
           FROM entry_search_fts
           JOIN content_entries e ON e.id = entry_search_fts.rowid
           JOIN content_types t ON t.key = e.content_type_key
           JOIN site_locales sl ON sl.code = e.locale
          WHERE entry_search_fts MATCH ?1
            AND e.locale = ?2
            AND {ELIGIBLE_ENTRY_PREDICATE}
          GROUP BY t.key
          ORDER BY cnt DESC, t.key ASC"
    );
    let facet_rows: Vec<(String, String, i64)> = sqlx::query_as(sqlx::AssertSqlSafe(facet_sql))
        .bind(match_expr)
        .bind(&resolved_locale)
        .fetch_all(&mut *conn)
        .await?;

    let facets: Vec<PublicSearchFacet> = facet_rows
        .into_iter()
        .map(|(type_key, label, count)| PublicSearchFacet {
            is_active: normalized_type.as_deref() == Some(type_key.as_str()),
            type_key,
            label,
            count,
        })
        .collect();

    if matches!(normalized_type.as_deref(), Some("pattern" | "section")) {
        return Ok(PublicSearchOutput {
            locale: resolved_locale,
            query: sanitized.display_query,
            has_query: true,
            tokens: sanitized.tokens,
            content_type: normalized_type,
            page,
            per_page,
            total_items: 0,
            facets,
            hits: Vec::new(),
        });
    }

    let count_sql = format!(
        "SELECT COUNT(*)
           FROM entry_search_fts
           JOIN content_entries e ON e.id = entry_search_fts.rowid
           JOIN content_types t ON t.key = e.content_type_key
           JOIN site_locales sl ON sl.code = e.locale
          WHERE entry_search_fts MATCH ?1
            AND e.locale = ?2
            AND (?3 IS NULL OR e.content_type_key = ?3)
            AND {ELIGIBLE_ENTRY_PREDICATE}"
    );
    let total_items: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql))
        .bind(match_expr)
        .bind(&resolved_locale)
        .bind(normalized_type.as_deref())
        .fetch_one(&mut *conn)
        .await?;

    if total_items == 0 {
        return Ok(PublicSearchOutput {
            locale: resolved_locale,
            query: sanitized.display_query,
            has_query: true,
            tokens: sanitized.tokens,
            content_type: normalized_type,
            page,
            per_page,
            total_items: 0,
            facets,
            hits: Vec::new(),
        });
    }

    let offset = (page - 1).saturating_mul(per_page);
    let hits_sql = format!(
        "SELECT e.id, e.content_type_key,
                COALESCE(json_extract(t.definition_json, '$.label'), t.key) AS type_label,
                e.locale, e.translation_group_id, e.slug, e.title, e.values_json,
                e.publication_status, e.version, e.created_at, e.updated_at, e.published_at,
                e.page_layout_key, e.custom_template_key, e.content_mode,
                e.primary_category_term_id,
                bm25(entry_search_fts, 10.0, 5.0, 1.0, 3.0) AS rank_score
           FROM entry_search_fts
           JOIN content_entries e ON e.id = entry_search_fts.rowid
           JOIN content_types t ON t.key = e.content_type_key
           JOIN site_locales sl ON sl.code = e.locale
          WHERE entry_search_fts MATCH ?1
            AND e.locale = ?2
            AND (?3 IS NULL OR e.content_type_key = ?3)
            AND {ELIGIBLE_ENTRY_PREDICATE}
          ORDER BY rank_score ASC, COALESCE(e.published_at, e.created_at) DESC, e.id DESC
          LIMIT ?4 OFFSET ?5"
    );
    let rows: Vec<SearchEntryDbRow> = sqlx::query_as(sqlx::AssertSqlSafe(hits_sql))
        .bind(match_expr)
        .bind(&resolved_locale)
        .bind(normalized_type.as_deref())
        .bind(per_page)
        .bind(offset)
        .fetch_all(&mut *conn)
        .await?;

    let hits = hydrate_search_hits_in_conn(&mut *conn, rows, &sanitized.tokens).await?;

    Ok(PublicSearchOutput {
        locale: resolved_locale,
        query: sanitized.display_query,
        has_query: true,
        tokens: sanitized.tokens,
        content_type: normalized_type,
        page,
        per_page,
        total_items,
        facets,
        hits,
    })
}

async fn hydrate_search_hits_in_conn(
    conn: &mut SqliteConnection,
    rows: Vec<SearchEntryDbRow>,
    tokens: &[String],
) -> ApiResult<Vec<PublicSearchHit>> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let entry_ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let pairs: Vec<(i64, String)> = rows
        .iter()
        .map(|r| (r.id, r.content_type_key.clone()))
        .collect();
    let distinct_types: Vec<String> = rows
        .iter()
        .map(|r| r.content_type_key.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    let urls = crate::storage::seo::resolve_public_entry_paths_by_entry_ids_in_conn(
        &mut *conn, &entry_ids,
    )
    .await?;
    let mut terms_map =
        crate::storage::public_content::batch_load_public_entry_terms_in_conn(&mut *conn, &pairs)
            .await?;
    let tax_map = crate::storage::public_content::batch_load_eligible_public_taxonomies_in_conn(
        &mut *conn,
        &distinct_types,
    )
    .await?;
    let mut proj_map = crate::storage::plugin_projections::batch_load_entry_projections_in_conn(
        &mut *conn, &entry_ids,
    )
    .await?;
    let block_schemas =
        crate::storage::page::load_unified_block_catalog_in_conn(&mut *conn).await?;

    let mut legacy_docs = HashMap::new();
    for chunk in entry_ids.chunks(500) {
        let ids_json = serde_json::to_string(chunk)?;
        let doc_rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT entry_id, document_json
               FROM entry_builder_documents
              WHERE entry_id IN (SELECT value FROM json_each(?))",
        )
        .bind(ids_json)
        .fetch_all(&mut *conn)
        .await?;
        for (eid, raw) in doc_rows {
            if let Ok(doc) = BuilderDocument::from_json(raw.as_bytes()) {
                legacy_docs.insert(eid, doc);
            }
        }
    }

    let mut schemas: HashMap<String, EffectiveContentSchema> = HashMap::new();
    for tk in &distinct_types {
        if let Some(eff) =
            crate::storage::content::load_effective_content_schema_in_tx(&mut *conn, tk).await?
        {
            schemas.insert(tk.clone(), eff);
        }
    }

    let mut hits = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(eff) = schemas.get(&row.content_type_key) else {
            continue;
        };
        let values: BTreeMap<String, serde_json::Value> =
            serde_json::from_str(&row.values_json).unwrap_or_default();
        let terms = terms_map.remove(&row.id).unwrap_or_default();
        let term_names: Vec<String> = terms.iter().map(|t| t.name.clone()).collect();
        let searchable = extract_searchable_document(
            &row.title,
            &values,
            eff,
            legacy_docs.get(&row.id),
            &block_schemas,
            &term_names,
        );
        let snippet_segments = build_structured_snippet(
            &searchable.snippet_source_text,
            tokens,
            SEARCH_SNIPPET_MAX_CHARS,
        );

        let content_mode = match row.content_mode.as_str() {
            "builder" => opendash_core::content::ContentMode::Builder,
            _ => opendash_core::content::ContentMode::Editor,
        };
        let record = ContentEntryRecord {
            id: row.id,
            content_type_key: row.content_type_key,
            locale: row.locale,
            translation_group_id: row.translation_group_id,
            slug: row.slug,
            title: row.title,
            values,
            term_ids: terms.iter().map(|t| t.id).collect(),
            primary_category_term_id: row.primary_category_term_id,
            page_layout_key: row.page_layout_key,
            custom_template_key: row.custom_template_key,
            content_mode,
            publication_status: row.publication_status,
            version: row.version,
            created_at: row.created_at,
            updated_at: row.updated_at,
            published_at: row.published_at,
            translations: Vec::new(),
        };

        let url = urls.get(&record.id).cloned();
        let eligible_taxonomies = tax_map
            .get(&record.content_type_key)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut view = entry_view_with_context(
            &record,
            eff,
            url,
            &terms,
            eligible_taxonomies,
            FieldRole::Editor,
        );
        if let Some(proj) = proj_map.remove(&record.id) {
            view.ext = proj.ext;
            view.theme_ext = proj.theme_ext;
        }
        view.snippet_segments = snippet_segments.clone();
        view.type_label = row.type_label.clone();
        hits.push(PublicSearchHit {
            view,
            type_label: row.type_label,
            snippet_segments,
            rank: row.rank_score,
        });
    }

    Ok(hits)
}
