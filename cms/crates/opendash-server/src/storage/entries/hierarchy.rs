use std::collections::{BTreeMap, BTreeSet};

use opendash_core::content::{
    ContentTypeSettings, EffectiveContentSchema, EffectiveFieldStatus, FieldRole,
    LocationEvalContext, PatchField,
};
use serde_json::Value;
use sqlx::SqliteConnection;

use crate::error::{ApiError, ApiResult};

pub(crate) const MAX_ENTRY_HIERARCHY_DEPTH: i64 = 8;

/// Validates `next_parent_id` for `content_entries` within `conn`:
/// requires `settings.hierarchical`, same type & locale, active (untrashed) parent,
/// no cycle, and total chain height `parent_depth + self_subtree_height <= 8`.
pub(in crate::storage) async fn validate_entry_parent_in_conn(
    conn: &mut SqliteConnection,
    type_key: &str,
    settings: &ContentTypeSettings,
    entry_locale: &str,
    entry_id: Option<i64>,
    next_parent_id: Option<i64>,
) -> ApiResult<()> {
    let Some(parent_id) = next_parent_id else {
        return Ok(());
    };
    if !settings.hierarchical {
        return Err(ApiError::bad_request(
            "content_type_not_hierarchical: Loại nội dung này không bật chế độ phân cấp (hierarchical).",
        ));
    }
    if parent_id <= 0 {
        return Err(ApiError::unprocessable("parent_id phải là ID nguyên dương."));
    }
    if entry_id == Some(parent_id) {
        return Err(ApiError::conflict(
            "hierarchy_cycle_or_depth_exceeded: Nội dung không thể tự làm mục cha của chính nó.",
        ));
    }
    let parent_row: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT content_type_key, locale, trashed_at
           FROM content_entries
          WHERE id = ?",
    )
    .bind(parent_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((parent_type, parent_locale, parent_trashed)) = parent_row else {
        return Err(ApiError::unprocessable("Mục cha không tồn tại."));
    };
    if parent_type != type_key {
        return Err(ApiError::unprocessable(
            "Mục cha phải thuộc cùng loại nội dung.",
        ));
    }
    if parent_locale != entry_locale {
        return Err(ApiError::unprocessable(
            "Mục cha phải thuộc cùng ngôn ngữ với nội dung.",
        ));
    }
    if parent_trashed.is_some() {
        return Err(ApiError::unprocessable(
            "Mục cha đang nằm trong Thùng rác và không thể gắn làm cha.",
        ));
    }

    let ancestors: Vec<(i64, i64)> = sqlx::query_as(
        "WITH RECURSIVE lineage(id, parent_id, depth) AS (
             SELECT id, parent_id, 1 FROM content_entries WHERE id = ?
             UNION ALL
             SELECT p.id, p.parent_id, lineage.depth + 1
               FROM content_entries p
               JOIN lineage ON p.id = lineage.parent_id
              WHERE lineage.depth < 16
         )
         SELECT id, depth FROM lineage",
    )
    .bind(parent_id)
    .fetch_all(&mut *conn)
    .await?;

    if let Some(eid) = entry_id
        && ancestors.iter().any(|(id, _)| *id == eid)
    {
        return Err(ApiError::conflict(
            "hierarchy_cycle_or_depth_exceeded: Phát hiện vòng lặp trong cây phân cấp nội dung.",
        ));
    }
    let parent_depth = ancestors.iter().map(|(_, d)| *d).max().unwrap_or(1);

    let self_subtree_height: i64 = if let Some(eid) = entry_id {
        sqlx::query_scalar(
            "WITH RECURSIVE sub(id, depth) AS (
                 SELECT id, 1 FROM content_entries WHERE id = ?
                 UNION ALL
                 SELECT c.id, sub.depth + 1
                   FROM content_entries c
                   JOIN sub ON c.parent_id = sub.id
                  WHERE sub.depth < 16
             )
             SELECT COALESCE(MAX(depth), 1) FROM sub",
        )
        .bind(eid)
        .fetch_one(&mut *conn)
        .await?
    } else {
        1
    };

    if parent_depth + self_subtree_height > MAX_ENTRY_HIERARCHY_DEPTH {
        return Err(ApiError::conflict(
            "hierarchy_cycle_or_depth_exceeded: Cây phân cấp nội dung tối đa 8 tầng.",
        ));
    }
    Ok(())
}

/// Resolves `(next_parent_id, next_menu_order)` from existing row + request patches.
pub(in crate::storage) async fn resolve_entry_hierarchy_patch_in_conn(
    conn: &mut SqliteConnection,
    type_key: &str,
    entry_id: Option<i64>,
    parent_patch: &PatchField<i64>,
    menu_order_patch: Option<i32>,
) -> ApiResult<(Option<i64>, i32)> {
    let existing: (Option<i64>, i32) = match entry_id {
        Some(id) => sqlx::query_as(
            "SELECT parent_id, menu_order
               FROM content_entries
              WHERE id = ? AND content_type_key = ?",
        )
        .bind(id)
        .bind(type_key)
        .fetch_one(&mut *conn)
        .await?,
        None => (None, 0),
    };
    let next_parent = match *parent_patch {
        PatchField::Omitted => existing.0,
        PatchField::Clear => None,
        PatchField::Set(pid) => Some(pid),
    };
    let next_menu_order = menu_order_patch.unwrap_or(existing.1);
    Ok((next_parent, next_menu_order))
}

/// Maps `source_parent_id` to an active sibling entry in `target_locale` when creating translations.
pub(in crate::storage) async fn map_parent_translation_in_conn(
    conn: &mut SqliteConnection,
    type_key: &str,
    source_parent_id: Option<i64>,
    target_locale: &str,
) -> ApiResult<Option<i64>> {
    let Some(src_pid) = source_parent_id else {
        return Ok(None);
    };
    Ok(sqlx::query_scalar(
        "SELECT dst.id
           FROM content_entries src
           JOIN content_entries dst
             ON dst.content_type_key = src.content_type_key
            AND dst.translation_group_id = src.translation_group_id
            AND dst.locale = ?2
          WHERE src.id = ?1
            AND src.content_type_key = ?3
            AND dst.trashed_at IS NULL
          LIMIT 1",
    )
    .bind(src_pid)
    .bind(target_locale)
    .bind(type_key)
    .fetch_optional(&mut *conn)
    .await?)
}

/// Enforces Step B Pre-Lookup Role Gate before touching existing entry rows (prevents oracle leaks).
pub(crate) fn check_entry_role_gate_before_lookup(
    effective: &EffectiveContentSchema,
    type_key: &str,
    caller_role: FieldRole,
    input_values: &BTreeMap<String, Value>,
) -> ApiResult<()> {
    for key in input_values.keys() {
        let Some(fe) = effective.effective_fields.get(key) else {
            return Err(ApiError::unprocessable(format!(
                "unknown_field_key: Trường '{key}' không thuộc loại nội dung '{type_key}'."
            )));
        };
        if caller_role < fe.definition.effective_acl().1 {
            return Err(ApiError::forbidden(format!(
                "field_edit_forbidden: Vai trò hiện tại không có quyền sửa trường '{key}'."
            )));
        }
        if caller_role < FieldRole::Admin && effective.is_group_admin_only(&fe.source_id) {
            return Err(ApiError::forbidden(format!(
                "field_edit_forbidden: Nhóm chứa trường '{key}' chỉ cho phép Quản trị viên chỉnh sửa."
            )));
        }
    }
    Ok(())
}

/// Builds `LocationEvalContext` for an entry (including `SpecialPage` from `site_reading_settings`).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn build_entry_location_ctx_in_conn(
    conn: &mut SqliteConnection,
    type_key: &str,
    entry_id: Option<i64>,
    entry_locale: &str,
    caller_role: FieldRole,
    page_layout: Option<String>,
    custom_template: Option<String>,
    term_ids: &[i64],
) -> ApiResult<LocationEvalContext> {
    let special_page = if type_key == "page"
        && let Some(eid) = entry_id
    {
        let reading: Option<(Option<i64>, Option<i64>)> = sqlx::query_as(
            "SELECT homepage_page_id, posts_page_id
               FROM site_reading_settings
              WHERE locale = ?",
        )
        .bind(entry_locale)
        .fetch_optional(&mut *conn)
        .await?;
        match reading {
            Some((hp, _)) if hp == Some(eid) => Some("homepage".to_owned()),
            Some((_, pp)) if pp == Some(eid) => Some("posts_page".to_owned()),
            _ => None,
        }
    } else {
        None
    };

    let mut entry_term_pairs = BTreeSet::new();
    if !term_ids.is_empty() {
        let ids_json = serde_json::to_string(term_ids)?;
        let rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT id, kind
               FROM taxonomy_terms
              WHERE id IN (SELECT value FROM json_each(?))",
        )
        .bind(ids_json)
        .fetch_all(&mut *conn)
        .await?;
        for (id, kind) in rows {
            entry_term_pairs.insert((kind, id));
        }
    }

    Ok(LocationEvalContext {
        content_type: Some(type_key.to_owned()),
        taxonomy_term: None,
        options_page: None,
        page_layout,
        custom_template,
        special_page,
        entry_term_pairs,
        entry_id,
        user_role: caller_role,
    })
}

/// Step C merge: applies submitted changes ONLY for active fields matching `loc_ctx`,
/// ignoring payload edits and preserving 100% of previously persisted hidden values.
pub(in crate::storage) fn apply_location_aware_value_merge(
    effective: &EffectiveContentSchema,
    loc_ctx: &LocationEvalContext,
    input_values: &BTreeMap<String, Value>,
    previous_values: Option<&BTreeMap<String, Value>>,
    is_create: bool,
) -> BTreeMap<String, Value> {
    let mut out = if is_create {
        let mut init = BTreeMap::new();
        for (k, fe) in &effective.effective_fields {
            if !input_values.contains_key(k)
                && fe.effective_status != EffectiveFieldStatus::Withdrawn
                && effective.is_field_active_in_context(fe, loc_ctx)
                && let Some(def) = &fe.definition.default
            {
                init.insert(k.clone(), def.clone());
            }
        }
        init
    } else {
        previous_values.cloned().unwrap_or_default()
    };

    for (k, v) in input_values {
        let active_in_ctx = effective
            .effective_fields
            .get(k)
            .is_some_and(|fe| effective.is_field_active_in_context(fe, loc_ctx));
        if !active_in_ctx {
            continue;
        }
        if v.is_null() {
            out.remove(k);
        } else {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}

/// Strips values the caller lacks `min_view_role` (or group role) to read before returning responses.
pub(in crate::storage) fn filter_values_map_for_role(
    values: &mut BTreeMap<String, Value>,
    effective: &EffectiveContentSchema,
    caller_role: FieldRole,
) {
    values.retain(|key, _| {
        effective.effective_fields.get(key).is_none_or(|fe| {
            caller_role >= fe.definition.effective_acl().0
                && !(caller_role < FieldRole::Admin && effective.is_group_admin_only(&fe.source_id))
        })
    });
}
