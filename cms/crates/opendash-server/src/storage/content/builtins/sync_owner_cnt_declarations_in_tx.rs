use super::*;

pub(in crate::storage) async fn sync_owner_cnt_declarations_in_tx(
    conn: &mut SqliteConnection,
    owner_kind_str: &str,
    owner_id_str: &str,
    declarations: &opendash_core::content::CntDeclarations,
    incompat_as_conflict: bool,
) -> ApiResult<()> {
    declarations
        .validate()
        .map_err(map_input_validation_error)?;

    crate::storage::taxonomy::sync_provider_definitions_in_tx(
        conn,
        owner_kind_str,
        owner_id_str,
        &declarations.taxonomy_definitions,
    )
    .await?;

    let has_fg_tables = table_exists_in_tx(conn, "content_field_groups").await?
        && table_exists_in_tx(conn, "content_field_group_targets").await?;

    // Snapshot declared/owned CPTs before mutating so verify_effective_schema_transition
    // can compare old_effective vs new_effective symmetrically.
    let mut candidate_types: BTreeSet<String> = BTreeSet::new();
    for ct in &declarations.content_types {
        candidate_types.insert(ct.key.clone());
    }
    let existing_owned_ct_keys: Vec<String> =
        sqlx::query_scalar("SELECT key FROM content_types WHERE owner_kind = ? AND owner_id = ?")
            .bind(owner_kind_str)
            .bind(owner_id_str)
            .fetch_all(&mut *conn)
            .await?;
    let existing_owned_ct_set: HashSet<String> = existing_owned_ct_keys.iter().cloned().collect();
    for k in &existing_owned_ct_keys {
        candidate_types.insert(k.clone());
    }

    let mut old_snapshots: HashMap<String, (EffectiveContentSchema, bool)> = HashMap::new();
    for t in &candidate_types {
        if let Some(old_eff) = load_effective_content_schema_in_tx(conn, t).await? {
            let has_entries: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM content_entries WHERE content_type_key = ?)",
            )
            .bind(t)
            .fetch_one(&mut *conn)
            .await?;
            old_snapshots.insert(t.clone(), (old_eff, has_entries));
        }
    }

    let mut declared_ct_keys: HashSet<String> = HashSet::new();
    let mut declared_eligibility: HashSet<(String, String)> = HashSet::new();
    let mut changed_cpt_types: BTreeSet<String> = BTreeSet::new();

    for ct in &declarations.content_types {
        if builtin_definition(&ct.key).is_some() {
            return Err(ApiError::conflict("post/page là khóa Core được bảo vệ."));
        }
        ensure_non_reserved_cpt_key_in_tx(&mut *conn, &ct.key).await?;
        let existing: Option<(String, String, Option<String>, String, String)> = sqlx::query_as(
            "SELECT definition_json, owner_kind, owner_id, provider_status, declaration_state
             FROM content_types WHERE key = ?",
        )
        .bind(&ct.key)
        .fetch_optional(&mut *conn)
        .await?;

        match existing {
            Some((raw_def, okind, oid, provider_status, declaration_state)) => {
                if okind != owner_kind_str || oid.as_deref() != Some(owner_id_str) {
                    return Err(ApiError::conflict(
                        "Khóa loại nội dung thuộc chủ sở hữu khác (409).",
                    ));
                }
                let existing_def: ContentTypeDefinition = serde_json::from_str(&raw_def)
                    .map_err(|_| ApiError::internal("Dữ liệu loại nội dung không hợp lệ."))?;
                let mut incoming_settings = ct
                    .settings
                    .clone()
                    .unwrap_or_else(|| existing_def.settings.clone());
                incoming_settings
                    .validate_and_normalize()
                    .map_err(map_input_validation_error)?;
                if existing_def.settings.hierarchical && !incoming_settings.hierarchical {
                    let has_children: bool = sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM content_entries WHERE content_type_key = ? AND parent_id IS NOT NULL)",
                    )
                    .bind(&ct.key)
                    .fetch_one(&mut *conn)
                    .await?;
                    if has_children {
                        return Err(ApiError::conflict(
                            "incompatible_schema_change: Không thể tắt phân cấp khi đã có bài viết con (parent_id).",
                        ));
                    }
                }
                if existing_def.settings.publication == ContentPublication::DraftPublish
                    && incoming_settings.publication == ContentPublication::Immediate
                {
                    let drafts: bool = sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM content_entries WHERE content_type_key = ? AND publication_status = 'draft')",
                    )
                    .bind(&ct.key)
                    .fetch_one(&mut *conn)
                    .await?;
                    if drafts {
                        return Err(ApiError::conflict(
                            "Không thể chuyển sang lưu ngay khi loại nội dung còn bản nháp.",
                        ));
                    }
                }
                let incoming = ContentTypeDefinition {
                    key: ct.key.clone(),
                    label: ct.label.clone(),
                    singular_label: ct.singular_label.clone(),
                    description: ct.description.clone(),
                    settings: incoming_settings,
                };
                incoming.validate().map_err(map_input_validation_error)?;
                let incoming_json = serde_json::to_string(&incoming)?;
                if existing_def != incoming
                    || provider_status != "active"
                    || declaration_state != "declared"
                {
                    changed_cpt_types.insert(ct.key.clone());
                }

                sqlx::query(
                    "UPDATE content_types
                     SET definition_json = ?, provider_status = 'active', declaration_state = 'declared', updated_at = datetime('now')
                     WHERE key = ?",
                )
                .bind(&incoming_json)
                .bind(&ct.key)
                .execute(&mut *conn)
                .await?;
            }
            None => {
                let mut settings = ct.settings.clone().unwrap_or_default();
                settings
                    .validate_and_normalize()
                    .map_err(map_input_validation_error)?;
                let incoming = ContentTypeDefinition {
                    key: ct.key.clone(),
                    label: ct.label.clone(),
                    singular_label: ct.singular_label.clone(),
                    description: ct.description.clone(),
                    settings,
                };
                incoming.validate().map_err(map_input_validation_error)?;
                let incoming_json = serde_json::to_string(&incoming)?;

                sqlx::query(
                    "INSERT INTO content_types (key, definition_json, owner_kind, owner_id, provider_status, declaration_state)
                     VALUES (?, ?, ?, ?, 'active', 'declared')",
                )
                .bind(&ct.key)
                .bind(&incoming_json)
                .bind(owner_kind_str)
                .bind(owner_id_str)
                .execute(&mut *conn)
                .await?;
            }
        }

        declared_ct_keys.insert(ct.key.clone());
        for tax in &ct.taxonomies {
            declared_eligibility.insert((ct.key.clone(), tax.clone()));
        }
    }

    // Withdraw omitted CPT declarations previously owned by this owner (never deleting schema/entries)
    for key in existing_owned_ct_keys {
        if !declared_ct_keys.contains(&key) {
            let changed = sqlx::query(
                "UPDATE content_types
                 SET declaration_state = 'withdrawn', provider_status = 'active', updated_at = datetime('now')
                 WHERE key = ? AND (declaration_state != 'withdrawn' OR provider_status != 'active')",
            )
            .bind(&key)
            .execute(&mut *conn)
            .await?
            .rows_affected();
            if changed > 0 {
                changed_cpt_types.insert(key);
            }
        }
    }

    // Merge explicit taxonomy_eligibility declarations and persist
    for elig in &declarations.taxonomy_eligibility {
        for tax in &elig.taxonomies {
            declared_eligibility.insert((elig.content_type.clone(), tax.clone()));
        }
    }

    for (ct_key, tax_kind) in &declared_eligibility {
        let taxonomy_owner: Option<(String, Option<String>, String, String)> = sqlx::query_as(
            "SELECT owner_kind,owner_id,provider_status,declaration_state
             FROM taxonomy_definitions WHERE key=?",
        )
        .bind(tax_kind)
        .fetch_optional(&mut *conn)
        .await?;
        match taxonomy_owner {
            Some((kind, id, provider_status, state))
                if (kind == "core"
                    || kind == "user"
                    || (kind == owner_kind_str && id.as_deref() == Some(owner_id_str)))
                    && provider_status == "active"
                    && state == "declared" => {}
            Some(_) => {
                return Err(ApiError::conflict(
                    "Không thể liên kết taxonomy thuộc provider khác hoặc không hoạt động.",
                ));
            }
            None => {
                return Err(ApiError::bad_request(
                    "Taxonomy eligibility chưa được định nghĩa.",
                ));
            }
        }
        if builtin_definition(ct_key).is_some() {
            return Err(ApiError::conflict(
                "Không thể chiếm quyền taxonomy eligibility của Core post/page.",
            ));
        }
        let ct_owner: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT owner_kind, owner_id FROM content_types WHERE key = ?")
                .bind(ct_key)
                .fetch_optional(&mut *conn)
                .await?;
        match ct_owner {
            Some((okind, oid))
                if okind == owner_kind_str && oid.as_deref() == Some(owner_id_str) => {}
            Some(_) => {
                return Err(ApiError::conflict(
                    "Không thể khai báo taxonomy eligibility cho loại nội dung thuộc chủ sở hữu khác (409).",
                ));
            }
            None => {
                return Err(ApiError::bad_request(
                    "Loại nội dung trong taxonomy_eligibility chưa được khai báo.",
                ));
            }
        }

        let existing_elig: Option<(String, Option<String>, String, String)> = sqlx::query_as(
            "SELECT owner_kind, owner_id, provider_status, declaration_state
             FROM content_type_taxonomy_eligibility
             WHERE content_type_key = ? AND taxonomy_kind = ?",
        )
        .bind(ct_key)
        .bind(tax_kind)
        .fetch_optional(&mut *conn)
        .await?;

        match existing_elig {
            Some((okind, oid, provider_status, declaration_state)) => {
                if okind != owner_kind_str || oid.as_deref() != Some(owner_id_str) {
                    return Err(ApiError::conflict(
                        "Taxonomy eligibility thuộc chủ sở hữu khác (409).",
                    ));
                }
                if provider_status != "active" || declaration_state != "declared" {
                    sqlx::query(
                        "UPDATE content_type_taxonomy_eligibility
                         SET provider_status = 'active', declaration_state = 'declared', updated_at = datetime('now')
                         WHERE content_type_key = ? AND taxonomy_kind = ?",
                    )
                    .bind(ct_key)
                    .bind(tax_kind)
                    .execute(&mut *conn)
                    .await?;
                    changed_cpt_types.insert(ct_key.clone());
                }
            }
            None => {
                sqlx::query(
                    "INSERT INTO content_type_taxonomy_eligibility
                     (content_type_key, taxonomy_kind, owner_kind, owner_id, provider_status, declaration_state)
                     VALUES (?, ?, ?, ?, 'active', 'declared')",
                )
                .bind(ct_key)
                .bind(tax_kind)
                .bind(owner_kind_str)
                .bind(owner_id_str)
                .execute(&mut *conn)
                .await?;
                changed_cpt_types.insert(ct_key.clone());
            }
        }
    }

    // Withdraw omitted taxonomy_eligibility rows owned by this owner (retaining entry_terms)
    let owned_eligs: Vec<(String, String)> = sqlx::query_as(
        "SELECT content_type_key, taxonomy_kind
         FROM content_type_taxonomy_eligibility
         WHERE owner_kind = ? AND owner_id = ?",
    )
    .bind(owner_kind_str)
    .bind(owner_id_str)
    .fetch_all(&mut *conn)
    .await?;

    for (ct_key, tax_kind) in owned_eligs {
        if !declared_eligibility.contains(&(ct_key.clone(), tax_kind.clone())) {
            let changed = sqlx::query(
                "UPDATE content_type_taxonomy_eligibility
                 SET declaration_state = 'withdrawn', provider_status = 'active', updated_at = datetime('now')
                 WHERE content_type_key = ? AND taxonomy_kind = ?
                   AND (declaration_state != 'withdrawn' OR provider_status != 'active')",
            )
            .bind(&ct_key)
            .bind(&tax_kind)
            .execute(&mut *conn)
            .await?
            .rows_affected();
            if changed > 0 {
                changed_cpt_types.insert(ct_key);
            }
        }
    }

    // This function runs inside the install/activation writer transaction.
    // Changing provider defaults or CPT schema/status without advancing the
    // CAS allows an Admin to save a stale taxonomy override with a still-valid
    // If-Match. Bump once per existing CPT affected by this provider install,
    // covering schema, additions, withdrawals and reactivations atomically.
    for ct_key in changed_cpt_types {
        if existing_owned_ct_set.contains(&ct_key) {
            sqlx::query(
                "UPDATE content_types SET version = version + 1,
                 updated_at = datetime('now') WHERE key = ?",
            )
            .bind(&ct_key)
            .execute(&mut *conn)
            .await?;
        }
    }

    // Enforce Rule COMPAT-1..COMPAT-4 across every declared/owned CPT before syncing field groups
    for t in &candidate_types {
        if let Some(new_eff) = load_effective_content_schema_in_tx(conn, t).await? {
            let (old_eff_opt, has_entries) = match old_snapshots.get(t) {
                Some((old_eff, has)) => (Some(old_eff), *has),
                None => (None, false),
            };
            verify_effective_schema_transition(old_eff_opt, &new_eff, has_entries)
                .map_err(|e| map_transition_error(e, incompat_as_conflict))?;
        }
    }

    // Sync Custom Options Pages before field groups so `options_page == custom:<key>` targets exist
    crate::storage::options_pages::sync_provider_options_pages_in_tx(
        conn,
        owner_kind_str,
        owner_id_str,
        &declarations.options_pages,
    )
    .await?;

    // Sync multi-domain package field_groups (Content, Term, OptionsPage) + COMPAT-1..4
    if has_fg_tables {
        sync_field_groups::sync_owner_field_groups_in_tx(
            conn,
            owner_kind_str,
            owner_id_str,
            &declarations.field_groups,
            incompat_as_conflict,
        )
        .await?;
    }

    let rewrites: Vec<crate::storage::permalinks::ProviderRewriteDeclaration> = declarations
        .content_types
        .iter()
        .map(
            |ct| crate::storage::permalinks::ProviderRewriteDeclaration {
                resource_kind: "content_type",
                resource_key: ct.key.clone(),
                rewrite_base: ct.rewrite_base.clone(),
            },
        )
        .chain(declarations.taxonomy_definitions.iter().map(|tax| {
            crate::storage::permalinks::ProviderRewriteDeclaration {
                resource_kind: "taxonomy",
                resource_key: tax.key.clone(),
                rewrite_base: tax.rewrite_base.clone(),
            }
        }))
        .collect();
    crate::storage::permalinks::sync_provider_rewrite_defaults_in_tx(
        conn,
        owner_kind_str,
        owner_id_str,
        &rewrites,
    )
    .await?;
    for t in &candidate_types {
        crate::storage::search::sync_entries_for_type_in_conn(conn, t).await?;
    }

    Ok(())
}
