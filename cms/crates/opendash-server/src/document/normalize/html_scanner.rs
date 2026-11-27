use super::*;

impl DocumentNormalizationDiagnostic {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

pub(crate) fn is_forbidden_tag(name_lower: &str, allow_trusted_theme_markup: bool) -> bool {
    if allow_trusted_theme_markup
        && matches!(name_lower, "script" | "svg" | "math" | "foreignobject")
    {
        return false;
    }
    matches!(
        name_lower,
        "script"
            | "object"
            | "base"
            | "iframe"
            | "frame"
            | "frameset"
            | "embed"
            | "applet"
            | "svg"
            | "math"
            | "foreignobject"
            | "portal"
            | "xmp"
            | "noembed"
            | "noframes"
            | "plaintext"
    )
}

pub(crate) fn parse_attributes(
    attrs_raw: &str,
    allow_trusted_theme_markup: bool,
) -> Result<Vec<(String, String)>, DocumentNormalizationDiagnostic> {
    let bytes = attrs_raw.as_bytes();
    let mut i = 0;
    let mut attrs = Vec::new();
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let name_start = i;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'='
            && bytes[i] != b'/'
        {
            if matches!(bytes[i], b'"' | b'\'' | b'<' | b'>' | b'`') {
                return Err(DocumentNormalizationDiagnostic::new(
                    "invalid_attribute_syntax",
                    "Tên thuộc tính HTML chứa ký tự không hợp lệ.",
                ));
            }
            i += 1;
        }
        let name = attrs_raw[name_start..i].to_ascii_lowercase();
        if name.is_empty() {
            if bytes[i] == b'/' {
                i += 1;
                continue;
            }
            return Err(DocumentNormalizationDiagnostic::new(
                "invalid_attribute_syntax",
                "Cú pháp thuộc tính HTML không hợp lệ.",
            ));
        }
        if name.starts_with("on") && !allow_trusted_theme_markup {
            return Err(DocumentNormalizationDiagnostic::new(
                "forbidden_event_handler",
                format!("Thuộc tính sự kiện `{name}` không được phép."),
            ));
        }
        if attrs.iter().any(|(k, _)| k == &name) {
            return Err(DocumentNormalizationDiagnostic::new(
                "duplicate_attribute",
                format!("Thuộc tính `{name}` bị trùng lặp."),
            ));
        }
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let value = if i < bytes.len() && bytes[i] == b'=' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() {
                String::new()
            } else if bytes[i] == b'"' || bytes[i] == b'\'' {
                let quote = bytes[i];
                i += 1;
                let val_start = i;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                if i >= bytes.len() {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "unclosed_attribute_quote",
                        "Giá trị thuộc tính HTML chưa đóng dấu nháy.",
                    ));
                }
                let v = attrs_raw[val_start..i].to_string();
                i += 1;
                v
            } else {
                let val_start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    if matches!(bytes[i], b'"' | b'\'' | b'<' | b'>' | b'=' | b'`') {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "invalid_attribute_syntax",
                            "Giá trị thuộc tính HTML không có dấu nháy chứa ký tự không hợp lệ.",
                        ));
                    }
                    i += 1;
                }
                attrs_raw[val_start..i].to_string()
            }
        } else {
            String::new()
        };
        attrs.push((name, value));
    }
    Ok(attrs)
}

pub(crate) fn is_safe_body_attr(name: &str) -> bool {
    matches!(name, "class" | "id" | "lang" | "dir")
        || (name.starts_with("data-")
            && name.len() > 5
            && name[5..]
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'))
}

pub(crate) fn is_authorized_stylesheet_link(attrs: &[(String, String)]) -> bool {
    let mut rel_value: Option<String> = None;
    let mut href_raw: Option<&str> = None;
    for (k, v) in attrs {
        if k == "rel" {
            if rel_value.is_some() {
                return false;
            }
            rel_value = Some(opendash_core::richtext::decode_html5_entities(v));
        } else if k == "href" {
            if href_raw.is_some() {
                return false;
            }
            href_raw = Some(v.as_str());
        }
    }
    let Some(decoded_rel) = rel_value else {
        return false;
    };
    let Some(raw_href) = href_raw else {
        return false;
    };
    if !decoded_rel.trim().eq_ignore_ascii_case("stylesheet") {
        return false;
    }
    if raw_href
        .chars()
        .any(|c| c.is_whitespace() || c.is_control())
    {
        return false;
    }
    let decoded_href = opendash_core::richtext::decode_html5_entities(raw_href);
    if decoded_href
        .chars()
        .any(|c| c.is_whitespace() || c.is_control())
    {
        return false;
    }
    if decoded_href.contains(['?', '#', '\\']) {
        return false;
    }
    decoded_href.starts_with('/') && !decoded_href.starts_with("//") && !decoded_href.contains("..")
}

pub(crate) fn find_tag_end(
    raw: &str,
    start: usize,
) -> Result<usize, DocumentNormalizationDiagnostic> {
    let bytes = raw.as_bytes();
    let mut i = start;
    let mut state = TagScanState::BeforeAttrName;
    while i < bytes.len() {
        let b = bytes[i];
        match state {
            TagScanState::BeforeAttrName => {
                if b == b'>' {
                    return Ok(i);
                }
                if b.is_ascii_whitespace() || b == b'/' {
                    // stay in BeforeAttrName
                } else if matches!(b, b'"' | b'\'' | b'<' | b'=' | b'`') {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "invalid_tag_syntax",
                        "Cú pháp thẻ HTML chứa ký tự không hợp lệ ở vị trí tên thuộc tính.",
                    ));
                } else {
                    state = TagScanState::AttrName;
                }
            }
            TagScanState::AttrName => {
                if b == b'>' {
                    return Ok(i);
                }
                if b.is_ascii_whitespace() {
                    state = TagScanState::AfterAttrName;
                } else if b == b'/' {
                    state = TagScanState::BeforeAttrName;
                } else if b == b'=' {
                    state = TagScanState::BeforeAttrValue;
                } else if matches!(b, b'"' | b'\'' | b'<' | b'`') {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "invalid_tag_syntax",
                        "Tên thuộc tính HTML chứa ký tự không hợp lệ.",
                    ));
                }
            }
            TagScanState::AfterAttrName => {
                if b == b'>' {
                    return Ok(i);
                }
                if b.is_ascii_whitespace() {
                    // stay
                } else if b == b'/' {
                    state = TagScanState::BeforeAttrName;
                } else if b == b'=' {
                    state = TagScanState::BeforeAttrValue;
                } else if matches!(b, b'"' | b'\'' | b'<' | b'`') {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "invalid_tag_syntax",
                        "Cú pháp thẻ HTML chứa ký tự không hợp lệ sau tên thuộc tính.",
                    ));
                } else {
                    state = TagScanState::AttrName;
                }
            }
            TagScanState::BeforeAttrValue => {
                if b.is_ascii_whitespace() {
                    // stay
                } else if b == b'>' {
                    return Ok(i);
                } else if b == b'"' || b == b'\'' {
                    state = TagScanState::QuotedValue(b);
                } else if matches!(b, b'<' | b'=' | b'`') {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "invalid_tag_syntax",
                        "Giá trị thuộc tính HTML bắt đầu bằng ký tự không hợp lệ.",
                    ));
                } else {
                    state = TagScanState::UnquotedValue;
                }
            }
            TagScanState::QuotedValue(q) => {
                if b == q {
                    state = TagScanState::BeforeAttrName;
                }
            }
            TagScanState::UnquotedValue => {
                if b == b'>' {
                    return Ok(i);
                }
                if b.is_ascii_whitespace() {
                    state = TagScanState::BeforeAttrName;
                } else if matches!(b, b'"' | b'\'' | b'<' | b'=' | b'`') {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "invalid_tag_syntax",
                        "Giá trị thuộc tính HTML không có dấu nháy chứa ký tự không hợp lệ.",
                    ));
                }
            }
        }
        i += 1;
    }
    Err(DocumentNormalizationDiagnostic::new(
        "unclosed_tag",
        "Thẻ HTML chưa được đóng.",
    ))
}

pub(crate) fn find_case_insensitive_end_style(
    raw: &str,
    start: usize,
) -> Result<Option<(usize, usize)>, DocumentNormalizationDiagnostic> {
    let bytes = raw.as_bytes();
    let needle = b"</style";
    let mut i = start;
    while i + needle.len() < bytes.len() {
        if bytes[i] == b'<' && bytes[i..i + needle.len()].eq_ignore_ascii_case(needle) {
            let next_pos = i + needle.len();
            if next_pos < bytes.len() {
                let next_b = bytes[next_pos];
                if next_b == b'>' {
                    return Ok(Some((i, next_pos + 1)));
                }
                if next_b.is_ascii_whitespace() || next_b == b'/' {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "malformed_tag",
                        "Thẻ đóng </style> không được chứa khoảng trắng hoặc `/`. ",
                    ));
                }
            }
        }
        i += 1;
    }
    Ok(None)
}

// HTML script data is raw text: operators such as `a < b`, HTML-like strings,
// and quotes inside JavaScript must not be parsed as ordinary HTML tokens.
// A matching end tag begins with `</script` (ASCII case insensitive), followed
// by an HTML5 tag-name delimiter. Reject malformed closing tags rather than
// accidentally emitting a second executable script from the remaining bytes.
pub(crate) fn find_case_insensitive_end_script(
    raw: &str,
    start: usize,
) -> Result<Option<usize>, DocumentNormalizationDiagnostic> {
    let bytes = raw.as_bytes();
    let needle = b"</script";
    let mut i = start;
    while i + needle.len() < bytes.len() {
        if bytes[i..i + needle.len()].eq_ignore_ascii_case(needle) {
            let next = bytes[i + needle.len()];
            if next == b'>' {
                return Ok(Some(i + needle.len() + 1));
            }
            if next.is_ascii_whitespace() || next == b'/' {
                let end = find_tag_end(raw, i + needle.len())?;
                // A browser recognizes whitespace after the script end-tag name.
                // Do not accept trailing attributes or a slash on closing tags.
                if !raw[i + needle.len()..end].trim().is_empty() {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "malformed_tag",
                        "Thẻ đóng </script> chứa ký tự không hợp lệ.",
                    ));
                }
                return Ok(Some(end + 1));
            }
        }
        i += 1;
    }
    Ok(None)
}

pub(crate) fn find_html5_comment_end(bytes: &[u8], start: usize) -> Option<usize> {
    let after_open = start + 4;
    if bytes.get(after_open) == Some(&b'>') {
        return Some(after_open + 1);
    }
    if bytes.get(after_open..after_open + 2) == Some(b"->") {
        return Some(after_open + 2);
    }
    let mut j = after_open;
    while j < bytes.len() {
        if bytes[j..].starts_with(b"-->") {
            return Some(j + 3);
        }
        if bytes[j..].starts_with(b"--!>") {
            return Some(j + 4);
        }
        j += 1;
    }
    None
}
