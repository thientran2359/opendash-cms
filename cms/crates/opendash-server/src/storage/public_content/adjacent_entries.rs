//! Adjacent (Prev/Next) published entry queries using Migration `0063` partial index
//! `idx_content_entries_pub_order` with zero temporary B-tree sorting.

use std::collections::{BTreeMap, BTreeSet};

use opendash_core::content::{EffectiveFieldStatus, FieldRole, PublicExposure};
use opendash_core::field_schema::{FieldType, MediaFieldKind, valid_sha256_media_id};
use serde_json::Value;
use sqlx::{Sqlite, Transaction};

use super::*;
use crate::content_render::{AdjacentEntriesView, AdjacentEntryItem, HydratedMedia};

type AdjacentSqlRow = (i64, String, String, String, Option<String>);

fn select_cover_media_spec_keys(
    schema: &opendash_core::content::EffectiveContentSchema,
) -> Option<&str> {
    if let Some(fe) = schema.effective_fields.get("featured_image")
        && !fe.definition.is_ui_only()
        && fe.effective_status == EffectiveFieldStatus::Active
        && fe.definition.field_type == FieldType::Media
        && fe.definition.effective_media_kind() == MediaFieldKind::Image
        && fe.definition.effective_acl().2 == PublicExposure::Public
        && fe.definition.effective_acl().0 <= FieldRole::Editor
    {
        return Some("featured_image");
    }
    schema
        .effective_fields
        .iter()
        .find(|(_, fe)| {
            !fe.definition.is_ui_only()
                && fe.effective_status == EffectiveFieldStatus::Active
                && fe.definition.field_type == FieldType::Media
                && fe.definition.effective_media_kind() == MediaFieldKind::Image
                && fe.definition.effective_acl().2 == PublicExposure::Public
                && fe.definition.effective_acl().0 <= FieldRole::Editor
        })
        .map(|(k, _)| k.as_str())
}

#[allow(dead_code)]
pub(in crate::storage) fn build_adjacent_sql(
    newer: bool,
    taxonomy_filter: AdjacentTaxFilter,
) -> String {
    let cmp_clause = if newer {
        "AND COALESCE(e.published_at, e.updated_at) >= ? \
         AND (COALESCE(e.published_at, e.updated_at) > ? OR e.id > ?)"
    } else {
        "AND COALESCE(e.published_at, e.updated_at) <= ? \
         AND (COALESCE(e.published_at, e.updated_at) < ? OR e.id < ?)"
    };
    let order_clause = if newer {
        "ORDER BY COALESCE(e.published_at, e.updated_at) ASC, e.id ASC LIMIT 1"
    } else {
        "ORDER BY COALESCE(e.published_at, e.updated_at) DESC, e.id DESC LIMIT 1"
    };
    let tax_clause = match taxonomy_filter {
        AdjacentTaxFilter::None => "",
        AdjacentTaxFilter::PrimaryCategory => "AND e.primary_category_term_id = ?",
        AdjacentTaxFilter::SharedEligibleTerm => {
            "AND EXISTS (\
                SELECT 1 \
                  FROM entry_terms et1 \
                  JOIN entry_terms et2 ON et2.term_id = et1.term_id \
                  JOIN taxonomy_terms tt ON tt.id = et1.term_id \
                  JOIN taxonomy_definitions d ON d.key = tt.kind \
                 WHERE et1.entry_id = ? \
                   AND et2.entry_id = e.id \
                   AND tt.locale = e.locale \
                   AND d.public = 1 \
                   AND d.provider_status = 'active' \
                   AND d.declaration_state = 'declared' \
                   AND (\
                       (EXISTS (SELECT 1 FROM content_type_taxonomy_overrides o WHERE o.content_type_key = e.content_type_key) \
                        AND EXISTS (SELECT 1 FROM content_type_taxonomy_overrides o, json_each(o.taxonomies_json) sel WHERE o.content_type_key = e.content_type_key AND sel.value = tt.kind)) \
                       OR \
                       (NOT EXISTS (SELECT 1 FROM content_type_taxonomy_overrides o WHERE o.content_type_key = e.content_type_key) \
                        AND EXISTS (SELECT 1 FROM content_type_taxonomy_eligibility el WHERE el.content_type_key = e.content_type_key AND el.taxonomy_kind = tt.kind AND el.provider_status = 'active' AND el.declaration_state = 'declared'))\
                   )\
            )"
        }
    };
    format!(
        "SELECT e.id, e.slug, e.title, e.values_json, e.published_at \
           FROM content_entries e \
          WHERE e.content_type_key = ? \
            AND e.locale = ? \
            AND e.publication_status = 'published' \
            AND e.trashed_at IS NULL \
            AND EXISTS (\
                SELECT 1 FROM content_types t \
                 WHERE t.key = e.content_type_key \
                   AND t.provider_status = 'active' \
                   AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1\
            ) \
            AND EXISTS (\
                SELECT 1 FROM site_locales sl \
                 WHERE sl.code = e.locale \
                   AND sl.deleted_at IS NULL \
                   AND (? = 1 OR (sl.enabled = 1 AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)))\
            ) \
            {cmp_clause} \
            {tax_clause} \
          {order_clause}"
    )
}

#[derive(Debug, Clone, Copy)]
pub(in crate::storage) enum AdjacentTaxFilter {
    None,
    PrimaryCategory,
    SharedEligibleTerm,
}

async fn query_one_adjacent_side(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    locale: &str,
    sort_ts: &str,
    anchor_id: i64,
    is_preview: bool,
    newer: bool,
    tax_filter: AdjacentTaxFilter,
    tax_bind_id: i64,
) -> ApiResult<Option<AdjacentSqlRow>> {
    let sql = build_adjacent_sql(newer, tax_filter);
    let mut q = sqlx::query_as::<_, AdjacentSqlRow>(sqlx::AssertSqlSafe(sql))
        .bind(type_key)
        .bind(locale)
        .bind(i64::from(is_preview))
        .bind(sort_ts)
        .bind(sort_ts)
        .bind(anchor_id);
    if !matches!(tax_filter, AdjacentTaxFilter::None) {
        q = q.bind(tax_bind_id);
    }
    q.fetch_optional(&mut **tx).await.map_err(Into::into)
}

pub async fn query_adjacent_entries_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    entry_id: i64,
    type_key: &str,
    locale: &str,
    same_taxonomy: bool,
    is_preview: bool,
) -> ApiResult<AdjacentEntriesView> {
    if matches!(type_key, "pattern" | "section") {
        return Ok(AdjacentEntriesView::default());
    }
    let anchor: Option<(String, Option<i64>)> = sqlx::query_as(
        "SELECT COALESCE(e.published_at, e.updated_at), e.primary_category_term_id
           FROM content_entries e
           JOIN site_locales sl ON sl.code = e.locale AND sl.deleted_at IS NULL
            AND (? = 1 OR (sl.enabled = 1 AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)))
           JOIN content_types t ON t.key = e.content_type_key
          WHERE e.id = ?
            AND e.content_type_key = ?
            AND e.locale = ?
            AND e.trashed_at IS NULL
            AND (? = 1 OR e.publication_status = 'published')
            AND t.provider_status = 'active'
            AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1",
    )
    .bind(i64::from(is_preview))
    .bind(entry_id)
    .bind(type_key)
    .bind(locale)
    .bind(i64::from(is_preview))
    .fetch_optional(&mut **tx)
    .await?;

    let Some((sort_ts, primary_cat_id)) = anchor else {
        return Ok(AdjacentEntriesView::default());
    };

    let (tax_filter, tax_bind_id) = if !same_taxonomy {
        (AdjacentTaxFilter::None, 0)
    } else if let Some(pid) = primary_cat_id {
        let cat_eligible: i64 = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1
                  FROM taxonomy_terms tt
                  JOIN taxonomy_definitions d ON d.key = tt.kind
                  LEFT JOIN content_type_taxonomy_overrides ov ON ov.content_type_key = ?
                 WHERE tt.id = ?
                   AND tt.kind = 'category'
                   AND tt.locale = ?
                   AND d.public = 1
                   AND d.provider_status = 'active'
                   AND d.declaration_state = 'declared'
                   AND (
                       (ov.content_type_key IS NOT NULL AND EXISTS (
                           SELECT 1 FROM json_each(ov.taxonomies_json) sel WHERE sel.value = 'category'
                       ))
                       OR (ov.content_type_key IS NULL AND EXISTS (
                           SELECT 1 FROM content_type_taxonomy_eligibility el
                            WHERE el.content_type_key = ? AND el.taxonomy_kind = 'category'
                              AND el.provider_status = 'active' AND el.declaration_state = 'declared'
                       ))
                   )
            )",
        )
        .bind(type_key)
        .bind(pid)
        .bind(locale)
        .bind(type_key)
        .fetch_one(&mut **tx)
        .await?;
        if cat_eligible == 1 {
            (AdjacentTaxFilter::PrimaryCategory, pid)
        } else {
            let has_shared_term: i64 = sqlx::query_scalar(
                "SELECT EXISTS(
                    SELECT 1 FROM entry_terms et
                      JOIN taxonomy_terms tt ON tt.id = et.term_id
                      JOIN taxonomy_definitions d ON d.key = tt.kind
                     WHERE et.entry_id = ? AND tt.locale = ?
                       AND d.public = 1 AND d.provider_status = 'active' AND d.declaration_state = 'declared'
                )",
            )
            .bind(entry_id)
            .bind(locale)
            .fetch_one(&mut **tx)
            .await?;
            if has_shared_term == 0 {
                return Ok(AdjacentEntriesView::default());
            }
            (AdjacentTaxFilter::SharedEligibleTerm, entry_id)
        }
    } else {
        let has_shared_term: i64 = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM entry_terms et
                  JOIN taxonomy_terms tt ON tt.id = et.term_id
                  JOIN taxonomy_definitions d ON d.key = tt.kind
                 WHERE et.entry_id = ? AND tt.locale = ?
                   AND d.public = 1 AND d.provider_status = 'active' AND d.declaration_state = 'declared'
            )",
        )
        .bind(entry_id)
        .bind(locale)
        .fetch_one(&mut **tx)
        .await?;
        if has_shared_term == 0 {
            return Ok(AdjacentEntriesView::default());
        }
        (AdjacentTaxFilter::SharedEligibleTerm, entry_id)
    };

    let prev_row = query_one_adjacent_side(
        tx,
        type_key,
        locale,
        &sort_ts,
        entry_id,
        is_preview,
        false,
        tax_filter,
        tax_bind_id,
    )
    .await?;
    let next_row = query_one_adjacent_side(
        tx,
        type_key,
        locale,
        &sort_ts,
        entry_id,
        is_preview,
        true,
        tax_filter,
        tax_bind_id,
    )
    .await?;

    let mut ids = Vec::with_capacity(2);
    if let Some(ref r) = prev_row {
        ids.push(r.0);
    }
    if let Some(ref r) = next_row {
        ids.push(r.0);
    }
    if ids.is_empty() {
        return Ok(AdjacentEntriesView::default());
    }

    let urls = if is_preview {
        BTreeMap::new()
    } else {
        crate::storage::seo::resolve_public_entry_paths_by_entry_ids_in_conn(&mut **tx, &ids).await?
    };
    let eff_schema = crate::storage::content::load_effective_content_schema_in_tx(&mut **tx, type_key)
        .await?;
    let cover_key = eff_schema.as_ref().and_then(select_cover_media_spec_keys);

    let extract_media_id = |values_json: &str| -> Option<String> {
        let ck = cover_key?;
        let map: BTreeMap<String, Value> = serde_json::from_str(values_json).ok()?;
        let raw_id = map.get(ck)?.as_str()?;
        valid_sha256_media_id(raw_id).then(|| raw_id.to_owned())
    };
    let mut media_ids = BTreeSet::new();
    let prev_mid = prev_row.as_ref().and_then(|r| extract_media_id(&r.3));
    let next_mid = next_row.as_ref().and_then(|r| extract_media_id(&r.3));
    if let Some(ref m) = prev_mid {
        media_ids.insert(m.clone());
    }
    if let Some(ref m) = next_mid {
        media_ids.insert(m.clone());
    }
    let media_map = if media_ids.is_empty() {
        BTreeMap::new()
    } else {
        crate::storage::fields::load_media_hydration_map_in_conn(&mut **tx, &media_ids).await?
    };

    let build_item = |row: AdjacentSqlRow, mid_opt: Option<String>| -> AdjacentEntryItem {
        let id = row.0;
        let url = if is_preview {
            Some(format!("/api/admin/content-types/{type_key}/entries/{id}/preview"))
        } else {
            urls.get(&id).cloned()
        };
        let cover = mid_opt
            .as_deref()
            .and_then(|mid| media_map.get(mid))
            .map(|m| HydratedMedia::from_row(m, is_preview));
        let cover_url = cover.as_ref().map(|c| c.url.clone());
        AdjacentEntryItem {
            id,
            title: row.2,
            slug: row.1,
            url,
            cover_url,
            cover,
            published_at: row.4,
        }
    };

    Ok(AdjacentEntriesView {
        prev: prev_row.map(|r| build_item(r, prev_mid)),
        next: next_row.map(|r| build_item(r, next_mid)),
    })
}
