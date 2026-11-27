use std::collections::{BTreeMap, BTreeSet};

use opendash_core::content::{
    EffectiveContentSchema, EffectiveFieldStatus, FieldRole, LocationEvalContext,
    TaxonomySelectionMode, TaxonomySettings,
};
use opendash_core::field_schema::{FieldSpec, FieldType, MAX_TERM_REF_ITEMS};
use serde_json::{Value, json};
use sqlx::{Sqlite, SqliteConnection, Transaction};

use crate::error::{ApiError, ApiResult};

fn parse_term_ref_ids(spec: &FieldSpec, val: &Value) -> Result<Vec<i64>, &'static str> {
    if val.is_null() {
        return Ok(Vec::new());
    }
    if spec.multiple {
        let arr = val
            .as_array()
            .ok_or("Giá trị trường term_ref (multiple) phải là mảng ID nguyên dương.")?;
        if arr.len() > MAX_TERM_REF_ITEMS {
            return Err("Giá trị trường term_ref vượt quá số lượng cho phép.");
        }
        let mut seen = BTreeSet::new();
        let mut out = Vec::with_capacity(arr.len());
        for item in arr {
            let id = item
                .as_i64()
                .filter(|n| *n > 0)
                .ok_or("Giá trị trường term_ref chứa ID không hợp lệ.")?;
            if seen.insert(id) {
                out.push(id);
            }
        }
        return Ok(out);
    }
    let id = val
        .as_i64()
        .filter(|n| *n > 0)
        .ok_or("Giá trị trường term_ref phải là ID nguyên dương.")?;
    Ok(vec![id])
}

/// Lists active, location-matching `TermRef(sync_entry_terms: true)` fields on `effective`.
pub(in crate::storage) fn collect_active_sync_term_fields<'a>(
    effective: &'a EffectiveContentSchema,
    loc_ctx: &LocationEvalContext,
    caller_role: Option<FieldRole>,
) -> Vec<&'a FieldSpec> {
    effective
        .effective_fields
        .values()
        .filter(|fe| {
            fe.effective_status == EffectiveFieldStatus::Active
                && fe.definition.field_type == FieldType::TermRef
                && fe.definition.sync_entry_terms
                && fe.definition.taxonomy.is_some()
                && effective.is_field_active_in_context(fe, loc_ctx)
                && caller_role.is_none_or(|role| role >= fe.definition.effective_acl().1)
        })
        .map(|fe| &fe.definition)
        .collect()
}

/// Step A: computes candidate `term_ids` when active editable `term_ref(sync_entry_terms)` fields
/// are present in `input_values`, replacing the corresponding taxonomy slice in `entry_terms`.
pub(in crate::storage) async fn compute_sync_entry_terms_step_a_in_conn(
    conn: &mut SqliteConnection,
    effective: &EffectiveContentSchema,
    non_tax_ctx: &LocationEvalContext,
    caller_role: FieldRole,
    input_values: &BTreeMap<String, Value>,
    explicit_term_ids: Option<&[i64]>,
    existing_term_ids: &[i64],
) -> ApiResult<Option<Vec<i64>>> {
    let sync_specs = collect_active_sync_term_fields(effective, non_tax_ctx, Some(caller_role));
    let mut touched_specs = Vec::new();
    for spec in &sync_specs {
        if let Some(val) = input_values.get(spec.key.as_str()) {
            let parsed = parse_term_ref_ids(spec, val).map_err(ApiError::unprocessable)?;
            if let Some(tax) = spec.taxonomy.as_deref() {
                touched_specs.push((tax.to_owned(), parsed));
            }
        }
    }
    if touched_specs.is_empty() {
        return Ok(explicit_term_ids.map(|s| s.to_vec()));
    }

    let mut working = explicit_term_ids.unwrap_or(existing_term_ids).to_vec();
    let kind_map = load_term_kinds_in_conn(conn, &working).await?;
    for (tax_key, new_ids) in touched_specs {
        working.retain(|id| kind_map.get(id).is_none_or(|k| k != &tax_key));
        for tid in new_ids {
            if !working.contains(&tid) {
                working.push(tid);
            }
        }
    }
    Ok(Some(working))
}

/// Reverse-syncs `entry_terms` back into matching `term_ref(sync_entry_terms: true)` keys in `values`.
/// Pass `skip_present_in = Some(input_values)` on composite Save so explicit user inputs keep exact validation.
pub(in crate::storage) async fn sync_term_ref_fields_from_entry_terms_in_conn(
    conn: &mut SqliteConnection,
    effective: &EffectiveContentSchema,
    loc_ctx: &LocationEvalContext,
    values: &mut BTreeMap<String, Value>,
    term_ids: &[i64],
    skip_present_in: Option<&BTreeMap<String, Value>>,
) -> ApiResult<bool> {
    let sync_specs = collect_active_sync_term_fields(effective, loc_ctx, None);
    if sync_specs.is_empty() {
        return Ok(false);
    }
    let kind_map = load_term_kinds_in_conn(conn, term_ids).await?;
    let mut changed = false;
    for spec in sync_specs {
        if skip_present_in.is_some_and(|map| map.contains_key(spec.key.as_str())) {
            continue;
        }
        let Some(tax_key) = spec.taxonomy.as_deref() else {
            continue;
        };
        let matched: Vec<i64> = term_ids
            .iter()
            .copied()
            .filter(|id| kind_map.get(id).is_some_and(|k| k == tax_key))
            .collect();
        let next_val = if spec.multiple {
            Some(Value::Array(matched.into_iter().map(Value::from).collect()))
        } else {
            matched.first().map(|&id| json!(id))
        };
        match next_val {
            Some(val) => {
                if values.get(spec.key.as_str()) != Some(&val) {
                    values.insert(spec.key.clone(), val);
                    changed = true;
                }
            }
            None => {
                if values.remove(spec.key.as_str()).is_some() {
                    changed = true;
                }
            }
        }
    }
    Ok(changed)
}

pub(in crate::storage) async fn load_term_kinds_in_conn(
    conn: &mut SqliteConnection,
    term_ids: &[i64],
) -> ApiResult<BTreeMap<i64, String>> {
    let mut out = BTreeMap::new();
    if term_ids.is_empty() {
        return Ok(out);
    }
    let ids_json = serde_json::to_string(term_ids)?;
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, kind FROM taxonomy_terms WHERE id IN (SELECT value FROM json_each(?))",
    )
    .bind(ids_json)
    .fetch_all(&mut *conn)
    .await?;
    for (id, kind) in rows {
        out.insert(id, kind);
    }
    Ok(out)
}

/// Validates `TaxonomySettings.selection_mode == Single` across per-taxonomy term counts.
pub(in crate::storage) async fn enforce_taxonomy_selection_mode_in_conn(
    conn: &mut SqliteConnection,
    type_key: &str,
    counts: &std::collections::HashMap<String, usize>,
) -> ApiResult<()> {
    let eligible = crate::storage::content::load_effective_taxonomies_in_tx(conn, type_key).await?;
    for (tax_key, count) in counts {
        if *count <= 1 || !eligible.iter().any(|k| k == tax_key) {
            continue;
        }
        let raw_settings: Option<String> = sqlx::query_scalar(
            "SELECT settings_json FROM taxonomy_definitions
              WHERE key = ? AND provider_status = 'active' AND declaration_state = 'declared'",
        )
        .bind(tax_key)
        .fetch_optional(&mut *conn)
        .await?;
        let Some(raw) = raw_settings else {
            continue;
        };
        let settings: TaxonomySettings = serde_json::from_str(&raw).unwrap_or_default();
        if settings.selection_mode == TaxonomySelectionMode::Single {
            return Err(ApiError::unprocessable(format!(
                "taxonomy_single_selection_violation: Taxonomy '{tax_key}' chỉ cho phép chọn tối đa 1 term."
            )));
        }
    }
    Ok(())
}

/// Enforces `selection_mode == Single` and `required_on_publish` (auto-assigning
/// locale-resolved `default_term_id` when empty, or returning `422 taxonomy_required_on_publish`).
/// Returns `Ok(true)` if `entry_terms` (and synced `values_json`) was modified.
pub(in crate::storage) async fn enforce_taxonomy_publish_rules_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    entry_id: i64,
    entry_locale: &str,
    effective: Option<&EffectiveContentSchema>,
    loc_ctx: Option<&LocationEvalContext>,
) -> ApiResult<bool> {
    if matches!(type_key, "pattern" | "section") {
        return Ok(false);
    }
    let eligible =
        crate::storage::content::load_effective_taxonomies_in_tx(&mut **tx, type_key).await?;
    if eligible.is_empty() {
        return Ok(false);
    }

    let eligible_json = serde_json::to_string(&eligible)?;
    let tax_defs: Vec<(String, String)> = sqlx::query_as(
        "SELECT key, settings_json
           FROM taxonomy_definitions
          WHERE key IN (SELECT value FROM json_each(?))
            AND provider_status = 'active'
            AND declaration_state = 'declared'
          ORDER BY key ASC",
    )
    .bind(eligible_json)
    .fetch_all(&mut **tx)
    .await?;

    let existing_terms: Vec<(i64, String)> = sqlx::query_as(
        "SELECT t.id, t.kind
           FROM entry_terms et
           JOIN taxonomy_terms t ON t.id = et.term_id
          WHERE et.entry_id = ?
          ORDER BY t.id ASC",
    )
    .bind(entry_id)
    .fetch_all(&mut **tx)
    .await?;

    let mut by_kind: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    let mut final_term_ids: Vec<i64> = existing_terms.iter().map(|(id, _)| *id).collect();
    for (tid, kind) in existing_terms {
        by_kind.entry(kind).or_default().push(tid);
    }

    let mut assigned_any = false;
    for (tax_key, raw_settings) in tax_defs {
        let settings: TaxonomySettings = serde_json::from_str(&raw_settings).unwrap_or_default();
        let count = by_kind.get(&tax_key).map_or(0, Vec::len);
        if settings.selection_mode == TaxonomySelectionMode::Single && count > 1 {
            return Err(ApiError::unprocessable(format!(
                "taxonomy_single_selection_violation: Taxonomy '{tax_key}' chỉ cho phép chọn tối đa 1 term."
            )));
        }
        if settings.required_on_publish && count == 0 {
            let resolved_default: Option<i64> = if let Some(def_id) = settings.default_term_id {
                sqlx::query_scalar(
                    "SELECT id
                       FROM taxonomy_terms
                      WHERE kind = ?1 AND locale = ?2
                        AND (id = ?3 OR translation_group_id = (
                            SELECT translation_group_id FROM taxonomy_terms WHERE id = ?3 AND kind = ?1
                        ))
                      LIMIT 1",
                )
                .bind(&tax_key)
                .bind(entry_locale)
                .bind(def_id)
                .fetch_optional(&mut **tx)
                .await?
            } else {
                None
            };
            let Some(default_tid) = resolved_default else {
                return Err(ApiError::unprocessable(format!(
                    "taxonomy_required_on_publish: Taxonomy '{tax_key}' bắt buộc phải có term khi xuất bản."
                )));
            };
            sqlx::query("INSERT OR IGNORE INTO entry_terms (entry_id, term_id) VALUES (?, ?)")
                .bind(entry_id)
                .bind(default_tid)
                .execute(&mut **tx)
                .await?;
            if !final_term_ids.contains(&default_tid) {
                final_term_ids.push(default_tid);
            }
            assigned_any = true;
        }
    }

    if assigned_any
        && let (Some(eff), Some(ctx)) = (effective, loc_ctx)
    {
        sync_entry_values_json_with_terms_in_tx(tx, entry_id, eff, ctx, &final_term_ids).await?;
    }
    Ok(assigned_any)
}

/// Syncs persisted `content_entries.values_json` for active `sync_entry_terms` fields in `tx`.
pub(in crate::storage) async fn sync_entry_values_json_with_terms_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    entry_id: i64,
    effective: &EffectiveContentSchema,
    loc_ctx: &LocationEvalContext,
    term_ids: &[i64],
) -> ApiResult<bool> {
    let raw_values: String =
        sqlx::query_scalar("SELECT values_json FROM content_entries WHERE id = ?")
            .bind(entry_id)
            .fetch_one(&mut **tx)
            .await?;
    let mut values: BTreeMap<String, Value> = serde_json::from_str(&raw_values)
        .map_err(|_| ApiError::internal("Dữ liệu custom fields không hợp lệ."))?;
    let changed = sync_term_ref_fields_from_entry_terms_in_conn(
        &mut **tx,
        effective,
        loc_ctx,
        &mut values,
        term_ids,
        None,
    )
    .await?;
    if changed {
        let next_json = serde_json::to_string(&values)?;
        sqlx::query("UPDATE content_entries SET values_json = ? WHERE id = ?")
            .bind(next_json)
            .bind(entry_id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(changed)
}
