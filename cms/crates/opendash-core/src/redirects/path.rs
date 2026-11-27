use crate::id::is_reserved_system_root;

use super::{
    MAX_NOT_FOUND_REFERRER_BYTES, MAX_NOT_FOUND_UA_BYTES, MAX_REDIRECT_NOTE_CHARS,
    MAX_REDIRECT_SOURCE_PATH_BYTES, MAX_REDIRECT_TARGET_URL_BYTES, RedirectValidationError,
};

fn decode_hex_pair(high: u8, low: u8) -> Option<u8> {
    let h = char::from(high).to_digit(16)?;
    let l = char::from(low).to_digit(16)?;
    Some(((h << 4) | l) as u8)
}

fn validate_path_segments(path: &str) -> Result<(), RedirectValidationError> {
    if !path.starts_with('/') {
        return Err(RedirectValidationError::SourcePathInvalidFormat);
    }
    if path.contains('\\')
        || path.contains('?')
        || path.contains('#')
        || path.contains("//")
        || path.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(RedirectValidationError::SourcePathInvalidFormat);
    }
    if path == "/" {
        return Ok(());
    }
    if path.ends_with('/') {
        return Err(RedirectValidationError::SourcePathInvalidFormat);
    }
    for seg in path[1..].split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return Err(RedirectValidationError::SourcePathInvalidFormat);
        }
    }
    Ok(())
}

/// Strictly decodes legacy percent-encoded URL paths without imposing a caller-specific
/// length limit.
///
/// Only `%20` (normalized to uppercase `"%20"`, never decoded into a literal space) and
/// valid multi-byte UTF-8 sequences (`>= 0x80`) are accepted. Every other `%XX`
/// (including ASCII `%2F`, `%25`, `%5C`, `%2E`, `%61`, control chars, invalid hex,
/// or invalid UTF-8) is rejected to prevent ASCII path aliasing.
pub fn decode_safe_legacy_request_path(raw: &str) -> Result<String, RedirectValidationError> {
    if !raw.starts_with('/') {
        return Err(RedirectValidationError::SourcePathInvalidFormat);
    }
    if raw.contains('\\')
        || raw.contains('?')
        || raw.contains('#')
        || raw.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(RedirectValidationError::SourcePathInvalidFormat);
    }
    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(RedirectValidationError::SourcePathUnsafeEncoding);
            }
            let decoded = decode_hex_pair(bytes[i + 1], bytes[i + 2])
                .ok_or(RedirectValidationError::SourcePathUnsafeEncoding)?;
            if decoded == 0x20 {
                out.push_str("%20");
                i += 3;
                continue;
            }
            if decoded < 0x80 {
                return Err(RedirectValidationError::SourcePathUnsafeEncoding);
            }
            let mut utf8_bytes = Vec::new();
            while i < bytes.len() && bytes[i] == b'%' {
                if i + 2 >= bytes.len() {
                    return Err(RedirectValidationError::SourcePathUnsafeEncoding);
                }
                let next = decode_hex_pair(bytes[i + 1], bytes[i + 2])
                    .ok_or(RedirectValidationError::SourcePathUnsafeEncoding)?;
                if next < 0x80 {
                    break;
                }
                utf8_bytes.push(next);
                i += 3;
            }
            let utf8_str = std::str::from_utf8(&utf8_bytes)
                .map_err(|_| RedirectValidationError::SourcePathUnsafeEncoding)?;
            if utf8_str
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
            {
                return Err(RedirectValidationError::SourcePathUnsafeEncoding);
            }
            out.push_str(utf8_str);
            continue;
        }
        let ch = raw[i..]
            .chars()
            .next()
            .ok_or(RedirectValidationError::SourcePathInvalidFormat)?;
        out.push(ch);
        i += ch.len_utf8();
    }
    validate_path_segments(&out)?;
    Ok(out)
}

pub fn normalize_redirect_source_path(raw: &str) -> Result<String, RedirectValidationError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(RedirectValidationError::EmptySourcePath);
    }
    if trimmed.contains('\\')
        || trimmed.contains('?')
        || trimmed.contains('#')
        || trimmed.contains("//")
        || trimmed.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(RedirectValidationError::SourcePathInvalidFormat);
    }
    let stripped = if trimmed.len() > 1 && trimmed.ends_with('/') {
        &trimmed[..trimmed.len() - 1]
    } else {
        trimmed
    };
    if stripped == "/" || stripped.ends_with('/') {
        return Err(RedirectValidationError::SourcePathInvalidFormat);
    }
    let normalized = if stripped.contains('%') {
        decode_safe_legacy_request_path(stripped)?
    } else {
        validate_path_segments(stripped)?;
        stripped.to_owned()
    };
    if normalized.len() > MAX_REDIRECT_SOURCE_PATH_BYTES {
        return Err(RedirectValidationError::SourcePathTooLong);
    }
    let first_seg = normalized[1..].split('/').next().unwrap_or("");
    if is_reserved_system_root(&first_seg.to_ascii_lowercase()) {
        return Err(RedirectValidationError::SourcePathReservedRoot);
    }
    Ok(normalized)
}

pub fn normalize_redirect_target_url(raw: &str) -> Result<String, RedirectValidationError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(RedirectValidationError::EmptyTargetUrl);
    }
    if trimmed.len() > MAX_REDIRECT_TARGET_URL_BYTES {
        return Err(RedirectValidationError::TargetUrlTooLong);
    }
    if trimmed.chars().any(char::is_control) {
        return Err(RedirectValidationError::TargetUrlInvalid);
    }
    if trimmed.starts_with('/') {
        if trimmed.starts_with("//") || trimmed.starts_with("/\\") {
            return Err(RedirectValidationError::TargetUrlUnsafeProtocolRelative);
        }
        let (before_frag, opt_frag) = match trimmed.split_once('#') {
            Some((b, f)) => (b, Some(f)),
            None => (trimmed, None),
        };
        let (path_raw, opt_query) = match before_frag.split_once('?') {
            Some((p, q)) => (p, Some(q)),
            None => (before_frag, None),
        };
        if path_raw.contains('\\') || path_raw.chars().any(char::is_whitespace) {
            return Err(RedirectValidationError::TargetUrlInvalid);
        }
        let stripped_path = if path_raw.len() > 1 && path_raw.ends_with('/') {
            &path_raw[..path_raw.len() - 1]
        } else {
            path_raw
        };
        let norm_path = if stripped_path == "/" {
            "/".to_owned()
        } else if stripped_path.contains('%') {
            decode_safe_legacy_request_path(stripped_path)
                .map_err(|_| RedirectValidationError::TargetUrlInvalid)?
        } else {
            validate_path_segments(stripped_path)
                .map_err(|_| RedirectValidationError::TargetUrlInvalid)?;
            stripped_path.to_owned()
        };
        let mut normalized = norm_path;
        if let Some(q) = opt_query {
            if !q.is_empty() {
                normalized.push('?');
                normalized.push_str(q);
            }
        }
        if let Some(frag) = opt_frag {
            if !frag.is_empty() {
                normalized.push('#');
                normalized.push_str(frag);
            }
        }
        if normalized.len() > MAX_REDIRECT_TARGET_URL_BYTES {
            return Err(RedirectValidationError::TargetUrlTooLong);
        }
        return Ok(normalized);
    }

    let parsed = url::Url::parse(trimmed).map_err(|_| RedirectValidationError::TargetUrlInvalid)?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(RedirectValidationError::TargetUrlInvalid);
    }
    let normalized = parsed.to_string();
    if normalized.len() > MAX_REDIRECT_TARGET_URL_BYTES {
        return Err(RedirectValidationError::TargetUrlTooLong);
    }
    Ok(normalized)
}

pub fn extract_internal_target_path(normalized_target: &str) -> Option<String> {
    if !normalized_target.starts_with('/') {
        return None;
    }
    let before_frag = normalized_target
        .split('#')
        .next()
        .unwrap_or(normalized_target);
    let path = before_frag.split('?').next().unwrap_or(before_frag);
    Some(path.to_owned())
}

pub fn extract_effective_target_path(
    normalized_target: &str,
    public_origin: Option<&str>,
) -> Option<String> {
    if normalized_target.starts_with('/') {
        return extract_internal_target_path(normalized_target);
    }
    let origin_raw = public_origin?.trim();
    if origin_raw.is_empty() {
        return None;
    }
    let origin_url = url::Url::parse(origin_raw).ok()?;
    let target_url = url::Url::parse(normalized_target).ok()?;
    let same_origin = target_url
        .scheme()
        .eq_ignore_ascii_case(origin_url.scheme())
        && match (target_url.host_str(), origin_url.host_str()) {
            (Some(th), Some(oh)) => th.eq_ignore_ascii_case(oh),
            _ => false,
        }
        && target_url.port_or_known_default() == origin_url.port_or_known_default();
    if !same_origin {
        return None;
    }
    let raw_path = target_url.path();
    let stripped = if raw_path.len() > 1 && raw_path.ends_with('/') {
        &raw_path[..raw_path.len() - 1]
    } else {
        raw_path
    };
    if stripped == "/" {
        return Some("/".to_owned());
    }
    if stripped.contains('%') {
        decode_safe_legacy_request_path(stripped).ok()
    } else if validate_path_segments(stripped).is_ok() {
        Some(stripped.to_owned())
    } else {
        None
    }
}

pub fn check_not_self_redirect(
    normalized_source: &str,
    normalized_target: &str,
    public_origin: Option<&str>,
) -> Result<(), RedirectValidationError> {
    if extract_effective_target_path(normalized_target, public_origin).as_deref()
        == Some(normalized_source)
    {
        return Err(RedirectValidationError::SelfRedirect);
    }
    Ok(())
}

pub fn build_redirect_location(normalized_target: &str, incoming_query: Option<&str>) -> String {
    let clean_query: String = incoming_query
        .unwrap_or("")
        .bytes()
        .filter(|&b| b >= 0x20 && b != 0x7F)
        .map(char::from)
        .collect();
    let (before_frag, opt_frag) = match normalized_target.split_once('#') {
        Some((b, f)) => (b, Some(f)),
        None => (normalized_target, None),
    };
    let mut assembled = String::with_capacity(normalized_target.len() + clean_query.len() + 2);
    assembled.push_str(before_frag);
    if !clean_query.is_empty() && !before_frag.contains('?') {
        assembled.push('?');
        assembled.push_str(&clean_query);
    }
    if let Some(frag) = opt_frag {
        assembled.push('#');
        assembled.push_str(frag);
    }
    let mut encoded = String::with_capacity(assembled.len());
    for &b in assembled.as_bytes() {
        if b < 0x20 || b == 0x7F {
            continue;
        }
        if b == b' ' {
            encoded.push_str("%20");
        } else if b >= 0x80 {
            encoded.push('%');
            encoded.push_str(&format!("{b:02X}"));
        } else {
            encoded.push(char::from(b));
        }
    }
    encoded
}

pub fn normalize_redirect_note(raw: Option<&str>) -> Result<String, RedirectValidationError> {
    let Some(raw) = raw else {
        return Ok(String::new());
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if trimmed.chars().any(char::is_control) {
        return Err(RedirectValidationError::NoteControlChars);
    }
    if trimmed.chars().count() > MAX_REDIRECT_NOTE_CHARS {
        return Err(RedirectValidationError::NoteTooLong);
    }
    Ok(trimmed.to_owned())
}

fn clamp_utf8_bytes(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_owned();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_owned()
}

pub fn sanitize_referer(raw: Option<&str>) -> Option<String> {
    let cleaned: String = raw?.chars().filter(|c| !c.is_control()).collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = if let Ok(mut parsed) = url::Url::parse(trimmed) {
        let _ = parsed.set_username("");
        let _ = parsed.set_password(None);
        parsed.to_string()
    } else {
        trimmed.to_owned()
    };
    let clamped = clamp_utf8_bytes(&normalized, MAX_NOT_FOUND_REFERRER_BYTES);
    if clamped.is_empty() {
        None
    } else {
        Some(clamped)
    }
}

pub fn sanitize_user_agent(raw: Option<&str>) -> Option<String> {
    let cleaned: String = raw?.chars().filter(|c| !c.is_control()).collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return None;
    }
    let clamped = clamp_utf8_bytes(trimmed, MAX_NOT_FOUND_UA_BYTES);
    if clamped.is_empty() {
        None
    } else {
        Some(clamped)
    }
}
