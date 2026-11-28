use super::*;

pub(super) use crate::field_schema::valid_safe_url;

pub const MAX_RICHTEXT_INPUT_BYTES: usize = 65_536;

pub const MAX_RICHTEXT_OUTPUT_BYTES: usize = 32_768;

pub const MAX_RICHTEXT_DEPTH: usize = 16;

/// Decodes HTML5 character references (numeric hex/decimal with optional leading zeros
/// and optional semicolon, plus HTML5 named entities needed for URL/HTML smuggling
/// defense and common typography). Unrecognized `&...` sequences are preserved literally.
pub fn decode_html5_entities(input: &str) -> String {
    if !input.contains('&') {
        return input.to_owned();
    }
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'&' {
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != b'&' {
                i += 1;
            }
            out.push_str(&input[start..i]);
            continue;
        }

        if let Some((decoded, next_idx)) = decode_entity_at(input, i) {
            out.push_str(decoded);
            i = next_idx;
        } else if let Some((ch, next_idx)) = decode_numeric_entity_at(bytes, i) {
            out.push(ch);
            i = next_idx;
        } else {
            out.push('&');
            i += 1;
        }
    }

    out
}

pub(super) fn decode_numeric_entity_at(bytes: &[u8], start: usize) -> Option<(char, usize)> {
    if start + 2 >= bytes.len() || bytes[start] != b'&' || bytes[start + 1] != b'#' {
        return None;
    }
    let mut j = start + 2;
    let is_hex = matches!(bytes.get(j), Some(b'x' | b'X'));
    if is_hex {
        j += 1;
    }
    let digit_start = j;
    let mut val: u32 = 0;

    if is_hex {
        while j < bytes.len() && bytes[j].is_ascii_hexdigit() {
            let d = match bytes[j] {
                b'0'..=b'9' => (bytes[j] - b'0') as u32,
                b'a'..=b'f' => (bytes[j] - b'a' + 10) as u32,
                b'A'..=b'F' => (bytes[j] - b'A' + 10) as u32,
                _ => unreachable!(),
            };
            val = val.saturating_mul(16).saturating_add(d);
            j += 1;
        }
    } else {
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            let d = (bytes[j] - b'0') as u32;
            val = val.saturating_mul(10).saturating_add(d);
            j += 1;
        }
    }

    if j == digit_start {
        return None;
    }
    if bytes.get(j) == Some(&b';') {
        j += 1;
    }

    // HTML5 numeric character reference end state: 0x00, > 0x10FFFF, and surrogates map to U+FFFD.
    // ASCII control bytes 0x01..=0x1F and 0x7F remain intact so downstream URL checks reject them.
    let ch = if val == 0 || val > 0x10_FFFF || (0xD800..=0xDFFF).contains(&val) {
        '\u{FFFD}'
    } else {
        char::from_u32(val).unwrap_or('\u{FFFD}')
    };
    Some((ch, j))
}

pub(super) fn decode_entity_at(input: &str, start: usize) -> Option<(&'static str, usize)> {
    let bytes = input.as_bytes();
    let after_amp = start + 1;
    if after_amp >= bytes.len() || bytes[after_amp] == b'#' {
        return None;
    }

    let mut end = after_amp;
    let max_end = bytes.len().min(after_amp + 20);
    while end < max_end && bytes[end].is_ascii_alphanumeric() {
        end += 1;
    }
    if end == after_amp {
        return None;
    }

    let has_semi = bytes.get(end) == Some(&b';');
    let name = &input[after_amp..end];
    if has_semi {
        if let Some(val) = lookup_named_entity(name) {
            return Some((val, end + 1));
        }
    }

    // Legacy semicolon-optional HTML5 named entities (when not immediately followed by `=` or ASCII alphanumeric).
    for (legacy_name, val) in [
        ("amp", "&"),
        ("AMP", "&"),
        ("lt", "<"),
        ("LT", "<"),
        ("gt", ">"),
        ("GT", ">"),
        ("quot", "\""),
        ("QUOT", "\""),
        ("nbsp", "\u{00A0}"),
        ("copy", "\u{00A9}"),
        ("COPY", "\u{00A9}"),
        ("reg", "\u{00AE}"),
        ("REG", "\u{00AE}"),
    ] {
        if let Some(rest) = input[after_amp..].strip_prefix(legacy_name) {
            let next = rest.as_bytes().first().copied();
            if !next.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'=') {
                return Some((val, after_amp + legacy_name.len()));
            }
        }
    }

    None
}

/// Sanitizes untrusted RichText HTML to a strict allowlist (`p`, `br`, `strong`, `b`,
/// `em`, `i`, `u`, `ul`, `ol`, `li`, `blockquote`, `a`) with balanced tags, depth cap,
/// RAWTEXT/RCDATA/foreign element stripping, and HTML5-decoded URL checking on `<a href>`.
pub fn sanitize_richtext(raw: &str) -> Result<String, &'static str> {
    sanitize_richtext_with_limits(raw, MAX_RICHTEXT_INPUT_BYTES, MAX_RICHTEXT_OUTPUT_BYTES)
}

pub fn sanitize_richtext_with_limits(
    raw: &str,
    max_input_bytes: usize,
    max_output_bytes: usize,
) -> Result<String, &'static str> {
    if raw.len() > max_input_bytes {
        return Err(if max_input_bytes <= MAX_RICHTEXT_INPUT_BYTES {
            "Nội dung RichText vượt quá giới hạn 64 KiB đầu vào."
        } else {
            "Nội dung RichText vượt quá giới hạn dung lượng đầu vào cho phép."
        });
    }
    if raw.contains('\0') {
        return Err("Nội dung RichText không được chứa ký tự NUL.");
    }

    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len().min(max_output_bytes));
    let mut open_stack: Vec<&'static str> = Vec::new();
    let mut cursor = 0usize;

    while cursor < bytes.len() {
        let Some(rel_lt) = raw[cursor..].find('<') else {
            let decoded = decode_html5_entities(&raw[cursor..]);
            push_escaped_text(&mut out, &decoded);
            break;
        };

        let lt = cursor + rel_lt;
        if lt > cursor {
            let decoded = decode_html5_entities(&raw[cursor..lt]);
            push_escaped_text(&mut out, &decoded);
        }

        let rest = &bytes[lt..];
        if rest.starts_with(b"<!--") {
            cursor = skip_html_comment(bytes, lt);
            continue;
        }
        if starts_with_ignore_ascii_case(rest, b"<![cdata[") {
            cursor = skip_cdata(bytes, lt);
            continue;
        }
        if rest.starts_with(b"<!") || rest.starts_with(b"<?") {
            cursor = skip_to_gt(bytes, lt + 2);
            continue;
        }

        if rest.starts_with(b"</") {
            if let Some(after_slash) = bytes.get(lt + 2)
                && after_slash.is_ascii_alphabetic()
            {
                let (tag_name, next_cursor) = parse_end_tag(raw, lt + 2);
                cursor = next_cursor;
                if let Some(allowed) = allowlist_closing_tag(&tag_name)
                    && let Some(pos) = open_stack.iter().rposition(|t| *t == allowed)
                {
                    while open_stack.len() > pos {
                        if let Some(top) = open_stack.pop() {
                            out.push_str("</");
                            out.push_str(top);
                            out.push('>');
                        }
                    }
                }
                continue;
            }
            out.push_str("&lt;");
            cursor = lt + 1;
            continue;
        }

        if let Some(first_ch) = bytes.get(lt + 1)
            && first_ch.is_ascii_alphabetic()
        {
            let Some((tag_name, attrs, self_closing, next_cursor)) = parse_start_tag(raw, lt + 1)
            else {
                // Unclosed `<tag...` at EOF: drop bogus incomplete tag to match HTML5 safety.
                break;
            };
            cursor = next_cursor;

            if tag_name == "plaintext" {
                break;
            }
            if tag_name == "embed" {
                continue;
            }
            if matches!(tag_name.as_str(), "svg" | "math") {
                if !self_closing {
                    cursor = skip_foreign_container(raw, cursor, &tag_name);
                }
                continue;
            }
            if is_stripped_container_tag(&tag_name) {
                cursor = skip_until_closing_tag(raw, cursor, &tag_name);
                continue;
            }

            if tag_name == "br" {
                out.push_str("<br>");
                continue;
            }

            if tag_name == "a" {
                if open_stack.len() >= MAX_RICHTEXT_DEPTH || open_stack.iter().any(|t| *t == "a") {
                    continue;
                }
                let href_attrs: Vec<&str> = attrs
                    .iter()
                    .filter(|(k, _)| k == "href")
                    .map(|(_, v)| v.as_str())
                    .collect();
                if href_attrs.len() != 1 {
                    continue;
                }
                let decoded_href = decode_html5_entities(href_attrs[0]);
                let trimmed_href = decoded_href.trim();
                // Reject raw control/null chars even before trim changed ends, and require no whitespace anywhere.
                if decoded_href.contains('\0')
                    || decoded_href.bytes().any(|b| b <= 0x1F || b == 0x7F)
                    || decoded_href != trimmed_href
                    || !valid_safe_url(&decoded_href)
                {
                    continue;
                }
                out.push_str("<a href=\"");
                push_escaped_text(&mut out, &decoded_href);
                out.push_str("\" rel=\"noopener noreferrer\">");
                open_stack.push("a");
                continue;
            }

            if let Some(allowed) = allowlist_open_tag(&tag_name)
                && open_stack.len() < MAX_RICHTEXT_DEPTH
            {
                out.push('<');
                out.push_str(allowed);
                out.push('>');
                open_stack.push(allowed);
            }
            continue;
        }

        out.push_str("&lt;");
        cursor = lt + 1;
    }

    while let Some(top) = open_stack.pop() {
        out.push_str("</");
        out.push_str(top);
        out.push('>');
    }

    if out.len() > max_output_bytes {
        return Err(if max_output_bytes <= MAX_RICHTEXT_OUTPUT_BYTES {
            "Nội dung RichText vượt quá giới hạn 32 KiB sau làm sạch."
        } else {
            "Nội dung RichText vượt quá giới hạn dung lượng đầu ra sau làm sạch."
        });
    }

    Ok(out)
}

/// Strips RichText HTML into normalized whitespace-collapsed plain text for search/extraction.
pub fn strip_richtext_to_plain(raw: &str) -> String {
    if raw.contains('\0') {
        return String::new();
    }
    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len().min(MAX_RICHTEXT_OUTPUT_BYTES));
    let mut cursor = 0usize;

    while cursor < bytes.len() {
        let Some(rel_lt) = raw[cursor..].find('<') else {
            let decoded = decode_html5_entities(&raw[cursor..]);
            out.push_str(&decoded);
            break;
        };
        let lt = cursor + rel_lt;
        if lt > cursor {
            let decoded = decode_html5_entities(&raw[cursor..lt]);
            out.push_str(&decoded);
        }

        let rest = &bytes[lt..];
        if rest.starts_with(b"<!--") {
            cursor = skip_html_comment(bytes, lt);
            continue;
        }
        if starts_with_ignore_ascii_case(rest, b"<![cdata[") {
            cursor = skip_cdata(bytes, lt);
            continue;
        }
        if rest.starts_with(b"<!") || rest.starts_with(b"<?") {
            cursor = skip_to_gt(bytes, lt + 2);
            continue;
        }
        if rest.starts_with(b"</") {
            if bytes.get(lt + 2).is_some_and(|b| b.is_ascii_alphabetic()) {
                let (tag_name, next_cursor) = parse_end_tag(raw, lt + 2);
                cursor = next_cursor;
                if is_plain_break_tag(&tag_name) {
                    out.push(' ');
                }
                continue;
            }
            out.push('<');
            cursor = lt + 1;
            continue;
        }
        if bytes.get(lt + 1).is_some_and(|b| b.is_ascii_alphabetic()) {
            let Some((tag_name, _, self_closing, next_cursor)) = parse_start_tag(raw, lt + 1)
            else {
                break;
            };
            cursor = next_cursor;
            if tag_name == "plaintext" {
                break;
            }
            if tag_name == "embed" {
                continue;
            }
            if matches!(tag_name.as_str(), "svg" | "math") {
                if !self_closing {
                    cursor = skip_foreign_container(raw, cursor, &tag_name);
                }
                continue;
            }
            if is_stripped_container_tag(&tag_name) {
                cursor = skip_until_closing_tag(raw, cursor, &tag_name);
                continue;
            }
            if is_plain_break_tag(&tag_name) {
                out.push(' ');
            }
            continue;
        }
        out.push('<');
        cursor = lt + 1;
    }

    collapse_whitespace(&out)
}
