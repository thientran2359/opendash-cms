use crate::i18n::{UNKNOWN_ERROR_CODE, translate_api_message};

pub const REDIRECT_ERROR_ENTRIES: &[(&str, &str, &str)] = &[
    (
        "redirects.empty_source_path",
        "Đường dẫn nguồn không được để trống.",
        "Source path must not be empty.",
    ),
    (
        "redirects.source_path_too_long",
        "Đường dẫn nguồn vượt quá giới hạn 512 byte.",
        "Source path exceeds the 512-byte limit.",
    ),
    (
        "redirects.source_path_invalid_format",
        "Đường dẫn nguồn phải bắt đầu bằng '/', không phải '/', và không chứa khoảng trắng, '?', '#', '\\' hoặc đoạn rỗng/'.'/'..'.",
        "Source path must start with '/', cannot be '/', and must not contain spaces, '?', '#', '\\', or empty/'.'/'..' segments.",
    ),
    (
        "redirects.source_path_reserved_root",
        "Đường dẫn nguồn trùng với tiền tố hệ thống được bảo vệ.",
        "Source path collides with a reserved system prefix.",
    ),
    (
        "redirects.source_path_unsafe_encoding",
        "Đường dẫn nguồn chứa mã hóa phần trăm không hợp lệ hoặc ký tự ASCII bị cấm.",
        "Source path contains invalid percent-encoding or disallowed ASCII escapes.",
    ),
    (
        "redirects.empty_target_url",
        "URL đích không được để trống.",
        "Target URL must not be empty.",
    ),
    (
        "redirects.target_url_too_long",
        "URL đích vượt quá giới hạn 2048 byte.",
        "Target URL exceeds the 2048-byte limit.",
    ),
    (
        "redirects.target_url_invalid",
        "URL đích phải là đường dẫn nội bộ bắt đầu bằng '/' hoặc URL http(s):// hợp lệ không chứa thông tin đăng nhập.",
        "Target URL must be an internal path starting with '/' or a valid http(s):// URL without credentials.",
    ),
    (
        "redirects.target_url_unsafe_protocol_relative",
        "URL đích không được bắt đầu bằng '//' hoặc '/\\'.",
        "Target URL must not start with '//' or '/\\'.",
    ),
    (
        "redirects.self_redirect",
        "Đường dẫn nguồn và URL đích không được trỏ về chính nó.",
        "Source path and target URL must not form a self-redirect.",
    ),
    (
        "redirects.invalid_status_code",
        "Mã trạng thái chuyển hướng chỉ hỗ trợ 301 hoặc 302.",
        "Redirect status code must be 301 or 302.",
    ),
    (
        "redirects.note_too_long",
        "Ghi chú chuyển hướng không được vượt quá 256 ký tự.",
        "Redirect note must not exceed 256 characters.",
    ),
    (
        "redirects.note_control_chars",
        "Ghi chú chuyển hướng không được chứa ký tự điều khiển.",
        "Redirect note must not contain control characters.",
    ),
    (
        "redirects.csv_empty",
        "Nội dung CSV không có dòng dữ liệu hợp lệ nào.",
        "CSV content contains no valid data rows.",
    ),
    (
        "redirects.csv_too_large",
        "Nội dung CSV vượt quá giới hạn tối đa 2 MiB.",
        "CSV content exceeds the maximum 2 MiB limit.",
    ),
    (
        "redirects.csv_too_many_rows",
        "Nội dung CSV vượt quá giới hạn tối đa 5.000 dòng dữ liệu.",
        "CSV content exceeds the maximum 5,000 data rows.",
    ),
    (
        "redirects.csv_malformed_quote",
        "Nội dung CSV có dấu ngoặc kép không hợp lệ.",
        "CSV content has malformed quotation marks.",
    ),
    (
        "redirects.csv_missing_required_headers",
        "Tiêu đề CSV bắt buộc phải có cột source_path và target_url.",
        "CSV header must include source_path and target_url columns.",
    ),
    (
        "redirects.csv_duplicate_mapped_headers",
        "Tiêu đề CSV có nhiều cột ánh xạ trùng về cùng một trường.",
        "CSV header maps multiple columns to the same field.",
    ),
    (
        "redirects.csv_invalid_status_value",
        "Giá trị status_code trong CSV chỉ chấp nhận 301 hoặc 302.",
        "CSV status_code value must be 301 or 302.",
    ),
    (
        "redirects.csv_invalid_enabled_value",
        "Giá trị enabled trong CSV chỉ chấp nhận true/false, 1/0 hoặc yes/no.",
        "CSV enabled value must be true/false, 1/0, or yes/no.",
    ),
    (
        "redirects.csv_invalid_column_count",
        "Số lượng cột trên dòng CSV không hợp lệ.",
        "CSV row has an invalid column count.",
    ),
    (
        "redirects.csv_duplicate_in_batch",
        "Đường dẫn nguồn bị lặp lại trong cùng tệp CSV.",
        "Duplicate source path within the same CSV batch.",
    ),
    (
        "redirects.conflict_existing_source",
        "Đường dẫn nguồn đã tồn tại trong bảng chuyển hướng.",
        "Source path already exists in manual redirects.",
    ),
    (
        "redirects.cycle_or_chain_exceeded",
        "Quy tắc tạo vòng lặp chuyển hướng hoặc chuỗi dài hơn 3 bước.",
        "Rule creates a redirect loop or a chain longer than 3 hops.",
    ),
    (
        "redirects.quota_exceeded",
        "Đã đạt giới hạn tối đa 20.000 chuyển hướng thủ công.",
        "Maximum quota of 20,000 manual redirects has been reached.",
    ),
    (
        "redirects.csv_import_validation_failed",
        "Nhập CSV thất bại: có dòng không hợp lệ hoặc tạo vòng lặp.",
        "CSV import failed: one or more rows are invalid or create a redirect loop.",
    ),
    (
        "redirects.csv_import_conflicts",
        "Nhập CSV thất bại: có đường dẫn nguồn đã tồn tại.",
        "CSV import failed: one or more source paths already exist.",
    ),
    (
        "redirects.rule_not_found",
        "Không tìm thấy quy tắc chuyển hướng.",
        "Manual redirect rule not found.",
    ),
    (
        "redirects.precondition_failed",
        "Quy tắc chuyển hướng đã bị thay đổi bởi thao tác khác; hãy tải lại.",
        "Manual redirect rule was modified concurrently; please reload.",
    ),
    (
        "redirects.not_found_log_not_found",
        "Không tìm thấy mục nhật ký 404.",
        "404 log entry not found.",
    ),
    (
        "redirects.invalid_bulk_delete_ids",
        "Danh sách ID cần xóa phải có từ 1 đến 100 số nguyên dương không trùng lặp.",
        "Bulk delete IDs must contain 1 to 100 unique positive integers.",
    ),
    (
        "redirects.invalid_clear_days",
        "Giá trị older_than_days phải nằm trong khoảng 1 đến 3650 ngày.",
        "older_than_days must be between 1 and 3650 days.",
    ),
    (
        "redirects.invalid_query",
        "Tham số truy vấn danh sách chuyển hướng hoặc nhật ký 404 không hợp lệ.",
        "Invalid query parameters for redirects or 404 logs.",
    ),
    (
        "redirects.invalid_json_or_too_large",
        "Dữ liệu JSON chuyển hướng không hợp lệ hoặc vượt quá giới hạn kích thước cho phép.",
        "Invalid or oversized redirect JSON payload.",
    ),
    (
        "redirects.csv_http_payload_too_large",
        "Nội dung yêu cầu nhập CSV vượt quá giới hạn kích thước HTTP cho phép.",
        "CSV import request payload exceeds the maximum allowed HTTP body size.",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use opendash_core::redirects::{CsvRowError, RedirectValidationError};

    #[test]
    fn redirect_error_table_covers_all_variants_and_codes() {
        for &(code, vi, en) in REDIRECT_ERROR_ENTRIES {
            assert!(!code.is_empty() && code.contains('.'));
            for raw in [vi, en] {
                for locale in ["en", "vi"] {
                    let loc = translate_api_message(raw, locale);
                    assert_ne!(loc.code, UNKNOWN_ERROR_CODE, "{raw}");
                    assert_eq!(loc.code, code);
                }
            }
        }

        let all_validation = [
            RedirectValidationError::EmptySourcePath,
            RedirectValidationError::SourcePathTooLong,
            RedirectValidationError::SourcePathInvalidFormat,
            RedirectValidationError::SourcePathReservedRoot,
            RedirectValidationError::SourcePathUnsafeEncoding,
            RedirectValidationError::EmptyTargetUrl,
            RedirectValidationError::TargetUrlTooLong,
            RedirectValidationError::TargetUrlInvalid,
            RedirectValidationError::TargetUrlUnsafeProtocolRelative,
            RedirectValidationError::SelfRedirect,
            RedirectValidationError::InvalidStatusCode,
            RedirectValidationError::NoteTooLong,
            RedirectValidationError::NoteControlChars,
            RedirectValidationError::CsvEmpty,
            RedirectValidationError::CsvTooLarge,
            RedirectValidationError::CsvTooManyRows,
            RedirectValidationError::CsvMalformedQuote,
            RedirectValidationError::CsvMissingRequiredHeaders,
            RedirectValidationError::CsvDuplicateMappedHeaders,
        ];
        for v in all_validation {
            for locale in ["en", "vi"] {
                let loc = translate_api_message(v.canonical_vi_message(), locale);
                assert_eq!(loc.code, v.code());
            }
        }

        let all_csv_row = [
            CsvRowError::InvalidStatusValue,
            CsvRowError::InvalidEnabledValue,
            CsvRowError::InvalidColumnCount,
            CsvRowError::DuplicateSourceInCsv,
            CsvRowError::ConflictExistingSource,
            CsvRowError::CycleOrChainExceeded,
            CsvRowError::QuotaExceeded,
        ];
        for r in all_csv_row {
            for locale in ["en", "vi"] {
                let loc = translate_api_message(r.canonical_vi_message(), locale);
                assert_eq!(loc.code, r.code());
            }
        }
    }
}
