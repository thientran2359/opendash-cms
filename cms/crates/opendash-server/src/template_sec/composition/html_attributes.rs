use super::*;

pub(crate) fn find_html5_comment_end_in_template(bytes: &[u8], start: usize) -> Option<usize> {
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

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ThemeTemplateUsageFlags {
    pub(crate) uses_related_entries: bool,
    pub(crate) uses_recent_posts: bool,
}

pub(crate) fn record_usage_flags_in_call_arg(
    arg: &minijinja::machinery::ast::CallArg<'_>,
    usage: &mut ThemeTemplateUsageFlags,
) {
    match arg {
        minijinja::machinery::ast::CallArg::Pos(e)
        | minijinja::machinery::ast::CallArg::Kwarg(_, e)
        | minijinja::machinery::ast::CallArg::PosSplat(e)
        | minijinja::machinery::ast::CallArg::KwargSplat(e) => record_usage_flags_in_expr(e, usage),
    }
}

pub(crate) fn record_usage_flags_in_expr(expr: &Expr<'_>, usage: &mut ThemeTemplateUsageFlags) {
    match expr {
        Expr::Var(v) => {
            if v.id == "related_entries" {
                usage.uses_related_entries = true;
            } else if v.id == "recent_posts" {
                usage.uses_recent_posts = true;
            }
        }
        Expr::Const(_) => {}
        Expr::Slice(s) => {
            record_usage_flags_in_expr(&s.expr, usage);
            if let Some(start) = &s.start {
                record_usage_flags_in_expr(start, usage);
            }
            if let Some(stop) = &s.stop {
                record_usage_flags_in_expr(stop, usage);
            }
            if let Some(step) = &s.step {
                record_usage_flags_in_expr(step, usage);
            }
        }
        Expr::UnaryOp(u) => record_usage_flags_in_expr(&u.expr, usage),
        Expr::BinOp(b) => {
            record_usage_flags_in_expr(&b.left, usage);
            record_usage_flags_in_expr(&b.right, usage);
        }
        Expr::Compare(c) => {
            record_usage_flags_in_expr(&c.expr, usage);
            for op in &c.ops {
                record_usage_flags_in_expr(&op.expr, usage);
            }
        }
        Expr::IfExpr(i) => {
            record_usage_flags_in_expr(&i.test_expr, usage);
            record_usage_flags_in_expr(&i.true_expr, usage);
            if let Some(f) = &i.false_expr {
                record_usage_flags_in_expr(f, usage);
            }
        }
        Expr::GetAttr(g) => record_usage_flags_in_expr(&g.expr, usage),
        Expr::GetItem(g) => {
            record_usage_flags_in_expr(&g.expr, usage);
            record_usage_flags_in_expr(&g.subscript_expr, usage);
        }
        Expr::List(l) => {
            for item in &l.items {
                record_usage_flags_in_expr(item, usage);
            }
        }
        Expr::Tuple(t) => {
            for item in &t.items {
                record_usage_flags_in_expr(item, usage);
            }
        }
        Expr::Map(m) => {
            for (k, v) in m.keys.iter().zip(m.values.iter()) {
                record_usage_flags_in_expr(k, usage);
                record_usage_flags_in_expr(v, usage);
            }
        }
        Expr::Filter(f) => {
            if let Some(e) = &f.expr {
                record_usage_flags_in_expr(e, usage);
            }
            for arg in &f.args {
                record_usage_flags_in_call_arg(arg, usage);
            }
        }
        Expr::Test(t) => {
            record_usage_flags_in_expr(&t.expr, usage);
            for arg in &t.args {
                record_usage_flags_in_call_arg(arg, usage);
            }
        }
        Expr::Call(c) => {
            record_usage_flags_in_expr(&c.expr, usage);
            for arg in &c.args {
                record_usage_flags_in_call_arg(arg, usage);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TagPhase {
    TagName,
    BeforeAttr,
    AttrName,
    AfterAttrName,
    BeforeAttrValue,
    AttrValue,
    SelfClosingSlash,
}

/// Match one forbidden script prefix incrementally across adjacent raw AST
/// nodes. Each pattern has only one possible proper suffix-prefix: "<".
pub(crate) fn next_script_escaped_prefix(
    pattern: &[u8],
    matched: usize,
    ascii: Option<u8>,
) -> usize {
    match ascii {
        Some(byte) if byte == pattern[matched] => matched + 1,
        Some(b'<') => 1,
        _ => 0,
    }
}

pub(crate) const MAX_THEME_WALKER_STATES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UrlAttrSlot {
    AnchorHref,
    Cite,
    SrcAndHref,
    LinkCss,
    FormAction,
}

pub(crate) fn is_safe_theme_relative_path(raw: &str) -> bool {
    !raw.is_empty()
        && raw.is_ascii()
        && !raw.starts_with('/')
        && !raw.ends_with('/')
        && !raw.contains('\\')
        && !raw.contains(':')
        && !raw.bytes().any(|b| b < 0x20 || b == 0x7f)
        && raw
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

pub(crate) fn is_valid_theme_stylesheet_href(raw_val: &str) -> bool {
    if raw_val.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let decoded = decode_html_entities_for_url(raw_val);
    if decoded.chars().any(|c| c.is_whitespace() || c.is_control())
        || decoded.contains(['?', '#', '\\'])
    {
        return false;
    }
    let Some(rest) = decoded.strip_prefix("/theme/") else {
        return false;
    };
    let Some((digest, rel_path)) = rest.split_once('/') else {
        return false;
    };
    digest.len() == 64
        && digest
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        && is_safe_theme_relative_path(rel_path)
        && rel_path.ends_with(".css")
}

pub(crate) fn is_dangerous_attr_name(
    tag_name: &str,
    attr_name: &str,
    allow_trusted_theme_markup: bool,
    allow_search_form: bool,
) -> bool {
    if matches!(
        attr_name,
        "srcdoc" | "formaction" | "formmethod" | "formenctype" | "formtarget" | "form"
    ) {
        return true;
    }
    if tag_name == "form" && matches!(attr_name, "enctype" | "target") {
        return true;
    }
    if tag_name == "input" && attr_name == "src" {
        return true;
    }
    if matches!(attr_name, "action" | "method") && !(tag_name == "form" && allow_search_form) {
        return true;
    }
    !allow_trusted_theme_markup
        && (attr_name.starts_with("on")
            || matches!(attr_name, "xmlns" | "xmlns:xlink" | "xlink:href"))
}

pub(crate) fn forbids_dynamic_interpolation(tag_name: &str, attr_name: &str) -> bool {
    attr_name.starts_with("on")
        || matches!(
            attr_name,
            "srcdoc"
                | "formaction"
                | "formmethod"
                | "formenctype"
                | "formtarget"
                | "form"
                | "xmlns"
                | "xmlns:xlink"
                | "xlink:href"
        )
        || matches!(
            (tag_name, attr_name),
            ("form", "method" | "enctype" | "target")
                | ("input", "type" | "src")
                | ("button", "type")
        )
}

pub(crate) fn is_security_checked_attr(attr_name: &str) -> bool {
    attr_name.starts_with("on")
        || matches!(
            attr_name,
            "href"
                | "src"
                | "poster"
                | "cite"
                | "style"
                | "srcset"
                | "ping"
                | "rel"
                | "http-equiv"
                | "content"
                | "name"
                | "action"
                | "method"
                | "type"
                | "enctype"
                | "target"
                | "form"
                | "formaction"
                | "formmethod"
                | "formenctype"
                | "formtarget"
                | "xmlns"
                | "xmlns:xlink"
                | "xlink:href"
        )
}

pub(crate) fn is_ascii_html_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

#[derive(Clone, Copy)]
pub(crate) struct HtmlStepOptions {
    pub(crate) is_part: bool,
    pub(crate) is_full_doctype_doc: bool,
    pub(crate) allow_trusted_theme_markup: bool,
    pub(crate) allow_search_form: bool,
    pub(crate) forbid_style_tag: bool,
}

pub(crate) fn classify_theme_url_slot(
    tag_name: &str,
    attr_name: &str,
    allow_search_form: bool,
) -> Option<UrlAttrSlot> {
    match (tag_name, attr_name) {
        ("a", "href") => Some(UrlAttrSlot::AnchorHref),
        ("link", "href") => Some(UrlAttrSlot::LinkCss),
        ("blockquote" | "q", "cite") => Some(UrlAttrSlot::Cite),
        ("form", "action") if allow_search_form => Some(UrlAttrSlot::FormAction),
        (t, "src" | "poster") if !matches!(t, "link" | "meta" | "style" | "title" | "input") => {
            Some(UrlAttrSlot::SrcAndHref)
        }
        _ => None,
    }
}
