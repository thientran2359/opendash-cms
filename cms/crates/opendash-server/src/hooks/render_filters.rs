use super::*;

pub(super) fn parse_and_sanitize_render_output(
    event: HookEvent,
    val: &Value,
    allow_trusted_html: bool,
) -> Option<crate::cache::CachedRenderFragment> {
    let (raw_html, raw_styles, raw_scripts): (String, Vec<String>, Vec<String>) = match val {
        Value::Null => return Some(crate::cache::CachedRenderFragment::default()),
        Value::String(s) => {
            if s.is_empty() {
                return Some(crate::cache::CachedRenderFragment::default());
            }
            (s.clone(), Vec::new(), Vec::new())
        }
        Value::Object(map) => {
            let html = match map.get("html") {
                None | Some(Value::Null) => String::new(),
                Some(Value::String(s)) => s.clone(),
                _ => return None,
            };
            let styles = match map.get("styles") {
                None | Some(Value::Null) => Vec::new(),
                Some(Value::Array(arr)) => {
                    let mut out = Vec::with_capacity(arr.len());
                    for item in arr {
                        out.push(item.as_str()?.to_owned());
                    }
                    out
                }
                _ => return None,
            };
            let head_scripts = match map.get("head_scripts") {
                None | Some(Value::Null) => Vec::new(),
                Some(Value::Array(arr)) => {
                    let mut out = Vec::with_capacity(arr.len());
                    for item in arr {
                        out.push(item.as_str()?.to_owned());
                    }
                    out
                }
                _ => return None,
            };
            (html, styles, head_scripts)
        }
        _ => return None,
    };

    let total_bytes = raw_html.len()
        + raw_styles.iter().map(String::len).sum::<usize>()
        + raw_scripts.iter().map(String::len).sum::<usize>();
    if total_bytes > MAX_RENDER_SUBSCRIBER_OUTPUT_BYTES {
        return None;
    }

    let mut fragment = crate::cache::CachedRenderFragment::default();
    if !allow_trusted_html {
        // Opt-out untrusted plugin: strip styles & head_scripts and enforce
        // strict C-SEC HTML/URL/CSS validation + non-trusted document normalizer.
        if !raw_html.is_empty() {
            crate::template_sec::validate_untrusted_extension_html_fragment(&raw_html).ok()?;
            let parts = if event == HookEvent::FilterRenderHead {
                crate::document::sanitize_render_head_fragment(&raw_html, false).ok()?
            } else {
                crate::document::normalize_public_document_with_options(&raw_html, false).ok()?
            };
            fragment.html = parts.body_fragment;
        }
        return Some(fragment);
    }

    if !raw_html.is_empty() {
        let parts = if event == HookEvent::FilterRenderHead {
            crate::document::sanitize_render_head_fragment(&raw_html, true).ok()?
        } else {
            crate::document::normalize_public_document_with_options(&raw_html, true).ok()?
        };
        fragment.html = parts.body_fragment;
        fragment.authorized_styles.extend(parts.authorized_styles);
        fragment
            .authorized_head_scripts
            .extend(parts.authorized_head_scripts);
    }

    for style_item in raw_styles {
        if style_item.trim().is_empty() {
            continue;
        }
        let wrapped = if style_item.trim_start().starts_with('<') {
            style_item
        } else {
            format!("<style>{style_item}</style>")
        };
        let parts = crate::document::sanitize_render_head_fragment(&wrapped, true).ok()?;
        fragment.authorized_styles.extend(parts.authorized_styles);
    }
    for script_item in raw_scripts {
        if script_item.trim().is_empty() {
            continue;
        }
        let wrapped = if script_item.trim_start().starts_with('<') {
            script_item
        } else {
            format!("<script>{script_item}</script>")
        };
        let parts = crate::document::sanitize_render_head_fragment(&wrapped, true).ok()?;
        fragment
            .authorized_head_scripts
            .extend(parts.authorized_head_scripts);
    }

    Some(fragment)
}
