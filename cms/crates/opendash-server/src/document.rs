//! M7-S03-T02 bounded single-pass trusted HTML document normalizer.
//! Strips untrusted head/meta/canonical/title tags and validates envelope structure
//! without regexes or literal `<body>` substring splitting.

pub const MAX_PUBLIC_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeDocumentParts {
    pub html_lang: Option<String>,
    pub html_dir: Option<String>,
    pub body_attributes: Vec<(String, String)>,
    pub body_fragment: String,
    pub authorized_styles: Vec<String>,
    pub authorized_head_scripts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentNormalizationDiagnostic {
    pub code: &'static str,
    pub message: String,
}

impl DocumentNormalizationDiagnostic {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind<'a> {
    Text(&'a str),
    Comment(&'a str),
    Doctype(&'a str),
    OpenTag {
        name_lower: &'a str,
        raw: &'a str,
        attrs_raw: &'a str,
        self_closing: bool,
    },
    CloseTag {
        name_lower: &'a str,
    },
    StyleBlock(&'a str),
    ScriptBlock(&'a str),
}

fn is_forbidden_tag(name_lower: &str, allow_trusted_theme_markup: bool) -> bool {
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

fn parse_attributes(
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
            i += 1;
        }
        let name = attrs_raw[name_start..i].to_ascii_lowercase();
        if name.is_empty() {
            i += 1;
            continue;
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

fn is_safe_body_attr(name: &str) -> bool {
    matches!(name, "class" | "id" | "lang" | "dir")
        || (name.starts_with("data-")
            && name.len() > 5
            && name[5..]
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'))
}

fn is_authorized_stylesheet_link(attrs: &[(String, String)]) -> bool {
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

fn find_tag_end(raw: &str, start: usize) -> Result<usize, DocumentNormalizationDiagnostic> {
    let bytes = raw.as_bytes();
    let mut i = start;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = quote {
            if b == q {
                quote = None;
            }
        } else if b == b'"' || b == b'\'' {
            quote = Some(b);
        } else if b == b'>' {
            return Ok(i);
        }
        i += 1;
    }
    Err(DocumentNormalizationDiagnostic::new(
        "unclosed_tag",
        "Thẻ HTML chưa được đóng.",
    ))
}

fn find_case_insensitive_end_style(
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
fn find_case_insensitive_end_script(
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

fn find_html5_comment_end(bytes: &[u8], start: usize) -> Option<usize> {
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

fn tokenize(
    raw: &str,
    allow_trusted_theme_markup: bool,
) -> Result<Vec<(TokenKind<'_>, String)>, DocumentNormalizationDiagnostic> {
    if raw.len() > MAX_PUBLIC_DOCUMENT_BYTES {
        return Err(DocumentNormalizationDiagnostic::new(
            "document_too_large",
            "Tài liệu HTML vượt quá giới hạn 4 MiB.",
        ));
    }
    let bytes = raw.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        if bytes[i] != b'<' {
            let text_start = i;
            while i < bytes.len() && bytes[i] != b'<' {
                i += 1;
            }
            out.push((TokenKind::Text(&raw[text_start..i]), String::new()));
            continue;
        }
        match bytes.get(i + 1).copied() {
            Some(b'?') => {
                return Err(DocumentNormalizationDiagnostic::new(
                    "malformed_tag",
                    "Tài liệu HTML không được chứa `<?`.",
                ));
            }
            Some(b'!') => {
                if raw[i..].starts_with("<!--") {
                    let Some(end) = find_html5_comment_end(bytes, i) else {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "unclosed_comment",
                            "Chú thích HTML chưa được đóng.",
                        ));
                    };
                    out.push((TokenKind::Comment(&raw[i..end]), String::new()));
                    i = end;
                    continue;
                }
                if raw[i..].len() >= 9
                    && raw[i..i + 9].eq_ignore_ascii_case("<!doctype")
                    && bytes
                        .get(i + 9)
                        .is_some_and(|b| *b == b'>' || b.is_ascii_whitespace())
                {
                    let mut j = i + 9;
                    let mut end = None;
                    while j < bytes.len() {
                        let b = bytes[j];
                        if b == b'>' {
                            end = Some(j);
                            break;
                        }
                        if matches!(b, b'<' | b'"' | b'\'' | 0x00..=0x1f | 0x7f) {
                            return Err(DocumentNormalizationDiagnostic::new(
                                "malformed_tag",
                                "DOCTYPE chứa ký tự không hợp lệ.",
                            ));
                        }
                        j += 1;
                    }
                    let Some(end) = end else {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "unclosed_tag",
                            "DOCTYPE chưa được đóng.",
                        ));
                    };
                    out.push((TokenKind::Doctype(&raw[i..=end]), String::new()));
                    i = end + 1;
                    continue;
                }
                return Err(DocumentNormalizationDiagnostic::new(
                    "malformed_tag",
                    "Chỉ thị `<!` không hợp lệ trong HTML công khai.",
                ));
            }
            Some(b'/') => {
                if !bytes.get(i + 2).is_some_and(u8::is_ascii_alphabetic) {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "malformed_tag",
                        "Thẻ đóng HTML phải bắt đầu bằng ký tự chữ cái ASCII.",
                    ));
                }
                let end = find_tag_end(raw, i + 2)?;
                let rest = &raw[i + 2..end];
                let name_end = rest
                    .find(|c: char| c.is_ascii_whitespace())
                    .unwrap_or(rest.len());
                let name_slice = &rest[..name_end];
                if !name_slice
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                    || !rest[name_end..].trim().is_empty()
                {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "malformed_tag",
                        "Thẻ đóng HTML chứa ký tự hoặc thuộc tính không hợp lệ.",
                    ));
                }
                let name_lower = name_slice.to_ascii_lowercase();
                if is_forbidden_tag(&name_lower, allow_trusted_theme_markup) {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "forbidden_tag",
                        format!("Thẻ `{name_lower}` không được phép."),
                    ));
                }
                if name_lower == "script" {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "unexpected_close_script",
                        "Thẻ </script> không có thẻ mở tương ứng.",
                    ));
                }
                out.push((TokenKind::CloseTag { name_lower: "" }, name_lower));
                i = end + 1;
                continue;
            }
            Some(b) if b.is_ascii_alphabetic() => {
                let end = find_tag_end(raw, i + 1)?;
                let tag_full = &raw[i..=end];
                let inner = &raw[i + 1..end];
                let self_closing = inner.ends_with('/');
                let inner_clean = if self_closing {
                    inner[..inner.len() - 1].trim_end()
                } else {
                    inner
                };
                let name_end = inner_clean
                    .find(|c: char| c.is_ascii_whitespace())
                    .unwrap_or(inner_clean.len());
                let name_slice = &inner_clean[..name_end];
                if name_slice.is_empty()
                    || !name_slice
                        .bytes()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == b'-')
                {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "malformed_tag",
                        "Tên thẻ HTML không hợp lệ.",
                    ));
                }
                let name_lower = name_slice.to_ascii_lowercase();
                if is_forbidden_tag(&name_lower, allow_trusted_theme_markup) {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "forbidden_tag",
                        format!("Thẻ `{name_lower}` không được phép."),
                    ));
                }
                let attrs_raw = &inner_clean[name_end..];
                let attrs = parse_attributes(attrs_raw, allow_trusted_theme_markup)?;
                if name_lower == "script" {
                    if self_closing {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "self_closing_script",
                            "Thẻ <script/> tự đóng không được phép.",
                        ));
                    }
                    let Some(after_close) = find_case_insensitive_end_script(raw, end + 1)? else {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "unclosed_script",
                            "Thẻ <script> chưa được đóng.",
                        ));
                    };
                    if raw[end + 1..after_close].contains("<!--")
                        || raw[end + 1..after_close].contains("<![CDATA[")
                    {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "script_escaped_state",
                            "HTML escaped state hoặc SVG CDATA trong <script> không an toàn.",
                        ));
                    }
                    let _ = attrs;
                    out.push((
                        TokenKind::ScriptBlock(&raw[i..after_close]),
                        "script".to_string(),
                    ));
                    i = after_close;
                    continue;
                }
                if name_lower == "style" && !self_closing {
                    let Some((style_end, after_close)) =
                        find_case_insensitive_end_style(raw, end + 1)?
                    else {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "unclosed_style",
                            "Thẻ <style> chưa được đóng.",
                        ));
                    };
                    let _ = attrs;
                    let full_style = &raw[i..after_close];
                    let _ = style_end;
                    out.push((TokenKind::StyleBlock(full_style), "style".to_string()));
                    i = after_close;
                    continue;
                }
                out.push((
                    TokenKind::OpenTag {
                        name_lower: "",
                        raw: tag_full,
                        attrs_raw,
                        self_closing,
                    },
                    name_lower,
                ));
                i = end + 1;
            }
            _ => {
                out.push((TokenKind::Text(&raw[i..i + 1]), String::new()));
                i += 1;
            }
        }
    }
    Ok(out)
}

pub fn normalize_public_document(
    raw_html: &str,
) -> Result<SafeDocumentParts, DocumentNormalizationDiagnostic> {
    normalize_public_document_with_options(raw_html, false)
}

pub fn normalize_public_document_with_options(
    raw_html: &str,
    allow_trusted_theme_markup: bool,
) -> Result<SafeDocumentParts, DocumentNormalizationDiagnostic> {
    let tokens = tokenize(raw_html, allow_trusted_theme_markup)?;

    // SVG foreignObject may embed HTML <body>/<html> as foreign content. It
    // must not turn a normal Theme fragment into a document envelope.
    let mut envelope_svg_depth = 0usize;
    let has_envelope = tokens.iter().any(|(tok, name)| match tok {
        TokenKind::OpenTag { self_closing, .. } if name == "svg" => {
            if !self_closing {
                envelope_svg_depth += 1;
            }
            false
        }
        TokenKind::CloseTag { .. } if name == "svg" => {
            envelope_svg_depth = envelope_svg_depth.saturating_sub(1);
            false
        }
        TokenKind::Doctype(_) => true,
        TokenKind::OpenTag { .. } | TokenKind::CloseTag { .. }
            if matches!(name.as_str(), "html" | "head" | "body") =>
        {
            envelope_svg_depth == 0
        }
        _ => false,
    });

    if !has_envelope {
        let mut authorized_styles = Vec::new();
        let authorized_head_scripts = Vec::new();
        let mut body_fragment = String::with_capacity(raw_html.len());
        let mut svg_depth = 0usize;
        for (tok, name) in tokens {
            match tok {
                TokenKind::Text(t) | TokenKind::Comment(t) => body_fragment.push_str(t),
                TokenKind::StyleBlock(s) => authorized_styles.push(s.to_string()),
                TokenKind::ScriptBlock(s) => body_fragment.push_str(s),
                TokenKind::OpenTag {
                    raw,
                    attrs_raw,
                    self_closing,
                    ..
                } => {
                    if name == "link" {
                        let attrs = parse_attributes(attrs_raw, allow_trusted_theme_markup)?;
                        if is_authorized_stylesheet_link(&attrs) {
                            authorized_styles.push(raw.to_string());
                        }
                    } else if name != "meta" && (name != "title" || svg_depth > 0) {
                        body_fragment.push_str(raw);
                    }
                    if name == "svg" && !self_closing {
                        svg_depth += 1;
                    }
                }
                TokenKind::CloseTag { .. } => {
                    if name != "title" || svg_depth > 0 {
                        body_fragment.push_str(&format!("</{name}>"));
                    }
                    if name == "svg" {
                        svg_depth = svg_depth.saturating_sub(1);
                    }
                }
                TokenKind::Doctype(_) => {}
            }
        }
        return Ok(SafeDocumentParts {
            html_lang: None,
            html_dir: None,
            body_attributes: Vec::new(),
            body_fragment,
            authorized_styles,
            authorized_head_scripts,
        });
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Phase {
        BeforeHtml,
        InHtmlBeforeHead,
        InHead,
        InHtmlBetweenHeadAndBody,
        InBody,
        AfterBody,
        AfterHtml,
    }

    let mut phase = Phase::BeforeHtml;
    let mut seen_doctype = false;
    let mut seen_head = false;
    let mut seen_body = false;
    let mut html_lang: Option<String> = None;
    let mut html_dir: Option<String> = None;
    let mut body_attributes = Vec::new();
    let mut authorized_styles = Vec::new();
    let mut authorized_head_scripts = Vec::new();
    let mut body_fragment = String::new();
    let mut svg_depth = 0usize;

    for (tok, name) in tokens {
        match tok {
            TokenKind::Doctype(_) => {
                if phase != Phase::BeforeHtml || seen_doctype {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "duplicate_or_misplaced_doctype",
                        "DOCTYPE trùng lặp hoặc đặt sai vị trí.",
                    ));
                }
                seen_doctype = true;
            }
            TokenKind::Text(t) => match phase {
                Phase::InBody => body_fragment.push_str(t),
                Phase::InHead => {}
                Phase::BeforeHtml
                | Phase::InHtmlBeforeHead
                | Phase::InHtmlBetweenHeadAndBody
                | Phase::AfterBody
                | Phase::AfterHtml => {
                    if !t.trim().is_empty() {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "text_outside_body",
                            "Nội dung văn bản nằm ngoài thẻ <body>.",
                        ));
                    }
                }
            },
            TokenKind::Comment(c) => {
                if phase == Phase::InBody {
                    body_fragment.push_str(c);
                }
            }
            TokenKind::StyleBlock(s) => match phase {
                Phase::InHead | Phase::InBody => authorized_styles.push(s.to_string()),
                _ => {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "style_outside_envelope",
                        "Thẻ <style> nằm ngoài <head>/<body>.",
                    ));
                }
            },
            TokenKind::ScriptBlock(s) => match phase {
                Phase::InHead => authorized_head_scripts.push(s.to_string()),
                Phase::InBody => body_fragment.push_str(s),
                _ => {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "script_outside_envelope",
                        "Thẻ <script> nằm ngoài <head>/<body>.",
                    ));
                }
            },
            TokenKind::OpenTag {
                raw,
                attrs_raw,
                self_closing,
                ..
            } => match name.as_str() {
                "html" if svg_depth == 0 => {
                    if phase != Phase::BeforeHtml || self_closing {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "invalid_html_envelope",
                            "Thẻ <html> trùng lặp hoặc lồng nhau.",
                        ));
                    }
                    let attrs = parse_attributes(attrs_raw, allow_trusted_theme_markup)?;
                    for (k, v) in attrs {
                        if k == "lang" {
                            let trimmed = v.trim();
                            if crate::storage::settings::valid_content_language(trimmed) {
                                html_lang = Some(trimmed.to_owned());
                            }
                        } else if k == "dir" {
                            if v.eq_ignore_ascii_case("ltr") {
                                html_dir = Some("ltr".to_owned());
                            } else if v.eq_ignore_ascii_case("rtl") {
                                html_dir = Some("rtl".to_owned());
                            }
                        }
                    }
                    phase = Phase::InHtmlBeforeHead;
                }
                "head" if svg_depth == 0 => {
                    if !matches!(phase, Phase::InHtmlBeforeHead) || seen_head || self_closing {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "invalid_head_envelope",
                            "Thẻ <head> trùng lặp, lồng nhau hoặc đặt sau <body>.",
                        ));
                    }
                    seen_head = true;
                    phase = Phase::InHead;
                }
                "body" if svg_depth == 0 => {
                    if !matches!(
                        phase,
                        Phase::InHtmlBeforeHead | Phase::InHtmlBetweenHeadAndBody
                    ) || seen_body
                        || self_closing
                    {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "invalid_body_envelope",
                            "Thẻ <body> trùng lặp hoặc đặt sai vị trí.",
                        ));
                    }
                    seen_body = true;
                    let attrs = parse_attributes(attrs_raw, allow_trusted_theme_markup)?;
                    for (k, v) in attrs {
                        if is_safe_body_attr(&k) {
                            body_attributes.push((k, v));
                        }
                    }
                    phase = Phase::InBody;
                }
                _ => match phase {
                    Phase::InHead => {
                        if name == "link" {
                            let attrs = parse_attributes(attrs_raw, allow_trusted_theme_markup)?;
                            if is_authorized_stylesheet_link(&attrs) {
                                authorized_styles.push(raw.to_string());
                            }
                        }
                        // Discard title, meta, link[rel=canonical], etc.
                    }
                    Phase::InBody => {
                        if name == "link" {
                            let attrs = parse_attributes(attrs_raw, allow_trusted_theme_markup)?;
                            if is_authorized_stylesheet_link(&attrs) {
                                authorized_styles.push(raw.to_string());
                            }
                        } else if name != "meta" && (name != "title" || svg_depth > 0) {
                            body_fragment.push_str(raw);
                        }
                        if name == "svg" && !self_closing {
                            svg_depth += 1;
                        }
                    }
                    _ => {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "tag_outside_envelope",
                            format!("Thẻ <{name}> nằm ngoài <head>/<body>."),
                        ));
                    }
                },
            },
            TokenKind::CloseTag { .. } => match name.as_str() {
                "head" if svg_depth == 0 => {
                    if phase != Phase::InHead {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "unexpected_close_head",
                            "Thẻ </head> không khớp.",
                        ));
                    }
                    phase = Phase::InHtmlBetweenHeadAndBody;
                }
                "body" if svg_depth == 0 => {
                    if phase != Phase::InBody {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "unexpected_close_body",
                            "Thẻ </body> không khớp.",
                        ));
                    }
                    phase = Phase::AfterBody;
                }
                "html" if svg_depth == 0 => {
                    if phase != Phase::AfterBody {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "unexpected_close_html",
                            "Thẻ </html> không khớp hoặc thiếu <body>.",
                        ));
                    }
                    phase = Phase::AfterHtml;
                }
                _ => match phase {
                    Phase::InHead => {}
                    Phase::InBody => {
                        if name != "title" || svg_depth > 0 {
                            body_fragment.push_str(&format!("</{name}>"));
                        }
                        if name == "svg" {
                            svg_depth = svg_depth.saturating_sub(1);
                        }
                    }
                    _ => {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "close_tag_outside_envelope",
                            format!("Thẻ </{name}> nằm ngoài envelope."),
                        ));
                    }
                },
            },
        }
    }

    if phase != Phase::AfterHtml || !seen_body {
        return Err(DocumentNormalizationDiagnostic::new(
            "incomplete_envelope",
            "Envelope HTML không đầy đủ.",
        ));
    }

    Ok(SafeDocumentParts {
        html_lang,
        html_dir,
        body_attributes,
        body_fragment,
        authorized_styles,
        authorized_head_scripts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_head_and_body_scripts_preserve_raw_text_and_split_authorization() {
        let html = r#"<!doctype html><html><head>
            <script type="module">const x = 1 < 2; const s = "<div>"; const t = "</scripture>";</ScRiPt >
            <title>SEO owned by composer</title>
            </head><body><main><script>if (a < b) { console.log("<span>"); }</SCRIPT></main></body></html>"#;
        let parts = normalize_public_document_with_options(html, true).unwrap();
        assert_eq!(parts.authorized_head_scripts.len(), 1);
        assert!(parts.authorized_head_scripts[0].contains("1 < 2"));
        assert!(parts.authorized_head_scripts[0].contains("</scripture>"));
        assert!(parts.authorized_head_scripts[0].ends_with("</ScRiPt >"));
        assert!(parts.body_fragment.contains("if (a < b)"));
        assert!(parts.body_fragment.contains("console.log(\"<span>\")"));
        assert!(!parts.body_fragment.contains("SEO owned by composer"));
        assert!(!parts.body_fragment.contains("const x"));
        assert!(parts.authorized_styles.is_empty());
    }

    #[test]
    fn trusted_fragment_preserves_svg_titles_static_handlers_and_scripts() {
        let html = r#"<svg xmlns="http://www.w3.org/2000/svg" onload="ready()"><title>Icon label</title>
            <svg><title>Nested label</title></svg>
            <foreignObject><div onclick="run()">content</div></foreignObject>
            <path d="M0 0 L1 1" /></svg>
            <script>const isLess = 1 < 2;</script><title>Discard this HTML title</title>"#;
        let parts = normalize_public_document_with_options(html, true).unwrap();
        assert_eq!(parts.authorized_head_scripts, Vec::<String>::new());
        assert!(parts.body_fragment.contains(r#"onload="ready()""#));
        assert!(parts.body_fragment.contains(r#"onclick="run()""#));
        assert!(parts.body_fragment.contains("<title>Icon label</title>"));
        assert!(parts.body_fragment.contains("<title>Nested label</title>"));
        assert!(parts.body_fragment.contains("const isLess = 1 < 2;"));
        assert!(
            !parts
                .body_fragment
                .contains("<title>Discard this HTML title</title>")
        );
    }

    #[test]
    fn trusted_foreignobject_html_body_does_not_create_or_close_document_envelope() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><foreignObject><body class="nested"><p>Nested HTML</p></body></foreignObject></svg>"#;
        let fragment = normalize_public_document_with_options(svg, true).unwrap();
        assert!(fragment.body_fragment.contains(r#"<body class="nested">"#));
        assert!(fragment.body_fragment.contains("<p>Nested HTML</p>"));
        assert!(
            fragment
                .body_fragment
                .contains("</body></foreignobject></svg>")
        );
        assert!(fragment.body_attributes.is_empty());

        let document = format!(
            "<!doctype html><html><head></head><body class=\"outer\"><main>{svg}</main></body></html>"
        );
        let normalized = normalize_public_document_with_options(&document, true).unwrap();
        assert_eq!(
            normalized.body_attributes,
            vec![("class".to_owned(), "outer".to_owned())]
        );
        assert!(
            normalized
                .body_fragment
                .contains(r#"<body class="nested">"#)
        );
        assert!(normalized.body_fragment.contains("<p>Nested HTML</p>"));
        assert!(
            normalized
                .body_fragment
                .contains("</body></foreignobject></svg>")
        );
        assert!(normalize_public_document(svg).is_err());
    }

    #[test]
    fn trusted_envelope_keeps_svg_title_but_filters_body_attrs() {
        let html = r#"<html lang="en" onload="bad()"><head></head><body class="ok" onload="bad()" data-live="1">
            <svg onpointerdown="onStatic()"><title>Accessible SVG</title></svg>
            <title>Document title</title></body></html>"#;
        let parts = normalize_public_document_with_options(html, true).unwrap();
        assert!(
            parts
                .body_fragment
                .contains("<title>Accessible SVG</title>")
        );
        assert!(
            !parts
                .body_fragment
                .contains("<title>Document title</title>")
        );
        assert!(
            parts
                .body_fragment
                .contains(r#"onpointerdown="onStatic()""#)
        );
        assert_eq!(
            parts.body_attributes,
            vec![
                ("class".to_string(), "ok".to_string()),
                ("data-live".to_string(), "1".to_string())
            ]
        );
    }

    #[test]
    fn trusted_scripts_require_matching_end_tag_and_valid_envelope_position() {
        for (html, code) in [
            ("<script>const x = 1 < 2", "unclosed_script"),
            ("<script/>", "self_closing_script"),
            ("<script src=\"/app.js\" />", "self_closing_script"),
            ("</script>", "unexpected_close_script"),
            ("<script>x</script></SCRIPT>", "unexpected_close_script"),
            (
                "<html><script>x</script><body></body></html>",
                "script_outside_envelope",
            ),
            (
                "<html><head></head><script>x</script><body></body></html>",
                "script_outside_envelope",
            ),
            (
                "<html><head></head><body></body><script>x</script></html>",
                "script_outside_envelope",
            ),
            (
                "<html><head></head><body></body></html><script>x</script>",
                "script_outside_envelope",
            ),
        ] {
            let result = normalize_public_document_with_options(html, true);
            assert_eq!(result.unwrap_err().code, code, "input: {html}");
        }
        assert!(
            normalize_public_document_with_options("<iframe src='/a'></iframe>", true).is_err()
        );
    }

    #[test]
    fn trusted_script_rejects_html5_double_escaped_data_smuggling() {
        for script in [
            "<script>\n<!--\n/*<script>*/\n// </script>\nalert(1)\n<!-- </script> -->",
            "<svg><script><![CDATA[// </script>\nalert(1)\n//<!-- ]]> </script> --> </svg>",
        ] {
            let error = normalize_public_document_with_options(script, true).unwrap_err();
            assert_eq!(error.code, "script_escaped_state");
            let document = format!(
                "<!doctype html><html><head></head><body><main>{script}</main></body></html>"
            );
            let error = normalize_public_document_with_options(&document, true).unwrap_err();
            assert_eq!(error.code, "script_escaped_state");
            assert!(normalize_public_document(script).is_err());
        }
    }

    #[test]
    fn strict_wrapper_rejects_scripts_svg_and_event_handlers() {
        for html in [
            "<script>const x = 1 < 2;</script>",
            "<svg><title>Icon</title></svg>",
            "<div onmouseover='run()'>text</div>",
            "<math><mi>x</mi></math>",
        ] {
            assert!(normalize_public_document(html).is_err(), "input: {html}");
            assert!(
                normalize_public_document_with_options(html, false).is_err(),
                "input: {html}"
            );
        }
        let parts = normalize_public_document("<p>safe</p>").unwrap();
        assert!(parts.authorized_head_scripts.is_empty());
        assert_eq!(parts.body_fragment, "<p>safe</p>");
    }

    #[test]
    fn normalizes_mixed_case_body_attrs_comments_style_and_strips_forged_head_tags() {
        let html = r#"<!DOCTYPE html>
<HTML lang="en">
  <HEAD>
    <TITLE>Forged Title</TITLE>
    <link rel="canonical" href="https://evil.example/steal">
    <meta name="robots" content="index,follow">
    <STYLE>body::before { content: "</head>"; color: red; }</STYLE>
    <link rel="stylesheet" href="/assets/theme.css">
  </HEAD>
  <!-- comment with </body> inside -->
  <BODY class="post-page" data-theme="v2" title="stripped">
    <main data-attr="</BODY>">Hello</main>
  </BODY>
</HTML>"#;
        let parts = normalize_public_document(html).expect("valid mixed-case envelope");
        assert_eq!(
            parts.body_attributes,
            vec![
                ("class".to_string(), "post-page".to_string()),
                ("data-theme".to_string(), "v2".to_string()),
            ]
        );
        assert_eq!(parts.authorized_styles.len(), 2);
        assert!(parts.authorized_styles[0].contains("</head>"));
        assert!(parts.body_fragment.contains("Hello"));
        assert!(!parts.body_fragment.contains("Forged Title"));
        assert!(!parts.body_fragment.contains("evil.example"));
    }

    #[test]
    fn rejects_duplicate_envelope_script_and_oversized() {
        let dup = "<html><body>a</body></html><html><body>b</body></html>";
        assert!(normalize_public_document(dup).is_err());

        let unclosed_style = "<html><head><style>body{}</head><body>a</body></html>";
        assert!(normalize_public_document(unclosed_style).is_err());

        let script = "<div><script>alert(1)</script></div>";
        assert!(normalize_public_document(script).is_err());

        for smuggled in [
            "<!-- --!><script>alert(1)</script><!-- -->",
            "<!--><script>alert(1)</script><!-- -->",
            "<!---><script>alert(1)</script><!-- -->",
            "<a href=\"https://ok.example\" href=\"javascript:alert(1)\">x</a>",
            "<xmp>raw</xmp>",
            "<plaintext>raw",
        ] {
            assert!(
                normalize_public_document(smuggled).is_err(),
                "expected reject on {smuggled}"
            );
        }

        let big = "a".repeat(MAX_PUBLIC_DOCUMENT_BYTES + 1);
        assert!(normalize_public_document(&big).is_err());
    }

    #[test]
    fn stylesheet_link_decodes_entities_and_preserves_plain_text_lt() {
        let html = concat!(
            r#"<!DOCTYPE html><html><head>"#,
            r#"<link rel="stylesheet" href="&#x2f;theme&#x2f;rev&#x2f;assets&#x2f;style.css">"#,
            r#"</head><body><p>1 < 2</p><p>1 < 2 it's ok</p><p>a < b</p>< !doctype></body></html>"#
        );
        let parts = normalize_public_document(html).expect("valid entity stylesheet and lt text");
        assert_eq!(parts.authorized_styles.len(), 1);
        assert!(parts.authorized_styles[0].contains("style.css"));
        assert!(parts.body_fragment.contains("1 < 2"));
        assert!(parts.body_fragment.contains("1 < 2 it's ok"));
        assert!(parts.body_fragment.contains("a < b"));
        assert!(parts.body_fragment.contains("< !doctype>"));

        let tokens = tokenize("< !doctype><p>1 < 2</p>", false).expect("text lt tokenizes cleanly");
        assert!(matches!(tokens[0].0, TokenKind::Text("<")));
        assert!(
            !tokens
                .iter()
                .any(|(t, _)| matches!(t, TokenKind::Doctype(_)))
        );

        for bad in [
            r#"<link rel="stylesheet" href="&#x2f;&#x2f;evil.example&#x2f;a.css">"#,
            r#"<link rel="stylesheet" href="&#x2f;theme&#x2f;rev&#x2f;a.css&#x07;">"#,
        ] {
            let parts =
                normalize_public_document(bad).expect("tokenizes but strips unauthorized link");
            assert!(
                parts.authorized_styles.is_empty(),
                "expected unauthorized link stripped: {bad}"
            );
        }

        assert!(!is_authorized_stylesheet_link(&[
            ("rel".to_owned(), "stylesheet".to_owned()),
            ("rel".to_owned(), "icon".to_owned()),
            ("href".to_owned(), "/a.css".to_owned()),
        ]));
        assert!(!is_authorized_stylesheet_link(&[
            ("rel".to_owned(), "stylesheet".to_owned()),
            ("href".to_owned(), "/a.css".to_owned()),
            ("href".to_owned(), "/b.css".to_owned()),
        ]));

        for rejected in [
            r#"<link rel="stylesheet" rel="icon" href="/a.css">"#,
            r#"<link rel="stylesheet" href="/a.css" href="/b.css">"#,
            r#"<?x <div title="><meta charset='utf-8'>">"#,
            r#"</1 <div title=">hi">"#,
            r#"<!doctypex>"#,
            r#"<!CDATA[foo]>"#,
            r#"<style>a{}</style >"#,
            r#"<style>a{}</style/>"#,
        ] {
            assert!(
                normalize_public_document(rejected).is_err(),
                "expected reject on {rejected}"
            );
        }
    }
}
