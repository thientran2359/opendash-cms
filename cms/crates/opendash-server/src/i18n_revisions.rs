use crate::i18n::{UNKNOWN_ERROR_CODE, localize_json_error, translate_api_message};

pub const REVISION_ERROR_ENTRIES: &[(&str, &str, &str)] = &[
    (
        "revision.pattern_admin_only",
        "Chỉ Quản trị viên mới được phép thao tác Pattern.",
        "Only Administrators may operate on Pattern revisions or autosaves.",
    ),
    (
        "revision.section_forbidden",
        "Loại nội dung Section không hỗ trợ lịch sử phiên bản hoặc lưu tự động.",
        "Section content type does not support revisions or autosaves.",
    ),
    (
        "revision.invalid_content_type_key",
        "Mã loại nội dung không hợp lệ.",
        "Invalid content type key for revision request.",
    ),
    (
        "revision.json_content_type_required",
        "Yêu cầu phải có Content-Type là application/json.",
        "Request must use Content-Type: application/json.",
    ),
    (
        "revision.invalid_revision_id",
        "ID phiên bản lịch sử không hợp lệ.",
        "Invalid revision ID.",
    ),
    (
        "revision.duplicate_limit_param",
        "Tham số limit bị trùng lặp trong truy vấn.",
        "Duplicate limit query parameter.",
    ),
    (
        "revision.limit_not_integer",
        "Tham số limit phải là số nguyên 1..=25.",
        "Query parameter limit must be an integer between 1 and 25.",
    ),
    (
        "revision.limit_out_of_range",
        "Tham số limit phải nằm trong khoảng 1..=25.",
        "Query parameter limit must be between 1 and 25.",
    ),
    (
        "revision.duplicate_offset_param",
        "Tham số offset bị trùng lặp trong truy vấn.",
        "Duplicate offset query parameter.",
    ),
    (
        "revision.offset_not_integer",
        "Tham số offset phải là số nguyên 0..=1000.",
        "Query parameter offset must be an integer between 0 and 1000.",
    ),
    (
        "revision.offset_out_of_range",
        "Tham số offset phải nằm trong khoảng 0..=1000.",
        "Query parameter offset must be between 0 and 1000.",
    ),
    (
        "revision.invalid_list_query",
        "Tham số truy vấn lịch sử phiên bản không hợp lệ.",
        "Invalid revision list query parameters.",
    ),
    (
        "revision.invalid_compare_target",
        "Tham số so sánh from/to không hợp lệ.",
        "Invalid revision compare target for from/to.",
    ),
    (
        "revision.duplicate_from_param",
        "Tham số from bị trùng lặp.",
        "Duplicate compare 'from' query parameter.",
    ),
    (
        "revision.duplicate_to_param",
        "Tham số to bị trùng lặp.",
        "Duplicate compare 'to' query parameter.",
    ),
    (
        "revision.invalid_compare_query",
        "Tham số truy vấn so sánh phiên bản không hợp lệ.",
        "Invalid revision compare query parameters.",
    ),
    (
        "revision.missing_compare_params",
        "Cần cung cấp đầy đủ hai tham số so sánh from và to.",
        "Both compare query parameters 'from' and 'to' are required.",
    ),
    (
        "revision.invalid_restore_json",
        "Dữ liệu khôi phục JSON không hợp lệ.",
        "Invalid revision restore JSON payload.",
    ),
    (
        "revision.missing_refs",
        "Bản lịch sử chứa tham chiếu đã bị xóa hoặc không còn hợp lệ. Hãy xác nhận để tự động dọn khi khôi phục.",
        "Revision snapshot contains missing or incompatible references. Confirm to sanitize them before restoring.",
    ),
    (
        "revision.not_found",
        "Không tìm thấy phiên bản lịch sử.",
        "Revision snapshot not found.",
    ),
    (
        "revision.trashed_entry",
        "entry_trashed: Nội dung đang ở trong Thùng rác và không thể thao tác lịch sử phiên bản hoặc bản lưu tự động.",
        "entry_trashed: Entry is in Trash and cannot access revisions or autosave.",
    ),
    (
        "revision.version_conflict_reload",
        "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi khôi phục.",
        "Content entry version changed. Reload before restoring revision.",
    ),
    (
        "revision.builder_disabled",
        "Trình dựng trang (Builder) hiện đang tắt cho loại nội dung này.",
        "Builder support is currently disabled for this content type.",
    ),
    (
        "revision.builder_disabled_field",
        "Trình dựng trang đã bị tắt.",
        "Builder support is disabled.",
    ),
    (
        "revision.required_field_missing",
        "Thiếu giá trị cho trường bắt buộc sau khi dọn dữ liệu tham chiếu đã xóa.",
        "Missing required field values after sanitizing deleted references.",
    ),
    (
        "revision.restore_validation_failed",
        "Dữ liệu nội dung vượt giới hạn 768 KiB.",
        "Restored content values payload exceeds 768 KiB limit.",
    ),
    (
        "revision.restore_validation_failed",
        "Nội dung Builder vượt quá giới hạn 384 KiB sau khi dọn.",
        "Sanitized Builder content exceeds 384 KiB limit.",
    ),
    (
        "revision.restore_validation_failed",
        "Nội dung Builder vượt quá giới hạn cho phép.",
        "Builder content exceeds the allowed size limit.",
    ),
    (
        "revision.restore_validation_failed",
        "Pattern không có Builder document chỉ hỗ trợ content_mode = 'editor'.",
        "Pattern without a Builder document only supports content_mode = 'editor'.",
    ),
    (
        "revision.restore_validation_failed",
        "content_mode không hợp lệ.",
        "Invalid content_mode for pattern restore.",
    ),
    (
        "revision.restore_validation_failed",
        "Chuyên mục chính không nằm trong term_ids.",
        "Primary category term must be included in term_ids.",
    ),
    (
        "revision.restore_validation_failed",
        "Chuyên mục chính không hợp lệ.",
        "Invalid primary category term.",
    ),
    (
        "revision.corrupt_snapshot",
        "Chế độ nội dung trong DB không hợp lệ; không thể tạo snapshot lịch sử.",
        "Invalid stored content_mode in DB; cannot capture revision snapshot.",
    ),
    (
        "revision.corrupt_snapshot",
        "Dữ liệu trường nội dung bị lỗi JSON; không thể tạo snapshot.",
        "Stored content field JSON is malformed; cannot capture revision snapshot.",
    ),
    (
        "revision.corrupt_snapshot",
        "Dữ liệu Builder JSON không hợp lệ; không thể tạo snapshot.",
        "Stored Builder JSON is invalid; cannot capture revision snapshot.",
    ),
    (
        "revision.corrupt_snapshot",
        "Dữ liệu Builder không phải JSON object; không thể tạo snapshot.",
        "Stored Builder payload is not a JSON object; cannot capture revision snapshot.",
    ),
    (
        "revision.corrupt_snapshot",
        "Nguồn revision không hợp lệ trong DB.",
        "Invalid revision source stored in DB.",
    ),
    (
        "revision.corrupt_snapshot",
        "Dữ liệu Builder JSON không hợp lệ.",
        "Invalid Builder document JSON payload.",
    ),
    (
        "revision.corrupt_snapshot",
        "Dữ liệu Builder JSON không phải UTF-8 hợp lệ.",
        "Builder JSON is not valid UTF-8.",
    ),
    (
        "revision.corrupt_snapshot",
        "Dữ liệu values_json của revision bị hỏng.",
        "Stored revision values_json is corrupted.",
    ),
    (
        "revision.corrupt_snapshot",
        "Dữ liệu term_ids_json của revision bị hỏng.",
        "Stored revision term_ids_json is corrupted.",
    ),
    (
        "revision.corrupt_snapshot",
        "Dữ liệu seo_json của revision bị hỏng.",
        "Stored revision seo_json is corrupted.",
    ),
    (
        "autosave.invalid_version_or_seq",
        "base_version và client_seq phải là số nguyên dương lớn hơn hoặc bằng 1.",
        "Autosave base_version and client_seq must be positive integers >= 1.",
    ),
    (
        "autosave.null_title_forbidden",
        "Tiêu đề bản lưu tự động không được đặt là null.",
        "Autosave title cannot be set to null.",
    ),
    (
        "autosave.invalid_title",
        "Tiêu đề bản lưu tự động không được chứa ký tự điều khiển hoặc vượt quá 200 ký tự.",
        "Autosave title must not contain control characters or exceed 200 characters.",
    ),
    (
        "autosave.too_many_cleared_fields",
        "Danh sách cleared_fields vượt quá giới hạn 128 trường.",
        "Autosave cleared_fields exceeds the maximum of 128 entries.",
    ),
    (
        "autosave.too_many_term_ids",
        "Danh sách term_ids vượt quá giới hạn 96 mục.",
        "Autosave term_ids exceeds the maximum of 96 entries.",
    ),
    (
        "autosave.invalid_seo",
        "Dữ liệu SEO lưu tự động không hợp lệ.",
        "Invalid autosave SEO payload.",
    ),
    (
        "autosave.seo_too_large",
        "Dữ liệu SEO lưu tự động vượt quá giới hạn 4096 byte.",
        "Autosave SEO payload exceeds 4096-byte limit.",
    ),
    (
        "autosave.pattern_too_large",
        "Dữ liệu lưu tự động Pattern vượt quá giới hạn kích thước cho phép.",
        "Pattern autosave payload exceeds allowed size limit.",
    ),
    (
        "autosave.invalid_json",
        "Dữ liệu lưu tự động JSON không hợp lệ.",
        "Invalid autosave JSON payload.",
    ),
    (
        "autosave.pattern_admin_only",
        "Pattern chỉ cho phép Admin lưu tự động.",
        "Only Administrators may save Pattern autosaves.",
    ),
    (
        "autosave.builder_locked_or_disabled",
        "Trình dựng trang bị khóa hoặc chưa bật cho tài khoản này.",
        "Builder is locked for your role or disabled for this content type.",
    ),
    (
        "autosave.base_version_in_future",
        "base_version vượt quá version hiện tại của nội dung.",
        "Autosave base_version exceeds current entry version.",
    ),
    (
        "autosave.payload_too_large",
        "Dữ liệu bản lưu tự động vượt giới hạn 768 KiB.",
        "Merged autosave payload exceeds 768 KiB limit.",
    ),
    (
        "autosave.not_found",
        "Không tìm thấy bản lưu tự động.",
        "Autosave draft not found.",
    ),
    (
        "autosave.corrupt_surface",
        "source_surface autosave không hợp lệ.",
        "Invalid stored autosave source_surface in DB.",
    ),
    (
        "autosave.pattern_studio_only",
        "Pattern autosave chỉ hỗ trợ source_surface = 'studio' và builder_document.",
        "Pattern autosave only supports source_surface = 'studio' with builder_document.",
    ),
    (
        "autosave.pattern_builder_required",
        "Pattern autosave yêu cầu builder_document.",
        "Pattern autosave requires builder_document.",
    ),
    (
        "autosave.studio_builder_required",
        "Studio autosave yêu cầu builder_document.",
        "Studio autosave requires builder_document.",
    ),
    (
        "autosave.builder_doc_too_large",
        "Builder document vượt giới hạn 256 KiB.",
        "Builder document exceeds 256 KiB limit.",
    ),
    (
        "autosave.invalid_builder_json",
        "Builder JSON không hợp lệ.",
        "Invalid Builder document JSON.",
    ),
    (
        "autosave.studio_body_too_large",
        "Body Studio vượt quá giới hạn 384 KiB.",
        "Serialized Studio body exceeds 384 KiB limit.",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn revision_error_table_covers_all_entries_and_json_responses() {
        for &(code, vi, en) in REVISION_ERROR_ENTRIES {
            assert!(!code.is_empty() && code.contains('.'), "bad code {code}");
            for raw in [vi, en] {
                for locale in ["en", "vi"] {
                    let loc = translate_api_message(raw, locale);
                    assert_ne!(
                        loc.code, UNKNOWN_ERROR_CODE,
                        "missing catalog entry for {raw}"
                    );
                    assert_eq!(loc.code, code, "unexpected code for {raw}");
                }
            }
        }

        let mut missing_refs_body = json!({
            "code": "revision_missing_refs",
            "message": "Bản lịch sử chứa tham chiếu đã bị xóa hoặc không còn hợp lệ. Hãy xác nhận để tự động dọn khi khôi phục.",
            "missing_refs": { "missing_term_ids": [42] }
        });
        assert_eq!(
            localize_json_error(&mut missing_refs_body, "en", None),
            Some("revision.missing_refs")
        );
        assert_eq!(missing_refs_body["missing_refs"]["missing_term_ids"][0], 42);
        assert_eq!(
            localize_json_error(&mut missing_refs_body, "vi", None),
            Some("revision.missing_refs")
        );

        let mut veto = json!({
            "code": "required_field_missing",
            "message": "Thiếu giá trị cho trường bắt buộc sau khi dọn dữ liệu tham chiếu đã xóa.",
            "field_errors": { "hero_image": "Hero Image" }
        });
        assert_eq!(
            localize_json_error(&mut veto, "en", None),
            Some("revision.required_field_missing")
        );
        assert_eq!(veto["field_errors"]["hero_image"], "Hero Image");

        let mut builder_veto = json!({
            "code": "builder_disabled",
            "message": "Trình dựng trang (Builder) hiện đang tắt cho loại nội dung này.",
            "field_errors": { "body": "Trình dựng trang đã bị tắt." }
        });
        assert_eq!(
            localize_json_error(&mut builder_veto, "en", None),
            Some("revision.builder_disabled")
        );
        assert_eq!(
            builder_veto["field_errors"]["body"],
            "Builder support is disabled."
        );
    }
}
