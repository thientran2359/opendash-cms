//! Up-front validation and shared helpers for transient `options_pages` Studio previews.

use std::collections::BTreeMap;

use opendash_core::{
    content::{MAX_CUSTOM_OPTIONS_PAGES_PER_SITE, valid_custom_options_page_key},
    registry::{DeclarationState, ProviderStatus},
};
use serde_json::Value;
use sqlx::SqliteConnection;

use crate::error::{ApiError, ApiResult};
use crate::storage::options_pages::get_options_page_in_conn;

const MAX_TRANSIENT_OPTIONS_PAGE_REFS: usize = MAX_CUSTOM_OPTIONS_PAGES_PER_SITE + 2;

/// Validates every `page_ref` key in `transient.options_pages` once at the start of Studio preview
/// before any early-return branch can skip unknown or inactive page targets.
pub(in crate::storage) async fn validate_transient_options_pages_refs_in_conn(
    conn: &mut SqliteConnection,
    active_theme_id: Option<&str>,
    transient_options_pages: Option<&BTreeMap<String, BTreeMap<String, Value>>>,
) -> ApiResult<()> {
    let Some(map) = transient_options_pages else {
        return Ok(());
    };
    if map.len() > MAX_TRANSIENT_OPTIONS_PAGE_REFS {
        return Err(ApiError::unprocessable(
            "Số lượng options_pages xem trước vượt quá giới hạn cho phép.",
        ));
    }
    for page_ref in map.keys() {
        if page_ref == "site_global" {
            continue;
        }
        if let Some(theme_id) = page_ref.strip_prefix("theme:") {
            if active_theme_id != Some(theme_id) {
                return Err(ApiError::unprocessable(
                    "theme:<id> trong transient.options_pages không khớp theme đang kích hoạt.",
                ));
            }
            continue;
        }
        if let Some(page_key) = page_ref.strip_prefix("custom:") {
            valid_custom_options_page_key(page_key).map_err(ApiError::unprocessable)?;
            let Some(page) = get_options_page_in_conn(conn, page_key).await? else {
                return Err(ApiError::unprocessable(
                    "Trang tùy chọn trong transient.options_pages không tồn tại.",
                ));
            };
            if page.provider_status != ProviderStatus::Active
                || page.declaration_state != DeclarationState::Declared
            {
                return Err(ApiError::unprocessable(
                    "Trang tùy chọn trong transient.options_pages không đang hoạt động.",
                ));
            }
            continue;
        }
        return Err(ApiError::unprocessable(
            "Khóa page_ref của transient.options_pages phải là 'site_global', 'theme:<id>' hoặc 'custom:<page_key>'.",
        ));
    }
    Ok(())
}

pub(in crate::storage) fn escape_preview_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}
