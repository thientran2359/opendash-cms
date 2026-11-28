use std::collections::{BTreeMap, BTreeSet};

use opendash_core::{
    content::{
        FieldRole, LocationEvalContext, evaluate_location_rules, is_restricted_to_admin_only,
    },
    field_schema::{
        FieldContext, FieldSpec, FieldType, HydrationVisibility, collect_media_ids,
        default_value_for_field_spec, validate_field_values,
    },
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
    resolve_requested_locale_in_conn, validate_url_or_link_spec_strict,
};

fn map_validation_err_to_unprocessable(err: ApiError) -> ApiError {
    if err.0 == axum::http::StatusCode::BAD_REQUEST {
        ApiError(axum::http::StatusCode::UNPROCESSABLE_ENTITY, err.1)
    } else {
        err
    }
}

fn reject_disallowed_nested_transient_keys(
    spec: &FieldSpec,
    value: &Value,
    vis: HydrationVisibility,
) -> Result<(), &'static str> {
    let child_vis = vis.child_visibility();
    match spec.field_type {
        FieldType::Group => {
            if let Some(obj) = value.as_object() {
                for (k, child_val) in obj {
                    let Some(sub) = spec.fields.iter().find(|f| f.key == *k) else {
                        return Err("Trường con không tồn tại trong schema.");
                    };
                    if !child_vis.allows(sub) {
                        return Err("Trường con không được phép xem trước công khai.");
                    }
                    reject_disallowed_nested_transient_keys(sub, child_val, child_vis)?;
                }
            }
        }
        FieldType::Repeater => {
            if let Some(items) = value.as_array() {
                for item in items {
                    if let Some(obj) = item.as_object() {
                        for (k, child_val) in obj {
                            let Some(sub) = spec.fields.iter().find(|f| f.key == *k) else {
                                return Err("Trường con của repeater không tồn tại trong schema.");
                            };
                            if !child_vis.allows(sub) {
                                return Err(
                                    "Trường con của repeater không được phép xem trước công khai.",
                                );
                            }
                            reject_disallowed_nested_transient_keys(sub, child_val, child_vis)?;
                        }
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn validate_and_apply_options_page_transient_in_conn(
    conn: &mut SqliteConnection,
    all_specs: &[FieldSpec],
    reserved_builtin_keys: Option<&BTreeSet<String>>,
    raw: &mut BTreeMap<String, Value>,
    ov: &BTreeMap<String, Value>,
    target_locale: &str,
    default_locale: &str,
) -> ApiResult<()> {
    if ov.is_empty() {
        return Ok(());
    }
    let is_secondary = target_locale != default_locale;
    let public_spec_map: BTreeMap<&str, &FieldSpec> = all_specs
        .iter()
        .filter(|s| HydrationVisibility::OptionsPublicOnly.allows(s))
        .map(|s| (s.key.as_str(), s))
        .collect();
    for (k, v) in ov {
        if reserved_builtin_keys.is_some_and(|r| r.contains(k)) {
            return Err(ApiError::unprocessable(
                "Option key trùng khóa sẵn có không thể ghi đè xem trước qua options_pages.",
            ));
        }
        let Some(&spec) = public_spec_map.get(k.as_str()) else {
            return Err(ApiError::unprocessable(
                "Option key không tồn tại hoặc không được phép xem trước công khai.",
            ));
        };
        if is_secondary && !spec.translatable {
            return Err(ApiError::unprocessable(
                "Không thể xem trước thay đổi cho trường không đa ngôn ngữ trên ngôn ngữ phụ.",
            ));
        }
        reject_disallowed_nested_transient_keys(spec, v, HydrationVisibility::OptionsPublicOnly)
            .map_err(ApiError::unprocessable)?;
        validate_url_or_link_spec_strict(spec, v).map_err(ApiError::unprocessable)?;
    }

    let mut merged = raw.clone();
    for (k, v) in ov {
        merged.insert(k.clone(), v.clone());
    }
    validate_field_values(all_specs, FieldContext::Options, &merged, Some(raw))
        .map_err(ApiError::unprocessable)?;

    let ov_specs: Vec<FieldSpec> = ov
        .keys()
        .filter_map(|k| public_spec_map.get(k.as_str()).map(|s| (*s).clone()))
        .collect();
    let ov_media = collect_media_ids(&ov_specs, ov);
    crate::storage::fields::verify_collected_media_ids_in_conn(conn, &ov_media)
        .await
        .map_err(map_validation_err_to_unprocessable)?;
    let req_loc = is_secondary.then_some(target_locale);
    crate::storage::fields::validate_relational_refs_in_conn(
        conn,
        all_specs,
        &merged,
        Some(raw),
        req_loc,
    )
    .await
    .map_err(map_validation_err_to_unprocessable)?;

    for (k, v) in ov {
        raw.insert(k.clone(), v.clone());
    }
    Ok(())
}

pub(in crate::storage) async fn load_public_options_page_values_in_conn(
    conn: &mut SqliteConnection,
    page_ref: &str,
    requested_locale: Option<&str>,
    is_preview: bool,
    richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> ApiResult<BTreeMap<String, Value>> {
    load_public_options_page_values_with_override_in_conn(
        conn,
        page_ref,
        requested_locale,
        is_preview,
        None,
        None,
        richtext_nonces,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(in crate::storage) async fn load_public_options_page_values_with_override_in_conn(
    conn: &mut SqliteConnection,
    page_ref: &str,
    requested_locale: Option<&str>,
    is_preview: bool,
    transient_override: Option<&BTreeMap<String, Value>>,
    reserved_builtin_keys: Option<&BTreeSet<String>>,
    richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> ApiResult<BTreeMap<String, Value>> {
    let has_non_empty_override = transient_override.is_some_and(|m| !m.is_empty());
    if page_ref != "site_global" && !page_ref.starts_with("theme:") {
        let Some(page_key) = page_ref.strip_prefix("custom:") else {
            if has_non_empty_override {
                return Err(ApiError::unprocessable(
                    "page_ref options_pages không hợp lệ.",
                ));
            }
            return Ok(BTreeMap::new());
        };
        let Some(page) = get_options_page_in_conn(conn, page_key).await? else {
            if has_non_empty_override {
                return Err(ApiError::unprocessable("Trang tùy chọn không tồn tại."));
            }
            return Ok(BTreeMap::new());
        };
        if page.provider_status != opendash_core::registry::ProviderStatus::Active
            || page.declaration_state != DeclarationState::Declared
        {
            if has_non_empty_override {
                return Err(ApiError::unprocessable(
                    "Trang tùy chọn không đang hoạt động.",
                ));
            }
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
        if has_non_empty_override {
            return Err(ApiError::unprocessable(
                "Không có field group công khai nào gắn với options_page để xem trước.",
            ));
        }
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
    if let Some(ov) = transient_override {
        validate_and_apply_options_page_transient_in_conn(
            conn,
            &specs,
            reserved_builtin_keys,
            &mut raw,
            ov,
            &target_locale,
            &default_locale,
        )
        .await?;
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
    richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> ApiResult<BTreeMap<String, Value>> {
    load_active_custom_options_public_with_overrides_in_conn(
        conn,
        requested_locale,
        is_preview,
        None,
        richtext_nonces,
    )
    .await
}

pub(in crate::storage) async fn load_active_custom_options_public_with_overrides_in_conn(
    conn: &mut SqliteConnection,
    requested_locale: Option<&str>,
    is_preview: bool,
    transient_options_pages: Option<&BTreeMap<String, BTreeMap<String, Value>>>,
    mut richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> ApiResult<BTreeMap<String, Value>> {
    let custom_transient_keys: Vec<&String> = transient_options_pages
        .into_iter()
        .flat_map(|m| m.keys())
        .filter(|k| k.starts_with("custom:"))
        .collect();
    if !table_exists_in_tx(conn, "custom_options_pages").await? {
        if !custom_transient_keys.is_empty() {
            return Err(ApiError::unprocessable(
                "Bảng custom_options_pages chưa sẵn sàng để xem trước.",
            ));
        }
        return Ok(BTreeMap::new());
    }
    let page_keys: Vec<String> = sqlx::query_scalar(
        "SELECT page_key FROM custom_options_pages
          WHERE provider_status = 'active' AND declaration_state = 'declared'
          ORDER BY COALESCE(position, 50) ASC, page_key ASC",
    )
    .fetch_all(&mut *conn)
    .await?;
    for page_ref in &custom_transient_keys {
        let Some(k) = page_ref.strip_prefix("custom:") else {
            return Err(ApiError::unprocessable(
                "page_ref custom options không hợp lệ.",
            ));
        };
        if !page_keys.iter().any(|active_k| active_k == k) {
            return Err(ApiError::unprocessable(
                "Trang tùy chọn không tồn tại hoặc không đang hoạt động.",
            ));
        }
    }
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
                specs.extend(grp.fields);
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
        if let Some(ov) = transient_options_pages.and_then(|m| m.get(&page_ref)) {
            validate_and_apply_options_page_transient_in_conn(
                conn,
                specs,
                None,
                &mut raw,
                ov,
                &target_locale,
                &default_locale,
            )
            .await?;
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
