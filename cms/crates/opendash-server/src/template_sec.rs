//! C-SEC MiniJinja 3.0.5 safe subset, AST allowlist, and contextual HTML/URL/CSS validator.

use std::collections::BTreeMap;

use minijinja::{
    AutoEscape, Environment, UndefinedBehavior, Value,
    machinery::{
        ast::{Expr, Stmt},
        parse,
    },
    syntax::SyntaxConfig,
};

pub const MAX_RENDERED_HTML_BYTES: usize = 512 * 1024;
pub const TEMPLATE_FUEL_LIMIT: u64 = 10_000;
pub const TEMPLATE_RECURSION_LIMIT: usize = 16;

const FORBIDDEN_TAGS: &[&str] = &[
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
];

const ALLOWED_DYNAMIC_URL_VARS: &[&str] = &[
    "entry.url",
    "entry.cover_url",
    "field.image_url",
    "url",
    "item.url",
    "previous_url",
    "next_url",
    "archive_url",
    "theme_asset_url",
];

/// Constructs the restricted `Environment::empty()` with unconditional HTML auto-escaping,
/// strict undefined behavior, fuel budget, and recursion limit.
const ALLOWED_FILTERS: &[&str] = &[
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
];

pub fn restricted_environment() -> Environment<'static> {
    let mut env = Environment::empty();
    env.set_auto_escape_callback(|_| AutoEscape::Html);
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_fuel(Some(TEMPLATE_FUEL_LIMIT));
    env.set_recursion_limit(TEMPLATE_RECURSION_LIMIT);
    env.add_filter("default", minijinja::filters::default);
    env.add_filter("d", minijinja::filters::default);
    env.add_filter("trim", minijinja::filters::trim);
    env.add_filter("lower", minijinja::filters::lower);
    env.add_filter("upper", minijinja::filters::upper);
    env.add_filter("length", minijinja::filters::length);
    env.add_filter("first", minijinja::filters::first);
    env.add_filter("last", minijinja::filters::last);
    env.add_filter("join", minijinja::filters::join);
    env.add_filter("replace", minijinja::filters::replace);
    env.add_filter("int", minijinja::filters::int);
    env.add_filter("float", minijinja::filters::float);
    env.add_filter("string", minijinja::filters::string);
    env.add_filter("list", minijinja::filters::list);
    env.add_filter("abs", minijinja::filters::abs);
    env.add_filter("round", minijinja::filters::round);
    env.add_filter("sort", minijinja::filters::sort);
    env.add_filter("reverse", minijinja::filters::reverse);
    env.add_filter("unique", minijinja::filters::unique);
    env.add_filter("title", minijinja::filters::title);
    env.add_filter("capitalize", minijinja::filters::capitalize);
    env
}

/// Validates a template against the C-SEC MiniJinja AST allowlist and contextual HTML/URL/CSS rules.
pub fn validate_template_csec(name: &str, source: &str) -> Result<(), &'static str> {
    if source.trim().is_empty() {
        return Err("Template không được để trống.");
    }
    let ast = parse(source, name, SyntaxConfig::default())
    .map_err(|_| "Cú pháp template MiniJinja không hợp lệ.")?;

    validate_stmt(&ast)?;
    validate_html_and_css_context(source, false)?;

    let mut env = restricted_environment();
    env.add_template(name, source)
        .map_err(|_| "Không thể biên dịch template trong môi trường an toàn.")?;
    Ok(())
}

/// Validates and renders a template under C-SEC limits (fuel, recursion, 512 KiB output cap,
/// and post-render URL/HTML boundary verification).
pub fn render_csec(
    name: &str,
    source: &str,
    context: Value,
) -> Result<String, &'static str> {
    validate_template_csec(name, source)?;
    let mut env = restricted_environment();
    env.add_template(name, source)
        .map_err(|_| "Không thể biên dịch template.")?;
    let tpl = env
        .get_template(name)
        .map_err(|_| "Không tìm thấy template đã biên dịch.")?;
    let rendered = tpl
        .render(context)
        .map_err(|_| "Lỗi thực thi template (fuel/recursion/strict undefined).")?;
    if rendered.len() > MAX_RENDERED_HTML_BYTES {
        return Err("HTML render vượt quá giới hạn 512 KiB.");
    }
    validate_html_and_css_context(&rendered, true)?;
    Ok(rendered)
}

/// Validates a CSS stylesheet (both `<style>` blocks and `/theme/*.css` files).
pub fn validate_safe_css(css: &str) -> Result<(), &'static str> {
    let lower = css.to_ascii_lowercase();
    if lower.contains("</style")
        || lower.contains("@import")
        || lower.contains("expression(")
        || lower.contains("-moz-binding")
        || lower.contains("behavior:")
    {
        return Err("CSS vi phạm chính sách C-SEC (chứa @import/expression/binding/breakout).");
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
            .or_else(|| raw_inner.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
            .unwrap_or(raw_inner)
            .trim();
        if !(inner.starts_with("/theme/") || inner.starts_with("/media/")) || inner.contains("..") {
            return Err("CSS url(...) chỉ được phép trỏ tới /theme/... hoặc /media/....");
        }
        idx = start + end_rel + 1;
    }
    Ok(())
}

/// Validates a static or post-render URL used in `href`, `src`, `poster`, or `cite`.
fn decode_html_entities_for_url(raw: &str) -> String {
    raw.replace("&#x2f;", "/")
        .replace("&#x2F;", "/")
        .replace("&#47;", "/")
        .replace("&#x3a;", ":")
        .replace("&#x3A;", ":")
        .replace("&#58;", ":")
        .replace("&amp;", "&")
}

pub fn validate_safe_url(raw: &str) -> Result<(), &'static str> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.bytes().any(|b| b < 0x20 || b == 0x7f)
        || trimmed.contains('\\')
    {
        return Err("URL trong template không hợp lệ.");
    }
    if trimmed.starts_with('/') {
        if trimmed.starts_with("//") || trimmed.contains("/../") || trimmed.ends_with("/..") {
            return Err("URL tương đối không được chứa // hoặc ..");
        }
        return Ok(());
    }
    let parsed = url::Url::parse(trimmed).map_err(|_| "URL không hợp lệ.")?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Chỉ cho phép URL http(s) hoặc đường dẫn /...");
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("URL không được chứa thông tin đăng nhập.");
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

fn validate_stmt(stmt: &Stmt<'_>) -> Result<(), &'static str> {
    match stmt {
        Stmt::Template(t) => {
            for child in &t.children {
                validate_stmt(child)?;
            }
            Ok(())
        }
        Stmt::EmitRaw(_) => Ok(()),
        Stmt::EmitExpr(e) => validate_expr(&e.expr),
        Stmt::ForLoop(f) => {
            validate_expr(&f.target)?;
            validate_expr(&f.iter)?;
            if let Some(cond) = &f.filter_expr {
                validate_expr(cond)?;
            }
            for child in &f.body {
                validate_stmt(child)?;
            }
            for child in &f.else_body {
                validate_stmt(child)?;
            }
            Ok(())
        }
        Stmt::IfCond(i) => {
            validate_expr(&i.expr)?;
            for child in &i.true_body {
                validate_stmt(child)?;
            }
            for child in &i.false_body {
                validate_stmt(child)?;
            }
            Ok(())
        }
        Stmt::WithBlock(w) => {
            for (k, v) in &w.assignments {
                validate_expr(k)?;
                validate_expr(v)?;
            }
            for child in &w.body {
                validate_stmt(child)?;
            }
            Ok(())
        }
        Stmt::Set(s) => {
            validate_expr(&s.target)?;
            validate_expr(&s.expr)
        }
        Stmt::SetBlock(sb) => {
            if sb.filter.is_some() {
                return Err("C-SEC cấm modifier filter trong khối set.");
            }
            validate_expr(&sb.target)?;
            for child in &sb.body {
                validate_stmt(child)?;
            }
            Ok(())
        }
        Stmt::AutoEscape(_)
        | Stmt::FilterBlock(_)
        | Stmt::Macro(_)
        | Stmt::CallBlock(_)
        | Stmt::Import(_)
        | Stmt::FromImport(_)
        | Stmt::Extends(_)
        | Stmt::Include(_)
        | Stmt::Block(_)
        | Stmt::Do(_) => Err("Template sử dụng câu lệnh MiniJinja bị C-SEC cấm."),
    }
}

fn validate_expr(expr: &Expr<'_>) -> Result<(), &'static str> {
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
            validate_expr(&t.expr)?;
            for arg in &t.args {
                validate_call_arg(arg)?;
            }
            Ok(())
        }
        Expr::Call(_) => Err("C-SEC cấm lời gọi hàm Call trong template."),
    }
}

fn validate_call_arg(arg: &minijinja::machinery::ast::CallArg<'_>) -> Result<(), &'static str> {
    match arg {
        minijinja::machinery::ast::CallArg::Pos(e)
        | minijinja::machinery::ast::CallArg::Kwarg(_, e)
        | minijinja::machinery::ast::CallArg::PosSplat(e)
        | minijinja::machinery::ast::CallArg::KwargSplat(e) => validate_expr(e),
    }
}

fn validate_html_and_css_context(src: &str, post_render: bool) -> Result<(), &'static str> {
    let lower = src.to_ascii_lowercase();

    // Validate <style>...</style> blocks
    let mut style_search = 0;
    while let Some(open_pos) = lower[style_search..].find("<style") {
        let tag_start = style_search + open_pos;
        let Some(gt_rel) = lower[tag_start..].find('>') else {
            return Err("Thẻ <style> không đóng đúng cú pháp.");
        };
        let body_start = tag_start + gt_rel + 1;
        let Some(close_rel) = lower[body_start..].find("</style>") else {
            return Err("Thẻ <style> thiếu </style>.");
        };
        let css_body = &src[body_start..body_start + close_rel];
        if !post_render && (css_body.contains("{{") || css_body.contains("{%")) {
            return Err("C-SEC cấm nội suy {{ ... }} bên trong <style>.");
        }
        validate_safe_css(css_body)?;
        style_search = body_start + close_rel + 8;
    }

    // Scan HTML tags and attributes
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Skip doctype or comments
            if lower[i..].starts_with("<!--") {
                if let Some(end) = lower[i + 4..].find("-->") {
                    i += 4 + end + 3;
                    continue;
                }
                return Err("HTML comment không đóng.");
            }
            if lower[i..].starts_with("<!doctype") {
                if let Some(end) = bytes[i..].iter().position(|&b| b == b'>') {
                    i += end + 1;
                    continue;
                }
                return Err("DOCTYPE không đóng.");
            }

            let mut j = i + 1;
            if j < bytes.len() && bytes[j] == b'/' {
                j += 1;
            }
            let name_start = j;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'-') {
                j += 1;
            }
            let tag_name = lower[name_start..j].to_string();
            if FORBIDDEN_TAGS.contains(&tag_name.as_str()) {
                return Err("Template chứa thẻ HTML bị C-SEC cấm.");
            }

            // Find end of tag (respecting quotes)
            let mut k = j;
            let mut quote: Option<u8> = None;
            while k < bytes.len() {
                let b = bytes[k];
                if let Some(q) = quote {
                    if b == q {
                        quote = None;
                    }
                } else if b == b'"' || b == b'\'' {
                    quote = Some(b);
                } else if b == b'>' {
                    break;
                }
                k += 1;
            }
            if k >= bytes.len() {
                // Could be `<` inside template text? Only if tag_name is non-empty!
                if !tag_name.is_empty() {
                    return Err("Thẻ HTML không đóng đúng cú pháp.");
                }
                i += 1;
                continue;
            }

            let attr_slice = &src[j..k];
            validate_tag_attributes(&tag_name, attr_slice, post_render)?;
            i = k + 1;
        } else {
            i += 1;
        }
    }

    Ok(())
}

fn validate_tag_attributes(
    tag_name: &str,
    attr_slice: &str,
    post_render: bool,
) -> Result<(), &'static str> {
    let attrs = parse_attributes(attr_slice)?;

    if tag_name == "meta" {
        let allowed_meta = attrs.contains_key("charset")
            || attrs
                .get("name")
                .is_some_and(|v| v.eq_ignore_ascii_case("viewport"));
        if !allowed_meta {
            return Err("C-SEC chỉ cho phép <meta charset> hoặc <meta name=\"viewport\">.");
        }
    }
    if tag_name == "link" {
        let rel_ok = attrs
            .get("rel")
            .is_some_and(|v| v.eq_ignore_ascii_case("stylesheet"));
        let href_ok = attrs
            .get("href")
            .is_some_and(|v| v.starts_with("/theme/") && !v.contains("{{") && !v.contains(".."));
        if !(rel_ok && href_ok) {
            return Err("C-SEC chỉ cho phép <link rel=\"stylesheet\" href=\"/theme/...\">.");
        }
    }

    for (name, val) in attrs {
        if name.starts_with("on")
            || matches!(
                name.as_str(),
                "srcdoc" | "formaction" | "action" | "xmlns" | "xlink:href"
            )
        {
            return Err("Template chứa thuộc tính HTML bị C-SEC cấm.");
        }

        if name == "style" {
            if !post_render && (val.contains("{{") || val.contains("{%")) {
                return Err("C-SEC cấm nội suy {{ ... }} bên trong thuộc tính style.");
            }
            validate_safe_css(&val)?;
        }

        if matches!(name.as_str(), "href" | "src" | "poster" | "cite") {
            if !post_render && (val.contains("{{") || val.contains("{%")) {
                let expr_inner = val
                    .trim()
                    .strip_prefix("{{")
                    .and_then(|s| s.strip_suffix("}}"))
                    .map(str::trim)
                    .ok_or("URL động trong thuộc tính chỉ được phép là biểu thức {{ ... }} đơn.")?;
                if !ALLOWED_DYNAMIC_URL_VARS.contains(&expr_inner) {
                    return Err("URL động trong thuộc tính không nằm trong whitelist typed URL.");
                }
            } else if post_render {
                validate_safe_url(&decode_html_entities_for_url(&val))?;
            } else {
                validate_safe_url(&val)?;
            }
        }
    }

    Ok(())
}

fn parse_attributes(slice: &str) -> Result<BTreeMap<String, String>, &'static str> {
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
        // Skip Jinja control/expr tokens inside tag if any (e.g. {% if ... %})
        if bytes[i] == b'{' && i + 1 < bytes.len() && (bytes[i + 1] == b'%' || bytes[i + 1] == b'{') {
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
            i += 1;
        }
        let name = slice[name_start..i].to_ascii_lowercase();
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
                    i += 1;
                }
                slice[v_start..i].to_string()
            }
        } else {
            String::new()
        };
        if !name.is_empty() {
            map.insert(name, val);
        }
    }
    Ok(map)
}
