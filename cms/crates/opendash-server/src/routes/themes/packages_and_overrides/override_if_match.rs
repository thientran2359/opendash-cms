use super::*;

pub(crate) fn override_if_match(headers: &HeaderMap) -> ApiResult<Option<i64>> {
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
    let active = theme::load_active_theme(&db, &themes_root.0).await?;
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
    let layout_response = crate::routes::public::rendered_layout_with_form_csp(
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

pub(crate) static RUN_SEQ: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_run_uuid() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = RUN_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("run-{}-{nanos}-{seq}", std::process::id())
}

pub(crate) fn headers(version: Option<i64>) -> HeaderMap {
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

pub(crate) fn response(value: ThemeList) -> Response {
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

pub(crate) fn parse_version(headers: &HeaderMap) -> ApiResult<i64> {
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

pub(crate) fn map_v2_zip_error(err: V2ZipError) -> ApiError {
    match err {
        V2ZipError::PayloadTooLarge(msg) => ApiError(StatusCode::PAYLOAD_TOO_LARGE, msg.into()),
        V2ZipError::BadRequest(msg) => ApiError::bad_request(msg),
    }
}

pub(crate) async fn find_revision_by_id(
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

pub(crate) async fn find_revision_by_digest(
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
    let active = theme::load_active_theme(&db, &themes_root.0).await?;
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
    // Active asset formats follow the validated Theme Format V1 package allowlist.
    if matches!(mime, "text/javascript; charset=utf-8" | "image/svg+xml")
        && (rev.source_format != theme::ThemeSourceFormat::V1
            || !opendash_core::theme::valid_theme_asset_path(&safe_rel))
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
    let actual_digest = format!("{:x}", Sha256::digest(&bytes));
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
