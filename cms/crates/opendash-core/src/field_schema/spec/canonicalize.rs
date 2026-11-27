use super::*;
#[inline]

pub fn canonicalize_f64(value: f64) -> f64 {
    if value == 0.0 { 0.0 } else { value }
}

pub fn canonicalize_value(value: &mut Value) {
    match value {
        Value::Number(num) => {
            if num.is_f64()
                && let Some(f) = num.as_f64()
                && f == 0.0
                && let Some(zero) = serde_json::Number::from_f64(0.0)
            {
                *num = zero;
            }
        }
        Value::Array(items) => {
            for item in items {
                canonicalize_value(item);
            }
        }
        Value::Object(map) => {
            for val in map.values_mut() {
                canonicalize_value(val);
            }
        }
        _ => {}
    }
}

pub fn canonicalize_field_specs(specs: &mut [FieldSpec]) {
    for spec in specs {
        spec.min = spec.min.map(canonicalize_f64);
        spec.max = spec.max.map(canonicalize_f64);
        spec.step = spec.step.map(canonicalize_f64);
        if matches!(spec.default, Some(Value::Null)) {
            spec.default = None;
        }
        if let Some(def) = spec.default.as_mut() {
            canonicalize_value(def);
        }
        if let Some(conds) = spec.conditions.as_mut() {
            for group in conds.iter_mut() {
                for rule in group.iter_mut() {
                    if let Some(val) = rule.value.as_mut() {
                        canonicalize_value(val);
                    }
                }
            }
        }
        if !spec.fields.is_empty() {
            canonicalize_field_specs(&mut spec.fields);
        }
    }
}

pub fn valid_safe_url(raw: &str) -> bool {
    if raw.is_empty() || raw.len() > MAX_URL_BYTES {
        return false;
    }
    if raw
        .bytes()
        .any(|b| b <= 0x1F || b == 0x7F || b.is_ascii_whitespace())
        || raw.chars().any(char::is_whitespace)
    {
        return false;
    }
    if raw == "#" {
        return true;
    }
    if let Some(fragment) = raw.strip_prefix('#') {
        return !fragment.is_empty()
            && fragment.len() <= 128
            && fragment
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    }
    if raw.starts_with('/') {
        return !raw.starts_with("//") && !raw.contains('\\') && !raw.contains("..");
    }
    let lower_prefix_8 = raw.get(..8).map(str::to_ascii_lowercase);
    let lower_prefix_7 = raw.get(..7).map(str::to_ascii_lowercase);
    let lower_prefix_4 = raw.get(..4).map(str::to_ascii_lowercase);

    if lower_prefix_7.as_deref() == Some("mailto:") {
        let addr = &raw[7..];
        let email_part = addr.split('?').next().unwrap_or("");
        let Some((local, domain)) = email_part.split_once('@') else {
            return false;
        };
        return !local.is_empty()
            && !domain.is_empty()
            && domain.contains('.')
            && !domain.starts_with('.')
            && !domain.ends_with('.')
            && !addr.contains('\\');
    }
    if lower_prefix_4.as_deref() == Some("tel:") {
        let number = &raw[4..];
        return !number.is_empty()
            && number.len() <= 64
            && number.bytes().all(|b| {
                b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'(' | b')' | b'*' | b'#')
            })
            && number.bytes().any(|b| b.is_ascii_digit());
    }
    if lower_prefix_7.as_deref() == Some("http://") || lower_prefix_8.as_deref() == Some("https://")
    {
        let Ok(parsed) = Url::parse(raw) else {
            return false;
        };
        return matches!(parsed.scheme(), "http" | "https")
            && parsed.has_host()
            && parsed.username().is_empty()
            && parsed.password().is_none();
    }
    false
}

pub(crate) fn parse_iso_date_parts(raw: &str) -> Option<(i64, i64, i64)> {
    let b = raw.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return None;
    }
    let year = raw[0..4].parse::<i64>().ok()?;
    let month = raw[5..7].parse::<i64>().ok()?;
    let day = raw[8..10].parse::<i64>().ok()?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if year >= 1 && day >= 1 && day <= days_in_month {
        Some((year, month, day))
    } else {
        None
    }
}

pub(crate) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = year - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Parses canonical ISO-8601 UTC datetimes (`YYYY-MM-DDTHH:MM[:SS[.fffffffff]]Z`)
/// into UTC epoch milliseconds so `.000Z` and `Z` compare identically.
pub fn parse_utc_datetime_millis(raw: &str) -> Option<i64> {
    let (date_part, rest) = raw.split_once('T')?;
    let (year, month, day) = parse_iso_date_parts(date_part)?;
    let time_part = rest.strip_suffix('Z')?;
    let (hms, frac_ms) = if let Some((main, frac)) = time_part.split_once('.') {
        if frac.is_empty() || frac.len() > 9 || !frac.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let mut padded = [b'0'; 3];
        for (idx, b) in frac.as_bytes().iter().take(3).enumerate() {
            padded[idx] = *b;
        }
        let ms = std::str::from_utf8(&padded).ok()?.parse::<i64>().ok()?;
        (main, ms)
    } else {
        (time_part, 0)
    };
    let hms_bytes = hms.as_bytes();
    let (hour, min, sec) = match hms_bytes.len() {
        5 if hms_bytes[2] == b':'
            && hms_bytes[..2].iter().all(u8::is_ascii_digit)
            && hms_bytes[3..5].iter().all(u8::is_ascii_digit) =>
        {
            (
                hms[0..2].parse::<i64>().ok()?,
                hms[3..5].parse::<i64>().ok()?,
                0,
            )
        }
        8 if hms_bytes[2] == b':'
            && hms_bytes[5] == b':'
            && hms_bytes[0..2].iter().all(u8::is_ascii_digit)
            && hms_bytes[3..5].iter().all(u8::is_ascii_digit)
            && hms_bytes[6..8].iter().all(u8::is_ascii_digit) =>
        {
            (
                hms[0..2].parse::<i64>().ok()?,
                hms[3..5].parse::<i64>().ok()?,
                hms[6..8].parse::<i64>().ok()?,
            )
        }
        _ => return None,
    };
    if hour > 23 || min > 59 || sec > 59 {
        return None;
    }
    let days = days_from_civil(year, month, day);
    let total_secs = days
        .checked_mul(86_400)?
        .checked_add(hour * 3_600 + min * 60 + sec)?;
    total_secs.checked_mul(1_000)?.checked_add(frac_ms)
}

/// Parses `HH:MM` or `HH:MM:SS` (24h) into seconds since midnight (`0..=86399`).
pub fn parse_time_seconds(raw: &str) -> Option<u32> {
    let b = raw.as_bytes();
    let (hour, min, sec) = match b.len() {
        5 if b[2] == b':'
            && b[0..2].iter().all(u8::is_ascii_digit)
            && b[3..5].iter().all(u8::is_ascii_digit) =>
        {
            (
                raw[0..2].parse::<u32>().ok()?,
                raw[3..5].parse::<u32>().ok()?,
                0,
            )
        }
        8 if b[2] == b':'
            && b[5] == b':'
            && b[0..2].iter().all(u8::is_ascii_digit)
            && b[3..5].iter().all(u8::is_ascii_digit)
            && b[6..8].iter().all(u8::is_ascii_digit) =>
        {
            (
                raw[0..2].parse::<u32>().ok()?,
                raw[3..5].parse::<u32>().ok()?,
                raw[6..8].parse::<u32>().ok()?,
            )
        }
        _ => return None,
    };
    if hour > 23 || min > 59 || sec > 59 {
        return None;
    }
    Some(hour * 3_600 + min * 60 + sec)
}

pub fn valid_iso_time(raw: &str) -> bool {
    parse_time_seconds(raw).is_some()
}

pub(crate) fn validate_optional_plain_text(
    raw: Option<&str>,
    max_chars: usize,
    allow_newlines: bool,
    err: &'static str,
) -> Result<(), &'static str> {
    let Some(s) = raw else { return Ok(()) };
    if s.chars().count() > max_chars
        || s.chars()
            .any(|c| c.is_control() && !(allow_newlines && c == '\n'))
    {
        return Err(err);
    }
    Ok(())
}

pub(crate) fn is_non_empty_json_value(spec: &FieldSpec, val: &Value) -> bool {
    match val {
        Value::Null => false,
        Value::String(s) => !s.trim().is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => {
            if spec.field_type == FieldType::Link {
                map.get("url")
                    .and_then(Value::as_str)
                    .is_some_and(|u| !u.trim().is_empty())
            } else if spec.field_type == FieldType::Spacing {
                ["top", "right", "bottom", "left"]
                    .iter()
                    .any(|side| map.get(*side).is_some_and(|v| v.is_number()))
            } else {
                !map.is_empty()
            }
        }
        Value::Number(_) | Value::Bool(_) => true,
    }
}

pub fn normalize_field_specs_for_scope(
    specs: &mut [FieldSpec],
    ctx: FieldContext,
    scope: FieldScopeKind,
) -> Result<(), &'static str> {
    if !matches!(ctx, FieldContext::Content | FieldContext::Term) {
        return Err("normalize_field_specs_for_scope chỉ áp dụng cho Content và Term.");
    }
    if scope == FieldScopeKind::TermPanelField && ctx != FieldContext::Term {
        return Err("TermPanelField yêu cầu context FieldContext::Term.");
    }
    canonicalize_field_specs(specs);
    let (default_view, default_edit, default_pub) = match scope {
        FieldScopeKind::BaseCptField | FieldScopeKind::UserFieldGroup => {
            (FieldRole::Editor, FieldRole::Editor, PublicExposure::Public)
        }
        FieldScopeKind::ExtensionGroupField | FieldScopeKind::TermPanelField => (
            FieldRole::Editor,
            FieldRole::Editor,
            PublicExposure::Private,
        ),
    };
    normalize_acl_recursive(specs, scope, default_view, default_edit, default_pub)?;
    validate_field_specs(specs, ctx)
}

pub(crate) fn normalize_acl_recursive(
    specs: &mut [FieldSpec],
    scope: FieldScopeKind,
    default_view: FieldRole,
    default_edit: FieldRole,
    default_pub: PublicExposure,
) -> Result<(), &'static str> {
    for spec in specs {
        if spec.is_ui_only() {
            if spec.acl.is_some() {
                return Err("UI layout field không được khai báo acl.");
            }
            continue;
        }
        let existing = spec.acl.take().unwrap_or_default();
        let min_view_role = existing.min_view_role.unwrap_or(default_view);
        let min_edit_role = existing.min_edit_role.unwrap_or(default_edit);
        let public_exposure = existing.public_exposure.unwrap_or(default_pub);

        if min_edit_role < min_view_role {
            return Err("invalid_field_acl: min_edit_role không được thấp hơn min_view_role.");
        }
        if public_exposure == PublicExposure::Public && min_view_role != FieldRole::Editor {
            return Err(
                "invalid_field_acl: Trường có min_view_role = admin không được đặt public_exposure = public.",
            );
        }
        if scope == FieldScopeKind::TermPanelField && public_exposure == PublicExposure::Public {
            return Err("TermPanelField không hỗ trợ public_exposure = public ở Đợt 1.");
        }
        spec.acl = Some(FieldAcl::canonical(
            min_view_role,
            min_edit_role,
            public_exposure,
        ));
        if spec.effective_required() && min_edit_role == FieldRole::Admin {
            let valid = spec
                .default
                .as_ref()
                .is_some_and(|v| is_non_empty_json_value(spec, v));
            if !valid {
                return Err(
                    "admin_required_field_missing_default: Trường required với min_edit_role = admin bắt buộc có default hợp lệ, khác rỗng.",
                );
            }
        }
        if !spec.fields.is_empty() {
            normalize_acl_recursive(
                &mut spec.fields,
                scope,
                min_view_role,
                min_edit_role,
                public_exposure,
            )?;
        }
    }
    Ok(())
}
