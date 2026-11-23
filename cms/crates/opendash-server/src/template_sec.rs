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

/// Explicit, manifest-derived theme anchor permissions. The private set only
/// contains exact `options.<key>` expressions declared as public URL options.
#[derive(Debug, Clone, Default)]
pub struct ThemeUrlPolicy {
    permitted_href_expressions: BTreeSet<String>,
}

impl ThemeUrlPolicy {
    pub(crate) fn from_option_keys(keys: impl IntoIterator<Item = String>) -> Self {
        Self {
            permitted_href_expressions: keys
                .into_iter()
                .map(|key| format!("options.{key}"))
                .collect(),
        }
    }

    fn allows_anchor_href(&self, expr: &str) -> bool {
        self.permitted_href_expressions.contains(expr)
    }
}

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
    part_expansions: usize,
}

impl CompiledCsecComposition {
    pub(crate) fn part_expansions(&self) -> usize {
        self.part_expansions
    }
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
        let mut remaining = TEMPLATE_FUEL_LIMIT;
        self.render_with_remaining_fuel(context, sanitized_content_html, &mut remaining)
    }

    /// Public Builder composition can contain multiple independently compiled
    /// theme segments; all of them consume the same request-local fuel.
    pub(crate) fn render_with_remaining_fuel(
        &self,
        context: Value,
        sanitized_content_html: Option<&str>,
        fuel_remaining: &mut u64,
    ) -> Result<String, &'static str> {
        if *fuel_remaining == 0 || *fuel_remaining > TEMPLATE_FUEL_LIMIT {
            return Err("Fuel composition không hợp lệ hoặc đã hết.");
        }
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
        env.set_fuel(Some(*fuel_remaining));
        env.add_template(&self.name, &source)
            .map_err(|_| "Không thể biên dịch theme composition.")?;
        let captured = env
            .get_template(&self.name)
            .map_err(|_| "Không tìm thấy theme composition.")?
            .render_captured(context)
            .map_err(|_| "Lỗi thực thi theme composition (fuel/recursion/undefined).")?;
        let (consumed, remaining) = captured
            .state()
            .fuel_levels()
            .ok_or("Thiếu bộ theo dõi fuel composition.")?;
        if fuel_remaining.checked_sub(consumed) != Some(remaining) {
            return Err("Fuel composition không hợp lệ.");
        }
        *fuel_remaining = remaining;
        let shell = captured.output();
        if shell.len() > MAX_RENDERED_HTML_BYTES {
            return Err("HTML render vượt quá giới hạn 512 KiB.");
        }
        validate_html_and_css_context(&shell, true)?;
        let Some(nonce) = nonce else {
            return Ok(shell.to_owned());
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

// Reserved only by the block RichText post-render nonce replacer.
pub const BLOCK_RICHTEXT_NONCE_PREFIX: &str = "OD_RICHTEXT_";

// Per-node fuel cap within the shared 10,000 request RenderBudget.
pub const MAX_BLOCK_FUEL_PER_NODE: u64 = 2_000;

#[derive(Debug, Clone)]
pub struct RenderBudget {
    pub fuel_remaining: u64,
    pub html_bytes_used: usize,
}

impl Default for RenderBudget {
    fn default() -> Self {
        Self {
            fuel_remaining: TEMPLATE_FUEL_LIMIT,
            html_bytes_used: 0,
        }
    }
}

impl RenderBudget {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn charge_html_bytes(&mut self, add_bytes: usize) -> Result<(), &'static str> {
        self.html_bytes_used = self.html_bytes_used.saturating_add(add_bytes);
        if self.html_bytes_used > MAX_RENDERED_HTML_BYTES {
            return Err("Builder template composition exceeds 512 KiB.");
        }
        Ok(())
    }
}

/// One budget per private preview. Storage owns graph discovery and records
/// edge occurrences and the active canonical-key stack before rendering.
/// Every contiguous C-SEC segment debits the same remaining fuel.
#[derive(Debug)]
pub(crate) struct PreviewCompositionBudget {
    pub fuel_remaining: u64,
    pub html_bytes_used: usize,
    pub expansions: usize,
    pub stack: Vec<String>,
}

impl Default for PreviewCompositionBudget {
    fn default() -> Self {
        Self {
            fuel_remaining: TEMPLATE_FUEL_LIMIT,
            html_bytes_used: 0,
            expansions: 0,
            stack: Vec::new(),
        }
    }
}

impl PreviewCompositionBudget {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_render_budget(rb: RenderBudget) -> Self {
        Self {
            fuel_remaining: rb.fuel_remaining,
            html_bytes_used: rb.html_bytes_used,
            expansions: 0,
            stack: Vec::new(),
        }
    }

    pub(crate) fn as_render_budget(&self) -> RenderBudget {
        RenderBudget {
            fuel_remaining: self.fuel_remaining,
            html_bytes_used: self.html_bytes_used,
        }
    }

    pub(crate) fn sync_from_render_budget(&mut self, rb: &RenderBudget) {
        self.fuel_remaining = rb.fuel_remaining;
        self.html_bytes_used = rb.html_bytes_used;
    }
}

/// Validated literal edges in source order, including repeated occurrences.
/// No execution is needed to discover the mixed preview graph.
pub(crate) fn preview_csec_part_keys(
    name: &str,
    source: &str,
) -> Result<Vec<String>, &'static str> {
    preview_csec_part_keys_with_theme_urls(name, source, &ThemeUrlPolicy::default())
}

pub(crate) fn preview_csec_part_keys_with_theme_urls(
    name: &str,
    source: &str,
    policy: &ThemeUrlPolicy,
) -> Result<Vec<String>, &'static str> {
    preview_composition_sites(name, source, policy)?
        .into_iter()
        .map(|site| match site.call {
            CompositionCall::Part(key) => Ok(key),
            CompositionCall::Content => Err("Template part không được chứa od_content()."),
        })
        .collect()
}

/// Literal part edges of a validated public Theme part. Unlike the preview
/// compiler this does not change the accepted source format or marker rules.
pub(crate) fn csec_part_keys(name: &str, source: &str) -> Result<Vec<String>, &'static str> {
    csec_part_keys_with_theme_urls(name, source, &ThemeUrlPolicy::default())
}

pub(crate) fn csec_part_keys_with_theme_urls(
    name: &str,
    source: &str,
    policy: &ThemeUrlPolicy,
) -> Result<Vec<String>, &'static str> {
    composition_sites(name, source, true, policy)?
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
    policy: &ThemeUrlPolicy,
) -> Result<Vec<CompositionSite>, &'static str> {
    if source.len() > MAX_RENDERED_HTML_BYTES {
        return Err("Template hoặc part vượt giới hạn 512 KiB.");
    }
    if source.contains(PREVIEW_PART_NONCE_PREFIX) {
        return Err("Theme HTML chứa marker nội bộ bị cấm.");
    }
    composition_sites(name, source, true, policy)
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
    render_preview_csec_fragment_with_theme_urls(
        name,
        source,
        context,
        theme_sources,
        trusted_builder_parts,
        budget,
        &ThemeUrlPolicy::default(),
    )
}

pub(crate) fn render_preview_csec_fragment_with_theme_urls(
    name: &str,
    source: &str,
    context: Value,
    theme_sources: &BTreeMap<String, String>,
    trusted_builder_parts: &BTreeMap<String, String>,
    budget: &mut PreviewCompositionBudget,
    policy: &ThemeUrlPolicy,
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
        policy,
    };
    let mut program = String::new();
    compiler.flatten(name, source, 0, &mut program)?;
    // Revalidate across source boundaries; the original source checks alone
    // cannot detect markup or syntax introduced by concatenation.
    validate_template_csec_with_theme_urls(name, &program, policy)?;
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
    policy: &'a ThemeUrlPolicy,
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
        let sites = preview_composition_sites(name, source, self.policy)?;
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
    "title",
    "xmp",
    "noembed",
    "noframes",
    "plaintext",
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
    validate_template_csec_with_theme_urls(name, source, &ThemeUrlPolicy::default())
}

pub fn validate_template_csec_with_theme_urls(
    name: &str,
    source: &str,
    policy: &ThemeUrlPolicy,
) -> Result<(), &'static str> {
    if source.trim().is_empty() {
        return Err("Template không được để trống.");
    }
    let ast = parse(source, name, SyntaxConfig::default())
        .map_err(|_| "Cú pháp template MiniJinja không hợp lệ.")?;

    validate_stmt(&ast)?;
    if !policy.permitted_href_expressions.is_empty() {
        reject_options_rebinding(&ast)?;
    }
    validate_theme_html_context(source, false, policy)?;

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
    validate_and_compile_csec_composition_with_theme_urls(
        root_name,
        root_source,
        parts,
        kind,
        &ThemeUrlPolicy::default(),
    )
}

pub fn validate_and_compile_csec_composition_with_theme_urls(
    root_name: &str,
    root_source: &str,
    parts: &BTreeMap<String, String>,
    kind: CsecContentKind,
    policy: &ThemeUrlPolicy,
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
    let root_sites = composition_sites(root_name, root_source, false, policy)?;
    let mut part_sites = BTreeMap::new();
    for (key, source) in parts {
        if !valid_part_key(key) {
            return Err("Part key không hợp lệ.");
        }
        part_sites.insert(key.clone(), composition_sites(key, source, true, policy)?);
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
    validate_template_csec_with_theme_urls(root_name, &compiled_source, policy)?;
    Ok(CompiledCsecComposition {
        source: compiled_source,
        name: root_name.to_owned(),
        content_count,
        part_expansions: expansions,
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
    policy: &ThemeUrlPolicy,
) -> Result<Vec<CompositionSite>, &'static str> {
    if source.trim().is_empty() {
        return Err("Template không được để trống.");
    }
    if source.contains(CONTENT_SLOT_TOKEN) {
        return Err("Theme HTML chứa marker nội bộ bị cấm.");
    }
    validate_theme_html_context(source, false, policy)?;
    let ast = parse(source, name, SyntaxConfig::default())
        .map_err(|_| "Cú pháp theme MiniJinja không hợp lệ.")?;
    if !policy.permitted_href_expressions.is_empty() {
        reject_options_rebinding(&ast)?;
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CommentScanSubState {
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

/// Bounded lexical HTML states for each possible Jinja control-flow path.
/// A Jinja branch cannot borrow a closing quote/tag from another branch.
/// The prefix is only used to recognize comments, style, and title tags; it is
/// deliberately bounded so hostile templates cannot grow validation state.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum HtmlScanState {
    Body,
    Tag { quote: Option<u8>, prefix: String },
    Comment(CommentScanSubState),
    Style { closing_prefix: usize },
    Title { closing_prefix: usize },
}

impl HtmlScanState {
    fn consume(self, byte: u8) -> Self {
        match self {
            Self::Body if byte == b'<' => Self::Tag {
                quote: None,
                prefix: "<".to_owned(),
            },
            Self::Body => Self::Body,
            Self::Comment(sub) => match sub {
                CommentScanSubState::Start => match byte {
                    b'>' => Self::Body,
                    b'-' => Self::Comment(CommentScanSubState::StartDash),
                    _ => Self::Comment(CommentScanSubState::Body),
                },
                CommentScanSubState::StartDash => match byte {
                    b'>' => Self::Body,
                    b'-' => Self::Comment(CommentScanSubState::Dash2),
                    _ => Self::Comment(CommentScanSubState::Body),
                },
                CommentScanSubState::Body => match byte {
                    b'-' => Self::Comment(CommentScanSubState::Dash1),
                    _ => Self::Comment(CommentScanSubState::Body),
                },
                CommentScanSubState::Dash1 => match byte {
                    b'-' => Self::Comment(CommentScanSubState::Dash2),
                    _ => Self::Comment(CommentScanSubState::Body),
                },
                CommentScanSubState::Dash2 => match byte {
                    b'>' => Self::Body,
                    b'!' => Self::Comment(CommentScanSubState::Dash2Bang),
                    b'-' => Self::Comment(CommentScanSubState::Dash2),
                    _ => Self::Comment(CommentScanSubState::Body),
                },
                CommentScanSubState::Dash2Bang => match byte {
                    b'>' => Self::Body,
                    b'-' => Self::Comment(CommentScanSubState::Dash1),
                    _ => Self::Comment(CommentScanSubState::Body),
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
                if prefix.len() < 7 {
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
fn decode_html_entities_for_url(raw: &str) -> String {
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
    if validate_safe_url(raw).is_ok() || opendash_core::block_schema::valid_block_url(trimmed) {
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

fn binds_options(expr: &Expr<'_>) -> bool {
    match expr {
        Expr::Var(var) => var.id == "options",
        Expr::Tuple(tuple) => tuple.items.iter().any(binds_options),
        Expr::List(list) => list.items.iter().any(binds_options),
        // Attribute and subscript assignments are forbidden rather than
        // allowing a mutable binding to masquerade as the manifest root.
        Expr::GetAttr(attr) => binds_options(&attr.expr),
        Expr::GetItem(item) => binds_options(&item.expr),
        _ => false,
    }
}

fn reject_options_rebinding(stmt: &Stmt<'_>) -> Result<(), &'static str> {
    let forbidden = || Err("C-SEC cấm gán lại biến options trong theme.");
    match stmt {
        Stmt::Template(t) => t.children.iter().try_for_each(reject_options_rebinding),
        Stmt::ForLoop(f) => {
            if binds_options(&f.target) {
                return forbidden();
            }
            f.body
                .iter()
                .chain(&f.else_body)
                .try_for_each(reject_options_rebinding)
        }
        Stmt::WithBlock(w) => {
            if w.assignments.iter().any(|(key, _)| binds_options(key)) {
                return forbidden();
            }
            w.body.iter().try_for_each(reject_options_rebinding)
        }
        Stmt::Set(s) => {
            if binds_options(&s.target) {
                forbidden()
            } else {
                Ok(())
            }
        }
        Stmt::SetBlock(sb) => {
            if binds_options(&sb.target) {
                return forbidden();
            }
            sb.body.iter().try_for_each(reject_options_rebinding)
        }
        Stmt::IfCond(i) => i
            .true_body
            .iter()
            .chain(&i.false_body)
            .try_for_each(reject_options_rebinding),
        _ => Ok(()),
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

fn find_html5_comment_end_in_template(bytes: &[u8], start: usize) -> Option<usize> {
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

fn validate_html_and_css_context(src: &str, post_render: bool) -> Result<(), &'static str> {
    validate_html_and_css_context_with_url_vars(src, post_render, None, None)
}

fn validate_theme_html_context(
    src: &str,
    post_render: bool,
    policy: &ThemeUrlPolicy,
) -> Result<(), &'static str> {
    validate_html_and_css_context_with_url_vars(src, post_render, None, Some(policy))
}

fn is_sensitive_global_option_key(key: &str) -> bool {
    opendash_core::block_schema::GLOBAL_PUBLIC_URL_OPTION_KEYS.contains(&key)
        || opendash_core::block_schema::GLOBAL_PUBLIC_MEDIA_OPTION_KEYS.contains(&key)
}

struct BlockUrlVarAllowlist {
    href_vars: BTreeSet<String>,
    src_vars: BTreeSet<String>,
    rep_href_subkeys: BTreeMap<String, BTreeSet<String>>,
    rep_src_subkeys: BTreeMap<String, BTreeSet<String>>,
}

impl BlockUrlVarAllowlist {
    fn allows_href_expr(&self, expr: &str) -> bool {
        if self.href_vars.contains(expr) || self.src_vars.contains(expr) {
            return true;
        }
        if let Some(opt_key) = expr.strip_prefix("options.") {
            return opendash_core::block_schema::GLOBAL_PUBLIC_URL_OPTION_KEYS.contains(&opt_key);
        }
        if let Some((_loop_var, sub_key)) = expr.split_once('.') {
            return self
                .rep_href_subkeys
                .values()
                .any(|set| set.contains(sub_key))
                || self
                    .rep_src_subkeys
                    .values()
                    .any(|set| set.contains(sub_key));
        }
        false
    }

    fn allows_src_expr(&self, expr: &str) -> bool {
        if self.src_vars.contains(expr) {
            return true;
        }
        if let Some(opt_key) = expr.strip_prefix("options.") {
            return opendash_core::block_schema::GLOBAL_PUBLIC_MEDIA_OPTION_KEYS.contains(&opt_key);
        }
        if let Some((_loop_var, sub_key)) = expr.split_once('.') {
            return self
                .rep_src_subkeys
                .values()
                .any(|set| set.contains(sub_key));
        }
        false
    }
}

fn validate_html_and_css_context_with_url_vars(
    src: &str,
    post_render: bool,
    block_url_vars: Option<&BlockUrlVarAllowlist>,
    theme_policy: Option<&ThemeUrlPolicy>,
) -> Result<(), &'static str> {
    let lower = src.to_ascii_lowercase();
    let is_full_doctype_doc =
        block_url_vars.is_none() && lower.trim_start().starts_with("<!doctype");

    // Validate <style>...</style> blocks
    let mut style_search = 0;
    while let Some(open_pos) = lower[style_search..].find("<style") {
        if block_url_vars.is_some() {
            return Err("Template block không được chứa thẻ <style> trực tiếp (dùng css_var).");
        }
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
            // Skip doctype or comments (using HTML5 comment end rules)
            if lower[i..].starts_with("<!--") {
                if let Some(end) = find_html5_comment_end_in_template(bytes, i) {
                    i = end;
                    continue;
                }
                return Err("HTML comment không đóng.");
            }
            if lower[i..].starts_with("<!doctype") {
                if block_url_vars.is_some() {
                    return Err("Template block không được chứa DOCTYPE.");
                }
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
            if FORBIDDEN_TAGS.contains(&tag_name.as_str())
                && !(tag_name == "title" && is_full_doctype_doc)
            {
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
            validate_tag_attributes(
                &tag_name,
                attr_slice,
                post_render,
                block_url_vars,
                theme_policy,
            )?;
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
    block_url_vars: Option<&BlockUrlVarAllowlist>,
    theme_policy: Option<&ThemeUrlPolicy>,
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

        if matches!(
            name.as_str(),
            "href" | "src" | "poster" | "cite" | "srcset" | "ping"
        ) {
            if !post_render && (val.contains("{{") || val.contains("{%")) {
                if matches!(name.as_str(), "srcset" | "ping") {
                    return Err("C-SEC cấm nội suy {{ ... }} trong thuộc tính srcset/ping.");
                }
                let expr_inner = val
                    .trim()
                    .strip_prefix("{{")
                    .and_then(|s| s.strip_suffix("}}"))
                    .map(str::trim)
                    .filter(|s| !s.contains("{{") && !s.contains("}}") && !s.contains("{%"))
                    .ok_or("URL động trong thuộc tính chỉ được phép là biểu thức {{ ... }} đơn.")?;
                if let Some(block_vars) = block_url_vars {
                    let allowed = if tag_name == "a" && name == "href" {
                        block_vars.allows_href_expr(expr_inner)
                    } else if matches!(name.as_str(), "src" | "poster") {
                        block_vars.allows_src_expr(expr_inner)
                    } else {
                        false
                    };
                    if !allowed {
                        return Err(
                            "URL động trong template block phải khớp đúng biến kiểu url (href) hoặc <media_key>_url (src/poster).",
                        );
                    }
                } else if !ALLOWED_DYNAMIC_URL_VARS.contains(&expr_inner)
                    && !(tag_name == "a"
                        && name == "href"
                        && theme_policy.is_some_and(|policy| policy.allows_anchor_href(expr_inner)))
                {
                    return Err("URL động trong thuộc tính không nằm trong whitelist typed URL.");
                }
            } else {
                let decoded = decode_html_entities_for_url(&val);
                if name == "srcset" {
                    if decoded.trim().is_empty()
                        || decoded.contains('\0')
                        || decoded.bytes().any(|b| b <= 0x1f || b == 0x7f)
                    {
                        return Err("Thuộc tính srcset không hợp lệ.");
                    }
                    for candidate in decoded.split(',') {
                        let url_part = candidate
                            .split_whitespace()
                            .next()
                            .ok_or("Thuộc tính srcset chứa mục rỗng.")?;
                        validate_safe_url(url_part)?;
                    }
                } else if name == "ping" {
                    if decoded.trim().is_empty()
                        || decoded.contains('\0')
                        || decoded.bytes().any(|b| b <= 0x1f || b == 0x7f)
                    {
                        return Err("Thuộc tính ping không hợp lệ.");
                    }
                    for target in decoded.split_whitespace() {
                        validate_safe_url(target)?;
                    }
                } else if tag_name == "a" && name == "href" {
                    validate_safe_href_url(&decoded)?;
                } else {
                    validate_safe_url(&decoded)?;
                }
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
        if !name.is_empty() && map.insert(name, val).is_some() {
            return Err("Thẻ HTML chứa thuộc tính bị trùng lặp.");
        }
    }
    Ok(map)
}

#[derive(Default)]
struct RepeaterSubContext {
    richtext_subkeys: BTreeSet<String>,
    href_subkeys: BTreeSet<String>,
    src_subkeys: BTreeSet<String>,
    media_raw_subkeys: BTreeSet<String>,
}

struct BlockSchemaTemplateContext {
    richtext_vars: BTreeSet<String>,
    href_vars: BTreeSet<String>,
    src_vars: BTreeSet<String>,
    media_raw_vars: BTreeSet<String>,
    repeaters: BTreeMap<String, RepeaterSubContext>,
}

impl BlockSchemaTemplateContext {
    fn from_schema(schema: &opendash_core::block_schema::BlockSchemaV1) -> Self {
        use opendash_core::block_schema::BlockFieldSpec;
        let mut ctx = Self {
            richtext_vars: BTreeSet::new(),
            href_vars: BTreeSet::new(),
            src_vars: BTreeSet::new(),
            media_raw_vars: BTreeSet::new(),
            repeaters: BTreeMap::new(),
        };
        for field in schema.flattened_top_fields() {
            match field {
                BlockFieldSpec::Richtext { key, .. } => {
                    ctx.richtext_vars.insert(key.clone());
                }
                BlockFieldSpec::Url { key, .. } | BlockFieldSpec::Link { key, .. } => {
                    ctx.href_vars.insert(key.clone());
                    if matches!(field, BlockFieldSpec::Link { .. }) {
                        ctx.href_vars.insert(format!("{key}_url"));
                    }
                }
                BlockFieldSpec::Media { key, .. } => {
                    ctx.media_raw_vars.insert(key.clone());
                    ctx.src_vars.insert(format!("{key}_url"));
                }
                BlockFieldSpec::Repeater { key, fields, .. } => {
                    let mut sub_ctx = RepeaterSubContext::default();
                    for sub in fields {
                        match sub {
                            BlockFieldSpec::Richtext { key: sk, .. } => {
                                sub_ctx.richtext_subkeys.insert(sk.clone());
                            }
                            BlockFieldSpec::Url { key: sk, .. }
                            | BlockFieldSpec::Link { key: sk, .. } => {
                                sub_ctx.href_subkeys.insert(sk.clone());
                                if matches!(sub, BlockFieldSpec::Link { .. }) {
                                    sub_ctx.href_subkeys.insert(format!("{sk}_url"));
                                }
                            }
                            BlockFieldSpec::Media { key: sk, .. } => {
                                sub_ctx.media_raw_subkeys.insert(sk.clone());
                                sub_ctx.src_subkeys.insert(format!("{sk}_url"));
                            }
                            _ => {}
                        }
                    }
                    ctx.repeaters.insert(key.clone(), sub_ctx);
                }
                _ => {}
            }
        }
        ctx
    }

    fn url_allowlist(&self) -> BlockUrlVarAllowlist {
        BlockUrlVarAllowlist {
            href_vars: self.href_vars.clone(),
            src_vars: self.src_vars.clone(),
            rep_href_subkeys: self
                .repeaters
                .iter()
                .map(|(k, v)| (k.clone(), v.href_subkeys.clone()))
                .collect(),
            rep_src_subkeys: self
                .repeaters
                .iter()
                .map(|(k, v)| (k.clone(), v.src_subkeys.clone()))
                .collect(),
        }
    }

    fn is_sensitive_name(&self, id: &str, active_loops: &BTreeMap<String, String>) -> bool {
        matches!(id, "options" | "theme_options" | "od_is_preview")
            || self.richtext_vars.contains(id)
            || self.href_vars.contains(id)
            || self.src_vars.contains(id)
            || self.media_raw_vars.contains(id)
            || self.repeaters.contains_key(id)
            || active_loops.contains_key(id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SensitiveVarKind {
    Richtext,
    HrefUrl,
    MediaUrl,
    MediaRaw,
    RepeaterOrLoopItem,
}

fn classify_bare_sensitive_expr(
    expr: &Expr<'_>,
    ctx: &BlockSchemaTemplateContext,
    active_loops: &BTreeMap<String, String>,
) -> Option<SensitiveVarKind> {
    match expr {
        Expr::Var(v) => {
            if ctx.richtext_vars.contains(v.id) {
                Some(SensitiveVarKind::Richtext)
            } else if ctx.href_vars.contains(v.id) {
                Some(SensitiveVarKind::HrefUrl)
            } else if ctx.src_vars.contains(v.id) {
                Some(SensitiveVarKind::MediaUrl)
            } else if ctx.media_raw_vars.contains(v.id) {
                Some(SensitiveVarKind::MediaRaw)
            } else if ctx.repeaters.contains_key(v.id) || active_loops.contains_key(v.id) {
                Some(SensitiveVarKind::RepeaterOrLoopItem)
            } else {
                None
            }
        }
        Expr::GetAttr(g) => {
            if let Expr::Var(v) = &g.expr {
                if v.id == "options" {
                    if opendash_core::block_schema::GLOBAL_PUBLIC_URL_OPTION_KEYS.contains(&g.name)
                    {
                        return Some(SensitiveVarKind::HrefUrl);
                    }
                    if opendash_core::block_schema::GLOBAL_PUBLIC_MEDIA_OPTION_KEYS
                        .contains(&g.name)
                    {
                        return Some(SensitiveVarKind::MediaUrl);
                    }
                }
                if let Some(rep_key) = active_loops.get(v.id)
                    && let Some(rep) = ctx.repeaters.get(rep_key)
                {
                    if rep.richtext_subkeys.contains(g.name) {
                        return Some(SensitiveVarKind::Richtext);
                    }
                    if rep.href_subkeys.contains(g.name) {
                        return Some(SensitiveVarKind::HrefUrl);
                    }
                    if rep.src_subkeys.contains(g.name) {
                        return Some(SensitiveVarKind::MediaUrl);
                    }
                    if rep.media_raw_subkeys.contains(g.name) {
                        return Some(SensitiveVarKind::MediaRaw);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn expr_touches_any_sensitive(
    expr: &Expr<'_>,
    ctx: &BlockSchemaTemplateContext,
    active_loops: &BTreeMap<String, String>,
) -> bool {
    if classify_bare_sensitive_expr(expr, ctx, active_loops).is_some() {
        return true;
    }
    match expr {
        Expr::Var(v) => ctx.is_sensitive_name(v.id, active_loops),
        Expr::Const(_) => false,
        Expr::Slice(s) => {
            expr_touches_any_sensitive(&s.expr, ctx, active_loops)
                || s.start
                    .as_ref()
                    .is_some_and(|e| expr_touches_any_sensitive(e, ctx, active_loops))
                || s.stop
                    .as_ref()
                    .is_some_and(|e| expr_touches_any_sensitive(e, ctx, active_loops))
                || s.step
                    .as_ref()
                    .is_some_and(|e| expr_touches_any_sensitive(e, ctx, active_loops))
        }
        Expr::UnaryOp(u) => expr_touches_any_sensitive(&u.expr, ctx, active_loops),
        Expr::BinOp(b) => {
            expr_touches_any_sensitive(&b.left, ctx, active_loops)
                || expr_touches_any_sensitive(&b.right, ctx, active_loops)
        }
        Expr::Compare(c) => {
            expr_touches_any_sensitive(&c.expr, ctx, active_loops)
                || c.ops
                    .iter()
                    .any(|op| expr_touches_any_sensitive(&op.expr, ctx, active_loops))
        }
        Expr::IfExpr(i) => {
            expr_touches_any_sensitive(&i.test_expr, ctx, active_loops)
                || expr_touches_any_sensitive(&i.true_expr, ctx, active_loops)
                || i.false_expr
                    .as_ref()
                    .is_some_and(|e| expr_touches_any_sensitive(e, ctx, active_loops))
        }
        Expr::GetAttr(g) => {
            if let Expr::Var(v) = &g.expr {
                if v.id == "options" {
                    return is_sensitive_global_option_key(g.name);
                }
                if let Some(rep_key) = active_loops.get(v.id)
                    && let Some(rep) = ctx.repeaters.get(rep_key)
                {
                    return rep.richtext_subkeys.contains(g.name)
                        || rep.href_subkeys.contains(g.name)
                        || rep.src_subkeys.contains(g.name)
                        || rep.media_raw_subkeys.contains(g.name);
                }
            }
            expr_touches_any_sensitive(&g.expr, ctx, active_loops)
        }
        Expr::GetItem(g) => {
            expr_touches_any_sensitive(&g.expr, ctx, active_loops)
                || expr_touches_any_sensitive(&g.subscript_expr, ctx, active_loops)
        }
        Expr::List(l) => l
            .items
            .iter()
            .any(|e| expr_touches_any_sensitive(e, ctx, active_loops)),
        Expr::Tuple(t) => t
            .items
            .iter()
            .any(|e| expr_touches_any_sensitive(e, ctx, active_loops)),
        Expr::Map(m) => {
            m.keys
                .iter()
                .any(|e| expr_touches_any_sensitive(e, ctx, active_loops))
                || m.values
                    .iter()
                    .any(|e| expr_touches_any_sensitive(e, ctx, active_loops))
        }
        Expr::Filter(f) => {
            f.expr
                .as_ref()
                .is_some_and(|e| expr_touches_any_sensitive(e, ctx, active_loops))
                || f.args
                    .iter()
                    .any(|arg| call_arg_touches_sensitive(arg, ctx, active_loops))
        }
        Expr::Test(t) => {
            expr_touches_any_sensitive(&t.expr, ctx, active_loops)
                || t.args
                    .iter()
                    .any(|arg| call_arg_touches_sensitive(arg, ctx, active_loops))
        }
        Expr::Call(_) => true,
    }
}

fn call_arg_touches_sensitive(
    arg: &minijinja::machinery::ast::CallArg<'_>,
    ctx: &BlockSchemaTemplateContext,
    active_loops: &BTreeMap<String, String>,
) -> bool {
    match arg {
        minijinja::machinery::ast::CallArg::Pos(e)
        | minijinja::machinery::ast::CallArg::Kwarg(_, e)
        | minijinja::machinery::ast::CallArg::PosSplat(e)
        | minijinja::machinery::ast::CallArg::KwargSplat(e) => {
            expr_touches_any_sensitive(e, ctx, active_loops)
        }
    }
}

fn check_block_expr_restrictions(
    expr: &Expr<'_>,
    ctx: &BlockSchemaTemplateContext,
    active_loops: &BTreeMap<String, String>,
) -> Result<(), &'static str> {
    match expr {
        Expr::Var(_) | Expr::Const(_) => Ok(()),
        Expr::GetItem(_) => Err("C-SEC cấm truy cập GetItem ([...]) trong template block."),
        Expr::Filter(f) => {
            if f.expr
                .as_ref()
                .is_some_and(|e| expr_touches_any_sensitive(e, ctx, active_loops))
                || f.args
                    .iter()
                    .any(|arg| call_arg_touches_sensitive(arg, ctx, active_loops))
            {
                return Err("C-SEC cấm áp dụng filter lên biến richtext/url/media/repeater.");
            }
            if let Some(e) = &f.expr {
                check_block_expr_restrictions(e, ctx, active_loops)?;
            }
            for arg in &f.args {
                check_block_call_arg_restrictions(arg, ctx, active_loops)?;
            }
            Ok(())
        }
        Expr::Slice(s) => {
            if expr_touches_any_sensitive(&s.expr, ctx, active_loops) {
                return Err("C-SEC cấm cắt lát (slice) trên biến nhạy cảm.");
            }
            check_block_expr_restrictions(&s.expr, ctx, active_loops)?;
            if let Some(start) = &s.start {
                check_block_expr_restrictions(start, ctx, active_loops)?;
            }
            if let Some(stop) = &s.stop {
                check_block_expr_restrictions(stop, ctx, active_loops)?;
            }
            if let Some(step) = &s.step {
                check_block_expr_restrictions(step, ctx, active_loops)?;
            }
            Ok(())
        }
        Expr::UnaryOp(u) => check_block_expr_restrictions(&u.expr, ctx, active_loops),
        Expr::BinOp(b) => {
            check_block_expr_restrictions(&b.left, ctx, active_loops)?;
            check_block_expr_restrictions(&b.right, ctx, active_loops)
        }
        Expr::Compare(c) => {
            check_block_expr_restrictions(&c.expr, ctx, active_loops)?;
            for op in &c.ops {
                check_block_expr_restrictions(&op.expr, ctx, active_loops)?;
            }
            Ok(())
        }
        Expr::IfExpr(i) => {
            check_block_expr_restrictions(&i.test_expr, ctx, active_loops)?;
            check_block_expr_restrictions(&i.true_expr, ctx, active_loops)?;
            if let Some(f) = &i.false_expr {
                check_block_expr_restrictions(f, ctx, active_loops)?;
            }
            Ok(())
        }
        Expr::GetAttr(g) => check_block_expr_restrictions(&g.expr, ctx, active_loops),
        Expr::List(l) => {
            for item in &l.items {
                check_block_expr_restrictions(item, ctx, active_loops)?;
            }
            Ok(())
        }
        Expr::Tuple(t) => {
            for item in &t.items {
                check_block_expr_restrictions(item, ctx, active_loops)?;
            }
            Ok(())
        }
        Expr::Map(m) => {
            for (k, v) in m.keys.iter().zip(m.values.iter()) {
                check_block_expr_restrictions(k, ctx, active_loops)?;
                check_block_expr_restrictions(v, ctx, active_loops)?;
            }
            Ok(())
        }
        Expr::Test(t) => {
            check_block_expr_restrictions(&t.expr, ctx, active_loops)?;
            for arg in &t.args {
                check_block_call_arg_restrictions(arg, ctx, active_loops)?;
            }
            Ok(())
        }
        Expr::Call(_) => Err("C-SEC cấm lời gọi hàm Call trong template block."),
    }
}

fn check_block_call_arg_restrictions(
    arg: &minijinja::machinery::ast::CallArg<'_>,
    ctx: &BlockSchemaTemplateContext,
    active_loops: &BTreeMap<String, String>,
) -> Result<(), &'static str> {
    match arg {
        minijinja::machinery::ast::CallArg::Pos(e)
        | minijinja::machinery::ast::CallArg::Kwarg(_, e)
        | minijinja::machinery::ast::CallArg::PosSplat(e)
        | minijinja::machinery::ast::CallArg::KwargSplat(e) => {
            check_block_expr_restrictions(e, ctx, active_loops)
        }
    }
}

fn walk_block_ast_sequence(
    children: &[Stmt<'_>],
    states: &BTreeSet<HtmlScanState>,
    ctx: &BlockSchemaTemplateContext,
    active_loops: &BTreeMap<String, String>,
) -> Result<BTreeSet<HtmlScanState>, &'static str> {
    let mut current = states.clone();
    for child in children {
        current = walk_block_ast_stmt(child, &current, ctx, active_loops)?;
        bounded_html_states(current.clone())?;
    }
    Ok(current)
}

fn walk_block_ast_stmt(
    stmt: &Stmt<'_>,
    states: &BTreeSet<HtmlScanState>,
    ctx: &BlockSchemaTemplateContext,
    active_loops: &BTreeMap<String, String>,
) -> Result<BTreeSet<HtmlScanState>, &'static str> {
    match stmt {
        Stmt::Template(t) => walk_block_ast_sequence(&t.children, states, ctx, active_loops),
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
        Stmt::EmitExpr(e) => {
            check_block_expr_restrictions(&e.expr, ctx, active_loops)?;
            if let Some(kind) = classify_bare_sensitive_expr(&e.expr, ctx, active_loops) {
                match kind {
                    SensitiveVarKind::Richtext => {
                        if states.iter().any(|s| *s != HtmlScanState::Body) {
                            return Err(
                                "Biến richtext chỉ được phép đặt trong phần thân HTML (HtmlScanState::Body).",
                            );
                        }
                    }
                    SensitiveVarKind::HrefUrl | SensitiveVarKind::MediaUrl => {
                        if states
                            .iter()
                            .any(|s| !matches!(s, HtmlScanState::Tag { quote: Some(_), .. }))
                        {
                            return Err(
                                "Biến url / <media_key>_url chỉ được xuất bên trong thuộc tính URL được trích dẫn.",
                            );
                        }
                    }
                    SensitiveVarKind::MediaRaw | SensitiveVarKind::RepeaterOrLoopItem => {
                        return Err(
                            "Không được xuất trực tiếp biến media ID thô hoặc đối tượng repeater.",
                        );
                    }
                }
            } else if expr_touches_any_sensitive(&e.expr, ctx, active_loops) {
                return Err(
                    "Biến richtext/url/media không được phép ghép chuỗi hoặc biến đổi trong biểu thức xuất.",
                );
            }
            Ok(states.clone())
        }
        Stmt::IfCond(cond) => {
            check_block_expr_restrictions(&cond.expr, ctx, active_loops)?;
            let mut result = walk_block_ast_sequence(&cond.true_body, states, ctx, active_loops)?;
            result.extend(walk_block_ast_sequence(
                &cond.false_body,
                states,
                ctx,
                active_loops,
            )?);
            bounded_html_states(result)
        }
        Stmt::ForLoop(loop_stmt) => {
            check_block_expr_restrictions(&loop_stmt.iter, ctx, active_loops)?;
            if expr_touches_any_sensitive(&loop_stmt.target, ctx, active_loops) {
                return Err("Biến vòng lặp for không được che khuất tên biến nhạy cảm.");
            }
            if let Some(cond) = &loop_stmt.filter_expr {
                check_block_expr_restrictions(cond, ctx, active_loops)?;
            }
            let mut inner_loops = active_loops.clone();
            match (&loop_stmt.target, &loop_stmt.iter) {
                (Expr::Var(target_var), Expr::Var(iter_var)) => {
                    if ctx.is_sensitive_name(target_var.id, active_loops) {
                        return Err("Biến vòng lặp for không được che khuất tên biến nhạy cảm.");
                    }
                    if let Some(kind) =
                        classify_bare_sensitive_expr(&loop_stmt.iter, ctx, active_loops)
                        && kind != SensitiveVarKind::RepeaterOrLoopItem
                    {
                        return Err("Không được lặp trực tiếp trên biến richtext/url/media.");
                    }
                    if ctx.repeaters.contains_key(iter_var.id) {
                        inner_loops.insert(target_var.id.to_owned(), iter_var.id.to_owned());
                    }
                }
                _ => {
                    if expr_touches_any_sensitive(&loop_stmt.iter, ctx, active_loops) {
                        return Err(
                            "Vòng lặp for trên repeater bắt buộc dùng cú pháp `{% for item in items %}`.",
                        );
                    }
                }
            }

            let mut possible = states.clone();
            let mut frontier = states.clone();
            let mut settled = false;
            for _ in 0..64 {
                let after = walk_block_ast_sequence(&loop_stmt.body, &frontier, ctx, &inner_loops)?;
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
                return Err("Không thể xác định vị trí HTML sau vòng lặp trong template block.");
            }
            let after_else =
                walk_block_ast_sequence(&loop_stmt.else_body, &possible, ctx, active_loops)?;
            possible.extend(after_else);
            bounded_html_states(possible)
        }
        Stmt::Set(s) => {
            check_block_expr_restrictions(&s.target, ctx, active_loops)?;
            check_block_expr_restrictions(&s.expr, ctx, active_loops)?;
            if expr_touches_any_sensitive(&s.target, ctx, active_loops)
                || expr_touches_any_sensitive(&s.expr, ctx, active_loops)
            {
                return Err(
                    "C-SEC cấm gán (set) hoặc tạo bí danh cho biến richtext/url/media/repeater.",
                );
            }
            Ok(states.clone())
        }
        Stmt::SetBlock(sb) => {
            if sb.filter.is_some() {
                return Err("C-SEC cấm modifier filter trong khối set.");
            }
            if expr_touches_any_sensitive(&sb.target, ctx, active_loops) {
                return Err("C-SEC cấm ghi đè biến nhạy cảm qua khối set.");
            }
            let _ = walk_block_ast_sequence(&sb.body, states, ctx, active_loops)?;
            // Forbid capturing any sensitive variable inside SetBlock body
            for child in &sb.body {
                if stmt_touches_sensitive(child, ctx, active_loops) {
                    return Err("C-SEC cấm bọc biến richtext/url/media trong khối set.");
                }
            }
            Ok(states.clone())
        }
        Stmt::WithBlock(w) => {
            for (k, v) in &w.assignments {
                check_block_expr_restrictions(k, ctx, active_loops)?;
                check_block_expr_restrictions(v, ctx, active_loops)?;
                if expr_touches_any_sensitive(k, ctx, active_loops)
                    || expr_touches_any_sensitive(v, ctx, active_loops)
                {
                    return Err(
                        "C-SEC cấm tạo bí danh cho biến richtext/url/media/repeater qua khối with.",
                    );
                }
            }
            walk_block_ast_sequence(&w.body, states, ctx, active_loops)
        }
        _ => Err("Template block sử dụng câu lệnh MiniJinja bị C-SEC cấm."),
    }
}

fn stmt_touches_sensitive(
    stmt: &Stmt<'_>,
    ctx: &BlockSchemaTemplateContext,
    active_loops: &BTreeMap<String, String>,
) -> bool {
    match stmt {
        Stmt::Template(t) => t
            .children
            .iter()
            .any(|c| stmt_touches_sensitive(c, ctx, active_loops)),
        Stmt::EmitRaw(_) => false,
        Stmt::EmitExpr(e) => expr_touches_any_sensitive(&e.expr, ctx, active_loops),
        Stmt::IfCond(i) => {
            expr_touches_any_sensitive(&i.expr, ctx, active_loops)
                || i.true_body
                    .iter()
                    .any(|c| stmt_touches_sensitive(c, ctx, active_loops))
                || i.false_body
                    .iter()
                    .any(|c| stmt_touches_sensitive(c, ctx, active_loops))
        }
        Stmt::ForLoop(f) => {
            expr_touches_any_sensitive(&f.iter, ctx, active_loops)
                || f.body
                    .iter()
                    .any(|c| stmt_touches_sensitive(c, ctx, active_loops))
                || f.else_body
                    .iter()
                    .any(|c| stmt_touches_sensitive(c, ctx, active_loops))
        }
        Stmt::Set(s) => {
            expr_touches_any_sensitive(&s.target, ctx, active_loops)
                || expr_touches_any_sensitive(&s.expr, ctx, active_loops)
        }
        Stmt::SetBlock(sb) => {
            expr_touches_any_sensitive(&sb.target, ctx, active_loops)
                || sb
                    .body
                    .iter()
                    .any(|c| stmt_touches_sensitive(c, ctx, active_loops))
        }
        Stmt::WithBlock(w) => {
            w.assignments.iter().any(|(k, v)| {
                expr_touches_any_sensitive(k, ctx, active_loops)
                    || expr_touches_any_sensitive(v, ctx, active_loops)
            }) || w
                .body
                .iter()
                .any(|c| stmt_touches_sensitive(c, ctx, active_loops))
        }
        _ => true,
    }
}

/// Validates a declarative `block.json` v1 `render.html` template using the joint
/// MiniJinja AST + `HtmlScanState` checker and typed URL/RichText/Repeater rules.
pub fn validate_block_template_csec(
    block_id: &str,
    source: &str,
    schema: &opendash_core::block_schema::BlockSchemaV1,
) -> Result<(), &'static str> {
    if source.trim().is_empty() {
        return Err("Template block không được để trống.");
    }
    if source.len() > MAX_RENDERED_HTML_BYTES {
        return Err("Template block vượt giới hạn 512 KiB.");
    }
    if source.contains(BLOCK_RICHTEXT_NONCE_PREFIX)
        || source.contains(CONTENT_SLOT_TOKEN)
        || source.contains(PREVIEW_PART_NONCE_PREFIX)
    {
        return Err("Template block chứa chuỗi nonce nội bộ bị cấm.");
    }

    let ctx = BlockSchemaTemplateContext::from_schema(schema);
    let url_allowlist = ctx.url_allowlist();
    validate_html_and_css_context_with_url_vars(source, false, Some(&url_allowlist), None)?;

    let ast = parse(source, block_id, SyntaxConfig::default())
        .map_err(|_| "Cú pháp MiniJinja của template block không hợp lệ.")?;
    validate_stmt(&ast)?;

    let final_states = walk_block_ast_stmt(
        &ast,
        &BTreeSet::from([HtmlScanState::Body]),
        &ctx,
        &BTreeMap::new(),
    )?;
    if final_states.iter().any(|s| *s != HtmlScanState::Body) {
        return Err("Template block phải kết thúc ở trạng thái thân HTML (đóng đủ thẻ/comment).");
    }

    let mut env = restricted_environment();
    env.add_template(block_id, source)
        .map_err(|_| "Không thể biên dịch template block trong môi trường C-SEC.")?;
    Ok(())
}

fn generate_richtext_nonce() -> Result<String, &'static str> {
    let mut random = [0_u8; 16];
    OsRng
        .try_fill_bytes(&mut random)
        .map_err(|_| "Không tạo được nonce RichText.")?;
    let mut nonce = String::with_capacity(BLOCK_RICHTEXT_NONCE_PREFIX.len() + 32);
    nonce.push_str(BLOCK_RICHTEXT_NONCE_PREFIX);
    for byte in random {
        use std::fmt::Write;
        write!(&mut nonce, "{byte:02x}").map_err(|_| "Không tạo được nonce RichText.")?;
    }
    Ok(nonce)
}

fn populate_leaf_field_context(
    field: &opendash_core::block_schema::BlockFieldSpec,
    raw_val: Option<&serde_json::Value>,
    raw_map: Option<&BTreeMap<String, serde_json::Value>>,
    raw_obj: Option<&serde_json::Map<String, serde_json::Value>>,
    is_preview: bool,
    target_map: &mut BTreeMap<String, Value>,
    nonces: &mut BTreeMap<String, String>,
) -> Result<(), &'static str> {
    use opendash_core::{
        block_schema::{BlockFieldSpec, valid_block_url, valid_sha256_media_id},
        richtext::sanitize_richtext,
    };

    let key = field.key().to_owned();
    match field {
        BlockFieldSpec::Text { default, .. } | BlockFieldSpec::Textarea { default, .. } => {
            let s = raw_val
                .and_then(serde_json::Value::as_str)
                .or(default.as_deref())
                .unwrap_or("");
            target_map.insert(key, Value::from(s.to_owned()));
        }
        BlockFieldSpec::Richtext { default, .. } => {
            let s = raw_val
                .and_then(serde_json::Value::as_str)
                .or(default.as_deref())
                .unwrap_or("");
            if s.trim().is_empty() {
                target_map.insert(key, Value::from(""));
            } else {
                let clean = sanitize_richtext(s)?;
                if clean.is_empty() {
                    target_map.insert(key, Value::from(""));
                } else {
                    let nonce = generate_richtext_nonce()?;
                    nonces.insert(nonce.clone(), clean);
                    target_map.insert(key, Value::from(nonce));
                }
            }
        }
        BlockFieldSpec::Number { default, .. } => {
            let n = extract_scalar_or_desktop_f64(raw_val)
                .or(*default)
                .unwrap_or(0.0);
            target_map.insert(key, Value::from(n));
        }
        BlockFieldSpec::Range { default, min, .. } => {
            let n = extract_scalar_or_desktop_f64(raw_val)
                .or(*default)
                .unwrap_or(*min);
            target_map.insert(key, Value::from(n));
        }
        BlockFieldSpec::Boolean { default, .. } => {
            let b = raw_val
                .and_then(serde_json::Value::as_bool)
                .or(*default)
                .unwrap_or(false);
            target_map.insert(key, Value::from(b));
        }
        BlockFieldSpec::Select {
            options, default, ..
        }
        | BlockFieldSpec::RadioButtons {
            options, default, ..
        } => {
            let fallback = default
                .as_deref()
                .or_else(|| options.first().map(|o| o.value.as_str()))
                .unwrap_or("");
            let s = extract_scalar_or_desktop_str(raw_val).unwrap_or(fallback);
            target_map.insert(key, Value::from(s.to_owned()));
        }
        BlockFieldSpec::Spacing {
            default,
            unit,
            responsive,
            ..
        } => {
            let css_unit = unit.unwrap_or(opendash_core::block_schema::CssUnit::Px);
            let formatted = extract_spacing_css(raw_val, *default, css_unit).unwrap_or_default();
            let classes = spacing_field_tailwind_classes(raw_val, *default, *responsive, css_unit)
                .map(|list| list.join(" "))
                .unwrap_or_default();
            target_map.insert(format!("{key}_classes"), Value::from(classes));
            target_map.insert(key, Value::from(formatted));
        }
        BlockFieldSpec::Color { default, .. } => {
            let s = raw_val
                .and_then(serde_json::Value::as_str)
                .or(default.as_deref())
                .unwrap_or("transparent");
            target_map.insert(key, Value::from(s.to_owned()));
        }
        BlockFieldSpec::Media { default, .. } => {
            let id = raw_val
                .and_then(serde_json::Value::as_str)
                .or(default.as_deref())
                .unwrap_or("");
            let url = if valid_sha256_media_id(id) {
                if is_preview {
                    format!("/api/admin/media/{id}/file")
                } else {
                    format!("/media/{id}/file")
                }
            } else {
                String::new()
            };
            let alt_key = format!("{key}_alt");
            let alt = raw_map
                .and_then(|m| m.get(&alt_key))
                .or_else(|| raw_obj.and_then(|m| m.get(&alt_key)))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            target_map.insert(format!("{key}_url"), Value::from(url));
            target_map.insert(alt_key, Value::from(alt.to_owned()));
            target_map.insert(format!("{key}_width"), Value::from(""));
            target_map.insert(format!("{key}_height"), Value::from(""));
            target_map.insert(key, Value::from(id.to_owned()));
        }
        BlockFieldSpec::Url { default, .. } => {
            let raw_u = raw_val
                .and_then(serde_json::Value::as_str)
                .or(default.as_deref())
                .unwrap_or("");
            let safe_u = if !raw_u.is_empty() && valid_block_url(raw_u) {
                raw_u
            } else {
                ""
            };
            target_map.insert(key, Value::from(safe_u.to_owned()));
        }
        BlockFieldSpec::Link { default, .. } => {
            let (raw_u, new_tab, nofollow) = match raw_val {
                Some(serde_json::Value::Object(obj)) => (
                    obj.get("url")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or(""),
                    obj.get("new_tab")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                    obj.get("nofollow")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                ),
                Some(serde_json::Value::String(s)) => (s.as_str(), false, false),
                _ => match default.as_ref() {
                    Some(def) => (def.url.as_str(), def.new_tab, def.nofollow),
                    None => ("", false, false),
                },
            };
            let safe_u = if !raw_u.is_empty() && valid_block_url(raw_u) {
                raw_u
            } else {
                ""
            };
            let target_attr = if new_tab { "_blank" } else { "_self" };
            let rel_attr = match (new_tab, nofollow) {
                (true, true) => "noopener noreferrer nofollow",
                (true, false) => "noopener noreferrer",
                (false, true) => "nofollow",
                (false, false) => "",
            };
            target_map.insert(format!("{key}_url"), Value::from(safe_u.to_owned()));
            target_map.insert(format!("{key}_target"), Value::from(target_attr));
            target_map.insert(format!("{key}_rel"), Value::from(rel_attr));
            target_map.insert(format!("{key}_new_tab"), Value::from(new_tab));
            target_map.insert(format!("{key}_nofollow"), Value::from(nofollow));
            target_map.insert(key, Value::from(safe_u.to_owned()));
        }
        BlockFieldSpec::Group { .. } | BlockFieldSpec::Repeater { .. } => {}
    }
    Ok(())
}

pub(crate) fn extract_spacing_breakpoint_values(
    val: Option<&serde_json::Value>,
    fallback: Option<opendash_core::block_schema::SpacingValue>,
    responsive: bool,
) -> (
    Option<opendash_core::block_schema::SpacingValue>,
    Option<opendash_core::block_schema::SpacingValue>,
    Option<opendash_core::block_schema::SpacingValue>,
) {
    use opendash_core::block_schema::SpacingValue;
    let parse_sv = |v: &serde_json::Value| serde_json::from_value::<SpacingValue>(v.clone()).ok();
    match val.and_then(|v| v.as_object()) {
        Some(obj)
            if obj.contains_key("top")
                || obj.contains_key("right")
                || obj.contains_key("bottom")
                || obj.contains_key("left") =>
        {
            let d = val.and_then(parse_sv).or(fallback);
            (d, None, None)
        }
        Some(obj) => {
            let d = obj.get("desktop").and_then(parse_sv).or(fallback);
            let t = if responsive {
                obj.get("tablet").and_then(parse_sv)
            } else {
                None
            };
            let m = if responsive {
                obj.get("mobile").and_then(parse_sv)
            } else {
                None
            };
            (d, t, m)
        }
        None => (fallback, None, None),
    }
}

pub(crate) fn spacing_field_tailwind_classes(
    val: Option<&serde_json::Value>,
    fallback: Option<opendash_core::block_schema::SpacingValue>,
    responsive: bool,
    unit: opendash_core::block_schema::CssUnit,
) -> Option<Vec<String>> {
    use opendash_core::{
        block_schema::{CssUnit, SpacingValue},
        builder::{BoxSpacing, compile_box_spacing_tailwind},
    };
    if unit != CssUnit::Px {
        return None;
    }
    let to_box = |sv: SpacingValue| -> Option<BoxSpacing> {
        let conv = |opt: Option<f64>| -> Option<Option<i16>> {
            match opt {
                None => Some(None),
                Some(n)
                    if n.is_finite()
                        && n.fract() == 0.0
                        && n >= 0.0
                        && n <= f64::from(i16::MAX) =>
                {
                    Some(Some(n as i16))
                }
                Some(_) => None,
            }
        };
        Some(BoxSpacing {
            top: conv(sv.top)?,
            right: conv(sv.right)?,
            bottom: conv(sv.bottom)?,
            left: conv(sv.left)?,
        })
    };

    let (d_sv, t_sv, m_sv) = extract_spacing_breakpoint_values(val, fallback, responsive);
    let d_box = match d_sv {
        Some(sv) => Some(to_box(sv)?),
        None => None,
    };
    let t_box = match t_sv {
        Some(sv) => Some(to_box(sv)?),
        None => None,
    };
    let m_box = match m_sv {
        Some(sv) => Some(to_box(sv)?),
        None => None,
    };
    compile_box_spacing_tailwind("p", d_box, t_box, m_box, false)
}

fn extract_spacing_css(
    val: Option<&serde_json::Value>,
    fallback: Option<opendash_core::block_schema::SpacingValue>,
    unit: opendash_core::block_schema::CssUnit,
) -> Option<String> {
    let (d_sv, _, _) = extract_spacing_breakpoint_values(val, fallback, false);
    d_sv?.format_css(unit)
}

fn extract_scalar_or_desktop_f64(val: Option<&serde_json::Value>) -> Option<f64> {
    let v = val?;
    v.as_f64()
        .or_else(|| v.as_object()?.get("desktop")?.as_f64())
}

fn extract_scalar_or_desktop_str(val: Option<&serde_json::Value>) -> Option<&str> {
    let v = val?;
    v.as_str()
        .or_else(|| v.as_object()?.get("desktop")?.as_str())
}

/// Renders a validated `block.json` v1 template under the shared request `RenderBudget`
/// (`<= 2_000` fuel per node, `<= 10_000` total fuel, `<= 512 KiB` total HTML) with
/// two-stage `sanitize_richtext` + post-render single-pass `HtmlScanState::Body` nonce replacement.
pub fn render_block_csec_with_budget(
    block_id: &str,
    template_source: &str,
    schema: &opendash_core::block_schema::BlockSchemaV1,
    values: &BTreeMap<String, serde_json::Value>,
    is_preview: bool,
    budget: &mut RenderBudget,
) -> Result<String, &'static str> {
    render_block_csec_with_options_and_budget(
        block_id,
        template_source,
        schema,
        values,
        None,
        is_preview,
        budget,
    )
}

pub fn render_block_csec_with_options_and_budget(
    block_id: &str,
    template_source: &str,
    schema: &opendash_core::block_schema::BlockSchemaV1,
    values: &BTreeMap<String, serde_json::Value>,
    site_options: Option<&BTreeMap<String, serde_json::Value>>,
    is_preview: bool,
    budget: &mut RenderBudget,
) -> Result<String, &'static str> {
    use opendash_core::block_schema::BlockFieldSpec;

    if budget.fuel_remaining == 0 {
        return Err("Đã hết ngân sách fuel render trang.");
    }
    let node_fuel = budget.fuel_remaining.min(MAX_BLOCK_FUEL_PER_NODE);

    let mut ctx_map: BTreeMap<String, Value> = BTreeMap::new();
    let mut nonces: BTreeMap<String, String> = BTreeMap::new();

    for field in schema.flattened_top_fields() {
        match field {
            BlockFieldSpec::Repeater {
                key,
                max_items,
                fields,
                ..
            } => {
                let mut rendered_items = Vec::new();
                if let Some(arr) = values
                    .get(key.as_str())
                    .and_then(serde_json::Value::as_array)
                {
                    for item in arr.iter().take(*max_items) {
                        if let Some(obj) = item.as_object() {
                            let mut item_map: BTreeMap<String, Value> = BTreeMap::new();
                            for sub in fields {
                                populate_leaf_field_context(
                                    sub,
                                    obj.get(sub.key()),
                                    None,
                                    Some(obj),
                                    is_preview,
                                    &mut item_map,
                                    &mut nonces,
                                )?;
                            }
                            rendered_items.push(Value::from_pairs(item_map));
                        }
                    }
                }
                ctx_map.insert(key.clone(), Value::from(rendered_items));
            }
            _ => {
                populate_leaf_field_context(
                    field,
                    values.get(field.key()),
                    Some(values),
                    None,
                    is_preview,
                    &mut ctx_map,
                    &mut nonces,
                )?;
            }
        }
    }

    let mut merged_options = opendash_core::block_schema::global_site_option_defaults();
    if let Some(extra_options) = site_options {
        for (k, v) in extra_options {
            merged_options.insert(k.clone(), v.clone());
        }
    }
    let options_value = Value::from_pairs(
        merged_options
            .into_iter()
            .map(|(k, v)| (k, json_to_safe_value(&v))),
    );
    ctx_map.entry("options".to_owned()).or_insert(options_value);
    ctx_map
        .entry("od_is_preview".to_owned())
        .or_insert_with(|| Value::from(is_preview));

    let mut env = restricted_environment();
    env.set_fuel(Some(node_fuel));
    env.add_template(block_id, template_source)
        .map_err(|_| "Không thể biên dịch template block.")?;
    let captured = env
        .get_template(block_id)
        .map_err(|_| "Không tìm thấy template block.")?
        .render_captured(Value::from_pairs(ctx_map))
        .map_err(|_| "Lỗi thực thi template block (fuel/recursion/undefined).")?;

    let (consumed, _remaining) = captured
        .state()
        .fuel_levels()
        .ok_or("Thiếu bộ theo dõi fuel cho template block.")?;
    budget.fuel_remaining = budget.fuel_remaining.saturating_sub(consumed);

    let shell = captured.output();
    if shell.len() > MAX_RENDERED_HTML_BYTES {
        return Err("HTML render của block vượt quá giới hạn 512 KiB.");
    }
    validate_html_and_css_context(shell, true)?;

    let out = if nonces.is_empty() {
        shell.to_owned()
    } else {
        let nonce_refs: BTreeMap<String, &str> = nonces
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str()))
            .collect();
        insert_preview_fragments(shell, &nonce_refs)?
    };

    budget.charge_html_bytes(out.len())?;
    Ok(out)
}

#[cfg(test)]
mod composition_tests {
    use super::*;

    #[test]
    fn theme_url_option_validation_covers_scalar_boundaries_and_entity_bypasses() {
        for allowed in [
            "/lien-he?from=header#form",
            "https://example.test/path?q=1&x=2",
            "#form",
            "mailto:hello@example.test",
            "tel:+842812345678",
            &format!("/{}", "🦀".repeat(4095)),
        ] {
            assert!(validate_theme_option_url(allowed).is_ok(), "{allowed}");
        }
        for rejected in [
            "",
            " ",
            " /a",
            "/a ",
            "/a b",
            "/a\n",
            "//evil.test",
            "/a\\b",
            "https://user:pass@example.test/",
            "javascript:alert(1)",
            "data:text/html,bad",
            "file:///etc/passwd",
            "vbscript:bad",
            "/a&#10;bad",
            "/a&quot;onmouseover",
            "/a&#x27;bad",
            "/a`bad",
            "http://evil.test@other.test/",
            &format!("/{}", "🦀".repeat(4096)),
        ] {
            assert!(validate_theme_option_url(rejected).is_err(), "{rejected}");
        }
    }

    #[test]
    fn theme_url_capability_is_exact_anchor_only_and_never_inherited_by_blocks() {
        let policy = ThemeUrlPolicy::from_option_keys(["cta_url".to_owned()]);
        let src = r#"<a href="{{ options.cta_url }}">Visit</a>"#;
        assert!(validate_template_csec_with_theme_urls("a", src, &policy).is_ok());
        assert!(validate_template_csec("a", src).is_err());
        let compiled = validate_and_compile_csec_composition_with_theme_urls(
            "root",
            src,
            &BTreeMap::new(),
            CsecContentKind::NonEntry,
            &policy,
        )
        .unwrap();
        let rendered = compiled
            .render(
                json_to_safe_value(&serde_json::json!({
                    "options": {"cta_url":"/lien-he?from=header&x=2"}
                })),
                None,
            )
            .unwrap();
        let rendered_href = rendered
            .split("href=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        assert_eq!(
            decode_html_entities_for_url(rendered_href),
            "/lien-he?from=header&x=2"
        );
        // Even a syntactically permitted anchor must fail the post-render
        // URL boundary when supplied an unsafe runtime/context value.
        assert!(
            compiled
                .render(
                    json_to_safe_value(&serde_json::json!({
                        "options": {"cta_url": "javascript:alert(1)"}
                    })),
                    None,
                )
                .is_err()
        );
        for forbidden in [
            r#"<a href="{{ options.other_url }}">bad</a>"#,
            r#"<a href="{{ theme_options.cta_url }}">bad</a>"#,
            r#"<a href="{{ options.cta_url | upper }}">bad</a>"#,
            r#"<a href="{{ options['cta_url'] }}">bad</a>"#,
            r#"<a href="/prefix{{ options.cta_url }}">bad</a>"#,
            r#"<img src="{{ options.cta_url }}">"#,
            r#"<video poster="{{ options.cta_url }}"></video>"#,
            r#"<cite cite="{{ options.cta_url }}">bad</cite>"#,
            r#"<a ping="{{ options.cta_url }}">bad</a>"#,
            r#"<img srcset="{{ options.cta_url }} 1x">"#,
            r#"<div style="background:{{ options.cta_url }}">bad</div>"#,
            r#"{% set options = {'cta_url':'/evil'} %}<a href="{{ options.cta_url }}">bad</a>"#,
            r#"{% with options = {'cta_url':'/evil'} %}<a href="{{ options.cta_url }}">bad</a>{% endwith %}"#,
            r#"{% for options, n in items %}<a href="{{ options.cta_url }}">bad</a>{% endfor %}"#,
            r#"{% set options %}bad{% endset %}<a href="{{ options.cta_url }}">bad</a>"#,
        ] {
            assert!(
                validate_template_csec_with_theme_urls("root", forbidden, &policy).is_err(),
                "{forbidden}"
            );
        }
        // A nested, dormant part is checked even if the public root never visits it.
        let parts = BTreeMap::from([(
            "dormant".into(),
            r#"<a href="{{ options.not_declared }}">bad</a>"#.into(),
        )]);
        assert!(
            validate_and_compile_csec_composition_with_theme_urls(
                "root",
                "<main>Good</main>",
                &parts,
                CsecContentKind::NonEntry,
                &policy,
            )
            .is_err()
        );
    }

    #[test]
    fn block_csec_global_options_bridge_enforces_typed_urls_and_forbids_shadowing() {
        let schema: opendash_core::block_schema::BlockSchemaV1 =
            serde_json::from_value(serde_json::json!({
                "schema_version": 1,
                "id": "demo/bridge",
                "title": "Bridge Test",
                "category": "content",
                "render": "render.html",
                "fields": [
                    { "type": "boolean", "key": "show_fb", "label": "FB", "default": true }
                ]
            }))
            .unwrap();

        let valid_tpl = r#"{% if show_fb and options.social_facebook %}<a href="{{ options.social_facebook }}">FB</a>{% endif %}{% if options.site_logo %}<img src="{{ options.site_logo }}" alt="Logo">{% endif %}<p>{{ options.contact_phone }}</p>"#;
        assert!(validate_block_template_csec("demo/bridge", valid_tpl, &schema).is_ok());

        for forbidden in [
            r#"<a href="{{ options.contact_phone }}">bad</a>"#,
            r#"<a href="{{ options.unknown_key }}">bad</a>"#,
            r#"<a href="{{ theme_options.social_facebook }}">bad</a>"#,
            r#"<p>{{ options.social_facebook }}</p>"#,
            r#"<p>{{ options }}</p>"#,
            r#"<a href="{{ options.social_facebook | upper }}">bad</a>"#,
            r#"{% set options = {'social_facebook': '/evil'} %}<a href="{{ options.social_facebook }}">bad</a>"#,
            r#"{% with options = {'social_facebook': '/evil'} %}<a href="{{ options.social_facebook }}">bad</a>{% endwith %}"#,
            r#"{% for options, x in [] %}<a href="{{ options.social_facebook }}">bad</a>{% endfor %}"#,
        ] {
            assert!(
                validate_block_template_csec("demo/bridge", forbidden, &schema).is_err(),
                "{forbidden}"
            );
        }
    }

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

    #[test]
    fn block_csec_blocks_html5_comment_smuggling_duplicate_attrs_entity_urls_and_richtext_aliasing()
    {
        use opendash_core::block_schema::BlockSchemaV1;
        use serde_json::json;

        let schema: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "core/test-card",
            "title": "Test Card",
            "category": "content",
            "render": "render.html",
            "fields": [
                { "type": "text", "key": "title", "label": "Title" },
                { "type": "richtext", "key": "body", "label": "Body" },
                { "type": "url", "key": "cta_url", "label": "CTA URL" },
                { "type": "media", "key": "hero", "label": "Hero", "media_kind": "image" },
                {
                    "type": "repeater",
                    "key": "items",
                    "label": "Items",
                    "fields": [
                        { "type": "text", "key": "label", "label": "Label" },
                        { "type": "richtext", "key": "content", "label": "Content" },
                        { "type": "url", "key": "link_url", "label": "Link" },
                        { "type": "media", "key": "thumb", "label": "Thumb", "media_kind": "image" }
                    ]
                }
            ]
        }))
        .unwrap();

        let bad_templates = [
            "<!-- --!><script>alert(1)</script><!-- -->",
            "<!--><script>alert(1)</script><!-- -->",
            "<!---><script>alert(1)</script><!-- -->",
            "<a href=\"javascript:alert(1)\" href=\"https://ok.example\">x</a>",
            "<a href=\"/&sol;evil.com\">x</a>",
            "<a href=\"java&#x09;script:alert(1)\">x</a>",
            "<img src=\"/media/ok.png\" srcset=\"/&sol;evil.com/a.png 1x\">",
            "<a href=\"https://ok.example\" ping=\"/&sol;evil.com/track\">x</a>",
            "<title>bad</title>",
            "<xmp>bad</xmp>",
            "<noembed>bad</noembed>",
            "<noframes>bad</noframes>",
            "<plaintext>bad",
            "{% set rt = body %}<div>{{ rt }}</div>",
            "{% with rt = body %}<div>{{ rt }}</div>{% endwith %}",
            "<div data-x=\"{{ body }}\"></div>",
            "<div>{{ body | upper }}</div>",
            "{% for item in items %}<div title=\"{{ item.content }}\"></div>{% endfor %}",
            "{% for item in items %}{% set x = item.content %}{{ x }}{% endfor %}",
            "<div>{{ items[0].label }}</div>",
            "<a href=\"{{ title }}\">bad non-url field in href</a>",
            "<img src=\"{{ cta_url }}\">",
            "<div>OD_RICHTEXT_0123456789abcdef0123456789abcdef</div>",
        ];

        for bad in bad_templates {
            assert!(
                validate_block_template_csec("core/test-card", bad, &schema).is_err(),
                "expected C-SEC reject for: {bad}"
            );
        }

        // Valid template with top-level & repeater richtext, url, and media_url
        let good_tpl = r#"<section class="card">
            <h2>{{ title }}</h2>
            {% if hero_url %}<img src="{{ hero_url }}" alt="{{ hero_alt }}">{% endif %}
            {% if body %}<div class="body">{{ body }}</div>{% endif %}
            {% if cta_url %}<a href="{{ cta_url }}">Go</a>{% endif %}
            {% for item in items %}
                <div class="item">
                    <span>{{ item.label }}</span>
                    {% if item.thumb_url %}<img src="{{ item.thumb_url }}" alt="">{% endif %}
                    {% if item.content %}<div>{{ item.content }}</div>{% endif %}
                    {% if item.link_url %}<a href="{{ item.link_url }}">More</a>{% endif %}
                </div>
            {% endfor %}
        </section>"#;
        validate_block_template_csec("core/test-card", good_tpl, &schema).unwrap();

        let media_id = "a".repeat(64);
        let values = BTreeMap::from([
            ("title".into(), json!("Chào <script>alert(1)</script>")),
            (
                "body".into(),
                json!(
                    "<p>Rich <strong>text</strong> <script>evil()</script><a href=\"https://opendash.vn\">link</a></p>"
                ),
            ),
            ("cta_url".into(), json!("/lien-he")),
            ("hero".into(), json!(media_id)),
            (
                "items".into(),
                json!([{
                    "label": "Mục 1",
                    "content": "<p>Nội dung <em>mục 1</em></p>",
                    "link_url": "#top",
                    "thumb": media_id
                }]),
            ),
        ]);

        let mut budget = RenderBudget::new();
        let rendered = render_block_csec_with_budget(
            "core/test-card",
            good_tpl,
            &schema,
            &values,
            false,
            &mut budget,
        )
        .unwrap();
        let decoded_rendered = opendash_core::richtext::decode_html5_entities(&rendered);
        assert!(rendered.contains("Chào &lt;script&gt;alert(1)&lt;"));
        assert!(rendered.contains("<p>Rich <strong>text</strong> <a href=\"https://opendash.vn\" rel=\"noopener noreferrer\">link</a></p>"));
        assert!(rendered.contains("<p>Nội dung <em>mục 1</em></p>"));
        assert!(decoded_rendered.contains(&format!("src=\"/media/{media_id}/file\"")));
        assert!(!rendered.contains("OD_RICHTEXT_"));
        assert!(budget.fuel_remaining < TEMPLATE_FUEL_LIMIT);
        assert!(budget.html_bytes_used > 0);

        // Preview mode uses /api/admin/media/{id}/file and validate_safe_scoped_block_css gates it on is_preview
        let preview_css = format!(
            "[data-od-node-scope=\"entry-b1\"]{{--od-bg:url(\"/api/admin/media/{media_id}/file\");}}"
        );
        validate_safe_scoped_block_css(&preview_css, true).unwrap();
        assert!(validate_safe_scoped_block_css(&preview_css, false).is_err());
    }
}
