use super::*;

/// Activate or rollback to an existing immutable revision using optimistic CAS (`expected_version`).
/// Verifies A-DURABILITY on disk before mutating `theme_active_selection`.
pub async fn activate_revision(
    db: &SqlitePool,
    themes_root: &Path,
    revision_id: &str,
    expected_version: Option<i64>,
) -> ApiResult<RevisionActivationOutcome> {
    let Some(version) = expected_version else {
        return Ok(RevisionActivationOutcome::MissingExpectedVersion);
    };
    let row: Option<RevisionRow> = sqlx::query_as(
        "SELECT revision_id, theme_id, version, revision_digest, source_format,
                manifest_json, file_digests_json, provenance_json, created_at
         FROM theme_revisions
         WHERE revision_id = ?",
    )
    .bind(revision_id)
    .fetch_optional(db)
    .await?;
    let Some(row) = row else {
        return Ok(RevisionActivationOutcome::NotFound);
    };
    let revision = row_to_revision(row)?;
    // Legacy ZIP revisions can stay in historical DB records; only validated
    // v3 revisions may be newly activated after the S02 cutover.
    if revision.source_format != ThemeSourceFormat::V3 {
        return Ok(RevisionActivationOutcome::NotFound);
    }
    let manifest: ThemeV3Manifest = serde_json::from_str(&revision.manifest_json)
        .map_err(|_| ApiError::bad_request("Theme revision manifest v3 không hợp lệ."))?;
    manifest.validate().map_err(ApiError::bad_request)?;
    if manifest.theme_id != revision.theme_id || manifest.version != revision.version {
        return Err(ApiError::bad_request(
            "Theme revision metadata không khớp manifest.",
        ));
    }
    let declarations = crate::theme::parse_theme_declarations(&revision.manifest_json)
        .map_err(|msg| ApiError(axum::http::StatusCode::UNPROCESSABLE_ENTITY, msg.into()))?;
    let extension_manifest: serde_json::Value = serde_json::from_str(&revision.manifest_json)
        .map_err(|_| ApiError::internal("Theme manifest của revision bị hỏng."))?;
    let extension_decls: Option<opendash_core::admin_extensions::AdminExtensions> =
        extension_manifest
            .get("admin_extensions")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|_| ApiError::internal("Admin extensions theme revision bị hỏng."))?;
    let durability = verify_revision_durability(themes_root, &revision);
    if durability != RevisionDurabilityStatus::Valid {
        return Ok(RevisionActivationOutcome::CorruptRevision(durability));
    }

    // Only read files explicitly included in this verified immutable revision.
    // Never discover arbitrary filesystem content under the revisions directory.
    let mut block_files = BTreeMap::new();
    let revision_dir = themes_root
        .join("revisions")
        .join(&revision.revision_digest);
    for path in revision
        .file_digests
        .keys()
        .filter(|path| path.starts_with("blocks/"))
    {
        let clean = safe_relative_path(path)?;
        let bytes = fs::read(revision_dir.join(clean))
            .map_err(|_| ApiError::internal("Không đọc được block trong theme revision."))?;
        block_files.insert(path.clone(), bytes);
    }
    let blocks = crate::theme::collect_package_blocks(&block_files, &revision.theme_id)
        .map_err(ApiError::bad_request)?;

    // Validate functions.settings_schema compatibility (forbid switching secret <-> non-secret).
    if let Some(func) = &manifest.functions {
        let saved_rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT option_key, value_json FROM theme_option_values
              WHERE owner_kind = 'theme_functions' AND owner_id = ?",
        )
        .bind(&revision.theme_id)
        .fetch_all(db)
        .await?;
        for (key, value_json) in saved_rows {
            let Some(field) = func.settings_schema.iter().find(|f| f.key == key) else {
                continue;
            };
            let is_stored_secret = serde_json::from_str::<serde_json::Value>(&value_json)
                .ok()
                .is_some_and(|v| crate::storage::settings::is_secret_sentinel(&v).is_some());
            let wants_secret = field.field_type == opendash_core::field_schema::FieldType::Secret;
            if is_stored_secret != wants_secret {
                return Err(ApiError::conflict(
                    "Schema option của revision mới không tương thích với giá trị đã lưu (409).",
                ));
            }
        }
    }
    let theme_action_ids: Vec<String> = manifest
        .functions
        .as_ref()
        .map(|f| f.actions.iter().map(|a| a.id.clone()).collect())
        .unwrap_or_default();

    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let block_update = crate::storage::page::sync_owner_blocks_in_tx(
        &mut conn,
        "theme",
        &revision.theme_id,
        ProviderStatus::Active,
        &revision.revision_digest,
        &blocks,
    )
    .await;
    if let Err(err) = block_update {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(err);
    }
    if let Err(err) = crate::storage::content::sync_theme_declarations_in_tx(
        &mut conn,
        &revision.theme_id,
        &declarations,
    )
    .await
    {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(err);
    }
    let extension_update = async {
        crate::storage::admin_extensions::require_capacity_in_conn(
            &mut conn,
            "theme",
            &revision.theme_id,
            extension_decls.as_ref(),
        )
        .await?;
        crate::storage::admin_extensions::sync_owner_in_conn(
            &mut conn,
            "theme",
            &revision.theme_id,
            extension_decls.as_ref(),
            "active",
            &theme_action_ids,
        )
        .await?;
        crate::storage::admin_extensions::set_theme_active_in_conn(
            &mut conn,
            Some(&revision.theme_id),
        )
        .await
    }
    .await;
    if let Err(err) = extension_update {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(err);
    }
    let res = sqlx::query(
        "UPDATE theme_active_selection
         SET revision_id = ?, version = version + 1
         WHERE id = 1 AND version = ?",
    )
    .bind(revision_id)
    .bind(version)
    .execute(&mut *conn)
    .await;

    match res {
        Ok(done) if done.rows_affected() == 1 => {
            if let Err(err) =
                sqlx::query("DELETE FROM plugin_entry_projections WHERE owner_kind = 'theme'")
                    .execute(&mut *conn)
                    .await
            {
                let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                return Err(err.into());
            }
            // The effective CPT body visibility and active theme selection must
            // already reflect the new revision when projecting public media.
            // Rebuild after every declaration update, still before COMMIT.
            if let Err(err) = crate::storage::page::resync_plugin_block_acls_in_tx(&mut conn).await
            {
                let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                return Err(err);
            }
            if let Err(err) = crate::storage::search::sync_builder_entries_in_conn(&mut conn).await
            {
                let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                return Err(err);
            }
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(RevisionActivationOutcome::Activated(
                get_active_selection(db).await?,
            ))
        }
        Ok(_) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Ok(RevisionActivationOutcome::Stale)
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(err.into())
        }
    }
}

impl ActiveThemeV3 {
    pub fn asset_url_map(&self) -> BTreeMap<String, String> {
        self.manifest
            .assets
            .iter()
            .map(|asset| {
                (
                    asset.clone(),
                    format!("/theme/{}/{}", self.revision_digest, asset),
                )
            })
            .collect()
    }

    pub fn l10n_catalog_map(
        &self,
        locale_code: &str,
    ) -> BTreeMap<String, std::sync::Arc<opendash_core::l10n::Catalog>> {
        let mut out = BTreeMap::new();
        let domain = self
            .manifest
            .text_domain
            .as_deref()
            .unwrap_or(&self.manifest.theme_id);
        let canon = opendash_core::l10n::canonicalize_locale_code(locale_code)
            .unwrap_or_else(|_| locale_code.to_ascii_lowercase());
        if let Some(cat) = self.l10n_catalogs.get(&canon) {
            out.insert(domain.to_owned(), cat.clone());
        } else if let Some((primary, _)) = canon.split_once('-')
            && let Some(cat) = self.l10n_catalogs.get(primary)
        {
            out.insert(domain.to_owned(), cat.clone());
        }
        out
    }
}

pub async fn upsert_owner_template_selection_in_conn(
    conn: &mut sqlx::SqliteConnection,
    owner_kind: &str,
    owner_id: &str,
    role: &str,
    scope_type_key: Option<&str>,
    template_selection_key: Option<&str>,
    ast_json: &str,
) -> ApiResult<OwnerTemplateSelection> {
    validate_owner_template_selection_key(owner_kind, owner_id, role, scope_type_key)?;
    if template_selection_key.is_some_and(|key| {
        !opendash_core::id::valid_kebab_segment_with_limit(key, 64)
            && !opendash_core::id::valid_snake_key(key)
    }) {
        return Err(ApiError::bad_request(
            "Tên template selection không hợp lệ.",
        ));
    }
    let _: opendash_core::builder::BuilderDocument = serde_json::from_str(ast_json)
        .map_err(|_| ApiError::bad_request("AST template selection không hợp lệ."))?;
    sqlx::query(
        "INSERT INTO owner_template_selections
         (owner_kind, owner_id, role, scope_type_key, template_selection_key, ast_json, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, datetime('now'))
         ON CONFLICT(owner_kind, owner_id, role, scope_type_key) DO UPDATE SET
           template_selection_key = excluded.template_selection_key,
           ast_json = excluded.ast_json,
           version = owner_template_selections.version + 1,
           updated_at = excluded.updated_at",
    )
    .bind(owner_kind)
    .bind(owner_id)
    .bind(role)
    .bind(scope_type_key.unwrap_or(""))
    .bind(template_selection_key)
    .bind(ast_json)
    .execute(&mut *conn)
    .await?;
    get_owner_template_selection_in_conn(conn, owner_kind, owner_id, role, scope_type_key)
        .await?
        .ok_or_else(|| ApiError::internal("Không đọc lại được template selection vừa lưu."))
}

pub async fn get_owner_template_selection_in_conn(
    conn: &mut sqlx::SqliteConnection,
    owner_kind: &str,
    owner_id: &str,
    role: &str,
    scope_type_key: Option<&str>,
) -> ApiResult<Option<OwnerTemplateSelection>> {
    validate_owner_template_selection_key(owner_kind, owner_id, role, scope_type_key)?;
    let row: Option<(String, String, String, String, Option<String>, String, i64)> =
        sqlx::query_as(
            "SELECT owner_kind, owner_id, role, scope_type_key, template_selection_key, ast_json, version
               FROM owner_template_selections
              WHERE owner_kind = ? AND owner_id = ? AND role = ? AND scope_type_key = ?",
        )
        .bind(owner_kind)
        .bind(owner_id)
        .bind(role)
        .bind(scope_type_key.unwrap_or(""))
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(
        |(owner_kind, owner_id, role, scope, template_selection_key, ast_json, version)| {
            OwnerTemplateSelection {
                owner_kind,
                owner_id,
                role,
                scope_type_key: (!scope.is_empty()).then_some(scope),
                template_selection_key,
                ast_json,
                version,
            }
        },
    ))
}

pub async fn list_local_theme_templates(
    db: &SqlitePool,
    themes_root: &Path,
) -> ApiResult<LocalThemeCatalog> {
    let selection = get_active_selection(db).await?;
    if selection.revision_id.is_none() {
        return Ok(LocalThemeCatalog {
            active_theme_id: None,
            theme_state: "no_active_theme",
            theme_templates: vec![],
        });
    }
    if selection.source_format != Some(ThemeSourceFormat::V3) {
        return Ok(LocalThemeCatalog {
            active_theme_id: selection.theme_id,
            theme_state: "unsupported_theme_format",
            theme_templates: vec![],
        });
    }
    // This accessor verifies the active manifest, file digests and complete
    // marker before enumerating role keys. A withdrawn/corrupt provider yields
    // a deterministic fallback, never stale or remotely fetched templates.
    let Some(active) = load_active_theme_v3(db, themes_root).await? else {
        return Ok(LocalThemeCatalog {
            active_theme_id: selection.theme_id,
            theme_state: "active_theme_unavailable",
            theme_templates: vec![],
        });
    };
    let theme_id = active.manifest.theme_id.clone();
    let mut theme_templates = Vec::new();
    let targets = active
        .templates
        .keys()
        .cloned()
        .chain(active.parts.keys().map(|key| format!("part:{key}")))
        .chain(
            active
                .page_layouts
                .keys()
                .map(|key| format!("page_layout:{key}")),
        );
    for role in targets {
        let reason = Some("Quản lý template qua typed target và Template Overrides.");
        theme_templates.push(LocalThemeTemplateDescriptor {
            role,
            theme_id: theme_id.clone(),
            revision_id: active.revision_id.clone(),
            revision_digest: active.revision_digest.clone(),
            can_apply: false,
            reason,
        });
    }
    Ok(LocalThemeCatalog {
        active_theme_id: Some(theme_id),
        theme_state: "active",
        theme_templates,
    })
}

pub async fn load_active_theme_functions_snapshot(
    db: &SqlitePool,
) -> ApiResult<Option<ActiveThemeFunctionsSnapshot>> {
    let row: Option<(String, String, String, i64, String, String, String)> = sqlx::query_as(
        "SELECT r.theme_id, r.revision_id, r.revision_digest, a.version,
                r.source_format, r.manifest_json, r.file_digests_json
           FROM theme_active_selection a
           JOIN theme_revisions r ON r.revision_id = a.revision_id
          WHERE a.id = 1",
    )
    .fetch_optional(db)
    .await?;
    let Some((
        theme_id,
        revision_id,
        revision_digest,
        selection_version,
        source_format,
        manifest_json,
        file_digests_json,
    )) = row
    else {
        return Ok(None);
    };
    if ThemeSourceFormat::from_db(&source_format)? != ThemeSourceFormat::V3 {
        return Ok(None);
    }
    let file_digests: BTreeMap<String, String> = serde_json::from_str(&file_digests_json)?;
    Ok(Some(ActiveThemeFunctionsSnapshot {
        theme_id,
        revision_id,
        revision_digest,
        selection_version,
        manifest_json,
        file_digests,
    }))
}

pub async fn load_active_theme_v3(
    db: &SqlitePool,
    themes_root: &Path,
) -> ApiResult<Option<ActiveThemeV3>> {
    let mut conn = db.acquire().await?;
    load_active_theme_v3_in_conn(&mut conn, themes_root).await
}

pub(in crate::storage) async fn load_active_theme_v3_in_conn(
    conn: &mut sqlx::SqliteConnection,
    themes_root: &Path,
) -> ApiResult<Option<ActiveThemeV3>> {
    let Some(CapturedThemeV3 {
        manifest,
        revision,
        rev_dir,
        dev_overlay,
    }) = capture_active_theme_v3_in_conn(conn, themes_root).await?
    else {
        return Ok(None);
    };
    if let Some(snap) = dev_overlay {
        return Ok(Some(ActiveThemeV3 {
            revision_id: revision.revision_id,
            revision_digest: snap.revision_digest,
            manifest: snap.manifest,
            templates: snap.templates,
            parts: snap.parts,
            page_layouts: snap.page_layouts,
            l10n_catalogs: snap.l10n_catalogs,
        }));
    }
    let mut templates = BTreeMap::new();
    for declaration in &manifest.templates {
        let Ok(key) = declaration.canonical_key() else {
            return Ok(None);
        };
        let Ok(source) = std::fs::read_to_string(rev_dir.join(&declaration.file)) else {
            return Ok(None);
        };
        templates.insert(key, source);
    }
    let mut parts = BTreeMap::new();
    for (key, file) in &manifest.parts {
        let Ok(source) = std::fs::read_to_string(rev_dir.join(file)) else {
            return Ok(None);
        };
        parts.insert(key.clone(), source);
    }
    let mut page_layouts = BTreeMap::new();
    for layout in &manifest.page_layouts {
        let Ok(source) = std::fs::read_to_string(rev_dir.join(&layout.file)) else {
            return Ok(None);
        };
        page_layouts.insert(layout.key.clone(), source);
    }
    let mut l10n_catalogs = BTreeMap::new();
    let domain = manifest
        .text_domain
        .as_deref()
        .unwrap_or(&manifest.theme_id);
    for rel_path in revision.file_digests.keys() {
        if let Some(rest) = rel_path.strip_prefix("languages/") {
            if let Some(stem) = rest.strip_suffix(".mo")
                && let Some(raw_loc) = stem.strip_prefix(&format!("{domain}-"))
                && let Ok(canon) = opendash_core::l10n::canonicalize_locale_code(raw_loc)
                && let Ok(bytes) = std::fs::read(rev_dir.join(rel_path))
                && let Ok(cat) = opendash_core::l10n::parse_mo(&bytes)
            {
                l10n_catalogs.insert(canon, std::sync::Arc::new(cat));
            } else if let Some(stem) = rest.strip_suffix(".po")
                && let Some(raw_loc) = stem.strip_prefix(&format!("{domain}-"))
                && let Ok(canon) = opendash_core::l10n::canonicalize_locale_code(raw_loc)
                && !l10n_catalogs.contains_key(&canon)
                && let Ok(text) = std::fs::read_to_string(rev_dir.join(rel_path))
                && let Ok(cat) = opendash_core::l10n::parse_po(&text)
            {
                l10n_catalogs.insert(canon, std::sync::Arc::new(cat));
            }
        }
    }
    Ok(Some(ActiveThemeV3 {
        revision_id: revision.revision_id,
        revision_digest: revision.revision_digest,
        manifest,
        templates,
        parts,
        page_layouts,
        l10n_catalogs,
    }))
}
