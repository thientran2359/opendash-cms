use opendash_core::{
    field_schema::{FieldSpec, FieldType, MediaFieldKind, valid_safe_url},
    media::valid_media_render_url,
};
use serde_json::Value;

use super::{
    blocks_entry_field::{escape_text_or_badge, render_spec_value},
    render::escape_html,
};
use crate::{
    content_render::{CoreL10n, EntryView, HydratedMedia},
    template_sec::RenderBudget,
};

pub(super) fn render_image_markup_for_media(
    media: &HydratedMedia,
    in_loop: bool,
    is_preview: bool,
    extra_cls: &str,
    alt_fallback: &str,
) -> Option<String> {
    let alt = if media.alt.trim().is_empty() {
        alt_fallback
    } else {
        media.alt.as_str()
    };
    if in_loop && !media.thumbnail_url.trim().is_empty() {
        let thumb = media.thumbnail_url.trim();
        if !valid_media_render_url(thumb, is_preview) {
            return None;
        }
        return Some(format!(
            "<img src=\"{}\" alt=\"{}\" class=\"{}\" loading=\"lazy\" decoding=\"async\">",
            escape_html(thumb),
            escape_html(alt),
            escape_html(extra_cls),
        ));
    }
    let pic_input = media.as_picture_input(
        "(max-width: 768px) 100vw, 480px",
        extra_cls,
        Some(alt),
        "lazy",
        "",
    );
    if let Some(pic) = opendash_core::media::render_responsive_picture_html(&pic_input, is_preview)
    {
        return Some(pic);
    }
    valid_media_render_url(media.url.trim(), is_preview).then(|| {
        format!(
            "<img src=\"{}\" alt=\"{}\" class=\"{}\" loading=\"lazy\" decoding=\"async\">",
            escape_html(media.url.trim()),
            escape_html(alt),
            escape_html(extra_cls),
        )
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_complex_spec_value(
    spec: &FieldSpec,
    val: Option<&Value>,
    display_format: &str,
    entry: &EntryView,
    in_loop: bool,
    is_preview: bool,
    timezone: &str,
    locale: &str,
    l10n: &CoreL10n,
    budget: &mut RenderBudget,
    depth: usize,
) -> Result<Option<String>, &'static str> {
    match spec.field_type {
        FieldType::Media => {
            let Some(media) = val.and_then(HydratedMedia::from_value) else {
                return Ok(None);
            };
            let kind = if media.kind.is_empty() {
                match spec.effective_media_kind() {
                    MediaFieldKind::Video => "video",
                    MediaFieldKind::File => "file",
                    _ => "image",
                }
            } else {
                media.kind.as_str()
            };
            if kind == "image" && display_format != "link" {
                return Ok(render_image_markup_for_media(
                    &media,
                    in_loop,
                    is_preview,
                    "w-full h-auto rounded object-cover",
                    &entry.title,
                )
                .map(|img| format!("<figure class=\"od-field-media\">{img}</figure>")));
            }
            if !valid_media_render_url(media.url.trim(), is_preview) {
                return Ok(None);
            }
            if kind == "video" && display_format != "link" {
                return Ok(Some(format!(
                    "<video controls class=\"w-full rounded\" src=\"{}\"></video>",
                    escape_html(media.url.trim())
                )));
            }
            let label = if media.alt.trim().is_empty() {
                media.url.trim()
            } else {
                media.alt.trim()
            };
            Ok(Some(format!(
                "<a href=\"{}\" class=\"underline hover:text-od-primary\" rel=\"noopener noreferrer\">{}</a>",
                escape_html(media.url.trim()),
                escape_html(label),
            )))
        }
        FieldType::Gallery => {
            let items: Vec<HydratedMedia> = val
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(HydratedMedia::from_value)
                .take(if in_loop { 4 } else { 24 })
                .collect();
            if items.is_empty() {
                return Ok(None);
            }
            let mut imgs = String::new();
            for item in &items {
                if let Some(m) = render_image_markup_for_media(
                    item,
                    in_loop,
                    is_preview,
                    "w-full h-32 rounded object-cover",
                    &entry.title,
                ) {
                    imgs.push_str(&m);
                }
            }
            Ok((!imgs.is_empty()).then(|| {
                format!("<div class=\"od-field-gallery grid grid-cols-2 sm:grid-cols-4 gap-2\">{imgs}</div>")
            }))
        }
        FieldType::EntryRef | FieldType::TermRef => {
            let max_refs = if in_loop { 5 } else { 24 };
            let list: Vec<&serde_json::Map<String, Value>> = match val {
                Some(Value::Object(obj)) => vec![obj],
                Some(Value::Array(arr)) => arr
                    .iter()
                    .filter_map(Value::as_object)
                    .take(max_refs)
                    .collect(),
                _ => Vec::new(),
            };
            if list.is_empty() {
                return Ok(None);
            }
            let mut out = Vec::new();
            for obj in list {
                let text = obj
                    .get("title")
                    .or_else(|| obj.get("name"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or("");
                if text.is_empty() {
                    continue;
                }
                let url = obj
                    .get("url")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|u| valid_safe_url(u));
                if display_format == "text" || url.is_none() {
                    out.push(escape_text_or_badge(text, display_format));
                } else if let Some(u) = url {
                    let cls = if display_format == "badge" {
                        "od-badge inline-block rounded-full bg-od-surface-alt px-2.5 py-0.5 text-xs hover:underline"
                    } else {
                        "underline hover:text-od-primary"
                    };
                    out.push(format!(
                        "<a href=\"{}\" class=\"{cls}\">{}</a>",
                        escape_html(u),
                        escape_html(text)
                    ));
                }
            }
            Ok((!out.is_empty()).then(|| {
                format!(
                    "<span class=\"inline-flex flex-wrap gap-2\">{}</span>",
                    out.join("")
                )
            }))
        }
        FieldType::Group => {
            let Some(obj) = val.and_then(Value::as_object) else {
                return Ok(None);
            };
            let mut items = String::new();
            for sub in &spec.fields {
                if let Some(sub_html) = render_spec_value(
                    sub,
                    obj.get(&sub.key),
                    None,
                    "auto",
                    entry,
                    in_loop,
                    is_preview,
                    timezone,
                    locale,
                    l10n,
                    budget,
                    depth + 1,
                )? {
                    items.push_str(&format!(
                        "<div class=\"flex gap-1.5\"><dt class=\"font-medium text-od-muted\">{}:</dt><dd>{sub_html}</dd></div>",
                        escape_html(&sub.label),
                    ));
                }
            }
            Ok(
                (!items.is_empty())
                    .then(|| format!("<dl class=\"space-y-1 text-sm\">{items}</dl>")),
            )
        }
        FieldType::Repeater => {
            let rows: Vec<&serde_json::Map<String, Value>> = val
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_object)
                .take(if in_loop { 4 } else { 32 })
                .collect();
            if rows.is_empty() {
                return Ok(None);
            }
            let mut li_html = String::new();
            for row in rows {
                let mut row_parts = Vec::new();
                for sub in &spec.fields {
                    if let Some(sub_html) = render_spec_value(
                        sub,
                        row.get(&sub.key),
                        None,
                        "auto",
                        entry,
                        in_loop,
                        is_preview,
                        timezone,
                        locale,
                        l10n,
                        budget,
                        depth + 1,
                    )? {
                        row_parts.push(format!(
                            "<span><strong class=\"font-medium\">{}:</strong> {sub_html}</span>",
                            escape_html(&sub.label),
                        ));
                    }
                }
                if !row_parts.is_empty() {
                    li_html.push_str(&format!("<li>{}</li>", row_parts.join(" · ")));
                }
            }
            Ok((!li_html.is_empty())
                .then(|| format!("<ul class=\"list-disc pl-4 space-y-1 text-sm\">{li_html}</ul>")))
        }
        _ => Ok(None),
    }
}
