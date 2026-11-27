use super::*;
/// Validates a scoped block CSS stylesheet emitted by `page::serialize_scoped_block_css`.
/// Rejects CSS breakout tokens and allows `/api/admin/media/{64hex}/file` only when `is_preview == true`.

pub fn validate_safe_scoped_block_css(css: &str, is_preview: bool) -> Result<(), &'static str> {
    if css.contains('\0')
        || css.contains("/*")
        || css.contains("*/")
        || css.contains('\\')
        || css.contains('\r')
        || css.contains('\n')
    {
        return Err("Scoped block CSS chứa ký tự hoặc comment bị cấm.");
    }
    let lower = css.to_ascii_lowercase();
    if lower.contains("</style")
        || lower.contains("@import")
        || lower.contains("expression(")
        || lower.contains("-moz-binding")
        || lower.contains("behavior:")
    {
        return Err("Scoped block CSS vi phạm chính sách C-SEC.");
    }

    let mut idx = 0;
    while let Some(pos) = lower[idx..].find("url(") {
        let start = idx + pos + 4;
        let Some(end_rel) = lower[start..].find(')') else {
            return Err("CSS url(...) không hợp lệ.");
        };
        let raw_inner = css[start..start + end_rel].trim();
        let inner = raw_inner
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| {
                raw_inner
                    .strip_prefix('\'')
                    .and_then(|s| s.strip_suffix('\''))
            })
            .unwrap_or(raw_inner)
            .trim();
        let allowed_public = inner.starts_with("/theme/") || inner.starts_with("/media/");
        let allowed_preview = is_preview && inner.starts_with("/api/admin/media/");
        if !(allowed_public || allowed_preview)
            || inner.contains("..")
            || inner.contains('\\')
            || inner.contains('{')
            || inner.contains('}')
            || inner.contains(';')
            || inner.bytes().any(|b| b <= 0x1f || b == 0x7f)
        {
            return Err("Scoped block CSS url(...) không hợp lệ hoặc không được phép.");
        }
        idx = start + end_rel + 1;
    }
    Ok(())
}

/// Validates a static or post-render URL used in `href`, `src`, `poster`, `cite`, `srcset`, or `ping`.
pub(crate) fn decode_html_entities_for_url(raw: &str) -> String {
    opendash_core::richtext::decode_html5_entities(raw)
}

pub fn validate_safe_url(raw: &str) -> Result<(), &'static str> {
    if raw.contains('\0') || raw.bytes().any(|b| b <= 0x1f || b == 0x7f) {
        return Err("URL trong template chứa ký tự điều khiển bị cấm.");
    }
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.bytes().any(|b| b < 0x20 || b == 0x7f)
        || trimmed.contains('\\')
    {
        return Err("URL trong template không hợp lệ.");
    }
    if trimmed.starts_with('/') {
        if trimmed.starts_with("//")
            || trimmed == "/.."
            || trimmed.contains("/../")
            || trimmed.ends_with("/..")
            || trimmed.contains("..")
        {
            return Err("URL tương đối không được chứa // hoặc ..");
        }
        return Ok(());
    }
    let parsed = url::Url::parse(trimmed).map_err(|_| "URL không hợp lệ.")?;
    if !matches!(parsed.scheme(), "http" | "https") || !parsed.has_host() {
        return Err("Chỉ cho phép URL http(s) hoặc đường dẫn /...");
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("URL không được chứa thông tin đăng nhập.");
    }
    Ok(())
}

/// Validates a URL used specifically in `<a href="...">`, allowing `#`, `#fragment`,
/// `mailto:`, and `tel:` in addition to `/...` and `http(s)://...`.
pub fn validate_safe_href_url(raw: &str) -> Result<(), &'static str> {
    if raw.contains('\0') || raw.bytes().any(|b| b <= 0x1f || b == 0x7f) {
        return Err("URL href chứa ký tự điều khiển bị cấm.");
    }
    let trimmed = raw.trim();
    if validate_safe_url(raw).is_ok() || opendash_core::field_schema::valid_safe_url(trimmed) {
        Ok(())
    } else {
        Err("URL trong <a href> không hợp lệ hoặc không an toàn.")
    }
}

/// Validated navigation value for an explicitly typed theme option. Stored
/// bytes are never trimmed or decoded; checking the decoded variant only
/// prevents entity-based scheme and attribute bypasses.
pub fn validate_theme_option_url(raw: &str) -> Result<(), &'static str> {
    if raw.is_empty() || raw.chars().count() > 4096 {
        return Err("Theme URL phải có 1–4096 ký tự Unicode.");
    }
    let decoded = decode_html_entities_for_url(raw);
    for value in [raw, decoded.as_str()] {
        if value.chars().any(|ch| {
            ch.is_whitespace()
                || ch.is_control()
                || matches!(ch, '\\' | '<' | '>' | '\'' | '"' | '`')
        }) {
            return Err("Theme URL chứa ký tự không an toàn.");
        }
        validate_safe_href_url(value)?;
    }
    Ok(())
}

pub fn json_to_safe_value(value: &serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value::from(()),
        serde_json::Value::Bool(v) => Value::from(*v),
        serde_json::Value::Number(v) => {
            if let Some(i) = v.as_i64() {
                Value::from(i)
            } else if let Some(u) = v.as_u64() {
                Value::from(u)
            } else {
                Value::from(v.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(v) => Value::from(v.clone()),
        serde_json::Value::Array(v) => {
            Value::from(v.iter().map(json_to_safe_value).collect::<Vec<_>>())
        }
        serde_json::Value::Object(v) => Value::from_pairs(
            v.iter()
                .map(|(k, val)| (k.clone(), json_to_safe_value(val))),
        ),
    }
}

/// Converts a JSON context object into a MiniJinja `Value`, attaching pre-built
/// `Arc`-backed `t` and `locale` values without re-serializing them on every fragment.
pub fn json_context_with_shared_locale(
    value: &serde_json::Value,
    shared_t: Option<&Value>,
    shared_locale: Option<&Value>,
) -> Value {
    if shared_t.is_none() && shared_locale.is_none() {
        return json_to_safe_value(value);
    }
    let mut pairs: BTreeMap<String, Value> = match value {
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(k, val)| (k.clone(), json_to_safe_value(val)))
            .collect(),
        _ => BTreeMap::new(),
    };
    if let Some(t) = shared_t {
        pairs.entry("t".to_owned()).or_insert_with(|| t.clone());
    }
    if let Some(locale) = shared_locale {
        pairs
            .entry("locale".to_owned())
            .or_insert_with(|| locale.clone());
    }
    Value::from_pairs(pairs)
}
