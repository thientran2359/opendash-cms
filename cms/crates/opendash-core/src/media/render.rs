//! Pure validation and responsive `<picture>` rendering helpers for C-SEC and builders.

use super::parse_variant_spec_ext;

pub fn valid_image_sizes_attr(raw: &str) -> bool {
    !raw.trim().is_empty()
        && raw.len() <= 256
        && raw.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(b, b' ' | b',' | b'.' | b'(' | b')' | b':' | b'%' | b'-')
        })
}

pub fn valid_image_loading_attr(raw: &str) -> bool {
    matches!(raw, "lazy" | "eager")
}

pub fn valid_image_decoding_attr(raw: &str) -> bool {
    matches!(raw, "async" | "sync" | "auto")
}

pub fn valid_image_fetchpriority_attr(raw: &str) -> bool {
    matches!(raw, "" | "high" | "low" | "auto")
}

pub fn valid_media_render_url(url: &str, allow_admin_preview: bool) -> bool {
    if url.is_empty()
        || url.contains('\0')
        || url
            .bytes()
            .any(|b| b <= 0x1f || b == 0x7f || b.is_ascii_whitespace())
        || url.chars().any(char::is_whitespace)
        || url.contains("..")
        || url.contains('#')
        || url.contains('\\')
    {
        return false;
    }
    let path = match url.split_once('?') {
        Some((path, query)) => {
            let Some(ver_str) = query.strip_prefix("v=") else {
                return false;
            };
            if ver_str.is_empty()
                || ver_str.starts_with('0')
                || !ver_str.bytes().all(|b| b.is_ascii_digit())
                || ver_str.parse::<u64>().is_err()
            {
                return false;
            }
            path
        }
        None => url,
    };
    let rest = if let Some(rest) = path.strip_prefix("/media/") {
        rest
    } else if allow_admin_preview {
        let Some(rest) = path.strip_prefix("/api/admin/media/") else {
            return false;
        };
        rest
    } else {
        return false;
    };
    let Some((id, tail)) = rest.split_once('/') else {
        return false;
    };
    if id.len() != 64 || !id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return false;
    }
    match tail {
        "file" | "thumbnail" => true,
        _ => tail
            .strip_prefix("variant/")
            .and_then(parse_variant_spec_ext)
            .is_some(),
    }
}

fn valid_srcset_descriptor(desc: &str) -> bool {
    if let Some(w_str) = desc.strip_suffix('w') {
        !w_str.is_empty()
            && !w_str.starts_with('0')
            && w_str.bytes().all(|b| b.is_ascii_digit())
            && w_str.parse::<u32>().is_ok_and(|w| w >= 1)
    } else if let Some(x_str) = desc.strip_suffix('x') {
        let mut dot_count = 0usize;
        let valid_chars = !x_str.is_empty()
            && !x_str.starts_with('.')
            && !x_str.ends_with('.')
            && x_str.bytes().all(|b| {
                if b == b'.' {
                    dot_count += 1;
                    dot_count <= 1
                } else {
                    b.is_ascii_digit()
                }
            });
        valid_chars && x_str.parse::<f64>().is_ok_and(|v| v.is_finite() && v > 0.0)
    } else {
        false
    }
}

pub fn valid_media_srcset(srcset: &str, allow_admin_preview: bool) -> bool {
    let trimmed = srcset.trim();
    if trimmed.is_empty() || srcset.contains('\0') || srcset.bytes().any(|b| b <= 0x1f || b == 0x7f)
    {
        return false;
    }
    for segment in trimmed.split(',') {
        let entry = segment.trim();
        if entry.is_empty() {
            return false;
        }
        let mut parts = entry.split_whitespace();
        let Some(url) = parts.next() else {
            return false;
        };
        let Some(desc) = parts.next() else {
            return false;
        };
        if parts.next().is_some() {
            return false;
        }
        if !valid_media_render_url(url, allow_admin_preview) || !valid_srcset_descriptor(desc) {
            return false;
        }
    }
    true
}

#[derive(Debug, Clone, Copy)]
pub struct ResponsivePictureInput<'a> {
    pub url: &'a str,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub webp_srcset: Option<&'a str>,
    pub fallback_srcset: Option<&'a str>,
    pub sizes: &'a str,
    pub class: &'a str,
    pub alt: &'a str,
    pub loading: &'a str,
    pub decoding: &'a str,
    pub fetchpriority: &'a str,
}

fn escape_attr_value(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Validates and builds responsive `<picture><source type="image/webp" ...><img ...></picture>`
/// or plain `<img ...>` markup when no WebP `srcset` is materialized.
pub fn render_responsive_picture_html(
    input: &ResponsivePictureInput<'_>,
    allow_admin_preview: bool,
) -> Option<String> {
    if !valid_media_render_url(input.url, allow_admin_preview)
        || !valid_image_sizes_attr(input.sizes)
        || (!input.class.is_empty() && !crate::icons::valid_icon_class_attr(input.class))
        || !valid_image_loading_attr(input.loading)
        || !valid_image_decoding_attr(input.decoding)
        || !valid_image_fetchpriority_attr(input.fetchpriority)
    {
        return None;
    }
    if let Some(ws) = input.webp_srcset
        && !valid_media_srcset(ws, allow_admin_preview)
    {
        return None;
    }
    if let Some(fs) = input.fallback_srcset
        && !valid_media_srcset(fs, allow_admin_preview)
    {
        return None;
    }

    let mut img = format!("<img src=\"{}\"", input.url);
    if let Some(fs) = input.fallback_srcset {
        img.push_str(&format!(" srcset=\"{fs}\" sizes=\"{}\"", input.sizes));
    }
    img.push_str(&format!(" alt=\"{}\"", escape_attr_value(input.alt)));
    if !input.class.is_empty() {
        img.push_str(&format!(" class=\"{}\"", input.class));
    }
    if let (Some(w), Some(h)) = (input.width, input.height)
        && w > 0
        && h > 0
    {
        img.push_str(&format!(" width=\"{w}\" height=\"{h}\""));
    }
    img.push_str(&format!(
        " loading=\"{}\" decoding=\"{}\"",
        input.loading, input.decoding
    ));
    if !input.fetchpriority.is_empty() {
        img.push_str(&format!(" fetchpriority=\"{}\"", input.fetchpriority));
    }
    img.push('>');

    if let Some(ws) = input.webp_srcset {
        Some(format!(
            "<picture><source type=\"image/webp\" srcset=\"{ws}\" sizes=\"{}\">{img}</picture>",
            input.sizes
        ))
    } else {
        Some(img)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::{
        MediaVariantFormat, MediaVariantSpec, MediaVariantView, build_responsive_srcsets,
    };

    #[test]
    fn validates_render_urls_srcsets_and_renders_picture_html() {
        let id = "a".repeat(64);
        assert!(valid_media_render_url(&format!("/media/{id}/file"), false));
        assert!(valid_media_render_url(
            &format!("/media/{id}/variant/medium.webp?v=2"),
            false
        ));
        assert!(!valid_media_render_url(
            &format!("/api/admin/media/{id}/file?v=1"),
            false
        ));
        assert!(valid_media_render_url(
            &format!("/api/admin/media/{id}/file?v=1"),
            true
        ));
        assert!(!valid_media_render_url(
            &format!("/media/{id}/file?foo=bar"),
            true
        ));
        assert!(!valid_media_render_url(
            &format!("/media/{id}/file?v=0"),
            true
        ));

        let ws = format!(
            "/media/{id}/variant/medium.webp?v=2 768w, /media/{id}/variant/full.webp?v=2 1200w"
        );
        let fs = format!("/media/{id}/variant/medium.jpg?v=2 768w, /media/{id}/file?v=2 1200w");
        assert!(valid_media_srcset(&ws, false));
        assert!(!valid_media_srcset(
            &format!("/api/admin/media/{id}/variant/medium.webp 768w"),
            false
        ));
        assert!(!valid_media_srcset(&format!("/media/{id}/file 0w"), true));

        let _ = (
            MediaVariantFormat::Webp,
            MediaVariantSpec::Medium,
            MediaVariantView {
                spec: MediaVariantSpec::Medium,
                format: MediaVariantFormat::Webp,
                width: 768,
                height: 512,
                byte_size: 100,
                url: "/media/a/variant/medium.webp?v=1".into(),
            },
            build_responsive_srcsets,
        );

        let html = render_responsive_picture_html(
            &ResponsivePictureInput {
                url: &format!("/media/{id}/file?v=2"),
                width: Some(1200),
                height: Some(800),
                webp_srcset: Some(&ws),
                fallback_srcset: Some(&fs),
                sizes: "(max-width: 768px) 100vw, 50vw",
                class: "h-auto w-full",
                alt: "Hello \"world\" & <tag>",
                loading: "lazy",
                decoding: "async",
                fetchpriority: "high",
            },
            false,
        )
        .unwrap();
        assert!(html.starts_with("<picture><source type=\"image/webp\" srcset=\""));
        assert!(html.contains("alt=\"Hello &quot;world&quot; &amp; &lt;tag&gt;\""));
        assert!(html.contains("width=\"1200\" height=\"800\""));
        assert!(html.contains("fetchpriority=\"high\""));
        assert!(html.ends_with("</picture>"));
    }
}
