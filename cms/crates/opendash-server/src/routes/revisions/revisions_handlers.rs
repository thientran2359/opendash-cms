use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State, rejection::QueryRejection},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use sqlx::SqlitePool;
use std::sync::Arc;

use crate::auth::{require_session, require_session_with_csrf};
use crate::error::{ApiError, ApiResult};
use crate::hooks::HookDispatcher;
use crate::routes::entries::expected_version;
use crate::routes::settings::no_store_headers;
use crate::storage::revisions::{
    CompareTarget, RestoreRevisionOutcome, RevisionRestoreRequest, compare_entry_revisions,
    get_entry_revision_filtered, list_entry_revisions, restore_entry_revision,
};

use super::{
    guard_type_key_access, map_restore_error, parse_positive_id, require_json_content_type,
};

type RawQueryPairs = Result<Query<Vec<(String, String)>>, QueryRejection>;

pub(super) fn parse_list_pairs(pairs: Vec<(String, String)>) -> ApiResult<(i64, i64)> {
    let mut limit: Option<i64> = None;
    let mut offset: Option<i64> = None;
    for (k, v) in pairs {
        match k.as_str() {
            "limit" => {
                if limit.is_some() {
                    return Err(ApiError::bad_request(
                        "Tham số limit bị trùng lặp trong truy vấn.",
                    ));
                }
                let parsed = v.parse::<i64>().map_err(|_| {
                    ApiError::bad_request("Tham số limit phải là số nguyên 1..=25.")
                })?;
                if !(1..=25).contains(&parsed) {
                    return Err(ApiError::bad_request(
                        "Tham số limit phải nằm trong khoảng 1..=25.",
                    ));
                }
                limit = Some(parsed);
            }
            "offset" => {
                if offset.is_some() {
                    return Err(ApiError::bad_request(
                        "Tham số offset bị trùng lặp trong truy vấn.",
                    ));
                }
                let parsed = v.parse::<i64>().map_err(|_| {
                    ApiError::bad_request("Tham số offset phải là số nguyên 0..=1000.")
                })?;
                if !(0..=1000).contains(&parsed) {
                    return Err(ApiError::bad_request(
                        "Tham số offset phải nằm trong khoảng 0..=1000.",
                    ));
                }
                offset = Some(parsed);
            }
            _ => {
                return Err(ApiError::bad_request(
                    "Tham số truy vấn lịch sử phiên bản không hợp lệ.",
                ));
            }
        }
    }
    Ok((limit.unwrap_or(25), offset.unwrap_or(0)))
}

fn parse_compare_target(raw: &str) -> ApiResult<CompareTarget> {
    match raw {
        "current" => Ok(CompareTarget::Current),
        "autosave" => Ok(CompareTarget::Autosave),
        _ => parse_positive_id(raw, "Tham số so sánh from/to không hợp lệ.")
            .map(|rev_id| CompareTarget::Revision { rev_id }),
    }
}

pub(super) fn parse_compare_pairs(
    pairs: Vec<(String, String)>,
) -> ApiResult<(CompareTarget, CompareTarget)> {
    let mut from: Option<CompareTarget> = None;
    let mut to: Option<CompareTarget> = None;
    for (k, v) in pairs {
        match k.as_str() {
            "from" => {
                if from.is_some() {
                    return Err(ApiError::bad_request("Tham số from bị trùng lặp."));
                }
                from = Some(parse_compare_target(&v)?);
            }
            "to" => {
                if to.is_some() {
                    return Err(ApiError::bad_request("Tham số to bị trùng lặp."));
                }
                to = Some(parse_compare_target(&v)?);
            }
            _ => {
                return Err(ApiError::bad_request(
                    "Tham số truy vấn so sánh phiên bản không hợp lệ.",
                ));
            }
        }
    }
    match (from, to) {
        (Some(f), Some(t)) => Ok((f, t)),
        _ => Err(ApiError::bad_request(
            "Cần cung cấp đầy đủ hai tham số so sánh from và to.",
        )),
    }
}

/// Lists immutable revision summaries for an entry in reverse chronological order.
pub async fn revisions_list(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, raw_id)): Path<(String, String)>,
    query: RawQueryPairs,
) -> ApiResult<Response> {
    let actor = require_session(&headers, &db).await?;
    guard_type_key_access(&type_key, actor.user.role)?;
    let entry_id = parse_positive_id(&raw_id, "ID nội dung không hợp lệ.")?;
    let pairs = query
        .map_err(|_| ApiError::bad_request("Tham số truy vấn lịch sử phiên bản không hợp lệ."))?
        .0;
    let (limit, offset) = parse_list_pairs(pairs)?;
    let resp = list_entry_revisions(&db, &actor, &type_key, entry_id, limit, offset).await?;
    let current_version = resp.current_version;
    Ok((no_store_headers(Some(current_version)), Json(resp)).into_response())
}

/// Compares any two targets (`rev_id`, `current`, `autosave`) for an entry with role scoping.
pub async fn revisions_compare(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, raw_id)): Path<(String, String)>,
    query: RawQueryPairs,
) -> ApiResult<Response> {
    let actor = require_session(&headers, &db).await?;
    guard_type_key_access(&type_key, actor.user.role)?;
    let entry_id = parse_positive_id(&raw_id, "ID nội dung không hợp lệ.")?;
    let pairs = query
        .map_err(|_| ApiError::bad_request("Tham số truy vấn so sánh phiên bản không hợp lệ."))?
        .0;
    let (from, to) = parse_compare_pairs(pairs)?;
    let resp = compare_entry_revisions(&db, &actor, &type_key, entry_id, from, to).await?;
    Ok((no_store_headers(None), Json(resp)).into_response())
}

/// Returns a role-filtered immutable revision detail snapshot and missing-ref report.
pub async fn revision_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, raw_id, raw_rev_id)): Path<(String, String, String)>,
) -> ApiResult<Response> {
    let actor = require_session(&headers, &db).await?;
    guard_type_key_access(&type_key, actor.user.role)?;
    let entry_id = parse_positive_id(&raw_id, "ID nội dung không hợp lệ.")?;
    let rev_id = parse_positive_id(&raw_rev_id, "ID phiên bản lịch sử không hợp lệ.")?;
    let resp = get_entry_revision_filtered(&db, &actor, &type_key, entry_id, rev_id).await?;
    Ok((no_store_headers(None), Json(resp)).into_response())
}

/// Restores an immutable revision via 4-phase preflight, hook check, CAS write, and side effects.
pub async fn revision_restore(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((type_key, raw_id, raw_rev_id)): Path<(String, String, String)>,
    bytes: Bytes,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    guard_type_key_access(&type_key, actor.user.role)?;
    let entry_id = parse_positive_id(&raw_id, "ID nội dung không hợp lệ.")?;
    let rev_id = parse_positive_id(&raw_rev_id, "ID phiên bản lịch sử không hợp lệ.")?;
    require_json_content_type(&headers)?;
    let req: RevisionRestoreRequest = serde_json::from_slice(&bytes)
        .map_err(|_| ApiError::bad_request("Dữ liệu khôi phục JSON không hợp lệ."))?;
    let ver = expected_version(&headers)?;

    match restore_entry_revision(
        &db,
        &hooks,
        &actor,
        &type_key,
        entry_id,
        rev_id,
        ver,
        req.acknowledge_missing_refs,
        lease,
    )
    .await
    {
        Ok(outcome) => {
            let new_version = match &outcome {
                RestoreRevisionOutcome::Entry { entry, .. } => entry.version,
                RestoreRevisionOutcome::Pattern { version, .. } => *version,
            };
            Ok((no_store_headers(Some(new_version)), Json(outcome)).into_response())
        }
        Err(err) => Ok(map_restore_error(err)),
    }
}
