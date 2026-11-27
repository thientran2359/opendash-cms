pub mod csv;
pub mod not_found_logs;
pub mod rules;

use axum::{
    extract::rejection::{JsonRejection, QueryRejection},
    http::{HeaderMap, StatusCode},
};
use opendash_core::redirects::RedirectValidationError;

use crate::{error::ApiError, storage::redirects::RedirectStorageError};

pub const MAX_REDIRECT_RULE_JSON_BYTES: usize = 16 * 1024;
pub const MAX_REDIRECT_CSV_HTTP_BODY_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_NOT_FOUND_CLEAR_JSON_BYTES: usize = 4 * 1024;

pub(super) fn map_storage_error(err: RedirectStorageError) -> ApiError {
    match err {
        RedirectStorageError::Database(e) => ApiError::from(e),
        RedirectStorageError::Json(e) => ApiError::from(e),
        RedirectStorageError::Validation(RedirectValidationError::CsvTooLarge) => ApiError(
            StatusCode::PAYLOAD_TOO_LARGE,
            RedirectValidationError::CsvTooLarge
                .canonical_vi_message()
                .into(),
        ),
        RedirectStorageError::Validation(v) => ApiError::unprocessable(v.canonical_vi_message()),
        RedirectStorageError::Row(r) => ApiError::unprocessable(r.canonical_vi_message()),
        RedirectStorageError::NotFound => {
            ApiError::not_found("Không tìm thấy quy tắc chuyển hướng.")
        }
        RedirectStorageError::PreconditionFailed => ApiError::precondition_failed(
            "Quy tắc chuyển hướng đã bị thay đổi bởi thao tác khác; hãy tải lại.",
        ),
        RedirectStorageError::ConflictSourcePath => {
            ApiError::conflict("Đường dẫn nguồn đã tồn tại trong bảng chuyển hướng.")
        }
        RedirectStorageError::CycleOrChainExceeded => {
            ApiError::unprocessable("Quy tắc tạo vòng lặp chuyển hướng hoặc chuỗi dài hơn 3 bước.")
        }
        RedirectStorageError::QuotaExceeded => {
            ApiError::unprocessable("Đã đạt giới hạn tối đa 20.000 chuyển hướng thủ công.")
        }
        RedirectStorageError::InvalidBulkDeleteIds => ApiError::unprocessable(
            "Danh sách ID cần xóa phải có từ 1 đến 100 số nguyên dương không trùng lặp.",
        ),
        RedirectStorageError::InvalidClearDays => ApiError::unprocessable(
            "Giá trị older_than_days phải nằm trong khoảng 1 đến 3650 ngày.",
        ),
        RedirectStorageError::InvalidFilter => ApiError::bad_request(
            "Tham số truy vấn danh sách chuyển hướng hoặc nhật ký 404 không hợp lệ.",
        ),
    }
}

pub(super) fn map_not_found_storage_error(err: RedirectStorageError) -> ApiError {
    match err {
        RedirectStorageError::NotFound => ApiError::not_found("Không tìm thấy mục nhật ký 404."),
        other => map_storage_error(other),
    }
}

pub(super) fn map_query_rejection(_err: QueryRejection) -> ApiError {
    ApiError::bad_request("Tham số truy vấn danh sách chuyển hướng hoặc nhật ký 404 không hợp lệ.")
}

pub(super) fn map_json_rejection(err: JsonRejection) -> ApiError {
    match err {
        JsonRejection::BytesRejection(b) if b.status() == StatusCode::PAYLOAD_TOO_LARGE => ApiError(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Dữ liệu JSON chuyển hướng không hợp lệ hoặc vượt quá giới hạn kích thước cho phép."
                .into(),
        ),
        _ => ApiError::bad_request(
            "Dữ liệu JSON chuyển hướng không hợp lệ hoặc vượt quá giới hạn kích thước cho phép.",
        ),
    }
}

pub(super) fn map_csv_json_rejection(err: JsonRejection) -> ApiError {
    match err {
        JsonRejection::BytesRejection(b) if b.status() == StatusCode::PAYLOAD_TOO_LARGE => {
            ApiError(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Nội dung yêu cầu nhập CSV vượt quá giới hạn kích thước HTTP cho phép.".into(),
            )
        }
        _ => ApiError::bad_request(
            "Dữ liệu JSON chuyển hướng không hợp lệ hoặc vượt quá giới hạn kích thước cho phép.",
        ),
    }
}

pub(super) fn extract_public_origin_from_headers(headers: &HeaderMap) -> Option<String> {
    let host = crate::routes::public_content::extract_origin_host(headers);
    if host.is_empty() {
        return None;
    }
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|p| p.eq_ignore_ascii_case("http") || p.eq_ignore_ascii_case("https"))
        .unwrap_or("http");
    Some(format!("{proto}://{host}"))
}
