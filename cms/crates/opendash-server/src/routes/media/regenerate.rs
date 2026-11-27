//! Cooperative multi-batch responsive variant status and regeneration routes.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{
    auth::{require_admin, require_admin_with_csrf},
    error::{ApiError, ApiResult},
    media::{
        DECODE_SLOTS, MediaStore, decode_validated_image, generate_variants_for_decoded,
        write_atomic_overwrite,
    },
    operations::SiteCoordinator,
    storage::{
        media::{self as media_storage, MediaVariantsStatus},
        operations as ops_storage,
    },
};

static MEDIA_REGENERATE_ACTIVE: AtomicBool = AtomicBool::new(false);

struct RegenerateRunLockGuard;

impl Drop for RegenerateRunLockGuard {
    fn drop(&mut self) {
        MEDIA_REGENERATE_ACTIVE.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegenerateVariantsRequest {
    pub after_id: Option<String>,
    pub batch_size: Option<u32>,
    pub force: Option<bool>,
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegenerateCumulativeStats {
    pub total_processed: i64,
    pub total_succeeded: i64,
    pub total_failed: i64,
    pub total_skipped_concurrent: i64,
    pub last_after_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegenerateVariantsResponse {
    pub run_id: String,
    pub batch_processed: i64,
    pub batch_succeeded: i64,
    pub batch_failed: i64,
    pub batch_skipped_concurrent: i64,
    pub failed_ids: Vec<String>,
    pub remaining: i64,
    pub next_after_id: Option<String>,
    pub done: bool,
    pub cumulative: RegenerateCumulativeStats,
}

pub async fn media_variants_status(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
) -> ApiResult<Json<MediaVariantsStatus>> {
    require_admin(&headers, &db).await?;
    Ok(Json(media_storage::get_variants_status(&db).await?))
}

pub async fn media_variants_regenerate(
    State(db): State<SqlitePool>,
    State(store): State<Arc<MediaStore>>,
    State(coordinator): State<Arc<SiteCoordinator>>,
    headers: HeaderMap,
    bytes: Bytes,
) -> ApiResult<Response> {
    let actor = require_admin_with_csrf(&headers, &db).await?;
    if MEDIA_REGENERATE_ACTIVE
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(ApiError::conflict(
            "Đang có tiến trình tái tạo biến thể media đang chạy.",
        ));
    }
    let _lock_guard = RegenerateRunLockGuard;

    let req: RegenerateVariantsRequest = if bytes.iter().all(|b| b.is_ascii_whitespace()) {
        RegenerateVariantsRequest::default()
    } else {
        serde_json::from_slice(&bytes)
            .map_err(|_| ApiError::bad_request("Dữ liệu JSON không hợp lệ."))?
    };
    if let Some(ref after) = req.after_id
        && !media_storage::valid_media_id(after)
    {
        return Err(ApiError::bad_request("Tham số after_id không hợp lệ."));
    }
    let batch_size = i64::from(req.batch_size.unwrap_or(15).clamp(1, 25));
    let force = req.force.unwrap_or(false);

    let mut job = match req.run_id {
        Some(raw_run_id) => {
            if !ops_storage::is_valid_job_id(&raw_run_id) {
                return Err(ApiError::bad_request("Tham số run_id không hợp lệ."));
            }
            drop(coordinator.admit_mutation().map_err(|_| {
                ApiError::service_unavailable("Hệ thống đang bảo trì; tạm dừng tái tạo biến thể.")
            })?);
            let existing = ops_storage::get_job(&db, &raw_run_id)
                .await?
                .ok_or_else(|| ApiError::not_found("Không tìm thấy tác vụ regenerate."))?;
            if existing.kind != ops_storage::OperationKind::MediaRegenerate {
                return Err(ApiError::bad_request(
                    "run_id không thuộc loại tác vụ media_regenerate.",
                ));
            }
            if existing.state != ops_storage::OperationState::Running {
                return Err(ApiError::conflict(
                    "regenerate_run_not_active: Tác vụ tái tạo biến thể đã kết thúc hoặc bị hủy.",
                ));
            }
            existing
        }
        None => {
            let _init_lease = coordinator.admit_mutation().map_err(|_| {
                ApiError::service_unavailable("Hệ thống đang bảo trì; tạm dừng tái tạo biến thể.")
            })?;
            let new_id = crate::auth::generate_random_token();
            let created = ops_storage::create_job(
                &db,
                ops_storage::JobCreateInput {
                    id: new_id,
                    kind: ops_storage::OperationKind::MediaRegenerate,
                    actor_user_id: Some(actor.user.id.to_string()),
                    reason: Some("media_variants_regenerate".to_string()),
                },
            )
            .await?;
            let initial_summary = serde_json::to_string(&RegenerateCumulativeStats::default())
                .map_err(|_| ApiError::internal("Không tuần tự hóa được summary regenerate."))?;
            ops_storage::transition_job_cas(
                &db,
                &created.id,
                ops_storage::JobTransitionInput {
                    expected_version: created.version,
                    next_state: ops_storage::OperationState::Running,
                    summary_json: Some(initial_summary),
                    error_code: None,
                    error_message: None,
                },
            )
            .await?
        }
    };

    let mut cumulative: RegenerateCumulativeStats = job
        .summary_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    let candidates =
        media_storage::list_images_for_regenerate(&db, req.after_id.as_deref(), batch_size, force)
            .await?;

    let started = Instant::now();
    let max_batch_duration = Duration::from_secs(15);
    let mut batch_processed: i64 = 0;
    let mut batch_succeeded: i64 = 0;
    let mut batch_failed: i64 = 0;
    let mut batch_skipped_concurrent: i64 = 0;
    let mut failed_ids = Vec::new();
    let mut last_cursor = req.after_id.clone();
    let mut maintenance_interrupted = false;

    for asset in candidates {
        if batch_processed > 0 && started.elapsed() >= max_batch_duration {
            break;
        }
        let Ok(read_lease) = store.acquire_read_lease(&asset.id).await else {
            last_cursor = Some(asset.id.clone());
            batch_processed += 1;
            batch_skipped_concurrent += 1;
            continue;
        };
        let orig_res = store
            .read_with_lease(&read_lease, &asset.id, &asset.extension, false)
            .await;
        let thumb_len = store
            .metadata_with_lease(&read_lease, &asset.id, &asset.extension, true)
            .await
            .ok()
            .and_then(|n| usize::try_from(n).ok());
        drop(read_lease);

        let Ok(orig_bytes) = orig_res else {
            last_cursor = Some(asset.id.clone());
            batch_processed += 1;
            batch_failed += 1;
            failed_ids.push(asset.id);
            continue;
        };

        let Ok(_slot) = DECODE_SLOTS.acquire().await else {
            return Err(ApiError::internal("Bộ xử lý ảnh không khả dụng."));
        };
        let mime_owned = asset.mime_type.clone();
        let encoded_res = tokio::task::spawn_blocking(move || -> ApiResult<_> {
            let (img, _fmt, _ext, mime) = decode_validated_image(&orig_bytes, &mime_owned)?;
            let thumb = img.thumbnail(img.width().min(320), img.height().min(320));
            let effective_thumb_len = match thumb_len {
                Some(len) if len > 0 => len,
                _ => {
                    let mut cur = std::io::Cursor::new(Vec::new());
                    thumb
                        .write_to(&mut cur, image::ImageFormat::Png)
                        .map_err(|_| ApiError::internal("Không tạo được ảnh thumbnail tạm."))?;
                    cur.into_inner().len()
                }
            };
            generate_variants_for_decoded(&img, mime, orig_bytes.len(), &thumb, effective_thumb_len)
        })
        .await
        .map_err(|_| ApiError::internal("Không thể xử lý lại biến thể ảnh."))?;

        let Ok(generated) = encoded_res else {
            last_cursor = Some(asset.id.clone());
            batch_processed += 1;
            batch_failed += 1;
            failed_ids.push(asset.id);
            continue;
        };

        let Ok(_lease) = coordinator.admit_mutation() else {
            maintenance_interrupted = true;
            break;
        };

        let mut written_paths: Vec<PathBuf> = Vec::new();
        let mut write_failed = false;
        for v in &generated {
            let Ok(dst) = store.variant_path(&asset.id, asset.version, v.spec, v.format) else {
                write_failed = true;
                break;
            };
            if write_atomic_overwrite(&dst, &v.bytes).await.is_err() {
                write_failed = true;
                break;
            }
            written_paths.push(dst);
        }
        if write_failed {
            MediaStore::remove_paths_best_effort(&written_paths).await;
            last_cursor = Some(asset.id.clone());
            batch_processed += 1;
            batch_failed += 1;
            failed_ids.push(asset.id);
            continue;
        }

        let committed = media_storage::commit_regenerated_variants_cas(
            &db,
            &asset.id,
            asset.generation,
            asset.version,
            &generated,
        )
        .await?;

        last_cursor = Some(asset.id.clone());
        batch_processed += 1;
        if committed {
            batch_succeeded += 1;
        } else {
            MediaStore::remove_paths_best_effort(&written_paths).await;
            batch_skipped_concurrent += 1;
        }
    }

    let remaining =
        media_storage::count_remaining_for_regenerate(&db, last_cursor.as_deref(), force).await?;
    let done = remaining == 0;
    let next_after_id = if done { None } else { last_cursor.clone() };

    cumulative.total_processed += batch_processed;
    cumulative.total_succeeded += batch_succeeded;
    cumulative.total_failed += batch_failed;
    cumulative.total_skipped_concurrent += batch_skipped_concurrent;
    cumulative.last_after_id = next_after_id.clone();

    let finish_lease = if maintenance_interrupted {
        None
    } else {
        coordinator.admit_mutation().ok()
    };
    let Some(_finish_lease) = finish_lease else {
        if batch_processed == 0 {
            return Err(ApiError::service_unavailable(
                "Hệ thống đang bảo trì; tạm dừng tái tạo biến thể.",
            ));
        }
        return Ok((
            StatusCode::OK,
            [(header::CACHE_CONTROL, "no-store")],
            Json(RegenerateVariantsResponse {
                run_id: job.id,
                batch_processed,
                batch_succeeded,
                batch_failed,
                batch_skipped_concurrent,
                failed_ids,
                remaining,
                next_after_id: last_cursor,
                done: false,
                cumulative,
            }),
        )
            .into_response());
    };

    let latest_job = ops_storage::get_job(&db, &job.id)
        .await?
        .unwrap_or_else(|| job.clone());
    if latest_job.state != ops_storage::OperationState::Running {
        return Err(ApiError::conflict(
            "regenerate_run_not_active: Tác vụ tái tạo biến thể đã bị hủy hoặc thay đổi trạng thái.",
        ));
    }
    let next_state = if done {
        ops_storage::OperationState::Succeeded
    } else {
        ops_storage::OperationState::Running
    };
    let summary_json = serde_json::to_string(&cumulative)
        .map_err(|_| ApiError::internal("Không tuần tự hóa được tiến độ regenerate."))?;
    job = ops_storage::transition_job_cas(
        &db,
        &job.id,
        ops_storage::JobTransitionInput {
            expected_version: latest_job.version,
            next_state,
            summary_json: Some(summary_json),
            error_code: None,
            error_message: None,
        },
    )
    .await?;
    drop(_finish_lease);

    Ok((
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(RegenerateVariantsResponse {
            run_id: job.id,
            batch_processed,
            batch_succeeded,
            batch_failed,
            batch_skipped_concurrent,
            failed_ids,
            remaining,
            next_after_id,
            done,
            cumulative,
        }),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::SitePaths;
    use crate::storage::{auth as auth_storage, init_db};

    #[tokio::test]
    async fn regenerate_blocked_by_active_maintenance_writes_zero_jobs() {
        let base_tmp =
            std::fs::canonicalize(std::env::temp_dir()).unwrap_or_else(|_| std::env::temp_dir());
        let root = base_tmp.join(format!("opendash-regen-maint-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let site_root = root.join("site");
        let db_path = root.join("cms.sqlite");
        std::fs::create_dir_all(&site_root).expect("site_root");

        let paths = SitePaths::from_parts(site_root, db_path);
        std::fs::create_dir_all(paths.uploads_dir()).expect("uploads dir");
        std::fs::create_dir_all(paths.runtime_dir()).expect("runtime dir");
        let coordinator = Arc::new(SiteCoordinator::acquire(paths.clone()).expect("coordinator"));
        let db = init_db(paths.db_path()).await.expect("init_db");
        let store = Arc::new(MediaStore::new(paths.uploads_dir()));

        auth_storage::setup_initial_admin(&db, "regenadmin", "$argon2id$stub")
            .await
            .expect("setup admin");
        let (uid, _, _, _, ver) = auth_storage::find_user_by_username(&db, "regenadmin")
            .await
            .expect("find admin")
            .expect("admin exists");
        let sess = crate::auth::generate_random_token();
        let csrf = crate::auth::generate_random_token();
        assert!(
            auth_storage::insert_session(
                &db,
                &crate::auth::hash_token(&sess),
                uid,
                &csrf,
                crate::auth::current_time_secs() + 3600,
                ver,
            )
            .await
            .expect("insert session")
        );

        let _maint = coordinator
            .begin_maintenance(ops_storage::OperationKind::Backup, Duration::from_secs(2))
            .await
            .expect("begin maintenance");

        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("{}={}", crate::auth::SESSION_COOKIE_NAME, sess)
                .parse()
                .unwrap(),
        );
        headers.insert("x-csrf-token", csrf.parse().unwrap());

        let err = media_variants_regenerate(
            State(db.clone()),
            State(store),
            State(coordinator.clone()),
            headers,
            Bytes::from_static(b"{}"),
        )
        .await
        .expect_err("maintenance must reject regenerate with 503");
        assert_eq!(err.0, StatusCode::SERVICE_UNAVAILABLE);

        let jobs = ops_storage::list_jobs(&db, 50).await.expect("list jobs");
        assert!(jobs.is_empty(), "expected 0 operation_jobs rows: {jobs:?}");

        let _ = std::fs::remove_dir_all(&root);
    }
}
