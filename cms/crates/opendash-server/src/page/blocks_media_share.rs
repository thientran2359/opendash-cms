use std::collections::BTreeMap;

use opendash_core::{
    builder::BlockNode,
    field_schema::{FieldSpec, valid_sha256_media_id},
};
use serde_json::Value;

use super::{
    blocks_chrome::{align_justify_classes, bool_prop, preview_empty_placeholder},
    blocks_interactive::{pad_attr, wrap_margin_padding},
    blocks_layout::{NativeParentInfo, responsive_str_cascade, spacing_classes_or_attr, str_prop},
    media_render::{render_responsive_image_html_with_priority, versioned_media_file_url},
    render::escape_html,
};
use crate::{
    content_render::core_l10n,
    error::{ApiError, ApiResult},
    storage::fields::HydratedMediaRow,
    template_sec::{RenderBudget, register_core_icon_in_budget},
};

fn percent_encode_component(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(byte));
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn clamp_cols_1_6(s: &str) -> &'static str {
    match s {
        "1" => "1",
        "2" => "2",
        "4" => "4",
        "5" => "5",
        "6" => "6",
        _ => "3",
    }
}

pub(super) fn render_gallery(
    node: &BlockNode,
    fields: &[FieldSpec],
    hydration_media: &BTreeMap<String, HydratedMediaRow>,
    is_preview: bool,
    native_parent: NativeParentInfo<'_>,
    budget: &mut RenderBudget,
) -> ApiResult<String> {
    let l10n = core_l10n(&budget.locale);
    let valid_images: Vec<&str> = node
        .props
        .get("images")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|id| {
                    valid_sha256_media_id(id)
                        && hydration_media
                            .get(*id)
                            .is_some_and(|row| row.kind == "image")
                })
                .take(24)
                .collect()
        })
        .unwrap_or_default();

    if valid_images.is_empty() {
        return Ok(
            preview_empty_placeholder(&l10n.gallery_empty_preview, is_preview).unwrap_or_default(),
        );
    }

    let captions_raw = node
        .props
        .get("captions")
        .and_then(Value::as_str)
        .unwrap_or("");
    let caption_lines: Vec<&str> = captions_raw.lines().map(str::trim).collect();
    let show_captions = bool_prop(&node.props, "show_captions", false);
    let lightbox = bool_prop(&node.props, "lightbox", true);
    let masonry = str_prop(&node.props, "layout", "grid") == "masonry";
    let (gap_cls, mb_cls) = match str_prop(&node.props, "gap", "16") {
        "8" => ("gap-2", "mb-2"),
        "24" => ("gap-6", "mb-6"),
        _ => ("gap-4", "mb-4"),
    };
    let aspect_cls = match str_prop(&node.props, "aspect_ratio", "4_3") {
        "auto" => "aspect-auto",
        "16_9" => "aspect-video",
        "1_1" => "aspect-square",
        "3_4" => "aspect-[3/4]",
        _ => "aspect-[4/3]",
    };
    let radius_cls = match str_prop(&node.props, "radius", "lg") {
        "none" => "",
        "sm" => "rounded-sm",
        "md" => "rounded-md",
        "xl" => "rounded-xl",
        _ => "rounded-lg",
    };

    let (is_resp, d_raw, t_raw, m_raw) = responsive_str_cascade(&node.props, "columns", "3");
    let (d, t, m) = (
        clamp_cols_1_6(d_raw),
        clamp_cols_1_6(t_raw),
        clamp_cols_1_6(m_raw),
    );
    let mut grid_cls = if masonry {
        vec![format!("columns-{m}"), gap_cls.to_owned()]
    } else {
        vec![
            "grid".to_owned(),
            format!("grid-cols-{m}"),
            gap_cls.to_owned(),
        ]
    };
    let prefix = if masonry { "columns" } else { "grid-cols" };
    if is_resp && t != m {
        grid_cls.push(format!("md:{prefix}-{t}"));
    }
    if is_resp && d != t {
        grid_cls.push(format!("lg:{prefix}-{d}"));
    }

    let mut img_cls_parts = vec!["w-full".to_owned(), "object-cover".to_owned()];
    if !masonry {
        img_cls_parts.push(aspect_cls.to_owned());
    }
    if !radius_cls.is_empty() {
        img_cls_parts.push(radius_cls.to_owned());
    }
    let img_cls = img_cls_parts.join(" ");
    let fetchpriority = if native_parent.in_deferred_media {
        "low"
    } else {
        ""
    };
    let use_lightbox = lightbox && !is_preview;
    if use_lightbox {
        budget.needs_blocks_js = true;
    }

    let mut items_html = String::new();
    let mut first_full = String::new();
    let mut first_alt = String::new();
    let mut first_cap = String::new();

    for (idx, media_id) in valid_images.iter().enumerate() {
        let cap = caption_lines.get(idx).copied().unwrap_or("");
        let fallback_alt = format!("{} {}", l10n.gallery_default_label, idx + 1);
        let alt = if cap.is_empty() {
            fallback_alt.as_str()
        } else {
            cap
        };
        let picture = render_responsive_image_html_with_priority(
            media_id,
            alt,
            &img_cls,
            "(min-width: 1024px) 33vw, (min-width: 768px) 50vw, 100vw",
            hydration_media,
            is_preview,
            "lazy",
            fetchpriority,
        );
        let full_url = versioned_media_file_url(media_id, hydration_media, false);
        if idx == 0 {
            first_full = full_url.clone();
            first_alt = alt.to_owned();
            first_cap = cap.to_owned();
        }
        let media_inner = if use_lightbox {
            let trig_cls = if radius_cls.is_empty() {
                "od-gallery-trigger block w-full overflow-hidden text-left".to_owned()
            } else {
                format!("od-gallery-trigger block w-full overflow-hidden text-left {radius_cls}")
            };
            format!(
                "<button type=\"button\" class=\"{trig_cls}\" data-od-lb-trigger data-od-lb-idx=\"{idx}\" data-od-lb-src=\"{}\" data-od-lb-alt=\"{}\" data-od-lb-caption=\"{}\" aria-label=\"{}: {}\">{picture}</button>",
                escape_html(&full_url),
                escape_html(alt),
                escape_html(cap),
                escape_html(&l10n.gallery_open_image),
                escape_html(alt),
            )
        } else {
            picture
        };
        let cap_html = if show_captions && !cap.is_empty() {
            format!(
                "<figcaption class=\"mt-1.5 text-xs text-od-muted\">{}</figcaption>",
                escape_html(cap),
            )
        } else {
            String::new()
        };
        let fig_cls = if masonry {
            format!(" class=\"break-inside-avoid {mb_cls}\"")
        } else {
            String::new()
        };
        items_html.push_str(&format!(
            "<figure{fig_cls}>{media_inner}{cap_html}</figure>"
        ));
    }

    let lightbox_html = if use_lightbox {
        let icon_x = register_core_icon_in_budget(budget, "x", Some("w-5 h-5"))
            .map_err(ApiError::bad_request)?
            .unwrap_or_default();
        let icon_prev = register_core_icon_in_budget(budget, "chevron-left", Some("w-5 h-5"))
            .map_err(ApiError::bad_request)?
            .unwrap_or_default();
        let icon_next = register_core_icon_in_budget(budget, "chevron-right", Some("w-5 h-5"))
            .map_err(ApiError::bad_request)?
            .unwrap_or_default();
        let single_hidden = if valid_images.len() == 1 {
            " hidden"
        } else {
            ""
        };
        format!(
            "<dialog closedby=\"any\" class=\"od-lightbox-dialog\" aria-label=\"{}\"><button type=\"button\" class=\"od-lb-close\" data-od-lb-close autofocus aria-label=\"{}\">{icon_x}</button><button type=\"button\" class=\"od-lb-nav od-lb-prev{single_hidden}\" data-od-lb-prev aria-label=\"{}\">{icon_prev}</button><figure class=\"od-lightbox-content\"><img data-od-lb-img src=\"{}\" alt=\"{}\" class=\"max-h-[80vh] max-w-full object-contain\" loading=\"lazy\"><figcaption data-od-lb-caption class=\"mt-2 text-center text-sm text-white\">{}</figcaption></figure><button type=\"button\" class=\"od-lb-nav od-lb-next{single_hidden}\" data-od-lb-next aria-label=\"{}\">{icon_next}</button><div class=\"od-lb-counter text-xs text-white/80\" data-od-lb-counter>1 / {}</div></dialog>",
            escape_html(&l10n.gallery_lightbox_label),
            escape_html(&l10n.gallery_close),
            escape_html(&l10n.slider_prev),
            escape_html(&first_full),
            escape_html(&first_alt),
            escape_html(&first_cap),
            escape_html(&l10n.slider_next),
            valid_images.len(),
        )
    } else {
        String::new()
    };

    let (pad_tw, pad_fb) = spacing_classes_or_attr(fields, &node.props, "padding");
    let (mar_tw, mar_fb) = spacing_classes_or_attr(fields, &node.props, "margin");
    let mut root_cls = vec!["od-gallery".to_owned()];
    root_cls.extend(pad_tw);
    let inner = format!(
        "<div class=\"{}\"{}><div class=\"{}\">{items_html}</div>{lightbox_html}</div>",
        root_cls.join(" "),
        pad_attr(pad_fb),
        grid_cls.join(" "),
    );
    Ok(wrap_margin_padding(
        "od-gallery-wrap",
        mar_tw,
        mar_fb,
        inner,
    ))
}

pub(super) fn render_social_share(
    node: &BlockNode,
    fields: &[FieldSpec],
    is_preview: bool,
    budget: &mut RenderBudget,
) -> ApiResult<String> {
    let l10n = core_l10n(&budget.locale);
    let show_fb = bool_prop(&node.props, "show_facebook", true);
    let show_x = bool_prop(&node.props, "show_x", false);
    let show_li = bool_prop(&node.props, "show_linkedin", true);
    let show_tg = bool_prop(&node.props, "show_telegram", false);
    let show_em = bool_prop(&node.props, "show_email", true);
    let show_copy = bool_prop(&node.props, "show_copy_link", true);
    if !show_fb && !show_x && !show_li && !show_tg && !show_em && !show_copy {
        return Ok(
            preview_empty_placeholder(&l10n.share_empty_preview, is_preview).unwrap_or_default(),
        );
    }

    let custom_raw = node
        .props
        .get("custom_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    let custom_valid = !custom_raw.is_empty()
        && !custom_raw.starts_with("//")
        && (custom_raw.starts_with("http://")
            || custom_raw.starts_with("https://")
            || custom_raw.starts_with('/'));
    let target_url: String = if is_preview {
        "https://example.com/".to_owned()
    } else if custom_valid {
        custom_raw.to_owned()
    } else {
        budget
            .page_canonical_url
            .as_deref()
            .unwrap_or("/")
            .to_owned()
    };
    let is_abs = target_url.starts_with("http://") || target_url.starts_with("https://");
    if !is_preview && (!is_abs || show_copy) {
        budget.needs_blocks_js = true;
    }

    let title_str = budget.page_title.as_deref().unwrap_or("");
    let enc_title = percent_encode_component(title_str);
    let url_token = if is_abs {
        percent_encode_component(&target_url)
    } else {
        "__OD_SHARE_URL__".to_owned()
    };

    let display_style = str_prop(&node.props, "display_style", "pill");
    let icon_only = display_style == "icon_only";
    let base_cls = match display_style {
        "icon_only" => {
            "inline-flex min-h-[28px] min-w-[28px] items-center justify-center rounded-full border border-od-border bg-od-surface-alt p-1.5 text-xs font-semibold text-od-text hover:border-od-primary"
        }
        "minimal" => {
            "inline-flex min-h-[24px] items-center gap-1.5 text-xs font-medium text-od-muted hover:text-od-primary"
        }
        _ => {
            "inline-flex min-h-[28px] items-center gap-1.5 rounded-full border border-od-border bg-od-surface-alt px-3 py-1 text-xs font-medium text-od-text hover:border-od-primary"
        }
    };

    let make_link = |template: String, badge: &str, label: &str| -> String {
        let href_attrs = if is_abs {
            format!("href=\"{}\"", escape_html(&template))
        } else {
            format!(
                "href=\"#share\" data-od-share-href=\"{}\" data-od-share-path=\"{}\"",
                escape_html(&template),
                escape_html(&target_url),
            )
        };
        let label_html = if icon_only {
            String::new()
        } else {
            format!("<span>{}</span>", escape_html(label))
        };
        format!(
            "<a {href_attrs} target=\"_blank\" rel=\"noopener noreferrer\" aria-label=\"{}\" class=\"{base_cls}\"><span aria-hidden=\"true\" class=\"font-bold\">{badge}</span>{label_html}</a>",
            escape_html(label),
        )
    };

    let mut items = String::new();
    if show_fb {
        items.push_str(&make_link(
            format!("https://www.facebook.com/sharer/sharer.php?u={url_token}"),
            "f",
            "Facebook",
        ));
    }
    if show_x {
        items.push_str(&make_link(
            format!("https://twitter.com/intent/tweet?url={url_token}&text={enc_title}"),
            "X",
            "X",
        ));
    }
    if show_li {
        items.push_str(&make_link(
            format!("https://www.linkedin.com/sharing/share-offsite/?url={url_token}"),
            "in",
            "LinkedIn",
        ));
    }
    if show_tg {
        items.push_str(&make_link(
            format!("https://t.me/share/url?url={url_token}&text={enc_title}"),
            "TG",
            "Telegram",
        ));
    }
    if show_em {
        let mail_ic = register_core_icon_in_budget(budget, "mail", Some("w-3.5 h-3.5 shrink-0"))
            .map_err(ApiError::bad_request)?
            .unwrap_or_default();
        let tpl = format!("mailto:?subject={enc_title}&body={url_token}");
        let href_attrs = if is_abs {
            format!("href=\"{}\"", escape_html(&tpl))
        } else {
            format!(
                "href=\"#share\" data-od-share-href=\"{}\" data-od-share-path=\"{}\"",
                escape_html(&tpl),
                escape_html(&target_url),
            )
        };
        let label_html = if icon_only {
            String::new()
        } else {
            format!("<span>{}</span>", escape_html(&l10n.share_email_label))
        };
        items.push_str(&format!(
            "<a {href_attrs} aria-label=\"{}\" class=\"{base_cls}\">{mail_ic}{label_html}</a>",
            escape_html(&l10n.share_email_label),
        ));
    }
    if show_copy {
        let ic_link =
            register_core_icon_in_budget(budget, "link", Some("od-copy-icon w-3.5 h-3.5 shrink-0"))
                .map_err(ApiError::bad_request)?
                .unwrap_or_default();
        let ic_check = register_core_icon_in_budget(
            budget,
            "check",
            Some("od-copied-icon hidden w-3.5 h-3.5 shrink-0"),
        )
        .map_err(ApiError::bad_request)?
        .unwrap_or_default();
        let text_html = if icon_only {
            String::new()
        } else {
            format!("<span>{}</span>", escape_html(&l10n.copy_link))
        };
        items.push_str(&format!(
            "<span class=\"inline-flex items-center gap-1.5\"><button type=\"button\" class=\"{base_cls} min-w-[24px] min-h-[24px]\" data-od-copy-url=\"{}\" data-od-copied-label=\"{}\" data-od-copy-failed-label=\"{}\" aria-label=\"{}\">{ic_link}{ic_check}{text_html}</button><span class=\"od-copy-tip text-xs text-od-muted\" role=\"status\"></span></span>",
            escape_html(&target_url),
            escape_html(&l10n.copied),
            escape_html(&l10n.copy_failed),
            escape_html(&l10n.copy_link),
        ));
    }

    let heading = node
        .props
        .get("heading")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let heading_html = heading
        .map(|h| {
            format!(
                "<span class=\"text-xs font-semibold uppercase tracking-wide text-od-muted\">{}</span>",
                escape_html(h),
            )
        })
        .unwrap_or_default();

    let (pad_tw, pad_fb) = spacing_classes_or_attr(fields, &node.props, "padding");
    let (mar_tw, mar_fb) = spacing_classes_or_attr(fields, &node.props, "margin");
    let mut cls = vec![
        "od-social-share".to_owned(),
        "flex".to_owned(),
        "flex-wrap".to_owned(),
        "items-center".to_owned(),
        "gap-2".to_owned(),
    ];
    cls.extend(align_justify_classes(&node.props, "left"));
    cls.extend(pad_tw);
    let inner = format!(
        "<div class=\"{}\" aria-label=\"{}\"{}>{heading_html}{items}</div>",
        cls.join(" "),
        escape_html(&l10n.social_share_label),
        pad_attr(pad_fb),
    );
    Ok(wrap_margin_padding("od-share-wrap", mar_tw, mar_fb, inner))
}
