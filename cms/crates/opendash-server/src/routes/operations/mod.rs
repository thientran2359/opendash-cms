mod backup;
mod restore_and_maintenance;
mod types;

pub use backup::*;
pub use restore_and_maintenance::*;
pub use types::*;

pub async fn upload_restore_archive(
    State(state): State<AppState>,
    headers: HeaderMap,
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    body: axum::body::Body,
) -> ApiResult<impl IntoResponse> {
    upload_restore_archive_with_timeout(state, headers, body, std::time::Duration::from_secs(300))
        .await
}

pub async fn upload_restore_archive_with_timeout(
    state: AppState,
    headers: HeaderMap,
    mut body: axum::body::Body,
    upload_timeout: std::time::Duration,
) -> ApiResult<impl IntoResponse> {
    use sha2::{Digest, Sha256};
    use std::io::Write;

    require_admin_with_csrf(&headers, &state.db).await?;
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .eq_ignore_ascii_case("application/octet-stream")
    {
        return Err(ApiError(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Chỉ chấp nhận application/octet-stream cho gói restore.".into(),
        ));
    }

    if let Some(len) = headers
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        && len > crate::backup_archive::MAX_ENCRYPTED_ARCHIVE_BYTES
    {
        return Err(ApiError(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Kích thước gói backup vượt quá giới hạn cho phép.".into(),
        ));
    }

    let upload_id = crate::auth::generate_random_token();
    let uploads_dir = state
        .coordinator
        .paths()
        .backups_staging_dir()
        .join("restore-uploads");
    crate::secrets::ensure_private_directory_tree(&uploads_dir)?;

    let part_path = uploads_dir.join(format!(".{upload_id}.part"));
    let final_path = uploads_dir.join(format!("{upload_id}.odbackup.age"));
    let mut file = crate::secrets::create_private_file(&part_path)?;
    let mut hasher = Sha256::new();
    let mut total_bytes: u64 = 0;

    let upload_deadline = tokio::time::Instant::now() + upload_timeout;
    loop {
        let frame_res = tokio::time::timeout_at(
            upload_deadline,
            std::future::poll_fn(|cx| {
                <axum::body::Body as http_body::Body>::poll_frame(std::pin::Pin::new(&mut body), cx)
            }),
        )
        .await;
        let frame_opt = match frame_res {
            Ok(opt) => opt,
            Err(_) => {
                drop(file);
                let _ = std::fs::remove_file(&part_path);
                return Err(ApiError(
                    StatusCode::REQUEST_TIMEOUT,
                    "Quá thời gian tải lên gói restore.".into(),
                ));
            }
        };
        match frame_opt {
            Some(Ok(frame)) => {
                if let Ok(data) = frame.into_data() {
                    let next_total = total_bytes.saturating_add(data.len() as u64);
                    if next_total > crate::backup_archive::MAX_ENCRYPTED_ARCHIVE_BYTES {
                        drop(file);
                        let _ = std::fs::remove_file(&part_path);
                        return Err(ApiError(
                            StatusCode::PAYLOAD_TOO_LARGE,
                            "Kích thước gói backup vượt quá giới hạn cho phép.".into(),
                        ));
                    }
                    for chunk in data.chunks(64 * 1024) {
                        if file.write_all(chunk).is_err() {
                            drop(file);
                            let _ = std::fs::remove_file(&part_path);
                            return Err(ApiError::internal("Không ghi được tệp upload restore."));
                        }
                        hasher.update(chunk);
                    }
                    total_bytes = next_total;
                }
            }
            Some(Err(_)) => {
                drop(file);
                let _ = std::fs::remove_file(&part_path);
                return Err(ApiError::bad_request("Luồng upload bị gián đoạn."));
            }
            None => break,
        }
    }

    if total_bytes == 0 {
        drop(file);
        let _ = std::fs::remove_file(&part_path);
        return Err(ApiError::bad_request("Tệp backup rỗng."));
    }
    if file.sync_all().is_err() {
        drop(file);
        let _ = std::fs::remove_file(&part_path);
        return Err(ApiError::internal("Không đồng bộ được tệp upload restore."));
    }
    drop(file);
    std::fs::rename(&part_path, &final_path).map_err(|_| {
        let _ = std::fs::remove_file(&part_path);
        ApiError::internal("Không hoàn tất được tệp upload restore.")
    })?;

    let sha256: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok((
        StatusCode::CREATED,
        Json(RestoreUploadResponse {
            upload_id,
            size_bytes: total_bytes,
            sha256,
        }),
    ))
}

#[cfg(test)]
#[path = "tests/tests.rs"]
mod tests;
