use super::*;

pub async fn execute_sample_import_with_cutpoint(
    db: &SqlitePool,
    themes_root: &std::path::Path,
    uploads_root: &std::path::Path,
    theme_id: &str,
    input: &ExecuteImportInput,
    cutpoint: ImportCutpoint,
) -> ApiResult<ImportExecutionOutcome> {
    let dry_input = DryRunImportInput {
        groups: input.groups.clone(),
        revision_id: input.revision_id.clone(),
    };
    let plan = plan_sample_import(db, theme_id, &dry_input).await?;

    let run_id = input.run_id.clone().unwrap_or_else(|| {
        format!(
            "run-{}-{}",
            &plan.plan_digest[..12],
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        )
    });

    // Idempotent retry check for same run_id
    let existing_run: Option<(String, String, Option<String>)> =
        sqlx::query_as("SELECT state, plan_digest, outcome_json FROM import_runs WHERE run_id = ?")
            .bind(&run_id)
            .fetch_optional(db)
            .await?;
    if let Some((state, old_digest, outcome_json)) = existing_run {
        if state == "committed" && old_digest == input.plan_digest {
            if let Some(raw) = outcome_json {
                let outcome: ImportExecutionOutcome = serde_json::from_str(&raw)?;
                return Ok(outcome);
            }
        }
        return Err(ApiError::conflict(
            "Import run_id đã tồn tại ở trạng thái chưa hoàn tất hoặc khác kế hoạch.",
        ));
    }

    if plan.plan_digest != input.plan_digest || plan.expected_versions != input.expected_versions {
        return Err(ApiError::precondition_failed(
            "Trạng thái site đã thay đổi sau dry-run (412). Vui lòng chạy lại dry-run.",
        ));
    }
    if plan
        .items
        .iter()
        .any(|i| i.action == PlanAction::MissingDependency)
    {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Kế hoạch vẫn còn dependency chưa được chọn.".into(),
        ));
    }
    if plan.requires_public_ack && !input.acknowledge_public_changes {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Cần xác nhận thay đổi hiển thị công khai (menu/options) trước khi thực thi.".into(),
        ));
    }

    let rev = find_revision_by_id(db, &plan.revision_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Revision theme chưa được cài đặt."))?;
    if rev.source_format != ThemeSourceFormat::V3 {
        return Err(ApiError::not_found("Revision theme v3 không tồn tại."));
    }
    let option_schema = settings::parse_theme_options_schema(&rev.manifest_json)
        .map_err(ApiError::unprocessable)?;
    let manifest: ThemeV3Manifest = serde_json::from_str(&rev.manifest_json)?;
    manifest.validate().map_err(ApiError::bad_request)?;
    let sample: ThemeSampleManifest = serde_json::from_value(
        manifest
            .sample
            .ok_or_else(|| ApiError::not_found("Theme không có dữ liệu mẫu."))?,
    )?;
    validate_sample_manifest(&sample, &rev.file_digests, &option_schema)?;

    // Phase 1: staged
    {
        let mut conn = db.acquire().await?;
        sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
        sqlx::query(
            "INSERT INTO import_runs (run_id, theme_id, sample_key, revision_id, plan_digest, state, media_manifest_json)
             VALUES (?, ?, ?, ?, ?, 'staged', '[]')",
        )
        .bind(&run_id)
        .bind(theme_id)
        .bind(&plan.sample_key)
        .bind(&plan.revision_id)
        .bind(&plan.plan_digest)
        .execute(&mut *conn)
        .await?;
        sqlx::query("COMMIT").execute(&mut *conn).await?;
        crate::cache::SiteCacheEpoch::global().bump();
    }

    if cutpoint == ImportCutpoint::AfterPhase1Staged {
        return Err(ApiError::internal(
            "Simulated crash after Phase 1 (staged).",
        ));
    }

    // Phase 2: files_verified
    let mut media_entries = Vec::new();
    let mut media_id_by_object = HashMap::<String, String>::new();
    let rev_dir = themes_root.join("revisions").join(&rev.revision_digest);

    for item in &plan.items {
        if item.resource_kind == SampleGroup::Media && item.action == PlanAction::Create {
            let m = sample
                .media
                .iter()
                .find(|x| x.object_key == item.object_key)
                .ok_or_else(|| ApiError::internal("Không tìm thấy media item trong sample."))?;
            let safe_rel = validate_theme_relative_path(&m.path).map_err(ApiError::bad_request)?;
            let mut full_path = rev_dir.clone();
            for seg in safe_rel.split('/') {
                full_path.push(seg);
            }
            let bytes = tokio::fs::read(&full_path)
                .await
                .map_err(|_| ApiError::internal("Không đọc được file media từ revision theme."))?;
            let mime = infer_media_mime(&safe_rel, m.mime_type.as_deref())?;
            let decoded = crate::media::decode_image(bytes, mime)?;
            let entry = stage_and_dedup_sample_media(
                db,
                uploads_root,
                &run_id,
                &m.object_key,
                &safe_rel,
                decoded,
            )
            .await?;
            media_id_by_object.insert(m.object_key.clone(), entry.media_id.clone());
            media_entries.push(entry);
        }
    }

    let _ = tokio::fs::remove_dir_all(uploads_root.join(".staging").join(&run_id)).await;
    let media_manifest_json = serde_json::to_string(&media_entries)?;
    sqlx::query(
        "UPDATE import_runs
         SET state = 'files_verified', media_manifest_json = ?, updated_at = datetime('now')
         WHERE run_id = ?",
    )
    .bind(&media_manifest_json)
    .bind(&run_id)
    .execute(db)
    .await?;

    if cutpoint == ImportCutpoint::AfterPhase2FilesVerified {
        return Err(ApiError::internal(
            "Simulated crash after Phase 2 (files_verified).",
        ));
    }

    // Phase 3: committed (single BEGIN IMMEDIATE transaction)
    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;

    let tx_res: ApiResult<ImportExecutionOutcome> = async {
        let cur_sel_ver: i64 =
            sqlx::query_scalar("SELECT version FROM theme_active_selection WHERE id = 1")
                .fetch_one(&mut *conn)
                .await?;
        let cur_opt_ver: i64 = sqlx::query_scalar(
            "SELECT COALESCE((SELECT version FROM theme_option_versions WHERE owner_kind = 'theme' AND owner_id = ?), 0)",
        )
        .bind(theme_id)
        .fetch_one(&mut *conn)
        .await?;
        if cur_sel_ver != input.expected_versions.theme_selection_version
            || cur_opt_ver != input.expected_versions.theme_options_version
        {
            return Err(ApiError::precondition_failed(
                "Trạng thái site đã thay đổi trước khi commit (412).",
            ));
        }

        let default_locale =
            crate::storage::settings::get_default_locale_code_in_conn(&mut conn).await?;
        let mut created = Vec::new();
        let mut skipped = Vec::new();
        let mut page_id_by_object = HashMap::<String, i64>::new();

        for item in &plan.items {
            if item.action == PlanAction::Skip {
                skipped.push(ImportOutcomeItem {
                    resource_kind: item.resource_kind,
                    object_key: item.object_key.clone(),
                    target_id: item.logical_target.clone(),
                    status: "skipped".to_string(),
                    reason: item.reason.clone(),
                });
            }
        }

        persist_imported_media_entries_in_tx(
            &mut conn,
            &media_entries,
            theme_id,
            &plan,
            &mut created,
        )
        .await?;

        // Insert pages (always draft) + provenance
        for item in &plan.items {
            if item.resource_kind == SampleGroup::Pages && item.action == PlanAction::Create {
                let p = sample
                    .pages
                    .iter()
                    .find(|x| x.object_key == item.object_key)
                    .ok_or_else(|| ApiError::internal("Không tìm thấy sample page."))?;
                let values_json = if p.values.is_object() {
                    serde_json::to_string(&p.values)?
                } else {
                    "{}".to_string()
                };
                let group_id = crate::storage::settings::generate_translation_group_id("eg");
                let entry_id: i64 = sqlx::query_scalar(
                    "INSERT INTO content_entries (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, version)
                     VALUES (?, ?, ?, ?, ?, ?, 'draft', 1) RETURNING id",
                )
                .bind(&p.content_type)
                .bind(&default_locale)
                .bind(&group_id)
                .bind(&p.slug)
                .bind(&p.title)
                .bind(&values_json)
                .fetch_one(&mut *conn)
                .await?;

                if let Some(effective) = crate::storage::content::load_effective_content_schema_in_tx(
                    &mut *conn,
                    &p.content_type,
                )
                .await?
                {
                    let values_map: BTreeMap<String, serde_json::Value> = p
                        .values
                        .as_object()
                        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                        .unwrap_or_default();
                    crate::storage::entries::sync_entry_body_refs_in_conn(
                        &mut *conn,
                        entry_id,
                        &effective,
                        &values_map,
                        None,
                        opendash_core::content::FieldRole::Admin,
                    )
                    .await?;
                }

                page_id_by_object.insert(p.object_key.clone(), entry_id);
                let target_hash = sha256_hex(format!("{}:{}:draft", p.slug, p.title).as_bytes());
                sqlx::query(
                    "INSERT INTO import_provenance
                     (theme_id, sample_key, resource_kind, object_key, revision_id, revision_digest, target_id, target_hash)
                     VALUES (?, ?, 'pages', ?, ?, ?, ?, ?)",
                )
                .bind(theme_id)
                .bind(&plan.sample_key)
                .bind(&p.object_key)
                .bind(&plan.revision_id)
                .bind(&plan.revision_digest)
                .bind(entry_id.to_string())
                .bind(&target_hash)
                .execute(&mut *conn)
                .await?;

                created.push(ImportOutcomeItem {
                    resource_kind: SampleGroup::Pages,
                    object_key: p.object_key.clone(),
                    target_id: entry_id.to_string(),
                    status: "created".to_string(),
                    reason: Some("created_as_draft".to_string()),
                });
            }
        }
        crate::storage::permalinks::sync_claims_in_tx(&mut *conn).await?;

        // Insert menus + items + slot assignment + provenance
        for item in &plan.items {
            if item.resource_kind == SampleGroup::Menus && item.action == PlanAction::Create {
                let menu = sample
                    .menus
                    .iter()
                    .find(|x| x.object_key == item.object_key)
                    .ok_or_else(|| ApiError::internal("Không tìm thấy sample menu."))?;
                let menu_id: i64 = sqlx::query_scalar(
                    "INSERT INTO site_menus (menu_key, title, version) VALUES (?, ?, 1) RETURNING id",
                )
                .bind(&menu.menu_key)
                .bind(&menu.title)
                .fetch_one(&mut *conn)
                .await?;

                for (pos, entry) in menu.items.iter().enumerate() {
                    let target_entry_id = if entry.target_kind == "entry" {
                        let key = entry.target_object_key.as_deref().unwrap_or("");
                        page_id_by_object.get(key).copied()
                    } else {
                        None
                    };
                    sqlx::query(
                        "INSERT INTO site_menu_items (menu_id, parent_id, position, label, target_kind, target_entry_id, external_url)
                         VALUES (?, NULL, ?, ?, ?, ?, ?)",
                    )
                    .bind(menu_id)
                    .bind(pos as i64)
                    .bind(&entry.label)
                    .bind(&entry.target_kind)
                    .bind(target_entry_id)
                    .bind(&entry.url)
                    .execute(&mut *conn)
                    .await?;
                }
                if let Some(slot_key) = menu.slot_key.as_deref() {
                    sqlx::query(
                        "INSERT OR IGNORE INTO site_menu_slot_assignments (theme_id, locale, slot_key, menu_id)
                         VALUES (?, ?, ?, ?)",
                    )
                    .bind(theme_id)
                    .bind(&default_locale)
                    .bind(slot_key)
                    .bind(menu_id)
                    .execute(&mut *conn)
                    .await?;
                    crate::storage::menu::bump_slot_version_for_locale_in_tx(
                        &mut conn,
                        theme_id,
                        &default_locale,
                    )
                    .await?;
                }
                let target_hash = sha256_hex(menu.menu_key.as_bytes());
                sqlx::query(
                    "INSERT INTO import_provenance
                     (theme_id, sample_key, resource_kind, object_key, revision_id, revision_digest, target_id, target_hash)
                     VALUES (?, ?, 'menus', ?, ?, ?, ?, ?)",
                )
                .bind(theme_id)
                .bind(&plan.sample_key)
                .bind(&menu.object_key)
                .bind(&plan.revision_id)
                .bind(&plan.revision_digest)
                .bind(menu_id.to_string())
                .bind(&target_hash)
                .execute(&mut *conn)
                .await?;

                created.push(ImportOutcomeItem {
                    resource_kind: SampleGroup::Menus,
                    object_key: menu.object_key.clone(),
                    target_id: menu_id.to_string(),
                    status: "created".to_string(),
                    reason: None,
                });
            }
        }

        // Insert settings (theme options) + provenance
        let mut any_setting_created = false;
        for item in &plan.items {
            if item.resource_kind == SampleGroup::Settings && item.action == PlanAction::Create {
                let s = sample
                    .settings
                    .iter()
                    .find(|x| x.object_key == item.object_key)
                    .ok_or_else(|| ApiError::internal("Không tìm thấy sample setting."))?;
                let val = if let Some(mref) = s.media_ref.as_deref() {
                    let mid = media_id_by_object
                        .get(mref)
                        .ok_or_else(|| ApiError::internal("Thiếu media_id cho sample setting."))?;
                    let resolved = serde_json::Value::String(mid.clone());
                    let spec = option_schema
                        .iter()
                        .find(|spec| spec.key == s.option_key && !spec.is_ui_only())
                        .ok_or_else(|| {
                            ApiError::unprocessable("Sample option key không tồn tại.")
                        })?;
                    let single_map = BTreeMap::from([(s.option_key.clone(), resolved.clone())]);
                    validate_field_values(
                        std::slice::from_ref(spec),
                        FieldContext::Options,
                        &single_map,
                        None,
                    )
                    .map_err(ApiError::unprocessable)?;
                    sqlx::query(
                        "DELETE FROM theme_option_media_refs
                          WHERE owner_kind = 'theme' AND owner_id = ? AND locale = '' AND option_key = ?",
                    )
                    .bind(theme_id)
                    .bind(&s.option_key)
                    .execute(&mut *conn)
                    .await?;
                    sqlx::query(
                        "INSERT INTO theme_option_media_refs (theme_id, owner_kind, owner_id, locale, option_key, media_id)
                         VALUES (?, 'theme', ?, '', ?, ?)",
                    )
                    .bind(theme_id)
                    .bind(theme_id)
                    .bind(&s.option_key)
                    .bind(mid)
                    .execute(&mut *conn)
                    .await?;
                    resolved
                } else {
                    s.value.clone()
                };
                let spec = option_schema
                    .iter()
                    .find(|spec| spec.key == s.option_key && !spec.is_ui_only())
                    .ok_or_else(|| ApiError::unprocessable("Sample option key không tồn tại."))?;
                let single_map = BTreeMap::from([(s.option_key.clone(), val.clone())]);
                validate_field_values(
                    std::slice::from_ref(spec),
                    FieldContext::Options,
                    &single_map,
                    None,
                )
                .map_err(ApiError::unprocessable)?;
                settings::validate_url_or_link_spec_strict(spec, &val)
                    .map_err(ApiError::unprocessable)?;
                let val_json = serde_json::to_string(&val)?;
                sqlx::query(
                    "INSERT INTO theme_option_values (theme_id, owner_kind, owner_id, option_key, value_json)
                     VALUES (?, 'theme', ?, ?, ?)",
                )
                .bind(theme_id)
                .bind(theme_id)
                .bind(&s.option_key)
                .bind(&val_json)
                .execute(&mut *conn)
                .await?;

                let target_hash = sha256_hex(val_json.as_bytes());
                sqlx::query(
                    "INSERT INTO import_provenance
                     (theme_id, sample_key, resource_kind, object_key, revision_id, revision_digest, target_id, target_hash)
                     VALUES (?, ?, 'settings', ?, ?, ?, ?, ?)",
                )
                .bind(theme_id)
                .bind(&plan.sample_key)
                .bind(&s.object_key)
                .bind(&plan.revision_id)
                .bind(&plan.revision_digest)
                .bind(&s.option_key)
                .bind(&target_hash)
                .execute(&mut *conn)
                .await?;
                any_setting_created = true;
                created.push(ImportOutcomeItem {
                    resource_kind: SampleGroup::Settings,
                    object_key: s.object_key.clone(),
                    target_id: s.option_key.clone(),
                    status: "created".to_string(),
                    reason: None,
                });
            }
        }
        if any_setting_created {
            sqlx::query(
                "INSERT INTO theme_option_versions (theme_id, owner_kind, owner_id, version) VALUES (?, 'theme', ?, 1)
                 ON CONFLICT(owner_kind, owner_id) DO UPDATE SET version = version + 1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            )
            .bind(theme_id)
            .bind(theme_id)
            .execute(&mut *conn)
            .await?;
        }

        let outcome = ImportExecutionOutcome {
            run_id: run_id.clone(),
            theme_id: theme_id.to_string(),
            revision_id: plan.revision_id.clone(),
            sample_key: plan.sample_key.clone(),
            plan_digest: plan.plan_digest.clone(),
            created,
            skipped,
        };
        let outcome_json = serde_json::to_string(&outcome)?;
        sqlx::query(
            "UPDATE import_runs
             SET state = 'committed', outcome_json = ?, updated_at = datetime('now')
             WHERE run_id = ?",
        )
        .bind(&outcome_json)
        .bind(&run_id)
        .execute(&mut *conn)
        .await?;
        Ok(outcome)
    }
    .await;

    match tx_res {
        Ok(outcome) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(outcome)
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            let _ = sqlx::query(
                "UPDATE import_runs SET state = 'failed', updated_at = datetime('now') WHERE run_id = ?",
            )
            .bind(&run_id)
            .execute(db)
            .await;
            Err(err)
        }
    }
}
