use std::collections::BTreeMap;

use opendash_core::{
    builder::{ImageProps, VideoProps},
    media::{ResponsivePictureInput, render_responsive_picture_html},
};

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

pub(super) fn render_builder_image_html(
    props: &ImageProps,
    hydration_media: &BTreeMap<String, HydratedMediaRow>,
    is_preview: bool,
) -> String {
    if let Some(row) = hydration_media.get(&props.media_id)
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
                sizes: "100vw",
                class: "",
                alt: &props.alt,
                loading: "lazy",
                decoding: "async",
                fetchpriority: "",
            },
            is_preview,
        ) {
            return picture_html;
        }
    }
    format!(
        "<img src=\"{}\" alt=\"{}\" loading=\"lazy\">",
        escape_html(&versioned_media_file_url(
            &props.media_id,
            hydration_media,
            is_preview
        )),
        escape_html(&props.alt)
    )
}

pub(super) fn render_builder_video_html(
    props: &VideoProps,
    hydration_media: &BTreeMap<String, HydratedMediaRow>,
    is_preview: bool,
) -> String {
    let video_url = versioned_media_file_url(&props.media_id, hydration_media, is_preview);
    let mut attrs = format!(
        "src=\"{}\" preload=\"metadata\" playsinline",
        escape_html(&video_url),
    );
    if let Some(poster) = props.poster_image_id.as_deref() {
        let poster_url = versioned_media_file_url(poster, hydration_media, is_preview);
        attrs.push_str(&format!(" poster=\"{}\"", escape_html(&poster_url)));
    }
    if props.controls {
        attrs.push_str(" controls");
    }
    if props.muted {
        attrs.push_str(" muted");
    }
    format!("<video {attrs}></video>")
}
