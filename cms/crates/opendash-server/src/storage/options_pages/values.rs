use std::collections::{BTreeMap, BTreeSet};

use opendash_core::{
    content::{
        AttachedFieldGroupSnapshot, FieldGroupDomain, FieldGroupPresentation, FieldGroupStatus,
        FieldRole, LocationEvalContext, evaluate_location_rules, valid_custom_options_page_key,
    },
    field_schema::{
        FieldContext, FieldSpec, FieldType, HydrationVisibility, collect_media_ids,
        default_value_for_field_spec, global_site_option_fields, validate_field_values,
    },
    registry::DeclarationState,
};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{SqliteConnection, SqlitePool};

use super::get_options_page_in_conn;
use crate::error::{ApiError, ApiResult};
use crate::storage::content::{
    begin_content_write_tx, parse_location_rules, parse_owner, parse_presentation,
};
use crate::storage::settings::{
    bump_owner_option_version_in_conn, get_default_locale_code_in_conn,
    load_owner_option_maps_in_conn, load_owner_option_version_in_conn,
    resolve_requested_locale_in_conn, sync_theme_option_media_refs_in_conn,
    upsert_owner_option_value_in_conn, validate_url_or_link_spec_strict,
};

#[derive(Debug, Clone, Serialize)]
pub struct OptionsPageFieldGroupView {
    pub group_id: String,
    pub title: String,
    pub description: String,
    pub presentation: FieldGroupPresentation,
    pub fields: Vec<FieldSpec>,
    pub read_only_keys: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OptionsPageValuesSnapshot {
    pub page_ref: String,
    pub locale: String,
    pub default_locale: String,
    pub version: i64,
    pub schema_token: String,
    pub groups: Vec<OptionsPageFieldGroupView>,
    pub values: BTreeMap<String, Value>,
    pub fallback_locales: BTreeMap<String, String>,
}

pub(in crate::storage) async fn verify_options_page_target_exists_in_conn(
    conn: &mut SqliteConnection,
    target_key: &str,
) -> ApiResult<()> {
    if target_key == "site_global" {
        return Ok(());
    }
    if let Some(theme_id) = target_key.strip_prefix("theme:") {
        if !opendash_core::id::valid_package_id(theme_id) {
            return Err(ApiError::bad_request("theme:<id> không hợp lệ."));
        }
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM theme_packages WHERE theme_id = ?)",
        )
        .bind(theme_id)
        .fetch_one(&mut *conn)
        .await?;
        if !exists {
            return Err(ApiError::not_found("Theme mục tiêu không tồn tại."));
        }
        return Ok(());
    }
    if let Some(page_key) = target_key.strip_prefix("custom:") {
        valid_custom_options_page_key(page_key).map_err(ApiError::bad_request)?;
        if get_options_page_in_conn(conn, page_key).await?.is_none() {
            return Err(ApiError::not_found(
                "Trang tùy chọn mục tiêu không tồn tại.",
            ));
        }
        return Ok(());
    }
    Err(ApiError::bad_request(
        "Mục tiêu options_page phải là 'site_global', 'theme:<id>' hoặc 'custom:<page_key>'.",
    ))
}

pub(in crate::storage) async fn load_attached_options_groups_in_conn(
    conn: &mut SqliteConnection,
    page_ref: &str,
) -> ApiResult<Vec<AttachedFieldGroupSnapshot>> {
    let rows: Vec<(
        String,
        String,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        i64,
        String,
    )> = sqlx::query_as(
        "SELECT g.group_id, g.owner_kind, g.owner_id, g.local_key, g.title, g.description,
                g.fields_json, g.location_rules_json, g.presentation_json, g.active,
                g.status, g.version, t.association_state
           FROM content_field_group_targets t
           JOIN content_field_groups g ON g.group_id = t.group_id
          WHERE t.target_kind = 'options_page' AND t.target_key = ?
            AND g.domain = 'options_page'
          ORDER BY g.group_id ASC",
    )
    .bind(page_ref)
    .fetch_all(&mut *conn)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for (
        group_id,
        owner_kind,
        owner_id,
        local_key,
        title,
        description,
        fields_json,
        loc_json,
        pres_json,
        active_int,
        status_str,
        version,
        assoc_str,
    ) in rows
    {
        let fields: Vec<FieldSpec> = serde_json::from_str(&fields_json)
            .map_err(|_| ApiError::internal("fields_json của field group không hợp lệ."))?;
        let location = parse_location_rules(&loc_json)?;
        let presentation = parse_presentation(&pres_json, "main_bottom", 50, true)?;
        let status = FieldGroupStatus::from_db(&status_str)
            .ok_or_else(|| ApiError::internal("status của field group không hợp lệ."))?;
        let association_state = match assoc_str.as_str() {
            "declared" => DeclarationState::Declared,
            _ => DeclarationState::Withdrawn,
        };
        out.push(AttachedFieldGroupSnapshot {
            group_id,
            owner: parse_owner(&owner_kind, owner_id)?,
            local_key,
            title,
            description,
            domain: FieldGroupDomain::OptionsPage,
            location,
            presentation,
            active: active_int != 0,
            fields,
            status,
            association_state,
            version,
        });
    }
    Ok(out)
}

pub(in crate::storage) async fn check_options_page_group_collisions_in_conn(
    conn: &mut SqliteConnection,
    page_ref: &str,
    groups: &[AttachedFieldGroupSnapshot],
) -> ApiResult<()> {
    let mut reserved: BTreeSet<String> = if page_ref == "site_global" {
        global_site_option_fields()
            .into_iter()
            .map(|f| f.key)
            .collect()
    } else {
        BTreeSet::new()
    };
    if let Some(theme_id) = page_ref.strip_prefix("theme:") {
        let raw_manifest: Option<String> = sqlx::query_scalar(
            "SELECT r.manifest_json
               FROM theme_revisions r
               JOIN theme_active_selection a ON a.revision_id = r.revision_id AND a.id = 1
              WHERE r.theme_id = ?",
        )
        .bind(theme_id)
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(manifest) = raw_manifest
            && let Ok(specs) = crate::storage::settings::parse_theme_options_schema(&manifest)
        {
            for s in specs {
                if !s.is_ui_only() {
                    reserved.insert(s.key);
                }
            }
        }
    }
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for g in groups {
        if !g.is_runtime_active() {
            continue;
        }
        for spec in &g.fields {
            if spec.is_ui_only() {
                continue;
            }
            if reserved.contains(&spec.key) {
                return Err(ApiError::conflict(format!(
                    "field_key_collision: Trường '{}' trùng khóa tùy chọn sẵn có trên '{}'.",
                    spec.key, page_ref
                )));
            }
            if !seen.insert(spec.key.clone()) {
                return Err(ApiError::conflict(format!(
                    "field_key_collision: Trường '{}' bị trùng giữa các nhóm trường trên '{}'.",
                    spec.key, page_ref
                )));
            }
        }
    }
    Ok(())
}

fn compute_options_schema_token(
    page_ref: &str,
    role: FieldRole,
    groups: &[&AttachedFieldGroupSnapshot],
) -> String {
    let payload = serde_json::json!({
        "page_ref": page_ref,
        "role": match role { FieldRole::Admin => "admin", FieldRole::Editor => "editor" },
        "groups": groups.iter().map(|g| (&g.group_id, g.version, &g.fields, &g.location)).collect::<Vec<_>>(),
    });
    let bytes = serde_json::to_vec(&payload).unwrap_or_default();
    let digest = Sha256::digest(&bytes);
    let mut hex = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{b:02x}");
    }
    format!("sha256:{hex}")
}

async fn ensure_custom_page_readable_in_conn(
    conn: &mut SqliteConnection,
    page_ref: &str,
    role: FieldRole,
    is_write: bool,
) -> ApiResult<()> {
    if page_ref == "site_global" || page_ref.starts_with("theme:") {
        if is_write && role != FieldRole::Admin {
            return Err(ApiError::forbidden(
                "Chỉ Quản trị viên mới được chỉnh sửa trang cấu hình hệ thống.",
            ));
        }
        verify_options_page_target_exists_in_conn(conn, page_ref).await?;
        return Ok(());
    }
    let page_key = page_ref
        .strip_prefix("custom:")
        .ok_or_else(|| ApiError::bad_request("page_ref không hợp lệ."))?;
    valid_custom_options_page_key(page_key).map_err(ApiError::bad_request)?;
    let page = get_options_page_in_conn(conn, page_key)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy trang tùy chọn."))?;
    if is_write
        && (page.provider_status != opendash_core::registry::ProviderStatus::Active
            || page.declaration_state != DeclarationState::Declared)
    {
        return Err(ApiError::conflict(
            "Trang tùy chọn từ gói mở rộng hiện không hoạt động.",
        ));
    }
    if role < page.min_edit_role {
        return Err(ApiError::forbidden(
            "Tài khoản không đủ quyền truy cập trang tùy chọn này.",
        ));
    }
    Ok(())
}

pub async fn get_options_page_values(
    db: &SqlitePool,
    page_ref: &str,
    requested_locale: Option<&str>,
    actor_role: FieldRole,
) -> ApiResult<OptionsPageValuesSnapshot> {
    let mut conn = db.acquire().await?;
    ensure_custom_page_readable_in_conn(&mut conn, page_ref, actor_role, false).await?;
    get_options_page_values_in_conn(&mut conn, page_ref, requested_locale, actor_role).await
}

pub(in crate::storage) async fn get_options_page_values_in_conn(
    conn: &mut SqliteConnection,
    page_ref: &str,
    requested_locale: Option<&str>,
    actor_role: FieldRole,
) -> ApiResult<OptionsPageValuesSnapshot> {
    let default_locale = get_default_locale_code_in_conn(conn).await?;
    let target_locale = resolve_requested_locale_in_conn(conn, requested_locale).await?;
    let groups_all = load_attached_options_groups_in_conn(conn, page_ref).await?;
    let eval_ctx = LocationEvalContext {
        options_page: Some(page_ref.to_owned()),
        user_role: actor_role,
        ..Default::default()
    };
    let matching: Vec<&AttachedFieldGroupSnapshot> = groups_all
        .iter()
        .filter(|g| g.is_runtime_active() && evaluate_location_rules(&g.location, &eval_ctx))
        .collect();
    let (global_map, locale_map) = load_owner_option_maps_in_conn(
        conn,
        "options_page",
        page_ref,
        &default_locale,
        &target_locale,
    )
    .await?;
    let version = load_owner_option_version_in_conn(
        conn,
        "options_page",
        page_ref,
        &default_locale,
        &target_locale,
    )
    .await?;

    let is_secondary = target_locale != default_locale;
    let mut group_views = Vec::new();
    let mut values = BTreeMap::new();
    let mut fallback_locales = BTreeMap::new();

    for grp in &matching {
        let mut visible_fields = Vec::new();
        let mut read_only = BTreeSet::new();
        for spec in &grp.fields {
            let (min_view, min_edit, _) = spec.effective_acl();
            if actor_role < min_view {
                continue;
            }
            if !spec.is_ui_only() && actor_role < min_edit {
                read_only.insert(spec.key.clone());
            }
            visible_fields.push(spec.clone());
            if spec.is_ui_only() {
                continue;
            }
            let val = if spec.field_type == FieldType::Secret {
                let present = global_map.contains_key(&spec.key);
                Value::Bool(present)
            } else if is_secondary && spec.translatable {
                if let Some(loc_val) = locale_map.get(&spec.key) {
                    loc_val.clone()
                } else {
                    fallback_locales.insert(spec.key.clone(), default_locale.clone());
                    global_map
                        .get(&spec.key)
                        .cloned()
                        .unwrap_or_else(|| default_value_for_field_spec(spec))
                }
            } else {
                global_map
                    .get(&spec.key)
                    .cloned()
                    .unwrap_or_else(|| default_value_for_field_spec(spec))
            };
            values.insert(spec.key.clone(), val);
        }
        if !visible_fields.is_empty() {
            group_views.push(OptionsPageFieldGroupView {
                group_id: grp.group_id.clone(),
                title: grp.title.clone(),
                description: grp.description.clone(),
                presentation: grp.presentation,
                fields: visible_fields,
                read_only_keys: read_only,
            });
        }
    }
    group_views.sort_by(|a, b| {
        a.presentation
            .order
            .cmp(&b.presentation.order)
            .then_with(|| a.group_id.cmp(&b.group_id))
    });

    Ok(OptionsPageValuesSnapshot {
        page_ref: page_ref.to_owned(),
        locale: target_locale,
        default_locale,
        version,
        schema_token: compute_options_schema_token(page_ref, actor_role, &matching),
        groups: group_views,
        values,
        fallback_locales,
    })
}

pub async fn put_options_page_values(
    db: &SqlitePool,
    page_ref: &str,
    requested_locale: Option<&str>,
    expected_version: i64,
    schema_token: Option<&str>,
    submitted: BTreeMap<String, Value>,
    actor_role: FieldRole,
) -> ApiResult<OptionsPageValuesSnapshot> {
    let mut tx = begin_content_write_tx(db).await?;
    ensure_custom_page_readable_in_conn(&mut tx, page_ref, actor_role, true).await?;

    let default_locale = get_default_locale_code_in_conn(&mut tx).await?;
    let target_locale = resolve_requested_locale_in_conn(&mut tx, requested_locale).await?;
    let is_secondary = target_locale != default_locale;
    if is_secondary {
        crate::storage::settings::require_multilingual_enabled_in_conn(&mut tx).await?;
    }

    let groups_all = load_attached_options_groups_in_conn(&mut tx, page_ref).await?;
    let eval_ctx = LocationEvalContext {
        options_page: Some(page_ref.to_owned()),
        user_role: actor_role,
        ..Default::default()
    };
    let matching: Vec<&AttachedFieldGroupSnapshot> = groups_all
        .iter()
        .filter(|g| g.is_runtime_active() && evaluate_location_rules(&g.location, &eval_ctx))
        .collect();
    check_options_page_group_collisions_in_conn(&mut tx, page_ref, &groups_all).await?;

    let current_token = compute_options_schema_token(page_ref, actor_role, &matching);
    if let Some(tok) = schema_token
        && tok != current_token
    {
        return Err(ApiError::conflict(
            "schema_token đã thay đổi, giữ bản nhập và tải lại cấu hình.",
        ));
    }

    let mut all_specs = Vec::new();
    let mut spec_map = BTreeMap::new();
    for g in &matching {
        for spec in &g.fields {
            if !spec.is_ui_only() {
                spec_map.insert(spec.key.as_str(), spec);
            }
            all_specs.push(spec.clone());
        }
    }
    for key in submitted.keys() {
        let Some(&spec) = spec_map.get(key.as_str()) else {
            return Err(ApiError::forbidden(
                "Trường không tồn tại hoặc không có quyền chỉnh sửa.",
            ));
        };
        let (_, min_edit, _) = spec.effective_acl();
        if actor_role < min_edit {
            return Err(ApiError::forbidden(
                "Tài khoản không có quyền chỉnh sửa trường này.",
            ));
        }
        if is_secondary && !spec.translatable {
            return Err(ApiError::bad_request(
                "Không thể lưu trường không hỗ trợ dịch ở ngôn ngữ phụ.",
            ));
        }
    }

    let (global_map, locale_map) = load_owner_option_maps_in_conn(
        &mut tx,
        "options_page",
        page_ref,
        &default_locale,
        &target_locale,
    )
    .await?;
    let mut saved_effective = BTreeMap::new();
    let mut merged_validation = BTreeMap::new();
    for spec in &all_specs {
        if spec.is_ui_only() {
            continue;
        }
        let saved = if is_secondary && spec.translatable {
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
        saved_effective.insert(spec.key.clone(), saved.clone());
        merged_validation.insert(
            spec.key.clone(),
            submitted.get(&spec.key).cloned().unwrap_or(saved),
        );
    }

    validate_field_values(
        &all_specs,
        FieldContext::Options,
        &merged_validation,
        Some(&saved_effective),
    )
    .map_err(ApiError::bad_request)?;

    let mut sub_specs = Vec::new();
    for (k, v) in &submitted {
        let spec = spec_map[k.as_str()];
        validate_url_or_link_spec_strict(spec, v).map_err(ApiError::bad_request)?;
        sub_specs.push(spec.clone());
    }
    let media_ids = collect_media_ids(&sub_specs, &submitted);
    crate::storage::fields::verify_collected_media_ids_in_conn(&mut tx, &media_ids).await?;
    crate::storage::fields::validate_relational_refs_in_conn(
        &mut tx,
        &all_specs,
        &merged_validation,
        Some(&saved_effective),
        is_secondary.then_some(target_locale.as_str()),
    )
    .await?;

    let sec_loc = is_secondary.then_some(target_locale.as_str());
    if !bump_owner_option_version_in_conn(
        &mut tx,
        "options_page",
        page_ref,
        "",
        sec_loc,
        expected_version,
    )
    .await?
    {
        return Err(ApiError::precondition_failed(
            "Phiên bản dữ liệu trang tùy chọn đã thay đổi.",
        ));
    }

    let ref_locale = sec_loc.unwrap_or("");
    for (k, v) in &submitted {
        let v_json = serde_json::to_string(v)?;
        upsert_owner_option_value_in_conn(&mut tx, "options_page", page_ref, "", sec_loc, k, &v_json)
            .await?;
        let spec = spec_map[k.as_str()];
        if matches!(
            spec.field_type,
            FieldType::Media | FieldType::Gallery | FieldType::Group | FieldType::Repeater
        ) {
            let single = BTreeMap::from([(k.clone(), v.clone())]);
            let ids = collect_media_ids(std::slice::from_ref(spec), &single).all_ids();
            sync_theme_option_media_refs_in_conn(
                &mut tx,
                "",
                "options_page",
                page_ref,
                ref_locale,
                k,
                &ids,
            )
            .await?;
        }
    }

    let out =
        get_options_page_values_in_conn(&mut tx, page_ref, requested_locale, actor_role).await?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(out)
}
