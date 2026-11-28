use super::*;

pub async fn plan_sample_import(
    db: &SqlitePool,
    theme_id: &str,
    input: &DryRunImportInput,
) -> ApiResult<ImportPlan> {
    if input.groups.is_empty() {
        return Err(ApiError::bad_request(
            "Cần chọn ít nhất một nhóm dữ liệu (pages/menus/media/settings).",
        ));
    }
    let selected_set: BTreeSet<SampleGroup> = input.groups.iter().copied().collect();
    let selected_groups: Vec<SampleGroup> = selected_set.iter().copied().collect();

    let selection = get_active_selection(db).await?;
    let target_rev_id = match (&input.revision_id, &selection.revision_id) {
        (Some(rid), _) => rid.clone(),
        (None, Some(active_rid)) => active_rid.clone(),
        (None, None) => {
            return Err(ApiError::not_found(
                "Chưa có revision theme v1 đang hoạt động.",
            ));
        }
    };

    let rev = find_revision_by_id(db, &target_rev_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Revision theme chưa được cài đặt."))?;
    if rev.theme_id != theme_id || rev.source_format != ThemeSourceFormat::V1 {
        return Err(ApiError::not_found(
            "Revision không thuộc theme yêu cầu hoặc không phải v1.",
        ));
    }

    let option_schema = settings::parse_theme_options_schema(&rev.manifest_json)
        .map_err(ApiError::unprocessable)?;
    let manifest: ThemeManifest = serde_json::from_str(&rev.manifest_json)
        .map_err(|_| ApiError::internal("Manifest theme v1 không hợp lệ."))?;
    manifest.validate().map_err(ApiError::internal)?;
    let Some(raw_sample) = manifest.sample else {
        return Err(ApiError::not_found(
            "Theme không khai báo dữ liệu mẫu (sample).",
        ));
    };
    let sample: ThemeSampleManifest = serde_json::from_value(raw_sample).map_err(|_| {
        ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Khai báo sample trong theme.json không đúng định dạng.".into(),
        )
    })?;
    validate_sample_manifest(&sample, &rev.file_digests, &option_schema)?;

    let mut items = Vec::new();
    let mut unselected_dependencies = Vec::new();
    let mut menu_versions = BTreeMap::new();
    let mut requires_public_ack = false;
    let mut implications_set = BTreeSet::new();

    if selected_set.contains(&SampleGroup::Media) {
        let mut sorted = sample.media.clone();
        sorted.sort_by(|a, b| a.object_key.cmp(&b.object_key));
        for m in sorted {
            let (action, reason) = if has_tombstone(
                db,
                theme_id,
                &sample.sample_key,
                SampleGroup::Media,
                &m.object_key,
            )
            .await?
            {
                (PlanAction::Skip, Some("tombstone_exists".to_string()))
            } else if has_provenance(
                db,
                theme_id,
                &sample.sample_key,
                SampleGroup::Media,
                &m.object_key,
            )
            .await?
            {
                (PlanAction::Skip, Some("already_imported".to_string()))
            } else {
                (PlanAction::Create, None)
            };
            implications_set.insert("media: private_until_referenced".to_string());
            items.push(ImportPlanItem {
                resource_kind: SampleGroup::Media,
                object_key: m.object_key,
                logical_target: format!("media:{}", m.path),
                action,
                reason,
                publication_implication: "private_until_referenced".to_string(),
                dependencies: Vec::new(),
            });
        }
    }

    if selected_set.contains(&SampleGroup::Pages) {
        let mut conn = db.acquire().await?;
        let default_locale =
            crate::storage::settings::get_default_locale_code_in_conn(&mut conn).await?;
        drop(conn);
        let mut sorted = sample.pages.clone();
        sorted.sort_by(|a, b| a.object_key.cmp(&b.object_key));
        for p in sorted {
            let mut missing_deps = Vec::new();
            if !selected_set.contains(&SampleGroup::Media) {
                for mref in &p.media_refs {
                    missing_deps.push(format!("media:{mref}"));
                    unselected_dependencies.push(UnselectedDependency {
                        required_group: SampleGroup::Media,
                        object_key: mref.clone(),
                        referenced_by_group: SampleGroup::Pages,
                        referenced_by_object: p.object_key.clone(),
                    });
                }
            }
            let exists_site: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM content_entries WHERE content_type_key = ? AND locale = ? AND slug = ?)",
            )
            .bind(&p.content_type)
            .bind(&default_locale)
            .bind(&p.slug)
            .fetch_one(db)
            .await?;

            let (action, reason) = if !missing_deps.is_empty() {
                (
                    PlanAction::MissingDependency,
                    Some("unselected_dependency:media".to_string()),
                )
            } else if has_tombstone(
                db,
                theme_id,
                &sample.sample_key,
                SampleGroup::Pages,
                &p.object_key,
            )
            .await?
            {
                (PlanAction::Skip, Some("tombstone_exists".to_string()))
            } else if has_provenance(
                db,
                theme_id,
                &sample.sample_key,
                SampleGroup::Pages,
                &p.object_key,
            )
            .await?
            {
                (PlanAction::Skip, Some("already_imported".to_string()))
            } else if exists_site {
                (PlanAction::Skip, Some("site_conflict".to_string()))
            } else {
                (PlanAction::Create, None)
            };

            implications_set.insert("pages: created_as_draft".to_string());
            items.push(ImportPlanItem {
                resource_kind: SampleGroup::Pages,
                object_key: p.object_key,
                logical_target: format!("{}:{}", p.content_type, p.slug),
                action,
                reason,
                publication_implication: "created_as_draft".to_string(),
                dependencies: p.media_refs.iter().map(|r| format!("media:{r}")).collect(),
            });
        }
    }

    if selected_set.contains(&SampleGroup::Menus) {
        let mut sorted = sample.menus.clone();
        sorted.sort_by(|a, b| a.object_key.cmp(&b.object_key));
        for menu in sorted {
            let mut deps = Vec::new();
            let mut missing = false;
            for entry in &menu.items {
                if entry.target_kind == "entry" {
                    if let Some(target) = entry.target_object_key.as_deref() {
                        deps.push(format!("pages:{target}"));
                        if !selected_set.contains(&SampleGroup::Pages) {
                            missing = true;
                            unselected_dependencies.push(UnselectedDependency {
                                required_group: SampleGroup::Pages,
                                object_key: target.to_string(),
                                referenced_by_group: SampleGroup::Menus,
                                referenced_by_object: menu.object_key.clone(),
                            });
                        }
                    }
                }
            }

            let existing_ver: Option<i64> =
                sqlx::query_scalar("SELECT version FROM site_menus WHERE menu_key = ?")
                    .bind(&menu.menu_key)
                    .fetch_optional(db)
                    .await?;
            if let Some(ver) = existing_ver {
                menu_versions.insert(menu.menu_key.clone(), ver);
            }

            let (action, reason) = if missing {
                (
                    PlanAction::MissingDependency,
                    Some("unselected_dependency:pages".to_string()),
                )
            } else if has_tombstone(
                db,
                theme_id,
                &sample.sample_key,
                SampleGroup::Menus,
                &menu.object_key,
            )
            .await?
            {
                (PlanAction::Skip, Some("tombstone_exists".to_string()))
            } else if has_provenance(
                db,
                theme_id,
                &sample.sample_key,
                SampleGroup::Menus,
                &menu.object_key,
            )
            .await?
            {
                (PlanAction::Skip, Some("already_imported".to_string()))
            } else if existing_ver.is_some() {
                (PlanAction::Skip, Some("site_conflict".to_string()))
            } else {
                requires_public_ack = true;
                (PlanAction::Create, None)
            };

            implications_set.insert("menus: public_effect_requires_ack".to_string());
            items.push(ImportPlanItem {
                resource_kind: SampleGroup::Menus,
                object_key: menu.object_key,
                logical_target: format!("menu:{}", menu.menu_key),
                action,
                reason,
                publication_implication: "public_effect_requires_ack".to_string(),
                dependencies: deps,
            });
        }
    }

    if selected_set.contains(&SampleGroup::Settings) {
        let mut sorted = sample.settings.clone();
        sorted.sort_by(|a, b| a.object_key.cmp(&b.object_key));
        for s in sorted {
            let mut deps = Vec::new();
            let mut missing = false;
            if let Some(mref) = s.media_ref.as_deref() {
                deps.push(format!("media:{mref}"));
                if !selected_set.contains(&SampleGroup::Media) {
                    missing = true;
                    unselected_dependencies.push(UnselectedDependency {
                        required_group: SampleGroup::Media,
                        object_key: mref.to_string(),
                        referenced_by_group: SampleGroup::Settings,
                        referenced_by_object: s.object_key.clone(),
                    });
                }
            }

            let exists_opt: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM theme_option_values WHERE owner_kind = 'theme' AND owner_id = ? AND option_key = ?)",
            )
            .bind(theme_id)
            .bind(&s.option_key)
            .fetch_one(db)
            .await?;

            let (action, reason) = if missing {
                (
                    PlanAction::MissingDependency,
                    Some("unselected_dependency:media".to_string()),
                )
            } else if has_tombstone(
                db,
                theme_id,
                &sample.sample_key,
                SampleGroup::Settings,
                &s.object_key,
            )
            .await?
            {
                (PlanAction::Skip, Some("tombstone_exists".to_string()))
            } else if has_provenance(
                db,
                theme_id,
                &sample.sample_key,
                SampleGroup::Settings,
                &s.object_key,
            )
            .await?
            {
                (PlanAction::Skip, Some("already_imported".to_string()))
            } else if exists_opt {
                (PlanAction::Skip, Some("site_conflict".to_string()))
            } else {
                requires_public_ack = true;
                (PlanAction::Create, None)
            };

            implications_set.insert("settings: public_effect_requires_ack".to_string());
            items.push(ImportPlanItem {
                resource_kind: SampleGroup::Settings,
                object_key: s.object_key,
                logical_target: format!("option:{}", s.option_key),
                action,
                reason,
                publication_implication: "public_effect_requires_ack".to_string(),
                dependencies: deps,
            });
        }
    }

    let theme_options_version: i64 = sqlx::query_scalar(
        "SELECT COALESCE((SELECT version FROM theme_option_versions WHERE owner_kind = 'theme' AND owner_id = ?), 0)",
    )
    .bind(theme_id)
    .fetch_one(db)
    .await?;

    let expected_versions = ExpectedVersions {
        theme_selection_version: selection.version,
        theme_options_version,
        menu_versions,
    };

    let publication_implications: Vec<String> = implications_set.into_iter().collect();

    let canonical_for_digest = serde_json::json!({
        "theme_id": theme_id,
        "revision_id": rev.revision_id,
        "revision_digest": rev.revision_digest,
        "sample_key": sample.sample_key,
        "selected_groups": selected_groups,
        "items": items,
        "unselected_dependencies": unselected_dependencies,
        "expected_versions": expected_versions,
        "requires_public_ack": requires_public_ack,
        "publication_implications": publication_implications,
    });
    let canonical_bytes = serde_json::to_vec(&canonical_for_digest)?;
    let digest_bytes = Sha256::digest(&canonical_bytes);
    let mut plan_digest = String::with_capacity(64);
    for b in digest_bytes {
        plan_digest.push_str(&format!("{b:02x}"));
    }

    Ok(ImportPlan {
        theme_id: theme_id.to_string(),
        revision_id: rev.revision_id,
        revision_digest: rev.revision_digest,
        sample_key: sample.sample_key,
        selected_groups,
        items,
        unselected_dependencies,
        expected_versions,
        requires_public_ack,
        publication_implications,
        plan_digest,
    })
}
