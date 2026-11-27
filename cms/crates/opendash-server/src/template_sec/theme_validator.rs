use super::*;

pub(super) fn parse_od_icon_call_ext<'a>(
    expr: &'a Expr<'_>,
    allow_field_path: bool,
) -> Result<Option<(OdIconNameArg<'a>, Option<&'a str>)>, &'static str> {
    let Expr::Call(call) = expr else {
        return Ok(None);
    };
    let Expr::Var(var) = &call.expr else {
        return Ok(None);
    };
    if var.id != "od_icon" {
        return Ok(None);
    }
    let [
        minijinja::machinery::ast::CallArg::Pos(first_arg),
        rest @ ..,
    ] = call.args.as_slice()
    else {
        return Err("od_icon() yêu cầu string literal name và class tùy chọn.");
    };
    let arg = match first_arg {
        Expr::Const(name) => {
            let name = name
                .value
                .as_str()
                .ok_or("od_icon() name phải là string literal.")?;
            if opendash_core::icons::lookup_core_icon_paths(name).is_none() {
                return Err("od_icon() tên icon không tồn tại trong core registry.");
            }
            OdIconNameArg::Literal(name)
        }
        other if allow_field_path => {
            let Some((root, attrs)) = extract_attr_chain(other) else {
                return Err("od_icon() yêu cầu string literal name và class tùy chọn.");
            };
            if attrs.last().copied() != Some("name") || attrs.is_empty() {
                return Err("od_icon() động phải truyền đường dẫn trường kết thúc bằng `.name`.");
            }
            OdIconNameArg::FieldNamePath(root, attrs)
        }
        _ => return Err("od_icon() yêu cầu string literal name và class tùy chọn."),
    };
    let class = match rest {
        [] => None,
        [minijinja::machinery::ast::CallArg::Pos(Expr::Const(class))] => {
            let class = class
                .value
                .as_str()
                .ok_or("od_icon() class phải là string literal.")?;
            if !opendash_core::icons::valid_icon_class_attr(class) {
                return Err("od_icon() class không hợp lệ.");
            }
            Some(class)
        }
        _ => return Err("od_icon() chỉ nhận name và class string literal tùy chọn."),
    };
    Ok(Some((arg, class)))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UrlScopeStack {
    pub(super) frames: Vec<BTreeMap<String, Option<UrlSourceKind>>>,
}

impl UrlScopeStack {
    pub(super) fn root() -> Self {
        Self {
            frames: vec![BTreeMap::from([(
                "entry".to_owned(),
                Some(UrlSourceKind::TrustedEntry),
            )])],
        }
    }

    pub(super) fn from_root_map(root: BTreeMap<String, Option<UrlSourceKind>>) -> Self {
        Self { frames: vec![root] }
    }

    pub(super) fn push_frame(&mut self, frame: BTreeMap<String, Option<UrlSourceKind>>) {
        self.frames.push(frame);
    }

    pub(super) fn pop_frame(&mut self) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
    }

    pub(super) fn insert_top(&mut self, name: String, binding: Option<UrlSourceKind>) {
        if let Some(top) = self.frames.last_mut() {
            top.insert(name, binding);
        }
    }

    pub(super) fn lookup(&self, name: &str) -> Option<&UrlSourceKind> {
        for frame in self.frames.iter().rev() {
            if let Some(binding) = frame.get(name) {
                return binding.as_ref();
            }
        }
        None
    }

    pub(super) fn intersect(a: &Self, b: &Self) -> Self {
        let frames = a
            .frames
            .iter()
            .zip(b.frames.iter())
            .map(|(fa, fb)| {
                let mut merged = BTreeMap::new();
                for key in fa.keys().chain(fb.keys()) {
                    let val = match (fa.get(key), fb.get(key)) {
                        (Some(Some(ka)), Some(Some(kb))) if ka == kb => Some(ka.clone()),
                        _ => None,
                    };
                    merged.insert(key.clone(), val);
                }
                merged
            })
            .collect();
        Self { frames }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ThemeHtmlState {
    Body,
    BodyLt,
    BodyLtSlash,
    BodyLtBang {
        buf: [u8; 7],
        len: u8,
    },
    Doctype,
    Comment(CommentScanSubState),
    Style {
        closing_prefix: usize,
        css_buf: String,
    },
    Script {
        closing_prefix: usize,
        comment_prefix: usize,
        cdata_prefix: usize,
    },
    Title {
        closing_prefix: usize,
    },
    Tag {
        tag_name: String,
        is_close: bool,
        phase: TagPhase,
        attr_name: String,
        quote: Option<u8>,
        attr_has_static_bytes: bool,
        attr_expr_count: u8,
        seen_attrs: BTreeSet<String>,
        link_rel_stylesheet: bool,
        link_has_valid_href: bool,
        meta_ok: bool,
        form_has_action: bool,
        attr_static_buf: String,
    },
    // Only trusted Theme/Block templates track foreign-content & form nesting.
    Trusted {
        inner: Box<ThemeHtmlState>,
        depths: HtmlDepths,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct HtmlDepths {
    pub(super) svg: usize,
    pub(super) math: usize,
    pub(super) form: usize,
}

impl HtmlDepths {
    pub(super) fn is_empty(self) -> bool {
        self.svg == 0 && self.math == 0 && self.form == 0
    }
}

impl ThemeHtmlState {
    pub(super) fn inner(&self) -> &Self {
        match self {
            Self::Trusted { inner, .. } => inner,
            other => other,
        }
    }

    pub(super) fn depths(&self) -> HtmlDepths {
        match self {
            Self::Trusted { depths, .. } => *depths,
            _ => HtmlDepths::default(),
        }
    }

    pub(super) fn form_depth(&self) -> usize {
        self.depths().form
    }

    pub(super) fn has_open_depths(&self) -> bool {
        !self.depths().is_empty()
    }

    pub(super) fn into_inner(self) -> Self {
        match self {
            Self::Trusted { inner, .. } => *inner,
            other => other,
        }
    }

    pub(super) fn with_depths(inner: Self, track_depths: bool, depths: HtmlDepths) -> Self {
        if track_depths || !depths.is_empty() {
            Self::Trusted {
                inner: Box::new(inner),
                depths,
            }
        } else {
            inner
        }
    }
}

pub(super) fn new_theme_tag_state(first_char: char, is_close: bool) -> ThemeHtmlState {
    ThemeHtmlState::Tag {
        tag_name: String::from(first_char.to_ascii_lowercase()),
        is_close,
        phase: TagPhase::TagName,
        attr_name: String::new(),
        quote: None,
        attr_has_static_bytes: false,
        attr_expr_count: 0,
        seen_attrs: BTreeSet::new(),
        link_rel_stylesheet: false,
        link_has_valid_href: false,
        meta_ok: false,
        form_has_action: false,
        attr_static_buf: String::new(),
    }
}

pub(super) fn check_theme_tag_name(
    tag_name: &str,
    is_close: bool,
    opts: HtmlStepOptions,
    depths: HtmlDepths,
) -> Result<(), &'static str> {
    if tag_name.is_empty() {
        return Err("Thẻ HTML không có tên hợp lệ.");
    }
    if opts.forbid_style_tag && tag_name == "style" {
        return Err("Template block không được chứa thẻ <style> trực tiếp (dùng css_var).");
    }
    if is_close
        && matches!(
            tag_name,
            "link" | "meta" | "title" | "style" | "script" | "input"
        )
    {
        return Err("Thẻ đóng HTML không hợp lệ ở vị trí này.");
    }
    if tag_name == "form" {
        if !is_close && depths.form > 0 {
            return Err("C-SEC cấm lồng thẻ <form> bên trong <form>.");
        }
        if is_close && depths.form == 0 {
            return Err("Thẻ </form> không có thẻ mở.");
        }
    }
    let allowed_search_form = opts.allow_search_form
        && depths.svg == 0
        && depths.math == 0
        && is_search_form_tag(tag_name);
    if FORBIDDEN_TAGS.contains(&tag_name)
        && !(opts.allow_trusted_theme_markup
            && matches!(tag_name, "script" | "svg" | "foreignobject" | "math"))
        && !allowed_search_form
        && !(tag_name == "title"
            && !is_close
            && ((!opts.is_part && opts.is_full_doctype_doc)
                || (opts.allow_trusted_theme_markup && depths.svg > 0)))
    {
        return Err("Template chứa thẻ HTML bị C-SEC cấm.");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn finalize_theme_tag(
    tag_name: &str,
    is_close: bool,
    link_rel_stylesheet: bool,
    link_has_valid_href: bool,
    meta_ok: bool,
    form_has_action: bool,
    opts: HtmlStepOptions,
    depths: HtmlDepths,
) -> Result<ThemeHtmlState, &'static str> {
    check_theme_tag_name(tag_name, is_close, opts, depths)?;
    if is_close {
        return Ok(ThemeHtmlState::Body);
    }
    if tag_name == "form" && !form_has_action {
        return Err("C-SEC yêu cầu <form> có thuộc tính action hợp lệ.");
    }
    if tag_name == "link" && !(link_rel_stylesheet && link_has_valid_href) {
        return Err("C-SEC chỉ cho phép <link rel=\"stylesheet\" href=\"/theme/...\">.");
    }
    if tag_name == "meta" && !meta_ok {
        return Err("C-SEC chỉ cho phép <meta charset> hoặc <meta name=\"viewport\">.");
    }
    match tag_name {
        "style" => Ok(ThemeHtmlState::Style {
            closing_prefix: 0,
            css_buf: String::new(),
        }),
        "script" => Ok(ThemeHtmlState::Script {
            closing_prefix: 0,
            comment_prefix: 0,
            cdata_prefix: 0,
        }),
        "title" => Ok(ThemeHtmlState::Title { closing_prefix: 0 }),
        _ => Ok(ThemeHtmlState::Body),
    }
}

pub(super) fn extract_bound_idents(expr: &Expr<'_>) -> Result<Vec<String>, &'static str> {
    match expr {
        Expr::Var(v) => Ok(vec![v.id.to_owned()]),
        Expr::Tuple(t) => {
            let mut out = Vec::new();
            for item in &t.items {
                out.extend(extract_bound_idents(item)?);
            }
            Ok(out)
        }
        Expr::List(l) => {
            let mut out = Vec::new();
            for item in &l.items {
                out.extend(extract_bound_idents(item)?);
            }
            Ok(out)
        }
        _ => Err("C-SEC cấm gán thuộc tính/phần tử bằng GetAttr hoặc GetItem."),
    }
}

pub(super) fn extract_attr_chain<'a>(expr: &'a Expr<'a>) -> Option<(&'a str, Vec<&'a str>)> {
    let mut attrs = Vec::new();
    let mut cur = expr;
    while let Expr::GetAttr(g) = cur {
        attrs.push(g.name);
        cur = &g.expr;
    }
    let Expr::Var(v) = cur else {
        return None;
    };
    attrs.reverse();
    Some((v.id, attrs))
}

pub(super) fn reject_style_control_flow(
    states: &BTreeSet<ThemeHtmlState>,
) -> Result<(), &'static str> {
    if states.iter().any(|s| {
        matches!(
            s.inner(),
            ThemeHtmlState::Style { .. } | ThemeHtmlState::Script { .. }
        ) || matches!(
            s.inner(),
            ThemeHtmlState::Tag { tag_name, attr_name, .. }
                if forbids_dynamic_interpolation(tag_name, attr_name)
        )
    }) {
        return Err("C-SEC cấm Jinja trong <style>/<script> và thuộc tính sự kiện/namespace HTML.");
    }
    Ok(())
}
