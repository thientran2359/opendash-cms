use opendash_core::{
    builder::BlockNode,
    content::{EffectiveContentSchema, EffectiveFieldStatus, FieldRole, PublicExposure},
    field_schema::{FieldSpec, FieldType, valid_color_value, valid_safe_url},
    richtext::strip_richtext_to_plain,
};
use serde_json::Value;

use super::{
    blocks_chrome::preview_empty_placeholder,
    blocks_entry_field_complex::{render_complex_spec_value, render_image_markup_for_media},
    blocks_layout::str_prop,
    render::escape_html,
};
use crate::{
    content_render::{
        CoreL10n, DateFormatPreset, EntryView, core_l10n, format_number_locale,
        format_temporal_field,
    },
    template_sec::{RenderBudget, register_core_icon_in_budget},
};

pub(super) const LOOP_TEXT_CLAMP_CHARS: usize = 300;
const LOOP_URL_LABEL_CLAMP_CHARS: usize = 120;

pub(super) fn clamp_chars_ellipsis(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let prefix: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{}…", prefix.trim_end())
}

pub(super) fn escape_text_or_badge(raw: &str, display_format: &str) -> String {
    if display_format == "badge" {
        format!(
            "<span class=\"od-badge inline-block rounded-full bg-od-surface-alt px-2.5 py-0.5 text-xs font-medium\">{}</span>",
            escape_html(raw),
        )
    } else {
        format!("<span>{}</span>", escape_html(raw))
    }
}

fn render_core_field_value(
    field_key: &str,
    display_format: &str,
    entry: &EntryView,
    in_loop: bool,
    is_preview: bool,
) -> Option<String> {
    match field_key {
        "title" => {
            let txt = if in_loop {
                clamp_chars_ellipsis(&entry.title, LOOP_TEXT_CLAMP_CHARS)
            } else {
                entry.title.clone()
            };
            (!txt.trim().is_empty()).then(|| escape_text_or_badge(&txt, display_format))
        }
        "slug" => {
            let txt = entry.slug.trim();
            (!txt.is_empty()).then(|| escape_text_or_badge(txt, display_format))
        }
        "url" => {
            let url = entry
                .url
                .as_deref()
                .map(str::trim)
                .filter(|u| valid_safe_url(u))?;
            let lbl = if in_loop {
                clamp_chars_ellipsis(url, LOOP_URL_LABEL_CLAMP_CHARS)
            } else {
                url.to_owned()
            };
            if display_format == "text" {
                Some(format!("<span>{}</span>", escape_html(&lbl)))
            } else {
                Some(format!(
                    "<a href=\"{}\" class=\"underline hover:text-od-primary\">{}</a>",
                    escape_html(url),
                    escape_html(&lbl),
                ))
            }
        }
        "cover_url" => {
            if let Some(cover) = &entry.cover {
                return render_image_markup_for_media(
                    cover,
                    in_loop,
                    is_preview,
                    "w-full h-auto rounded object-cover",
                    &entry.title,
                );
            }
            let url = entry
                .cover_url
                .as_deref()
                .map(str::trim)
                .filter(|u| opendash_core::media::valid_media_render_url(u, is_preview))?;
            Some(format!(
                "<img src=\"{}\" alt=\"{}\" class=\"w-full h-auto rounded object-cover\" loading=\"lazy\" decoding=\"async\">",
                escape_html(url),
                escape_html(&entry.title),
            ))
        }
        _ => None,
    }
}

fn render_string_list_pills(
    val: Option<&Value>,
    in_loop: bool,
    display_format: &str,
) -> Option<String> {
    let items: Vec<String> = val
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .take(if in_loop { 6 } else { 32 })
        .map(str::to_owned)
        .collect();
    if items.is_empty() {
        return None;
    }
    if display_format == "text" {
        return Some(format!("<span>{}</span>", escape_html(&items.join(", "))));
    }
    let pills: Vec<String> = items
        .iter()
        .map(|item| {
            format!(
                "<span class=\"od-badge inline-block rounded-full bg-od-surface-alt px-2 py-0.5 text-xs\">{}</span>",
                escape_html(item)
            )
        })
        .collect();
    Some(format!(
        "<span class=\"inline-flex flex-wrap gap-1\">{}</span>",
        pills.join("")
    ))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_spec_value(
    spec: &FieldSpec,
    val: Option<&Value>,
    richtext_override: Option<&str>,
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
    if spec.is_ui_only()
        || matches!(spec.field_type, FieldType::Secret | FieldType::Spacing)
        || depth > 4
    {
        return Ok(None);
    }

    match spec.field_type {
        FieldType::Text | FieldType::Textarea | FieldType::RadioButtons => {
            let Some(s) = val
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                return Ok(None);
            };
            let clamped = if in_loop {
                clamp_chars_ellipsis(s, LOOP_TEXT_CLAMP_CHARS)
            } else {
                s.to_owned()
            };
            Ok(Some(escape_text_or_badge(&clamped, display_format)))
        }
        FieldType::Select => {
            if spec.multiple {
                Ok(render_string_list_pills(val, in_loop, display_format))
            } else {
                let Some(s) = val
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                else {
                    return Ok(None);
                };
                let clamped = if in_loop {
                    clamp_chars_ellipsis(s, LOOP_TEXT_CLAMP_CHARS)
                } else {
                    s.to_owned()
                };
                Ok(Some(escape_text_or_badge(&clamped, display_format)))
            }
        }
        FieldType::Checkbox => Ok(render_string_list_pills(val, in_loop, display_format)),
        FieldType::Email => {
            let Some(s) = val
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                return Ok(None);
            };
            let mailto = format!("mailto:{s}");
            if display_format == "text" || !valid_safe_url(&mailto) {
                Ok(Some(format!("<span>{}</span>", escape_html(s))))
            } else {
                Ok(Some(format!(
                    "<a href=\"{}\" class=\"underline hover:text-od-primary\">{}</a>",
                    escape_html(&mailto),
                    escape_html(s),
                )))
            }
        }
        FieldType::Url => {
            let Some(u) = val
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty() && valid_safe_url(s))
            else {
                return Ok(None);
            };
            let label = if in_loop {
                clamp_chars_ellipsis(u, LOOP_URL_LABEL_CLAMP_CHARS)
            } else {
                u.to_owned()
            };
            if display_format == "text" {
                Ok(Some(format!("<span>{}</span>", escape_html(&label))))
            } else {
                Ok(Some(format!(
                    "<a href=\"{}\" class=\"underline hover:text-od-primary\" rel=\"noopener noreferrer\">{}</a>",
                    escape_html(u),
                    escape_html(&label),
                )))
            }
        }
        FieldType::Link => {
            let Some(obj) = val.and_then(Value::as_object) else {
                return Ok(None);
            };
            let Some(u) = obj
                .get("url")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty() && valid_safe_url(s))
            else {
                return Ok(None);
            };
            let label = obj
                .get("label")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(u);
            if display_format == "text" {
                Ok(Some(format!("<span>{}</span>", escape_html(label))))
            } else if display_format == "badge" {
                Ok(Some(format!(
                    "<a href=\"{}\" class=\"od-badge inline-block rounded-full bg-od-surface-alt px-2.5 py-0.5 text-xs font-medium hover:underline\" rel=\"noopener noreferrer\">{}</a>",
                    escape_html(u),
                    escape_html(label),
                )))
            } else {
                let target = if obj.get("new_tab").and_then(Value::as_bool) == Some(true) {
                    " target=\"_blank\" rel=\"noopener noreferrer\""
                } else {
                    " rel=\"noopener noreferrer\""
                };
                Ok(Some(format!(
                    "<a href=\"{}\"{target} class=\"underline hover:text-od-primary\">{}</a>",
                    escape_html(u),
                    escape_html(label),
                )))
            }
        }
        FieldType::Number | FieldType::Range => {
            let Some(num) = val.and_then(Value::as_f64).filter(|f| f.is_finite()) else {
                return Ok(None);
            };
            Ok(Some(escape_text_or_badge(
                &format_number_locale(num, locale),
                display_format,
            )))
        }
        FieldType::Boolean => {
            let Some(flag) = val.and_then(Value::as_bool) else {
                return Ok(None);
            };
            let text = if flag {
                l10n.yes.as_ref()
            } else {
                l10n.no.as_ref()
            };
            Ok(Some(escape_text_or_badge(text, display_format)))
        }
        FieldType::Date | FieldType::Datetime | FieldType::Time => {
            let Some(s) = val
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                return Ok(None);
            };
            let fmt = format_temporal_field(
                spec.field_type,
                s,
                DateFormatPreset::DmySlash,
                timezone,
                locale,
            );
            Ok(Some(escape_text_or_badge(&fmt, display_format)))
        }
        FieldType::Color => {
            let Some(s) = val
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty() && valid_color_value(s))
            else {
                return Ok(None);
            };
            let esc_c = escape_html(s);
            if display_format == "text" {
                Ok(Some(format!("<span>{esc_c}</span>")))
            } else {
                Ok(Some(format!(
                    "<span class=\"inline-flex items-center gap-1.5 text-xs\"><span class=\"inline-block h-3.5 w-3.5 rounded-full border border-od-border\" style=\"background-color:{esc_c};\" aria-hidden=\"true\"></span><code>{esc_c}</code></span>"
                )))
            }
        }
        FieldType::Icon => {
            let Some(name) = val
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                return Ok(None);
            };
            Ok(register_core_icon_in_budget(
                budget,
                name,
                Some("w-4 h-4 inline-block"),
            )?)
        }
        FieldType::Richtext => {
            let clean_html = richtext_override
                .map(str::to_owned)
                .or_else(|| {
                    val.and_then(Value::as_str)
                        .filter(|s| !s.trim().is_empty() && !s.contains("OD_RICHTEXT_"))
                        .and_then(|raw| opendash_core::richtext::sanitize_richtext(raw).ok())
                })
                .filter(|s| !s.trim().is_empty());
            let Some(html) = clean_html else {
                return Ok(None);
            };
            render_sanitized_richtext_field(&html, display_format, in_loop, budget)
        }
        _ => render_complex_spec_value(
            spec,
            val,
            display_format,
            entry,
            in_loop,
            is_preview,
            timezone,
            locale,
            l10n,
            budget,
            depth,
        ),
    }
}

fn render_sanitized_richtext_field(
    html: &str,
    display_format: &str,
    in_loop: bool,
    budget: &mut RenderBudget,
) -> Result<Option<String>, &'static str> {
    if in_loop || display_format == "text" {
        let plain = strip_richtext_to_plain(html);
        let clean = clamp_chars_ellipsis(
            plain
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .as_str(),
            LOOP_TEXT_CLAMP_CHARS,
        );
        return Ok(
            (!clean.trim().is_empty()).then(|| format!("<span>{}</span>", escape_html(&clean)))
        );
    }
    let nonce = budget.richtext_nonces.register_sanitized(html.to_owned())?;
    Ok(Some(format!("<div class=\"od-prose\">{nonce}</div>")))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_entry_field(
    node: &BlockNode,
    entry: Option<&EntryView>,
    schema: Option<&EffectiveContentSchema>,
    in_loop: bool,
    is_preview: bool,
    timezone: &str,
    budget: &mut RenderBudget,
) -> Result<Option<String>, &'static str> {
    let field_key = str_prop(&node.props, "field_key", "");
    let Some(entry) = entry.filter(|_| !field_key.is_empty()) else {
        return Ok(preview_empty_placeholder("Entry Field", is_preview));
    };
    let label_mode = str_prop(&node.props, "label_mode", "inline");
    let custom_label = node
        .props
        .get("custom_label")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    let display_format = str_prop(&node.props, "display_format", "auto");
    let locale = budget.locale.clone();
    let l10n = core_l10n(&locale);

    let (label_text, value_html): (String, Option<String>) =
        if matches!(field_key, "title" | "slug" | "url" | "cover_url") {
            let lbl = if !custom_label.is_empty() {
                custom_label.to_owned()
            } else {
                field_key.to_owned()
            };
            (
                lbl,
                render_core_field_value(field_key, display_format, entry, in_loop, is_preview),
            )
        } else if let Some(eff) = schema {
            let Some(fe) = eff.effective_fields.get(field_key) else {
                return Ok(preview_empty_placeholder("Entry Field", is_preview));
            };
            let (min_view, _, exposure) = fe.definition.effective_acl();
            if fe.effective_status != EffectiveFieldStatus::Active
                || exposure != PublicExposure::Public
                || min_view > FieldRole::Editor
            {
                return Ok(preview_empty_placeholder("Entry Field", is_preview));
            }
            let lbl = if !custom_label.is_empty() {
                custom_label.to_owned()
            } else {
                fe.definition.label.clone()
            };
            let rt_html = entry.richtext_html.get(field_key).map(String::as_str);
            let html = render_spec_value(
                &fe.definition,
                entry.fields.get(field_key),
                rt_html,
                display_format,
                entry,
                in_loop,
                is_preview,
                timezone,
                &locale,
                &l10n,
                budget,
                0,
            )?;
            (lbl, html)
        } else if let Some(rt_html) = entry.richtext_html.get(field_key) {
            let lbl = if !custom_label.is_empty() {
                custom_label.to_owned()
            } else {
                entry
                    .field_list
                    .iter()
                    .find(|f| f.key == field_key)
                    .map(|f| f.label.clone())
                    .unwrap_or_else(|| field_key.to_owned())
            };
            let html = render_sanitized_richtext_field(rt_html, display_format, in_loop, budget)?;
            (lbl, html)
        } else {
            let fv = entry.field_list.iter().find(|f| f.key == field_key);
            let lbl = if !custom_label.is_empty() {
                custom_label.to_owned()
            } else {
                fv.map(|f| f.label.clone())
                    .unwrap_or_else(|| field_key.to_owned())
            };
            let html = fv.and_then(|f| {
                let txt = f.text.trim();
                (!txt.is_empty()).then(|| {
                    let s = if in_loop {
                        clamp_chars_ellipsis(txt, LOOP_TEXT_CLAMP_CHARS)
                    } else {
                        txt.to_owned()
                    };
                    escape_text_or_badge(&s, display_format)
                })
            });
            (lbl, html)
        };

    let Some(val_html) = value_html else {
        return Ok(preview_empty_placeholder("Entry Field", is_preview));
    };
    let out = match label_mode {
        "hide" => format!("<div class=\"od-entry-field\">{val_html}</div>"),
        "stacked" => format!(
            "<div class=\"od-entry-field flex flex-col gap-1\"><span class=\"text-xs font-medium text-od-muted\">{}</span><div>{val_html}</div></div>",
            escape_html(&label_text),
        ),
        _ => format!(
            "<div class=\"od-entry-field flex flex-wrap items-baseline gap-1.5\"><span class=\"font-medium text-od-muted\">{}:</span><div>{val_html}</div></div>",
            escape_html(&label_text),
        ),
    };
    Ok(Some(out))
}
