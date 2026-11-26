use super::*;

pub fn valid_email_address(raw: &str) -> bool {
    if raw.is_empty() || raw.len() > MAX_EMAIL_CHARS || raw.chars().count() > MAX_EMAIL_CHARS {
        return false;
    }
    if raw
        .bytes()
        .any(|b| b <= 0x1F || b >= 0x7F || b.is_ascii_whitespace())
    {
        return false;
    }
    let Some((local, domain)) = raw.split_once('@') else {
        return false;
    };
    if local.is_empty() || local.len() > 64 || domain.is_empty() || domain.contains('@') {
        return false;
    }
    if local.starts_with('.') || local.ends_with('.') || local.contains("..") {
        return false;
    }
    let valid_local = local.bytes().all(|b| {
        b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'.' | b'!'
                    | b'#'
                    | b'$'
                    | b'%'
                    | b'&'
                    | b'\''
                    | b'*'
                    | b'+'
                    | b'-'
                    | b'/'
                    | b'='
                    | b'?'
                    | b'^'
                    | b'_'
                    | b'`'
                    | b'{'
                    | b'|'
                    | b'}'
                    | b'~'
            )
    });
    if !valid_local || !domain.contains('.') {
        return false;
    }
    let labels: Vec<&str> = domain.split('.').collect();
    if labels.len() < 2 {
        return false;
    }
    for label in &labels {
        if label.is_empty() || label.len() > 63 {
            return false;
        }
        let bytes = label.as_bytes();
        if !bytes[0].is_ascii_alphanumeric() || !bytes[bytes.len() - 1].is_ascii_alphanumeric() {
            return false;
        }
        if !bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'-')
        {
            return false;
        }
    }
    let tld = labels[labels.len() - 1];
    tld.len() >= 2
}

pub fn valid_color_value(raw: &str) -> bool {
    if raw == "transparent" || raw == "currentColor" || ALLOWED_COLOR_TOKENS.contains(&raw) {
        return true;
    }
    let Some(hex) = raw.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 3 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn valid_css_var_name(raw: &str) -> bool {
    let Some(suffix) = raw.strip_prefix("--od-") else {
        return false;
    };
    !suffix.is_empty()
        && suffix.len() <= 48
        && suffix
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub fn valid_css_select_value(raw: &str) -> bool {
    !raw.is_empty()
        && raw.len() <= 32
        && raw
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

pub fn valid_sha256_media_id(raw: &str) -> bool {
    raw.len() == 64 && raw.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub fn valid_iso_date(raw: &str) -> bool {
    parse_iso_date_parts(raw).is_some()
}

pub fn valid_iso_datetime_utc(raw: &str) -> bool {
    parse_utc_datetime_millis(raw).is_some()
}

pub(super) fn collect_css_var<'a>(
    css_var: Option<&'a str>,
    seen_css_vars: &mut BTreeSet<&'a str>,
) -> Result<(), &'static str> {
    if let Some(var) = css_var {
        if !valid_css_var_name(var) {
            return Err("Tên biến CSS phải khớp ^--od-[a-z0-9-]{1,48}$.");
        }
        if !seen_css_vars.insert(var) {
            return Err("Biến CSS không được trùng lặp trong cùng một block.");
        }
    }
    Ok(())
}

pub(super) fn validate_option_list(
    options: &[SelectOption],
    max_len: usize,
    require_css_safe: bool,
    allow_icons: bool,
) -> Result<(), &'static str> {
    if options.is_empty() || options.len() > max_len {
        return Err("Số lượng lựa chọn vượt giới hạn hợp lệ.");
    }
    let mut seen = BTreeSet::new();
    for opt in options {
        if opt.value.is_empty()
            || opt.value.chars().count() > MAX_OPTION_VALUE_CHARS
            || opt.value.chars().any(char::is_control)
        {
            return Err("Giá trị option phải không rỗng và tối đa 64 ký tự.");
        }
        if !seen.insert(opt.value.as_str()) {
            return Err("Giá trị option không được trùng lặp.");
        }
        validate_label(&opt.label)?;
        if let Some(icon) = &opt.icon {
            if !allow_icons {
                return Err("Kiểu field này không hỗ trợ icon trên từng option.");
            }
            if lookup_core_icon_paths(icon).is_none() {
                return Err("Tên icon option không nằm trong danh mục CORE_ICONS.");
            }
        }
        if require_css_safe && !valid_css_select_value(&opt.value) {
            return Err("Option có css_var phải khớp ^[a-z0-9_-]{1,32}$.");
        }
    }
    Ok(())
}
