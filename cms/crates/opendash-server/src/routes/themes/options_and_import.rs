use super::*;

pub async fn theme_install(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    Extension(themes_root): Extension<ThemeStoreRoot>,
    request_headers: HeaderMap,
    bytes: Bytes,
) -> ApiResult<Response> {
    require_admin_with_csrf(&request_headers, &db).await?;
    let content_type = request_headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok());
    if !is_valid_zip_content_type(content_type, true) {
        return Err(ApiError::unsupported_media_type("Cần gói application/zip."));
    }

    let pkg = parse_theme_zip_v3(&bytes).map_err(map_v2_zip_error)?;
    crate::storage::settings::parse_theme_options_schema(&pkg.manifest_json)
        .map_err(ApiError::bad_request)?;
    let provenance_json = serde_json::json!({
        "source": "admin_upload",
        "zip_bytes": bytes.len(),
    })
    .to_string();
    let outcome = theme::stage_and_commit_v3_revision(
        &db,
        &themes_root.0,
        &next_run_uuid(),
        &pkg.manifest.theme_id,
        &pkg.manifest.name,
        &pkg.manifest.version,
        &pkg.manifest_json,
        &provenance_json,
        &pkg.files,
    )
    .await?;
    let (status, rec) = match outcome {
        RevisionInstallOutcome::Installed(rec) => (StatusCode::CREATED, rec),
        RevisionInstallOutcome::AlreadyExists(rec) => (StatusCode::OK, rec),
    };
    Ok((
        status,
        headers(None),
        Json(ThemeV3InstallResponse {
            revision_id: rec.revision_id,
            theme_id: rec.theme_id,
            version: rec.version,
            revision_digest: rec.revision_digest,
            source_format: "v3",
            activated: false,
        }),
    )
        .into_response())
}

pub async fn theme_activate(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<crate::plugin::PluginManager>>,
    State(hooks): State<Arc<crate::hooks::HookDispatcher>>,
    State(coordinator): State<Arc<crate::operations::SiteCoordinator>>,
    Extension(themes_root): Extension<ThemeStoreRoot>,
    Extension(recovery): Extension<crate::config::RecoveryState>,
    request_headers: HeaderMap,
    State(content_registry): State<
        Arc<
            tokio::sync::RwLock<
                opendash_core::registry::RegistryCatalog<
                    opendash_core::content::ContentTypeDefinition,
                >,
            >,
        >,
    >,
    input: Result<Json<ActivateInput>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&request_headers, &db).await?;
    let version = parse_version(&request_headers)?;
    let Json(input) =
        input.map_err(|_| ApiError::bad_request("Dữ liệu kích hoạt không hợp lệ."))?;

    match input.revision_id {
        revision_id if !revision_id.trim().is_empty() => {
            let rev = find_revision_by_id(&db, &revision_id)
                .await?
                .ok_or_else(|| ApiError::not_found("Revision theme chưa được cài đặt."))?;
            crate::storage::settings::check_theme_options_compatibility(
                &db,
                &rev.theme_id,
                &rev.manifest_json,
            )
            .await?;
            match theme::activate_revision(&db, &themes_root.0, &revision_id, Some(version)).await?
            {
                RevisionActivationOutcome::Activated(selection) => {
                    *content_registry.write().await =
                        crate::storage::content::load_content_type_catalog(&db).await?;
                    plugins.invalidate_active_theme_cache().await;
                    if !recovery.is_safe_mode() {
                        if let Some((theme_id, rev_id, sel_ver, runtime_manifest)) =
                            plugins.active_theme_manifest().await
                        {
                            if runtime_manifest
                                .hooks
                                .contains(&opendash_core::hooks::HookEvent::ThemeAfterActivate)
                            {
                                hooks.enqueue_async(crate::hooks::AsyncHookJob {
                                    event: opendash_core::hooks::HookEvent::ThemeAfterActivate,
                                    principal: opendash_core::hooks::ExecutionPrincipal::Admin,
                                    locale: None,
                                    type_key: None,
                                    payload: serde_json::json!({
                                        "theme_id": &theme_id,
                                        "revision_id": &rev_id,
                                        "selection_version": sel_ver,
                                    }),
                                    dispatch_depth: 0,
                                    lease: lease.clone(),
                                });
                            }
                            if runtime_manifest.filters.iter().any(|f| {
                                matches!(
                                    f,
                                    opendash_core::hooks::HookEvent::FilterSeoHead
                                        | opendash_core::hooks::HookEvent::FilterEntryView
                                )
                            }) {
                                let _handle = crate::storage::plugin_projections::spawn_recompute_owner_projections_batched(
                                    db.clone(),
                                    Arc::clone(&hooks),
                                    Arc::clone(&coordinator),
                                    opendash_core::hooks::ExtensionOwnerKind::Theme,
                                    theme_id,
                                    Some((rev_id, sel_ver)),
                                    coordinator.maintenance_cancel_rx(),
                                    coordinator.shutdown_rx(),
                                )
                                .await;
                            }
                        }
                    }
                    let _ = crate::l10n::CatalogRegistry::global()
                        .reload_site_with_active_packages(&db, coordinator.paths())
                        .await;
                    Ok((headers(Some(selection.version)), Json(selection)).into_response())
                }
                RevisionActivationOutcome::NotFound => {
                    Err(ApiError::not_found("Revision theme chưa được cài đặt."))
                }
                RevisionActivationOutcome::MissingExpectedVersion => Err(
                    ApiError::precondition_required("Cần If-Match để kích hoạt theme."),
                ),
                RevisionActivationOutcome::Stale => Err(ApiError::precondition_failed(
                    "Theme đã thay đổi ở phiên khác (412). Tải lại trạng thái trước khi thử tiếp.",
                )),
                RevisionActivationOutcome::CorruptRevision(_) => Err(ApiError::conflict(
                    "Revision trên đĩa bị thiếu hoặc không khớp .complete/digest.",
                )),
            }
        }
        _ => Err(ApiError::bad_request("Cần revision_id theme v3.")),
    }
}
