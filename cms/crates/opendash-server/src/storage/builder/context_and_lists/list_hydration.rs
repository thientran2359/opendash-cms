use std::collections::{BTreeMap, BTreeSet};

use opendash_core::content::{
    EffectiveContentSchema, EffectiveFieldStatus, FieldRole, PublicExposure,
};
use opendash_core::field_schema::{FieldType, MediaFieldKind};
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqliteConnection, Transaction};

use crate::error::ApiResult;
use crate::storage::entries::{ContentEntryRecord, EntryFilters};

/// Public taxonomy term metadata attached to a hydrated `ListItem`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListItemTerm {
    pub kind: String,
    pub name: String,
    pub slug: String,
    pub url: Option<String>,
}

/// Opt-in flags controlling which Zero-N+1 hydration queries run for list items.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ListHydrationOptions {
    pub show_image: bool,
    pub show_date: bool,
    pub show_excerpt: bool,
    pub show_taxonomy: bool,
}

/// Batch-resolves canonical public URLs for published entries in a single query.
pub async fn batch_canonical_entry_paths_in_conn(
    conn: &mut SqliteConnection,
    entry_ids: &[i64],
) -> ApiResult<BTreeMap<i64, String>> {
    crate::storage::seo::resolve_public_entry_paths_by_entry_ids_in_conn(conn, entry_ids).await
}

/// Resolves a term ID to its active translation in `active_locale` via `translation_group_id`.
/// Fails closed (`Ok(None)`) when the source term, taxonomy definition, locale, or target
/// translation is missing, private, inactive, or undeclared (without requiring the source
/// term's own locale to remain enabled on shared template overrides).
pub async fn resolve_term_id_for_locale_in_conn(
    conn: &mut SqliteConnection,
    term_id: i64,
    expected_kind: &str,
    active_locale: &str,
) -> ApiResult<Option<i64>> {
    if term_id <= 0 {
        return Ok(None);
    }
    let group_id: Option<String> = sqlx::query_scalar(
        "SELECT term.translation_group_id
           FROM taxonomy_terms term
           JOIN taxonomy_definitions d ON d.key = term.kind
          WHERE term.id = ?
            AND term.kind = ?
            AND d.public = 1
            AND d.provider_status = 'active'
            AND d.declaration_state = 'declared'",
    )
    .bind(term_id)
    .bind(expected_kind)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(group_id) = group_id else {
        return Ok(None);
    };
    let target_id: Option<i64> = sqlx::query_scalar(
        "SELECT target.id
           FROM taxonomy_terms target
           JOIN taxonomy_definitions d ON d.key = target.kind
           JOIN site_locales sl ON sl.code = target.locale AND sl.deleted_at IS NULL
          WHERE target.kind = ?
            AND target.translation_group_id = ?
            AND target.locale = ?
            AND d.public = 1
            AND d.provider_status = 'active'
            AND d.declaration_state = 'declared'
            AND sl.enabled = 1
            AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)",
    )
    .bind(expected_kind)
    .bind(&group_id)
    .bind(active_locale)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(target_id)
}

pub(super) enum LocalizedListFilterOutcome {
    Ready(EntryFilters),
    MissingConfig,
    EmptyDueToMissingTranslation,
}

/// Translates saved taxonomy IDs in a list filter to `active_locale` while preserving
/// `MissingConfig` when the raw type/taxonomy/term is invalid or ineligible.
pub(super) async fn resolve_list_filters_for_locale_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    mut filters: EntryFilters,
    active_locale: &str,
) -> ApiResult<LocalizedListFilterOutcome> {
    let raw_filters = filters.clone();
    let mut missing_translation = false;

    if let Some(cat_id) = filters.category_id {
        match resolve_term_id_for_locale_in_conn(&mut **tx, cat_id, "category", active_locale)
            .await?
        {
            Some(resolved) => filters.category_id = Some(resolved),
            None => missing_translation = true,
        }
    }
    if let Some(tag_id) = filters.tag_id {
        match resolve_term_id_for_locale_in_conn(&mut **tx, tag_id, "tag", active_locale).await? {
            Some(resolved) => filters.tag_id = Some(resolved),
            None => missing_translation = true,
        }
    }
    if let (Some(kind), Some(term_id)) = (filters.taxonomy_key.as_deref(), filters.term_id) {
        match resolve_term_id_for_locale_in_conn(&mut **tx, term_id, kind, active_locale).await? {
            Some(resolved) => filters.term_id = Some(resolved),
            None => missing_translation = true,
        }
    }

    if missing_translation {
        if crate::storage::entries::preflight_published_list_filters_in_tx(
            tx,
            type_key,
            &raw_filters,
        )
        .await?
        .is_err()
        {
            return Ok(LocalizedListFilterOutcome::MissingConfig);
        }
        return Ok(LocalizedListFilterOutcome::EmptyDueToMissingTranslation);
    }

    Ok(LocalizedListFilterOutcome::Ready(filters))
}

fn select_cover_field_key(schema: &EffectiveContentSchema) -> Option<String> {
    schema
        .effective_fields
        .iter()
        .filter(|(_, fe)| {
            fe.effective_status == EffectiveFieldStatus::Active
                && fe.definition.effective_acl().2 == PublicExposure::Public
                && fe.definition.effective_acl().0 == FieldRole::Editor
                && fe.definition.field_type == FieldType::Media
                && fe.definition.effective_media_kind() == MediaFieldKind::Image
        })
        .fold(None::<String>, |best, (key, _)| {
            if key == "featured_image" {
                Some(key.clone())
            } else {
                best.or_else(|| Some(key.clone()))
            }
        })
}

fn extract_public_list_excerpt(
    entry: &ContentEntryRecord,
    schema: Option<&EffectiveContentSchema>,
) -> Option<String> {
    let is_public_editor_field = |key: &str| -> bool {
        let Some(schema) = schema else {
            return false;
        };
        let Some(fe) = schema.effective_fields.get(key) else {
            return false;
        };
        fe.effective_status == EffectiveFieldStatus::Active
            && fe.definition.effective_acl().2 == PublicExposure::Public
            && fe.definition.effective_acl().0 == FieldRole::Editor
    };
    let raw = if is_public_editor_field("summary") {
        entry
            .values
            .get("summary")
            .and_then(serde_json::Value::as_str)
    } else {
        None
    }
    .or_else(|| {
        if is_public_editor_field("body") {
            entry.values.get("body").and_then(serde_json::Value::as_str)
        } else {
            None
        }
    })?;
    let plain = if opendash_core::builder_blocks::contains_blocks(raw) {
        opendash_core::builder_blocks::extract_visible_text(raw)
    } else {
        crate::content_render::strip_richtext_to_plain(raw)
    };
    let trimmed = crate::storage::fields::truncate_entry_excerpt(&plain);
    (!trimmed.is_empty()).then_some(trimmed)
}

/// Zero-N+1 hydration for list items: always resolves public canonical URLs in a
/// single batch and only loads schemas/media/excerpts/terms requested by `options`.
pub async fn hydrate_list_items_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    _type_key: &str,
    entries: &[ContentEntryRecord],
    options: ListHydrationOptions,
) -> ApiResult<Vec<super::ListItem>> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    let entry_ids: Vec<i64> = entries.iter().map(|entry| entry.id).collect();
    let mut urls = batch_canonical_entry_paths_in_conn(&mut **tx, &entry_ids).await?;

    let mut schemas_by_type: BTreeMap<String, Option<EffectiveContentSchema>> = BTreeMap::new();
    let mut cover_field_by_type: BTreeMap<String, Option<String>> = BTreeMap::new();
    if options.show_image || options.show_excerpt {
        let distinct_types: BTreeSet<&str> = entries
            .iter()
            .map(|entry| entry.content_type_key.as_str())
            .collect();
        for tk in distinct_types {
            let schema =
                crate::storage::content::load_effective_content_schema_in_tx(&mut **tx, tk).await?;
            let cover_key = schema.as_ref().and_then(select_cover_field_key);
            cover_field_by_type.insert(tk.to_owned(), cover_key);
            schemas_by_type.insert(tk.to_owned(), schema);
        }
    }

    let media_map = if options.show_image {
        let mut media_ids = BTreeSet::new();
        for entry in entries {
            if let Some(Some(cover_key)) = cover_field_by_type.get(&entry.content_type_key)
                && let Some(mid) = entry
                    .values
                    .get(cover_key)
                    .and_then(serde_json::Value::as_str)
                    .filter(|id| crate::storage::media::valid_media_id(id))
            {
                media_ids.insert(mid.to_owned());
            }
        }
        if media_ids.is_empty() {
            BTreeMap::new()
        } else {
            crate::storage::fields::load_media_hydration_map_in_conn(&mut **tx, &media_ids).await?
        }
    } else {
        BTreeMap::new()
    };

    let mut terms_map = if options.show_taxonomy {
        let entry_pairs: Vec<(i64, String)> = entries
            .iter()
            .map(|entry| (entry.id, entry.content_type_key.clone()))
            .collect();
        crate::storage::public_content::batch_load_public_entry_terms_in_conn(
            &mut **tx,
            &entry_pairs,
        )
        .await?
    } else {
        BTreeMap::new()
    };

    let mut items = Vec::with_capacity(entries.len());
    for entry in entries {
        let cover_url = if options.show_image {
            cover_field_by_type
                .get(&entry.content_type_key)
                .and_then(|opt| opt.as_deref())
                .and_then(|key| entry.values.get(key))
                .and_then(serde_json::Value::as_str)
                .and_then(|mid| media_map.get(mid))
                .filter(|media| media.kind == "image")
                .map(|media| format!("/media/{}/file", media.id))
        } else {
            None
        };
        let excerpt = if options.show_excerpt {
            let schema = schemas_by_type
                .get(&entry.content_type_key)
                .and_then(|opt| opt.as_ref());
            extract_public_list_excerpt(entry, schema)
        } else {
            None
        };
        let terms = if options.show_taxonomy {
            terms_map
                .remove(&entry.id)
                .unwrap_or_default()
                .into_iter()
                .map(|term| ListItemTerm {
                    kind: term.kind,
                    name: term.name,
                    slug: term.slug,
                    url: term.url,
                })
                .collect()
        } else {
            Vec::new()
        };
        items.push(super::ListItem {
            entry_id: entry.id,
            title: entry.title.clone(),
            slug: entry.slug.clone(),
            href: urls.remove(&entry.id),
            cover_url,
            published_at: options
                .show_date
                .then(|| entry.published_at.clone())
                .flatten(),
            excerpt,
            terms,
        });
    }
    Ok(items)
}
