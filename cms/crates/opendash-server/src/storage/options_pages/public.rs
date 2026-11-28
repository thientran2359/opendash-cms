use std::collections::{BTreeMap, BTreeSet};

use opendash_core::{
    content::{
        FieldRole, LocationEvalContext, evaluate_location_rules, is_restricted_to_admin_only,
    },
    field_schema::{FieldSpec, HydrationVisibility, default_value_for_field_spec},
    registry::DeclarationState,
};
use serde_json::Value;
use sqlx::{SqliteConnection, SqlitePool};

use super::get_options_page_in_conn;
use super::values::load_attached_options_groups_in_conn;
use crate::error::{ApiError, ApiResult};
use crate::storage::content::table_exists_in_tx;
use crate::storage::settings::{
    get_default_locale_code_in_conn, load_owner_option_maps_in_conn,
    resolve_requested_locale_in_conn,
};

pub(in crate::storage) async fn load_public_options_page_values_in_conn(
    conn: &mut SqliteConnection,
    page_ref: &str,
    requested_locale: Option<&str>,
    is_preview: bool,
    richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> ApiResult<BTreeMap<String, Value>> {
    if page_ref != "site_global" && !page_ref.starts_with("theme:") {
        let Some(page_key) = page_ref.strip_prefix("custom:") else {
            return Ok(BTreeMap::new());
        };
        let Some(page) = get_options_page_in_conn(conn, page_key).await? else {
            return Ok(BTreeMap::new());
        };
        if page.provider_status != opendash_core::registry::ProviderStatus::Active
            || page.declaration_state != DeclarationState::Declared
        {
            return Ok(BTreeMap::new());
        }
    }
    let default_locale = get_default_locale_code_in_conn(conn).await?;
    let target_locale = resolve_requested_locale_in_conn(conn, requested_locale).await?;
    let groups = load_attached_options_groups_in_conn(conn, page_ref).await?;
    let eval_ctx = LocationEvalContext {
        options_page: Some(page_ref.to_owned()),
        user_role: FieldRole::Editor,
        ..Default::default()
    };
    let mut specs = Vec::new();
    for grp in groups {
        if grp.is_runtime_active()
            && !is_restricted_to_admin_only(&grp.location)
            && evaluate_location_rules(&grp.location, &eval_ctx)
        {
            specs.extend(grp.fields);
        }
    }
    if specs.is_empty() {
        return Ok(BTreeMap::new());
    }
    let (global_map, locale_map) = load_owner_option_maps_in_conn(
        conn,
        "options_page",
        page_ref,
        &default_locale,
        &target_locale,
    )
    .await?;
    let is_secondary = target_locale != default_locale;
    let mut raw = BTreeMap::new();
    for spec in &specs {
        if spec.is_ui_only() {
            continue;
        }
        let val = if is_secondary && spec.translatable {
            locale_map
                .get(&spec.key)
                .or_else(|| global_map.get(&spec.key))
                .cloned()
                .unwrap_or_else(|| default_value_for_field_spec(spec))
        } else {
            global_map
                .get(&spec.key)
                .cloned()
                .unwrap_or_else(|| default_value_for_field_spec(spec))
        };
        raw.insert(spec.key.clone(), val);
    }

    let mut media_ids = BTreeSet::new();
    let mut entry_ids = BTreeSet::new();
    let mut term_ids = BTreeSet::new();
    crate::field_hydrate::collect_hydration_ids(
        &specs,
        &raw,
        HydrationVisibility::OptionsPublicOnly,
        &mut media_ids,
        &mut entry_ids,
        &mut term_ids,
    );
    let entries = crate::storage::fields::load_entry_ref_hydration_map_in_conn(
        conn,
        &entry_ids,
        is_preview,
        Some(&target_locale),
    )
    .await?;
    let terms = crate::storage::fields::load_term_ref_hydration_map_in_conn(
        conn,
        &term_ids,
        is_preview,
        Some(&target_locale),
    )
    .await?;
    for term in terms.values() {
        if let Some(ref mid) = term.featured_image_media_id {
            media_ids.insert(mid.clone());
        }
    }
    let media = crate::storage::fields::load_media_hydration_map_in_conn(conn, &media_ids).await?;
    let ctx = crate::field_hydrate::HydrationContext {
        media: &media,
        entries: &entries,
        terms: &terms,
        is_preview,
        tag_media_provenance: true,
    };
    let mut hydrated = crate::field_hydrate::hydrate_field_values(
        &specs,
        &raw,
        &ctx,
        richtext_nonces,
        HydrationVisibility::OptionsPublicOnly,
    )
    .map_err(ApiError::internal)?;
    if is_preview {
        for v in hydrated.values_mut() {
            crate::content_render::rewrite_media_urls_in_value(v);
        }
    }
    Ok(hydrated)
}

pub(in crate::storage) async fn load_active_custom_options_public_in_conn(
    conn: &mut SqliteConnection,
    requested_locale: Option<&str>,
    is_preview: bool,
    mut richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> ApiResult<BTreeMap<String, Value>> {
    if !table_exists_in_tx(conn, "custom_options_pages").await? {
        return Ok(BTreeMap::new());
    }
    let page_keys: Vec<String> = sqlx::query_scalar(
        "SELECT page_key FROM custom_options_pages
          WHERE provider_status = 'active' AND declaration_state = 'declared'
          ORDER BY COALESCE(position, 50) ASC, page_key ASC",
    )
    .fetch_all(&mut *conn)
    .await?;
    if page_keys.is_empty() {
        return Ok(BTreeMap::new());
    }
    let default_locale = get_default_locale_code_in_conn(conn).await?;
    let target_locale = resolve_requested_locale_in_conn(conn, requested_locale).await?;
    let is_secondary = target_locale != default_locale;

    let mut page_specs: BTreeMap<String, Vec<FieldSpec>> = BTreeMap::new();
    let mut all_page_refs = Vec::with_capacity(page_keys.len());
    for key in &page_keys {
        let page_ref = format!("custom:{key}");
        let groups = load_attached_options_groups_in_conn(conn, &page_ref).await?;
        let eval_ctx = LocationEvalContext {
            options_page: Some(page_ref.clone()),
            user_role: FieldRole::Editor,
            ..Default::default()
        };
        let mut specs = Vec::new();
        for grp in groups {
            if grp.is_runtime_active()
                && !is_restricted_to_admin_only(&grp.location)
                && evaluate_location_rules(&grp.location, &eval_ctx)
            {
                for spec in grp.fields {
                    if HydrationVisibility::OptionsPublicOnly.allows(&spec) {
                        specs.push(spec);
                    }
                }
            }
        }
        all_page_refs.push(page_ref);
        page_specs.insert(key.clone(), specs);
    }

    let mut global_values_by_ref: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
    let mut locale_values_by_ref: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
    for chunk in all_page_refs.chunks(500) {
        let refs_json = serde_json::to_string(chunk)?;
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT owner_id, option_key, value_json
               FROM theme_option_values
              WHERE owner_kind = 'options_page' AND theme_id = ''
                AND owner_id IN (SELECT value FROM json_each(?))",
        )
        .bind(&refs_json)
        .fetch_all(&mut *conn)
        .await?;
        for (owner_id, opt_key, val_json) in rows {
            if let Ok(val) = serde_json::from_str::<Value>(&val_json) {
                global_values_by_ref
                    .entry(owner_id)
                    .or_default()
                    .insert(opt_key, val);
            }
        }
        if is_secondary {
            let lrows: Vec<(String, String, String)> = sqlx::query_as(
                "SELECT owner_id, option_key, value_json
                   FROM theme_option_locale_values
                  WHERE owner_kind = 'options_page' AND theme_id = '' AND locale = ?
                    AND owner_id IN (SELECT value FROM json_each(?))",
            )
            .bind(&target_locale)
            .bind(&refs_json)
            .fetch_all(&mut *conn)
            .await?;
            for (owner_id, opt_key, val_json) in lrows {
                if let Ok(val) = serde_json::from_str::<Value>(&val_json) {
                    locale_values_by_ref
                        .entry(owner_id)
                        .or_default()
                        .insert(opt_key, val);
                }
            }
        }
    }

    let mut raw_by_key: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
    let mut media_ids = BTreeSet::new();
    let mut entry_ids = BTreeSet::new();
    let mut term_ids = BTreeSet::new();
    for key in &page_keys {
        let page_ref = format!("custom:{key}");
        let specs = page_specs.get(key).map(Vec::as_slice).unwrap_or(&[]);
        let g_map = global_values_by_ref.get(&page_ref);
        let l_map = locale_values_by_ref.get(&page_ref);
        let mut raw = BTreeMap::new();
        for spec in specs {
            if spec.is_ui_only() {
                continue;
            }
            let val = if is_secondary && spec.translatable {
                l_map
                    .and_then(|m| m.get(&spec.key))
                    .or_else(|| g_map.and_then(|m| m.get(&spec.key)))
                    .cloned()
                    .unwrap_or_else(|| default_value_for_field_spec(spec))
            } else {
                g_map
                    .and_then(|m| m.get(&spec.key))
                    .cloned()
                    .unwrap_or_else(|| default_value_for_field_spec(spec))
            };
            raw.insert(spec.key.clone(), val);
        }
        crate::field_hydrate::collect_hydration_ids(
            specs,
            &raw,
            HydrationVisibility::OptionsPublicOnly,
            &mut media_ids,
            &mut entry_ids,
            &mut term_ids,
        );
        raw_by_key.insert(key.clone(), raw);
    }

    let entries = if entry_ids.is_empty() {
        BTreeMap::new()
    } else {
        crate::storage::fields::load_entry_ref_hydration_map_in_conn(
            conn,
            &entry_ids,
            is_preview,
            Some(&target_locale),
        )
        .await?
    };
    let terms = if term_ids.is_empty() {
        BTreeMap::new()
    } else {
        let map = crate::storage::fields::load_term_ref_hydration_map_in_conn(
            conn,
            &term_ids,
            is_preview,
            Some(&target_locale),
        )
        .await?;
        for term in map.values() {
            if let Some(ref mid) = term.featured_image_media_id {
                media_ids.insert(mid.clone());
            }
        }
        map
    };
    let media = if media_ids.is_empty() {
        BTreeMap::new()
    } else {
        crate::storage::fields::load_media_hydration_map_in_conn(conn, &media_ids).await?
    };
    let ctx = crate::field_hydrate::HydrationContext {
        media: &media,
        entries: &entries,
        terms: &terms,
        is_preview,
        tag_media_provenance: true,
    };

    let mut out = BTreeMap::new();
    for key in page_keys {
        let specs = page_specs.get(&key).map(Vec::as_slice).unwrap_or(&[]);
        let raw = raw_by_key.remove(&key).unwrap_or_default();
        let mut hydrated = crate::field_hydrate::hydrate_field_values(
            specs,
            &raw,
            &ctx,
            richtext_nonces.as_deref_mut(),
            HydrationVisibility::OptionsPublicOnly,
        )
        .map_err(ApiError::internal)?;
        if is_preview {
            for v in hydrated.values_mut() {
                crate::content_render::rewrite_media_urls_in_value(v);
            }
        }
        out.insert(key, Value::Object(hydrated.into_iter().collect()));
    }
    Ok(out)
}

pub async fn load_active_custom_options_public(
    db: &SqlitePool,
    requested_locale: Option<&str>,
    is_preview: bool,
    richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> ApiResult<BTreeMap<String, Value>> {
    let mut conn = db.acquire().await?;
    load_active_custom_options_public_in_conn(
        &mut conn,
        requested_locale,
        is_preview,
        richtext_nonces,
    )
    .await
}
