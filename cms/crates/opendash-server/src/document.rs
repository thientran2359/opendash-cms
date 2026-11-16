//! M7-S03-T02 bounded single-pass trusted HTML document normalizer.
//! Strips untrusted head/meta/canonical/title tags and validates envelope structure
//! without regexes or literal `<body>` substring splitting.

pub const MAX_PUBLIC_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeDocumentParts {
    pub body_attributes: Vec<(String, String)>,
    pub body_fragment: String,
    pub authorized_styles: Vec<String>,
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
}

fn is_forbidden_tag(name_lower: &str) -> bool {
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
    )
}

fn parse_attributes(
    attrs_raw: &str,
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
        if name.starts_with("on") {
            return Err(DocumentNormalizationDiagnostic::new(
                "forbidden_event_handler",
                format!("Thuộc tính sự kiện `{name}` không được phép."),
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
    let mut is_stylesheet = false;
    let mut href_ok = false;
    for (k, v) in attrs {
        if k == "rel" && v.eq_ignore_ascii_case("stylesheet") {
            is_stylesheet = true;
        } else if k == "href" {
            let trimmed = v.trim();
            href_ok = trimmed.starts_with('/') && !trimmed.starts_with("//");
        }
    }
    is_stylesheet && href_ok
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

fn find_case_insensitive_end_style(raw: &str, start: usize) -> Option<(usize, usize)> {
    let bytes = raw.as_bytes();
    let needle = b"</style";
    let mut i = start;
    while i + needle.len() < bytes.len() {
        if bytes[i] == b'<' && bytes[i..i + needle.len()].eq_ignore_ascii_case(needle) {
            let mut j = i + needle.len();
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'>' {
                return Some((i, j + 1));
            }
        }
        i += 1;
    }
    None
}

fn tokenize(raw: &str) -> Result<Vec<(TokenKind<'_>, String)>, DocumentNormalizationDiagnostic> {
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
        if raw[i..].starts_with("<!--") {
            let Some(rel_end) = raw[i + 4..].find("-->") else {
                return Err(DocumentNormalizationDiagnostic::new(
                    "unclosed_comment",
                    "Chú thích HTML chưa được đóng.",
                ));
            };
            let end = i + 4 + rel_end + 3;
            out.push((TokenKind::Comment(&raw[i..end]), String::new()));
            i = end;
            continue;
        }
        let end = find_tag_end(raw, i + 1)?;
        let tag_full = &raw[i..=end];
        let inner = raw[i + 1..end].trim();
        if inner.len() >= 8 && inner[..8].eq_ignore_ascii_case("!doctype") {
            out.push((TokenKind::Doctype(tag_full), String::new()));
            i = end + 1;
            continue;
        }
        if let Some(rest) = inner.strip_prefix('/') {
            let name_end = rest
                .find(|c: char| c.is_ascii_whitespace() || c == '>')
                .unwrap_or(rest.len());
            let name_lower = rest[..name_end].to_ascii_lowercase();
            if is_forbidden_tag(&name_lower) {
                return Err(DocumentNormalizationDiagnostic::new(
                    "forbidden_tag",
                    format!("Thẻ `{name_lower}` không được phép."),
                ));
            }
            out.push((
                TokenKind::CloseTag { name_lower: "" },
                name_lower,
            ));
            i = end + 1;
            continue;
        }
        let self_closing = inner.ends_with('/');
        let inner_clean = if self_closing {
            inner[..inner.len() - 1].trim_end()
        } else {
            inner
        };
        let name_end = inner_clean
            .find(|c: char| c.is_ascii_whitespace())
            .unwrap_or(inner_clean.len());
        let name_lower = inner_clean[..name_end].to_ascii_lowercase();
        if name_lower.is_empty() {
            return Err(DocumentNormalizationDiagnostic::new(
                "malformed_tag",
                "Thẻ HTML rỗng không hợp lệ.",
            ));
        }
        if is_forbidden_tag(&name_lower) {
            return Err(DocumentNormalizationDiagnostic::new(
                "forbidden_tag",
                format!("Thẻ `{name_lower}` không được phép."),
            ));
        }
        let attrs_raw = &inner_clean[name_end..];
        let attrs = parse_attributes(attrs_raw)?;
        if name_lower == "style" && !self_closing {
            let Some((style_end, after_close)) = find_case_insensitive_end_style(raw, end + 1)
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
    Ok(out)
}

pub fn normalize_public_document(
    raw_html: &str,
) -> Result<SafeDocumentParts, DocumentNormalizationDiagnostic> {
    let tokens = tokenize(raw_html)?;

    let has_envelope = tokens.iter().any(|(tok, name)| {
        matches!(tok, TokenKind::Doctype(_))
            || matches!(
                (tok, name.as_str()),
                (TokenKind::OpenTag { .. }, "html" | "head" | "body")
                    | (TokenKind::CloseTag { .. }, "html" | "head" | "body")
            )
    });

    if !has_envelope {
        let mut authorized_styles = Vec::new();
        let mut body_fragment = String::with_capacity(raw_html.len());
        for (tok, name) in tokens {
            match tok {
                TokenKind::Text(t) | TokenKind::Comment(t) => body_fragment.push_str(t),
                TokenKind::StyleBlock(s) => authorized_styles.push(s.to_string()),
                TokenKind::OpenTag {
                    raw, attrs_raw, ..
                } => {
                    if name == "link" {
                        let attrs = parse_attributes(attrs_raw)?;
                        if is_authorized_stylesheet_link(&attrs) {
                            authorized_styles.push(raw.to_string());
                        }
                    } else if name != "meta" && name != "title" {
                        body_fragment.push_str(raw);
                    }
                }
                TokenKind::CloseTag { .. } => {
                    if name != "title" {
                        body_fragment.push_str(&format!("</{name}>"));
                    }
                }
                TokenKind::Doctype(_) => {}
            }
        }
        return Ok(SafeDocumentParts {
            body_attributes: Vec::new(),
            body_fragment,
            authorized_styles,
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
    let mut body_attributes = Vec::new();
    let mut authorized_styles = Vec::new();
    let mut body_fragment = String::new();

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
                Phase::BeforeHtml | Phase::InHtmlBeforeHead | Phase::InHtmlBetweenHeadAndBody | Phase::AfterBody | Phase::AfterHtml => {
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
            TokenKind::OpenTag {
                raw,
                attrs_raw,
                self_closing,
                ..
            } => match name.as_str() {
                "html" => {
                    if phase != Phase::BeforeHtml || self_closing {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "invalid_html_envelope",
                            "Thẻ <html> trùng lặp hoặc lồng nhau.",
                        ));
                    }
                    phase = Phase::InHtmlBeforeHead;
                }
                "head" => {
                    if !matches!(phase, Phase::InHtmlBeforeHead) || seen_head || self_closing {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "invalid_head_envelope",
                            "Thẻ <head> trùng lặp, lồng nhau hoặc đặt sau <body>.",
                        ));
                    }
                    seen_head = true;
                    phase = Phase::InHead;
                }
                "body" => {
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
                    let attrs = parse_attributes(attrs_raw)?;
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
                            let attrs = parse_attributes(attrs_raw)?;
                            if is_authorized_stylesheet_link(&attrs) {
                                authorized_styles.push(raw.to_string());
                            }
                        }
                        // Discard title, meta, link[rel=canonical], etc.
                    }
                    Phase::InBody => {
                        if name == "link" {
                            let attrs = parse_attributes(attrs_raw)?;
                            if is_authorized_stylesheet_link(&attrs) {
                                authorized_styles.push(raw.to_string());
                            }
                        } else if name != "meta" && name != "title" {
                            body_fragment.push_str(raw);
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
                "head" => {
                    if phase != Phase::InHead {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "unexpected_close_head",
                            "Thẻ </head> không khớp.",
                        ));
                    }
                    phase = Phase::InHtmlBetweenHeadAndBody;
                }
                "body" => {
                    if phase != Phase::InBody {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "unexpected_close_body",
                            "Thẻ </body> không khớp.",
                        ));
                    }
                    phase = Phase::AfterBody;
                }
                "html" => {
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
                        if name != "title" {
                            body_fragment.push_str(&format!("</{name}>"));
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
        body_attributes,
        body_fragment,
        authorized_styles,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let big = "a".repeat(MAX_PUBLIC_DOCUMENT_BYTES + 1);
        assert!(normalize_public_document(&big).is_err());
    }
}
