use super::*;

pub async fn load_term_ref_hydration_map_in_conn(
    conn: &mut SqliteConnection,
    term_ids: &BTreeSet<i64>,
    is_preview: bool,
    _active_locale: Option<&str>,
) -> ApiResult<BTreeMap<i64, HydratedTermRefRow>> {
    let ids: Vec<i64> = term_ids
        .iter()
        .copied()
        .filter(|id| *id > 0)
        .take(MAX_HYDRATE_TERM_IDS)
        .collect();
    let mut out = BTreeMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    let preview_flag: i64 = i64::from(is_preview);
    for chunk in ids.chunks(HYDRATE_SQL_CHUNK) {
        let raw_json = serde_json::to_string(chunk)?;
        let rows: Vec<(i64, String, String, String, Option<String>, Option<String>)> =
            sqlx::query_as(
                "SELECT tm.id, tm.kind, tm.name, tm.slug, c.path, tm.featured_image_media_id
                   FROM taxonomy_terms tm
                   JOIN taxonomy_definitions d ON d.key = tm.kind
                   JOIN site_locales sl ON sl.code = tm.locale AND sl.deleted_at IS NULL
                   LEFT JOIN permalink_route_claims c
                     ON c.target_kind = 'term'
                    AND c.target_id = CAST(tm.id AS TEXT)
                    AND c.locale = tm.locale
                  WHERE tm.id IN (SELECT value FROM json_each(?1))
                    AND d.provider_status = 'active'
                    AND d.declaration_state = 'declared'
                    AND (?2 = 1 OR (
                        d.public = 1
                        AND sl.enabled = 1
                        AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                    ))",
            )
            .bind(raw_json)
            .bind(preview_flag)
            .fetch_all(&mut *conn)
            .await?;
        for (id, taxonomy_key, name, slug, claim_path, featured_image_media_id) in rows {
            out.insert(
                id,
                HydratedTermRefRow {
                    id,
                    taxonomy_key,
                    name,
                    slug,
                    url: claim_path.unwrap_or_default(),
                    featured_image_media_id: featured_image_media_id
                        .filter(|mid| valid_sha256_media_id(mid)),
                },
            );
        }
    }
    Ok(out)
}

pub(in crate::storage) async fn count_active_media_with_kind_in_conn(
    conn: &mut SqliteConnection,
    ids: &BTreeSet<String>,
    required_kind: Option<&str>,
) -> ApiResult<usize> {
    if ids.is_empty() {
        return Ok(0);
    }
    let vec_ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    let mut found = 0usize;
    for chunk in vec_ids.chunks(HYDRATE_SQL_CHUNK) {
        let raw_json = serde_json::to_string(chunk)?;
        let cnt: i64 = match required_kind {
            Some(kind) => {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM media_assets
                      WHERE lifecycle_state = 'active'
                        AND kind = ?1
                        AND id IN (SELECT value FROM json_each(?2))",
                )
                .bind(kind)
                .bind(raw_json)
                .fetch_one(&mut *conn)
                .await?
            }
            None => {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM media_assets
                      WHERE lifecycle_state = 'active'
                        AND id IN (SELECT value FROM json_each(?1))",
                )
                .bind(raw_json)
                .fetch_one(&mut *conn)
                .await?
            }
        };
        found += cnt.max(0) as usize;
    }
    Ok(found)
}

pub(in crate::storage) fn collect_new_relational_checks(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    previous_values: Option<&BTreeMap<String, Value>>,
    root_specs: &[FieldSpec],
    root_values: &BTreeMap<String, Value>,
    entry_checks: &mut Vec<PendingEntryCheck>,
    term_checks: &mut Vec<PendingTermCheck>,
) {
    for spec in specs {
        if spec.is_ui_only() {
            continue;
        }
        let Some(cur_val) = values.get(&spec.key) else {
            continue;
        };
        let prev_val = previous_values.and_then(|p| p.get(&spec.key));
        match spec.field_type {
            FieldType::EntryRef => {
                let mut cur_ids = extract_ids_from_value(cur_val);
                if let Some(pv) = prev_val {
                    for pid in extract_ids_from_value(pv) {
                        cur_ids.remove(&pid);
                    }
                }
                if !cur_ids.is_empty() {
                    entry_checks.push(PendingEntryCheck {
                        target_types: spec.target_types.clone(),
                        ids: cur_ids,
                    });
                }
            }
            FieldType::TermRef => {
                let mut cur_ids = extract_ids_from_value(cur_val);
                if let Some(pv) = prev_val {
                    for pid in extract_ids_from_value(pv) {
                        cur_ids.remove(&pid);
                    }
                }
                if !cur_ids.is_empty() {
                    let resolved_tax = if let Some(ref field_ref) = spec.taxonomy_from_field {
                        values
                            .get(field_ref)
                            .or_else(|| root_values.get(field_ref))
                            .or_else(|| {
                                specs
                                    .iter()
                                    .chain(root_specs.iter())
                                    .find(|s| &s.key == field_ref)
                                    .and_then(|s| s.default.as_ref())
                            })
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                    } else {
                        spec.taxonomy.clone()
                    };
                    term_checks.push(PendingTermCheck {
                        taxonomy: resolved_tax,
                        ids: cur_ids,
                    });
                }
            }
            FieldType::Group => {
                if let Some(cur_obj) = cur_val.as_object() {
                    let cur_map: BTreeMap<String, Value> = cur_obj
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    let prev_map: Option<BTreeMap<String, Value>> = prev_val
                        .and_then(Value::as_object)
                        .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
                    collect_new_relational_checks(
                        &spec.fields,
                        &cur_map,
                        prev_map.as_ref(),
                        root_specs,
                        root_values,
                        entry_checks,
                        term_checks,
                    );
                }
            }
            FieldType::Repeater => {
                let mut prev_entry_pool = BTreeSet::new();
                let mut prev_term_pool = BTreeSet::new();
                if let Some(pv) = prev_val {
                    let mut dummy_prev = BTreeMap::new();
                    dummy_prev.insert(spec.key.clone(), pv.clone());
                    let rel = opendash_core::field_schema::collect_relational_refs(
                        std::slice::from_ref(spec),
                        &dummy_prev,
                    );
                    prev_entry_pool = rel.entry_ids;
                    prev_term_pool = rel.term_ids;
                }
                if let Some(items) = cur_val.as_array() {
                    for item in items {
                        if let Some(obj) = item.as_object() {
                            let item_map: BTreeMap<String, Value> =
                                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                            let before_entries = entry_checks.len();
                            let before_terms = term_checks.len();
                            collect_new_relational_checks(
                                &spec.fields,
                                &item_map,
                                None,
                                root_specs,
                                root_values,
                                entry_checks,
                                term_checks,
                            );
                            for check in &mut entry_checks[before_entries..] {
                                check.ids.retain(|id| !prev_entry_pool.contains(id));
                            }
                            for check in &mut term_checks[before_terms..] {
                                check.ids.retain(|id| !prev_term_pool.contains(id));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

pub async fn validate_relational_refs_in_conn(
    conn: &mut SqliteConnection,
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    previous_values: Option<&BTreeMap<String, Value>>,
    required_locale: Option<&str>,
) -> ApiResult<()> {
    let mut entry_checks = Vec::new();
    let mut term_checks = Vec::new();
    collect_new_relational_checks(
        specs,
        values,
        previous_values,
        specs,
        values,
        &mut entry_checks,
        &mut term_checks,
    );
    entry_checks.retain(|c| !c.ids.is_empty());
    term_checks.retain(|c| !c.ids.is_empty());

    let all_entry_ids: BTreeSet<i64> = entry_checks
        .iter()
        .flat_map(|c| c.ids.iter().copied())
        .collect();
    if !all_entry_ids.is_empty() {
        let mut valid_entries: BTreeMap<i64, (String, String)> = BTreeMap::new();
        let ids_vec: Vec<i64> = all_entry_ids.into_iter().collect();
        for chunk in ids_vec.chunks(HYDRATE_SQL_CHUNK) {
            let raw_json = serde_json::to_string(chunk)?;
            let rows: Vec<(i64, String, String)> = sqlx::query_as(
                "SELECT e.id, e.content_type_key, e.locale
                   FROM content_entries e
                   JOIN content_types ct ON ct.key = e.content_type_key
                   JOIN site_locales sl ON sl.code = e.locale AND sl.deleted_at IS NULL
                  WHERE e.id IN (SELECT value FROM json_each(?1))
                    AND e.trashed_at IS NULL
                    AND ct.provider_status = 'active'
                    AND e.content_type_key NOT IN ('pattern', 'section')",
            )
            .bind(raw_json)
            .fetch_all(&mut *conn)
            .await?;
            for (eid, tk, loc) in rows {
                valid_entries.insert(eid, (tk, loc));
            }
        }
        for check in &entry_checks {
            let is_wildcard = check.target_types.len() == 1 && check.target_types[0] == "*";
            for id in &check.ids {
                let Some((tk, loc)) = valid_entries.get(id) else {
                    return Err(ApiError::bad_request("ID nội dung không hợp lệ."));
                };
                if !is_wildcard && !check.target_types.iter().any(|t| t == tk) {
                    return Err(ApiError::bad_request("ID nội dung không hợp lệ."));
                }
                if let Some(req_loc) = required_locale
                    && loc != req_loc
                {
                    return Err(ApiError::bad_request("ID nội dung không hợp lệ."));
                }
            }
        }
    }

    let all_term_ids: BTreeSet<i64> = term_checks
        .iter()
        .flat_map(|c| c.ids.iter().copied())
        .collect();
    if !all_term_ids.is_empty() {
        for check in &term_checks {
            if check.taxonomy.is_none() {
                return Err(ApiError::bad_request("term_id không hợp lệ."));
            }
        }
        let mut valid_terms: BTreeMap<i64, (String, String)> = BTreeMap::new();
        let ids_vec: Vec<i64> = all_term_ids.into_iter().collect();
        for chunk in ids_vec.chunks(HYDRATE_SQL_CHUNK) {
            let raw_json = serde_json::to_string(chunk)?;
            let rows: Vec<(i64, String, String)> = sqlx::query_as(
                "SELECT tm.id, tm.kind, tm.locale
                   FROM taxonomy_terms tm
                   JOIN taxonomy_definitions d ON d.key = tm.kind
                   JOIN site_locales sl ON sl.code = tm.locale AND sl.deleted_at IS NULL
                  WHERE tm.id IN (SELECT value FROM json_each(?1))
                    AND d.provider_status = 'active'
                    AND d.declaration_state = 'declared'",
            )
            .bind(raw_json)
            .fetch_all(&mut *conn)
            .await?;
            for (tid, kind, loc) in rows {
                valid_terms.insert(tid, (kind, loc));
            }
        }
        for check in &term_checks {
            let Some(ref expected_tax) = check.taxonomy else {
                return Err(ApiError::bad_request("term_id không hợp lệ."));
            };
            for id in &check.ids {
                let Some((kind, loc)) = valid_terms.get(id) else {
                    return Err(ApiError::bad_request("term_id không hợp lệ."));
                };
                if kind != expected_tax {
                    return Err(ApiError::bad_request("term_id không hợp lệ."));
                }
                if let Some(req_loc) = required_locale
                    && loc != req_loc
                {
                    return Err(ApiError::bad_request("term_id không hợp lệ."));
                }
            }
        }
    }
    Ok(())
}

pub async fn load_active_v3_manifest_in_conn(
    conn: &mut SqliteConnection,
) -> ApiResult<Option<ThemeV3Manifest>> {
    if let Some(dev) = DevOverlayConfig::global()
        && dev.dev_theme_root().is_some()
        && let Ok(Some(snap)) = dev.load_dev_theme()
    {
        return Ok(Some(snap.manifest));
    }

    let manifest_json: Option<String> = sqlx::query_scalar(
        "SELECT r.manifest_json
           FROM theme_active_selection a
           JOIN theme_revisions r ON r.revision_id = a.revision_id
          WHERE a.id = 1 AND r.source_format = 'v3'",
    )
    .fetch_optional(&mut *conn)
    .await?;
    let Some(raw) = manifest_json else {
        return Ok(None);
    };
    let Ok(manifest) = serde_json::from_str::<ThemeV3Manifest>(&raw) else {
        return Ok(None);
    };
    Ok(Some(manifest))
}
