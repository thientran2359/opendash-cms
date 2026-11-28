use std::collections::{BTreeMap, BTreeSet, HashSet};

use opendash_core::content::{
    FieldGroupDomain, FieldGroupRecord, LocationRuleGroups, ManifestFieldGroupInput,
    NormalizedFieldGroup, extract_materialized_targets,
};
use opendash_core::field_schema::{FieldScopeKind, FieldSpec};

use super::*;

#[derive(Default)]
pub(super) struct TargetDomainSnapshots {
    pub content: BTreeMap<String, (EffectiveContentSchema, bool)>,
    pub term: BTreeMap<String, (EffectiveTermSchema, bool)>,
    pub options_pages: BTreeSet<String>,
}

pub(super) async fn collect_target_snapshots_in_tx(
    conn: &mut SqliteConnection,
    domain: FieldGroupDomain,
    targets: &[(String, String)],
    missing_target_as_unprocessable: bool,
) -> ApiResult<TargetDomainSnapshots> {
    let mut out = TargetDomainSnapshots::default();
    let missing_err = |msg: &'static str| {
        if missing_target_as_unprocessable {
            ApiError::unprocessable(msg)
        } else {
            ApiError::not_found(msg)
        }
    };

    for (kind, key) in targets {
        match domain {
            FieldGroupDomain::Content => {
                if kind != "content_type" {
                    return Err(ApiError::unprocessable(
                        "invalid_location_rules_domain: Nhóm trường Content chỉ gắn với content_type.",
                    ));
                }
                if key == "*" {
                    let all_cpts: Vec<String> = sqlx::query_scalar(
                        "SELECT key FROM content_types WHERE key NOT IN ('pattern', 'section') ORDER BY key ASC",
                    )
                    .fetch_all(&mut *conn)
                    .await?;
                    for cpt in all_cpts {
                        if !out.content.contains_key(&cpt)
                            && let Some(eff) =
                                load_effective_content_schema_in_tx(conn, &cpt).await?
                        {
                            let has_entries: bool = sqlx::query_scalar(
                                "SELECT EXISTS(SELECT 1 FROM content_entries WHERE content_type_key = ?)",
                            )
                            .bind(&cpt)
                            .fetch_one(&mut *conn)
                            .await?;
                            out.content.insert(cpt, (eff, has_entries));
                        }
                    }
                } else if !out.content.contains_key(key) {
                    let Some(eff) = load_effective_content_schema_in_tx(conn, key).await? else {
                        return Err(missing_err("Loại nội dung mục tiêu không tồn tại."));
                    };
                    let has_entries: bool = sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM content_entries WHERE content_type_key = ?)",
                    )
                    .bind(key)
                    .fetch_one(&mut *conn)
                    .await?;
                    out.content.insert(key.clone(), (eff, has_entries));
                }
            }
            FieldGroupDomain::Term => {
                if kind != "taxonomy_term" {
                    return Err(ApiError::unprocessable(
                        "invalid_location_rules_domain: Nhóm trường Term chỉ gắn với taxonomy_term.",
                    ));
                }
                if key == "*" {
                    let all_tax: Vec<String> =
                        sqlx::query_scalar("SELECT key FROM taxonomy_definitions ORDER BY key ASC")
                            .fetch_all(&mut *conn)
                            .await?;
                    for tax in all_tax {
                        if !out.term.contains_key(&tax)
                            && let Some(eff) =
                                crate::storage::taxonomy::load_effective_term_schema_in_tx(
                                    conn, &tax,
                                )
                                .await?
                        {
                            let has_terms: bool = sqlx::query_scalar(
                                "SELECT EXISTS(SELECT 1 FROM taxonomy_terms WHERE kind = ?)",
                            )
                            .bind(&tax)
                            .fetch_one(&mut *conn)
                            .await?;
                            out.term.insert(tax, (eff, has_terms));
                        }
                    }
                } else if !out.term.contains_key(key) {
                    let Some(eff) =
                        crate::storage::taxonomy::load_effective_term_schema_in_tx(conn, key)
                            .await?
                    else {
                        return Err(missing_err("Taxonomy mục tiêu không tồn tại."));
                    };
                    let has_terms: bool = sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM taxonomy_terms WHERE kind = ?)",
                    )
                    .bind(key)
                    .fetch_one(&mut *conn)
                    .await?;
                    out.term.insert(key.clone(), (eff, has_terms));
                }
            }
            FieldGroupDomain::OptionsPage => {
                if kind != "options_page" {
                    return Err(ApiError::unprocessable(
                        "invalid_location_rules_domain: Nhóm trường OptionsPage chỉ gắn với options_page.",
                    ));
                }
                crate::storage::options_pages::verify_options_page_target_exists_in_conn(conn, key)
                    .await
                    .map_err(|err| {
                        if missing_target_as_unprocessable
                            && err.0 == axum::http::StatusCode::NOT_FOUND
                        {
                            ApiError::unprocessable("Trang tùy chọn mục tiêu không tồn tại.")
                        } else {
                            err
                        }
                    })?;
                out.options_pages.insert(key.clone());
            }
        }
    }
    Ok(out)
}

pub(super) async fn verify_snapshots_after_write_in_tx(
    conn: &mut SqliteConnection,
    snapshots: &TargetDomainSnapshots,
    incompat_as_conflict: bool,
) -> ApiResult<()> {
    for (cpt, (old_eff, has_entries)) in &snapshots.content {
        if let Some(new_eff) = load_effective_content_schema_in_tx(conn, cpt).await? {
            verify_effective_schema_transition(Some(old_eff), &new_eff, *has_entries)
                .map_err(|e| map_transition_error(e, incompat_as_conflict))?;
        }
    }
    for (tax, (old_eff, has_terms)) in &snapshots.term {
        if let Some(new_eff) =
            crate::storage::taxonomy::load_effective_term_schema_in_tx(conn, tax).await?
        {
            verify_effective_term_schema_transition(Some(old_eff), &new_eff, *has_terms)
                .map_err(|e| map_transition_error(e, incompat_as_conflict))?;
        }
    }
    for page_ref in &snapshots.options_pages {
        let attached =
            crate::storage::options_pages::load_attached_options_groups_in_conn(conn, page_ref)
                .await?;
        crate::storage::options_pages::check_options_page_group_collisions_in_conn(
            conn, page_ref, &attached,
        )
        .await?;
    }
    Ok(())
}

pub(super) async fn sync_search_for_field_group_targets_in_tx(
    conn: &mut SqliteConnection,
    domain: FieldGroupDomain,
    targets: &[(String, String)],
) -> ApiResult<()> {
    if domain != FieldGroupDomain::Content || targets.is_empty() {
        return Ok(());
    }
    if targets
        .iter()
        .any(|(kind, key)| kind == "content_type" && key == "*")
    {
        crate::storage::search::rebuild_search_index_in_conn(conn).await?;
        return Ok(());
    }
    let distinct: BTreeSet<&str> = targets
        .iter()
        .filter(|(kind, _)| kind == "content_type")
        .map(|(_, key)| key.as_str())
        .collect();
    for key in distinct {
        crate::storage::search::sync_entries_for_type_in_conn(conn, key).await?;
    }
    Ok(())
}

pub(super) async fn upsert_group_targets_in_tx(
    conn: &mut SqliteConnection,
    group_id: &str,
    existing_targets: &[FieldGroupTargetBinding],
    incoming_targets: &[(String, String)],
    fields_for_check: &[FieldSpec],
    purge_unlinked_unused_only: bool,
) -> ApiResult<()> {
    let incoming_set: HashSet<(&str, &str)> = incoming_targets
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    for (t_kind, t_key) in incoming_targets {
        sqlx::query(
            "INSERT INTO content_field_group_targets
             (group_id, target_kind, target_key, association_state)
             VALUES (?, ?, ?, 'declared')
             ON CONFLICT(group_id, target_kind, target_key) DO UPDATE SET
                 association_state = 'declared',
                 updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
        )
        .bind(group_id)
        .bind(t_kind)
        .bind(t_key)
        .execute(&mut *conn)
        .await?;
    }

    for old in existing_targets {
        if !incoming_set.contains(&(old.target_kind.as_str(), old.target_key.as_str())) {
            let mut keep_withdrawn = !purge_unlinked_unused_only;
            if purge_unlinked_unused_only {
                let has_values = has_persisted_group_values_for_target_in_tx(
                    conn,
                    &old.target_kind,
                    &old.target_key,
                    fields_for_check,
                )
                .await?;
                let has_layout_ref: bool = if old.target_kind == "content_type" {
                    sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM content_editor_layouts WHERE type_key = ? AND instr(layout_json, ?) > 0)",
                    )
                    .bind(&old.target_key)
                    .bind(group_id)
                    .fetch_one(&mut *conn)
                    .await?
                } else {
                    false
                };
                keep_withdrawn = has_values || has_layout_ref;
            }
            if keep_withdrawn {
                sqlx::query(
                    "UPDATE content_field_group_targets
                     SET association_state = 'withdrawn',
                         updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                     WHERE group_id = ? AND target_kind = ? AND target_key = ?",
                )
                .bind(group_id)
                .bind(&old.target_kind)
                .bind(&old.target_key)
                .execute(&mut *conn)
                .await?;
            } else {
                sqlx::query(
                    "DELETE FROM content_field_group_targets
                     WHERE group_id = ? AND target_kind = ? AND target_key = ?",
                )
                .bind(group_id)
                .bind(&old.target_kind)
                .bind(&old.target_key)
                .execute(&mut *conn)
                .await?;
            }
        }
    }
    Ok(())
}

pub(super) async fn sync_owner_field_groups_in_tx(
    conn: &mut SqliteConnection,
    owner_kind_str: &str,
    owner_id_str: &str,
    field_groups: &[ManifestFieldGroupInput],
    incompat_as_conflict: bool,
) -> ApiResult<()> {
    let mut normalized_groups: Vec<(
        String,
        ManifestFieldGroupInput,
        NormalizedFieldGroup,
        Vec<(String, String)>,
    )> = Vec::with_capacity(field_groups.len());
    let mut all_targets_by_domain: BTreeMap<FieldGroupDomain, Vec<(String, String)>> =
        BTreeMap::new();

    for fg in field_groups {
        let group_id = format!("{owner_kind_str}:{owner_id_str}:{}", fg.local_key);
        let norm = fg
            .validate_and_normalize_full(FieldScopeKind::ExtensionGroupField)
            .map_err(map_input_validation_error)?;
        let targets = extract_materialized_targets(&norm.location)
            .map_err(|e| map_transition_error(e, incompat_as_conflict))?;
        all_targets_by_domain
            .entry(norm.domain)
            .or_default()
            .extend(targets.iter().cloned());
        normalized_groups.push((group_id, fg.clone(), norm, targets));
    }

    // Include existing targets of owned groups when verifying domain transitions
    let existing_target_rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT g.domain, t.target_kind, t.target_key
           FROM content_field_group_targets t
           JOIN content_field_groups g ON g.group_id = t.group_id
          WHERE g.owner_kind = ? AND g.owner_id = ?",
    )
    .bind(owner_kind_str)
    .bind(owner_id_str)
    .fetch_all(&mut *conn)
    .await?;
    for (dom_str, t_kind, t_key) in existing_target_rows {
        let dom = parse_domain(&dom_str)?;
        all_targets_by_domain
            .entry(dom)
            .or_default()
            .push((t_kind, t_key));
    }

    let mut snapshots = TargetDomainSnapshots::default();
    for (dom, dom_targets) in &all_targets_by_domain {
        let sub = collect_target_snapshots_in_tx(conn, *dom, dom_targets, true).await?;
        snapshots.content.extend(sub.content);
        snapshots.term.extend(sub.term);
        snapshots.options_pages.extend(sub.options_pages);
    }

    let mut declared_group_ids: HashSet<String> = HashSet::new();
    for (group_id, fg, norm, targets) in &normalized_groups {
        let existing_opt: Option<FieldGroupRecord> = load_field_group_in_tx(conn, group_id).await?;
        let location_json = serde_json::to_string(&norm.location)?;
        let presentation_json = serde_json::to_string(&norm.presentation)?;
        let domain_str = domain_to_db(norm.domain);

        if let Some(existing_group) = existing_opt {
            if existing_group.domain != norm.domain {
                return Err(ApiError::conflict(format!(
                    "incompatible_schema_change: Không thể đổi domain của nhóm trường '{group_id}' khi tái khai báo gói."
                )));
            }
            let merged_fields =
                merge_package_redeclared_fields(&existing_group.fields, &norm.fields);
            let fields_json = serde_json::to_string(&merged_fields)?;
            sqlx::query(
                "UPDATE content_field_groups
                 SET title = ?, description = ?, fields_json = ?,
                     domain = ?, location_rules_json = ?, presentation_json = ?, active = ?,
                     default_region = ?, default_order = ?, collapsible = ?, status = 'active',
                     version = version + 1,
                     updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 WHERE group_id = ?",
            )
            .bind(&fg.title)
            .bind(&fg.description)
            .bind(&fields_json)
            .bind(domain_str)
            .bind(&location_json)
            .bind(&presentation_json)
            .bind(i64::from(norm.active))
            .bind(norm.presentation.region.as_str())
            .bind(norm.presentation.order)
            .bind(i64::from(norm.presentation.collapsible))
            .bind(group_id)
            .execute(&mut *conn)
            .await?;

            upsert_group_targets_in_tx(
                conn,
                group_id,
                &existing_group.targets,
                targets,
                &existing_group.fields,
                false,
            )
            .await?;
        } else {
            let fields_json = serde_json::to_string(&norm.fields)?;
            sqlx::query(
                "INSERT INTO content_field_groups
                 (group_id, owner_kind, owner_id, local_key, title, description, fields_json,
                  domain, location_rules_json, presentation_json, active,
                  default_region, default_order, collapsible, status, version)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', 1)",
            )
            .bind(group_id)
            .bind(owner_kind_str)
            .bind(owner_id_str)
            .bind(&fg.local_key)
            .bind(&fg.title)
            .bind(&fg.description)
            .bind(&fields_json)
            .bind(domain_str)
            .bind(&location_json)
            .bind(&presentation_json)
            .bind(i64::from(norm.active))
            .bind(norm.presentation.region.as_str())
            .bind(norm.presentation.order)
            .bind(i64::from(norm.presentation.collapsible))
            .execute(&mut *conn)
            .await?;

            upsert_group_targets_in_tx(conn, group_id, &[], targets, &[], false).await?;
        }
        declared_group_ids.insert(group_id.clone());
    }

    let owned_group_ids: Vec<String> = sqlx::query_scalar(
        "SELECT group_id FROM content_field_groups WHERE owner_kind = ? AND owner_id = ?",
    )
    .bind(owner_kind_str)
    .bind(owner_id_str)
    .fetch_all(&mut *conn)
    .await?;
    for gid in owned_group_ids {
        if !declared_group_ids.contains(&gid) {
            sqlx::query(
                "UPDATE content_field_groups
                 SET status = 'withdrawn', version = version + 1,
                     updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 WHERE group_id = ?",
            )
            .bind(&gid)
            .execute(&mut *conn)
            .await?;
            sqlx::query(
                "UPDATE content_field_group_targets
                 SET association_state = 'withdrawn',
                     updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 WHERE group_id = ?",
            )
            .bind(&gid)
            .execute(&mut *conn)
            .await?;
        }
    }

    verify_snapshots_after_write_in_tx(conn, &snapshots, incompat_as_conflict).await?;
    if let Some(content_targets) = all_targets_by_domain.get(&FieldGroupDomain::Content) {
        sync_search_for_field_group_targets_in_tx(conn, FieldGroupDomain::Content, content_targets)
            .await?;
    }
    Ok(())
}
