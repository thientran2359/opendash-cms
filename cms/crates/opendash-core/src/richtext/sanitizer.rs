use super::*;

pub(super) fn lookup_named_entity(name: &str) -> Option<&'static str> {
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

pub(super) fn collapse_whitespace(input: &str) -> String {
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

pub(super) fn is_plain_break_tag(tag: &str) -> bool {
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

pub(super) fn push_escaped_text(out: &mut String, text: &str) {
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

pub(super) fn allowlist_open_tag(tag: &str) -> Option<&'static str> {
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

pub(super) fn allowlist_closing_tag(tag: &str) -> Option<&'static str> {
    if tag == "a" {
        return Some("a");
    }
    allowlist_open_tag(tag)
}

pub(super) fn is_stripped_container_tag(tag: &str) -> bool {
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

pub(super) fn skip_html_comment(bytes: &[u8], lt: usize) -> usize {
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

pub(super) fn skip_cdata(bytes: &[u8], lt: usize) -> usize {
    let mut i = lt + 9;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"]]>") {
            return i + 3;
        }
        i += 1;
    }
    bytes.len()
}

pub(super) fn skip_to_gt(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        if bytes[i] == b'>' {
            return i + 1;
        }
        i += 1;
    }
    bytes.len()
}

pub(super) fn starts_with_ignore_ascii_case(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.len() >= needle.len() && haystack[..needle.len()].eq_ignore_ascii_case(needle)
}

pub(super) fn parse_end_tag(raw: &str, name_start: usize) -> (String, usize) {
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

pub(super) fn parse_start_tag(
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

pub(super) fn skip_until_closing_tag(raw: &str, mut cursor: usize, tag_name: &str) -> usize {
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

pub(super) fn skip_foreign_container(raw: &str, mut cursor: usize, tag_name: &str) -> usize {
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
