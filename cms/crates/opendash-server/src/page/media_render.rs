use std::collections::BTreeMap;

use opendash_core::media::{ResponsivePictureInput, render_responsive_picture_html};

use super::escape_html;
use crate::{field_hydrate::build_media_urls_and_srcsets, storage::fields::HydratedMediaRow};

pub(super) fn versioned_media_file_url(
    media_id: &str,
    hydration_media: &BTreeMap<String, HydratedMediaRow>,
    is_preview: bool,
) -> String {
    let prefix = if is_preview {
        "/api/admin/media"
    } else {
        "/media"
    };
    match hydration_media.get(media_id) {
        Some(row) => format!("{prefix}/{media_id}/file?v={}", row.version.max(1)),
        None => format!("{prefix}/{media_id}/file"),
    }
}

pub(super) fn render_responsive_image_html(
    media_id: &str,
    alt: &str,
    class: &str,
    sizes: &str,
    hydration_media: &BTreeMap<String, HydratedMediaRow>,
    is_preview: bool,
) -> String {
    if let Some(row) = hydration_media.get(media_id)
        && row.kind == "image"
    {
        let (file_url, _thumb, _variants, webp_srcset, fallback_srcset) =
            build_media_urls_and_srcsets(row, is_preview);
        if let Some(picture_html) = render_responsive_picture_html(
            &ResponsivePictureInput {
                url: &file_url,
                width: row.width,
                height: row.height,
                webp_srcset: webp_srcset.as_deref(),
                fallback_srcset: fallback_srcset.as_deref(),
                sizes,
                class,
                alt,
                loading: "lazy",
                decoding: "async",
                fetchpriority: "",
            },
            is_preview,
        ) {
            return picture_html;
        }
    }
    let class_attr = if class.is_empty() {
        String::new()
    } else {
        format!(" class=\"{}\"", escape_html(class))
    };
    format!(
        "<img src=\"{}\" alt=\"{}\" loading=\"lazy\"{class_attr}>",
        escape_html(&versioned_media_file_url(
            media_id,
            hydration_media,
            is_preview
        )),
        escape_html(alt),
    )
}
