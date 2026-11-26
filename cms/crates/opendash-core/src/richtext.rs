use crate::field_schema::valid_safe_url;

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

fn decode_numeric_entity_at(bytes: &[u8], start: usize) -> Option<(char, usize)> {
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

fn decode_entity_at(input: &str, start: usize) -> Option<(&'static str, usize)> {
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

fn lookup_named_entity(name: &str) -> Option<&'static str> {
    match name {
        "Tab" => Some("\t"),
        "NewLine" => Some("\n"),
        "excl" => Some("!"),
        "quot" | "QUOT" => Some("\""),
        "num" => Some("#"),
        "dollar" => Some("$"),
        "percnt" => Some("%"),
        "amp" | "AMP" => Some("&"),
        "apos" => Some("'"),
        "lpar" => Some("("),
        "rpar" => Some(")"),
        "ast" => Some("*"),
        "plus" => Some("+"),
        "comma" => Some(","),
        "hyphen" | "dash" => Some("-"),
        "period" => Some("."),
        "sol" => Some("/"),
        "colon" => Some(":"),
        "semi" => Some(";"),
        "lt" | "LT" => Some("<"),
        "equals" => Some("="),
        "gt" | "GT" => Some(">"),
        "quest" => Some("?"),
        "commat" => Some("@"),
        "lsqb" => Some("["),
        "bsol" => Some("\\"),
        "rsqb" => Some("]"),
        "Hat" => Some("^"),
        "lowbar" => Some("_"),
        "grave" => Some("`"),
        "lcub" => Some("{"),
        "verbar" | "VerticalLine" => Some("|"),
        "rcub" => Some("}"),
        "tilde" => Some("~"),
        "nbsp" | "NonBreakingSpace" => Some("\u{00A0}"),
        "sect" => Some("\u{00A7}"),
        "copy" | "COPY" => Some("\u{00A9}"),
        "laquo" => Some("\u{00AB}"),
        "reg" | "REG" => Some("\u{00AE}"),
        "deg" => Some("\u{00B0}"),
        "plusmn" => Some("\u{00B1}"),
        "middot" => Some("\u{00B7}"),
        "raquo" => Some("\u{00BB}"),
        "frac14" => Some("\u{00BC}"),
        "frac12" => Some("\u{00BD}"),
        "frac34" => Some("\u{00BE}"),
        "times" => Some("\u{00D7}"),
        "divide" => Some("\u{00F7}"),
        "ndash" => Some("\u{2013}"),
        "mdash" => Some("\u{2014}"),
        "lsquo" => Some("\u{2018}"),
        "rsquo" => Some("\u{2019}"),
        "ldquo" => Some("\u{201C}"),
        "rdquo" => Some("\u{201D}"),
        "bull" => Some("\u{2022}"),
        "hellip" => Some("\u{2026}"),
        "trade" => Some("\u{2122}"),
        "euro" => Some("\u{20AC}"),
        "pound" => Some("\u{00A3}"),
        "yen" => Some("\u{00A5}"),
        _ => None,
    }
}

/// Sanitizes untrusted RichText HTML to a strict allowlist (`p`, `br`, `strong`, `b`,
/// `em`, `i`, `u`, `ul`, `ol`, `li`, `blockquote`, `a`) with balanced tags, depth cap,
/// RAWTEXT/RCDATA/foreign element stripping, and HTML5-decoded URL checking on `<a href>`.
pub fn sanitize_richtext(raw: &str) -> Result<String, &'static str> {
    if raw.len() > MAX_RICHTEXT_INPUT_BYTES {
        return Err("Nội dung RichText vượt quá giới hạn 64 KiB đầu vào.");
    }
    if raw.contains('\0') {
        return Err("Nội dung RichText không được chứa ký tự NUL.");
    }

    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len().min(MAX_RICHTEXT_OUTPUT_BYTES));
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

    if out.len() > MAX_RICHTEXT_OUTPUT_BYTES {
        return Err("Nội dung RichText vượt quá giới hạn 32 KiB sau làm sạch.");
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

fn collapse_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_space = true;
    for ch in input.chars() {
        if ch.is_whitespace() || ch == '\u{00A0}' {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

fn is_plain_break_tag(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "br"
            | "li"
            | "ul"
            | "ol"
            | "blockquote"
            | "div"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "hr"
    )
}

fn push_escaped_text(out: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '\0' => out.push('\u{FFFD}'),
            _ => out.push(ch),
        }
    }
}

fn allowlist_open_tag(tag: &str) -> Option<&'static str> {
    match tag {
        "p" => Some("p"),
        "strong" => Some("strong"),
        "b" => Some("b"),
        "em" => Some("em"),
        "i" => Some("i"),
        "u" => Some("u"),
        "ul" => Some("ul"),
        "ol" => Some("ol"),
        "li" => Some("li"),
        "blockquote" => Some("blockquote"),
        _ => None,
    }
}

fn allowlist_closing_tag(tag: &str) -> Option<&'static str> {
    if tag == "a" {
        return Some("a");
    }
    allowlist_open_tag(tag)
}

fn is_stripped_container_tag(tag: &str) -> bool {
    matches!(
        tag,
        "script"
            | "style"
            | "noscript"
            | "title"
            | "textarea"
            | "xmp"
            | "noembed"
            | "noframes"
            | "iframe"
            | "object"
    )
}

fn skip_html_comment(bytes: &[u8], lt: usize) -> usize {
    let mut i = lt + 4;
    if bytes.get(i) == Some(&b'>') {
        return i + 1;
    }
    if bytes[i..].starts_with(b"->") {
        return i + 2;
    }
    while i < bytes.len() {
        if bytes[i..].starts_with(b"--!>") {
            return i + 4;
        }
        if bytes[i..].starts_with(b"-->") {
            return i + 3;
        }
        i += 1;
    }
    bytes.len()
}

fn skip_cdata(bytes: &[u8], lt: usize) -> usize {
    let mut i = lt + 9;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"]]>") {
            return i + 3;
        }
        i += 1;
    }
    bytes.len()
}

fn skip_to_gt(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        if bytes[i] == b'>' {
            return i + 1;
        }
        i += 1;
    }
    bytes.len()
}

fn starts_with_ignore_ascii_case(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.len() >= needle.len() && haystack[..needle.len()].eq_ignore_ascii_case(needle)
}

fn parse_end_tag(raw: &str, name_start: usize) -> (String, usize) {
    let bytes = raw.as_bytes();
    let mut i = name_start;
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' && bytes[i] != b'/'
    {
        i += 1;
    }
    let name = raw[name_start..i].to_ascii_lowercase();
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = quote {
            if b == q {
                quote = None;
            }
            i += 1;
        } else if b == b'"' || b == b'\'' {
            quote = Some(b);
            i += 1;
        } else if b == b'>' {
            return (name, i + 1);
        } else {
            i += 1;
        }
    }
    (name, bytes.len())
}

fn parse_start_tag(
    raw: &str,
    name_start: usize,
) -> Option<(String, Vec<(String, String)>, bool, usize)> {
    let bytes = raw.as_bytes();
    let mut i = name_start;
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' && bytes[i] != b'/'
    {
        i += 1;
    }
    let tag_name = raw[name_start..i].to_ascii_lowercase();
    let mut attrs: Vec<(String, String)> = Vec::new();
    let mut self_closing = false;

    loop {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        if bytes[i] == b'>' {
            return Some((tag_name, attrs, self_closing, i + 1));
        }
        if bytes[i] == b'/' {
            self_closing = true;
            i += 1;
            continue;
        }
        self_closing = false;
        let attr_start = i;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'='
            && bytes[i] != b'>'
            && bytes[i] != b'/'
        {
            i += 1;
        }
        if i == attr_start {
            i += 1;
            continue;
        }
        let attr_name = raw[attr_start..i].to_ascii_lowercase();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut attr_val = String::new();
        if i < bytes.len() && bytes[i] == b'=' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() {
                return None;
            }
            if bytes[i] == b'"' || bytes[i] == b'\'' {
                let q = bytes[i];
                i += 1;
                let val_start = i;
                while i < bytes.len() && bytes[i] != q {
                    i += 1;
                }
                attr_val = raw[val_start..i].to_owned();
                if i >= bytes.len() {
                    return None;
                }
                i += 1;
            } else {
                let val_start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
                    i += 1;
                }
                attr_val = raw[val_start..i].to_owned();
            }
        }
        if attrs.len() < 32 {
            attrs.push((attr_name, attr_val));
        }
    }
}

fn skip_until_closing_tag(raw: &str, mut cursor: usize, tag_name: &str) -> usize {
    let bytes = raw.as_bytes();
    let tag_bytes = tag_name.as_bytes();
    while let Some(rel) = raw[cursor..].find("</") {
        let pos = cursor + rel;
        let after_name = pos + 2 + tag_bytes.len();
        if starts_with_ignore_ascii_case(&bytes[pos + 2..], tag_bytes)
            && bytes
                .get(after_name)
                .is_some_and(|b| b.is_ascii_whitespace() || *b == b'/' || *b == b'>')
        {
            return skip_to_gt(bytes, after_name);
        }
        cursor = pos + 2;
    }
    bytes.len()
}

fn skip_foreign_container(raw: &str, mut cursor: usize, tag_name: &str) -> usize {
    let bytes = raw.as_bytes();
    let mut depth = 1usize;
    while cursor < bytes.len() && depth > 0 {
        let Some(rel) = raw[cursor..].find('<') else {
            return bytes.len();
        };
        let lt = cursor + rel;
        if bytes[lt..].starts_with(b"<!--") {
            cursor = skip_html_comment(bytes, lt);
            continue;
        }
        if starts_with_ignore_ascii_case(&bytes[lt..], b"<![cdata[") {
            cursor = skip_cdata(bytes, lt);
            continue;
        }
        if bytes[lt..].starts_with(b"</") {
            if bytes.get(lt + 2).is_some_and(|b| b.is_ascii_alphabetic()) {
                let (end_name, next_cursor) = parse_end_tag(raw, lt + 2);
                cursor = next_cursor;
                if end_name == tag_name {
                    depth = depth.saturating_sub(1);
                }
                continue;
            }
            cursor = lt + 1;
            continue;
        }
        if bytes.get(lt + 1).is_some_and(|b| b.is_ascii_alphabetic()) {
            let Some((open_name, _, self_closing, next_cursor)) = parse_start_tag(raw, lt + 1)
            else {
                return bytes.len();
            };
            cursor = next_cursor;
            if open_name == tag_name && !self_closing {
                depth += 1;
            }
            continue;
        }
        cursor = lt + 1;
    }
    cursor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_basic_allowlist_and_adds_rel() {
        let out = sanitize_richtext(
            r#"<p class="x" onclick="evil()">Hello <strong>World</strong> and <a href="https://example.com/path?a=1&b=2" target="_blank">Link</a></p>"#,
        )
        .unwrap();
        assert_eq!(
            out,
            r#"<p>Hello <strong>World</strong> and <a href="https://example.com/path?a=1&amp;b=2" rel="noopener noreferrer">Link</a></p>"#
        );
        // Idempotent under second sanitize pass
        assert_eq!(sanitize_richtext(&out).unwrap(), out);
    }

    #[test]
    fn blocks_url_entity_smuggling_and_duplicate_href() {
        let cases = [
            r#"<a href="java&#x09;script:alert(1)">bad</a>"#,
            r#"<a href="java&#10;script:alert(1)">bad</a>"#,
            r#"<a href="&#106;&#97;vascript:alert(1)">bad</a>"#,
            r#"<a href="&#0000106avascript:alert(1)">bad</a>"#,
            r#"<a href="javascript&colon;alert(1)">bad</a>"#,
            r#"<a href="java&#0;script:alert(1)">bad</a>"#,
            r#"<a href="data:text/html,<script>alert(1)</script>">bad</a>"#,
            r#"<a href="vbscript:msgbox(1)">bad</a>"#,
            r#"<a href="//evil.com/phish">bad</a>"#,
            r#"<a href="/foo/../secret">bad</a>"#,
            r#"<a href="https://safe.com" href="javascript:alert(1)">bad</a>"#,
        ];
        for raw in cases {
            assert_eq!(sanitize_richtext(raw).unwrap(), "bad", "failed on {raw}");
        }
    }

    #[test]
    fn forbids_nested_anchors_and_balances_misnested_tags() {
        let nested = sanitize_richtext(
            r#"<a href="https://a.example">outer <a href="https://b.example">inner</a></a>"#,
        )
        .unwrap();
        assert_eq!(
            nested,
            r#"<a href="https://a.example" rel="noopener noreferrer">outer inner</a>"#
        );

        let balanced = sanitize_richtext("<b><i>bolditalic</b> tail</i>").unwrap();
        assert_eq!(balanced, "<b><i>bolditalic</i></b> tail");
    }

    #[test]
    fn strips_rawtext_rcdata_foreign_and_html5_comments() {
        assert_eq!(
            sanitize_richtext("<p>ok</p><script>alert(1)</script><style>body{}</style>").unwrap(),
            "<p>ok</p>"
        );
        assert_eq!(
            sanitize_richtext("<title><img src=x onerror=1></title><p>hi</p>").unwrap(),
            "<p>hi</p>"
        );
        assert_eq!(
            sanitize_richtext("<textarea>bad</textarea><xmp>bad</xmp><noembed>bad</noembed><noframes>bad</noframes><p>fine</p>").unwrap(),
            "<p>fine</p>"
        );
        assert_eq!(
            sanitize_richtext("<p>a</p><plaintext><b>ignored forever</b>").unwrap(),
            "<p>a</p>"
        );
        assert_eq!(
            sanitize_richtext(
                "<iframe src='x'>sub</iframe><object>obj</object><embed src='y'><span>kept</span>"
            )
            .unwrap(),
            "kept"
        );
        assert_eq!(
            sanitize_richtext("<svg><svg></svg><script>alert(1)</script></svg><math><mtext>x</mtext></math><b>ok</b>").unwrap(),
            "<b>ok</b>"
        );
        assert_eq!(sanitize_richtext("<!--><b>ok</b>").unwrap(), "<b>ok</b>");
        assert_eq!(sanitize_richtext("<!---><p>x</p>").unwrap(), "<p>x</p>");
        assert_eq!(
            sanitize_richtext("<!-- hide --!><p>ok</p>").unwrap(),
            "<p>ok</p>"
        );
    }

    #[test]
    fn enforces_depth_cap_null_and_byte_limits() {
        assert!(sanitize_richtext("abc\0def").is_err());

        let too_big = "a".repeat(MAX_RICHTEXT_INPUT_BYTES + 1);
        assert!(sanitize_richtext(&too_big).is_err());

        // Raw & expands to &amp; (5x), exceeding 32 KiB output limit even under 64 KiB input.
        let amplifies = "&".repeat(10_000);
        assert!(sanitize_richtext(&amplifies).is_err());

        let deep_open = "<b>".repeat(20);
        let deep_close = "</b>".repeat(20);
        let out = sanitize_richtext(&format!("{deep_open}hi{deep_close}")).unwrap();
        assert_eq!(
            out,
            format!(
                "{}hi{}",
                "<b>".repeat(MAX_RICHTEXT_DEPTH),
                "</b>".repeat(MAX_RICHTEXT_DEPTH)
            )
        );
    }

    #[test]
    fn strips_richtext_to_plain_cleanly() {
        assert_eq!(
            strip_richtext_to_plain(
                "<p>Xin   chào</p><script>evil()</script><p><b>OpenDash</b>&nbsp;Studio &amp; CMS</p>"
            ),
            "Xin chào OpenDash Studio & CMS"
        );
    }
}
