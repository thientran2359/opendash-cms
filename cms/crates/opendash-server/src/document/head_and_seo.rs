use super::*;

/// Normalizes a `filter:render/head` fragment while keeping Core SEO invariants
/// (`<title>`, `<meta name="robots">`, `<meta http-equiv>`, `<link rel="canonical|alternate">`,
/// and `<script type="application/ld+json">`) strictly inviolable.
pub fn sanitize_render_head_fragment(
    raw_html: &str,
    allow_trusted_html: bool,
) -> Result<SafeDocumentParts, DocumentNormalizationDiagnostic> {
    let tokens = tokenize(raw_html, allow_trusted_html)?;
    let mut authorized_styles = Vec::new();
    let mut authorized_head_scripts = Vec::new();
    let mut head_fragment = String::with_capacity(raw_html.len());
    let mut title_depth = 0usize;

    for (tok, name) in tokens {
        match tok {
            TokenKind::Doctype(_) => {
                return Err(DocumentNormalizationDiagnostic::new(
                    "forbidden_head_envelope",
                    "filter:render/head không được chứa DOCTYPE.",
                ));
            }
            TokenKind::OpenTag {
                raw,
                attrs_raw,
                self_closing,
                ..
            } => {
                if matches!(name.as_str(), "html" | "head" | "body") {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "forbidden_head_envelope",
                        "filter:render/head không được chứa thẻ html/head/body.",
                    ));
                }
                // Strip <title> unconditionally (including inside <svg>)
                if name == "title" {
                    if !self_closing {
                        title_depth += 1;
                    }
                    continue;
                }
                if title_depth > 0 {
                    continue;
                }
                let attrs = parse_attributes(attrs_raw, allow_trusted_html)?;
                if name == "meta" {
                    let is_robots = attrs.iter().any(|(k, v)| {
                        k == "name"
                            && opendash_core::richtext::decode_html5_entities(v)
                                .trim()
                                .eq_ignore_ascii_case("robots")
                    });
                    let is_http_equiv = attrs.iter().any(|(k, _)| k == "http-equiv");
                    if is_robots || is_http_equiv || !allow_trusted_html {
                        continue;
                    }
                    head_fragment.push_str(raw);
                    continue;
                }
                if name == "link" {
                    let is_forbidden_rel = attrs.iter().any(|(k, v)| {
                        if k != "rel" {
                            return false;
                        }
                        let decoded =
                            opendash_core::richtext::decode_html5_entities(v).to_ascii_lowercase();
                        decoded
                            .split_whitespace()
                            .any(|t| matches!(t, "canonical" | "alternate"))
                    });
                    if is_forbidden_rel {
                        continue;
                    }
                    if !allow_trusted_html {
                        continue;
                    }
                    if is_authorized_stylesheet_link(&attrs) {
                        authorized_styles.push(raw.to_string());
                    } else {
                        head_fragment.push_str(raw);
                    }
                    continue;
                }
                head_fragment.push_str(raw);
            }
            TokenKind::CloseTag { .. } => {
                if matches!(name.as_str(), "html" | "head" | "body") {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "forbidden_head_envelope",
                        "filter:render/head không được chứa thẻ html/head/body.",
                    ));
                }
                if name == "title" {
                    title_depth = title_depth.saturating_sub(1);
                    continue;
                }
                if title_depth > 0 {
                    continue;
                }
                if !allow_trusted_html
                    && matches!(name.as_str(), "meta" | "link" | "style" | "script")
                {
                    continue;
                }
                head_fragment.push_str(&format!("</{name}>"));
            }
            TokenKind::StyleBlock(s) => {
                if title_depth == 0 && allow_trusted_html {
                    authorized_styles.push(s.to_string());
                }
            }
            TokenKind::ScriptBlock(s) => {
                if title_depth == 0 && allow_trusted_html && !is_json_ld_script_block(s) {
                    authorized_head_scripts.push(s.to_string());
                }
            }
            TokenKind::Text(t) | TokenKind::Comment(t) => {
                if title_depth == 0 {
                    head_fragment.push_str(t);
                }
            }
        }
    }

    Ok(SafeDocumentParts {
        html_lang: None,
        html_dir: None,
        body_attributes: Vec::new(),
        body_fragment: head_fragment,
        authorized_styles,
        authorized_head_scripts,
    })
}
