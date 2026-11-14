//! Pure SEO and custom entry path validation/projection helpers.
use opendash_core::id::valid_kebab_segment_with_limit;

pub const MAX_CUSTOM_PATH_BYTES: usize = 256;
pub const MAX_CUSTOM_PATH_SEGMENTS: usize = 5;

const RESERVED_SYSTEM_ROOTS: &[&str] = &[
    "admin",
    "api",
    "assets",
    "auth",
    "builder",
    "content",
    "forms",
    "health",
    "media",
    "page",
    "preview",
    "registry",
    "robots.txt",
    "save",
    "sitemap.xml",
    "sitemaps",
    "taxonomy",
    "theme",
    "themes",
];

/// Validates a custom entry path: `/` followed by 1..=5 ASCII kebab segments,
/// each <= 64 bytes, whole path <= 256 bytes, no trailing slash, query, fragment,
/// percent-encoding, backslash, dot, control/whitespace, or reserved system roots.
pub fn validate_custom_entry_path(raw: &str) -> Result<String, &'static str> {
    if raw.is_empty() || raw.len() > MAX_CUSTOM_PATH_BYTES {
        return Err("Đường dẫn tùy chỉnh phải từ 2 đến 256 bytes.");
    }
    if !raw.starts_with('/') || raw == "/" || raw.ends_with('/') {
        return Err("Đường dẫn tùy chỉnh phải bắt đầu bằng '/' và không có '/' ở cuối.");
    }
    if raw.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("Đường dẫn tùy chỉnh không được chứa khoảng trắng hoặc ký tự điều khiển.");
    }
    if raw.contains('?') || raw.contains('#') || raw.contains('%') || raw.contains('\\') || raw.contains('.') {
        return Err("Đường dẫn tùy chỉnh không được chứa ?, #, %, \\ hoặc dấu chấm.");
    }
    let segments: Vec<&str> = raw[1..].split('/').collect();
    if segments.is_empty() || segments.len() > MAX_CUSTOM_PATH_SEGMENTS {
        return Err("Đường dẫn tùy chỉnh chỉ được có từ 1 đến 5 đoạn (segments).");
    }
    for seg in &segments {
        if !valid_kebab_segment_with_limit(seg, 64) {
            return Err("Mỗi đoạn đường dẫn phải là ASCII kebab-case (1–64 bytes).");
        }
    }
    if RESERVED_SYSTEM_ROOTS.contains(&segments[0]) {
        return Err("Đường dẫn không được trùng với tiền tố hệ thống được bảo vệ.");
    }
    Ok(raw.to_owned())
}

use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    error::{ApiError, ApiResult},
    storage::media::valid_media_id,
};

pub const MAX_SEO_TITLE_CHARS: usize = 120;
pub const MAX_SEO_DESCRIPTION_CHARS: usize = 320;
pub const MAX_CANONICAL_URL_BYTES: usize = 2048;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SeoMetadataInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub og_image_id: Option<String>,
    pub canonical_url: Option<String>,
    #[serde(default)]
    pub noindex: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntrySeoSnapshot {
    pub entry_version: i64,
    pub title: Option<String>,
    pub description: Option<String>,
    pub og_image_id: Option<String>,
    pub canonical_url: Option<String>,
    pub noindex: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct HomeSeoSnapshot {
    pub version: i64,
    pub title: Option<String>,
    pub description: Option<String>,
    pub og_image_id: Option<String>,
    pub canonical_url: Option<String>,
    pub noindex: bool,
}

#[derive(Debug, Clone)]
pub struct SeoView {
    pub lang: String,
    pub title: String,
    pub description: Option<String>,
    pub canonical_url: Option<String>,
    pub robots: String,
    pub og_title: String,
    pub og_description: Option<String>,
    pub og_url: Option<String>,
    pub og_image_url: Option<String>,
}

impl SeoView {
    pub fn for_noindex_page(lang: &str, title: &str) -> Self {
        Self {
            lang: if lang.trim().is_empty() {
                "vi".to_string()
            } else {
                lang.trim().to_string()
            },
            title: title.to_string(),
            description: None,
            canonical_url: None,
            robots: "noindex,nofollow".to_string(),
            og_title: title.to_string(),
            og_description: None,
            og_url: None,
            og_image_url: None,
        }
    }
}

fn clean_plain_text(
    raw: Option<&str>,
    max_chars: usize,
    label: &str,
) -> ApiResult<Option<String>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(ApiError::unprocessable(format!(
            "{label} không được chứa ký tự điều khiển."
        )));
    }
    if trimmed.chars().count() > max_chars {
        return Err(ApiError::unprocessable(format!(
            "{label} vượt quá giới hạn {max_chars} ký tự."
        )));
    }
    Ok(Some(trimmed.to_string()))
}

pub fn validate_seo_input(input: &SeoMetadataInput) -> ApiResult<SeoMetadataInput> {
    let title = clean_plain_text(input.title.as_deref(), MAX_SEO_TITLE_CHARS, "Tiêu đề SEO")?;
    let description = clean_plain_text(
        input.description.as_deref(),
        MAX_SEO_DESCRIPTION_CHARS,
        "Mô tả SEO",
    )?;
    let og_image_id = match input.og_image_id.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(id) => {
            if !valid_media_id(id) {
                return Err(ApiError::unprocessable("ID ảnh OG không hợp lệ."));
            }
            Some(id.to_string())
        }
    };
    let canonical_url = match input.canonical_url.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(raw) => {
            if raw.len() > MAX_CANONICAL_URL_BYTES || raw.chars().any(|c| c.is_control()) {
                return Err(ApiError::unprocessable("Canonical URL không hợp lệ."));
            }
            let parsed = Url::parse(raw)
                .map_err(|_| ApiError::unprocessable("Canonical URL không hợp lệ."))?;
            if !matches!(parsed.scheme(), "http" | "https")
                || parsed.host_str().is_none()
                || !parsed.username().is_empty()
                || parsed.password().is_some()
                || parsed.fragment().is_some()
            {
                return Err(ApiError::unprocessable(
                    "Canonical URL phải là URL http/https hợp lệ không có fragment hoặc userinfo.",
                ));
            }
            Some(parsed.to_string())
        }
    };
    Ok(SeoMetadataInput {
        title,
        description,
        og_image_id,
        canonical_url,
        noindex: input.noindex,
    })
}
