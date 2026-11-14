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
