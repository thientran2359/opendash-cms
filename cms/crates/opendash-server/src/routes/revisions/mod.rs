use axum::{
    Json,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use opendash_core::content::valid_content_type_key;
use serde_json::json;

use crate::auth::Role;
use crate::error::{ApiError, ApiResult};
use crate::routes::settings::no_store_headers;
use crate::storage::revisions::RestoreError;

mod autosave_handlers;
mod revisions_handlers;

pub use autosave_handlers::*;
pub use revisions_handlers::*;

pub(super) fn parse_positive_id(raw: &str, msg: &'static str) -> ApiResult<i64> {
    if raw.is_empty() || raw.starts_with('0') || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ApiError::bad_request(msg));
    }
    let parsed = raw.parse::<i64>().map_err(|_| ApiError::bad_request(msg))?;
    if parsed < 1 {
        return Err(ApiError::bad_request(msg));
    }
    Ok(parsed)
}

pub(super) fn guard_type_key_access(type_key: &str, role: Role) -> ApiResult<()> {
    if type_key == "pattern" && role != Role::Admin {
        return Err(ApiError::forbidden(
            "Chỉ Quản trị viên mới được phép thao tác Pattern.",
        ));
    }
    if type_key == "section" {
        return Err(ApiError::forbidden(
            "Loại nội dung Section không hỗ trợ lịch sử phiên bản hoặc lưu tự động.",
        ));
    }
    if !valid_content_type_key(type_key) {
        return Err(ApiError::bad_request("Mã loại nội dung không hợp lệ."));
    }
    Ok(())
}

pub(super) fn require_json_content_type(headers: &HeaderMap) -> ApiResult<()> {
    let is_json = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"));
    if !is_json {
        return Err(ApiError::unsupported_media_type(
            "Yêu cầu phải có Content-Type là application/json.",
        ));
    }
    Ok(())
}

pub(super) fn map_restore_error(err: RestoreError) -> Response {
    match err {
        RestoreError::MissingRefs(missing_refs) => (
            StatusCode::CONFLICT,
            no_store_headers(None),
            Json(json!({
                "code": "revision_missing_refs",
                "message": "Bản lịch sử chứa tham chiếu đã bị xóa hoặc không còn hợp lệ. Hãy xác nhận để tự động dọn khi khôi phục.",
                "missing_refs": missing_refs,
            })),
        )
            .into_response(),
        RestoreError::ValidationVeto(veto) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            no_store_headers(None),
            Json(veto),
        )
            .into_response(),
        RestoreError::Api(api) => api.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::ContentBeforeSaveVeto;
    use opendash_core::content::PatchField;
    use opendash_core::revisions::MissingRefsReport;
    use std::collections::BTreeMap;

    #[test]
    fn parse_positive_id_and_type_key_guards_follow_spec() {
        assert_eq!(parse_positive_id("1", "bad").unwrap(), 1);
        assert_eq!(parse_positive_id("42", "bad").unwrap(), 42);
        assert!(parse_positive_id("0", "bad").is_err());
        assert!(parse_positive_id("-1", "bad").is_err());
        assert!(parse_positive_id("01", "bad").is_err());
        assert!(parse_positive_id("abc", "bad").is_err());

        assert_eq!(
            guard_type_key_access("pattern", Role::Editor)
                .unwrap_err()
                .0,
            StatusCode::FORBIDDEN
        );
        assert!(guard_type_key_access("pattern", Role::Admin).is_ok());
        assert_eq!(
            guard_type_key_access("section", Role::Admin).unwrap_err().0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            guard_type_key_access("INVALID!", Role::Admin)
                .unwrap_err()
                .0,
            StatusCode::BAD_REQUEST
        );
        assert!(guard_type_key_access("post", Role::Editor).is_ok());
    }

    #[test]
    fn list_compare_and_autosave_syntax_validation_enforce_bounds() {
        assert_eq!(parse_list_pairs(vec![]).unwrap(), (25, 0));
        assert_eq!(
            parse_list_pairs(vec![
                ("limit".into(), "1".into()),
                ("offset".into(), "1000".into()),
            ])
            .unwrap(),
            (1, 1000)
        );
        assert!(parse_list_pairs(vec![("limit".into(), "0".into())]).is_err());
        assert!(parse_list_pairs(vec![("limit".into(), "26".into())]).is_err());
        assert!(parse_list_pairs(vec![("offset".into(), "1001".into())]).is_err());
        assert!(parse_list_pairs(vec![("unknown".into(), "1".into())]).is_err());

        let (from, to) = parse_compare_pairs(vec![
            ("from".into(), "12".into()),
            ("to".into(), "autosave".into()),
        ])
        .unwrap();
        assert_eq!(
            serde_json::to_value(from).unwrap(),
            json!({"kind": "revision", "rev_id": 12})
        );
        assert_eq!(
            serde_json::to_value(to).unwrap(),
            json!({"kind": "autosave"})
        );
        assert!(parse_compare_pairs(vec![("from".into(), "current".into())]).is_err());

        let mut input: crate::storage::revisions::AutosaveUpsertInput =
            serde_json::from_value(json!({
                "source_surface": "editor",
                "base_version": 1,
                "client_seq": 1,
                "title": "ă".repeat(200),
            }))
            .unwrap();
        assert!(validate_autosave_input_syntax(&input).is_ok());

        input.title = PatchField::Clear;
        assert!(validate_autosave_input_syntax(&input).is_err());

        input.title = PatchField::Set("ă".repeat(201));
        assert!(validate_autosave_input_syntax(&input).is_err());

        input.title = PatchField::Set("ok\nline".into());
        assert!(validate_autosave_input_syntax(&input).is_err());

        input.title = PatchField::Omitted;
        input.base_version = 0;
        assert!(validate_autosave_input_syntax(&input).is_err());

        let resp_missing =
            map_restore_error(RestoreError::MissingRefs(MissingRefsReport::default()));
        assert_eq!(resp_missing.status(), StatusCode::CONFLICT);
        let resp_veto = map_restore_error(RestoreError::ValidationVeto(ContentBeforeSaveVeto {
            code: "required_field_missing".into(),
            message: "msg".into(),
            field_errors: BTreeMap::from([("hero".into(), "Hero".into())]),
        }));
        assert_eq!(resp_veto.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
