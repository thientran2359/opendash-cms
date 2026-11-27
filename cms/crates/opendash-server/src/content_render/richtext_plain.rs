pub fn strip_richtext_to_plain(html: &str) -> String {
    fn is_block_tag(name: &str) -> bool {
        matches!(
            name,
            "p" | "div"
                | "br"
                | "li"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "tr"
                | "td"
                | "th"
                | "blockquote"
                | "section"
                | "article"
                | "header"
                | "footer"
                | "pre"
                | "hr"
                | "ul"
                | "ol"
        )
    }

    let mut stripped = String::with_capacity(html.len());
    let mut i = 0;
    let bytes = html.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let next = bytes.get(i + 1).copied();
            let looks_like_tag =
                next.is_some_and(|b| b.is_ascii_alphabetic() || matches!(b, b'/' | b'!' | b'?'));
            if looks_like_tag {
                let mut quote: Option<u8> = None;
                let mut j = i + 1;
                let mut closed = false;
                while j < bytes.len() {
                    let b = bytes[j];
                    if let Some(q) = quote {
                        if b == q {
                            quote = None;
                        }
                    } else if b == b'"' || b == b'\'' {
                        quote = Some(b);
                    } else if b == b'>' {
                        closed = true;
                        break;
                    }
                    j += 1;
                }
                if closed {
                    let inner = html[i + 1..j].trim_start();
                    let inner = inner.strip_prefix('/').unwrap_or(inner);
                    let tag_name = inner
                        .split(|c: char| c.is_ascii_whitespace() || c == '/')
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if is_block_tag(&tag_name) && !stripped.ends_with(' ') {
                        stripped.push(' ');
                    }
                    i = j + 1;
                    continue;
                }
            }
        }
        let ch = html[i..].chars().next().unwrap_or('\0');
        stripped.push(ch);
        i += ch.len_utf8();
    }

    // Single-pass basic HTML entity decoding so "&amp;lt;" stays "&lt;".
    let mut decoded = String::with_capacity(stripped.len());
    let mut rest = stripped.as_str();
    while let Some(pos) = rest.find('&') {
        decoded.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        let mut matched = None;
        for (entity, replacement) in [
            ("&nbsp;", " "),
            ("&#160;", " "),
            ("&amp;", "&"),
            ("&lt;", "<"),
            ("&gt;", ">"),
            ("&quot;", "\""),
            ("&#39;", "'"),
            ("&#x27;", "'"),
            ("&apos;", "'"),
        ] {
            if tail.starts_with(entity) {
                decoded.push_str(replacement);
                rest = &tail[entity.len()..];
                matched = Some(());
                break;
            }
        }
        if matched.is_none() {
            decoded.push('&');
            rest = &tail[1..];
        }
    }
    decoded.push_str(rest);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}
