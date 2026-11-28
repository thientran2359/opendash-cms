//! Admin-only atomic in-place media replace handler (`PUT /api/admin/media/{id}/file`).

use std::{pin::Pin, sync::Arc};

use axum::{
    Json,
    body::{Body, HttpBody},
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tokio::io::AsyncWriteExt;

use super::upload_and_read::display_filename;
use crate::{
    auth::require_admin_with_csrf,
    error::{ApiError, ApiResult},
    media::{
        DECODE_SLOTS, MAX_MEDIA_BYTES, MediaStore, ReplaceStagingGuard, StagedReplaceContent,
        StagedReplacePayload, VIDEO_INGEST_SLOTS, decode_image, execute_replace_swap_and_commit,
    },
    media_video::{MAX_VIDEO_BYTES, inspect_mp4},
    operations::SiteCoordinator,
    storage::media as media_storage,
};

pub async fn media_replace(
    State(db): State<SqlitePool>,
    State(store): State<Arc<MediaStore>>,
    State(coordinator): State<Arc<SiteCoordinator>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    mut body: Body,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let (expected_gen, expected_ver) = crate::media_lifecycle::parse_media_if_match(&headers)?;
    if !media_storage::valid_media_id(&id) {
        return Err(ApiError::bad_request("Mã media không hợp lệ."));
    }

    let current = media_storage::find(&db, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy media."))?;
    if current.generation != expected_gen || current.version != expected_ver {
        return Err(ApiError::precondition_failed(
            "Phiên bản hoặc generation của media đã thay đổi.",
        ));
    }

    let declared_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(';').next())
        .map(str::trim)
        .unwrap_or("")
        .to_ascii_lowercase();
    if current.kind == "image" && !declared_type.starts_with("image/") {
        return Err(ApiError::unprocessable(
            "Không thể thay thế ảnh bằng tệp không phải định dạng ảnh.",
        ));
    }
    if current.kind == "video" && declared_type != "video/mp4" {
        return Err(ApiError::unprocessable(
            "Tệp thay thế cho video bắt buộc phải là video/mp4.",
        ));
    }

    let max_bytes = if current.kind == "image" {
        MAX_MEDIA_BYTES
    } else {
        MAX_VIDEO_BYTES
    };
    if let Some(len) = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok())
    {
        if len == 0 {
            return Err(ApiError::bad_request("Tệp thay thế rỗng (0 byte)."));
        }
        if len > max_bytes {
            return Err(ApiError(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Tệp thay thế vượt quá dung lượng tối đa cho phép.".into(),
            ));
        }
    }

    let job_id = crate::auth::generate_random_token();
    let stage_dir = store.create_replace_staging_dir(&job_id).await?;
    let stage_guard = ReplaceStagingGuard::new(stage_dir.clone());

    let content = if current.kind == "image" {
        let mut buf = Vec::new();
        while let Some(frame_res) =
            std::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)).await
        {
            let frame = frame_res
                .map_err(|_| ApiError::bad_request("Luồng tải lên tệp thay thế bị gián đoạn."))?;
            if let Some(chunk) = frame.data_ref() {
                if buf.len().saturating_add(chunk.len()) > max_bytes {
                    return Err(ApiError(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "Ảnh thay thế vượt quá giới hạn 8 MiB.".into(),
                    ));
                }
                buf.extend_from_slice(chunk);
            }
        }
        if buf.is_empty() {
            return Err(ApiError::bad_request("Ảnh thay thế rỗng (0 byte)."));
        }
        let _slot = DECODE_SLOTS
            .acquire()
            .await
            .map_err(|_| ApiError::internal("Bộ xử lý ảnh không khả dụng."))?;
        let mime_owned = declared_type.clone();
        let decoded = tokio::task::spawn_blocking(move || decode_image(buf, &mime_owned))
            .await
            .map_err(|_| ApiError::internal("Không thể giải mã ảnh thay thế."))??;
        let variants = store
            .stage_replace_image_bundle(&stage_dir, &id, expected_ver + 1, &decoded)
            .await?;
        let filename = display_filename(&headers, decoded.extension);
        StagedReplaceContent::Image {
            decoded,
            filename,
            variants,
        }
    } else {
        let _slot = VIDEO_INGEST_SLOTS.try_acquire().map_err(|_| {
            ApiError::too_many_requests("Đang có quá nhiều lượt xử lý video đồng thời.")
        })?;
        let video_stage_path = stage_dir.join("canonical.mp4");
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&video_stage_path)
            .await
            .map_err(|_| ApiError::internal("Không mở được tệp tạm để thay thế video."))?;
        let mut hasher = Sha256::new();
        let mut total_bytes: u64 = 0;
        while let Some(frame_res) =
            std::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)).await
        {
            let frame = frame_res
                .map_err(|_| ApiError::bad_request("Luồng tải lên video thay thế bị gián đoạn."))?;
            if let Some(chunk) = frame.data_ref() {
                let next_total = total_bytes
                    .checked_add(chunk.len() as u64)
                    .ok_or_else(|| ApiError::bad_request("Kích thước video vượt giới hạn."))?;
                if next_total > MAX_VIDEO_BYTES as u64 {
                    return Err(ApiError(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "Video thay thế vượt quá giới hạn 100 MiB.".into(),
                    ));
                }
                hasher.update(chunk);
                file.write_all(chunk)
                    .await
                    .map_err(|_| ApiError::internal("Không ghi được dữ liệu video tạm."))?;
                total_bytes = next_total;
            }
        }
        if total_bytes == 0 {
            return Err(ApiError::bad_request("Video thay thế rỗng (0 byte)."));
        }
        file.sync_all()
            .await
            .map_err(|_| ApiError::internal("Không đồng bộ được tệp video tạm."))?;
        drop(file);

        let sha256 = format!("{:x}", hasher.finalize());
        let inspect_path = video_stage_path.clone();
        let validated =
            tokio::task::spawn_blocking(move || inspect_mp4(&inspect_path, total_bytes, sha256))
                .await
                .map_err(|_| ApiError::internal("Không thể kiểm tra video thay thế."))??;
        let filename = display_filename(&headers, "mp4");
        let meta = validated.metadata(filename);
        StagedReplaceContent::Video {
            content_sha256: meta.content_sha256,
            filename: meta.filename,
            mime_type: meta.mime_type,
            extension: meta.extension,
            byte_size: meta.byte_size,
            width: meta.width,
            height: meta.height,
            duration_ms: meta.duration_ms.unwrap_or(1),
            video_codec: meta.video_codec.unwrap_or_else(|| "h264".to_string()),
            audio_codec: meta.audio_codec,
            staged_file_rel: "canonical.mp4".to_string(),
        }
    };

    let details = execute_replace_swap_and_commit(
        &db,
        &store,
        &coordinator,
        &current,
        StagedReplacePayload {
            job_id,
            stage_guard,
            content,
        },
    )
    .await?;

    let etag = details.etag.clone();
    Ok((
        StatusCode::OK,
        [
            (header::ETAG, etag.as_str()),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        Json(details),
    )
        .into_response())
}
