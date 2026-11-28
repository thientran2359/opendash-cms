//! Related entries queries in transaction/pool preserving ACF Location Rules & `LoopProjection`.

use sqlx::{Sqlite, SqlitePool, Transaction};

use super::*;

pub(super) fn filter_specs_for_loop_projection<'a>(
    ordered: Vec<&'a opendash_core::field_schema::FieldSpec>,
    projection: &opendash_core::builder::LoopProjection,
) -> Vec<&'a opendash_core::field_schema::FieldSpec> {
    if projection.needs_all_fields {
        return ordered;
    }
    let has_featured = ordered.iter().any(|s| s.key == "featured_image");
    let first_image_key = if has_featured {
        Some("featured_image")
    } else {
        ordered
            .iter()
            .find(|s| {
                s.field_type == opendash_core::field_schema::FieldType::Media
                    && s.effective_media_kind()
                        == opendash_core::field_schema::MediaFieldKind::Image
            })
            .map(|s| s.key.as_str())
    };
    let needs_fallback_text = (projection.needs_excerpt || projection.needs_reading_time)
        && !ordered
            .iter()
            .any(|s| matches!(s.key.as_str(), "excerpt" | "summary" | "body"));
    ordered
        .into_iter()
        .filter(|spec| {
            if projection.custom_field_keys.contains(&spec.key) {
                return true;
            }
            if projection.needs_cover && first_image_key == Some(spec.key.as_str()) {
                return true;
            }
            if (projection.needs_excerpt || projection.needs_reading_time)
                && (matches!(spec.key.as_str(), "body")
                    || (projection.needs_excerpt
                        && matches!(spec.key.as_str(), "excerpt" | "summary"))
                    || (needs_fallback_text
                        && !matches!(
                            spec.field_type,
                            opendash_core::field_schema::FieldType::Media
                                | opendash_core::field_schema::FieldType::Gallery
                                | opendash_core::field_schema::FieldType::EntryRef
                                | opendash_core::field_schema::FieldType::TermRef
                        )))
            {
                return true;
            }
            false
        })
        .collect()
}

/// Fetches up to `limit` (clamped <= 20) published entries in the same CPT and locale
/// sharing at least one public, active, declared, CPT-eligible taxonomy term.
pub async fn query_related_entries_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    entry_id: i64,
    type_key: &str,
    limit: usize,
    is_preview: bool,
    projection: &opendash_core::builder::LoopProjection,
) -> ApiResult<Vec<crate::content_render::EntryView>> {
    let clamped = (limit.min(20)) as i64;
    if clamped == 0 || matches!(type_key, "pattern" | "section") {
        return Ok(Vec::new());
    }
    let Some(source_locale): Option<String> = sqlx::query_scalar(
        "SELECT locale FROM content_entries WHERE id = ? AND trashed_at IS NULL",
    )
    .bind(entry_id)
    .fetch_optional(&mut **tx)
    .await?
    else {
        return Ok(Vec::new());
    };
    let rows: Vec<HydratedEntrySqlRow> = sqlx::query_as(
        "SELECT e.id, e.content_type_key, e.slug, e.title, e.values_json,
                e.publication_status, e.version, e.created_at, e.updated_at, e.published_at,
                e.primary_category_term_id, e.locale, e.parent_id, e.menu_order
           FROM content_entries e
           JOIN site_locales sl ON sl.code = e.locale AND sl.deleted_at IS NULL
            AND (? = 1 OR (sl.enabled = 1 AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)))
           JOIN content_types t ON t.key = e.content_type_key
           LEFT JOIN content_type_taxonomy_overrides override
             ON override.content_type_key = e.content_type_key
          WHERE e.content_type_key = ?
            AND e.locale = ?
            AND e.id != ?
            AND e.trashed_at IS NULL
            AND e.publication_status = 'published'
            AND e.content_type_key != 'pattern'
            AND t.provider_status = 'active'
            AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
            AND EXISTS (
                SELECT 1
                  FROM entry_terms et1
                  JOIN entry_terms et2 ON et2.term_id = et1.term_id
                  JOIN taxonomy_terms tt ON tt.id = et1.term_id
                  JOIN taxonomy_definitions d ON d.key = tt.kind
                 WHERE et1.entry_id = ?
                   AND et2.entry_id = e.id
                   AND tt.locale = e.locale
                   AND d.public = 1
                   AND d.provider_status = 'active'
                   AND d.declaration_state = 'declared'
                   AND (
                       (override.content_type_key IS NOT NULL AND EXISTS (
                           SELECT 1 FROM json_each(override.taxonomies_json) sel
                            WHERE sel.value = tt.kind
                       ))
                       OR
                       (override.content_type_key IS NULL AND EXISTS (
                           SELECT 1 FROM content_type_taxonomy_eligibility el
                            WHERE el.content_type_key = e.content_type_key
                              AND el.taxonomy_kind = tt.kind
                              AND el.provider_status = 'active'
                              AND el.declaration_state = 'declared'
                       ))
                   )
            )
          ORDER BY COALESCE(e.published_at, e.updated_at) DESC, e.id DESC
          LIMIT ?",
    )
    .bind(i64::from(is_preview))
    .bind(type_key)
    .bind(&source_locale)
    .bind(entry_id)
    .bind(entry_id)
    .bind(clamped)
    .fetch_all(&mut **tx)
    .await?;

    let records = decode_hydrated_entry_rows(rows)?;
    hydrate_entry_views_projected_in_conn(
        &mut **tx,
        records,
        Some(&source_locale),
        is_preview,
        opendash_core::content::FieldRole::Editor,
        None,
        projection,
    )
    .await
}

/// Fetches up to `limit` (clamped <= 20) recent published `post` entries in the given
/// (or default) locale, optionally excluding `exclude_post_id`.
pub async fn query_related_entries(
    db: &SqlitePool,
    entry_id: i64,
    type_key: &str,
    limit: usize,
) -> ApiResult<Vec<crate::content_render::EntryView>> {
    let mut tx = db.begin().await?;
    let out = query_related_entries_in_tx(
        &mut tx,
        entry_id,
        type_key,
        limit,
        false,
        &opendash_core::builder::LoopProjection::all(),
    )
    .await?;
    tx.commit().await?;
    Ok(out)
}
