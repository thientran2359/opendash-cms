use std::collections::{BTreeMap, BTreeSet};

use opendash_core::{
    content::{
        AttachedFieldGroupSnapshot, EffectiveTermSchema, FieldGroupDomain, FieldGroupPresentation,
        FieldRole, LocationEvalContext, TaxonomyDefinition, TermSeoInput, evaluate_location_rules,
        validate_taxonomy_target_cpts,
    },
    field_schema::{
        FieldContext, FieldSpec, FieldType, collect_media_ids, default_value_for_field_spec,
        validate_field_values,
    },
};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection, SqlitePool};

use super::definitions::definition_in_conn;
use super::terms::mutation_error;
use crate::error::{ApiError, ApiResult};
use crate::storage::content::{load_attached_groups_in_tx, map_transition_error, table_exists_in_tx};

#[derive(Debug, Clone, Serialize)]
pub struct TermFieldGroupView {
    pub group_id: String,
    pub title: String,
    pub description: String,
    pub presentation: FieldGroupPresentation,
    pub fields: Vec<FieldSpec>,
    pub read_only_keys: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EffectiveTermEditorConfig {
    pub schema_version: u8,
    pub schema_token: String,
    pub taxonomy: TaxonomyDefinition,
    pub groups: Vec<TermFieldGroupView>,
    pub values: BTreeMap<String, Value>,
    pub seo: TermSeoInput,
}

pub(in crate::storage) async fn load_effective_term_schema_in_tx(
    conn: &mut SqliteConnection,
    kind: &str,
) -> ApiResult<Option<EffectiveTermSchema>> {
    let Some(base) = definition_in_conn(conn, kind).await? else {
        return Ok(None);
    };
    let groups =
        load_attached_groups_in_tx(conn, "taxonomy_term", kind, FieldGroupDomain::Term).await?;
    let eff = EffectiveTermSchema::assemble(base, groups)
        .map_err(|e| map_transition_error(e, true))?;
    Ok(Some(eff))
}

pub(in crate::storage) async fn load_taxonomy_content_types_map_in_conn(
    conn: &mut SqliteConnection,
) -> ApiResult<BTreeMap<String, Vec<String>>> {
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if !table_exists_in_tx(conn, "content_types").await? {
        return Ok(map);
    }
    let cpts: Vec<String> = sqlx::query_scalar(
        "SELECT key FROM content_types
          WHERE key NOT IN ('pattern', 'section')
            AND provider_status = 'active'
            AND declaration_state = 'declared'
          ORDER BY key ASC",
    )
    .fetch_all(&mut *conn)
    .await?;
    for cpt in cpts {
        let taxs = crate::storage::content::load_effective_taxonomies_in_tx(conn, &cpt).await?;
        for tax in taxs {
            map.entry(tax).or_default().push(cpt.clone());
        }
    }
    Ok(map)
}

pub(in crate::storage) async fn sync_taxonomy_content_types_in_tx(
    conn: &mut SqliteConnection,
    tax_key: &str,
    target_cpts: &[String],
) -> ApiResult<()> {
    validate_taxonomy_target_cpts(target_cpts).map_err(ApiError::bad_request)?;
    let target_set: BTreeSet<&str> = target_cpts.iter().map(String::as_str).collect();
    for cpt in &target_set {
        let exists: Option<(String, String)> = sqlx::query_as(
            "SELECT provider_status, declaration_state FROM content_types WHERE key = ?",
        )
        .bind(cpt)
        .fetch_optional(&mut *conn)
        .await?;
        if exists.is_none() {
            return Err(ApiError::not_found(format!(
                "Loại nội dung '{cpt}' được chọn trong content_types không tồn tại."
            )));
        }
    }
    let all_cpts: Vec<String> = sqlx::query_scalar(
        "SELECT key FROM content_types
          WHERE key NOT IN ('pattern', 'section')
          ORDER BY key ASC",
    )
    .fetch_all(&mut *conn)
    .await?;
    for cpt in all_cpts {
        let mut current =
            crate::storage::content::load_effective_taxonomies_in_tx(conn, &cpt).await?;
        let wants = target_set.contains(cpt.as_str());
        let has = current.iter().any(|t| t == tax_key);
        if wants == has {
            continue;
        }
        if wants {
            current.push(tax_key.to_owned());
            current.sort();
            current.dedup();
        } else {
            current.retain(|t| t != tax_key);
        }
        let json = serde_json::to_string(&current)?;
        sqlx::query(
            "INSERT INTO content_type_taxonomy_overrides (content_type_key, taxonomies_json)
             VALUES (?, ?)
             ON CONFLICT(content_type_key) DO UPDATE SET
               taxonomies_json = excluded.taxonomies_json,
               updated_at = datetime('now')",
        )
        .bind(&cpt)
        .bind(&json)
        .execute(&mut *conn)
        .await?;
        sqlx::query(
            "UPDATE content_types SET version = version + 1, updated_at = datetime('now') WHERE key = ?",
        )
        .bind(&cpt)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

pub(in crate::storage) async fn load_stored_term_values_and_seo_in_conn(
    conn: &mut SqliteConnection,
    term_id: i64,
) -> ApiResult<(BTreeMap<String, Value>, TermSeoInput)> {
    let row: Option<(String, Option<String>, Option<String>, Option<String>, Option<String>, i64)> =
        sqlx::query_as(
            "SELECT values_json, seo_title, seo_description, seo_og_image_id, seo_canonical_url, seo_noindex
               FROM taxonomy_terms WHERE id = ?",
        )
        .bind(term_id)
        .fetch_optional(&mut *conn)
        .await?;
    let Some((raw_values, title, description, og_image_id, canonical_url, noindex)) = row else {
        return Err(ApiError::not_found("Không tìm thấy term."));
    };
    let values: BTreeMap<String, Value> = serde_json::from_str(&raw_values)
        .map_err(|_| ApiError::internal("values_json của taxonomy_terms không hợp lệ."))?;
    let seo = TermSeoInput {
        title,
        description,
        og_image_id,
        canonical_url,
        noindex: noindex != 0,
    };
    Ok((values, seo))
}

pub(in crate::storage) fn compute_term_schema_token(
    eff: &EffectiveTermSchema,
    role: FieldRole,
) -> String {
    let payload = eff.canonical_token_payload_json(None, &[], Some(role));
    let digest = Sha256::digest(payload.as_bytes());
    let mut hex = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{b:02x}");
    }
    format!("sha256:{hex}")
}

pub(in crate::storage) fn project_term_editor_config(
    eff: &EffectiveTermSchema,
    stored_values: &BTreeMap<String, Value>,
    seo: TermSeoInput,
    role: FieldRole,
) -> EffectiveTermEditorConfig {
    let eval_ctx = LocationEvalContext {
        taxonomy_term: Some(eff.taxonomy.key.clone()),
        user_role: role,
        ..Default::default()
    };
    let mut groups_out = Vec::new();
    let mut values_out = BTreeMap::new();

    for grp in &eff.attached_groups {
        if !grp.is_runtime_active() || !evaluate_location_rules(&grp.location, &eval_ctx) {
            continue;
        }
        let mut visible_fields = Vec::new();
        let mut read_only = BTreeSet::new();
        for spec in &grp.fields {
            let (min_view, min_edit, _) = spec.effective_acl();
            if role < min_view {
                continue;
            }
            if !spec.is_ui_only() && role < min_edit {
                read_only.insert(spec.key.clone());
            }
            visible_fields.push(spec.clone());
            if !spec.is_ui_only() {
                let val = if spec.field_type == FieldType::Secret {
                    Value::Bool(stored_values.contains_key(&spec.key))
                } else {
                    stored_values
                        .get(&spec.key)
                        .cloned()
                        .unwrap_or_else(|| default_value_for_field_spec(spec))
                };
                values_out.insert(spec.key.clone(), val);
            }
        }
        if !visible_fields.is_empty() {
            groups_out.push(TermFieldGroupView {
                group_id: grp.group_id.clone(),
                title: grp.title.clone(),
                description: grp.description.clone(),
                presentation: grp.presentation,
                fields: visible_fields,
                read_only_keys: read_only,
            });
        }
    }
    groups_out.sort_by(|a, b| {
        a.presentation
            .order
            .cmp(&b.presentation.order)
            .then_with(|| a.group_id.cmp(&b.group_id))
    });
    EffectiveTermEditorConfig {
        schema_version: 2,
        schema_token: compute_term_schema_token(eff, role),
        taxonomy: eff.taxonomy.clone(),
        groups: groups_out,
        values: values_out,
        seo,
    }
}

pub async fn effective_term_editor_config(
    db: &SqlitePool,
    term_id: i64,
    kind: &str,
    role: FieldRole,
) -> ApiResult<EffectiveTermEditorConfig> {
    let mut conn = db.acquire().await?;
    effective_term_editor_config_in_conn(&mut conn, term_id, kind, role).await
}

pub(in crate::storage) async fn effective_term_editor_config_in_conn(
    conn: &mut SqliteConnection,
    term_id: i64,
    kind: &str,
    role: FieldRole,
) -> ApiResult<EffectiveTermEditorConfig> {
    let eff = load_effective_term_schema_in_tx(conn, kind)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy taxonomy."))?;
    let (stored_values, seo) = load_stored_term_values_and_seo_in_conn(conn, term_id).await?;
    Ok(project_term_editor_config(&eff, &stored_values, seo, role))
}

#[allow(clippy::too_many_arguments)]
pub(in crate::storage) async fn write_term_meta_and_seo_in_tx(
    conn: &mut SqliteConnection,
    term_id: i64,
    kind: &str,
    term_locale: &str,
    actor_role: FieldRole,
    schema_token: Option<&str>,
    values_patch: Option<&BTreeMap<String, Value>>,
    seo_patch: Option<&TermSeoInput>,
) -> ApiResult<()> {
    let Some(eff) = load_effective_term_schema_in_tx(conn, kind).await? else {
        return Err(ApiError::not_found("Không tìm thấy taxonomy."));
    };
    let (stored_values, _) = load_stored_term_values_and_seo_in_conn(conn, term_id).await?;
    let eval_ctx = LocationEvalContext {
        taxonomy_term: Some(kind.to_owned()),
        user_role: actor_role,
        ..Default::default()
    };

    if let Some(patch) = values_patch {
        // Pre-lookup Role Gate BEFORE token check (never disclose restricted schema on downgrade)
        for key in patch.keys() {
            let Some(entry) = eff.effective_fields.get(key) else {
                return Err(ApiError::forbidden("Không có quyền cập nhật trường này."));
            };
            let Some(grp) = eff
                .attached_groups
                .iter()
                .find(|g| g.group_id == entry.source_id)
            else {
                return Err(ApiError::forbidden("Nhóm trường không tồn tại."));
            };
            if !evaluate_location_rules(&grp.location, &eval_ctx) {
                return Err(ApiError::forbidden(
                    "Không có quyền chỉnh sửa trường trong ngữ cảnh này.",
                ));
            }
            let (_, min_edit, _) = entry.definition.effective_acl();
            if actor_role < min_edit {
                return Err(ApiError::forbidden("Không có quyền chỉnh sửa trường này."));
            }
        }
        let token = schema_token
            .ok_or_else(|| ApiError::bad_request("Thiếu schema_token để ghi term meta."))?;
        let current_tok = compute_term_schema_token(&eff, actor_role);
        if token != current_tok {
            return Err(ApiError::conflict(
                "schema_token đã thay đổi, giữ bản nhập và tải lại cấu hình.",
            ));
        }

        let mut next_values = stored_values.clone();
        for (k, v) in patch {
            if v.is_null() {
                next_values.remove(k);
            } else {
                next_values.insert(k.clone(), v.clone());
            }
        }

        // Validate against ALL active term groups matching taxonomy_term == kind (using Admin view)
        // while skipping hidden non-evaluated branches.
        let mut active_specs = Vec::new();
        for grp in &eff.attached_groups {
            let grp_ctx = LocationEvalContext {
                taxonomy_term: Some(kind.to_owned()),
                user_role: FieldRole::Admin,
                ..Default::default()
            };
            if grp.is_runtime_active() && evaluate_location_rules(&grp.location, &grp_ctx) {
                active_specs.extend(grp.fields.clone());
            }
        }
        let valid_keys: BTreeSet<&str> = active_specs
            .iter()
            .filter(|s| !s.is_ui_only())
            .map(|s| s.key.as_str())
            .collect();
        let val_prev: BTreeMap<String, Value> = stored_values
            .iter()
            .filter(|(k, _)| valid_keys.contains(k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let val_next: BTreeMap<String, Value> = next_values
            .iter()
            .filter(|(k, _)| valid_keys.contains(k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        validate_field_values(
            &active_specs,
            FieldContext::Term,
            &val_next,
            Some(&val_prev),
        )
        .map_err(ApiError::bad_request)?;
        let collected = collect_media_ids(&active_specs, &val_next);
        crate::storage::fields::verify_collected_media_ids_in_conn(conn, &collected).await?;
        crate::storage::fields::validate_relational_refs_in_conn(
            conn,
            &active_specs,
            &val_next,
            Some(&val_prev),
            Some(term_locale),
        )
        .await?;

        sqlx::query("UPDATE taxonomy_terms SET values_json = ? WHERE id = ?")
            .bind(serde_json::to_string(&next_values)?)
            .bind(term_id)
            .execute(&mut *conn)
            .await
            .map_err(mutation_error)?;

        sqlx::query("DELETE FROM term_field_media_refs WHERE term_id = ?")
            .bind(term_id)
            .execute(&mut *conn)
            .await?;
        for spec in &active_specs {
            if spec.is_ui_only()
                || !matches!(
                    spec.field_type,
                    FieldType::Media | FieldType::Gallery | FieldType::Group | FieldType::Repeater
                )
            {
                continue;
            }
            if let Some(v) = val_next.get(&spec.key) {
                let single = BTreeMap::from([(spec.key.clone(), v.clone())]);
                let ids = collect_media_ids(std::slice::from_ref(spec), &single).all_ids();
                for mid in ids {
                    sqlx::query(
                        "INSERT OR IGNORE INTO term_field_media_refs (term_id, locale, field_key, media_id)
                         VALUES (?, ?, ?, ?)",
                    )
                    .bind(term_id)
                    .bind(term_locale)
                    .bind(&spec.key)
                    .bind(&mid)
                    .execute(&mut *conn)
                    .await?;
                }
            }
        }
    }

    if let Some(seo) = seo_patch {
        if actor_role != FieldRole::Admin {
            return Err(ApiError::forbidden(
                "Editor không được chỉnh sửa SEO của term.",
            ));
        }
        let seo = crate::seo::validate_seo_input(&crate::seo::SeoMetadataInput {
            title: seo.title.clone(),
            description: seo.description.clone(),
            og_image_id: seo.og_image_id.clone(),
            canonical_url: seo.canonical_url.clone(),
            noindex: seo.noindex,
        })
        .map_err(|ApiError(_, msg)| ApiError::bad_request(msg))?;
        if let Some(og_id) = seo.og_image_id.as_deref() {
            let row = sqlx::query(
                "SELECT kind FROM media_assets WHERE id = ? AND lifecycle_state = 'active'",
            )
            .bind(og_id)
            .fetch_optional(&mut *conn)
            .await?;
            let Some(r) = row else {
                return Err(ApiError::bad_request("Ảnh SEO og_image_id không tồn tại hoặc đang ở thùng rác."));
            };
            let kind_str: String = r.try_get("kind")?;
            if kind_str != "image" {
                return Err(ApiError::bad_request("og_image_id của term SEO phải là ảnh."));
            }
        }
        sqlx::query(
            "UPDATE taxonomy_terms
                SET seo_title = ?, seo_description = ?, seo_og_image_id = ?,
                    seo_canonical_url = ?, seo_noindex = ?
              WHERE id = ?",
        )
        .bind(&seo.title)
        .bind(&seo.description)
        .bind(&seo.og_image_id)
        .bind(&seo.canonical_url)
        .bind(i64::from(seo.noindex))
        .bind(term_id)
        .execute(&mut *conn)
        .await
        .map_err(mutation_error)?;

        sqlx::query("DELETE FROM seo_public_media_refs WHERE source_kind = 'term_seo' AND source_id = ?")
            .bind(term_id)
            .execute(&mut *conn)
            .await?;
        if let Some(og_id) = seo.og_image_id.as_deref() {
            sqlx::query(
                "INSERT OR IGNORE INTO seo_public_media_refs (source_kind, source_id, media_id)
                 VALUES ('term_seo', ?, ?)",
            )
            .bind(term_id)
            .bind(og_id)
            .execute(&mut *conn)
            .await?;
        }
    }

    Ok(())
}
