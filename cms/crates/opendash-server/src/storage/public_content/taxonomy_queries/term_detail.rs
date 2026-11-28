use std::collections::{BTreeMap, BTreeSet};

use opendash_core::{
    content::{
        FieldRole, LocationEvalContext, evaluate_location_rules, is_restricted_to_admin_only,
    },
    field_schema::{
        FieldSpec, FieldType, HydrationVisibility, MediaFieldKind, default_value_for_field_spec,
    },
};
use serde_json::Value;
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::*;
use crate::content_render::{PublicTermDetailView, rewrite_preview_term_media_urls};
use crate::error::ApiError;

pub(in crate::storage) async fn load_public_term_detail_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    term_id: i64,
    is_preview: bool,
    mut richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> ApiResult<Option<PublicTermDetailView>> {
    let row: Option<(String, String, String, String, i32, Option<String>, String, Option<String>)> =
        sqlx::query_as(
            "SELECT t.kind, t.name, t.slug, t.description, t.sort_order,
                    t.featured_image_media_id, t.values_json, c.path
               FROM taxonomy_terms t
               JOIN site_locales sl ON sl.code = t.locale AND sl.deleted_at IS NULL
                AND (? = 1 OR (sl.enabled = 1 AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)))
               JOIN taxonomy_definitions d ON d.key = t.kind
               LEFT JOIN permalink_route_claims c
                 ON c.target_kind = 'term' AND c.target_id = CAST(t.id AS TEXT) AND c.locale = t.locale
              WHERE t.id = ? AND d.public = 1 AND d.provider_status = 'active'
                AND d.declaration_state = 'declared'",
        )
        .bind(i64::from(is_preview))
        .bind(term_id)
        .fetch_optional(&mut **tx)
        .await?;
    let Some((
        kind,
        name,
        slug,
        description,
        sort_order,
        featured_image_media_id,
        raw_values_json,
        url,
    )) = row
    else {
        return Ok(None);
    };
    let locale_row: String = sqlx::query_scalar("SELECT locale FROM taxonomy_terms WHERE id = ?")
        .bind(term_id)
        .fetch_one(&mut **tx)
        .await?;

    let stored_values: BTreeMap<String, Value> =
        serde_json::from_str(&raw_values_json).unwrap_or_default();

    let eff = crate::storage::taxonomy::load_effective_term_schema_in_tx(&mut **tx, &kind).await?;
    let mut active_specs: Vec<FieldSpec> = Vec::new();
    if let Some(ref eff) = eff {
        let eval_ctx = LocationEvalContext {
            taxonomy_term: Some(kind.clone()),
            user_role: FieldRole::Editor,
            ..Default::default()
        };
        let active_groups: BTreeSet<&str> = eff
            .attached_groups
            .iter()
            .filter(|grp| {
                grp.is_runtime_active()
                    && !is_restricted_to_admin_only(&grp.location)
                    && evaluate_location_rules(&grp.location, &eval_ctx)
            })
            .map(|grp| grp.group_id.as_str())
            .collect();
        let mut seen = BTreeSet::new();
        for grp in &eff.attached_groups {
            if !active_groups.contains(grp.group_id.as_str()) {
                continue;
            }
            for spec in &grp.fields {
                if seen.insert(spec.key.as_str())
                    && !spec.is_ui_only()
                    && eff.effective_fields.get(&spec.key).is_some_and(|fe| {
                        fe.effective_status == opendash_core::content::EffectiveFieldStatus::Active
                    })
                    && spec.effective_acl().2 == opendash_core::content::PublicExposure::Public
                    && spec.effective_acl().0 <= FieldRole::Editor
                {
                    active_specs.push(spec.clone());
                }
            }
        }
    }

    let mut raw_values = BTreeMap::new();
    for spec in &active_specs {
        let val = stored_values
            .get(&spec.key)
            .cloned()
            .unwrap_or_else(|| default_value_for_field_spec(spec));
        raw_values.insert(spec.key.clone(), val);
    }

    let mut fi_spec = FieldSpec::new(FieldType::Media, "featured_image", "Ảnh đại diện");
    fi_spec.media_kind = Some(MediaFieldKind::Image);
    fi_spec.acl = Some(opendash_core::field_schema::FieldAcl::canonical(
        FieldRole::Editor,
        FieldRole::Editor,
        opendash_core::content::PublicExposure::Public,
    ));

    let vis = HydrationVisibility::ContentPublic {
        max_view_role: FieldRole::Editor,
    };
    let mut media_ids = BTreeSet::new();
    let mut entry_ids = BTreeSet::new();
    let mut term_ids = BTreeSet::new();
    if let Some(ref mid) = featured_image_media_id {
        media_ids.insert(mid.clone());
    }
    crate::field_hydrate::collect_hydration_ids(
        &active_specs,
        &raw_values,
        vis,
        &mut media_ids,
        &mut entry_ids,
        &mut term_ids,
    );

    let entries = if entry_ids.is_empty() {
        BTreeMap::new()
    } else {
        crate::storage::fields::load_entry_ref_hydration_map_in_conn(
            &mut **tx,
            &entry_ids,
            is_preview,
            Some(&locale_row),
        )
        .await?
    };
    let terms = if term_ids.is_empty() {
        BTreeMap::new()
    } else {
        let map = crate::storage::fields::load_term_ref_hydration_map_in_conn(
            &mut **tx,
            &term_ids,
            is_preview,
            Some(&locale_row),
        )
        .await?;
        for t in map.values() {
            if let Some(ref mid) = t.featured_image_media_id {
                media_ids.insert(mid.clone());
            }
        }
        map
    };
    let media = if media_ids.is_empty() {
        BTreeMap::new()
    } else {
        crate::storage::fields::load_media_hydration_map_in_conn(&mut **tx, &media_ids).await?
    };
    let ctx = crate::field_hydrate::HydrationContext {
        media: &media,
        entries: &entries,
        terms: &terms,
        is_preview,
        tag_media_provenance: true,
    };

    let featured_image = featured_image_media_id
        .as_deref()
        .map(|mid| Value::String(mid.to_owned()))
        .and_then(|val| {
            crate::field_hydrate::hydrate_field_value(&fi_spec, Some(&val), &ctx, None, vis)
                .ok()
                .filter(|v| !v.is_null())
        });
    let fields = crate::field_hydrate::hydrate_field_values(
        &active_specs,
        &raw_values,
        &ctx,
        richtext_nonces.as_deref_mut(),
        vis,
    )
    .map_err(ApiError::internal)?;

    let view = PublicTermDetailView {
        id: term_id,
        kind,
        name,
        slug,
        description,
        sort_order,
        url,
        featured_image,
        fields,
    };
    Ok(Some(if is_preview {
        rewrite_preview_term_media_urls(view)
    } else {
        view
    }))
}

pub async fn taxonomy_public_page_and_detail_by_id(
    db: &SqlitePool,
    term_id: i64,
    filters: PublicFilters,
    limit: i64,
    offset: i64,
) -> ApiResult<
    Option<(
        String,
        PublicTerm,
        TaxonomyPublishedPage,
        PublicTermDetailView,
        crate::template_sec::RichTextNonceTable,
    )>,
> {
    let mut tx = db.begin().await?;
    let row: Option<(String, String, String, String, i64)> = sqlx::query_as(
        "SELECT t.kind, t.name, t.slug, t.locale, d.hierarchical
           FROM taxonomy_terms t
           JOIN site_locales sl ON sl.code = t.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL
            AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
           JOIN taxonomy_definitions d ON d.key = t.kind
          WHERE t.id = ? AND d.public = 1 AND d.provider_status = 'active'
            AND d.declaration_state = 'declared'",
    )
    .bind(term_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((kind, name, slug, term_locale, hierarchical)) = row else {
        return Ok(None);
    };
    validate_taxonomy_page_filters_in_tx(&mut tx, &kind, hierarchical, &filters).await?;
    let page = list_taxonomy_published_in_tx(
        &mut tx,
        &kind,
        term_id,
        &term_locale,
        filters,
        limit,
        offset,
    )
    .await?;
    let mut richtext_nonces = crate::template_sec::RichTextNonceTable::default();
    let Some(detail) =
        load_public_term_detail_in_tx(&mut tx, term_id, false, Some(&mut richtext_nonces)).await?
    else {
        return Ok(None);
    };
    tx.commit().await?;
    Ok(Some((
        kind,
        PublicTerm {
            id: term_id,
            name,
            slug,
        },
        page,
        detail,
        richtext_nonces,
    )))
}
