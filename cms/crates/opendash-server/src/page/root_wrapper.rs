use super::*;

pub struct RootAttrs<'a> {
    pub node_id: &'a str,
    pub block_id: &'a str,
    pub node_scope: &'a str,
    pub html_id: Option<&'a str>,
    pub extra_classes: &'a [String],
    pub kind: Option<&'a str>,
    pub is_loop_clone: bool,
}

fn is_void_or_replaced_root(tag_lower: &str) -> bool {
    matches!(
        tag_lower,
        "img"
            | "hr"
            | "br"
            | "input"
            | "video"
            | "audio"
            | "iframe"
            | "embed"
            | "source"
            | "area"
            | "base"
            | "col"
            | "link"
            | "meta"
            | "param"
            | "track"
            | "wbr"
    )
}

fn is_html_void_tag(tag_lower: &str) -> bool {
    matches!(
        tag_lower,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn skip_ws_and_comments(src: &str, mut i: usize) -> usize {
    let bytes = src.as_bytes();
    loop {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if src[i..].starts_with("<!--") {
            if let Some(end_rel) = src[i + 4..].find("-->") {
                i = i + 4 + end_rel + 3;
                continue;
            }
            return src.len();
        }
        return i;
    }
}

fn find_tag_end(bytes: &[u8], mut i: usize) -> Option<(usize, bool)> {
    let mut quote: Option<u8> = None;
    let mut trailing_slash = false;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = quote {
            trailing_slash = false;
            if b == q {
                quote = None;
            }
        } else if b == b'"' || b == b'\'' {
            quote = Some(b);
            trailing_slash = false;
        } else if b == b'>' {
            return Some((i, trailing_slash));
        } else if b == b'/' {
            trailing_slash = true;
        } else if !b.is_ascii_whitespace() {
            trailing_slash = false;
        }
        i += 1;
    }
    None
}

fn single_root_span(src: &str) -> Option<(usize, usize, usize, String)> {
    let bytes = src.as_bytes();
    let open_start = skip_ws_and_comments(src, 0);
    if open_start >= bytes.len() || bytes[open_start] != b'<' {
        return None;
    }
    if bytes
        .get(open_start + 1)
        .is_none_or(|b| !b.is_ascii_alphabetic())
    {
        return None;
    }
    let mut name_end = open_start + 1;
    while name_end < bytes.len()
        && (bytes[name_end].is_ascii_alphanumeric() || bytes[name_end] == b'-')
    {
        name_end += 1;
    }
    let root_tag = src[open_start + 1..name_end].to_ascii_lowercase();
    if is_void_or_replaced_root(&root_tag) {
        return None;
    }
    let (open_gt, self_closing) = find_tag_end(bytes, name_end)?;
    if self_closing {
        return None;
    }

    let mut depth: usize = 1;
    let mut i = open_gt + 1;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if src[i..].starts_with("<!--") {
            let end_rel = src[i + 4..].find("-->")?;
            i = i + 4 + end_rel + 3;
            continue;
        }
        let is_close = bytes.get(i + 1) == Some(&b'/');
        let tag_start = i + if is_close { 2 } else { 1 };
        if !bytes.get(tag_start).is_some_and(u8::is_ascii_alphabetic) {
            i += 1;
            continue;
        }
        let mut t_end = tag_start;
        while t_end < bytes.len() && (bytes[t_end].is_ascii_alphanumeric() || bytes[t_end] == b'-')
        {
            t_end += 1;
        }
        let t_name = src[tag_start..t_end].to_ascii_lowercase();
        let (gt, tag_self_close) = find_tag_end(bytes, t_end)?;
        if is_close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                let tail = skip_ws_and_comments(src, gt + 1);
                return (tail == src.len()).then_some((open_start, name_end, open_gt, root_tag));
            }
            i = gt + 1;
        } else {
            if matches!(t_name.as_str(), "script" | "style" | "textarea" | "title")
                && !tag_self_close
            {
                let close_pat = format!("</{t_name}>");
                let lower_tail = src[gt + 1..].to_ascii_lowercase();
                let rel = lower_tail.find(&close_pat)?;
                i = gt + 1 + rel + close_pat.len();
                continue;
            }
            if !tag_self_close && !is_html_void_tag(&t_name) {
                depth += 1;
            }
            i = gt + 1;
        }
    }
    None
}

fn parse_attr_tokens(attr_slice: &str) -> Vec<(String, Option<String>)> {
    let bytes = attr_slice.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let n_start = i;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'='
            && bytes[i] != b'/'
        {
            i += 1;
        }
        let name = attr_slice[n_start..i].to_owned();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let val = if i < bytes.len() && bytes[i] == b'=' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                let q = bytes[i];
                i += 1;
                let v_start = i;
                while i < bytes.len() && bytes[i] != q {
                    i += 1;
                }
                let v = attr_slice[v_start..i].to_owned();
                if i < bytes.len() {
                    i += 1;
                }
                Some(v)
            } else {
                let v_start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                Some(attr_slice[v_start..i].to_owned())
            }
        } else {
            None
        };
        if !name.is_empty() {
            out.push((name, val));
        }
    }
    out
}

fn build_wrapper_attr_suffix(attrs: &RootAttrs<'_>, is_preview: bool) -> String {
    if attrs.is_loop_clone {
        let mut out = format!(
            " data-block-id=\"{}\" data-od-node-scope=\"{}\"",
            escape_html(attrs.block_id),
            escape_html(attrs.node_scope),
        );
        if is_preview {
            out.push_str(" data-od-loop-clone=\"1\"");
        }
        return out;
    }
    let mut out = format!(
        " data-node-id=\"{}\" data-block-id=\"{}\" data-od-node-scope=\"{}\"",
        escape_html(attrs.node_id),
        escape_html(attrs.block_id),
        escape_html(attrs.node_scope),
    );
    if is_preview && let Some(kind) = attrs.kind {
        out.push_str(&format!(
            " data-od-editable=\"1\" data-od-kind=\"{}\"",
            escape_html(kind)
        ));
    }
    out
}

/// Merges Studio node attributes onto a single non-void root element without
/// duplicating `class`/`id`, or wraps multi-root/text/void output in a `<div>`.
pub fn apply_single_root_wrapper(
    rendered: &str,
    attrs: &RootAttrs<'_>,
    is_preview: bool,
) -> String {
    if rendered.trim().is_empty() {
        if !is_preview {
            return String::new();
        }
        let mut classes = vec!["od-block-empty-preview".to_owned()];
        classes.extend(attrs.extra_classes.iter().cloned());
        let id_attr = if attrs.is_loop_clone {
            String::new()
        } else {
            attrs
                .html_id
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|id| format!(" id=\"{}\"", escape_html(id)))
                .unwrap_or_default()
        };
        return format!(
            "<div{id_attr} class=\"{}\"{}></div>",
            escape_html(&classes.join(" ")),
            build_wrapper_attr_suffix(attrs, is_preview),
        );
    }

    let Some((open_start, name_end, open_gt, _root_tag)) = single_root_span(rendered) else {
        let id_attr = if attrs.is_loop_clone {
            String::new()
        } else {
            attrs
                .html_id
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|id| format!(" id=\"{}\"", escape_html(id)))
                .unwrap_or_default()
        };
        let class_attr = if attrs.extra_classes.is_empty() {
            String::new()
        } else {
            format!(" class=\"{}\"", escape_html(&attrs.extra_classes.join(" ")))
        };
        return format!(
            "<div{id_attr}{class_attr}{}>{rendered}</div>",
            build_wrapper_attr_suffix(attrs, is_preview),
        );
    };

    let mut parsed = parse_attr_tokens(&rendered[name_end..open_gt]);
    let extra_class_joined = attrs
        .extra_classes
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut merged_class = false;
    let mut updated_id = false;
    let clean_html_id = if attrs.is_loop_clone {
        None
    } else {
        attrs.html_id.map(str::trim).filter(|s| !s.is_empty())
    };

    for (name, val) in &mut parsed {
        if name.eq_ignore_ascii_case("class") {
            merged_class = true;
            if !extra_class_joined.is_empty() {
                let existing = val.take().unwrap_or_default();
                let combined = if existing.trim().is_empty() {
                    escape_html(&extra_class_joined)
                } else {
                    format!("{} {}", existing.trim(), escape_html(&extra_class_joined))
                };
                *val = Some(combined);
            }
        } else if name.eq_ignore_ascii_case("id")
            && let Some(new_id) = clean_html_id
        {
            updated_id = true;
            *val = Some(escape_html(new_id));
        }
    }
    if !merged_class && !extra_class_joined.is_empty() {
        parsed.push(("class".to_owned(), Some(escape_html(&extra_class_joined))));
    }
    if !updated_id && let Some(new_id) = clean_html_id {
        parsed.push(("id".to_owned(), Some(escape_html(new_id))));
    }

    let mut out = String::with_capacity(rendered.len() + 128);
    out.push_str(&rendered[..name_end]);
    for (name, val) in parsed {
        if matches!(
            name.to_ascii_lowercase().as_str(),
            "data-node-id"
                | "data-block-id"
                | "data-od-node-scope"
                | "data-od-editable"
                | "data-od-kind"
                | "data-od-loop-clone"
        ) {
            continue;
        }
        match val {
            Some(v) => out.push_str(&format!(" {name}=\"{v}\"")),
            None => out.push_str(&format!(" {name}")),
        }
    }
    out.push_str(&build_wrapper_attr_suffix(attrs, is_preview));
    out.push_str(&rendered[open_gt..]);
    let _ = open_start;
    out
}
