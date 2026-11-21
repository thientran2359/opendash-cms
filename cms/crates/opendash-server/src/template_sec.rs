//! C-SEC MiniJinja 3.0.5 safe subset, AST allowlist, and contextual HTML/URL/CSS validator.

use std::collections::{BTreeMap, BTreeSet};

use argon2::password_hash::rand_core::{OsRng, RngCore};

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
const MAX_COMPOSITION_PART_DEPTH: usize = 3;
const MAX_COMPOSITION_EXPANSIONS: usize = 32;

/// The permitted number of content slots depends on the resolved template target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsecContentKind {
    /// Home, Page, PageLayout, Single* and Singular require exactly one slot.
    Entry,
    /// Index may omit the slot or include it once.
    Index,
    /// Archive*, Taxonomy*, NotFound and template parts cannot inject content.
    NonEntry,
}

/// Validated theme v3 template, flattened into a single MiniJinja program.
/// This ensures every part shares the same context, fuel and output budget.
#[derive(Debug, Clone)]
pub struct CompiledCsecComposition {
    source: String,
    name: String,
    content_count: usize,
}

impl CompiledCsecComposition {
    /// Renders the C-SEC shell before substituting sanitized entry HTML.
    ///
    /// The injected HTML was sanitized by the content renderer and can contain
    /// a contact form. The theme shell itself is checked with the strict
    /// template allowlist, including the ban on forms and scripts.
    pub fn render(
        &self,
        context: Value,
        sanitized_content_html: Option<&str>,
    ) -> Result<String, &'static str> {
        let nonce = if self.content_count == 1 {
            let mut bytes = [0_u8; 16];
            OsRng.fill_bytes(&mut bytes);
            let mut nonce = String::from("OD_CONTENT_");
            for byte in bytes {
                use std::fmt::Write;
                write!(&mut nonce, "{byte:02x}").map_err(|_| "Không tạo được nonce.")?;
            }
            Some(nonce)
        } else {
            None
        };

        let source = if let Some(nonce) = &nonce {
            self.source.replace(CONTENT_SLOT_TOKEN, nonce)
        } else {
            self.source.clone()
        };
        let mut env = restricted_environment();
        env.add_template(&self.name, &source)
            .map_err(|_| "Không thể biên dịch theme composition.")?;
        let shell = env
            .get_template(&self.name)
            .map_err(|_| "Không tìm thấy theme composition.")?
            .render(context)
            .map_err(|_| "Lỗi thực thi theme composition (fuel/recursion/undefined).")?;
        if shell.len() > MAX_RENDERED_HTML_BYTES {
            return Err("HTML render vượt quá giới hạn 512 KiB.");
        }
        validate_html_and_css_context(&shell, true)?;
        let Some(nonce) = nonce else {
            return Ok(shell);
        };
        if shell.matches(&nonce).count() != 1 {
            return Err("Nonce nội dung của theme không xuất hiện đúng một lần.");
        }
        let content = sanitized_content_html.unwrap_or("");
        let final_len = shell.len() - nonce.len() + content.len();
        if final_len > MAX_RENDERED_HTML_BYTES {
            return Err("HTML sau khi chèn nội dung vượt quá giới hạn 512 KiB.");
        }
        Ok(shell.replacen(&nonce, content, 1))
    }
}

// A compiler-only marker. It is never passed to MiniJinja as a callable or
// trusted HTML variable. A fresh 128-bit random value replaces it per render.
const CONTENT_SLOT_TOKEN: &str = "OD_CONTENT_SLOT_COMPILE_MARKER";

// Reserved only by the preview compiler, never a template variable or helper.
const PREVIEW_PART_NONCE_PREFIX: &str = "OD_PREVIEW_PART_";

/// One budget per private preview. Storage owns graph discovery and records
/// edge occurrences and the active canonical-key stack before rendering.
/// Every contiguous C-SEC segment debits the same remaining fuel.
#[derive(Debug)]
pub(crate) struct PreviewCompositionBudget {
    pub fuel_remaining: u64,
    pub expansions: usize,
    pub stack: Vec<String>,
}

impl Default for PreviewCompositionBudget {
    fn default() -> Self {
        Self {
            fuel_remaining: TEMPLATE_FUEL_LIMIT,
            expansions: 0,
            stack: Vec::new(),
        }
    }
}

impl PreviewCompositionBudget {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

/// Validated literal edges in source order, including repeated occurrences.
/// No execution is needed to discover the mixed preview graph.
pub(crate) fn preview_csec_part_keys(
    name: &str,
    source: &str,
) -> Result<Vec<String>, &'static str> {
    preview_composition_sites(name, source)?
        .into_iter()
        .map(|site| match site.call {
            CompositionCall::Part(key) => Ok(key),
            CompositionCall::Content => Err("Template part không được chứa od_content()."),
        })
        .collect()
}

fn preview_composition_sites(
    name: &str,
    source: &str,
) -> Result<Vec<CompositionSite>, &'static str> {
    if source.len() > MAX_RENDERED_HTML_BYTES {
        return Err("Template hoặc part vượt giới hạn 512 KiB.");
    }
    if source.contains(PREVIEW_PART_NONCE_PREFIX) {
        return Err("Theme HTML chứa marker nội bộ bị cấm.");
    }
    composition_sites(name, source, true)
}

/// Private preview bridge. Theme descendants share their parent's lexical
/// MiniJinja context. Only DB Builder children (or a server-owned missing-part
/// fallback) are supplied as already rendered, trusted HTML.
pub(crate) fn render_preview_csec_fragment(
    name: &str,
    source: &str,
    context: Value,
    theme_sources: &BTreeMap<String, String>,
    trusted_builder_parts: &BTreeMap<String, String>,
    budget: &mut PreviewCompositionBudget,
) -> Result<String, &'static str> {
    if budget.fuel_remaining == 0 || budget.fuel_remaining > TEMPLATE_FUEL_LIMIT {
        return Err("Fuel preview không hợp lệ hoặc đã hết.");
    }
    if budget.expansions > MAX_COMPOSITION_EXPANSIONS {
        return Err("od_part vượt quá 32 lần mở rộng.");
    }
    let mut compiler = PreviewFragmentCompiler {
        theme_sources,
        trusted_builder_parts,
        nonces: BTreeMap::new(),
    };
    let mut program = String::new();
    compiler.flatten(name, source, 0, &mut program)?;
    // Revalidate across source boundaries; the original source checks alone
    // cannot detect markup or syntax introduced by concatenation.
    validate_template_csec(name, &program)?;
    let mut env = restricted_environment();
    env.set_fuel(Some(budget.fuel_remaining));
    env.add_template(name, &program)
        .map_err(|_| "Không thể biên dịch theme preview.")?;
    let captured = env
        .get_template(name)
        .map_err(|_| "Không tìm thấy theme preview.")?
        .render_captured(context)
        .map_err(|_| "Lỗi thực thi theme preview (fuel/recursion/undefined).")?;
    let (consumed, remaining) = captured
        .state()
        .fuel_levels()
        .ok_or("Thiếu bộ theo dõi fuel preview.")?;
    let debited = budget
        .fuel_remaining
        .checked_sub(consumed)
        .ok_or("Fuel preview không hợp lệ.")?;
    if debited == 0 || debited != remaining {
        return Err("Fuel preview không hợp lệ hoặc đã hết.");
    }
    budget.fuel_remaining = debited;
    let shell = captured.output();
    if shell.len() > MAX_RENDERED_HTML_BYTES {
        return Err("HTML render vượt quá giới hạn 512 KiB.");
    }
    validate_html_and_css_context(shell, true)?;
    insert_preview_fragments(shell, &compiler.nonces)
}

struct PreviewFragmentCompiler<'a> {
    theme_sources: &'a BTreeMap<String, String>,
    trusted_builder_parts: &'a BTreeMap<String, String>,
    nonces: BTreeMap<String, &'a str>,
}

impl<'a> PreviewFragmentCompiler<'a> {
    fn flatten(
        &mut self,
        name: &str,
        source: &str,
        depth: usize,
        out: &mut String,
    ) -> Result<(), &'static str> {
        // The mixed graph is already checked by storage. This guard also
        // bounds recursion if an inconsistent segment is passed internally.
        if depth > MAX_COMPOSITION_PART_DEPTH {
            return Err("Độ sâu od_part vượt giới hạn 3.");
        }
        let sites = preview_composition_sites(name, source)?;
        let mut cursor = 0;
        for site in sites {
            append_preview_fragment(out, &source[cursor..site.start])?;
            let CompositionCall::Part(key) = site.call else {
                return Err("Template part không được chứa od_content().");
            };
            if depth >= MAX_COMPOSITION_PART_DEPTH {
                return Err("Độ sâu od_part vượt giới hạn 3.");
            }
            // Resolution follows the request-local DB-before-theme graph.
            if let Some(html) = self.trusted_builder_parts.get(&key) {
                if html.len() > MAX_RENDERED_HTML_BYTES {
                    return Err("HTML part vượt quá giới hạn 512 KiB.");
                }
                let mut random = [0_u8; 16];
                OsRng
                    .try_fill_bytes(&mut random)
                    .map_err(|_| "Không tạo được nonce preview.")?;
                let mut nonce = PREVIEW_PART_NONCE_PREFIX.to_owned();
                for byte in random {
                    use std::fmt::Write;
                    write!(&mut nonce, "{byte:02x}")
                        .map_err(|_| "Không tạo được nonce preview.")?;
                }
                if self.nonces.insert(nonce.clone(), html).is_some() {
                    return Err("Nonce preview bị trùng.");
                }
                append_preview_fragment(out, &nonce)?;
            } else {
                let child = self
                    .theme_sources
                    .get(&key)
                    .ok_or("od_part trỏ tới part không tồn tại.")?;
                // Substitute source at each occurrence, not independently
                // rendered HTML: set/with locals and conditional paths survive.
                self.flatten(&key, child, depth + 1, out)?;
            }
            cursor = site.end;
        }
        append_preview_fragment(out, &source[cursor..])
    }
}

fn append_preview_fragment(out: &mut String, fragment: &str) -> Result<(), &'static str> {
    if fragment.len() > MAX_RENDERED_HTML_BYTES.saturating_sub(out.len()) {
        return Err("HTML preview vượt quá giới hạn 512 KiB.");
    }
    out.push_str(fragment);
    Ok(())
}

fn insert_preview_fragments(
    shell: &str,
    nonces: &BTreeMap<String, &str>,
) -> Result<String, &'static str> {
    // Search the validated shell only. Never interpret marker-like text
    // inside inserted Builder HTML or accept a client-supplied marker.
    let mut substitutions = Vec::new();
    for (nonce, html) in nonces {
        let mut matches = shell.match_indices(nonce);
        if let Some((offset, _)) = matches.next() {
            if matches.next().is_some() {
                return Err("Nonce preview xuất hiện nhiều hơn một lần.");
            }
            substitutions.push((offset, nonce.len(), *html));
        }
    }
    substitutions.sort_unstable_by_key(|(offset, _, _)| *offset);
    let mut result = String::new();
    let mut cursor = 0;
    let mut state = HtmlScanState::Body;
    for (offset, len, html) in substitutions {
        let before = shell
            .get(cursor..offset)
            .ok_or("Vị trí nonce preview không hợp lệ.")?;
        state = before.bytes().fold(state, HtmlScanState::consume);
        if state != HtmlScanState::Body {
            return Err("Nonce preview không nằm trong thân HTML.");
        }
        append_preview_fragment(&mut result, before)?;
        append_preview_fragment(&mut result, html)?;
        cursor = offset + len;
    }
    append_preview_fragment(&mut result, &shell[cursor..])?;
    Ok(result)
}

#[derive(Debug, Clone)]
enum CompositionCall {
    Part(String),
    Content,
}

#[derive(Debug, Clone)]
struct CompositionSite {
    start: usize,
    end: usize,
    call: CompositionCall,
}

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
    env.add_test("defined", minijinja::tests::is_defined);
    env.add_test("undefined", minijinja::tests::is_undefined);
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
pub fn render_csec(name: &str, source: &str, context: Value) -> Result<String, &'static str> {
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

/// Checks theme v3 HTML and every declared part, then resolves the literal
/// `od_part("key")` DAG into one ordinary, C-SEC-approved MiniJinja source.
///
/// `parts` maps the literal part key to its HTML fragment, not a filesystem
/// path. No template loader, dynamic include, raw filter or function is enabled.
/// Depth counts part expansions from the root, so root -> a -> b -> c is 3.
pub fn validate_and_compile_csec_composition(
    root_name: &str,
    root_source: &str,
    parts: &BTreeMap<String, String>,
    kind: CsecContentKind,
) -> Result<CompiledCsecComposition, &'static str> {
    // Each resource is bounded independently. A theme can declare many safe
    // parts which are not all rendered in one request; the flattened reachable
    // program and rendered output have their own 512 KiB checks below.
    if root_source.len() > MAX_RENDERED_HTML_BYTES
        || parts
            .values()
            .any(|part| part.len() > MAX_RENDERED_HTML_BYTES)
    {
        return Err("Template hoặc part vượt giới hạn 512 KiB.");
    }
    let root_sites = composition_sites(root_name, root_source, false)?;
    let mut part_sites = BTreeMap::new();
    for (key, source) in parts {
        if !valid_part_key(key) {
            return Err("Part key không hợp lệ.");
        }
        part_sites.insert(key.clone(), composition_sites(key, source, true)?);
    }

    // Check all declared parts, even unreachable ones: a dormant cycle must
    // not become executable when a different template references that part.
    for key in parts.keys() {
        let mut visiting = BTreeSet::new();
        check_part_graph(key, &part_sites, &mut visiting, 1, &mut 0)?;
    }
    let mut expansions = 0_usize;
    let mut visiting = BTreeSet::new();
    let mut content_count = 0_usize;
    let compiled_source = flatten_composition(
        root_source,
        &root_sites,
        parts,
        &part_sites,
        &mut visiting,
        0,
        &mut expansions,
        &mut content_count,
    )?;

    match kind {
        CsecContentKind::Entry if content_count != 1 => {
            return Err("Template bản ghi phải có đúng một od_content().");
        }
        CsecContentKind::Index if content_count > 1 => {
            return Err("Index chỉ được có tối đa một od_content().");
        }
        CsecContentKind::NonEntry if content_count != 0 => {
            return Err("Template không phải bản ghi cấm od_content().");
        }
        _ => {}
    }
    if compiled_source.len() > MAX_RENDERED_HTML_BYTES {
        return Err("Template composition vượt giới hạn 512 KiB.");
    }
    // This last check catches dangerous markup introduced across part boundaries,
    // in addition to the independent source checks before expansion.
    validate_template_csec(root_name, &compiled_source)?;
    Ok(CompiledCsecComposition {
        source: compiled_source,
        name: root_name.to_owned(),
        content_count,
    })
}

fn valid_part_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && !key.contains("..")
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'/'))
}

fn check_part_graph(
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

#[allow(clippy::too_many_arguments)]
fn flatten_composition(
    source: &str,
    sites: &[CompositionSite],
    parts: &BTreeMap<String, String>,
    part_sites: &BTreeMap<String, Vec<CompositionSite>>,
    visiting: &mut BTreeSet<String>,
    depth: usize,
    expansions: &mut usize,
    content_count: &mut usize,
) -> Result<String, &'static str> {
    let mut out = String::new();
    let mut cursor = 0_usize;
    for site in sites {
        out.push_str(&source[cursor..site.start]);
        match &site.call {
            CompositionCall::Content => {
                *content_count += 1;
                out.push_str(CONTENT_SLOT_TOKEN);
            }
            CompositionCall::Part(key) => {
                *expansions += 1;
                if *expansions > MAX_COMPOSITION_EXPANSIONS {
                    return Err("od_part vượt quá 32 lần mở rộng.");
                }
                if depth >= MAX_COMPOSITION_PART_DEPTH {
                    return Err("Độ sâu od_part vượt giới hạn 3.");
                }
                if !visiting.insert(key.to_owned()) {
                    return Err("Phát hiện vòng lặp od_part.");
                }
                let part = parts
                    .get(key)
                    .ok_or("od_part trỏ tới part không tồn tại.")?;
                let child_sites = part_sites.get(key).ok_or("Thiếu part đã kiểm tra.")?;
                let child = flatten_composition(
                    part,
                    child_sites,
                    parts,
                    part_sites,
                    visiting,
                    depth + 1,
                    expansions,
                    content_count,
                )?;
                visiting.remove(key);
                out.push_str(&child);
            }
        }
        if out.len() > MAX_RENDERED_HTML_BYTES {
            return Err("Template composition vượt giới hạn 512 KiB.");
        }
        cursor = site.end;
    }
    out.push_str(&source[cursor..]);
    if out.len() > MAX_RENDERED_HTML_BYTES {
        return Err("Template composition vượt giới hạn 512 KiB.");
    }
    Ok(out)
}

fn composition_sites(
    name: &str,
    source: &str,
    is_part: bool,
) -> Result<Vec<CompositionSite>, &'static str> {
    if source.trim().is_empty() {
        return Err("Template không được để trống.");
    }
    if source.contains(CONTENT_SLOT_TOKEN) {
        return Err("Theme HTML chứa marker nội bộ bị cấm.");
    }
    validate_html_and_css_context(source, false)?;
    let ast = parse(source, name, SyntaxConfig::default())
        .map_err(|_| "Cú pháp theme MiniJinja không hợp lệ.")?;
    // Scan what each possible MiniJinja branch can actually emit. A linear
    // scan through the source can see a closing quote in the *other* branch
    // and mistake a real runtime attribute for an HTML body position.
    let final_contexts =
        ensure_composition_body_context(&ast, &BTreeSet::from([HtmlScanState::Body]))?;
    // A part is substituted into its caller as a raw HTML fragment. Even if
    // the caller's next od_content() is body-scoped, an unclosed attribute,
    // comment or style from one Jinja branch of the part would move the
    // injected content into an unsafe context. Require every possible part
    // execution path to return to the HTML body before composing it.
    if is_part
        && final_contexts
            .iter()
            .any(|state| *state != HtmlScanState::Body)
    {
        return Err("Template part phải kết thúc ngoài HTML tag/comment/style.");
    }
    let mut sites = Vec::new();
    collect_composition_sites(&ast, source, false, &mut sites)?;
    sites.sort_by_key(|site| site.start);
    let mut cursor = 0;
    for site in &sites {
        if site.start < cursor || site.end > source.len() {
            return Err("Vị trí od_part/od_content không hợp lệ.");
        }
        if is_part && matches!(site.call, CompositionCall::Content) {
            return Err("Template part không được chứa od_content().");
        }
        cursor = site.end;
    }
    Ok(sites)
}

fn collect_composition_sites(
    stmt: &Stmt<'_>,
    source: &str,
    in_for_or_set_block: bool,
    sites: &mut Vec<CompositionSite>,
) -> Result<(), &'static str> {
    match stmt {
        Stmt::Template(t) => {
            for child in &t.children {
                collect_composition_sites(child, source, in_for_or_set_block, sites)?;
            }
        }
        Stmt::EmitExpr(e) => {
            let Some(call) = parse_composition_call(&e.expr)? else {
                return Ok(());
            };
            if in_for_or_set_block {
                return Err("od_part/od_content không được nằm trong for hoặc set block.");
            }
            let start = e.span().start_offset as usize;
            let expr_end = e.span().end_offset as usize;
            if start >= source.len()
                || expr_end > source.len()
                || !source[start..].starts_with("{{")
            {
                return Err("od_part/od_content chỉ được xuất hiện ở phần thân HTML.");
            }
            // The AST span covers '{{' through the final expression token,
            // but not the closing '}}'. Search only after that token so a
            // quoted string containing '}}' cannot truncate the replacement.
            let suffix = source
                .get(expr_end..)
                .ok_or("Vị trí biểu thức composition không hợp lệ.")?;
            let close = suffix
                .find("}}")
                .ok_or("od_part/od_content thiếu dấu đóng.")?;
            if !suffix[..close]
                .chars()
                .all(|ch| ch.is_whitespace() || ch == '-')
            {
                return Err("Biểu thức composition chứa token ngoài allowlist.");
            }
            sites.push(CompositionSite {
                start,
                end: expr_end + close + 2,
                call,
            });
        }
        Stmt::ForLoop(f) => {
            validate_expr(&f.target)?;
            validate_expr(&f.iter)?;
            if let Some(expr) = &f.filter_expr {
                validate_expr(expr)?;
            }
            for child in f.body.iter().chain(&f.else_body) {
                collect_composition_sites(child, source, true, sites)?;
            }
        }
        Stmt::IfCond(i) => {
            validate_expr(&i.expr)?;
            for child in i.true_body.iter().chain(&i.false_body) {
                collect_composition_sites(child, source, in_for_or_set_block, sites)?;
            }
        }
        Stmt::WithBlock(w) => {
            for (key, val) in &w.assignments {
                validate_expr(key)?;
                validate_expr(val)?;
            }
            for child in &w.body {
                collect_composition_sites(child, source, in_for_or_set_block, sites)?;
            }
        }
        Stmt::SetBlock(sb) => {
            if sb.filter.is_some() {
                return Err("C-SEC cấm modifier filter trong khối set.");
            }
            validate_expr(&sb.target)?;
            for child in &sb.body {
                collect_composition_sites(child, source, true, sites)?;
            }
        }
        _ => validate_stmt(stmt)?,
    }
    Ok(())
}

fn parse_composition_call(expr: &Expr<'_>) -> Result<Option<CompositionCall>, &'static str> {
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

/// Bounded lexical HTML states for each possible Jinja control-flow path.
/// A Jinja branch cannot borrow a closing quote/tag from another branch.
/// The prefix is only used to recognize comments and style tags; it is
/// deliberately bounded so hostile templates cannot grow validation state.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum HtmlScanState {
    Body,
    Tag { quote: Option<u8>, prefix: String },
    Comment { trailing_hyphens: u8 },
    Style { closing_prefix: usize },
}

impl HtmlScanState {
    fn consume(self, byte: u8) -> Self {
        match self {
            Self::Body if byte == b'<' => Self::Tag {
                quote: None,
                prefix: "<".to_owned(),
            },
            Self::Body => Self::Body,
            Self::Comment { trailing_hyphens } => match byte {
                b'>' if trailing_hyphens == 2 => Self::Body,
                b'-' => Self::Comment {
                    trailing_hyphens: trailing_hyphens.saturating_add(1).min(2),
                },
                _ => Self::Comment {
                    trailing_hyphens: 0,
                },
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
            Self::Tag {
                mut quote,
                mut prefix,
            } => {
                if prefix.len() < 10 {
                    prefix.push(char::from(byte));
                }
                if prefix == "<!--" {
                    return Self::Comment {
                        trailing_hyphens: 0,
                    };
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
                    return Self::Body;
                }
                Self::Tag { quote, prefix }
            }
        }
    }
}

fn ensure_composition_body_context(
    stmt: &Stmt<'_>,
    states: &BTreeSet<HtmlScanState>,
) -> Result<BTreeSet<HtmlScanState>, &'static str> {
    match stmt {
        Stmt::Template(t) => ensure_composition_body_sequence(&t.children, states),
        Stmt::EmitRaw(raw) => {
            let mut result = BTreeSet::new();
            for initial in states {
                let final_state = raw
                    .raw
                    .bytes()
                    .fold(initial.clone(), HtmlScanState::consume);
                result.insert(final_state);
            }
            Ok(result)
        }
        Stmt::EmitExpr(expr) => {
            if parse_composition_call(&expr.expr)?.is_some()
                && states.iter().any(|state| *state != HtmlScanState::Body)
            {
                return Err("od_part/od_content chỉ được xuất hiện trong thân HTML.");
            }
            Ok(states.clone())
        }
        Stmt::IfCond(condition) => {
            let mut result = ensure_composition_body_sequence(&condition.true_body, states)?;
            result.extend(ensure_composition_body_sequence(
                &condition.false_body,
                states,
            )?);
            bounded_html_states(result)
        }
        Stmt::WithBlock(block) => ensure_composition_body_sequence(&block.body, states),
        Stmt::ForLoop(loop_stmt) => {
            // Zero, one and repeated iterations can leave different HTML
            // contexts for a following slot. Close over those finite states;
            // composition *within* the loop is separately rejected by AST.
            let mut possible = states.clone();
            let mut frontier = states.clone();
            let mut settled = false;
            for _ in 0..64 {
                let after = ensure_composition_body_sequence(&loop_stmt.body, &frontier)?;
                let new_states: BTreeSet<_> = after.difference(&possible).cloned().collect();
                if new_states.is_empty() {
                    settled = true;
                    break;
                }
                possible.extend(new_states.iter().cloned());
                bounded_html_states(possible.clone())?;
                frontier = new_states;
            }
            if !settled {
                return Err("Không thể xác định vị trí HTML sau vòng lặp.");
            }
            // The else body may follow the loop; over-approximate safely.
            let after_else = ensure_composition_body_sequence(&loop_stmt.else_body, &possible)?;
            possible.extend(after_else);
            bounded_html_states(possible)
        }
        // SetBlock captures text into a variable and does not emit the body
        // directly. Calls within it are prohibited by collect_composition_sites.
        Stmt::SetBlock(_) => Ok(states.clone()),
        _ => Ok(states.clone()),
    }
}

fn ensure_composition_body_sequence(
    children: &[Stmt<'_>],
    states: &BTreeSet<HtmlScanState>,
) -> Result<BTreeSet<HtmlScanState>, &'static str> {
    let mut result = states.clone();
    for child in children {
        result = ensure_composition_body_context(child, &result)?;
        bounded_html_states(result.clone())?;
    }
    Ok(result)
}

fn bounded_html_states(
    states: BTreeSet<HtmlScanState>,
) -> Result<BTreeSet<HtmlScanState>, &'static str> {
    if states.len() > 64 {
        return Err("Quá nhiều trạng thái HTML tại điểm composition.");
    }
    Ok(states)
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
            .or_else(|| {
                raw_inner
                    .strip_prefix('\'')
                    .and_then(|s| s.strip_suffix('\''))
            })
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
        if bytes[i] == b'{' && i + 1 < bytes.len() && (bytes[i + 1] == b'%' || bytes[i + 1] == b'{')
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

#[cfg(test)]
mod composition_tests {
    use super::*;

    fn parts(items: &[(&str, &str)]) -> BTreeMap<String, String> {
        items
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect()
    }

    fn compile(
        source: &str,
        parts: &BTreeMap<String, String>,
        kind: CsecContentKind,
    ) -> Result<CompiledCsecComposition, &'static str> {
        validate_and_compile_csec_composition("root", source, parts, kind)
    }

    #[test]
    fn content_slot_rules_for_entry_index_and_non_entry() {
        let empty = parts(&[]);
        assert!(compile("<main></main>", &empty, CsecContentKind::Entry).is_err());
        assert!(
            compile(
                "<main>{{ od_content() }}</main>",
                &empty,
                CsecContentKind::Entry
            )
            .is_ok()
        );
        assert!(
            compile(
                "<main>{{ od_content() }}{{ od_content() }}</main>",
                &empty,
                CsecContentKind::Entry
            )
            .is_err()
        );
        assert!(compile("<main></main>", &empty, CsecContentKind::Index).is_ok());
        assert!(
            compile(
                "<main>{{ od_content() }}</main>",
                &empty,
                CsecContentKind::Index
            )
            .is_ok()
        );
        assert!(
            compile(
                "<main>{{ od_content() }}{{ od_content() }}</main>",
                &empty,
                CsecContentKind::Index
            )
            .is_err()
        );
        assert!(compile("<main></main>", &empty, CsecContentKind::NonEntry).is_ok());
        assert!(
            compile(
                "<main>{{ od_content() }}</main>",
                &empty,
                CsecContentKind::NonEntry
            )
            .is_err()
        );
        assert!(
            compile(
                "<div>{{ od_part('header') }}</div>",
                &parts(&[("header", "<b>{{ od_content() }}</b>")]),
                CsecContentKind::Entry
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_cycles_and_unreferenced_cycles() {
        let self_ref = parts(&[("a", "<div>{{ od_part('a') }}</div>")]);
        assert!(
            compile(
                "<main>{{ od_part('a') }}</main>",
                &self_ref,
                CsecContentKind::NonEntry
            )
            .is_err()
        );
        let indirect = parts(&[("a", "{{ od_part('b') }}"), ("b", "{{ od_part('a') }}")]);
        assert!(compile("<main></main>", &indirect, CsecContentKind::NonEntry).is_err());
    }

    #[test]
    fn rejects_missing_part_and_excess_depth() {
        assert!(
            compile(
                "<main>{{ od_part('missing') }}</main>",
                &parts(&[]),
                CsecContentKind::NonEntry
            )
            .is_err()
        );
        let valid = parts(&[
            ("a", "{{ od_part('b') }}"),
            ("b", "{{ od_part('c') }}"),
            ("c", "<b>end</b>"),
        ]);
        assert!(
            compile(
                "<main>{{ od_part('a') }}</main>",
                &valid,
                CsecContentKind::NonEntry
            )
            .is_ok()
        );
        let too_deep = parts(&[
            ("a", "{{ od_part('b') }}"),
            ("b", "{{ od_part('c') }}"),
            ("c", "{{ od_part('d') }}"),
            ("d", "end"),
        ]);
        assert!(
            compile(
                "<main>{{ od_part('a') }}</main>",
                &too_deep,
                CsecContentKind::NonEntry
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_more_than_32_expansions_even_with_shared_subgraphs() {
        let repeat = "{{ od_part('leaf') }}".repeat(33);
        let nodes = parts(&[("leaf", "<span>safe</span>")]);
        assert!(
            compile(
                &format!("<main>{repeat}</main>"),
                &nodes,
                CsecContentKind::NonEntry
            )
            .is_err()
        );
        assert!(
            compile(
                &format!("<main>{}</main>", "{{ od_part('leaf') }}".repeat(32)),
                &nodes,
                CsecContentKind::NonEntry
            )
            .is_ok()
        );
        let repeated = parts(&[
            ("a", "{{ od_part('leaf') }}{{ od_part('leaf') }}"),
            ("leaf", "<b>x</b>"),
        ]);
        assert!(
            compile(
                &format!("<main>{}</main>", "{{ od_part('a') }}".repeat(11)),
                &repeated,
                CsecContentKind::NonEntry
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_composition_in_loop_attribute_comment_style_and_set_block() {
        let nodes = parts(&[("header", "<b>OK</b>")]);
        let bad = [
            "<main>{% for n in [1,2] %}{{ od_part('header') }}{% endfor %}</main>",
            "<main>{% for n in [1,2] %}{{ od_content() }}{% endfor %}</main>",
            "<a href=\"{{ od_part('header') }}\">x</a>",
            "<div data-test=\"{{ od_content() }}\"></div>",
            "<!-- {{ od_part('header') }} -->",
            "<style>.x{color:{{ od_content() }}}</style>",
            "{% set value %}{{ od_part('header') }}{% endset %}",
        ];
        for source in bad {
            assert!(
                compile(source, &nodes, CsecContentKind::Index).is_err(),
                "accepted unsafe {source:?}"
            );
        }
    }

    #[test]
    fn rejects_branch_split_attribute_and_comment_context_for_all_runtime_paths() {
        let nodes = parts(&[("header", "<b>Header</b>")]);
        let bad = [
            // Previously the false branch's quote + '>' closed the tag in a
            // linear source scan, while the true branch emitted an OPEN id
            // attribute with od_content as its value.
            "{% if cond %}<div id=\"{% else %}\">{% endif %}{{ od_content() }}\">",
            "{% if cond %}<div id=\"{% else %}\">{% endif %}{{ od_part('header') }}\">",
            "{% if outer %}{% if inner %}<div data-x=\"{% else %}\">{% endif %}{% else %}<section>{% endif %}{{ od_content() }}\">",
            "{% if cond %}<!--{% else %}-->{% endif %}{{ od_content() }}-->",
            "{% if cond %}<div id=\"{% else %}\">{% endif %}{% if cond %}{{ od_content() }}{% else %}ok{% endif %}\">",
            "{% for n in [1] %}<div id=\"{% else %}\">{% endfor %}{{ od_content() }}\">",
        ];
        for source in bad {
            assert!(
                compile(source, &nodes, CsecContentKind::Index).is_err(),
                "branch-dependent HTML context accepted: {source:?}"
            );
        }
    }

    #[test]
    fn rejects_part_that_leaves_attribute_open_in_only_one_branch() {
        let incomplete = parts(&[("header", "{% if cond %}<div id=\"{% else %}\">{% endif %}")]);
        assert!(
            compile(
                "<main>{{ od_part('header') }}{{ od_content() }}</main>",
                &incomplete,
                CsecContentKind::Entry
            )
            .is_err(),
            "part must not transfer an unclosed attribute context into od_content"
        );
    }

    #[test]
    fn allows_safe_conditional_body_markup_and_repeated_closed_tags() {
        let nodes = parts(&[("header", "<h1>{{ title }}</h1>")]);
        let source = "<main>{% if cond %}<section>Yes</section>{% else %}<aside>No</aside>{% endif %}{% for n in [1,2] %}<span>{{ n }}</span>{% endfor %}{{ od_part('header') }}{{ od_content() }}</main>";
        let template = compile(source, &nodes, CsecContentKind::Entry).unwrap();
        let form = "<form method=\"post\" action=\"/contact\"><input name=\"email\"></form>";
        let yes = template
            .render(
                minijinja::context!(cond => true, title => "Hello"),
                Some(form),
            )
            .unwrap();
        let no = template
            .render(
                minijinja::context!(cond => false, title => "Hello"),
                Some(form),
            )
            .unwrap();
        assert!(yes.contains("<section>Yes</section>"));
        assert!(no.contains("<aside>No</aside>"));
        for html in [yes, no] {
            assert!(html.contains("<span>1</span><span>2</span>"));
            assert!(html.contains("<h1>Hello</h1>"));
            assert!(html.contains(form));
        }
    }

    #[test]
    fn rejects_nonliteral_nested_or_unsafe_jinja_calls() {
        let nodes = parts(&[("header", "<b>OK</b>")]);
        let bad = [
            "{{ od_part(part_name) }}",
            "{{ od_part('header') | upper }}",
            "{{ od_part('header', 2) }}",
            "{{ od_content('x') }}",
            "{% set x = od_part('header') %}{{ x }}",
            "{{ arbitrary_function('x') }}",
            "{% include 'header' %}",
            "{{ '<script>alert(1)</script>' | safe }}",
        ];
        for source in bad {
            assert!(
                compile(source, &nodes, CsecContentKind::Index).is_err(),
                "accepted unsafe {source:?}"
            );
        }
    }

    #[test]
    fn expanded_parts_share_parent_context_and_fuel() {
        let nodes = parts(&[("header", "<h1>{{ title | upper }}</h1>")]);
        let tpl = compile(
            "<main>{% set title = 'ok' %}{{ od_part('header') }}</main>",
            &nodes,
            CsecContentKind::NonEntry,
        )
        .unwrap();
        assert_eq!(
            tpl.render(Value::UNDEFINED, None).unwrap(),
            "<main><h1>OK</h1></main>"
        );
        let malicious = parts(&[("header", "<script>alert(1)</script>")]);
        assert!(
            compile(
                "<main>{{ od_part('header') }}</main>",
                &malicious,
                CsecContentKind::NonEntry
            )
            .is_err()
        );
    }

    #[test]
    fn nonce_injection_allows_sanitized_contact_form_but_not_theme_form() {
        let empty = parts(&[]);
        let tpl = compile(
            "<main><article>{{ od_content() }}</article></main>",
            &empty,
            CsecContentKind::Entry,
        )
        .unwrap();
        let form = "<form method=\"post\" action=\"/contact\"><input name=\"email\"></form>";
        assert_eq!(
            tpl.render(Value::UNDEFINED, Some(form)).unwrap(),
            format!("<main><article>{form}</article></main>")
        );
        assert!(
            compile(
                "<main><form>{{ od_content() }}</form></main>",
                &empty,
                CsecContentKind::Entry
            )
            .is_err()
        );
        assert!(
            compile(
                "<main><script>{{ od_content() }}</script></main>",
                &empty,
                CsecContentKind::Entry
            )
            .is_err()
        );
    }

    #[test]
    fn output_and_input_limits_include_injected_content_and_parts() {
        let empty = parts(&[]);
        let tpl = compile(
            "<main>{{ od_content() }}</main>",
            &empty,
            CsecContentKind::Entry,
        )
        .unwrap();
        assert!(
            tpl.render(Value::UNDEFINED, Some(&"X".repeat(MAX_RENDERED_HTML_BYTES)))
                .is_err()
        );
        let huge = parts(&[("header", &"x".repeat(MAX_RENDERED_HTML_BYTES + 1))]);
        assert!(compile("<main></main>", &huge, CsecContentKind::NonEntry).is_err());
        let long = "x".repeat(300_000);
        let nodes = parts(&[("large", &long)]);
        assert!(
            compile(
                "<main>{{ od_part('large') }}{{ od_part('large') }}</main>",
                &nodes,
                CsecContentKind::NonEntry
            )
            .is_err()
        );
    }

    #[test]
    fn independent_unused_parts_do_not_consume_render_output_budget() {
        let sources = (0..5)
            .map(|n| (format!("unused-{n}"), "x".repeat(115_000)))
            .collect();
        let template = compile("<main>safe</main>", &sources, CsecContentKind::NonEntry).unwrap();
        assert_eq!(
            template.render(Value::UNDEFINED, None).unwrap(),
            "<main>safe</main>"
        );
    }

    #[test]
    fn preview_theme_children_share_lexical_parent_and_conditional_execution() {
        let theme = parts(&[
            (
                "heading",
                "<h1>{{ title | upper }}</h1>{{ od_part('card') }}",
            ),
            ("unused", "{{ undefined_value }}"),
            ("card", "<p>theme card must lose to DB</p>"),
        ]);
        let trusted = parts(&[("card", "<article>DB card</article>")]);
        let root = "{% set title = 'outer' %}{{ od_part('heading') }}\
                    {% with title = 'inner' %}{{ od_part('heading') }}{% endwith %}\
                    {% if false %}{{ od_part('unused') }}{% endif %}\
                    {{ od_part('heading') }}";
        assert_eq!(
            preview_csec_part_keys("root", root).unwrap(),
            ["heading", "heading", "unused", "heading"]
        );
        let mut budget = PreviewCompositionBudget::new();
        budget.expansions = 7;
        let html = render_preview_csec_fragment(
            "root",
            root,
            Value::UNDEFINED,
            &theme,
            &trusted,
            &mut budget,
        )
        .unwrap();
        assert_eq!(
            html,
            "<h1>OUTER</h1><article>DB card</article>\
             <h1>INNER</h1><article>DB card</article>\
             <h1>OUTER</h1><article>DB card</article>"
        );
        assert!(budget.fuel_remaining < TEMPLATE_FUEL_LIMIT);
        assert_eq!(budget.expansions, 7);
        assert!(budget.stack.is_empty());
    }

    #[test]
    fn preview_fuel_is_debited_across_independent_theme_segments() {
        let empty = parts(&[]);
        let root = "{% for item in items %}{{ item }}{% endfor %}";
        // Each iteration consumes several VM instructions. Find a fixed
        // fixture that consumes more than half of the request's total fuel
        // while remaining valid in a fresh budget.
        let context = minijinja::context!(items => vec![1_u32; 1500]);
        let mut budget = PreviewCompositionBudget::new();
        let first = render_preview_csec_fragment(
            "first",
            root,
            context.clone(),
            &empty,
            &empty,
            &mut budget,
        )
        .unwrap();
        assert_eq!(first.len(), 1500);
        assert!(budget.fuel_remaining < TEMPLATE_FUEL_LIMIT / 2);
        assert!(
            render_preview_csec_fragment(
                "second",
                root,
                context.clone(),
                &empty,
                &empty,
                &mut budget,
            )
            .is_err()
        );
        assert!(
            render_preview_csec_fragment(
                "second",
                root,
                context,
                &empty,
                &empty,
                &mut PreviewCompositionBudget::new(),
            )
            .is_ok()
        );
        budget.fuel_remaining = 0;
        assert!(
            render_preview_csec_fragment(
                "exhausted",
                "safe",
                Value::UNDEFINED,
                &empty,
                &empty,
                &mut budget,
            )
            .is_err()
        );
    }

    #[test]
    fn preview_only_inserts_trusted_parts_in_executed_body_sites() {
        let empty = parts(&[]);
        let trusted = parts(&[("card", "<form><input disabled></form>")]);
        let source = "<main>{{ client_text }}{% if show %}{{ od_part('card') }}{% endif %}</main>";
        let context = minijinja::context!(
            client_text => "<form>OD_PREVIEW_PART_fake</form>",
            show => true
        );
        let html = render_preview_csec_fragment(
            "root",
            source,
            context,
            &empty,
            &trusted,
            &mut PreviewCompositionBudget::new(),
        )
        .unwrap();
        assert!(
            html.contains("&lt;form&gt;OD_PREVIEW_PART_fake&lt;&#x2f;form&gt;")
                || html.contains("&lt;form&gt;OD_PREVIEW_PART_fake&lt;/form&gt;")
        );
        assert!(html.contains("<form><input disabled></form>"));
        let hidden = render_preview_csec_fragment(
            "root",
            source,
            minijinja::context!(client_text => "", show => false),
            &empty,
            &trusted,
            &mut PreviewCompositionBudget::new(),
        )
        .unwrap();
        assert_eq!(hidden, "<main></main>");
        for bad in [
            "<div data-x=\"{{ od_part('card') }}\"></div>",
            "<style>.x{color:{{ od_part('card') }}}</style>",
            "<form>{{ od_part('card') }}</form>",
            "<script>{{ od_part('card') }}</script>",
            "OD_PREVIEW_PART_abcdef",
            "{{ od_content() }}",
        ] {
            assert!(preview_csec_part_keys("root", bad).is_err(), "{bad}");
            assert!(
                render_preview_csec_fragment(
                    "root",
                    bad,
                    Value::UNDEFINED,
                    &empty,
                    &trusted,
                    &mut PreviewCompositionBudget::new(),
                )
                .is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn preview_nonce_replacement_is_single_pass_and_checks_multiplicity_context_and_size() {
        let nonce_table = BTreeMap::from([
            ("marker-first".to_owned(), "<b>marker-second</b>"),
            ("marker-second".to_owned(), "<i>second</i>"),
        ]);
        assert_eq!(
            insert_preview_fragments("marker-first marker-second", &nonce_table).unwrap(),
            "<b>marker-second</b> <i>second</i>"
        );
        assert!(insert_preview_fragments("marker-first marker-first", &nonce_table).is_err());
        assert!(insert_preview_fragments("<div id=\"marker-first\"></div>", &nonce_table).is_err());
        let empty = parts(&[]);
        let large = "x".repeat(MAX_RENDERED_HTML_BYTES / 2 + 1);
        let trusted = parts(&[("card", &large)]);
        assert!(
            render_preview_csec_fragment(
                "root",
                "{{ od_part('card') }}{{ od_part('card') }}",
                Value::UNDEFINED,
                &empty,
                &trusted,
                &mut PreviewCompositionBudget::new(),
            )
            .is_err()
        );
    }
}
