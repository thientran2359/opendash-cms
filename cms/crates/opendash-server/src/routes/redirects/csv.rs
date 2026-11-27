use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use opendash_core::redirects::{
    CsvImportRequest, CsvImportResponse, CsvPreviewRow, format_redirects_csv,
};
use sqlx::SqlitePool;

use super::{extract_public_origin_from_headers, map_csv_json_rejection, map_storage_error};
use crate::{
    auth::{require_admin, require_admin_with_csrf},
    error::ApiResult,
    i18n::{
        PrelocalizedErrorResponse, effective_locale_for_error, translate_api_message_with_registry,
    },
    operations::AdmittedMutation,
    redirects::RedirectTelemetryBuffer,
    routes::settings::no_store_headers,
    state::AppState,
    storage::redirects::{export_manual_redirects_csv, import_manual_redirects_csv},
};

pub async fn import_redirects(
    AdmittedMutation(_lease): AdmittedMutation,
    State(state): State<AppState>,
    telemetry: Option<Extension<Arc<RedirectTelemetryBuffer>>>,
    headers: HeaderMap,
    payload: Result<Json<CsvImportRequest>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &state.db).await?;
    let Json(req) = payload.map_err(map_csv_json_rejection)?;
    let public_origin = extract_public_origin_from_headers(&headers);

    let exec = if let Some(Extension(buf)) = telemetry {
        if !req.dry_run {
            let flush_guard = buf.lock_flush().await;
            let res = import_manual_redirects_csv(
                &state.db,
                &req.csv_text,
                false,
                req.conflict_policy,
                public_origin.as_deref(),
            )
            .await
            .map_err(map_storage_error)?;
            if !res.applied_paths.is_empty() {
                buf.evict_not_found_paths_locked(&flush_guard, &res.applied_paths);
            }
            res
        } else {
            import_manual_redirects_csv(
                &state.db,
                &req.csv_text,
                true,
                req.conflict_policy,
                public_origin.as_deref(),
            )
            .await
            .map_err(map_storage_error)?
        }
    } else {
        import_manual_redirects_csv(
            &state.db,
            &req.csv_text,
            req.dry_run,
            req.conflict_policy,
            public_origin.as_deref(),
        )
        .await
        .map_err(map_storage_error)?
    };

    let locale = effective_locale_for_error(&state.db, &state.ui_locale).await;
    let reg = Some(state.catalog_registry.as_ref());
    let rows: Vec<CsvPreviewRow> = exec
        .rows
        .into_iter()
        .map(|row| {
            let localized_err = row.error.map(|err| {
                translate_api_message_with_registry(err.canonical_vi_message(), &locale, reg)
            });
            CsvPreviewRow {
                row_number: row.row_number,
                source_path: row.source_path,
                target_url: row.target_url,
                status_code: row.status_code,
                enabled: row.enabled,
                note: row.note,
                status: row.status,
                error_code: localized_err.as_ref().map(|l| l.code.to_owned()),
                error: localized_err.map(|l| l.message),
            }
        })
        .collect();

    let truncated = exec.total_rows > rows.len();
    if !exec.dry_run && exec.error_count > 0 {
        let (http_status, top_code, top_raw_vi) = if exec.conflicts_only {
            (
                StatusCode::CONFLICT,
                "redirects.csv_import_conflicts",
                "Nhập CSV thất bại: có đường dẫn nguồn đã tồn tại.",
            )
        } else {
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                "redirects.csv_import_validation_failed",
                "Nhập CSV thất bại: có dòng không hợp lệ hoặc tạo vòng lặp.",
            )
        };
        let top_loc = translate_api_message_with_registry(top_raw_vi, &locale, reg);
        let body = CsvImportResponse {
            dry_run: false,
            total_rows: exec.total_rows,
            create_count: exec.create_count,
            update_count: exec.update_count,
            skip_count: exec.skip_count,
            error_count: exec.error_count,
            truncated,
            rows,
            error: Some(top_loc.message),
            code: Some(top_code.to_owned()),
        };
        let mut response = (http_status, no_store_headers(None), Json(body)).into_response();
        response.headers_mut().insert(
            HeaderName::from_static("x-opendash-error-code"),
            HeaderValue::from_static(top_code),
        );
        response.extensions_mut().insert(PrelocalizedErrorResponse);
        return Ok(response);
    }

    let body = CsvImportResponse {
        dry_run: exec.dry_run,
        total_rows: exec.total_rows,
        create_count: exec.create_count,
        update_count: exec.update_count,
        skip_count: exec.skip_count,
        error_count: exec.error_count,
        truncated,
        rows,
        error: None,
        code: None,
    };
    Ok((StatusCode::OK, no_store_headers(None), Json(body)).into_response())
}

pub async fn export_redirects(
    State(db): State<SqlitePool>,
    telemetry: Option<Extension<Arc<RedirectTelemetryBuffer>>>,
    headers: HeaderMap,
) -> ApiResult<impl IntoResponse> {
    require_admin(&headers, &db).await?;
    if let Some(Extension(buf)) = telemetry {
        let _ = buf.flush_now().await;
    }
    let rows = export_manual_redirects_csv(&db)
        .await
        .map_err(map_storage_error)?;
    let csv = format_redirects_csv(&rows);
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"opendash-redirects.csv\"",
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        csv,
    ))
}
