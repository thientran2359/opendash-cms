//! M7-S03-T02 bounded single-pass trusted HTML document normalizer.
//! Strips untrusted head/meta/canonical/title tags and validates envelope structure
//! without regexes or literal `<body>` substring splitting.

use super::*;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TokenKind<'a> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DocumentNormalizeOptions {
    pub allow_trusted_theme_markup: bool,
    pub allow_admin_media_preview: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TagScanState {
    BeforeAttrName,
    AttrName,
    AfterAttrName,
    BeforeAttrValue,
    QuotedValue(u8),
    UnquotedValue,
}

pub fn normalize_public_document_with_options(
    raw_html: &str,
    allow_trusted_theme_markup: bool,
) -> Result<SafeDocumentParts, DocumentNormalizationDiagnostic> {
    normalize_document_with_policy(
        raw_html,
        DocumentNormalizeOptions {
            allow_trusted_theme_markup,
            allow_admin_media_preview: false,
        },
    )
}

pub fn normalize_document_with_policy(
    raw_html: &str,
    opts: DocumentNormalizeOptions,
) -> Result<SafeDocumentParts, DocumentNormalizationDiagnostic> {
    let allow_trusted_theme_markup = opts.allow_trusted_theme_markup;
    let tokens = tokenize_with_policy(raw_html, opts)?;

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
