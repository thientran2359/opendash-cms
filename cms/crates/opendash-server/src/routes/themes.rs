//! Admin-only theme v3 revision install/activation/rollback,
//! and revision-scoped `/theme/{revision_digest}/{*path}` asset serving.
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Extension, Json,
    body::Bytes,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use opendash_core::theme::TemplateCandidate;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::{
    auth::{
        Role, require_admin, require_admin_with_csrf, require_session, require_session_with_csrf,
    },
    error::{ApiError, ApiResult},
    package::{V2ZipError, is_valid_zip_content_type, validate_theme_relative_path},
    storage::{
        overrides, patterns,
        theme::{
            self, RevisionActivationOutcome, RevisionInstallOutcome, ThemeList, ThemeRevisionRecord,
        },
    },
    theme::{parse_theme_zip_v3, theme_asset_mime},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateOverrideSaveInput {
    target: TemplateCandidate,
    document: opendash_core::builder::BuilderDocument,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateOverridePreviewInput {
    target: TemplateCandidate,
    document: opendash_core::builder::BuilderDocument,
    #[serde(default)]
    preview_entry: Option<overrides::TemplatePreviewEntry>,
    #[serde(default)]
    preview_locale: Option<String>,
}

fn override_if_match(headers: &HeaderMap) -> ApiResult<Option<i64>> {
    let Some(raw) = headers.get(header::IF_MATCH) else {
        return Ok(None);
    };
    let val = raw
        .to_str()
        .map_err(|_| ApiError::bad_request("If-Match không hợp lệ."))?;
    let version = val
        .strip_prefix("\"v")
        .and_then(|tail| tail.strip_suffix('"'))
        .and_then(|s| {
            if s.is_empty()
                || !s.bytes().all(|b| b.is_ascii_digit())
                || (s.len() > 1 && s.starts_with('0'))
            {
                None
            } else {
                s.parse::<i64>().ok()
            }
        })
        .filter(|n| *n >= 1)
        .ok_or_else(|| ApiError::bad_request("If-Match phải có dạng \"vN\"."))?;
    Ok(Some(version))
}

pub async fn template_overrides_list(
    State(db): State<SqlitePool>,
    request_headers: HeaderMap,
) -> ApiResult<Response> {
    require_admin(&request_headers, &db).await?;
    let items = overrides::list_overrides(&db).await?;
    let items: Result<Vec<_>, serde_json::Error> = items
        .into_iter()
        .map(|record| {
            let mut value = serde_json::to_value(&record)?;
            if let Some(map) = value.as_object_mut() {
                map.insert(
                    "effective_source".to_owned(),
                    serde_json::to_value(
                        overrides::TemplateResolution::DbOverride(record).effective_source(),
                    )?,
                );
            }
            Ok(value)
        })
        .collect();
    Ok((headers(None), Json(serde_json::json!({"items":items?}))).into_response())
}

pub async fn template_overrides_put(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    request_headers: HeaderMap,
    Json(input): Json<TemplateOverrideSaveInput>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&request_headers, &db).await?;
    let expected = override_if_match(&request_headers)?;
    let saved =
        overrides::save_typed_override(&db, &input.target, &input.document, expected).await?;
    let status = if saved.version == 1 {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, headers(Some(saved.version)), Json(saved)).into_response())
}

pub async fn template_overrides_reset(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    Extension(themes_root): Extension<ThemeStoreRoot>,
    request_headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&request_headers, &db).await?;
    let expected = override_if_match(&request_headers)?;
    let rows = overrides::list_overrides(&db).await?;
    let target = rows
        .iter()
        .find(|row| row.id == id)
        .ok_or_else(|| ApiError::not_found("Không có template override để reset."))?;
    let candidates = overrides::build_reset_candidate_chain(&db, &target.target).await?;
    let active = theme::load_active_theme_v3(&db, &themes_root.0).await?;
    let reset = overrides::reset_typed_override(
        &db,
        id,
        expected,
        &candidates,
        active.as_ref().map(|v| &v.templates),
        active.as_ref().map(|v| &v.parts),
        active.as_ref().map(|v| &v.page_layouts),
    )
    .await?;
    Ok((headers(None), Json(reset)).into_response())
}

pub const MAX_STUDIO_PREVIEW_JSON_BYTES: usize = 512 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StudioPreviewRequest {
    target: TemplateCandidate,
    #[serde(default)]
    document: Option<opendash_core::builder::BuilderDocument>,
    #[serde(default)]
    preview_entry: Option<overrides::TemplatePreviewEntry>,
    #[serde(default)]
    preview_annotations: bool,
    #[serde(default)]
    transient: Option<overrides::TransientStudioOverrides>,
    #[serde(default)]
    preview_locale: Option<String>,
}

pub async fn studio_targets(
    State(db): State<SqlitePool>,
    Extension(themes_root): Extension<ThemeStoreRoot>,
    request_headers: HeaderMap,
) -> ApiResult<Response> {
    require_admin(&request_headers, &db).await?;
    let result = overrides::list_studio_targets(&db, &themes_root.0).await?;
    let mut response_headers = headers(None);
    response_headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok((response_headers, Json(result)).into_response())
}

pub async fn studio_preview(
    State(db): State<SqlitePool>,
    State(block_template_cache): State<Arc<crate::page::BlockTemplateCache>>,
    Extension(themes_root): Extension<ThemeStoreRoot>,
    request_headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Response> {
    let actor = require_admin_with_csrf(&request_headers, &db).await?;
    let content_type = request_headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    if !content_type.is_some_and(|value| value.eq_ignore_ascii_case("application/json")) {
        return Err(ApiError::unsupported_media_type(
            "Expected application/json.",
        ));
    }
    if body.len() > MAX_STUDIO_PREVIEW_JSON_BYTES {
        return Err(ApiError::bad_request("Studio preview exceeds 512 KiB."));
    }
    let input: StudioPreviewRequest = serde_json::from_slice(&body)
        .map_err(|_| ApiError::bad_request("Invalid studio preview JSON."))?;
    let rendered = overrides::preview_studio_with_cache_and_locale(
        &db,
        &themes_root.0,
        &actor,
        &input.target,
        input.document.as_ref(),
        input.preview_entry.as_ref(),
        input.preview_annotations,
        input.transient.as_ref(),
        input.preview_locale.as_deref(),
        &block_template_cache,
    )
    .await?;
    let mut response_headers = headers(None);
    response_headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    let layout_response = super::public::rendered_layout_with_form_csp(
        rendered.html.clone(),
        rendered.has_active_contact_form,
    );
    for name in [header::CONTENT_SECURITY_POLICY, header::REFERRER_POLICY] {
        if let Some(value) = layout_response.headers().get(&name) {
            response_headers.insert(name, value.clone());
        }
    }
    Ok((response_headers, Json(rendered)).into_response())
}

/// Render a shared template draft against one private read snapshot.
pub async fn template_overrides_preview(
    State(db): State<SqlitePool>,
    State(block_template_cache): State<Arc<crate::page::BlockTemplateCache>>,
    Extension(themes_root): Extension<ThemeStoreRoot>,
    request_headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Response> {
    let actor = require_admin_with_csrf(&request_headers, &db).await?;
    let content_type = request_headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    if !content_type.is_some_and(|value| value.eq_ignore_ascii_case("application/json")) {
        return Err(ApiError::unsupported_media_type(
            "Expected application/json.",
        ));
    }
    if body.len() > 256 * 1024 {
        return Err(ApiError::bad_request("Template preview exceeds 256 KiB."));
    }
    let expected = override_if_match(&request_headers)?;
    let input: TemplateOverridePreviewInput = serde_json::from_slice(&body)
        .map_err(|_| ApiError::bad_request("Invalid template preview JSON."))?;
    let rendered = overrides::preview_typed_override_with_cache_and_locale(
        &db,
        &themes_root.0,
        &actor,
        &input.target,
        &input.document,
        input.preview_entry.as_ref(),
        expected,
        input.preview_locale.as_deref(),
        &block_template_cache,
    )
    .await?;
    let mut response_headers = headers(None);
    response_headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok((response_headers, Json(rendered)).into_response())
}

static RUN_SEQ: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub struct ThemeStoreRoot(pub PathBuf);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivateInput {
    revision_id: String,
}

#[derive(Serialize)]
struct ThemeV3InstallResponse {
    revision_id: String,
    theme_id: String,
    version: String,
    revision_digest: String,
    source_format: &'static str,
    activated: bool,
}

fn next_run_uuid() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = RUN_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("run-{}-{nanos}-{seq}", std::process::id())
}

fn headers(version: Option<i64>) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Some(v) = version {
        h.insert(
            header::ETAG,
            HeaderValue::from_str(&format!("\"v{v}\"")).expect("version is integer"),
        );
    }
    h
}

fn response(value: ThemeList) -> Response {
    (headers(Some(value.version)), Json(value)).into_response()
}

/// Site-local catalog: Editors may list private Pattern metadata, while theme
/// template descriptors remain Admin-only. Pattern AST is retrieved only through
/// the generic authenticated entry Builder API.
pub async fn local_library_list(
    State(db): State<SqlitePool>,
    Extension(themes_root): Extension<ThemeStoreRoot>,
    request_headers: HeaderMap,
) -> ApiResult<Response> {
    let actor = require_session(&request_headers, &db).await?;
    let patterns = patterns::list(&db).await?;
    let templates = if actor.user.role == Role::Admin {
        theme::list_local_theme_templates(&db, &themes_root.0).await?
    } else {
        theme::LocalThemeCatalog {
            active_theme_id: None,
            theme_state: "admin_only",
            theme_templates: vec![],
        }
    };
    Ok((
        headers(None),
        Json(serde_json::json!({
            "patterns": patterns,
            "theme_templates": templates.theme_templates,
            "theme_state": templates.theme_state,
            "active_theme_id": templates.active_theme_id,
        })),
    )
        .into_response())
}

fn parse_version(headers: &HeaderMap) -> ApiResult<i64> {
    let val = headers
        .get(header::IF_MATCH)
        .ok_or_else(|| ApiError::precondition_required("Cần If-Match để kích hoạt theme."))?
        .to_str()
        .map_err(|_| ApiError::bad_request("If-Match không hợp lệ."))?;
    val.strip_prefix("\"v")
        .and_then(|s| s.strip_suffix('"'))
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|v| *v >= 1)
        .ok_or_else(|| ApiError::bad_request("If-Match phải có định dạng \"vN\"."))
}

fn map_v2_zip_error(err: V2ZipError) -> ApiError {
    match err {
        V2ZipError::PayloadTooLarge(msg) => ApiError(StatusCode::PAYLOAD_TOO_LARGE, msg.into()),
        V2ZipError::BadRequest(msg) => ApiError::bad_request(msg),
    }
}

async fn find_revision_by_id(
    db: &SqlitePool,
    revision_id: &str,
) -> ApiResult<Option<ThemeRevisionRecord>> {
    for pkg in theme::list_packages(db).await? {
        for rev in theme::list_revisions(db, &pkg.theme_id).await? {
            if rev.revision_id == revision_id {
                return Ok(Some(rev));
            }
        }
    }
    Ok(None)
}

async fn find_revision_by_digest(
    db: &SqlitePool,
    digest: &str,
) -> ApiResult<Option<ThemeRevisionRecord>> {
    for pkg in theme::list_packages(db).await? {
        for rev in theme::list_revisions(db, &pkg.theme_id).await? {
            if rev.revision_digest == digest {
                return Ok(Some(rev));
            }
        }
    }
    Ok(None)
}

pub async fn theme_list(
    State(db): State<SqlitePool>,
    request_headers: HeaderMap,
) -> ApiResult<Response> {
    require_admin(&request_headers, &db).await?;
    Ok(response(theme::list(&db).await?))
}

/// Editors can discover the current theme's available design choices without
/// being exposed to revision directories or server-side template HTML.
pub async fn active_layouts(
    State(db): State<SqlitePool>,
    Extension(themes_root): Extension<ThemeStoreRoot>,
    request_headers: HeaderMap,
) -> ApiResult<Response> {
    require_session(&request_headers, &db).await?;
    let active = theme::load_active_theme_v3(&db, &themes_root.0).await?;
    let (active_theme_id, page_layouts, custom_templates) = match active {
        Some(active) => {
            let layouts: Vec<_> = active
                .manifest
                .page_layouts
                .iter()
                .map(|layout| {
                    serde_json::json!({
                        "key": layout.key, "label": layout.label,
                    })
                })
                .collect();
            let customs: Vec<_> = active
                .manifest
                .templates
                .iter()
                .filter(|tpl| tpl.kind == opendash_core::theme::TemplateCandidateKind::SingleCustom)
                .map(|tpl| {
                    serde_json::json!({
                        "custom_key": tpl.custom_key, "label": tpl.label,
                        "allowed_types": tpl.allowed_types,
                    })
                })
                .collect();
            (Some(active.manifest.theme_id), layouts, customs)
        }
        None => (None, vec![], vec![]),
    };
    Ok((
        headers(None),
        Json(serde_json::json!({
            "active_theme_id": active_theme_id,
            "page_layouts": page_layouts,
            "custom_templates": custom_templates,
        })),
    )
        .into_response())
}

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

pub async fn theme_asset(
    State(db): State<SqlitePool>,
    Extension(themes_root): Extension<ThemeStoreRoot>,
    Path((revision_digest, rel_path)): Path<(String, String)>,
) -> Response {
    let Ok(safe_rel) = validate_theme_relative_path(&rel_path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(mime) = theme_asset_mime(&safe_rel) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    if let Some(dev_hex) = revision_digest.strip_prefix("dev-") {
        if dev_hex.len() != 64
            || !dev_hex
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        {
            return StatusCode::NOT_FOUND.into_response();
        }
        let Some(dev) = crate::dev_mode::DevOverlayConfig::global() else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let Ok(bytes) = dev.read_dev_theme_asset(&safe_rel) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let mut response = (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, HeaderValue::from_static(mime)),
                (
                    header::X_CONTENT_TYPE_OPTIONS,
                    HeaderValue::from_static("nosniff"),
                ),
                (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            ],
            bytes,
        )
            .into_response();
        if mime == "image/svg+xml" {
            response.headers_mut().insert(
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'; sandbox"),
            );
        }
        return response;
    }

    if revision_digest.len() != 64
        || !revision_digest
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Ok(Some(rev)) = find_revision_by_digest(&db, &revision_digest).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    // New active asset formats are supported only by validated v3 packages.
    if matches!(mime, "text/javascript; charset=utf-8" | "image/svg+xml")
        && (rev.source_format != theme::ThemeSourceFormat::V3
            || !opendash_core::theme::valid_v3_asset_path(&safe_rel))
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(expected_digest) = rev.file_digests.get(&safe_rel) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let rev_dir = themes_root.0.join("revisions").join(&revision_digest);
    let marker_path = rev_dir.join(".complete");
    let Ok(marker_text) = fs::read_to_string(&marker_path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if marker_text.trim() != revision_digest {
        return StatusCode::NOT_FOUND.into_response();
    }

    let mut file_path = rev_dir.clone();
    for segment in safe_rel.split('/') {
        file_path.push(segment);
    }

    let Ok(meta) = fs::symlink_metadata(&file_path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if meta.file_type().is_symlink() || !meta.is_file() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Ok(bytes) = fs::read(&file_path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let actual_digest = hex::encode(Sha256::digest(&bytes));
    if actual_digest != *expected_digest {
        return StatusCode::NOT_FOUND.into_response();
    }

    let mut response = (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(mime)),
            (
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            ),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=31536000, immutable"),
            ),
        ],
        bytes,
    )
        .into_response();
    if mime == "image/svg+xml" {
        response.headers_mut().insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'; sandbox"),
        );
    }
    response
}

#[derive(Debug, Serialize)]
pub struct ThemeFunctionsSettingsResponse {
    pub theme_id: Option<String>,
    pub revision_id: Option<String>,
    pub has_functions: bool,
    pub version: i64,
    pub schema: Vec<opendash_core::plugin::PluginSettingField>,
    pub values: serde_json::Map<String, serde_json::Value>,
    pub actions: Vec<opendash_core::plugin::PluginActionDeclaration>,
}

#[derive(Debug, Deserialize)]
pub struct ThemeFunctionsLogsQuery {
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn theme_functions_get_settings(
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<crate::plugin::PluginManager>>,
    request_headers: HeaderMap,
) -> ApiResult<Response> {
    require_admin(&request_headers, &db).await?;
    if let Some((theme_id, revision_id, _sel_ver, manifest)) = plugins.active_theme_manifest().await
    {
        let snapshot = crate::storage::settings::get_theme_functions_settings_redacted(
            &db,
            &theme_id,
            &manifest.settings_schema,
        )
        .await?;
        let resp = ThemeFunctionsSettingsResponse {
            theme_id: Some(theme_id),
            revision_id: Some(revision_id),
            has_functions: true,
            version: snapshot.version,
            schema: snapshot.schema,
            values: snapshot.values,
            actions: manifest.actions,
        };
        return Ok((
            crate::routes::settings::no_store_headers(Some(resp.version)),
            Json(resp),
        )
            .into_response());
    }
    let selection = theme::get_active_selection(&db).await?;
    let resp = ThemeFunctionsSettingsResponse {
        theme_id: selection.theme_id,
        revision_id: selection.revision_id,
        has_functions: false,
        version: 0,
        schema: Vec::new(),
        values: serde_json::Map::new(),
        actions: Vec::new(),
    };
    Ok((crate::routes::settings::no_store_headers(None), Json(resp)).into_response())
}

#[allow(clippy::too_many_arguments)]
pub async fn theme_functions_put_settings(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<crate::plugin::PluginManager>>,
    State(hooks): State<Arc<crate::hooks::HookDispatcher>>,
    State(coordinator): State<Arc<crate::operations::SiteCoordinator>>,
    Extension(recovery): Extension<crate::config::RecoveryState>,
    request_headers: HeaderMap,
    input: Result<Json<crate::routes::plugin::PluginSettingsPutInput>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&request_headers, &db).await?;
    let expected_version = crate::routes::settings::parse_if_match(&request_headers)?;
    let Json(input) = input.map_err(|err| {
        if err.status() == StatusCode::PAYLOAD_TOO_LARGE {
            ApiError(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Dữ liệu cài đặt Theme Functions vượt quá 1 MiB.".into(),
            )
        } else {
            ApiError::bad_request("Dữ liệu cài đặt Theme Functions không hợp lệ.")
        }
    })?;
    let Some((theme_id, revision_id, sel_ver, manifest)) = plugins.active_theme_manifest().await
    else {
        return Err(ApiError::not_found(
            "Theme hiện hành không có functions.js.",
        ));
    };
    let has_projection_filters = manifest.filters.iter().any(|f| {
        matches!(
            f,
            opendash_core::hooks::HookEvent::FilterSeoHead
                | opendash_core::hooks::HookEvent::FilterEntryView
        )
    });
    let outcome = crate::storage::settings::put_theme_functions_settings(
        &db,
        plugins.secrets_root(),
        &theme_id,
        expected_version,
        &manifest.settings_schema,
        &input.values,
        has_projection_filters,
    )
    .await?;

    if !recovery.is_safe_mode() {
        if outcome.changed && outcome.has_projection_filters {
            let _handle =
                crate::storage::plugin_projections::spawn_recompute_owner_projections_batched(
                    db.clone(),
                    Arc::clone(&hooks),
                    Arc::clone(&coordinator),
                    opendash_core::hooks::ExtensionOwnerKind::Theme,
                    theme_id.clone(),
                    Some((revision_id.clone(), sel_ver)),
                    coordinator.maintenance_cancel_rx(),
                    coordinator.shutdown_rx(),
                )
                .await;
        }
        hooks.enqueue_async(crate::hooks::AsyncHookJob {
            event: opendash_core::hooks::HookEvent::SettingsAfterSave,
            principal: opendash_core::hooks::ExecutionPrincipal::Admin,
            locale: None,
            type_key: None,
            payload: serde_json::json!({
                "scope": "theme_functions",
                "theme_id": &theme_id,
                "version": outcome.snapshot.version,
            }),
            dispatch_depth: 0,
            lease,
        });
    }

    let resp = ThemeFunctionsSettingsResponse {
        theme_id: Some(theme_id),
        revision_id: Some(revision_id),
        has_functions: true,
        version: outcome.snapshot.version,
        schema: outcome.snapshot.schema,
        values: outcome.snapshot.values,
        actions: manifest.actions,
    };
    Ok((
        crate::routes::settings::no_store_headers(Some(resp.version)),
        Json(resp),
    )
        .into_response())
}

#[allow(clippy::too_many_arguments)]
pub async fn theme_active_action(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<crate::plugin::PluginManager>>,
    State(content_registry): State<
        Arc<
            tokio::sync::RwLock<
                opendash_core::registry::RegistryCatalog<
                    opendash_core::content::ContentTypeDefinition,
                >,
            >,
        >,
    >,
    State(hooks): State<Arc<crate::hooks::HookDispatcher>>,
    request_headers: HeaderMap,
    Path(action_id): Path<String>,
    bytes: Bytes,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&request_headers, &db).await?;
    let input_val: serde_json::Value = if bytes.is_empty() {
        serde_json::json!({})
    } else {
        match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "success": false,
                        "data": null,
                        "error": format!("Dữ liệu đầu vào không phải JSON hợp lệ: {e}"),
                        "code": "invalid_input",
                        "correlation_id": ""
                    })),
                )
                    .into_response());
            }
        }
    };
    let correlation_id = format!(
        "tact-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    match plugins
        .execute_owner_action(
            opendash_core::hooks::ExtensionOwnerKind::Theme,
            None,
            &action_id,
            &actor,
            input_val,
            &content_registry,
            &hooks,
            correlation_id,
        )
        .await
    {
        Ok(resp) => {
            let principal = match actor.user.role {
                Role::Admin => opendash_core::hooks::ExecutionPrincipal::Admin,
                Role::Editor => opendash_core::hooks::ExecutionPrincipal::Editor,
            };
            hooks.enqueue_plugin_action_hooks(principal, resp.data.as_ref(), lease);
            Ok((StatusCode::OK, Json(resp)).into_response())
        }
        Err((status, resp)) => Ok((status, Json(resp)).into_response()),
    }
}

pub async fn theme_functions_logs(
    State(db): State<SqlitePool>,
    request_headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<ThemeFunctionsLogsQuery>,
) -> ApiResult<Response> {
    require_admin(&request_headers, &db).await?;
    let limit = query.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::bad_request("Tham số limit phải từ 1 đến 100."));
    }
    let active_theme_id = crate::dev_mode::DevOverlayConfig::global()
        .and_then(|d| d.dev_theme_id().map(str::to_owned))
        .or(theme::get_active_selection(&db).await?.theme_id);

    let items = match active_theme_id.as_deref() {
        Some(theme_id) => {
            crate::storage::plugin::list_owner_logs(
                &db,
                opendash_core::hooks::ExtensionOwnerKind::Theme,
                theme_id,
                limit,
            )
            .await?
        }
        None => Vec::new(),
    };
    Ok((
        crate::routes::settings::no_store_headers(None),
        Json(serde_json::json!({
            "theme_id": active_theme_id,
            "items": items,
        })),
    )
        .into_response())
}

pub async fn dev_sync(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<crate::plugin::PluginManager>>,
    State(content_registry): State<
        Arc<
            tokio::sync::RwLock<
                opendash_core::registry::RegistryCatalog<
                    opendash_core::content::ContentTypeDefinition,
                >,
            >,
        >,
    >,
    Extension(recovery): Extension<crate::config::RecoveryState>,
    request_headers: HeaderMap,
) -> ApiResult<Response> {
    require_admin_with_csrf(&request_headers, &db).await?;
    if recovery.is_safe_mode() {
        return Err(ApiError::service_unavailable(
            "Hệ thống đang ở safe mode; không thể đồng bộ Dev Manifest.",
        ));
    }
    let Some(dev) = crate::dev_mode::DevOverlayConfig::global() else {
        return Err(ApiError::not_found("Dev Mode hiện không được bật."));
    };

    let mut synced_theme: Option<String> = None;
    if let Some(theme_snap) = dev.load_dev_theme().map_err(ApiError::bad_request)? {
        theme::sync_dev_theme_in_db(&db, &theme_snap).await?;
        synced_theme = Some(theme_snap.manifest.theme_id);
    }

    let mut synced_plugins: Vec<String> = Vec::new();
    let plugin_snaps = dev
        .load_all_dev_plugins(plugins.engine())
        .map_err(ApiError::bad_request)?;
    for snap in plugin_snaps {
        crate::storage::plugin::sync_dev_plugin_in_db(&db, &snap).await?;
        synced_plugins.push(snap.manifest.id);
    }

    *content_registry.write().await =
        crate::storage::content::load_content_type_catalog(&db).await?;
    plugins.invalidate_active_theme_cache().await;
    plugins.invalidate_cache().await;
    crate::cache::SiteCacheEpoch::global().bump();

    Ok((
        crate::routes::settings::no_store_headers(None),
        Json(serde_json::json!({
            "synced_theme": synced_theme,
            "synced_plugins": synced_plugins,
        })),
    )
        .into_response())
}

mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        let mut out = String::with_capacity(bytes.as_ref().len() * 2);
        for b in bytes.as_ref() {
            out.push_str(&format!("{b:02x}"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn s01_serves_revision_scoped_js_and_sandboxed_svg() {
        let root = std::env::temp_dir().join(format!(
            "opendash-theme-assets-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let db = crate::storage::init_db(&root.join("site.sqlite"))
            .await
            .unwrap();
        let themes_root = root.join("themes");
        let manifest = r#"{"manifest_version":3,"theme_id":"asset-test","name":"Asset Test","version":"1.0.0","assets":["assets/main.js","assets/module.mjs","assets/icon.svg","assets/style.css"]}"#;
        let assets: [(&str, &[u8], &str); 4] = [
            (
                "assets/main.js",
                b"console.log('hello');",
                "text/javascript; charset=utf-8",
            ),
            (
                "assets/module.mjs",
                b"export {};",
                "text/javascript; charset=utf-8",
            ),
            (
                "assets/icon.svg",
                br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#,
                "image/svg+xml",
            ),
            ("assets/style.css", b"body{}", "text/css; charset=utf-8"),
        ];
        let mut files = std::collections::BTreeMap::from([(
            "theme.json".to_owned(),
            manifest.as_bytes().to_vec(),
        )]);
        for (path, bytes, _) in assets {
            files.insert(path.to_owned(), bytes.to_vec());
        }
        let revision = match theme::stage_and_commit_v3_revision(
            &db,
            &themes_root,
            "s01-assets",
            "asset-test",
            "Asset Test",
            "1.0.0",
            manifest,
            "{}",
            &files,
        )
        .await
        .unwrap()
        {
            RevisionInstallOutcome::Installed(revision) => revision,
            other => panic!("unexpected install result: {other:?}"),
        };
        for (path, bytes, mime) in assets {
            let response = theme_asset(
                State(db.clone()),
                Extension(ThemeStoreRoot(themes_root.clone())),
                Path((revision.revision_digest.clone(), path.to_owned())),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()[header::CONTENT_TYPE], mime);
            assert_eq!(
                response.headers()[header::X_CONTENT_TYPE_OPTIONS],
                "nosniff"
            );
            assert_eq!(
                response.headers()[header::CACHE_CONTROL],
                "public, max-age=31536000, immutable"
            );
            if mime == "image/svg+xml" {
                assert_eq!(
                    response.headers()[header::CONTENT_SECURITY_POLICY],
                    "default-src 'none'; style-src 'unsafe-inline'; sandbox"
                );
            } else {
                assert!(
                    !response
                        .headers()
                        .contains_key(header::CONTENT_SECURITY_POLICY)
                );
            }
            assert_eq!(
                axum::body::to_bytes(response.into_body(), 1024)
                    .await
                    .unwrap(),
                bytes
            );
        }
        // The new MIME allowlist must still enforce immutable revision digests.
        fs::write(
            themes_root
                .join("revisions")
                .join(&revision.revision_digest)
                .join("assets/main.js"),
            b"modified",
        )
        .unwrap();
        let response = theme_asset(
            State(db.clone()),
            Extension(ThemeStoreRoot(themes_root)),
            Path((revision.revision_digest, "assets/main.js".into())),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        db.close().await;
    }

    #[test]
    fn requires_canonical_etag() {
        let mut h = HeaderMap::new();
        assert_eq!(
            parse_version(&h).unwrap_err().0,
            StatusCode::PRECONDITION_REQUIRED
        );
        for s in ["v1", "W/\"v1\"", "\"v0\"", "\"v1\",\"v2\""] {
            h.insert(header::IF_MATCH, HeaderValue::from_str(s).unwrap());
            assert_eq!(parse_version(&h).unwrap_err().0, StatusCode::BAD_REQUEST);
        }
        h.insert(header::IF_MATCH, HeaderValue::from_static("\"v42\""));
        assert_eq!(parse_version(&h).unwrap(), 42);
    }
}
