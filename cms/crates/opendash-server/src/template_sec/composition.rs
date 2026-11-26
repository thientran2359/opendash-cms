use super::*;

pub const ICON_NONCE_PREFIX: &str = "OD_ICON_";

pub const TEMPLATE_RECURSION_LIMIT: usize = 16;

pub(super) const MAX_COMPOSITION_PART_DEPTH: usize = 3;

pub(super) const MAX_COMPOSITION_EXPANSIONS: usize = 32;

pub(super) const MAX_THEME_CACHE_ENTRIES: usize = 256;

// A compiler-only marker. It is never passed to MiniJinja as a callable or
// trusted HTML variable. A fresh 128-bit random value replaces it per render.
pub(super) const CONTENT_SLOT_TOKEN: &str = "OD_CONTENT_SLOT_COMPILE_MARKER";

// Reserved only by the preview compiler, never a template variable or helper.
pub(super) const PREVIEW_PART_NONCE_PREFIX: &str = "OD_PREVIEW_PART_";

pub const MAX_RICHTEXT_NONCES_PER_REQUEST: usize = 128;

pub const MAX_RICHTEXT_NONCE_REPEATS: usize = 16;

pub const MAX_HOOKS_PER_REQUEST: usize = 32;

pub const MAX_HOOK_ARGS_JSON_BYTES: usize = 16 * 1024;

pub fn valid_hook_slot_name(slot: &str) -> bool {
    !slot.is_empty()
        && slot.len() <= 64
        && slot
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-'))
}

// Per-node fuel cap within the shared 10,000 request RenderBudget.
pub const MAX_BLOCK_FUEL_PER_NODE: u64 = 2_000;

#[derive(Debug, Clone)]
pub(super) enum CompositionCall {
    Part(String),
    Content,
}

#[derive(Debug, Clone)]
pub(super) struct CompositionSite {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) call: CompositionCall,
}

pub(super) const FORBIDDEN_TAGS: &[&str] = &[
    "script",
    "iframe",
    "frame",
    "frameset",
    "object",
    "embed",
    "applet",
    "base",
    "form",
    "input",
    "textarea",
    "select",
    "fieldset",
    "svg",
    "math",
    "foreignobject",
    "noscript",
    "template",
    "portal",
    "title",
    "xmp",
    "noembed",
    "noframes",
    "plaintext",
];

pub(super) const MAX_FILTER_STRING_BYTES: usize = 128 * 1024;

pub(super) const MAX_FILTER_SEQ_ITEMS: usize = 500;

/// Constructs the restricted `Environment::empty()` with unconditional HTML auto-escaping,
/// strict undefined behavior, fuel budget, and recursion limit.
pub(super) const ALLOWED_FILTERS: &[&str] = &[
    "default",
    "d",
    "trim",
    "lower",
    "upper",
    "length",
    "first",
    "last",
    "join",
    "replace",
    "int",
    "float",
    "string",
    "list",
    "abs",
    "round",
    "sort",
    "reverse",
    "unique",
    "title",
    "capitalize",
    "date_format",
    "truncate",
    "format_number",
    "batch",
    "selectattr",
    "rejectattr",
    "map",
];

pub(super) const UNARY_ALLOWED_TESTS: &[&str] = &[
    "defined",
    "undefined",
    "even",
    "odd",
    "number",
    "string",
    "sequence",
    "mapping",
    "none",
    "true",
    "false",
];

pub(super) const BINARY_ALLOWED_TESTS: &[&str] = &[
    "eq",
    "equalto",
    "ne",
    "lt",
    "le",
    "gt",
    "ge",
    "in",
    "startingwith",
    "endingwith",
];

pub(super) const ALLOWED_TESTS: &[&str] = &[
    "defined",
    "undefined",
    "in",
    "startingwith",
    "endingwith",
    "even",
    "odd",
    "number",
    "string",
    "sequence",
    "mapping",
    "eq",
    "equalto",
    "ne",
    "lt",
    "le",
    "gt",
    "ge",
    "none",
    "true",
    "false",
];

pub(super) fn valid_part_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && !key.contains("..")
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'/'))
}

pub(super) fn check_part_graph(
    key: &str,
    sites: &BTreeMap<String, Vec<CompositionSite>>,
    visiting: &mut BTreeSet<String>,
    depth: usize,
    expansions: &mut usize,
) -> Result<(), &'static str> {
    if depth > MAX_COMPOSITION_PART_DEPTH {
        return Err("Độ sâu od_part vượt giới hạn 3.");
    }
    if !visiting.insert(key.to_owned()) {
        return Err("Phát hiện vòng lặp od_part.");
    }
    *expansions += 1;
    if *expansions > MAX_COMPOSITION_EXPANSIONS {
        return Err("od_part vượt quá 32 lần mở rộng.");
    }
    let node = sites
        .get(key)
        .ok_or("od_part trỏ tới part không tồn tại.")?;
    for site in node {
        if let CompositionCall::Part(child) = &site.call {
            check_part_graph(child, sites, visiting, depth + 1, expansions)?;
        }
    }
    visiting.remove(key);
    Ok(())
}

pub(super) fn parse_composition_call(
    expr: &Expr<'_>,
) -> Result<Option<CompositionCall>, &'static str> {
    let Expr::Call(call) = expr else {
        validate_expr(expr)?;
        return Ok(None);
    };
    match &call.expr {
        Expr::Var(var) if var.id == "od_content" => {
            if !call.args.is_empty() {
                return Err("od_content() không nhận tham số.");
            }
            Ok(Some(CompositionCall::Content))
        }
        Expr::Var(var) if var.id == "od_part" => {
            let [minijinja::machinery::ast::CallArg::Pos(Expr::Const(value))] =
                call.args.as_slice()
            else {
                return Err("od_part() chỉ nhận một string literal.");
            };
            let key = value
                .value
                .as_str()
                .ok_or("od_part() phải dùng string literal.")?;
            if !valid_part_key(key) {
                return Err("od_part key không hợp lệ.");
            }
            Ok(Some(CompositionCall::Part(key.to_owned())))
        }
        _ => Err("C-SEC cấm lời gọi hàm Call ngoài od_part/od_content."),
    }
}

#[derive(Debug, Clone)]
pub(super) enum OdIconNameArg<'a> {
    Literal(&'a str),
    FieldNamePath(&'a str, Vec<&'a str>),
}

pub(super) fn parse_od_icon_call<'a>(
    expr: &'a Expr<'_>,
) -> Result<Option<(&'a str, Option<&'a str>)>, &'static str> {
    match parse_od_icon_call_ext(expr, false)? {
        Some((OdIconNameArg::Literal(name), class)) => Ok(Some((name, class))),
        Some((OdIconNameArg::FieldNamePath(_, _), _)) => {
            Err("od_icon() yêu cầu string literal name và class tùy chọn.")
        }
        None => Ok(None),
    }
}

pub(super) fn parse_od_hook_call<'a>(
    expr: &'a Expr<'_>,
) -> Result<Option<(&'a str, Option<&'a Expr<'a>>)>, &'static str> {
    let Expr::Call(call) = expr else {
        return Ok(None);
    };
    let Expr::Var(var) = &call.expr else {
        return Ok(None);
    };
    if var.id != "od_hook" {
        return Ok(None);
    }
    let [
        minijinja::machinery::ast::CallArg::Pos(Expr::Const(slot)),
        rest @ ..,
    ] = call.args.as_slice()
    else {
        return Err("od_hook() yêu cầu string literal slot và biểu thức tham số tùy chọn.");
    };
    let slot = slot
        .value
        .as_str()
        .ok_or("od_hook() slot phải là string literal.")?;
    if !valid_hook_slot_name(slot) {
        return Err("od_hook() tên slot không hợp lệ.");
    }
    let arg_expr = match rest {
        [] => None,
        [minijinja::machinery::ast::CallArg::Pos(arg)] => {
            validate_expr(arg)?;
            Some(arg)
        }
        _ => return Err("od_hook() chỉ nhận slot string literal và tối đa một tham số tùy chọn."),
    };
    Ok(Some((slot, arg_expr)))
}

pub(super) fn reject_icons_in_set_block(children: &[Stmt<'_>]) -> Result<(), &'static str> {
    fn contains_icon(stmt: &Stmt<'_>) -> bool {
        match stmt {
            Stmt::EmitExpr(e) => {
                parse_od_icon_call_ext(&e.expr, true).is_ok_and(|v| v.is_some())
                    || parse_od_hook_call(&e.expr).is_ok_and(|v| v.is_some())
            }
            Stmt::IfCond(c) => c.true_body.iter().chain(&c.false_body).any(contains_icon),
            Stmt::ForLoop(c) => c.body.iter().chain(&c.else_body).any(contains_icon),
            Stmt::WithBlock(c) => c.body.iter().any(contains_icon),
            Stmt::SetBlock(c) => c.body.iter().any(contains_icon),
            Stmt::Template(c) => c.children.iter().any(contains_icon),
            _ => false,
        }
    }
    if children.iter().any(contains_icon) {
        Err("C-SEC cấm od_icon() trong set block.")
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum CommentScanSubState {
    /// Immediately after `<!--`: `>` closes (`<!-->`), `-` enters `StartDash`.
    Start,
    /// Immediately after `<!---`: `>` closes (`<!--->`), `-` enters `Dash2`.
    StartDash,
    /// Inside comment body.
    Body,
    /// Saw one `-` in comment body.
    Dash1,
    /// Saw `--`: `>` closes (`-->`), `!` enters `Dash2Bang`, `-` stays `Dash2`.
    Dash2,
    /// Saw `--!`: `>` closes (`--!>`), `-` enters `Dash1`.
    Dash2Bang,
}

pub(super) fn step_comment_substate(
    sub: CommentScanSubState,
    byte: u8,
) -> Option<CommentScanSubState> {
    match sub {
        CommentScanSubState::Start => match byte {
            b'>' => None,
            b'-' => Some(CommentScanSubState::StartDash),
            _ => Some(CommentScanSubState::Body),
        },
        CommentScanSubState::StartDash => match byte {
            b'>' => None,
            b'-' => Some(CommentScanSubState::Dash2),
            _ => Some(CommentScanSubState::Body),
        },
        CommentScanSubState::Body => match byte {
            b'-' => Some(CommentScanSubState::Dash1),
            _ => Some(CommentScanSubState::Body),
        },
        CommentScanSubState::Dash1 => match byte {
            b'-' => Some(CommentScanSubState::Dash2),
            _ => Some(CommentScanSubState::Body),
        },
        CommentScanSubState::Dash2 => match byte {
            b'>' => None,
            b'!' => Some(CommentScanSubState::Dash2Bang),
            b'-' => Some(CommentScanSubState::Dash2),
            _ => Some(CommentScanSubState::Body),
        },
        CommentScanSubState::Dash2Bang => match byte {
            b'>' => None,
            b'-' => Some(CommentScanSubState::Dash1),
            _ => Some(CommentScanSubState::Body),
        },
    }
}

impl HtmlScanState {
    pub(super) fn consume(self, byte: u8) -> Self {
        match self {
            Self::Body if byte == b'<' => Self::Tag {
                quote: None,
                prefix: "<".to_owned(),
            },
            Self::Body => Self::Body,
            Self::Comment(sub) => match step_comment_substate(sub, byte) {
                Some(next) => Self::Comment(next),
                None => Self::Body,
            },
            Self::Style { closing_prefix } => {
                const CLOSE: &[u8] = b"</style>";
                let lowered = byte.to_ascii_lowercase();
                let next = if lowered == CLOSE[closing_prefix] {
                    closing_prefix + 1
                } else if lowered == b'<' {
                    1
                } else {
                    0
                };
                if next == CLOSE.len() {
                    Self::Body
                } else {
                    Self::Style {
                        closing_prefix: next,
                    }
                }
            }
            Self::Script { closing_prefix } => {
                const CLOSE: &[u8] = b"</script>";
                let lowered = byte.to_ascii_lowercase();
                let next = if lowered == CLOSE[closing_prefix] {
                    closing_prefix + 1
                } else if lowered == b'<' {
                    1
                } else {
                    0
                };
                if next == CLOSE.len() {
                    Self::Body
                } else {
                    Self::Script {
                        closing_prefix: next,
                    }
                }
            }
            Self::Title { closing_prefix } => {
                const CLOSE: &[u8] = b"</title>";
                let lowered = byte.to_ascii_lowercase();
                let next = if lowered == CLOSE[closing_prefix] {
                    closing_prefix + 1
                } else if lowered == b'<' {
                    1
                } else {
                    0
                };
                if next == CLOSE.len() {
                    Self::Body
                } else {
                    Self::Title {
                        closing_prefix: next,
                    }
                }
            }
            Self::Tag {
                mut quote,
                mut prefix,
            } => {
                if prefix.len() < 8 {
                    prefix.push(char::from(byte));
                }
                if prefix == "<!--" {
                    return Self::Comment(CommentScanSubState::Start);
                }
                if let Some(q) = quote {
                    if byte == q {
                        quote = None;
                    }
                } else if byte == b'"' || byte == b'\'' {
                    quote = Some(byte);
                } else if byte == b'>' {
                    let lower = prefix.to_ascii_lowercase();
                    if lower.starts_with("<style")
                        && lower
                            .as_bytes()
                            .get(6)
                            .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>')
                    {
                        return Self::Style { closing_prefix: 0 };
                    }
                    if lower.starts_with("<script")
                        && lower
                            .as_bytes()
                            .get(7)
                            .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>')
                    {
                        return Self::Script { closing_prefix: 0 };
                    }
                    if lower.starts_with("<title")
                        && lower
                            .as_bytes()
                            .get(6)
                            .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>')
                    {
                        return Self::Title { closing_prefix: 0 };
                    }
                    return Self::Body;
                }
                Self::Tag { quote, prefix }
            }
        }
    }
}

/// Validates a scoped block CSS stylesheet emitted by `page::serialize_scoped_block_css`.
/// Rejects CSS breakout tokens and allows `/api/admin/media/{64hex}/file` only when `is_preview == true`.
pub fn validate_safe_scoped_block_css(css: &str, is_preview: bool) -> Result<(), &'static str> {
    if css.contains('\0')
        || css.contains("/*")
        || css.contains("*/")
        || css.contains('\\')
        || css.contains('\r')
        || css.contains('\n')
    {
        return Err("Scoped block CSS chứa ký tự hoặc comment bị cấm.");
    }
    let lower = css.to_ascii_lowercase();
    if lower.contains("</style")
        || lower.contains("@import")
        || lower.contains("expression(")
        || lower.contains("-moz-binding")
        || lower.contains("behavior:")
    {
        return Err("Scoped block CSS vi phạm chính sách C-SEC.");
    }

    let mut idx = 0;
    while let Some(pos) = lower[idx..].find("url(") {
        let start = idx + pos + 4;
        let Some(end_rel) = lower[start..].find(')') else {
            return Err("CSS url(...) không hợp lệ.");
        };
        let raw_inner = css[start..start + end_rel].trim();
        let inner = raw_inner
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| {
                raw_inner
                    .strip_prefix('\'')
                    .and_then(|s| s.strip_suffix('\''))
            })
            .unwrap_or(raw_inner)
            .trim();
        let allowed_public = inner.starts_with("/theme/") || inner.starts_with("/media/");
        let allowed_preview = is_preview && inner.starts_with("/api/admin/media/");
        if !(allowed_public || allowed_preview)
            || inner.contains("..")
            || inner.contains('\\')
            || inner.contains('{')
            || inner.contains('}')
            || inner.contains(';')
            || inner.bytes().any(|b| b <= 0x1f || b == 0x7f)
        {
            return Err("Scoped block CSS url(...) không hợp lệ hoặc không được phép.");
        }
        idx = start + end_rel + 1;
    }
    Ok(())
}

/// Validates a static or post-render URL used in `href`, `src`, `poster`, `cite`, `srcset`, or `ping`.
pub(super) fn decode_html_entities_for_url(raw: &str) -> String {
    opendash_core::richtext::decode_html5_entities(raw)
}

pub fn validate_safe_url(raw: &str) -> Result<(), &'static str> {
    if raw.contains('\0') || raw.bytes().any(|b| b <= 0x1f || b == 0x7f) {
        return Err("URL trong template chứa ký tự điều khiển bị cấm.");
    }
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.bytes().any(|b| b < 0x20 || b == 0x7f)
        || trimmed.contains('\\')
    {
        return Err("URL trong template không hợp lệ.");
    }
    if trimmed.starts_with('/') {
        if trimmed.starts_with("//")
            || trimmed == "/.."
            || trimmed.contains("/../")
            || trimmed.ends_with("/..")
            || trimmed.contains("..")
        {
            return Err("URL tương đối không được chứa // hoặc ..");
        }
        return Ok(());
    }
    let parsed = url::Url::parse(trimmed).map_err(|_| "URL không hợp lệ.")?;
    if !matches!(parsed.scheme(), "http" | "https") || !parsed.has_host() {
        return Err("Chỉ cho phép URL http(s) hoặc đường dẫn /...");
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("URL không được chứa thông tin đăng nhập.");
    }
    Ok(())
}

/// Validates a URL used specifically in `<a href="...">`, allowing `#`, `#fragment`,
/// `mailto:`, and `tel:` in addition to `/...` and `http(s)://...`.
pub fn validate_safe_href_url(raw: &str) -> Result<(), &'static str> {
    if raw.contains('\0') || raw.bytes().any(|b| b <= 0x1f || b == 0x7f) {
        return Err("URL href chứa ký tự điều khiển bị cấm.");
    }
    let trimmed = raw.trim();
    if validate_safe_url(raw).is_ok() || opendash_core::field_schema::valid_safe_url(trimmed) {
        Ok(())
    } else {
        Err("URL trong <a href> không hợp lệ hoặc không an toàn.")
    }
}

/// Validated navigation value for an explicitly typed theme option. Stored
/// bytes are never trimmed or decoded; checking the decoded variant only
/// prevents entity-based scheme and attribute bypasses.
pub fn validate_theme_option_url(raw: &str) -> Result<(), &'static str> {
    if raw.is_empty() || raw.chars().count() > 4096 {
        return Err("Theme URL phải có 1–4096 ký tự Unicode.");
    }
    let decoded = decode_html_entities_for_url(raw);
    for value in [raw, decoded.as_str()] {
        if value.chars().any(|ch| {
            ch.is_whitespace()
                || ch.is_control()
                || matches!(ch, '\\' | '<' | '>' | '\'' | '"' | '`')
        }) {
            return Err("Theme URL chứa ký tự không an toàn.");
        }
        validate_safe_href_url(value)?;
    }
    Ok(())
}

pub fn json_to_safe_value(value: &serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value::from(()),
        serde_json::Value::Bool(v) => Value::from(*v),
        serde_json::Value::Number(v) => {
            if let Some(i) = v.as_i64() {
                Value::from(i)
            } else if let Some(u) = v.as_u64() {
                Value::from(u)
            } else {
                Value::from(v.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(v) => Value::from(v.clone()),
        serde_json::Value::Array(v) => {
            Value::from(v.iter().map(json_to_safe_value).collect::<Vec<_>>())
        }
        serde_json::Value::Object(v) => Value::from_pairs(
            v.iter()
                .map(|(k, val)| (k.clone(), json_to_safe_value(val))),
        ),
    }
}

/// Converts a JSON context object into a MiniJinja `Value`, attaching pre-built
/// `Arc`-backed `t` and `locale` values without re-serializing them on every fragment.
pub fn json_context_with_shared_locale(
    value: &serde_json::Value,
    shared_t: Option<&Value>,
    shared_locale: Option<&Value>,
) -> Value {
    if shared_t.is_none() && shared_locale.is_none() {
        return json_to_safe_value(value);
    }
    let mut pairs: BTreeMap<String, Value> = match value {
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(k, val)| (k.clone(), json_to_safe_value(val)))
            .collect(),
        _ => BTreeMap::new(),
    };
    if let Some(t) = shared_t {
        pairs.entry("t".to_owned()).or_insert_with(|| t.clone());
    }
    if let Some(locale) = shared_locale {
        pairs
            .entry("locale".to_owned())
            .or_insert_with(|| locale.clone());
    }
    Value::from_pairs(pairs)
}

pub(super) fn validate_stmt(stmt: &Stmt<'_>) -> Result<(), &'static str> {
    validate_stmt_with_icons(stmt, false, false)
}

pub(super) fn validate_stmt_with_icons(
    stmt: &Stmt<'_>,
    allow_od_icon: bool,
    in_set_block: bool,
) -> Result<(), &'static str> {
    validate_stmt_with_icons_ext(stmt, allow_od_icon, false, in_set_block)
}

pub(super) const RESERVED_CONTEXT_VARS: &[&str] = &[
    "od_icon",
    "od_hook",
    "options",
    "site_options",
    "theme_options",
    "theme_assets",
    "site",
    "menus",
    "entries",
    "pagination",
    "breadcrumbs",
    "taxonomy",
    "related_entries",
    "recent_posts",
    "route",
    "locale",
    "t",
    "content_type",
    "theme_asset_url",
    "previous_url",
    "next_url",
    "archive_url",
    "od_content",
    "od_part",
];

pub(super) fn binds_reserved_context(expr: &Expr<'_>) -> bool {
    match expr {
        Expr::Var(var) => RESERVED_CONTEXT_VARS.contains(&var.id),
        Expr::Tuple(tuple) => tuple.items.iter().any(binds_reserved_context),
        Expr::List(list) => list.items.iter().any(binds_reserved_context),
        // Attribute and subscript assignments are forbidden rather than
        // allowing a mutable binding to masquerade as a reserved context root.
        Expr::GetAttr(attr) => binds_reserved_context(&attr.expr),
        Expr::GetItem(item) => binds_reserved_context(&item.expr),
        _ => false,
    }
}

pub(super) fn reject_reserved_context_rebinding(stmt: &Stmt<'_>) -> Result<(), &'static str> {
    let forbidden = || Err("C-SEC cấm gán lại biến ngữ cảnh hệ thống trong theme.");
    match stmt {
        Stmt::Template(t) => t
            .children
            .iter()
            .try_for_each(reject_reserved_context_rebinding),
        Stmt::ForLoop(f) => {
            if binds_reserved_context(&f.target) {
                return forbidden();
            }
            f.body
                .iter()
                .chain(&f.else_body)
                .try_for_each(reject_reserved_context_rebinding)
        }
        Stmt::WithBlock(w) => {
            if w.assignments
                .iter()
                .any(|(key, _)| binds_reserved_context(key))
            {
                return forbidden();
            }
            w.body
                .iter()
                .try_for_each(reject_reserved_context_rebinding)
        }
        Stmt::Set(s) => {
            if binds_reserved_context(&s.target) {
                forbidden()
            } else {
                Ok(())
            }
        }
        Stmt::SetBlock(sb) => {
            if binds_reserved_context(&sb.target) {
                return forbidden();
            }
            sb.body
                .iter()
                .try_for_each(reject_reserved_context_rebinding)
        }
        Stmt::IfCond(i) => i
            .true_body
            .iter()
            .chain(&i.false_body)
            .try_for_each(reject_reserved_context_rebinding),
        _ => Ok(()),
    }
}

pub(super) fn validate_expr(expr: &Expr<'_>) -> Result<(), &'static str> {
    match expr {
        Expr::Var(_) | Expr::Const(_) => Ok(()),
        Expr::Slice(s) => {
            validate_expr(&s.expr)?;
            if let Some(start) = &s.start {
                validate_expr(start)?;
            }
            if let Some(stop) = &s.stop {
                validate_expr(stop)?;
            }
            if let Some(step) = &s.step {
                validate_expr(step)?;
            }
            Ok(())
        }
        Expr::UnaryOp(u) => validate_expr(&u.expr),
        Expr::BinOp(b) => {
            validate_expr(&b.left)?;
            validate_expr(&b.right)
        }
        Expr::Compare(c) => {
            validate_expr(&c.expr)?;
            for op in &c.ops {
                validate_expr(&op.expr)?;
            }
            Ok(())
        }
        Expr::IfExpr(i) => {
            validate_expr(&i.test_expr)?;
            validate_expr(&i.true_expr)?;
            if let Some(f) = &i.false_expr {
                validate_expr(f)?;
            }
            Ok(())
        }
        Expr::GetAttr(g) => {
            if g.name.starts_with("__") {
                return Err("C-SEC cấm truy cập thuộc tính dunder (__).");
            }
            validate_expr(&g.expr)
        }
        Expr::GetItem(g) => {
            if matches!(&g.expr, Expr::Var(v) if v.id == "t") {
                return Err("C-SEC cấm truy cập t[...] động; hãy dùng t.<key>.");
            }
            if matches!(&g.subscript_expr, Expr::Const(c) if c.value.as_str().is_some_and(|s| s.starts_with("__")))
            {
                return Err("C-SEC cấm truy cập thuộc tính dunder (__).");
            }
            validate_expr(&g.expr)?;
            validate_expr(&g.subscript_expr)
        }
        Expr::List(l) => {
            for item in &l.items {
                validate_expr(item)?;
            }
            Ok(())
        }
        Expr::Tuple(t) => {
            for item in &t.items {
                validate_expr(item)?;
            }
            Ok(())
        }
        Expr::Map(m) => {
            for (k, v) in m.keys.iter().zip(m.values.iter()) {
                validate_expr(k)?;
                validate_expr(v)?;
            }
            Ok(())
        }
        Expr::Filter(f) => {
            if !ALLOWED_FILTERS.contains(&f.name) {
                return Err("Template sử dụng filter MiniJinja không nằm trong whitelist C-SEC.");
            }
            if let Some(e) = &f.expr {
                validate_expr(e)?;
            }
            for arg in &f.args {
                validate_call_arg(arg)?;
            }
            Ok(())
        }
        Expr::Test(t) => {
            if !ALLOWED_TESTS.contains(&t.name) {
                return Err("Template sử dụng test MiniJinja không nằm trong whitelist C-SEC.");
            }
            validate_expr(&t.expr)?;
            for arg in &t.args {
                validate_call_arg(arg)?;
            }
            Ok(())
        }
        Expr::Call(_) => Err("C-SEC cấm lời gọi hàm Call trong template."),
    }
}

pub(super) fn validate_call_arg(
    arg: &minijinja::machinery::ast::CallArg<'_>,
) -> Result<(), &'static str> {
    match arg {
        minijinja::machinery::ast::CallArg::Pos(e)
        | minijinja::machinery::ast::CallArg::Kwarg(_, e)
        | minijinja::machinery::ast::CallArg::PosSplat(e)
        | minijinja::machinery::ast::CallArg::KwargSplat(e) => validate_expr(e),
    }
}

/// Collects every `t.<key>` translation key referenced by a theme template AST.
pub fn collect_theme_translation_keys(source: &str) -> Result<BTreeSet<String>, &'static str> {
    let ast = parse(source, "theme-l10n", SyntaxConfig::default())
        .map_err(|_| "Cú pháp theme MiniJinja không hợp lệ.")?;
    let mut keys = BTreeSet::new();
    collect_t_keys_stmt(&ast, &mut keys);
    Ok(keys)
}

pub(super) fn collect_t_keys_expr(expr: &Expr<'_>, out: &mut BTreeSet<String>) {
    match expr {
        Expr::GetAttr(g) => {
            if matches!(&g.expr, Expr::Var(v) if v.id == "t") {
                out.insert(g.name.to_owned());
            } else {
                collect_t_keys_expr(&g.expr, out);
            }
        }
        Expr::GetItem(g) => {
            collect_t_keys_expr(&g.expr, out);
            collect_t_keys_expr(&g.subscript_expr, out);
        }
        Expr::Slice(s) => {
            collect_t_keys_expr(&s.expr, out);
            if let Some(start) = &s.start {
                collect_t_keys_expr(start, out);
            }
            if let Some(stop) = &s.stop {
                collect_t_keys_expr(stop, out);
            }
            if let Some(step) = &s.step {
                collect_t_keys_expr(step, out);
            }
        }
        Expr::UnaryOp(u) => collect_t_keys_expr(&u.expr, out),
        Expr::BinOp(b) => {
            collect_t_keys_expr(&b.left, out);
            collect_t_keys_expr(&b.right, out);
        }
        Expr::Compare(c) => {
            collect_t_keys_expr(&c.expr, out);
            for op in &c.ops {
                collect_t_keys_expr(&op.expr, out);
            }
        }
        Expr::IfExpr(i) => {
            collect_t_keys_expr(&i.test_expr, out);
            collect_t_keys_expr(&i.true_expr, out);
            if let Some(f) = &i.false_expr {
                collect_t_keys_expr(f, out);
            }
        }
        Expr::List(l) => {
            for item in &l.items {
                collect_t_keys_expr(item, out);
            }
        }
        Expr::Tuple(t) => {
            for item in &t.items {
                collect_t_keys_expr(item, out);
            }
        }
        Expr::Map(m) => {
            for (k, v) in m.keys.iter().zip(m.values.iter()) {
                collect_t_keys_expr(k, out);
                collect_t_keys_expr(v, out);
            }
        }
        Expr::Filter(f) => {
            if let Some(e) = &f.expr {
                collect_t_keys_expr(e, out);
            }
            for arg in &f.args {
                collect_t_keys_call_arg(arg, out);
            }
        }
        Expr::Test(t) => {
            collect_t_keys_expr(&t.expr, out);
            for arg in &t.args {
                collect_t_keys_call_arg(arg, out);
            }
        }
        Expr::Var(_) | Expr::Const(_) | Expr::Call(_) => {}
    }
}

pub(super) fn collect_t_keys_call_arg(
    arg: &minijinja::machinery::ast::CallArg<'_>,
    out: &mut BTreeSet<String>,
) {
    match arg {
        minijinja::machinery::ast::CallArg::Pos(e)
        | minijinja::machinery::ast::CallArg::Kwarg(_, e)
        | minijinja::machinery::ast::CallArg::PosSplat(e)
        | minijinja::machinery::ast::CallArg::KwargSplat(e) => collect_t_keys_expr(e, out),
    }
}

pub(super) fn find_html5_comment_end_in_template(bytes: &[u8], start: usize) -> Option<usize> {
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
pub(super) struct ThemeTemplateUsageFlags {
    pub(super) uses_related_entries: bool,
    pub(super) uses_recent_posts: bool,
}

pub(super) fn record_usage_flags_in_call_arg(
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

pub(super) fn record_usage_flags_in_expr(expr: &Expr<'_>, usage: &mut ThemeTemplateUsageFlags) {
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
pub(super) enum TagPhase {
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
pub(super) fn next_script_escaped_prefix(
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

pub(super) const MAX_THEME_WALKER_STATES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UrlAttrSlot {
    AnchorHref,
    Cite,
    SrcAndHref,
    LinkCss,
}

pub(super) fn is_safe_theme_relative_path(raw: &str) -> bool {
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

pub(super) fn is_dangerous_attr_name(attr_name: &str, allow_trusted_theme_markup: bool) -> bool {
    matches!(attr_name, "srcdoc" | "formaction" | "action")
        || (!allow_trusted_theme_markup
            && (attr_name.starts_with("on")
                || matches!(attr_name, "xmlns" | "xmlns:xlink" | "xlink:href")))
}

pub(super) fn forbids_dynamic_interpolation(attr_name: &str) -> bool {
    attr_name.starts_with("on")
        || matches!(
            attr_name,
            "srcdoc" | "formaction" | "action" | "xmlns" | "xmlns:xlink" | "xlink:href"
        )
}

pub(super) fn is_security_checked_attr(attr_name: &str) -> bool {
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
                | "xmlns"
                | "xmlns:xlink"
                | "xlink:href"
        )
}

pub(super) fn is_ascii_html_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

#[derive(Clone, Copy)]
pub(super) struct HtmlStepOptions {
    pub(super) is_part: bool,
    pub(super) is_full_doctype_doc: bool,
    pub(super) allow_trusted_theme_markup: bool,
    pub(super) forbid_style_tag: bool,
}

pub(super) fn classify_theme_url_slot(tag_name: &str, attr_name: &str) -> Option<UrlAttrSlot> {
    match (tag_name, attr_name) {
        ("a", "href") => Some(UrlAttrSlot::AnchorHref),
        ("link", "href") => Some(UrlAttrSlot::LinkCss),
        ("blockquote" | "q", "cite") => Some(UrlAttrSlot::Cite),
        (t, "src" | "poster") if !matches!(t, "link" | "meta" | "style" | "title") => {
            Some(UrlAttrSlot::SrcAndHref)
        }
        _ => None,
    }
}

/// Strict C-SEC HTML/URL/CSS validator for opt-out untrusted plugin fragments
/// (`render_trusted_html == false`). Forbids `<script>`, `<svg>`, `<style>`, `<meta>`,
/// `<link>`, `<!doctype>`, `<form>`, `on*`, `javascript:`/`data:` URLs, `formaction`/`action`,
/// and unsafe `style="..."` CSS (`url()`, `expression()`, `@import`, etc.).
pub(crate) fn validate_untrusted_extension_html_fragment(src: &str) -> Result<(), &'static str> {
    let lower = src.to_ascii_lowercase();
    for forbidden_prefix in ["<style", "</style", "<meta", "<link", "<!doctype"] {
        let mut cursor = 0;
        while let Some(rel) = lower[cursor..].find(forbidden_prefix) {
            let end = cursor + rel + forbidden_prefix.len();
            let boundary = lower
                .as_bytes()
                .get(end)
                .is_none_or(|b| b.is_ascii_whitespace() || matches!(*b, b'>' | b'/'));
            if boundary {
                return Err("Fragment untrusted chứa thẻ HTML bị C-SEC cấm.");
            }
            cursor = end;
        }
    }
    validate_html_and_css_context(src, true, false)
}

pub(super) fn find_rawtext_or_rcdata_end(
    src: &str,
    body_start: usize,
    tag_name: &str,
) -> Result<(usize, usize), &'static str> {
    let bytes = src.as_bytes();
    let needle: &[u8] = match tag_name {
        "style" => b"</style",
        "script" => b"</script",
        "title" => b"</title",
        _ => return Err("Thẻ raw-text không hợp lệ."),
    };
    let mut pos = body_start;
    while pos + needle.len() <= bytes.len() {
        if bytes[pos..pos + needle.len()].eq_ignore_ascii_case(needle) {
            match bytes.get(pos + needle.len()) {
                Some(&b'>') => return Ok((pos, pos + needle.len() + 1)),
                Some(&b) if is_ascii_html_whitespace(b) || b == b'/' => {
                    return Err("Thẻ đóng raw-text không được chứa khoảng trắng hoặc /.");
                }
                None => return Err("Thẻ đóng raw-text không hoàn chỉnh."),
                Some(_) => {}
            }
        }
        pos += 1;
    }
    Err("Thẻ raw-text thiếu thẻ đóng hợp lệ.")
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PostRenderTagScanState {
    BeforeAttrName,
    AttrName,
    AfterAttrName,
    BeforeAttrValue,
    QuotedValue(u8),
    UnquotedValue,
}

pub(super) fn find_post_render_tag_end(bytes: &[u8], start: usize) -> Result<usize, &'static str> {
    let mut i = start;
    let mut state = PostRenderTagScanState::BeforeAttrName;
    while i < bytes.len() {
        let b = bytes[i];
        match state {
            PostRenderTagScanState::BeforeAttrName => {
                if b == b'>' {
                    return Ok(i);
                }
                if b.is_ascii_whitespace() || b == b'/' {
                    // stay
                } else if matches!(b, b'"' | b'\'' | b'<' | b'=' | b'`') {
                    return Err("Thẻ HTML chứa ký tự không hợp lệ ở vị trí tên thuộc tính.");
                } else {
                    state = PostRenderTagScanState::AttrName;
                }
            }
            PostRenderTagScanState::AttrName => {
                if b == b'>' {
                    return Ok(i);
                }
                if b.is_ascii_whitespace() {
                    state = PostRenderTagScanState::AfterAttrName;
                } else if b == b'/' {
                    state = PostRenderTagScanState::BeforeAttrName;
                } else if b == b'=' {
                    state = PostRenderTagScanState::BeforeAttrValue;
                } else if matches!(b, b'"' | b'\'' | b'<' | b'`') {
                    return Err("Tên thuộc tính HTML chứa ký tự không hợp lệ.");
                }
            }
            PostRenderTagScanState::AfterAttrName => {
                if b == b'>' {
                    return Ok(i);
                }
                if b.is_ascii_whitespace() {
                    // stay
                } else if b == b'/' {
                    state = PostRenderTagScanState::BeforeAttrName;
                } else if b == b'=' {
                    state = PostRenderTagScanState::BeforeAttrValue;
                } else if matches!(b, b'"' | b'\'' | b'<' | b'`') {
                    return Err("Cú pháp thẻ HTML chứa ký tự không hợp lệ sau tên thuộc tính.");
                } else {
                    state = PostRenderTagScanState::AttrName;
                }
            }
            PostRenderTagScanState::BeforeAttrValue => {
                if b.is_ascii_whitespace() {
                    // stay
                } else if b == b'>' {
                    return Ok(i);
                } else if b == b'"' || b == b'\'' {
                    state = PostRenderTagScanState::QuotedValue(b);
                } else if matches!(b, b'<' | b'=' | b'`') {
                    return Err("Giá trị thuộc tính HTML bắt đầu bằng ký tự không hợp lệ.");
                } else {
                    state = PostRenderTagScanState::UnquotedValue;
                }
            }
            PostRenderTagScanState::QuotedValue(q) => {
                if b == q {
                    state = PostRenderTagScanState::BeforeAttrName;
                }
            }
            PostRenderTagScanState::UnquotedValue => {
                if b == b'>' {
                    return Ok(i);
                }
                if b.is_ascii_whitespace() {
                    state = PostRenderTagScanState::BeforeAttrName;
                } else if matches!(b, b'"' | b'\'' | b'<' | b'=' | b'`') {
                    return Err(
                        "Giá trị thuộc tính HTML không có dấu nháy chứa ký tự không hợp lệ.",
                    );
                }
            }
        }
        i += 1;
    }
    Err("Thẻ HTML không đóng đúng cú pháp.")
}

pub(super) fn parse_attributes(
    slice: &str,
    post_render: bool,
) -> Result<BTreeMap<String, String>, &'static str> {
    let mut map = BTreeMap::new();
    let bytes = slice.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        // Skip Jinja control/expr tokens inside tag ONLY during pre-render template
        // validation. Post-render HTML must parse every token as raw HTML attributes
        // so `{{ href="javascript:..." }}` cannot smuggle attributes past C-SEC.
        if !post_render
            && bytes[i] == b'{'
            && i + 1 < bytes.len()
            && (bytes[i + 1] == b'%' || bytes[i + 1] == b'{')
        {
            let needle = if bytes[i + 1] == b'%' { "%}" } else { "}}" };
            if let Some(end) = slice[i + 2..].find(needle) {
                i += 2 + end + 2;
                continue;
            }
            return Err("Biểu thức template trong thẻ HTML không đóng.");
        }
        let name_start = i;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'='
            && bytes[i] != b'/'
        {
            if post_render && matches!(bytes[i], b'"' | b'\'' | b'<' | b'>' | b'`') {
                return Err("Tên thuộc tính HTML chứa ký tự không hợp lệ.");
            }
            i += 1;
        }
        let name = slice[name_start..i].to_ascii_lowercase();
        if post_render && name.is_empty() {
            return Err("Cú pháp thuộc tính HTML không hợp lệ.");
        }
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
                if i >= bytes.len() {
                    return Err("Giá trị thuộc tính HTML thiếu dấu đóng ngoặc.");
                }
                let v = slice[v_start..i].to_string();
                i += 1;
                v
            } else {
                let v_start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    if post_render && matches!(bytes[i], b'"' | b'\'' | b'<' | b'>' | b'=' | b'`') {
                        return Err(
                            "Giá trị thuộc tính HTML không có dấu nháy chứa ký tự không hợp lệ.",
                        );
                    }
                    i += 1;
                }
                slice[v_start..i].to_string()
            }
        } else {
            String::new()
        };
        if !name.is_empty() && map.insert(name, val).is_some() {
            return Err("Thẻ HTML chứa thuộc tính bị trùng lặp.");
        }
    }
    Ok(map)
}

pub(super) fn is_forbidden_block_shadow_name(id: &str) -> bool {
    matches!(
        id,
        "options"
            | "site_options"
            | "theme_options"
            | "menus"
            | "od_is_preview"
            | "od_icon"
            | "od_hook"
    )
}

pub(super) const BLOCK_HTML_STEP_OPTIONS: HtmlStepOptions = HtmlStepOptions {
    is_part: false,
    is_full_doctype_doc: false,
    allow_trusted_theme_markup: true,
    forbid_style_tag: true,
};
