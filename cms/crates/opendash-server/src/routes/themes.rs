//! Admin-only theme install/list/CAS activation, v2 revision install/activation/rollback,
//! and revision-scoped `/theme/{revision_digest}/{*path}` asset serving.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Extension, Json,
    body::Bytes,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::{
    auth::{require_admin, require_admin_with_csrf},
    error::{ApiError, ApiResult},
    package::{V2ZipError, is_valid_zip_content_type, validate_v2_relative_path},
    storage::{
        page::insert_theme_block,
        theme::{
            self, ActivationOutcome, RevisionActivationOutcome, RevisionInstallOutcome, ThemeItem,
            ThemeList, ThemeRevisionRecord,
        },
    },
    theme::{
        Theme, is_v2_theme_zip, manifest_has_declarations, parse_theme_zip_v2, theme_asset_mime,
    },
};

static RUN_SEQ: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub struct ThemeStoreRoot(pub PathBuf);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivateInput {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    revision_id: Option<String>,
}

#[derive(Serialize)]
struct ThemeV2InstallResponse {
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

pub async fn theme_install(
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

    if is_v2_theme_zip(&bytes).map_err(map_v2_zip_error)? {
        let pkg = parse_theme_zip_v2(&bytes).map_err(map_v2_zip_error)?;
        let run_uuid = next_run_uuid();
        let provenance_json = serde_json::json!({
            "source": "admin_upload",
            "zip_bytes": bytes.len(),
        })
        .to_string();

        let outcome = theme::stage_and_commit_v2_revision(
            &db,
            &themes_root.0,
            &run_uuid,
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
        let payload = ThemeV2InstallResponse {
            revision_id: rec.revision_id,
            theme_id: rec.theme_id,
            version: rec.version,
            revision_digest: rec.revision_digest,
            source_format: "v2",
            activated: false,
        };
        return Ok((status, headers(None), Json(payload)).into_response());
    }

    let data = Theme::from_zip(&bytes).map_err(ApiError::bad_request)?;
    let item = ThemeItem {
        id: data.descriptor.id.clone(),
        compatible_block_id: data.compatible_block_id.clone(),
    };
    insert_theme_block(&db, &data).await?;
    Ok((StatusCode::CREATED, headers(None), Json(item)).into_response())
}

pub async fn theme_activate(
    State(db): State<SqlitePool>,
    Extension(themes_root): Extension<ThemeStoreRoot>,
    request_headers: HeaderMap,
    input: Result<Json<ActivateInput>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&request_headers, &db).await?;
    let version = parse_version(&request_headers)?;
    let Json(input) =
        input.map_err(|_| ApiError::bad_request("Dữ liệu kích hoạt không hợp lệ."))?;

    match (input.id, input.revision_id) {
        (Some(id), None) => match theme::activate(&db, &id, version).await? {
            ActivationOutcome::Activated(value) => Ok(response(value)),
            ActivationOutcome::NotFound => Err(ApiError::not_found("Theme chưa được cài đặt.")),
            ActivationOutcome::Stale => Err(ApiError::precondition_failed(
                "Theme đã thay đổi ở phiên khác (412). Tải lại trạng thái trước khi thử tiếp.",
            )),
        },
        (None, Some(revision_id)) => {
            let rev = find_revision_by_id(&db, &revision_id)
                .await?
                .ok_or_else(|| ApiError::not_found("Revision theme chưa được cài đặt."))?;
            if manifest_has_declarations(&rev.manifest_json) {
                return Err(ApiError(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "unsupported_declarations_pending_cnt_bridge: Theme chứa declarations (CPT/fields/taxonomy) chưa thể kích hoạt trước M5-S10-T01.".into(),
                ));
            }
            match theme::activate_revision(&db, &themes_root.0, &revision_id, Some(version))
                .await?
            {
                RevisionActivationOutcome::Activated(selection) => {
                    Ok((headers(Some(selection.version)), Json(selection)).into_response())
                }
                RevisionActivationOutcome::NotFound => {
                    Err(ApiError::not_found("Revision theme chưa được cài đặt."))
                }
                RevisionActivationOutcome::MissingExpectedVersion => {
                    Err(ApiError::precondition_required("Cần If-Match để kích hoạt theme."))
                }
                RevisionActivationOutcome::Stale => Err(ApiError::precondition_failed(
                    "Theme đã thay đổi ở phiên khác (412). Tải lại trạng thái trước khi thử tiếp.",
                )),
                RevisionActivationOutcome::CorruptRevision(_) => Err(ApiError::conflict(
                    "Revision trên đĩa bị thiếu hoặc không khớp .complete/digest.",
                )),
            }
        }
        _ => Err(ApiError::bad_request(
            "Cần cung cấp đúng một trong hai trường id hoặc revision_id.",
        )),
    }
}

pub async fn theme_asset(
    State(db): State<SqlitePool>,
    Extension(themes_root): Extension<ThemeStoreRoot>,
    Path((revision_digest, rel_path)): Path<(String, String)>,
) -> Response {
    if revision_digest.len() != 64
        || !revision_digest
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Ok(safe_rel) = validate_v2_relative_path(&rel_path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(mime) = theme_asset_mime(&safe_rel) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let Ok(Some(rev)) = find_revision_by_digest(&db, &revision_digest).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
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

    (
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
        .into_response()
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
