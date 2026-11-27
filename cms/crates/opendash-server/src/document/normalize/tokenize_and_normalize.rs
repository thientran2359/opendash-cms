use super::*;

pub(crate) fn tokenize(
    raw: &str,
    allow_trusted_theme_markup: bool,
) -> Result<Vec<(TokenKind<'_>, String)>, DocumentNormalizationDiagnostic> {
    tokenize_with_policy(
        raw,
        DocumentNormalizeOptions {
            allow_trusted_theme_markup,
            allow_admin_media_preview: false,
        },
    )
}

pub(crate) fn validate_media_element_attrs(
    name_lower: &str,
    attrs: &[(String, String)],
    allow_admin_media_preview: bool,
) -> Result<(), DocumentNormalizationDiagnostic> {
    if !matches!(name_lower, "img" | "source" | "video") {
        return Ok(());
    }
    for (k, raw_val) in attrs {
        if k == "srcset" && matches!(name_lower, "img" | "source") {
            let decoded = opendash_core::richtext::decode_html5_entities(raw_val);
            if !opendash_core::media::valid_media_srcset(&decoded, allow_admin_media_preview) {
                return Err(DocumentNormalizationDiagnostic::new(
                    "invalid_media_srcset",
                    "Thuộc tính srcset chứa URL hoặc mô tả không hợp lệ.",
                ));
            }
        } else if k == "src" || (name_lower == "video" && k == "poster") {
            let decoded = opendash_core::richtext::decode_html5_entities(raw_val);
            let trimmed = decoded.trim();
            if trimmed.is_empty()
                || trimmed.contains('\0')
                || trimmed.bytes().any(|b| b <= 0x1f || b == 0x7f)
                || trimmed.contains('\\')
                || trimmed.contains("..")
            {
                return Err(DocumentNormalizationDiagnostic::new(
                    "invalid_media_src",
                    "Thuộc tính src/poster của thẻ media không hợp lệ.",
                ));
            }
            if trimmed.starts_with('/') {
                if trimmed.starts_with("//") {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "invalid_media_src",
                        "Thuộc tính src/poster không được dùng scheme-relative URL.",
                    ));
                }
                let lower = trimmed.to_ascii_lowercase();
                if lower.starts_with("/api/admin/media/") {
                    if !allow_admin_media_preview {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "forbidden_admin_media_url",
                            "URL media nội bộ admin không được phép trong tài liệu công khai.",
                        ));
                    }
                    if !opendash_core::media::valid_media_render_url(trimmed, true) {
                        return Err(DocumentNormalizationDiagnostic::new(
                            "invalid_media_src",
                            "URL preview admin media không hợp lệ.",
                        ));
                    }
                } else if lower.starts_with("/media/")
                    && !opendash_core::media::valid_media_render_url(trimmed, false)
                {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "invalid_media_src",
                        "URL công khai của media không hợp lệ.",
                    ));
                }
            } else {
                let parsed = url::Url::parse(trimmed).map_err(|_| {
                    DocumentNormalizationDiagnostic::new(
                        "invalid_media_src",
                        "URL tuyệt đối trong src/poster không hợp lệ.",
                    )
                })?;
                if !matches!(parsed.scheme(), "http" | "https")
                    || !parsed.has_host()
                    || !parsed.username().is_empty()
                    || parsed.password().is_some()
                {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "invalid_media_src",
                        "Chỉ cho phép URL http(s) an toàn trong src/poster.",
                    ));
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn tokenize_with_policy(
    raw: &str,
    opts: DocumentNormalizeOptions,
) -> Result<Vec<(TokenKind<'_>, String)>, DocumentNormalizationDiagnostic> {
    let allow_trusted_theme_markup = opts.allow_trusted_theme_markup;
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
                if name_lower == "source" {
                    return Err(DocumentNormalizationDiagnostic::new(
                        "unexpected_void_close_tag",
                        "Thẻ tự đóng `<source>` không được có thẻ đóng `</source>`.",
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
                validate_media_element_attrs(&name_lower, &attrs, opts.allow_admin_media_preview)?;
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

pub(crate) fn is_json_ld_script_block(raw_script: &str) -> bool {
    let Ok(gt) = find_tag_end(raw_script, "<script".len()) else {
        return true;
    };
    let open = &raw_script[..gt];
    let attrs_raw = open
        .get("<script".len()..)
        .unwrap_or("")
        .trim_end_matches('/');
    let Ok(attrs) = parse_attributes(attrs_raw, true) else {
        return true;
    };
    attrs.iter().any(|(k, v)| {
        if k != "type" {
            return false;
        }
        let decoded = opendash_core::richtext::decode_html5_entities(v);
        decoded
            .split(';')
            .next()
            .map(str::trim)
            .is_some_and(|mime| mime.eq_ignore_ascii_case("application/ld+json"))
    })
}
