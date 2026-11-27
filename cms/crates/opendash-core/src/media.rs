//! Pure responsive media variant models, `ETag` parsers, and `srcset` builders.

use serde::{Deserialize, Serialize};

pub const MEDIUM_MAX_WIDTH: u32 = 768;
pub const LARGE_MAX_WIDTH: u32 = 1536;
pub const THUMBNAIL_MAX_DIM: u32 = 320;
pub const MAX_IMAGE_DIMENSION: u32 = 6000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaVariantSpec {
    Thumbnail,
    Medium,
    Large,
    Full,
}

impl MediaVariantSpec {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Thumbnail => "thumbnail",
            Self::Medium => "medium",
            Self::Large => "large",
            Self::Full => "full",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "thumbnail" => Some(Self::Thumbnail),
            "medium" => Some(Self::Medium),
            "large" => Some(Self::Large),
            "full" => Some(Self::Full),
            _ => None,
        }
    }

    pub fn max_width(self) -> Option<u32> {
        match self {
            Self::Thumbnail => Some(THUMBNAIL_MAX_DIM),
            Self::Medium => Some(MEDIUM_MAX_WIDTH),
            Self::Large => Some(LARGE_MAX_WIDTH),
            Self::Full => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaVariantFormat {
    Jpeg,
    Png,
    Webp,
}

impl MediaVariantFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Jpeg => "jpeg",
            Self::Png => "png",
            Self::Webp => "webp",
        }
    }

    pub fn parse_db(raw: &str) -> Option<Self> {
        match raw {
            "jpeg" => Some(Self::Jpeg),
            "png" => Some(Self::Png),
            "webp" => Some(Self::Webp),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Webp => "webp",
        }
    }

    pub fn mime_type(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
        }
    }

    pub fn parse_extension(ext: &str) -> Option<Self> {
        match ext {
            "jpg" => Some(Self::Jpeg),
            "png" => Some(Self::Png),
            "webp" => Some(Self::Webp),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaVariantView {
    pub spec: MediaVariantSpec,
    pub format: MediaVariantFormat,
    pub width: u32,
    pub height: u32,
    pub byte_size: u64,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuedMediaVariant {
    pub spec: MediaVariantSpec,
    pub format: MediaVariantFormat,
    pub width: u32,
    pub height: u32,
    pub byte_size: u64,
    pub relative_path: String,
    pub sha256: String,
    pub staged_rel_path: String,
}

/// Parses `<spec>.<ext>` matching strictly `^(thumbnail|medium|large|full)\.(jpg|png|webp)$`.
pub fn parse_variant_spec_ext(spec_ext: &str) -> Option<(MediaVariantSpec, MediaVariantFormat)> {
    let (spec_raw, ext_raw) = spec_ext.split_once('.')?;
    let spec = MediaVariantSpec::parse(spec_raw)?;
    let format = MediaVariantFormat::parse_extension(ext_raw)?;
    Some((spec, format))
}

/// Computes non-upscaling target dimensions preserving aspect ratio.
pub fn scaled_dimensions_for_width(
    orig_width: u32,
    orig_height: u32,
    target_width: u32,
) -> (u32, u32) {
    if orig_width == 0 || orig_height == 0 || target_width == 0 || orig_width <= target_width {
        return (orig_width, orig_height);
    }
    let target_height = ((u64::from(orig_height) * u64::from(target_width)
        + u64::from(orig_width) / 2)
        / u64::from(orig_width))
    .max(1) as u32;
    (target_width, target_height)
}

pub fn format_media_etag(generation: i64, version: i64) -> String {
    format!("gv{generation}-v{version}")
}

pub fn format_media_http_etag(generation: i64, version: i64, suffix: Option<&str>) -> String {
    match suffix {
        Some(s) if !s.is_empty() => format!("\"gv{generation}-v{version}-{s}\""),
        _ => format!("\"gv{generation}-v{version}\""),
    }
}

/// Parses strong ETag token `"gv<gen>-v<ver>"` (or unquoted `gv<gen>-v<ver>`).
/// Rejects weak tags (`W/`) and malformed headers so `If-Match` CAS stays strict.
pub fn parse_media_etag_token(raw: &str) -> Option<(i64, i64)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.starts_with("W/") || trimmed.starts_with("w/") {
        return None;
    }
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(trimmed);
    let rest = unquoted.strip_prefix("gv")?;
    let (gen_s, ver_s) = rest.split_once("-v")?;
    if !gen_s.bytes().all(|b| b.is_ascii_digit())
        || !ver_s.bytes().all(|b| b.is_ascii_digit())
        || gen_s.is_empty()
        || ver_s.is_empty()
    {
        return None;
    }
    let generation: i64 = gen_s.parse().ok()?;
    let ver: i64 = ver_s.parse().ok()?;
    if generation < 1 || ver < 1 {
        return None;
    }
    Some((generation, ver))
}

/// Builds `(webp_srcset, fallback_srcset)` from materialized variants and versioned `file_url`.
/// Never produces a degenerate 1-entry `fallback_srcset` without `medium`/`large` responsive widths.
pub fn build_responsive_srcsets(
    mime_type: &str,
    orig_width: u32,
    file_url_with_version: &str,
    variants: &[MediaVariantView],
) -> (Option<String>, Option<String>) {
    let find_variant = |spec: MediaVariantSpec, is_webp: bool| {
        variants
            .iter()
            .find(|v| v.spec == spec && ((v.format == MediaVariantFormat::Webp) == is_webp))
    };

    if mime_type.eq_ignore_ascii_case("image/webp") {
        let mut webp_parts = Vec::new();
        for spec in [MediaVariantSpec::Medium, MediaVariantSpec::Large] {
            if let Some(v) = find_variant(spec, true) {
                webp_parts.push(format!("{} {}w", v.url, v.width));
            }
        }
        let webp_srcset = if webp_parts.is_empty() {
            None
        } else {
            webp_parts.push(format!("{file_url_with_version} {orig_width}w"));
            Some(webp_parts.join(", "))
        };
        return (webp_srcset, None);
    }

    let mut webp_parts = Vec::new();
    for spec in [
        MediaVariantSpec::Medium,
        MediaVariantSpec::Large,
        MediaVariantSpec::Full,
    ] {
        if let Some(v) = find_variant(spec, true) {
            webp_parts.push(format!("{} {}w", v.url, v.width));
        }
    }
    let webp_srcset = (!webp_parts.is_empty()).then(|| webp_parts.join(", "));

    let mut fallback_parts = Vec::new();
    for spec in [MediaVariantSpec::Medium, MediaVariantSpec::Large] {
        if let Some(v) = find_variant(spec, false) {
            fallback_parts.push(format!("{} {}w", v.url, v.width));
        }
    }
    let fallback_srcset = if fallback_parts.is_empty() {
        None
    } else {
        fallback_parts.push(format!("{file_url_with_version} {orig_width}w"));
        Some(fallback_parts.join(", "))
    };

    (webp_srcset, fallback_srcset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_variant_spec_ext_strictly() {
        assert_eq!(
            parse_variant_spec_ext("medium.webp"),
            Some((MediaVariantSpec::Medium, MediaVariantFormat::Webp))
        );
        assert_eq!(
            parse_variant_spec_ext("large.jpg"),
            Some((MediaVariantSpec::Large, MediaVariantFormat::Jpeg))
        );
        assert_eq!(parse_variant_spec_ext("medium.jpeg"), None);
        assert_eq!(parse_variant_spec_ext("original.webp"), None);
        assert_eq!(parse_variant_spec_ext("medium.webp/extra"), None);
    }

    #[test]
    fn scales_without_upscaling() {
        assert_eq!(scaled_dimensions_for_width(500, 400, 768), (500, 400));
        assert_eq!(scaled_dimensions_for_width(768, 512, 768), (768, 512));
        assert_eq!(scaled_dimensions_for_width(1536, 1024, 768), (768, 512));
        assert_eq!(scaled_dimensions_for_width(3000, 2000, 1536), (1536, 1024));
    }

    #[test]
    fn builds_srcsets_for_jpeg_and_webp_sources() {
        let jpeg_variants = vec![
            MediaVariantView {
                spec: MediaVariantSpec::Medium,
                format: MediaVariantFormat::Jpeg,
                width: 768,
                height: 512,
                byte_size: 20_000,
                url: "/media/abc/variant/medium.jpg?v=2".to_string(),
            },
            MediaVariantView {
                spec: MediaVariantSpec::Medium,
                format: MediaVariantFormat::Webp,
                width: 768,
                height: 512,
                byte_size: 14_000,
                url: "/media/abc/variant/medium.webp?v=2".to_string(),
            },
            MediaVariantView {
                spec: MediaVariantSpec::Full,
                format: MediaVariantFormat::Webp,
                width: 1200,
                height: 800,
                byte_size: 35_000,
                url: "/media/abc/variant/full.webp?v=2".to_string(),
            },
        ];
        let (webp, fallback) =
            build_responsive_srcsets("image/jpeg", 1200, "/media/abc/file?v=2", &jpeg_variants);
        assert_eq!(
            webp.as_deref(),
            Some("/media/abc/variant/medium.webp?v=2 768w, /media/abc/variant/full.webp?v=2 1200w")
        );
        assert_eq!(
            fallback.as_deref(),
            Some("/media/abc/variant/medium.jpg?v=2 768w, /media/abc/file?v=2 1200w")
        );

        let webp_source_variants = vec![MediaVariantView {
            spec: MediaVariantSpec::Medium,
            format: MediaVariantFormat::Webp,
            width: 768,
            height: 512,
            byte_size: 14_000,
            url: "/media/abc/variant/medium.webp?v=3".to_string(),
        }];
        let (w2, f2) = build_responsive_srcsets(
            "image/webp",
            1200,
            "/media/abc/file?v=3",
            &webp_source_variants,
        );
        assert_eq!(
            w2.as_deref(),
            Some("/media/abc/variant/medium.webp?v=3 768w, /media/abc/file?v=3 1200w")
        );
        assert_eq!(f2, None);

        // No medium/large on WebP source -> None (avoid single-item srcset)
        let (w3, f3) = build_responsive_srcsets("image/webp", 600, "/media/abc/file?v=1", &[]);
        assert_eq!((w3, f3), (None, None));
    }
}
