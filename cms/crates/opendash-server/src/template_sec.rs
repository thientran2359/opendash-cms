//! C-SEC MiniJinja 3.0.5 safe subset, AST allowlist, and contextual HTML/URL/CSS validator.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock, RwLock};

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
/// Mirrors `document::MAX_PUBLIC_DOCUMENT_BYTES` without coupling `template_sec.rs`
/// to `crate::document` (some integration tests include `template_sec.rs` standalone).
pub const MAX_ICON_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;
pub const ICON_NONCE_PREFIX: &str = "OD_ICON_";
pub const MAX_ICONS_PER_REQUEST: usize = 128;
pub const TEMPLATE_FUEL_LIMIT: u64 = 10_000;
pub const TEMPLATE_RECURSION_LIMIT: usize = 16;
const MAX_COMPOSITION_PART_DEPTH: usize = 3;
const MAX_COMPOSITION_EXPANSIONS: usize = 32;

/// Host formatting configuration supplied per request or preview to C-SEC helpers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderFormattingConfig {
    pub timezone: String,
}

#[cfg(test)]
mod trusted_theme_markup_tests {
    use super::*;

    fn theme(source: &str) -> Result<CompiledCsecComposition, &'static str> {
        validate_and_compile_csec_composition(
            "trusted_markup",
            source,
            &BTreeMap::new(),
            CsecContentKind::NonEntry,
        )
    }

    fn block_schema() -> opendash_core::block_schema::BlockSchemaV1 {
        serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "id": "trusted/markup",
            "title": "Trusted markup",
            "category": "content",
            "render": "render.html",
            "fields": [{ "type": "text", "key": "label", "label": "Label" }]
        }))
        .unwrap()
    }

    #[test]
    fn trusted_theme_svg_scripts_events_and_raw_text_survive_compile_and_render() {
        let source = r##"<main onclick="this.dataset.clicked='1'"><svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 20 20"><title>Icon</title><use xlink:href="#symbol"/></svg><script>if (window.innerWidth < 768 && 3 > 1) { document.body.dataset.mobile = '1'; }</script></main>"##;
        let compiled = theme(source).unwrap();
        let output = compiled.render(Value::UNDEFINED, None).unwrap();
        assert_eq!(output, source);

        let nested =
            r#"<svg xmlns="http://www.w3.org/2000/svg"><svg><title>Nested</title></svg></svg>"#;
        assert_eq!(
            theme(nested)
                .unwrap()
                .render(Value::UNDEFINED, None)
                .unwrap(),
            nested
        );
    }

    #[test]
    fn legacy_render_and_validation_stay_strict() {
        for source in [
            "<svg><title>Icon</title></svg>",
            "<script>const a = 1 < 2;</script>",
            "<button onclick=\"go()\">Click</button>",
            "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>",
        ] {
            assert!(
                validate_template_csec("legacy", source).is_err(),
                "{source}"
            );
            assert!(
                render_csec("legacy", source, Value::UNDEFINED).is_err(),
                "{source}"
            );
        }
    }

    #[test]
    fn trusted_theme_rejects_dynamic_js_and_malformed_rawtext() {
        for source in [
            r#"<button onclick="{{ entry.title }}">Click</button>"#,
            r#"<button onclick="{% if active %}go(){% endif %}">Click</button>"#,
            r#"<button onclick="{% raw %}{{ entry.title }}{% endraw %}">Click</button>"#,
            r#"<svg onload="{{ title }}"></svg>"#,
            r#"<script>const value = "{{ entry.title }}";</script>"#,
            r#"<script>{% if cond %}go(){% endif %}</script>"#,
            r#"<script>{% for item in items %}go(){% endfor %}</script>"#,
            r#"<script>{% set a = 'x' %}go()</script>"#,
            r#"<script>{{ od_icon('check') }}</script>"#,
            r#"<script>{{ od_part('header') }}</script>"#,
            "<script>\n<!--\n/*<script>*/\n// </script>\n{{ entry.title }}\n<!-- </script> -->",
            "<svg><script><![CDATA[// </script>\n{{ entry.title }}\n//<!-- ]]> </script> --> </svg>",
            r#"<script src="/theme/app.js"/>"#,
            r#"<script>const n = 2 < 3;"#,
            r#"</script><p>orphan</p>"#,
            r#"<svg><title>Unclosed</title>"#,
            r#"</svg>"#,
            r#"<title>Not an SVG title</title>"#,
            r#"<svg><use xlink:href="javascript:alert(1)"/></svg>"#,
        ] {
            assert!(theme(source).is_err(), "accepted unsafe template: {source}");
        }
    }

    #[test]
    fn block_template_allows_static_js_svg_but_rejects_dynamic_handlers_and_scripts() {
        let schema = block_schema();
        let source = r#"<button onclick="this.dataset.ok='1'">{{ label }}</button><svg xmlns="http://www.w3.org/2000/svg"><title>OK</title></svg><script>if (1 < 2 && 3 > 2) { document.body.dataset.ok = '1'; }</script>"#;
        validate_block_template_csec("trusted/markup", source, &schema).unwrap();
        let mut budget = RenderBudget::new();
        let rendered = render_block_csec_with_budget(
            "trusted/markup",
            source,
            &schema,
            &BTreeMap::from([("label".to_owned(), serde_json::json!("<safe>"))]),
            false,
            &mut budget,
        )
        .unwrap();
        assert!(rendered.contains("onclick=\"this.dataset.ok='1'\""));
        assert!(rendered.contains("&lt;safe&gt;"));
        assert!(rendered.contains("if (1 < 2 && 3 > 2)"));

        for source in [
            r#"<button onclick="{{ label }}">Click</button>"#,
            r#"<button onclick="{% if label %}go(){% endif %}">Click</button>"#,
            r#"<script>const value = "{{ label }}";</script>"#,
            r#"<script>{% if label %}go(){% endif %}</script>"#,
            "<script>\n<!--\n/*<script>*/\n// </script>\n{{ label }}\n<!-- </script> -->",
            "<svg><script><![CDATA[// </script>\n{{ label }}\n//<!-- ]]> </script> --> </svg>",
            r#"<script/>"#,
            r#"</script>"#,
        ] {
            assert!(
                validate_block_template_csec("trusted/markup", source, &schema).is_err(),
                "accepted unsafe block: {source}"
            );
        }
    }

    #[test]
    fn icon_nonce_replacement_never_inserts_svg_in_script_rawtext() {
        let nonce = "OD_ICON_0123456789abcdef0123456789abcdef";
        let table = IconNonceTable {
            entries: vec![(nonce.to_owned(), "<svg/>".to_owned())],
        };
        let source = format!("<script>if (1 < 2 && 3 > 1) {{ console.log('{nonce}'); }}</script>");
        assert!(table.substitute_into_document(source).is_err());
    }
}

impl Default for RenderFormattingConfig {
    fn default() -> Self {
        Self {
            timezone: "UTC".to_owned(),
        }
    }
}

/// Explicit, manifest-derived theme URL and declared-asset permissions.
#[derive(Debug, Clone, Default)]
pub struct ThemeUrlPolicy {
    permitted_href_expressions: BTreeSet<String>,
    permitted_src_expressions: BTreeSet<String>,
    declared_assets: BTreeSet<String>,
}

impl ThemeUrlPolicy {
    pub(crate) fn new(
        permitted_href_expressions: BTreeSet<String>,
        permitted_src_expressions: BTreeSet<String>,
        declared_assets: BTreeSet<String>,
    ) -> Self {
        Self {
            permitted_href_expressions,
            permitted_src_expressions,
            declared_assets,
        }
    }

    pub(crate) fn from_option_keys(keys: impl IntoIterator<Item = String>) -> Self {
        Self::new(
            keys.into_iter()
                .map(|key| format!("options.{key}"))
                .collect(),
            BTreeSet::new(),
            BTreeSet::new(),
        )
    }

    fn allows_cite_href(&self, expr: &str) -> bool {
        self.permitted_href_expressions.contains(expr)
    }

    fn allows_media_src(&self, expr: &str) -> bool {
        self.permitted_src_expressions.contains(expr)
    }

    fn allows_declared_asset(&self, path: &str) -> bool {
        self.declared_assets.contains(path)
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

const MAX_THEME_CACHE_ENTRIES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ThemeCompositionCacheKey {
    pub revision_digest: String,
    pub target_key: String,
    pub content_kind: u8,
}

#[derive(Debug, Default)]
pub struct ThemeCompositionCache {
    inner: RwLock<BTreeMap<ThemeCompositionCacheKey, Arc<CompiledCsecComposition>>>,
}

impl ThemeCompositionCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn global() -> &'static Arc<Self> {
        static GLOBAL: OnceLock<Arc<ThemeCompositionCache>> = OnceLock::new();
        GLOBAL.get_or_init(|| Arc::new(Self::new()))
    }

    pub fn get_or_compile(
        &self,
        revision_digest: &str,
        target_key: &str,
        source: &str,
        parts: &BTreeMap<String, String>,
        content_kind: CsecContentKind,
        policy: &ThemeUrlPolicy,
    ) -> Result<Arc<CompiledCsecComposition>, &'static str> {
        let compile = || {
            validate_and_compile_csec_composition_with_theme_urls(
                target_key,
                source,
                parts,
                content_kind,
                policy,
            )
            .map(Arc::new)
        };
        if revision_digest.len() != 64 || !revision_digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return compile();
        }
        let key = ThemeCompositionCacheKey {
            revision_digest: revision_digest.to_owned(),
            target_key: target_key.to_owned(),
            content_kind: match content_kind {
                CsecContentKind::Entry => 0,
                CsecContentKind::Index => 1,
                CsecContentKind::NonEntry => 2,
            },
        };
        if let Some(found) = self
            .inner
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
            .cloned()
        {
            return Ok(found);
        }
        let compiled = compile()?;
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if let Some(found) = guard.get(&key).cloned() {
            return Ok(found);
        }
        if guard.len() >= MAX_THEME_CACHE_ENTRIES {
            guard.clear();
        }
        guard.insert(key, compiled.clone());
        Ok(compiled)
    }
}

/// Validated theme v3 template, flattened into a single MiniJinja program.
/// This ensures every part shares the same context, fuel and output budget.
#[derive(Debug, Clone)]
pub struct CompiledCsecComposition {
    source: String,
    name: String,
    content_count: usize,
    part_expansions: usize,
    pub max_part_depth: usize,
    pub uses_related_entries: bool,
    pub uses_recent_posts: bool,
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
        self.render_with_config_and_remaining_fuel(
            context,
            sanitized_content_html,
            fuel_remaining,
            &RenderFormattingConfig::default(),
        )
    }

    /// Renders the compiled theme shell with host-configured date/timezone formatting
    /// while sharing the request-local fuel budget.
    pub(crate) fn render_with_config_and_remaining_fuel(
        &self,
        context: Value,
        sanitized_content_html: Option<&str>,
        fuel_remaining: &mut u64,
        fmt_cfg: &RenderFormattingConfig,
    ) -> Result<String, &'static str> {
        let mut budget = RenderBudget::new();
        budget.fuel_remaining = *fuel_remaining;
        let result = self.render_with_config_and_budget(
            context,
            sanitized_content_html,
            &mut budget,
            fmt_cfg,
        );
        *fuel_remaining = budget.fuel_remaining;
        result
    }

    pub(crate) fn render_with_config_and_budget(
        &self,
        context: Value,
        sanitized_content_html: Option<&str>,
        budget: &mut RenderBudget,
        fmt_cfg: &RenderFormattingConfig,
    ) -> Result<String, &'static str> {
        if budget.fuel_remaining == 0 || budget.fuel_remaining > TEMPLATE_FUEL_LIMIT {
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
        let mut env = restricted_environment_with_config(fmt_cfg);
        let local = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let local_hooks = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let overflowed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        install_icon_function(
            &mut env,
            &budget.icon_nonces,
            &local,
            budget.html_bytes_used,
            &overflowed,
        );
        install_hook_function(&mut env, &budget.hook_nonces, &local_hooks);
        env.set_fuel(Some(budget.fuel_remaining));
        env.add_template(&self.name, &source)
            .map_err(|_| "Không thể biên dịch theme composition.")?;
        let captured = env
            .get_template(&self.name)
            .map_err(|_| "Không tìm thấy theme composition.")?
            .render_captured(context)
            .map_err(|_| {
                if overflowed.load(std::sync::atomic::Ordering::Relaxed) {
                    budget.html_bytes_used = MAX_RENDERED_HTML_BYTES + 1;
                }
                "Lỗi thực thi theme composition (fuel/recursion/undefined)."
            })?;
        let (consumed, remaining) = captured
            .state()
            .fuel_levels()
            .ok_or("Thiếu bộ theo dõi fuel composition.")?;
        if budget.fuel_remaining.checked_sub(consumed) != Some(remaining) {
            return Err("Fuel composition không hợp lệ.");
        }
        budget.fuel_remaining = remaining;
        let shell = captured.output();
        if shell.len() > MAX_RENDERED_HTML_BYTES {
            return Err("HTML render vượt quá giới hạn 512 KiB.");
        }
        validate_html_and_css_context(&shell, true, true)?;
        let out = if let Some(nonce) = nonce {
            if shell.matches(&nonce).count() != 1 {
                return Err("Nonce nội dung của theme không xuất hiện đúng một lần.");
            }
            let content = sanitized_content_html.unwrap_or("");
            let final_len = shell.len() - nonce.len() + content.len();
            if final_len > MAX_RENDERED_HTML_BYTES {
                return Err("HTML sau khi chèn nội dung vượt quá giới hạn 512 KiB.");
            }
            shell.replacen(&nonce, content, 1)
        } else {
            shell.to_owned()
        };
        let additions = local.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let icon_growth: usize = additions
            .iter()
            .map(|(n, s)| s.len().saturating_sub(n.len()))
            .sum();
        budget.charge_html_bytes(icon_growth)?;
        budget.icon_nonces.extend(additions);
        let hook_additions = local_hooks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        budget.hook_nonces.extend(hook_additions);
        Ok(out)
    }
}

// A compiler-only marker. It is never passed to MiniJinja as a callable or
// trusted HTML variable. A fresh 128-bit random value replaces it per render.
const CONTENT_SLOT_TOKEN: &str = "OD_CONTENT_SLOT_COMPILE_MARKER";

// Reserved only by the preview compiler, never a template variable or helper.
const PREVIEW_PART_NONCE_PREFIX: &str = "OD_PREVIEW_PART_";

// Reserved only by the block RichText post-render nonce replacer.
pub const BLOCK_RICHTEXT_NONCE_PREFIX: &str = "OD_RICHTEXT_";

#[derive(Debug, Clone, Default)]
pub struct IconNonceTable {
    entries: Vec<(String, String)>,
}

impl IconNonceTable {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn extend(&mut self, entries: Vec<(String, String)>) {
        self.entries.extend(entries);
    }

    /// Substitute server-generated SVG only after document normalization. Any
    /// nonce that disappeared during slot fallback is intentionally ignored.
    pub fn substitute_into_document(&self, html: String) -> Result<String, &'static str> {
        if self.entries.is_empty() {
            return Ok(html);
        }
        let mut replacements = Vec::new();
        for (nonce, svg) in &self.entries {
            let mut matches = html.match_indices(nonce);
            if let Some((offset, _)) = matches.next() {
                if matches.next().is_some() {
                    return Err("Nonce icon xuất hiện nhiều hơn một lần.");
                }
                replacements.push((offset, nonce.as_str(), svg.as_str()));
            }
        }
        replacements.sort_unstable_by_key(|(offset, _, _)| *offset);
        let mut out = String::with_capacity(html.len());
        let mut state = HtmlScanState::Body;
        let mut cursor = 0;
        let mut in_diagnostic = false;
        let mut diagnostic_pre = false;
        for (offset, nonce, svg) in replacements {
            let before = html
                .get(cursor..offset)
                .ok_or("Vị trí nonce icon không hợp lệ.")?;
            for (index, byte) in before.bytes().enumerate() {
                // The normalizer's diagnostic markup is server-owned. A nonce
                // printed as escaped diagnostic text must never become SVG.
                let suffix = &before.as_bytes()[index..];
                if state == HtmlScanState::Body && suffix.starts_with(b"<main data-doc-diagnostic=")
                {
                    in_diagnostic = true;
                } else if state == HtmlScanState::Body && suffix.starts_with(b"</main>") {
                    in_diagnostic = false;
                    diagnostic_pre = false;
                } else if state == HtmlScanState::Body
                    && in_diagnostic
                    && suffix.starts_with(b"<pre")
                {
                    diagnostic_pre = true;
                } else if state == HtmlScanState::Body && suffix.starts_with(b"</pre>") {
                    diagnostic_pre = false;
                }
                state = state.consume(byte);
            }
            if state != HtmlScanState::Body || diagnostic_pre || in_diagnostic {
                return Err("Nonce icon không nằm trong thân HTML an toàn.");
            }
            out.push_str(before);
            out.push_str(svg);
            cursor = offset + nonce.len();
            if out.len() > MAX_ICON_DOCUMENT_BYTES {
                return Err("Document chứa icon vượt giới hạn HTML.");
            }
        }
        out.push_str(&html[cursor..]);
        if out.len() > MAX_ICON_DOCUMENT_BYTES {
            return Err("Document chứa icon vượt giới hạn HTML.");
        }
        Ok(out)
    }
}

fn new_icon_nonce() -> Result<String, &'static str> {
    let mut bytes = [0_u8; 16];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| "Không tạo được nonce icon.")?;
    let mut nonce = ICON_NONCE_PREFIX.to_owned();
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut nonce, "{byte:02x}").map_err(|_| "Không tạo được nonce icon.")?;
    }
    Ok(nonce)
}

fn install_icon_function(
    env: &mut Environment<'_>,
    existing: &IconNonceTable,
    local: &std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>,
    html_bytes_used: usize,
    overflowed: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    let existing_count = existing.entries.len();
    let local = local.clone();
    let overflowed = overflowed.clone();
    env.add_function(
        "od_icon",
        move |name: &str, class: Option<&str>| -> Result<String, minijinja::Error> {
            let svg = opendash_core::icons::render_core_icon_svg(name, class)
                .ok_or_else(|| filter_invalid_op("od_icon: tên icon hoặc class không hợp lệ"))?;
            let mut table = local.lock().unwrap_or_else(|e| e.into_inner());
            if existing_count.saturating_add(table.len()) >= MAX_ICONS_PER_REQUEST {
                return Err(filter_invalid_op("od_icon vượt quá 128 icon/request"));
            }
            let nonce = new_icon_nonce().map_err(filter_invalid_op)?;
            let local_growth: usize = table
                .iter()
                .map(|(n, s)| s.len().saturating_sub(n.len()))
                .sum();
            if html_bytes_used
                .saturating_add(local_growth)
                .saturating_add(svg.len().saturating_sub(nonce.len()))
                > MAX_RENDERED_HTML_BYTES
            {
                overflowed.store(true, std::sync::atomic::Ordering::Relaxed);
                return Err(filter_invalid_op("od_icon vượt giới hạn HTML 512 KiB"));
            }
            table.push((nonce.clone(), svg));
            Ok(nonce)
        },
    );
}

pub const HOOK_NONCE_PREFIX: &str = "OD_HOOK_";
pub const MAX_HOOKS_PER_REQUEST: usize = 32;
pub const MAX_HOOK_ARGS_JSON_BYTES: usize = 16 * 1024;

pub fn valid_hook_slot_name(slot: &str) -> bool {
    !slot.is_empty()
        && slot.len() <= 64
        && slot
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-'))
}

#[derive(Debug, Clone)]
pub struct HookSlotInvocation {
    pub nonce: String,
    pub slot: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Default)]
pub struct HookNonceTable {
    entries: Vec<HookSlotInvocation>,
}

impl HookNonceTable {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[HookSlotInvocation] {
        &self.entries
    }

    pub fn extend(&mut self, additions: Vec<HookSlotInvocation>) {
        self.entries.extend(additions);
    }
}

fn new_hook_nonce() -> Result<String, &'static str> {
    let mut bytes = [0_u8; 16];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| "Không tạo được nonce hook.")?;
    let mut nonce = HOOK_NONCE_PREFIX.to_owned();
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut nonce, "{byte:02x}").map_err(|_| "Không tạo được nonce hook.")?;
    }
    Ok(nonce)
}

fn minijinja_value_to_json(
    val: &Value,
    depth: usize,
) -> Result<serde_json::Value, minijinja::Error> {
    use minijinja::value::ValueKind;
    if depth > 12 {
        return Err(filter_invalid_op("od_hook: tham số lồng quá sâu"));
    }
    match val.kind() {
        ValueKind::Undefined | ValueKind::None => Ok(serde_json::Value::Null),
        ValueKind::Bool => Ok(serde_json::Value::Bool(val.is_true())),
        ValueKind::Number => {
            if let Ok(i) = i64::try_from(val.clone()) {
                Ok(serde_json::Value::Number(i.into()))
            } else if let Ok(u) = u64::try_from(val.clone()) {
                Ok(serde_json::Value::Number(u.into()))
            } else if let Ok(f) = f64::try_from(val.clone()) {
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .ok_or_else(|| filter_invalid_op("od_hook: số không hợp lệ trong tham số"))
            } else {
                Err(filter_invalid_op("od_hook: số không hợp lệ trong tham số"))
            }
        }
        ValueKind::String => Ok(serde_json::Value::String(
            val.as_str().unwrap_or_default().to_owned(),
        )),
        ValueKind::Seq | ValueKind::Iterable => {
            let mut items = Vec::new();
            for item in val.try_iter()? {
                items.push(minijinja_value_to_json(&item, depth + 1)?);
            }
            Ok(serde_json::Value::Array(items))
        }
        ValueKind::Map => {
            let mut map = serde_json::Map::new();
            for key in val.try_iter()? {
                let k = key
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| key.to_string());
                let v = val.get_item(&key)?;
                map.insert(k, minijinja_value_to_json(&v, depth + 1)?);
            }
            Ok(serde_json::Value::Object(map))
        }
        _ => Ok(serde_json::Value::String(val.to_string())),
    }
}

fn install_hook_function(
    env: &mut Environment<'_>,
    existing: &HookNonceTable,
    local: &std::sync::Arc<std::sync::Mutex<Vec<HookSlotInvocation>>>,
) {
    let existing_count = existing.entries.len();
    let local = local.clone();
    env.add_function(
        "od_hook",
        move |slot: &str, args: Option<Value>| -> Result<String, minijinja::Error> {
            if !valid_hook_slot_name(slot) {
                return Err(filter_invalid_op("od_hook: tên slot không hợp lệ"));
            }
            let args_json = match args {
                None => serde_json::Value::Null,
                Some(v) if v.is_undefined() || v.is_none() => serde_json::Value::Null,
                Some(v) => minijinja_value_to_json(&v, 0)?,
            };
            if serde_json::to_vec(&args_json)
                .map(|b| b.len())
                .unwrap_or(usize::MAX)
                > MAX_HOOK_ARGS_JSON_BYTES
            {
                return Err(filter_invalid_op("od_hook: tham số vượt quá 16 KiB"));
            }
            let mut table = local.lock().unwrap_or_else(|e| e.into_inner());
            if existing_count.saturating_add(table.len()) >= MAX_HOOKS_PER_REQUEST {
                return Err(filter_invalid_op("od_hook vượt quá 32 slot/request"));
            }
            let nonce = new_hook_nonce().map_err(filter_invalid_op)?;
            table.push(HookSlotInvocation {
                nonce: nonce.clone(),
                slot: slot.to_owned(),
                args: args_json,
            });
            Ok(nonce)
        },
    );
}

// Per-node fuel cap within the shared 10,000 request RenderBudget.
pub const MAX_BLOCK_FUEL_PER_NODE: u64 = 2_000;

#[derive(Debug, Clone)]
pub struct RenderBudget {
    pub fuel_remaining: u64,
    pub html_bytes_used: usize,
    pub locale: String,
    pub icon_nonces: IconNonceTable,
    pub hook_nonces: HookNonceTable,
}

impl Default for RenderBudget {
    fn default() -> Self {
        Self {
            fuel_remaining: TEMPLATE_FUEL_LIMIT,
            html_bytes_used: 0,
            locale: "vi".to_owned(),
            icon_nonces: IconNonceTable::default(),
            hook_nonces: HookNonceTable::default(),
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
    pub icon_nonces: IconNonceTable,
    pub hook_nonces: HookNonceTable,
}

impl Default for PreviewCompositionBudget {
    fn default() -> Self {
        Self {
            fuel_remaining: TEMPLATE_FUEL_LIMIT,
            html_bytes_used: 0,
            expansions: 0,
            stack: Vec::new(),
            icon_nonces: IconNonceTable::default(),
            hook_nonces: HookNonceTable::default(),
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
            icon_nonces: rb.icon_nonces,
            hook_nonces: rb.hook_nonces,
        }
    }

    pub(crate) fn as_render_budget(&self) -> RenderBudget {
        RenderBudget {
            fuel_remaining: self.fuel_remaining,
            html_bytes_used: self.html_bytes_used,
            locale: "vi".to_owned(),
            icon_nonces: self.icon_nonces.clone(),
            hook_nonces: self.hook_nonces.clone(),
        }
    }

    pub(crate) fn sync_from_render_budget(&mut self, rb: &RenderBudget) {
        self.fuel_remaining = rb.fuel_remaining;
        self.html_bytes_used = rb.html_bytes_used;
        self.icon_nonces = rb.icon_nonces.clone();
        self.hook_nonces = rb.hook_nonces.clone();
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

fn preview_composition_sites(
    name: &str,
    source: &str,
    policy: &ThemeUrlPolicy,
) -> Result<Vec<CompositionSite>, &'static str> {
    if source.len() > MAX_RENDERED_HTML_BYTES {
        return Err("Template hoặc part vượt giới hạn 512 KiB.");
    }
    if source.contains(PREVIEW_PART_NONCE_PREFIX)
        || source.contains(ICON_NONCE_PREFIX)
        || source.contains(HOOK_NONCE_PREFIX)
    {
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
    render_preview_csec_fragment_with_theme_urls_and_config(
        name,
        source,
        context,
        theme_sources,
        trusted_builder_parts,
        budget,
        policy,
        &RenderFormattingConfig::default(),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_preview_csec_fragment_with_theme_urls_and_config(
    name: &str,
    source: &str,
    context: Value,
    theme_sources: &BTreeMap<String, String>,
    trusted_builder_parts: &BTreeMap<String, String>,
    budget: &mut PreviewCompositionBudget,
    policy: &ThemeUrlPolicy,
    fmt_cfg: &RenderFormattingConfig,
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
    validate_template_csec_with_usage_flags(name, &program, policy, true, true)?;
    let mut env = restricted_environment_with_config(fmt_cfg);
    let local_icons = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let local_hooks = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let overflowed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    install_icon_function(
        &mut env,
        &budget.icon_nonces,
        &local_icons,
        budget.html_bytes_used,
        &overflowed,
    );
    install_hook_function(&mut env, &budget.hook_nonces, &local_hooks);
    env.set_fuel(Some(budget.fuel_remaining));
    env.add_template(name, &program)
        .map_err(|_| "Không thể biên dịch theme preview.")?;
    let captured = env
        .get_template(name)
        .map_err(|_| "Không tìm thấy theme preview.")?
        .render_captured(context)
        .map_err(|_| {
            if overflowed.load(std::sync::atomic::Ordering::Relaxed) {
                budget.html_bytes_used = MAX_RENDERED_HTML_BYTES + 1;
            }
            "Lỗi thực thi theme preview (fuel/recursion/undefined)."
        })?;
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
    validate_html_and_css_context(shell, true, true)?;
    let out = insert_preview_fragments(shell, &compiler.nonces)?;
    let additions = local_icons
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let icon_growth: usize = additions
        .iter()
        .map(|(n, s)| s.len().saturating_sub(n.len()))
        .sum();
    budget.html_bytes_used = budget.html_bytes_used.saturating_add(icon_growth);
    if budget.html_bytes_used > MAX_RENDERED_HTML_BYTES {
        return Err("HTML preview chứa icon vượt giới hạn 512 KiB.");
    }
    budget.icon_nonces.extend(additions);
    let hook_additions = local_hooks
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    budget.hook_nonces.extend(hook_additions);
    Ok(out)
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

const MAX_FILTER_STRING_BYTES: usize = 128 * 1024;
const MAX_FILTER_SEQ_ITEMS: usize = 500;

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
    "date_format",
    "truncate",
    "format_number",
    "batch",
    "selectattr",
    "rejectattr",
    "map",
];

const UNARY_ALLOWED_TESTS: &[&str] = &[
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

const BINARY_ALLOWED_TESTS: &[&str] = &[
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

const ALLOWED_TESTS: &[&str] = &[
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

fn filter_default(
    state: &minijinja::State,
    value: &Value,
    args: minijinja::value::Rest<Value>,
) -> Result<Value, minijinja::Error> {
    if value.is_none() {
        if args.len() > 2 {
            return Err(minijinja::Error::new(
                minijinja::ErrorKind::TooManyArguments,
                "too many arguments to default filter",
            ));
        }
        Ok(args.first().cloned().unwrap_or_else(|| Value::from("")))
    } else {
        minijinja::filters::default(state, value, args)
    }
}

fn filter_invalid_op(msg: &'static str) -> minijinja::Error {
    minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, msg)
}

fn filter_date_format(
    value: Value,
    fmt: Option<&str>,
    tz: Option<&str>,
    default_tz: &jiff::tz::TimeZone,
) -> Result<Value, minijinja::Error> {
    if value.is_undefined() || value.is_none() {
        return Ok(Value::from(""));
    }
    if value.kind() != minijinja::value::ValueKind::String {
        return Err(filter_invalid_op("date_format requires a string value"));
    }
    let raw = value
        .as_str()
        .ok_or_else(|| filter_invalid_op("date_format requires a string value"))?;
    if raw.len() > 128 {
        return Err(filter_invalid_op("date_format input exceeds 128 bytes"));
    }
    let s = raw.trim();
    if s.is_empty() {
        return Ok(Value::from(""));
    }
    let fmt = match fmt {
        Some("") => return Err(filter_invalid_op("date_format format cannot be empty")),
        Some(f) if f.len() > 64 => {
            return Err(filter_invalid_op("date_format format exceeds 64 bytes"));
        }
        Some(f) => f,
        None => "%d/%m/%Y",
    };
    let target_tz = match tz {
        Some(t) => {
            if t.len() > 64 {
                return Err(filter_invalid_op("date_format timezone exceeds 64 bytes"));
            }
            let trimmed_tz = t.trim();
            if trimmed_tz.is_empty() {
                default_tz.clone()
            } else {
                jiff::tz::TimeZone::get(trimmed_tz)
                    .map_err(|_| filter_invalid_op("date_format invalid timezone"))?
            }
        }
        None => default_tz.clone(),
    };

    if s.len() == 10
        && s.is_ascii()
        && let Ok(date) = jiff::civil::Date::strptime("%Y-%m-%d", s)
    {
        let dt = date.at(0, 0, 0, 0);
        let formatted = jiff::fmt::strtime::format(fmt, dt)
            .map_err(|_| filter_invalid_op("date_format invalid format for civil date"))?;
        return Ok(Value::from(formatted));
    }
    if let Ok(ts) = s.parse::<jiff::Timestamp>() {
        let zdt = ts.to_zoned_datetime(target_tz);
        let formatted = jiff::fmt::strtime::format(fmt, &zdt)
            .map_err(|_| filter_invalid_op("date_format failed to format timestamp"))?;
        return Ok(Value::from(formatted));
    }
    if let Ok(naive) = s.parse::<jiff::civil::DateTime>() {
        let utc_zdt = naive
            .to_zoned_datetime(jiff::tz::TimeZone::UTC)
            .map_err(|_| filter_invalid_op("date_format failed to normalize naive datetime"))?;
        let zdt = utc_zdt.with_time_zone(target_tz);
        let formatted = jiff::fmt::strtime::format(fmt, &zdt)
            .map_err(|_| filter_invalid_op("date_format failed to format datetime"))?;
        return Ok(Value::from(formatted));
    }
    Ok(Value::from(""))
}

fn filter_truncate(
    value: Value,
    max_chars: Option<i64>,
    suffix: Option<&str>,
) -> Result<Value, minijinja::Error> {
    if value.is_undefined() || value.is_none() {
        return Ok(Value::from(""));
    }
    if value.kind() != minijinja::value::ValueKind::String {
        return Err(filter_invalid_op("truncate requires a string value"));
    }
    let s = value
        .as_str()
        .ok_or_else(|| filter_invalid_op("truncate requires a string value"))?;
    if s.len() > MAX_FILTER_STRING_BYTES {
        return Err(filter_invalid_op("truncate input exceeds 128 KiB"));
    }
    let limit = match max_chars {
        Some(n) if n <= 0 => return Err(filter_invalid_op("truncate length must be positive")),
        Some(n) => (n as usize).min(10_000),
        None => 160,
    };
    let suffix = suffix.unwrap_or("...");
    if suffix.chars().count() > 32 {
        return Err(filter_invalid_op("truncate suffix exceeds 32 chars"));
    }
    let half = limit / 2;
    let mut hard_end = None;
    let mut last_ws_after_half = None;
    for (count, (byte_idx, ch)) in s.char_indices().enumerate() {
        if count == limit {
            hard_end = Some(byte_idx);
            break;
        }
        if ch.is_whitespace() && count >= half {
            last_ws_after_half = Some(byte_idx);
        }
    }
    let Some(hard_end) = hard_end else {
        return Ok(Value::from(s));
    };
    let cut = last_ws_after_half.unwrap_or(hard_end);
    let prefix = s[..cut].trim_end();
    let mut out = String::with_capacity(prefix.len() + suffix.len());
    out.push_str(prefix);
    out.push_str(suffix);
    Ok(Value::from(out))
}

fn group_thousands_and_join(
    negative: bool,
    int_part: &str,
    frac_part: &str,
    thousands_sep: &str,
    decimal_sep: &str,
) -> String {
    let mut out = String::with_capacity(
        usize::from(negative)
            + int_part.len()
            + (int_part.len() / 3) * thousands_sep.len()
            + decimal_sep.len()
            + frac_part.len(),
    );
    if negative {
        out.push('-');
    }
    for (idx, chunk) in int_part.as_bytes().rchunks(3).rev().enumerate() {
        if idx > 0 {
            out.push_str(thousands_sep);
        }
        out.push_str(std::str::from_utf8(chunk).unwrap_or(""));
    }
    if !frac_part.is_empty() {
        out.push_str(decimal_sep);
        out.push_str(frac_part);
    }
    out
}

fn filter_format_number(
    value: Value,
    decimals: Option<i64>,
    thousands_sep: Option<&str>,
    decimal_sep: Option<&str>,
) -> Result<Value, minijinja::Error> {
    if value.is_undefined() || value.is_none() {
        return Ok(Value::from(""));
    }
    if matches!(
        value.kind(),
        minijinja::value::ValueKind::Bool
            | minijinja::value::ValueKind::Seq
            | minijinja::value::ValueKind::Map
            | minijinja::value::ValueKind::Bytes
    ) {
        return Err(filter_invalid_op("format_number requires number or string"));
    }
    let dec = decimals.unwrap_or(0);
    if !(0..=6).contains(&dec) {
        return Err(filter_invalid_op("format_number decimals must be 0..=6"));
    }
    let thousands_sep = thousands_sep.unwrap_or(".");
    let decimal_sep = decimal_sep.unwrap_or(",");
    if thousands_sep.chars().count() > 4 || decimal_sep.chars().count() > 4 {
        return Err(filter_invalid_op("format_number separator exceeds 4 chars"));
    }
    let dec_usize = dec as usize;

    enum ParsedNum {
        Int(i128),
        Float(f64),
        NonNumeric,
    }

    let parsed = match value.kind() {
        minijinja::value::ValueKind::Number => {
            if value.is_integer() {
                if let Ok(i) = i128::try_from(value.clone()) {
                    ParsedNum::Int(i)
                } else {
                    ParsedNum::Float(f64::try_from(value)?)
                }
            } else {
                ParsedNum::Float(f64::try_from(value)?)
            }
        }
        minijinja::value::ValueKind::String => {
            let s = value.as_str().unwrap_or("");
            if s.len() > 64 {
                return Err(filter_invalid_op("format_number string exceeds 64 bytes"));
            }
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return Ok(Value::from(""));
            }
            if !trimmed.contains('.') && !trimmed.contains(['e', 'E']) {
                if let Ok(i) = trimmed.parse::<i128>() {
                    ParsedNum::Int(i)
                } else if let Ok(f) = trimmed.parse::<f64>() {
                    ParsedNum::Float(f)
                } else {
                    ParsedNum::NonNumeric
                }
            } else if let Ok(f) = trimmed.parse::<f64>() {
                ParsedNum::Float(f)
            } else {
                ParsedNum::NonNumeric
            }
        }
        _ => return Err(filter_invalid_op("format_number unsupported value type")),
    };

    match parsed {
        ParsedNum::NonNumeric => Ok(Value::from("")),
        ParsedNum::Int(i) => {
            let abs_digits = i.unsigned_abs().to_string();
            if abs_digits.len() > 32 {
                return Err(filter_invalid_op("format_number integer exceeds 32 digits"));
            }
            let frac = if dec_usize > 0 {
                "0".repeat(dec_usize)
            } else {
                String::new()
            };
            let negative = i < 0;
            Ok(Value::from(group_thousands_and_join(
                negative,
                &abs_digits,
                &frac,
                thousands_sep,
                decimal_sep,
            )))
        }
        ParsedNum::Float(num) => {
            if !num.is_finite() {
                return Err(filter_invalid_op("format_number value must be finite"));
            }
            let formatted = format!("{:.*}", dec_usize, num.abs());
            let (int_part, frac_part) = formatted.split_once('.').unwrap_or((&formatted, ""));
            if int_part.len() > 32 {
                return Err(filter_invalid_op(
                    "format_number integer part exceeds 32 digits",
                ));
            }
            let negative = num.is_sign_negative()
                && (int_part.bytes().any(|b| b != b'0') || frac_part.bytes().any(|b| b != b'0'));
            Ok(Value::from(group_thousands_and_join(
                negative,
                int_part,
                frac_part,
                thousands_sep,
                decimal_sep,
            )))
        }
    }
}

fn read_bounded_sequence(value: &Value, name: &str) -> Result<Vec<Value>, minijinja::Error> {
    if value.kind() != minijinja::value::ValueKind::Seq {
        return Err(filter_invalid_op(if name == "batch" {
            "batch requires a sequence"
        } else {
            "sequence filter requires a sequence"
        }));
    }
    let iter = value.try_iter()?;
    let mut items = Vec::new();
    for item in iter.take(MAX_FILTER_SEQ_ITEMS + 1) {
        if items.len() == MAX_FILTER_SEQ_ITEMS {
            return Err(filter_invalid_op("sequence exceeds 500 items limit"));
        }
        items.push(item);
    }
    Ok(items)
}

fn filter_batch(
    value: Value,
    size: i64,
    extra: minijinja::value::Rest<Value>,
) -> Result<Value, minijinja::Error> {
    if value.is_undefined() || value.is_none() {
        return Ok(Value::from(Vec::<Value>::new()));
    }
    if extra.len() > 1 {
        return Err(filter_invalid_op(
            "batch accepts at most 1 extra fill argument",
        ));
    }
    if size <= 0 || size > 100 {
        return Err(filter_invalid_op("batch size must be in 1..=100"));
    }
    let items = read_bounded_sequence(&value, "batch")?;
    let fill = if extra.len() == 1 && !extra[0].is_undefined() {
        Some(extra[0].clone())
    } else {
        None
    };
    let chunk_size = size as usize;
    let mut chunks = Vec::with_capacity(items.len().div_ceil(chunk_size));
    for slice in items.chunks(chunk_size) {
        let mut chunk = slice.to_vec();
        if chunk.len() < chunk_size
            && let Some(pad) = &fill
        {
            while chunk.len() < chunk_size {
                chunk.push(pad.clone());
            }
        }
        chunks.push(Value::from(chunk));
    }
    Ok(Value::from(chunks))
}

fn parse_validated_attr_path(attr: &str) -> Result<Vec<&str>, minijinja::Error> {
    let segments: Vec<&str> = attr.split('.').collect();
    if !(1..=3).contains(&segments.len()) {
        return Err(filter_invalid_op("attribute path must have 1..=3 segments"));
    }
    for seg in &segments {
        if seg.is_empty()
            || seg.len() > 64
            || seg.starts_with("__")
            || !seg.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(filter_invalid_op("invalid attribute path segment"));
        }
    }
    Ok(segments)
}

fn resolve_attr_path(item: &Value, segments: &[&str]) -> Value {
    let mut cur = item.clone();
    for seg in segments {
        if cur.kind() != minijinja::value::ValueKind::Map {
            return Value::UNDEFINED;
        }
        match cur.get_attr(seg) {
            Ok(next) => cur = next,
            Err(_) => return Value::UNDEFINED,
        }
    }
    cur
}

fn eval_attr_test_predicate(
    state: &minijinja::State,
    attr_val: &Value,
    test_name: Option<&str>,
    rhs: Option<&Value>,
) -> Result<bool, minijinja::Error> {
    let Some(test_name) = test_name else {
        return Ok(attr_val.is_true());
    };
    match rhs {
        None => Ok(match test_name {
            "defined" => minijinja::tests::is_defined(attr_val),
            "undefined" => minijinja::tests::is_undefined(attr_val),
            "even" => minijinja::tests::is_even(attr_val.clone()),
            "odd" => minijinja::tests::is_odd(attr_val.clone()),
            "number" => minijinja::tests::is_number(attr_val),
            "string" => minijinja::tests::is_string(attr_val),
            "sequence" => minijinja::tests::is_sequence(attr_val),
            "mapping" => minijinja::tests::is_mapping(attr_val),
            "none" => minijinja::tests::is_none(attr_val),
            "true" => minijinja::tests::is_true(attr_val),
            "false" => minijinja::tests::is_false(attr_val),
            _ => return Err(filter_invalid_op("disallowed unary test")),
        }),
        Some(rhs) => match test_name {
            "eq" | "equalto" => Ok(minijinja::tests::is_eq(attr_val, rhs)),
            "ne" => Ok(minijinja::tests::is_ne(attr_val, rhs)),
            "lt" => Ok(minijinja::tests::is_lt(attr_val, rhs)),
            "le" => Ok(minijinja::tests::is_le(attr_val, rhs)),
            "gt" => Ok(minijinja::tests::is_gt(attr_val, rhs)),
            "ge" => Ok(minijinja::tests::is_ge(attr_val, rhs)),
            "in" => minijinja::tests::is_in(state, attr_val, rhs),
            "startingwith" => {
                let prefix = rhs
                    .as_str()
                    .ok_or_else(|| filter_invalid_op("startingwith requires string operand"))?;
                Ok(attr_val.as_str().is_some_and(|s| {
                    minijinja::tests::is_startingwith(
                        std::borrow::Cow::Borrowed(s),
                        std::borrow::Cow::Borrowed(prefix),
                    )
                }))
            }
            "endingwith" => {
                let suffix = rhs
                    .as_str()
                    .ok_or_else(|| filter_invalid_op("endingwith requires string operand"))?;
                Ok(attr_val.as_str().is_some_and(|s| {
                    minijinja::tests::is_endingwith(
                        std::borrow::Cow::Borrowed(s),
                        std::borrow::Cow::Borrowed(suffix),
                    )
                }))
            }
            _ => Err(filter_invalid_op("disallowed binary test")),
        },
    }
}

fn filter_sequence_by_attr(
    state: &minijinja::State,
    value: Value,
    attr: &str,
    extra: minijinja::value::Rest<Value>,
    invert: bool,
) -> Result<Value, minijinja::Error> {
    if value.is_undefined() || value.is_none() {
        return Ok(Value::from(Vec::<Value>::new()));
    }
    let segments = parse_validated_attr_path(attr)?;
    let (test_name, rhs): (Option<&str>, Option<&Value>) = match extra.len() {
        0 => (None, None),
        1 => {
            let name = extra[0]
                .as_str()
                .ok_or_else(|| filter_invalid_op("test name must be a string"))?;
            if !UNARY_ALLOWED_TESTS.contains(&name) {
                return Err(filter_invalid_op("test is not an allowed unary C-SEC test"));
            }
            (Some(name), None)
        }
        2 => {
            let name = extra[0]
                .as_str()
                .ok_or_else(|| filter_invalid_op("test name must be a string"))?;
            if !BINARY_ALLOWED_TESTS.contains(&name) {
                return Err(filter_invalid_op(
                    "test is not an allowed binary C-SEC test",
                ));
            }
            if matches!(name, "startingwith" | "endingwith") && extra[1].as_str().is_none() {
                return Err(filter_invalid_op("string test requires string operand"));
            }
            (Some(name), Some(&extra[1]))
        }
        _ => return Err(filter_invalid_op("too many arguments to attr filter")),
    };
    let items = read_bounded_sequence(&value, "attr")?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let attr_val = resolve_attr_path(&item, &segments);
        let matched = eval_attr_test_predicate(state, &attr_val, test_name, rhs)?;
        if matched ^ invert {
            out.push(item);
        }
    }
    Ok(Value::from(out))
}

fn filter_selectattr(
    state: &minijinja::State,
    value: Value,
    attr: &str,
    extra: minijinja::value::Rest<Value>,
) -> Result<Value, minijinja::Error> {
    filter_sequence_by_attr(state, value, attr, extra, false)
}

fn filter_rejectattr(
    state: &minijinja::State,
    value: Value,
    attr: &str,
    extra: minijinja::value::Rest<Value>,
) -> Result<Value, minijinja::Error> {
    filter_sequence_by_attr(state, value, attr, extra, true)
}

fn filter_map(
    value: Value,
    rest: minijinja::value::Rest<minijinja::value::ValueOrKwargs>,
) -> Result<Value, minijinja::Error> {
    if value.is_undefined() || value.is_none() {
        return Ok(Value::from(Vec::<Value>::new()));
    }
    let raw = rest.into_values();
    let (args, kwargs) = minijinja::value::from_args::<(&[Value], minijinja::value::Kwargs)>(&raw)?;
    let (attr, default_val) = match args.len() {
        1 => {
            let attr = args[0]
                .as_str()
                .ok_or_else(|| filter_invalid_op("map positional argument must be a string"))?;
            kwargs.assert_all_used()?;
            (attr, Value::from(()))
        }
        0 => {
            let attr: &str = kwargs.get("attribute")?;
            let default_opt: Option<Value> = kwargs.get("default")?;
            kwargs.assert_all_used()?;
            (attr, default_opt.unwrap_or_else(|| Value::from(())))
        }
        _ => return Err(filter_invalid_op("map requires attribute argument")),
    };
    let segments = parse_validated_attr_path(attr)?;
    let items = read_bounded_sequence(&value, "map")?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let resolved = resolve_attr_path(&item, &segments);
        if resolved.is_undefined() {
            out.push(default_val.clone());
        } else {
            out.push(resolved);
        }
    }
    Ok(Value::from(out))
}

pub fn restricted_environment() -> Environment<'static> {
    restricted_environment_with_config(&RenderFormattingConfig::default())
}

pub fn restricted_environment_with_config(cfg: &RenderFormattingConfig) -> Environment<'static> {
    let mut env = Environment::empty();
    env.set_auto_escape_callback(|_| AutoEscape::Html);
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_fuel(Some(TEMPLATE_FUEL_LIMIT));
    env.set_recursion_limit(TEMPLATE_RECURSION_LIMIT);
    env.set_formatter(|out, state, value| {
        if value.is_none() {
            Ok(())
        } else {
            minijinja::escape_formatter(out, state, value)
        }
    });
    env.add_filter("default", filter_default);
    env.add_filter("d", filter_default);
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
    let default_tz =
        jiff::tz::TimeZone::get(cfg.timezone.as_str()).unwrap_or(jiff::tz::TimeZone::UTC);
    env.add_filter(
        "date_format",
        move |value: Value, fmt: Option<&str>, tz: Option<&str>| {
            filter_date_format(value, fmt, tz, &default_tz)
        },
    );
    env.add_filter("truncate", filter_truncate);
    env.add_filter("format_number", filter_format_number);
    env.add_filter("batch", filter_batch);
    env.add_filter("selectattr", filter_selectattr);
    env.add_filter("rejectattr", filter_rejectattr);
    env.add_filter("map", filter_map);

    env.add_test("defined", minijinja::tests::is_defined);
    env.add_test("undefined", minijinja::tests::is_undefined);
    env.add_test("eq", minijinja::tests::is_eq);
    env.add_test("equalto", minijinja::tests::is_eq);
    env.add_test("ne", minijinja::tests::is_ne);
    env.add_test("lt", minijinja::tests::is_lt);
    env.add_test("le", minijinja::tests::is_le);
    env.add_test("gt", minijinja::tests::is_gt);
    env.add_test("ge", minijinja::tests::is_ge);
    env.add_test("none", minijinja::tests::is_none);
    env.add_test("true", minijinja::tests::is_true);
    env.add_test("false", minijinja::tests::is_false);
    env.add_test("in", minijinja::tests::is_in);
    env.add_test("startingwith", minijinja::tests::is_startingwith);
    env.add_test("endingwith", minijinja::tests::is_endingwith);
    env.add_test("even", minijinja::tests::is_even);
    env.add_test("odd", minijinja::tests::is_odd);
    env.add_test("number", minijinja::tests::is_number);
    env.add_test("string", minijinja::tests::is_string);
    env.add_test("sequence", minijinja::tests::is_sequence);
    env.add_test("mapping", minijinja::tests::is_mapping);
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
    validate_template_csec_with_usage_flags(name, source, policy, false, false).map(|_| ())
}

fn validate_template_csec_with_usage_flags(
    name: &str,
    source: &str,
    policy: &ThemeUrlPolicy,
    allow_od_icon: bool,
    allow_trusted_theme_markup: bool,
) -> Result<ThemeTemplateUsageFlags, &'static str> {
    if source.trim().is_empty() {
        return Err("Template không được để trống.");
    }
    if source.contains(ICON_NONCE_PREFIX) || source.contains(HOOK_NONCE_PREFIX) {
        return Err("Theme HTML chứa marker icon nội bộ bị cấm.");
    }
    let ast = parse(source, name, SyntaxConfig::default())
        .map_err(|_| "Cú pháp template MiniJinja không hợp lệ.")?;

    validate_stmt_with_icons(&ast, allow_od_icon, false)?;
    reject_reserved_context_rebinding(&ast)?;
    let is_full_doctype_doc = source
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("<!doctype");
    let usage = walk_theme_ast_with_url_scopes(
        &ast,
        false,
        is_full_doctype_doc,
        policy,
        allow_trusted_theme_markup,
    )?;

    let mut env = restricted_environment();
    env.add_template(name, source)
        .map_err(|_| "Không thể biên dịch template trong môi trường an toàn.")?;
    Ok(usage)
}

/// Validates and renders a template under C-SEC limits (fuel, recursion, 512 KiB output cap,
/// and post-render URL/HTML boundary verification).
pub fn render_csec(name: &str, source: &str, context: Value) -> Result<String, &'static str> {
    render_csec_with_config(name, source, context, &RenderFormattingConfig::default())
}

pub fn render_csec_with_config(
    name: &str,
    source: &str,
    context: Value,
    fmt_cfg: &RenderFormattingConfig,
) -> Result<String, &'static str> {
    validate_template_csec(name, source)?;
    let mut env = restricted_environment_with_config(fmt_cfg);
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
    validate_html_and_css_context(&rendered, true, false)?;
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
    let mut max_part_depth = 0_usize;
    let compiled_source = flatten_composition(
        root_source,
        &root_sites,
        parts,
        &part_sites,
        &mut visiting,
        0,
        &mut expansions,
        &mut content_count,
        &mut max_part_depth,
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
    let usage =
        validate_template_csec_with_usage_flags(root_name, &compiled_source, policy, true, true)?;
    Ok(CompiledCsecComposition {
        source: compiled_source,
        name: root_name.to_owned(),
        content_count,
        part_expansions: expansions,
        max_part_depth,
        uses_related_entries: usage.uses_related_entries,
        uses_recent_posts: usage.uses_recent_posts,
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
    max_part_depth: &mut usize,
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
                *max_part_depth = (*max_part_depth).max(depth + 1);
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
                    max_part_depth,
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
    if source.contains(CONTENT_SLOT_TOKEN)
        || source.contains(ICON_NONCE_PREFIX)
        || source.contains(HOOK_NONCE_PREFIX)
    {
        return Err("Theme HTML chứa marker nội bộ bị cấm.");
    }
    let ast = parse(source, name, SyntaxConfig::default())
        .map_err(|_| "Cú pháp theme MiniJinja không hợp lệ.")?;
    reject_reserved_context_rebinding(&ast)?;
    let is_full_doctype_doc = !is_part
        && source
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("<!doctype");
    let _ = walk_theme_ast_with_url_scopes(&ast, is_part, is_full_doctype_doc, policy, true)?;
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
            if parse_od_icon_call(&e.expr)?.is_some() || parse_od_hook_call(&e.expr)?.is_some() {
                return Ok(());
            }
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

fn parse_od_icon_call<'a>(
    expr: &'a Expr<'_>,
) -> Result<Option<(&'a str, Option<&'a str>)>, &'static str> {
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
        minijinja::machinery::ast::CallArg::Pos(Expr::Const(name)),
        rest @ ..,
    ] = call.args.as_slice()
    else {
        return Err("od_icon() yêu cầu string literal name và class tùy chọn.");
    };
    let name = name
        .value
        .as_str()
        .ok_or("od_icon() name phải là string literal.")?;
    if opendash_core::icons::lookup_core_icon_paths(name).is_none() {
        return Err("od_icon() tên icon không tồn tại trong core registry.");
    }
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
    Ok(Some((name, class)))
}

fn parse_od_hook_call<'a>(
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

fn reject_icons_in_set_block(children: &[Stmt<'_>]) -> Result<(), &'static str> {
    fn contains_icon(stmt: &Stmt<'_>) -> bool {
        match stmt {
            Stmt::EmitExpr(e) => {
                parse_od_icon_call(&e.expr).is_ok_and(|v| v.is_some())
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

fn step_comment_substate(sub: CommentScanSubState, byte: u8) -> Option<CommentScanSubState> {
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

/// Bounded lexical HTML states for block templates and preview nonce replacement.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum HtmlScanState {
    Body,
    Tag { quote: Option<u8>, prefix: String },
    Comment(CommentScanSubState),
    Style { closing_prefix: usize },
    Script { closing_prefix: usize },
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

fn validate_stmt(stmt: &Stmt<'_>) -> Result<(), &'static str> {
    validate_stmt_with_icons(stmt, false, false)
}

fn validate_stmt_with_icons(
    stmt: &Stmt<'_>,
    allow_od_icon: bool,
    in_set_block: bool,
) -> Result<(), &'static str> {
    match stmt {
        Stmt::Template(t) => {
            for child in &t.children {
                validate_stmt_with_icons(child, allow_od_icon, in_set_block)?;
            }
            Ok(())
        }
        Stmt::EmitRaw(_) => Ok(()),
        Stmt::EmitExpr(e) => {
            if allow_od_icon
                && !in_set_block
                && (parse_od_icon_call(&e.expr)?.is_some()
                    || parse_od_hook_call(&e.expr)?.is_some())
            {
                Ok(())
            } else {
                validate_expr(&e.expr)
            }
        }
        Stmt::ForLoop(f) => {
            validate_expr(&f.target)?;
            validate_expr(&f.iter)?;
            if let Some(cond) = &f.filter_expr {
                validate_expr(cond)?;
            }
            for child in &f.body {
                validate_stmt_with_icons(child, allow_od_icon, in_set_block)?;
            }
            for child in &f.else_body {
                validate_stmt_with_icons(child, allow_od_icon, in_set_block)?;
            }
            Ok(())
        }
        Stmt::IfCond(i) => {
            validate_expr(&i.expr)?;
            for child in &i.true_body {
                validate_stmt_with_icons(child, allow_od_icon, in_set_block)?;
            }
            for child in &i.false_body {
                validate_stmt_with_icons(child, allow_od_icon, in_set_block)?;
            }
            Ok(())
        }
        Stmt::WithBlock(w) => {
            for (k, v) in &w.assignments {
                validate_expr(k)?;
                validate_expr(v)?;
            }
            for child in &w.body {
                validate_stmt_with_icons(child, allow_od_icon, in_set_block)?;
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
                validate_stmt_with_icons(child, allow_od_icon, true)?;
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

const RESERVED_CONTEXT_VARS: &[&str] = &[
    "od_icon",
    "od_hook",
    "options",
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

fn binds_reserved_context(expr: &Expr<'_>) -> bool {
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

fn reject_reserved_context_rebinding(stmt: &Stmt<'_>) -> Result<(), &'static str> {
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

fn validate_call_arg(arg: &minijinja::machinery::ast::CallArg<'_>) -> Result<(), &'static str> {
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

fn collect_t_keys_stmt(stmt: &Stmt<'_>, out: &mut BTreeSet<String>) {
    match stmt {
        Stmt::Template(t) => {
            for child in &t.children {
                collect_t_keys_stmt(child, out);
            }
        }
        Stmt::EmitExpr(e) => collect_t_keys_expr(&e.expr, out),
        Stmt::ForLoop(f) => {
            collect_t_keys_expr(&f.iter, out);
            if let Some(cond) = &f.filter_expr {
                collect_t_keys_expr(cond, out);
            }
            for child in f.body.iter().chain(&f.else_body) {
                collect_t_keys_stmt(child, out);
            }
        }
        Stmt::IfCond(i) => {
            collect_t_keys_expr(&i.expr, out);
            for child in i.true_body.iter().chain(&i.false_body) {
                collect_t_keys_stmt(child, out);
            }
        }
        Stmt::WithBlock(w) => {
            for (_, val) in &w.assignments {
                collect_t_keys_expr(val, out);
            }
            for child in &w.body {
                collect_t_keys_stmt(child, out);
            }
        }
        Stmt::Set(s) => collect_t_keys_expr(&s.expr, out),
        Stmt::SetBlock(sb) => {
            if let Some(filter) = &sb.filter {
                collect_t_keys_expr(filter, out);
            }
            for child in &sb.body {
                collect_t_keys_stmt(child, out);
            }
        }
        _ => {}
    }
}

fn collect_t_keys_expr(expr: &Expr<'_>, out: &mut BTreeSet<String>) {
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

fn collect_t_keys_call_arg(
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

#[derive(Debug, Default, Clone, Copy)]
struct ThemeTemplateUsageFlags {
    uses_related_entries: bool,
    uses_recent_posts: bool,
}

fn record_usage_flags_in_call_arg(
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

fn record_usage_flags_in_expr(expr: &Expr<'_>, usage: &mut ThemeTemplateUsageFlags) {
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
enum UrlSourceKind {
    TrustedEntry,
    TrustedEntryBatch,
    TrustedLink,
    TrustedField,
    TrustedRawUrl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UrlScopeStack {
    frames: Vec<BTreeMap<String, Option<UrlSourceKind>>>,
}

impl UrlScopeStack {
    fn root() -> Self {
        Self {
            frames: vec![BTreeMap::from([(
                "entry".to_owned(),
                Some(UrlSourceKind::TrustedEntry),
            )])],
        }
    }

    fn push_frame(&mut self, frame: BTreeMap<String, Option<UrlSourceKind>>) {
        self.frames.push(frame);
    }

    fn pop_frame(&mut self) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
    }

    fn insert_top(&mut self, name: String, binding: Option<UrlSourceKind>) {
        if let Some(top) = self.frames.last_mut() {
            top.insert(name, binding);
        }
    }

    fn lookup(&self, name: &str) -> Option<UrlSourceKind> {
        for frame in self.frames.iter().rev() {
            if let Some(binding) = frame.get(name) {
                return *binding;
            }
        }
        None
    }

    fn intersect(a: &Self, b: &Self) -> Self {
        let frames = a
            .frames
            .iter()
            .zip(b.frames.iter())
            .map(|(fa, fb)| {
                let mut merged = BTreeMap::new();
                for key in fa.keys().chain(fb.keys()) {
                    let val = match (fa.get(key), fb.get(key)) {
                        (Some(Some(ka)), Some(Some(kb))) if ka == kb => Some(*ka),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum TagPhase {
    TagName,
    BeforeAttr,
    AttrName,
    AfterAttrName,
    BeforeAttrValue,
    AttrValue,
    SelfClosingSlash,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ThemeHtmlState {
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
        attr_static_buf: String,
    },
    // Only trusted Theme/Block templates track foreign-content nesting. The
    // wrapper preserves SVG depth across tags, comments, and Jinja branches.
    Trusted {
        inner: Box<ThemeHtmlState>,
        svg_depth: usize,
    },
}

impl ThemeHtmlState {
    fn inner(&self) -> &Self {
        match self {
            Self::Trusted { inner, .. } => inner,
            other => other,
        }
    }

    fn trusted_depth(&self) -> usize {
        match self {
            Self::Trusted { svg_depth, .. } => *svg_depth,
            _ => 0,
        }
    }

    fn into_inner(self) -> Self {
        match self {
            Self::Trusted { inner, .. } => *inner,
            other => other,
        }
    }

    fn with_trust(inner: Self, trusted: bool, svg_depth: usize) -> Self {
        if trusted {
            Self::Trusted {
                inner: Box::new(inner),
                svg_depth,
            }
        } else {
            inner
        }
    }
}

/// Match one forbidden script prefix incrementally across adjacent raw AST
/// nodes. Each pattern has only one possible proper suffix-prefix: "<".
fn next_script_escaped_prefix(pattern: &[u8], matched: usize, ascii: Option<u8>) -> usize {
    match ascii {
        Some(byte) if byte == pattern[matched] => matched + 1,
        Some(b'<') => 1,
        _ => 0,
    }
}

const MAX_THEME_WALKER_STATES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UrlAttrSlot {
    AnchorHref,
    Cite,
    SrcAndHref,
    LinkCss,
}

fn is_safe_theme_relative_path(raw: &str) -> bool {
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

fn is_dangerous_attr_name(attr_name: &str, allow_trusted_theme_markup: bool) -> bool {
    matches!(attr_name, "srcdoc" | "formaction" | "action")
        || (!allow_trusted_theme_markup
            && (attr_name.starts_with("on")
                || matches!(attr_name, "xmlns" | "xmlns:xlink" | "xlink:href")))
}

fn forbids_dynamic_interpolation(attr_name: &str) -> bool {
    attr_name.starts_with("on")
        || matches!(
            attr_name,
            "srcdoc" | "formaction" | "action" | "xmlns" | "xmlns:xlink" | "xlink:href"
        )
}

fn is_security_checked_attr(attr_name: &str) -> bool {
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

fn new_theme_tag_state(first_char: char, is_close: bool) -> ThemeHtmlState {
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
        attr_static_buf: String::new(),
    }
}

fn is_ascii_html_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

fn check_theme_tag_name(
    tag_name: &str,
    is_close: bool,
    is_part: bool,
    is_full_doctype_doc: bool,
    allow_trusted_theme_markup: bool,
    svg_depth: usize,
) -> Result<(), &'static str> {
    if tag_name.is_empty() {
        return Err("Thẻ HTML không có tên hợp lệ.");
    }
    if is_close && matches!(tag_name, "link" | "meta" | "title" | "style" | "script") {
        return Err("Thẻ đóng HTML không hợp lệ ở vị trí này.");
    }
    if FORBIDDEN_TAGS.contains(&tag_name)
        && !(allow_trusted_theme_markup
            && matches!(tag_name, "script" | "svg" | "foreignobject" | "math"))
        && !(tag_name == "title"
            && !is_close
            && ((!is_part && is_full_doctype_doc) || (allow_trusted_theme_markup && svg_depth > 0)))
    {
        return Err("Template chứa thẻ HTML bị C-SEC cấm.");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn finalize_theme_attr(
    tag_name: &str,
    attr_name: &mut String,
    quote: &mut Option<u8>,
    attr_has_static_bytes: &mut bool,
    attr_expr_count: &mut u8,
    seen_attrs: &mut BTreeSet<String>,
    link_rel_stylesheet: &mut bool,
    link_has_valid_href: &mut bool,
    meta_ok: &mut bool,
    attr_static_buf: &mut String,
    allow_trusted_theme_markup: bool,
) -> Result<(), &'static str> {
    if attr_name.is_empty() {
        return Ok(());
    }
    if !seen_attrs.insert(attr_name.clone()) {
        return Err("Thẻ HTML chứa thuộc tính bị trùng lặp.");
    }
    if is_dangerous_attr_name(attr_name, allow_trusted_theme_markup) {
        return Err("Template chứa thuộc tính HTML bị C-SEC cấm.");
    }
    if forbids_dynamic_interpolation(attr_name)
        && (*attr_expr_count != 0
            || attr_static_buf.contains("{{")
            || attr_static_buf.contains("{%"))
    {
        return Err("C-SEC cấm nội suy trong thuộc tính sự kiện/namespace HTML.");
    }
    if tag_name == "meta" {
        if !matches!(attr_name.as_str(), "charset" | "name" | "content") {
            return Err("C-SEC chỉ cho phép <meta charset> hoặc <meta name=\"viewport\">.");
        }
        if attr_name == "charset" {
            *meta_ok = true;
        } else if attr_name == "name" {
            if *attr_expr_count != 0 || !attr_static_buf.trim().eq_ignore_ascii_case("viewport") {
                return Err("C-SEC chỉ cho phép <meta charset> hoặc <meta name=\"viewport\">.");
            }
            *meta_ok = true;
        }
    }
    if tag_name == "link" {
        if attr_name == "rel"
            && *attr_expr_count == 0
            && attr_static_buf.trim().eq_ignore_ascii_case("stylesheet")
        {
            *link_rel_stylesheet = true;
        } else if attr_name == "href" && *attr_expr_count == 0 {
            *link_has_valid_href = is_valid_theme_stylesheet_href(attr_static_buf);
        }
    }
    if attr_name == "style" && *attr_expr_count == 0 {
        validate_safe_css(attr_static_buf)?;
    }
    if matches!(attr_name.as_str(), "srcset" | "ping") && *attr_expr_count == 0 {
        let decoded = decode_html_entities_for_url(attr_static_buf);
        if decoded.trim().is_empty()
            || decoded.contains('\0')
            || decoded.bytes().any(|b| b <= 0x1f || b == 0x7f)
        {
            return Err("Thuộc tính URL danh sách không hợp lệ.");
        }
        if attr_name == "srcset" {
            for candidate in decoded.split(',') {
                let url_part = candidate
                    .split_whitespace()
                    .next()
                    .ok_or("Thuộc tính srcset chứa mục rỗng.")?;
                validate_safe_url(url_part)?;
            }
        } else {
            for target in decoded.split_whitespace() {
                validate_safe_url(target)?;
            }
        }
    }
    if matches!(
        attr_name.as_str(),
        "href" | "src" | "poster" | "cite" | "xlink:href"
    ) {
        if *attr_expr_count > 1 || (*attr_expr_count > 0 && *attr_has_static_bytes) {
            return Err("URL động trong thuộc tính chỉ được phép là biểu thức {{ ... }} đơn.");
        }
        if *attr_expr_count == 0 && !(tag_name == "link" && attr_name == "href") {
            let decoded = decode_html_entities_for_url(attr_static_buf);
            if (tag_name == "a" && attr_name == "href") || attr_name == "xlink:href" {
                validate_safe_href_url(&decoded)?;
            } else {
                validate_safe_url(&decoded)?;
            }
        }
    }
    attr_name.clear();
    *quote = None;
    *attr_has_static_bytes = false;
    *attr_expr_count = 0;
    attr_static_buf.clear();
    Ok(())
}

fn finalize_theme_tag(
    tag_name: &str,
    is_close: bool,
    link_rel_stylesheet: bool,
    link_has_valid_href: bool,
    meta_ok: bool,
    is_part: bool,
    is_full_doctype_doc: bool,
    allow_trusted_theme_markup: bool,
    svg_depth: usize,
) -> Result<ThemeHtmlState, &'static str> {
    check_theme_tag_name(
        tag_name,
        is_close,
        is_part,
        is_full_doctype_doc,
        allow_trusted_theme_markup,
        svg_depth,
    )?;
    if is_close {
        return Ok(ThemeHtmlState::Body);
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

fn step_theme_raw_char(
    state: ThemeHtmlState,
    ch: char,
    is_part: bool,
    is_full_doctype_doc: bool,
    allow_trusted_theme_markup: bool,
    svg_depth: usize,
) -> Result<ThemeHtmlState, &'static str> {
    if let ThemeHtmlState::Trusted {
        inner,
        svg_depth: depth,
    } = state
    {
        let completed_svg_tag = match inner.as_ref() {
            ThemeHtmlState::Tag {
                tag_name,
                is_close,
                phase,
                quote,
                ..
            } if tag_name == "svg" && ch == '>' && quote.is_none() => {
                Some((*is_close, *phase == TagPhase::SelfClosingSlash))
            }
            _ => None,
        };
        let next = step_theme_raw_char(*inner, ch, is_part, is_full_doctype_doc, true, depth)?;
        let depth = if let Some((is_close, self_closing)) = completed_svg_tag {
            if is_close {
                depth.checked_sub(1).ok_or("Thẻ </svg> không có thẻ mở.")?
            } else if self_closing {
                depth
            } else {
                depth
                    .checked_add(1)
                    .filter(|d| *d <= 64)
                    .ok_or("SVG lồng nhau quá sâu.")?
            }
        } else {
            depth
        };
        return Ok(ThemeHtmlState::with_trust(next, true, depth));
    }
    let ascii = if ch.is_ascii() { Some(ch as u8) } else { None };
    match state {
        ThemeHtmlState::Body => {
            if ascii == Some(b'<') {
                Ok(ThemeHtmlState::BodyLt)
            } else {
                Ok(ThemeHtmlState::Body)
            }
        }
        ThemeHtmlState::BodyLt => match ascii {
            Some(b'a'..=b'z' | b'A'..=b'Z') => Ok(new_theme_tag_state(ch, false)),
            Some(b'/') => Ok(ThemeHtmlState::BodyLtSlash),
            Some(b'!') => Ok(ThemeHtmlState::BodyLtBang {
                buf: [0; 7],
                len: 0,
            }),
            Some(b'<') => Ok(ThemeHtmlState::BodyLt),
            Some(b'?') => {
                Err("Template không được chứa processing instruction hoặc bogus comment `<?`.")
            }
            _ => Ok(ThemeHtmlState::Body),
        },
        ThemeHtmlState::BodyLtSlash => match ascii {
            Some(b'a'..=b'z' | b'A'..=b'Z') => Ok(new_theme_tag_state(ch, true)),
            _ => Err("Thẻ đóng HTML phải bắt đầu bằng ký tự chữ cái ASCII."),
        },
        ThemeHtmlState::BodyLtBang { mut buf, mut len } => {
            let Some(b) = ascii else {
                return Err("Template chỉ cho phép HTML comment <!-- hoặc <!DOCTYPE>.");
            };
            if len == 7 {
                if is_part {
                    return Err("Part không được chứa DOCTYPE.");
                }
                if b == b'>' {
                    return Ok(ThemeHtmlState::Body);
                }
                if is_ascii_html_whitespace(b) {
                    return Ok(ThemeHtmlState::Doctype);
                }
                return Err("<!DOCTYPE> không hợp lệ.");
            }
            buf[len as usize] = b.to_ascii_lowercase();
            len += 1;
            if len == 2 && &buf[..2] == b"--" {
                return Ok(ThemeHtmlState::Comment(CommentScanSubState::Start));
            }
            let prefix = &buf[..len as usize];
            if b"--".starts_with(prefix) || b"doctype".starts_with(prefix) {
                Ok(ThemeHtmlState::BodyLtBang { buf, len })
            } else {
                Err("Template chỉ cho phép HTML comment <!-- hoặc <!DOCTYPE>.")
            }
        }
        ThemeHtmlState::Doctype => {
            if let Some(b) = ascii {
                if b == b'>' {
                    return Ok(ThemeHtmlState::Body);
                }
                if matches!(b, b'<' | b'"' | b'\'' | 0x00..=0x1f | 0x7f) {
                    return Err("<!DOCTYPE> chứa ký tự không hợp lệ.");
                }
            }
            Ok(ThemeHtmlState::Doctype)
        }
        ThemeHtmlState::Comment(sub) => {
            let next = match ascii {
                Some(b) => step_comment_substate(sub, b),
                None => Some(CommentScanSubState::Body),
            };
            Ok(match next {
                Some(s) => ThemeHtmlState::Comment(s),
                None => ThemeHtmlState::Body,
            })
        }
        ThemeHtmlState::Style {
            mut closing_prefix,
            mut css_buf,
        } => {
            const EXPECTED: &[u8; 7] = b"</style";
            if closing_prefix == 7 {
                if ascii == Some(b'>') {
                    let trim_to = css_buf.len().saturating_sub(7);
                    css_buf.truncate(trim_to);
                    if css_buf.len() > MAX_RENDERED_HTML_BYTES {
                        return Err("Khối <style> vượt giới hạn 512 KiB.");
                    }
                    validate_safe_css(&css_buf)?;
                    return Ok(ThemeHtmlState::Body);
                }
                if ascii.is_some_and(|b| is_ascii_html_whitespace(b) || b == b'/') {
                    return Err("Thẻ đóng </style> không được chứa khoảng trắng hoặc dấu /.");
                }
                if css_buf.len() + ch.len_utf8() > MAX_RENDERED_HTML_BYTES + 8 {
                    return Err("Khối <style> vượt giới hạn 512 KiB.");
                }
                css_buf.push(ch);
                closing_prefix = if ascii == Some(b'<') { 1 } else { 0 };
                return Ok(ThemeHtmlState::Style {
                    closing_prefix,
                    css_buf,
                });
            }
            if css_buf.len() + ch.len_utf8() > MAX_RENDERED_HTML_BYTES + 8 {
                return Err("Khối <style> vượt giới hạn 512 KiB.");
            }
            css_buf.push(ch);
            closing_prefix = match ascii {
                Some(b) if b.to_ascii_lowercase() == EXPECTED[closing_prefix] => closing_prefix + 1,
                Some(b'<') => 1,
                _ => 0,
            };
            Ok(ThemeHtmlState::Style {
                closing_prefix,
                css_buf,
            })
        }
        ThemeHtmlState::Script {
            mut closing_prefix,
            mut comment_prefix,
            mut cdata_prefix,
        } => {
            // Keep the HTML5 escaped and SVG CDATA scanners in the Theme AST
            // walker too. A fake closing tag inside either state may cause
            // untrusted Jinja output after it to execute as JavaScript.
            comment_prefix = next_script_escaped_prefix(b"<!--", comment_prefix, ascii);
            cdata_prefix = next_script_escaped_prefix(b"<![CDATA[", cdata_prefix, ascii);
            if comment_prefix == 4 || cdata_prefix == 9 {
                return Err("C-SEC cam HTML escaped state va SVG CDATA trong <script>.");
            }
            const EXPECTED: &[u8; 8] = b"</script";
            if closing_prefix == EXPECTED.len() {
                if ascii == Some(b'>') {
                    return Ok(ThemeHtmlState::Body);
                }
                if ascii.is_some_and(|b| is_ascii_html_whitespace(b) || b == b'/') {
                    return Err("Thẻ đóng </script> không được chứa khoảng trắng hoặc /.");
                }
                closing_prefix = if ascii == Some(b'<') { 1 } else { 0 };
            } else {
                closing_prefix = match ascii {
                    Some(b) if b.to_ascii_lowercase() == EXPECTED[closing_prefix] => {
                        closing_prefix + 1
                    }
                    Some(b'<') => 1,
                    _ => 0,
                };
            }
            Ok(ThemeHtmlState::Script {
                closing_prefix,
                comment_prefix,
                cdata_prefix,
            })
        }
        ThemeHtmlState::Title { mut closing_prefix } => {
            const EXPECTED: &[u8; 7] = b"</title";
            if closing_prefix == 7 {
                if ascii == Some(b'>') {
                    return Ok(ThemeHtmlState::Body);
                }
                if ascii.is_some_and(|b| is_ascii_html_whitespace(b) || b == b'/') {
                    return Err("Thẻ đóng </title> không được chứa khoảng trắng hoặc dấu /.");
                }
                closing_prefix = if ascii == Some(b'<') { 1 } else { 0 };
                return Ok(ThemeHtmlState::Title { closing_prefix });
            }
            closing_prefix = match ascii {
                Some(b) if b.to_ascii_lowercase() == EXPECTED[closing_prefix] => closing_prefix + 1,
                Some(b'<') => 1,
                _ => 0,
            };
            Ok(ThemeHtmlState::Title { closing_prefix })
        }
        ThemeHtmlState::Tag {
            mut tag_name,
            is_close,
            mut phase,
            mut attr_name,
            mut quote,
            mut attr_has_static_bytes,
            mut attr_expr_count,
            mut seen_attrs,
            mut link_rel_stylesheet,
            mut link_has_valid_href,
            mut meta_ok,
            mut attr_static_buf,
        } => {
            if ascii == Some(0) {
                return Err("Thẻ HTML chứa byte NUL bị cấm.");
            }
            match phase {
                TagPhase::TagName => match ascii {
                    Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-') => {
                        if tag_name.len() >= 32 {
                            return Err("Tên thẻ HTML vượt giới hạn 32 ký tự.");
                        }
                        tag_name.push(ch.to_ascii_lowercase());
                    }
                    Some(b) if is_ascii_html_whitespace(b) => {
                        check_theme_tag_name(
                            &tag_name,
                            is_close,
                            is_part,
                            is_full_doctype_doc,
                            allow_trusted_theme_markup,
                            svg_depth,
                        )?;
                        phase = TagPhase::BeforeAttr;
                    }
                    Some(b'/') => {
                        if is_close
                            || tag_name.is_empty()
                            || matches!(tag_name.as_str(), "style" | "title" | "script")
                        {
                            return Err("Dấu tự đóng `/` trên thẻ HTML không hợp lệ.");
                        }
                        check_theme_tag_name(
                            &tag_name,
                            is_close,
                            is_part,
                            is_full_doctype_doc,
                            allow_trusted_theme_markup,
                            svg_depth,
                        )?;
                        phase = TagPhase::SelfClosingSlash;
                    }
                    Some(b'>') => {
                        return finalize_theme_tag(
                            &tag_name,
                            is_close,
                            link_rel_stylesheet,
                            link_has_valid_href,
                            meta_ok,
                            is_part,
                            is_full_doctype_doc,
                            allow_trusted_theme_markup,
                            svg_depth,
                        );
                    }
                    _ => return Err("Tên thẻ HTML chứa ký tự không hợp lệ."),
                },
                TagPhase::BeforeAttr => {
                    if is_close {
                        match ascii {
                            Some(b) if is_ascii_html_whitespace(b) => {}
                            Some(b'>') => {
                                return finalize_theme_tag(
                                    &tag_name,
                                    is_close,
                                    link_rel_stylesheet,
                                    link_has_valid_href,
                                    meta_ok,
                                    is_part,
                                    is_full_doctype_doc,
                                    allow_trusted_theme_markup,
                                    svg_depth,
                                );
                            }
                            _ => return Err("Thẻ đóng HTML không được chứa thuộc tính hoặc /."),
                        }
                    } else {
                        match ascii {
                            Some(b) if is_ascii_html_whitespace(b) => {}
                            Some(b'a'..=b'z' | b'A'..=b'Z') => {
                                attr_name.clear();
                                attr_name.push(ch.to_ascii_lowercase());
                                quote = None;
                                attr_has_static_bytes = false;
                                attr_expr_count = 0;
                                attr_static_buf.clear();
                                phase = TagPhase::AttrName;
                            }
                            Some(b'/') => {
                                if matches!(tag_name.as_str(), "style" | "title" | "script") {
                                    return Err("Thẻ <style>/<title> không được tự đóng.");
                                }
                                phase = TagPhase::SelfClosingSlash;
                            }
                            Some(b'>') => {
                                return finalize_theme_tag(
                                    &tag_name,
                                    is_close,
                                    link_rel_stylesheet,
                                    link_has_valid_href,
                                    meta_ok,
                                    is_part,
                                    is_full_doctype_doc,
                                    allow_trusted_theme_markup,
                                    svg_depth,
                                );
                            }
                            _ => return Err("Thuộc tính HTML phải bắt đầu bằng chữ cái ASCII."),
                        }
                    }
                }
                TagPhase::AttrName => match ascii {
                    Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b':' | b'-') => {
                        if attr_name.len() >= 32 {
                            return Err("Tên thuộc tính HTML vượt giới hạn 32 ký tự.");
                        }
                        attr_name.push(ch.to_ascii_lowercase());
                    }
                    Some(b) if is_ascii_html_whitespace(b) => {
                        phase = TagPhase::AfterAttrName;
                    }
                    Some(b'=') => {
                        phase = TagPhase::BeforeAttrValue;
                    }
                    Some(b'/') => {
                        finalize_theme_attr(
                            &tag_name,
                            &mut attr_name,
                            &mut quote,
                            &mut attr_has_static_bytes,
                            &mut attr_expr_count,
                            &mut seen_attrs,
                            &mut link_rel_stylesheet,
                            &mut link_has_valid_href,
                            &mut meta_ok,
                            &mut attr_static_buf,
                            allow_trusted_theme_markup,
                        )?;
                        if matches!(tag_name.as_str(), "style" | "title" | "script") {
                            return Err("Thẻ <style>/<title> không được tự đóng.");
                        }
                        phase = TagPhase::SelfClosingSlash;
                    }
                    Some(b'>') => {
                        finalize_theme_attr(
                            &tag_name,
                            &mut attr_name,
                            &mut quote,
                            &mut attr_has_static_bytes,
                            &mut attr_expr_count,
                            &mut seen_attrs,
                            &mut link_rel_stylesheet,
                            &mut link_has_valid_href,
                            &mut meta_ok,
                            &mut attr_static_buf,
                            allow_trusted_theme_markup,
                        )?;
                        return finalize_theme_tag(
                            &tag_name,
                            is_close,
                            link_rel_stylesheet,
                            link_has_valid_href,
                            meta_ok,
                            is_part,
                            is_full_doctype_doc,
                            allow_trusted_theme_markup,
                            svg_depth,
                        );
                    }
                    _ => return Err("Tên thuộc tính HTML chứa ký tự không hợp lệ."),
                },
                TagPhase::AfterAttrName => match ascii {
                    Some(b) if is_ascii_html_whitespace(b) => {}
                    Some(b'=') => {
                        phase = TagPhase::BeforeAttrValue;
                    }
                    Some(b'/') => {
                        finalize_theme_attr(
                            &tag_name,
                            &mut attr_name,
                            &mut quote,
                            &mut attr_has_static_bytes,
                            &mut attr_expr_count,
                            &mut seen_attrs,
                            &mut link_rel_stylesheet,
                            &mut link_has_valid_href,
                            &mut meta_ok,
                            &mut attr_static_buf,
                            allow_trusted_theme_markup,
                        )?;
                        if matches!(tag_name.as_str(), "style" | "title" | "script") {
                            return Err("Thẻ <style>/<title> không được tự đóng.");
                        }
                        phase = TagPhase::SelfClosingSlash;
                    }
                    Some(b'>') => {
                        finalize_theme_attr(
                            &tag_name,
                            &mut attr_name,
                            &mut quote,
                            &mut attr_has_static_bytes,
                            &mut attr_expr_count,
                            &mut seen_attrs,
                            &mut link_rel_stylesheet,
                            &mut link_has_valid_href,
                            &mut meta_ok,
                            &mut attr_static_buf,
                            allow_trusted_theme_markup,
                        )?;
                        return finalize_theme_tag(
                            &tag_name,
                            is_close,
                            link_rel_stylesheet,
                            link_has_valid_href,
                            meta_ok,
                            is_part,
                            is_full_doctype_doc,
                            allow_trusted_theme_markup,
                            svg_depth,
                        );
                    }
                    Some(b'a'..=b'z' | b'A'..=b'Z') => {
                        finalize_theme_attr(
                            &tag_name,
                            &mut attr_name,
                            &mut quote,
                            &mut attr_has_static_bytes,
                            &mut attr_expr_count,
                            &mut seen_attrs,
                            &mut link_rel_stylesheet,
                            &mut link_has_valid_href,
                            &mut meta_ok,
                            &mut attr_static_buf,
                            allow_trusted_theme_markup,
                        )?;
                        attr_name.push(ch.to_ascii_lowercase());
                        phase = TagPhase::AttrName;
                    }
                    _ => return Err("Cú pháp sau tên thuộc tính HTML không hợp lệ."),
                },
                TagPhase::BeforeAttrValue => match ascii {
                    Some(b) if is_ascii_html_whitespace(b) => {}
                    Some(q @ (b'"' | b'\'')) => {
                        quote = Some(q);
                        phase = TagPhase::AttrValue;
                    }
                    Some(b'>' | b'<' | b'=' | b'`' | 0x00..=0x1f | 0x7f) => {
                        return Err("Giá trị thuộc tính HTML chứa ký tự không hợp lệ.");
                    }
                    _ => {
                        quote = None;
                        phase = TagPhase::AttrValue;
                        attr_has_static_bytes = true;
                        if is_security_checked_attr(&attr_name) {
                            if attr_static_buf.len() + ch.len_utf8() > 4096 {
                                return Err("Giá trị thuộc tính tĩnh vượt quá 4096 bytes.");
                            }
                            attr_static_buf.push(ch);
                        }
                    }
                },
                TagPhase::AttrValue => {
                    if let Some(q) = quote {
                        if ascii == Some(q) {
                            finalize_theme_attr(
                                &tag_name,
                                &mut attr_name,
                                &mut quote,
                                &mut attr_has_static_bytes,
                                &mut attr_expr_count,
                                &mut seen_attrs,
                                &mut link_rel_stylesheet,
                                &mut link_has_valid_href,
                                &mut meta_ok,
                                &mut attr_static_buf,
                                allow_trusted_theme_markup,
                            )?;
                            phase = TagPhase::BeforeAttr;
                        } else {
                            if matches!(attr_name.as_str(), "href" | "src" | "poster" | "cite")
                                && attr_expr_count > 0
                            {
                                return Err(
                                    "URL động trong thuộc tính chỉ được phép là biểu thức {{ ... }} đơn.",
                                );
                            }
                            attr_has_static_bytes = true;
                            if is_security_checked_attr(&attr_name) {
                                if attr_static_buf.len() + ch.len_utf8() > 4096 {
                                    return Err("Giá trị thuộc tính tĩnh vượt quá 4096 bytes.");
                                }
                                attr_static_buf.push(ch);
                            }
                        }
                    } else if ascii.is_some_and(is_ascii_html_whitespace) {
                        finalize_theme_attr(
                            &tag_name,
                            &mut attr_name,
                            &mut quote,
                            &mut attr_has_static_bytes,
                            &mut attr_expr_count,
                            &mut seen_attrs,
                            &mut link_rel_stylesheet,
                            &mut link_has_valid_href,
                            &mut meta_ok,
                            &mut attr_static_buf,
                            allow_trusted_theme_markup,
                        )?;
                        phase = TagPhase::BeforeAttr;
                    } else if ascii == Some(b'>') {
                        finalize_theme_attr(
                            &tag_name,
                            &mut attr_name,
                            &mut quote,
                            &mut attr_has_static_bytes,
                            &mut attr_expr_count,
                            &mut seen_attrs,
                            &mut link_rel_stylesheet,
                            &mut link_has_valid_href,
                            &mut meta_ok,
                            &mut attr_static_buf,
                            allow_trusted_theme_markup,
                        )?;
                        return finalize_theme_tag(
                            &tag_name,
                            is_close,
                            link_rel_stylesheet,
                            link_has_valid_href,
                            meta_ok,
                            is_part,
                            is_full_doctype_doc,
                            allow_trusted_theme_markup,
                            svg_depth,
                        );
                    } else if matches!(
                        ascii,
                        Some(b'"' | b'\'' | b'<' | b'=' | b'`' | 0x00..=0x1f | 0x7f)
                    ) {
                        return Err(
                            "Giá trị thuộc tính HTML không ngoặc kép chứa ký tự không hợp lệ.",
                        );
                    } else {
                        attr_has_static_bytes = true;
                        if is_security_checked_attr(&attr_name) {
                            if attr_static_buf.len() + ch.len_utf8() > 4096 {
                                return Err("Giá trị thuộc tính tĩnh vượt quá 4096 bytes.");
                            }
                            attr_static_buf.push(ch);
                        }
                    }
                }
                TagPhase::SelfClosingSlash => {
                    if ascii == Some(b'>') {
                        return finalize_theme_tag(
                            &tag_name,
                            is_close,
                            link_rel_stylesheet,
                            link_has_valid_href,
                            meta_ok,
                            is_part,
                            is_full_doctype_doc,
                            allow_trusted_theme_markup,
                            svg_depth,
                        );
                    }
                    return Err("Sau `/` ở cuối thẻ mở phải là `>`.");
                }
            }
            Ok(ThemeHtmlState::Tag {
                tag_name,
                is_close,
                phase,
                attr_name,
                quote,
                attr_has_static_bytes,
                attr_expr_count,
                seen_attrs,
                link_rel_stylesheet,
                link_has_valid_href,
                meta_ok,
                attr_static_buf,
            })
        }
        ThemeHtmlState::Trusted { .. } => unreachable!("trusted state unwrapped above"),
    }
}

fn extract_bound_idents(expr: &Expr<'_>) -> Result<Vec<String>, &'static str> {
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

fn resolve_collection_source(expr: &Expr<'_>, scopes: &UrlScopeStack) -> Option<UrlSourceKind> {
    match expr {
        Expr::Slice(s) => resolve_collection_source(&s.expr, scopes),
        Expr::Filter(f) => {
            let inner = f.expr.as_ref()?;
            if matches!(f.name, "selectattr" | "rejectattr" | "sort" | "reverse") {
                resolve_collection_source(inner, scopes)
            } else if f.name == "batch" {
                match resolve_collection_source(inner, scopes) {
                    Some(UrlSourceKind::TrustedEntry) => Some(UrlSourceKind::TrustedEntryBatch),
                    _ => None,
                }
            } else {
                None
            }
        }
        Expr::Var(v) => {
            if matches!(v.id, "entries" | "related_entries" | "recent_posts") {
                Some(UrlSourceKind::TrustedEntry)
            } else if v.id == "breadcrumbs" {
                Some(UrlSourceKind::TrustedLink)
            } else if scopes.lookup(v.id) == Some(UrlSourceKind::TrustedEntryBatch) {
                Some(UrlSourceKind::TrustedEntry)
            } else {
                None
            }
        }
        Expr::GetAttr(g) => match &g.expr {
            Expr::Var(v) if v.id == "pagination" && g.name == "pages" => {
                Some(UrlSourceKind::TrustedLink)
            }
            Expr::Var(v) if v.id == "locale" && g.name == "alternates" => {
                Some(UrlSourceKind::TrustedLink)
            }
            Expr::GetAttr(menu_slot)
                if g.name == "items"
                    && matches!(&menu_slot.expr, Expr::Var(v) if v.id == "menus") =>
            {
                Some(UrlSourceKind::TrustedLink)
            }
            Expr::GetItem(menu_slot)
                if g.name == "items"
                    && matches!(&menu_slot.expr, Expr::Var(v) if v.id == "menus")
                    && matches!(&menu_slot.subscript_expr, Expr::Const(c) if c.value.as_str().is_some()) =>
            {
                Some(UrlSourceKind::TrustedLink)
            }
            Expr::Var(v)
                if g.name == "children"
                    && scopes.lookup(v.id) == Some(UrlSourceKind::TrustedLink) =>
            {
                Some(UrlSourceKind::TrustedLink)
            }
            Expr::Var(v)
                if g.name == "all_terms"
                    && scopes.lookup(v.id) == Some(UrlSourceKind::TrustedEntry) =>
            {
                Some(UrlSourceKind::TrustedLink)
            }
            Expr::GetAttr(terms_attr)
                if terms_attr.name == "terms"
                    && matches!(
                        &terms_attr.expr,
                        Expr::Var(v) if scopes.lookup(v.id) == Some(UrlSourceKind::TrustedEntry)
                    ) =>
            {
                Some(UrlSourceKind::TrustedLink)
            }
            Expr::Var(v)
                if g.name == "fields"
                    && scopes.lookup(v.id) == Some(UrlSourceKind::TrustedEntry) =>
            {
                Some(UrlSourceKind::TrustedField)
            }
            Expr::Var(v)
                if g.name == "gallery_urls"
                    && scopes.lookup(v.id) == Some(UrlSourceKind::TrustedField) =>
            {
                Some(UrlSourceKind::TrustedRawUrl)
            }
            _ => None,
        },
        Expr::GetItem(g) => {
            let Expr::Const(c) = &g.subscript_expr else {
                return None;
            };
            let _tax = c.value.as_str()?;
            match &g.expr {
                Expr::GetAttr(terms_attr)
                    if terms_attr.name == "terms"
                        && matches!(
                            &terms_attr.expr,
                            Expr::Var(v) if scopes.lookup(v.id) == Some(UrlSourceKind::TrustedEntry)
                        ) =>
                {
                    Some(UrlSourceKind::TrustedLink)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn is_href_only_expr(expr: &Expr<'_>, scopes: &UrlScopeStack, policy: &ThemeUrlPolicy) -> bool {
    match expr {
        Expr::Var(v) => matches!(v.id, "archive_url" | "previous_url" | "next_url"),
        Expr::GetAttr(g) => match &g.expr {
            Expr::Var(v) if v.id == "pagination" => {
                matches!(g.name, "previous_url" | "next_url")
            }
            Expr::Var(v) if v.id == "locale" => g.name == "home_url",
            Expr::Var(v) if v.id == "options" => {
                policy.allows_cite_href(&format!("options.{}", g.name))
            }
            Expr::Var(v)
                if g.name == "url"
                    && matches!(
                        scopes.lookup(v.id),
                        Some(UrlSourceKind::TrustedEntry | UrlSourceKind::TrustedLink)
                    ) =>
            {
                true
            }
            Expr::GetAttr(cat_attr)
                if g.name == "url"
                    && cat_attr.name == "primary_category"
                    && matches!(
                        &cat_attr.expr,
                        Expr::Var(v) if scopes.lookup(v.id) == Some(UrlSourceKind::TrustedEntry)
                    ) =>
            {
                true
            }
            _ => false,
        },
        _ => false,
    }
}

fn is_src_and_href_expr(expr: &Expr<'_>, scopes: &UrlScopeStack, policy: &ThemeUrlPolicy) -> bool {
    match expr {
        Expr::Var(v) => {
            v.id == "theme_asset_url" || scopes.lookup(v.id) == Some(UrlSourceKind::TrustedRawUrl)
        }
        Expr::GetAttr(g) => match &g.expr {
            Expr::Var(v) if v.id == "options" => {
                policy.allows_media_src(&format!("options.{}", g.name))
            }
            Expr::Var(v)
                if g.name == "cover_url"
                    && scopes.lookup(v.id) == Some(UrlSourceKind::TrustedEntry) =>
            {
                true
            }
            Expr::Var(v)
                if g.name == "image_url"
                    && scopes.lookup(v.id) == Some(UrlSourceKind::TrustedField) =>
            {
                true
            }
            _ => false,
        },
        Expr::GetItem(g) => {
            matches!(&g.expr, Expr::Var(v) if v.id == "theme_assets")
                && matches!(
                    &g.subscript_expr,
                    Expr::Const(c) if c.value.as_str().is_some_and(|p| policy.allows_declared_asset(p))
                )
        }
        _ => false,
    }
}

fn is_valid_theme_url_expr(
    expr: &Expr<'_>,
    slot: UrlAttrSlot,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> bool {
    match slot {
        UrlAttrSlot::AnchorHref => {
            is_href_only_expr(expr, scopes, policy) || is_src_and_href_expr(expr, scopes, policy)
        }
        UrlAttrSlot::Cite => is_href_only_expr(expr, scopes, policy),
        UrlAttrSlot::SrcAndHref => is_src_and_href_expr(expr, scopes, policy),
        UrlAttrSlot::LinkCss => {
            matches!(&expr, Expr::GetItem(g)
                if matches!(&g.expr, Expr::Var(v) if v.id == "theme_assets")
                    && matches!(
                        &g.subscript_expr,
                        Expr::Const(c) if c.value.as_str().is_some_and(|p| {
                            policy.allows_declared_asset(p)
                                && is_safe_theme_relative_path(p)
                                && p.ends_with(".css")
                        })
                    )
            )
        }
    }
}

fn classify_theme_url_slot(tag_name: &str, attr_name: &str) -> Option<UrlAttrSlot> {
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

fn reject_style_control_flow(states: &BTreeSet<ThemeHtmlState>) -> Result<(), &'static str> {
    if states.iter().any(|s| {
        matches!(
            s.inner(),
            ThemeHtmlState::Style { .. } | ThemeHtmlState::Script { .. }
        ) || matches!(
            s.inner(),
            ThemeHtmlState::Tag { attr_name, .. }
                if forbids_dynamic_interpolation(attr_name)
        )
    }) {
        return Err("C-SEC cấm Jinja trong <style>/<script> và thuộc tính sự kiện/namespace HTML.");
    }
    Ok(())
}

fn walk_theme_ast_with_url_scopes(
    stmt: &Stmt<'_>,
    is_part: bool,
    is_full_doctype_doc: bool,
    policy: &ThemeUrlPolicy,
    allow_trusted_theme_markup: bool,
) -> Result<ThemeTemplateUsageFlags, &'static str> {
    let mut usage = ThemeTemplateUsageFlags::default();
    let (final_states, _) = walk_theme_stmt_with_url_scopes(
        stmt,
        BTreeSet::from([ThemeHtmlState::with_trust(
            ThemeHtmlState::Body,
            allow_trusted_theme_markup,
            0,
        )]),
        UrlScopeStack::root(),
        is_part,
        is_full_doctype_doc,
        policy,
        allow_trusted_theme_markup,
        &mut usage,
    )?;
    if final_states
        .iter()
        .any(|state| state.inner() != &ThemeHtmlState::Body || state.trusted_depth() != 0)
    {
        return Err(
            "Template phải kết thúc ở trạng thái thân HTML (đóng đủ thẻ/comment/style/title).",
        );
    }
    Ok(usage)
}

fn walk_theme_stmts_with_url_scopes(
    stmts: &[Stmt<'_>],
    mut states: BTreeSet<ThemeHtmlState>,
    mut scopes: UrlScopeStack,
    is_part: bool,
    is_full_doctype_doc: bool,
    policy: &ThemeUrlPolicy,
    allow_trusted_theme_markup: bool,
    usage: &mut ThemeTemplateUsageFlags,
) -> Result<(BTreeSet<ThemeHtmlState>, UrlScopeStack), &'static str> {
    for child in stmts {
        let (next_states, next_scopes) = walk_theme_stmt_with_url_scopes(
            child,
            states,
            scopes,
            is_part,
            is_full_doctype_doc,
            policy,
            allow_trusted_theme_markup,
            usage,
        )?;
        states = next_states;
        scopes = next_scopes;
        if states.len() > MAX_THEME_WALKER_STATES {
            return Err("Quá nhiều trạng thái HTML trong theme template.");
        }
    }
    Ok((states, scopes))
}

fn walk_theme_stmt_with_url_scopes(
    stmt: &Stmt<'_>,
    states: BTreeSet<ThemeHtmlState>,
    mut scopes: UrlScopeStack,
    is_part: bool,
    is_full_doctype_doc: bool,
    policy: &ThemeUrlPolicy,
    allow_trusted_theme_markup: bool,
    usage: &mut ThemeTemplateUsageFlags,
) -> Result<(BTreeSet<ThemeHtmlState>, UrlScopeStack), &'static str> {
    match stmt {
        Stmt::Template(t) => walk_theme_stmts_with_url_scopes(
            &t.children,
            states,
            scopes,
            is_part,
            is_full_doctype_doc,
            policy,
            allow_trusted_theme_markup,
            usage,
        ),
        Stmt::EmitRaw(raw) => {
            let mut next_states = BTreeSet::new();
            for initial in states {
                let mut cur = initial;
                for ch in raw.raw.chars() {
                    let depth = cur.trusted_depth();
                    cur = step_theme_raw_char(
                        cur,
                        ch,
                        is_part,
                        is_full_doctype_doc,
                        allow_trusted_theme_markup,
                        depth,
                    )?;
                }
                next_states.insert(cur);
            }
            if next_states.len() > MAX_THEME_WALKER_STATES {
                return Err("Quá nhiều trạng thái HTML trong theme template.");
            }
            Ok((next_states, scopes))
        }
        Stmt::EmitExpr(expr) => {
            reject_style_control_flow(&states)?;
            if parse_od_icon_call(&expr.expr)?.is_some() {
                if states.iter().any(|s| s.inner() != &ThemeHtmlState::Body) {
                    return Err("od_icon() chỉ được xuất trong thân HTML.");
                }
                return Ok((states, scopes));
            }
            if let Some((_, arg_expr)) = parse_od_hook_call(&expr.expr)? {
                if states.iter().any(|s| s.inner() != &ThemeHtmlState::Body) {
                    return Err("od_hook() chỉ được xuất trong thân HTML.");
                }
                if let Some(arg) = arg_expr {
                    record_usage_flags_in_expr(arg, usage);
                }
                return Ok((states, scopes));
            }
            if parse_composition_call(&expr.expr)?.is_some() {
                if states.iter().any(|s| s.inner() != &ThemeHtmlState::Body) {
                    return Err("od_part/od_content chỉ được xuất hiện trong thân HTML.");
                }
                return Ok((states, scopes));
            }
            record_usage_flags_in_expr(&expr.expr, usage);
            if states.iter().any(|s| {
                matches!(
                    s.inner(),
                    ThemeHtmlState::Style { .. }
                        | ThemeHtmlState::Script { .. }
                        | ThemeHtmlState::Comment(_)
                        | ThemeHtmlState::BodyLt
                        | ThemeHtmlState::BodyLtSlash
                        | ThemeHtmlState::BodyLtBang { .. }
                        | ThemeHtmlState::Doctype
                        | ThemeHtmlState::Title {
                            closing_prefix: 1..
                        }
                )
            }) {
                return Err(
                    "C-SEC cấm nội suy Jinja ở comment, <style>, DOCTYPE hoặc biên thẻ HTML.",
                );
            }
            let has_non_tag = states
                .iter()
                .any(|s| !matches!(s.inner(), ThemeHtmlState::Tag { .. }));
            let has_tag = states
                .iter()
                .any(|s| matches!(s.inner(), ThemeHtmlState::Tag { .. }));
            if has_non_tag && has_tag {
                return Err("C-SEC cấm trộn lẫn ngữ cảnh thẻ HTML và thân HTML tại điểm nội suy.");
            }
            if !has_tag {
                return Ok((states, scopes));
            }

            let mut expected_url_slot: Option<Option<UrlAttrSlot>> = None;
            let mut updated_states = BTreeSet::new();
            for state in states {
                let trusted = allow_trusted_theme_markup;
                let svg_depth = state.trusted_depth();
                let ThemeHtmlState::Tag {
                    tag_name,
                    is_close,
                    phase,
                    attr_name,
                    quote,
                    attr_has_static_bytes,
                    mut attr_expr_count,
                    seen_attrs,
                    link_rel_stylesheet,
                    mut link_has_valid_href,
                    meta_ok,
                    attr_static_buf,
                } = state.into_inner()
                else {
                    unreachable!("non-tag state already filtered");
                };
                if is_close || phase != TagPhase::AttrValue || quote.is_none() {
                    return Err(
                        "C-SEC chỉ cho phép nội suy bên trong giá trị thuộc tính có dấu ngoặc kép.",
                    );
                }
                if forbids_dynamic_interpolation(&attr_name)
                    || matches!(attr_name.as_str(), "style" | "srcset" | "ping")
                    || (tag_name == "link" && attr_name == "rel")
                    || tag_name == "meta"
                {
                    return Err(
                        "C-SEC cấm nội suy bên trong thuộc tính bảo mật hoặc sự kiện HTML.",
                    );
                }
                if matches!(attr_name.as_str(), "href" | "src" | "poster" | "cite") {
                    if attr_has_static_bytes || attr_expr_count != 0 {
                        return Err(
                            "URL động trong thuộc tính chỉ được phép là biểu thức {{ ... }} đơn.",
                        );
                    }
                    let Some(slot) = classify_theme_url_slot(&tag_name, &attr_name) else {
                        return Err("C-SEC cấm nội suy URL động trên thuộc tính hoặc thẻ này.");
                    };
                    if let Some(prev_slot) = expected_url_slot {
                        if prev_slot != Some(slot) {
                            return Err("Nhóm thuộc tính URL phải đồng nhất qua các nhánh rẽ.");
                        }
                    } else {
                        expected_url_slot = Some(Some(slot));
                    }
                    if !is_valid_theme_url_expr(&expr.expr, slot, &scopes, policy) {
                        return Err(
                            "URL động trong thuộc tính không nằm trong whitelist typed URL.",
                        );
                    }
                    attr_expr_count = 1;
                    if slot == UrlAttrSlot::LinkCss {
                        link_has_valid_href = true;
                    }
                } else {
                    if let Some(prev_slot) = expected_url_slot {
                        if prev_slot.is_some() {
                            return Err(
                                "Không thể trộn thuộc tính URL và thuộc tính thường qua các nhánh.",
                            );
                        }
                    } else {
                        expected_url_slot = Some(None);
                    }
                    attr_expr_count = attr_expr_count.saturating_add(1);
                }
                updated_states.insert(ThemeHtmlState::with_trust(
                    ThemeHtmlState::Tag {
                        tag_name,
                        is_close,
                        phase,
                        attr_name,
                        quote,
                        attr_has_static_bytes,
                        attr_expr_count,
                        seen_attrs,
                        link_rel_stylesheet,
                        link_has_valid_href,
                        meta_ok,
                        attr_static_buf,
                    },
                    trusted,
                    svg_depth,
                ));
            }
            Ok((updated_states, scopes))
        }
        Stmt::IfCond(cond) => {
            reject_style_control_flow(&states)?;
            validate_expr(&cond.expr)?;
            record_usage_flags_in_expr(&cond.expr, usage);
            let (mut true_states, true_scopes) = walk_theme_stmts_with_url_scopes(
                &cond.true_body,
                states.clone(),
                scopes.clone(),
                is_part,
                is_full_doctype_doc,
                policy,
                allow_trusted_theme_markup,
                usage,
            )?;
            let (false_states, false_scopes) = walk_theme_stmts_with_url_scopes(
                &cond.false_body,
                states,
                scopes,
                is_part,
                is_full_doctype_doc,
                policy,
                allow_trusted_theme_markup,
                usage,
            )?;
            true_states.extend(false_states);
            if true_states.len() > MAX_THEME_WALKER_STATES {
                return Err("Quá nhiều trạng thái HTML tại nhánh điều kiện.");
            }
            Ok((
                true_states,
                UrlScopeStack::intersect(&true_scopes, &false_scopes),
            ))
        }
        Stmt::WithBlock(w) => {
            reject_style_control_flow(&states)?;
            let mut frame = BTreeMap::new();
            for (k, v) in &w.assignments {
                validate_expr(k)?;
                validate_expr(v)?;
                record_usage_flags_in_expr(v, usage);
                for ident in extract_bound_idents(k)? {
                    if ident == "entry" {
                        return Err("C-SEC cấm gán lại biến ngữ cảnh `entry` trong with.");
                    }
                    frame.insert(ident, None);
                }
            }
            scopes.push_frame(frame);
            let (out_states, mut out_scopes) = walk_theme_stmts_with_url_scopes(
                &w.body,
                states,
                scopes,
                is_part,
                is_full_doctype_doc,
                policy,
                allow_trusted_theme_markup,
                usage,
            )?;
            out_scopes.pop_frame();
            Ok((out_states, out_scopes))
        }
        Stmt::Set(s) => {
            reject_style_control_flow(&states)?;
            validate_expr(&s.target)?;
            validate_expr(&s.expr)?;
            record_usage_flags_in_expr(&s.expr, usage);
            for ident in extract_bound_idents(&s.target)? {
                if ident == "entry" {
                    return Err("C-SEC cấm gán lại biến ngữ cảnh `entry` qua set.");
                }
                scopes.insert_top(ident, None);
            }
            Ok((states, scopes))
        }
        Stmt::SetBlock(sb) => {
            reject_style_control_flow(&states)?;
            reject_icons_in_set_block(&sb.body)?;
            if sb.filter.is_some() {
                return Err("C-SEC cấm modifier filter trong khối set.");
            }
            validate_expr(&sb.target)?;
            let target_idents = extract_bound_idents(&sb.target)?;
            if target_idents.iter().any(|id| id == "entry") {
                return Err("C-SEC cấm gán lại biến ngữ cảnh `entry` qua set block.");
            }
            let (sb_states, sb_scopes) = walk_theme_stmts_with_url_scopes(
                &sb.body,
                BTreeSet::from([ThemeHtmlState::with_trust(
                    ThemeHtmlState::Body,
                    allow_trusted_theme_markup,
                    0,
                )]),
                scopes.clone(),
                is_part,
                is_full_doctype_doc,
                policy,
                allow_trusted_theme_markup,
                usage,
            )?;
            if sb_states
                .iter()
                .any(|s| s.inner() != &ThemeHtmlState::Body || s.trusted_depth() != 0)
            {
                return Err("Khối set block phải đóng đủ thẻ/comment HTML bên trong.");
            }
            scopes = UrlScopeStack::intersect(&scopes, &sb_scopes);
            for ident in target_idents {
                scopes.insert_top(ident, None);
            }
            Ok((states, scopes))
        }
        Stmt::ForLoop(f) => {
            reject_style_control_flow(&states)?;
            validate_expr(&f.target)?;
            validate_expr(&f.iter)?;
            record_usage_flags_in_expr(&f.iter, usage);
            if let Some(cond) = &f.filter_expr {
                validate_expr(cond)?;
                record_usage_flags_in_expr(cond, usage);
            }

            let (else_states, else_scopes) = walk_theme_stmts_with_url_scopes(
                &f.else_body,
                states.clone(),
                scopes.clone(),
                is_part,
                is_full_doctype_doc,
                policy,
                allow_trusted_theme_markup,
                usage,
            )?;

            let target_idents = extract_bound_idents(&f.target)?;
            let iter_kind = resolve_collection_source(&f.iter, &scopes);
            if target_idents.iter().any(|id| id == "entry") {
                let valid_entry_loop = matches!(&f.target, Expr::Var(v) if v.id == "entry")
                    && iter_kind == Some(UrlSourceKind::TrustedEntry);
                if !valid_entry_loop {
                    return Err("C-SEC cấm gán lại biến ngữ cảnh `entry` từ nguồn không tin cậy.");
                }
            }

            let loop_targets: BTreeMap<String, Option<UrlSourceKind>> =
                if let Expr::Var(v) = &f.target {
                    BTreeMap::from([(v.id.to_owned(), iter_kind)])
                } else {
                    target_idents.into_iter().map(|id| (id, None)).collect()
                };

            let mut loop_states = states;
            let mut loop_scopes = scopes;
            loop_scopes.push_frame(loop_targets.clone());

            let mut settled = false;
            let mut final_loop_states = BTreeSet::new();
            let mut final_loop_scopes = loop_scopes.clone();

            for _ in 0..MAX_THEME_WALKER_STATES {
                let (after_states, after_scopes) = walk_theme_stmts_with_url_scopes(
                    &f.body,
                    loop_states.clone(),
                    loop_scopes.clone(),
                    is_part,
                    is_full_doctype_doc,
                    policy,
                    allow_trusted_theme_markup,
                    usage,
                )?;
                let mut next_states = loop_states.clone();
                next_states.extend(after_states);
                if next_states.len() > MAX_THEME_WALKER_STATES {
                    return Err("Quá nhiều trạng thái HTML trong vòng lặp.");
                }
                let next_scopes = UrlScopeStack::intersect(&loop_scopes, &after_scopes);
                let mut next_header_scopes = next_scopes.clone();
                for (k, v) in &loop_targets {
                    next_header_scopes.insert_top(k.clone(), *v);
                }
                if next_states == loop_states && next_header_scopes == loop_scopes {
                    final_loop_states = next_states;
                    final_loop_scopes = next_scopes;
                    settled = true;
                    break;
                }
                loop_states = next_states;
                loop_scopes = next_header_scopes;
            }
            if !settled {
                return Err("Không thể hội tụ trạng thái HTML/URL trong vòng lặp.");
            }

            final_loop_scopes.pop_frame();
            let mut combined_states = final_loop_states;
            combined_states.extend(else_states);
            if combined_states.len() > MAX_THEME_WALKER_STATES {
                return Err("Quá nhiều trạng thái HTML sau vòng lặp.");
            }
            let combined_scopes = UrlScopeStack::intersect(&final_loop_scopes, &else_scopes);
            Ok((combined_states, combined_scopes))
        }
        _ => Err("Template sử dụng câu lệnh MiniJinja bị C-SEC cấm."),
    }
}

fn validate_html_and_css_context(
    src: &str,
    post_render: bool,
    allow_trusted_theme_markup: bool,
) -> Result<(), &'static str> {
    validate_html_and_css_context_with_url_vars(src, post_render, None, allow_trusted_theme_markup)
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
    validate_html_and_css_context_with_url_vars(src, true, None, false)
}

fn find_rawtext_or_rcdata_end(
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
    allow_trusted_theme_markup: bool,
) -> Result<(), &'static str> {
    let lower = src.to_ascii_lowercase();
    let is_full_doctype_doc =
        block_url_vars.is_none() && lower.trim_start().starts_with("<!doctype");

    // Scan HTML plus RAWTEXT style/script and RCDATA title. SVG depth limits
    // the exceptional <title> permission to actual SVG foreign content.
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut svg_depth = 0_usize;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if lower[i..].starts_with("<!--") {
            if let Some(end) = find_html5_comment_end_in_template(bytes, i) {
                i = end;
                continue;
            }
            return Err("HTML comment không đóng.");
        }
        if bytes.get(i + 1) == Some(&b'?') {
            return Err("Template không được chứa processing instruction `<?`.");
        }
        if bytes.get(i + 1) == Some(&b'!') {
            if lower[i..].starts_with("<!doctype")
                && bytes
                    .get(i + 9)
                    .is_some_and(|b| *b == b'>' || is_ascii_html_whitespace(*b))
            {
                if block_url_vars.is_some() {
                    return Err("Template block không được chứa DOCTYPE.");
                }
                let mut d = i + 9;
                while d < bytes.len() && bytes[d] != b'>' {
                    if matches!(bytes[d], b'<' | b'"' | b'\'' | 0x00..=0x1f | 0x7f) {
                        return Err("<!DOCTYPE> chứa ký tự không hợp lệ.");
                    }
                    d += 1;
                }
                if d >= bytes.len() {
                    return Err("DOCTYPE không đóng.");
                }
                i = d + 1;
                continue;
            }
            return Err("HTML chỉ cho phép comment <!-- hoặc <!DOCTYPE>.");
        }

        let is_close = if bytes.get(i + 1) == Some(&b'/') {
            if !bytes.get(i + 2).is_some_and(u8::is_ascii_alphabetic) {
                return Err("Thẻ đóng HTML phải bắt đầu bằng ký tự chữ cái ASCII.");
            }
            true
        } else if bytes.get(i + 1).is_some_and(u8::is_ascii_alphabetic) {
            false
        } else {
            // Plain text '<' (e.g. "1 < 2") per WHATWG HTML data state.
            i += 1;
            continue;
        };

        let mut j = i + if is_close { 2 } else { 1 };
        let name_start = j;
        while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'-') {
            j += 1;
        }
        let tag_name = lower[name_start..j].to_string();
        if tag_name.is_empty() {
            return Err("Tên thẻ HTML không hợp lệ.");
        }
        if is_close
            && matches!(
                tag_name.as_str(),
                "style" | "title" | "script" | "link" | "meta"
            )
        {
            return Err("Thẻ đóng HTML không hợp lệ ở vị trí này.");
        }
        if FORBIDDEN_TAGS.contains(&tag_name.as_str())
            && !(allow_trusted_theme_markup
                && matches!(
                    tag_name.as_str(),
                    "script" | "svg" | "foreignobject" | "math"
                ))
            && !(tag_name == "title"
                && !is_close
                && (is_full_doctype_doc || (allow_trusted_theme_markup && svg_depth > 0)))
        {
            return Err("Template chứa thẻ HTML bị C-SEC cấm.");
        }

        let k = if post_render {
            find_post_render_tag_end(bytes, j)?
        } else {
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
                return Err("Thẻ HTML không đóng đúng cú pháp.");
            }
            k
        };

        let attr_slice = &src[j..k];
        if is_close {
            if !attr_slice.trim().is_empty() {
                return Err("Thẻ đóng HTML không được chứa thuộc tính hoặc /.");
            }
            if tag_name == "svg" {
                svg_depth = svg_depth
                    .checked_sub(1)
                    .ok_or("Thẻ </svg> không có thẻ mở.")?;
            }
            i = k + 1;
            continue;
        }

        validate_tag_attributes(
            &tag_name,
            attr_slice,
            post_render,
            block_url_vars,
            allow_trusted_theme_markup,
        )?;
        let self_closing = attr_slice.trim_end().ends_with('/');
        if matches!(tag_name.as_str(), "style" | "title" | "script") && self_closing {
            return Err("Thẻ <style>/<title>/<script> không được tự đóng.");
        }
        if tag_name == "svg" && !self_closing {
            svg_depth = svg_depth
                .checked_add(1)
                .filter(|depth| *depth <= 64)
                .ok_or("SVG lồng nhau quá sâu.")?;
        }

        if tag_name == "style" {
            if block_url_vars.is_some() {
                return Err("Template block không được chứa thẻ <style> trực tiếp (dùng css_var).");
            }
            let (close_start, next_i) = find_rawtext_or_rcdata_end(src, k + 1, "style")?;
            let css_body = &src[k + 1..close_start];
            if !post_render && (css_body.contains("{{") || css_body.contains("{%")) {
                return Err("C-SEC cấm nội suy {{ ... }} bên trong <style>.");
            }
            validate_safe_css(css_body)?;
            i = next_i;
            continue;
        }
        if tag_name == "title" {
            let (_close_start, next_i) = find_rawtext_or_rcdata_end(src, k + 1, "title")?;
            i = next_i;
            continue;
        }
        if tag_name == "script" {
            let (close_start, next_i) = find_rawtext_or_rcdata_end(src, k + 1, "script")?;
            let js_body = &src[k + 1..close_start];
            // HTML5 escaped script states after a comment opener can keep a
            // literal closing tag inside JavaScript instead of closing it.
            // Reject that opener so template data cannot become executable JS.
            if js_body.contains("<!--") || js_body.contains("<![CDATA[") {
                return Err("C-SEC cam HTML escaped state va SVG CDATA trong <script>.");
            }
            if !post_render && (js_body.contains("{{") || js_body.contains("{%")) {
                return Err("C-SEC cấm nội suy Jinja bên trong <script>.");
            }
            i = next_i;
            continue;
        }
        i = k + 1;
    }
    if svg_depth != 0 {
        return Err("SVG thiếu thẻ đóng.");
    }
    Ok(())
}

fn validate_tag_attributes(
    tag_name: &str,
    attr_slice: &str,
    post_render: bool,
    block_url_vars: Option<&BlockUrlVarAllowlist>,
    allow_trusted_theme_markup: bool,
) -> Result<(), &'static str> {
    let attrs = parse_attributes(attr_slice, post_render)?;

    if tag_name == "meta" {
        if !attrs
            .keys()
            .all(|k| matches!(k.as_str(), "charset" | "name" | "content"))
        {
            return Err("C-SEC chỉ cho phép <meta charset> hoặc <meta name=\"viewport\">.");
        }
        let allowed_meta = attrs.contains_key("charset")
            || attrs
                .get("name")
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("viewport"));
        if !allowed_meta {
            return Err("C-SEC chỉ cho phép <meta charset> hoặc <meta name=\"viewport\">.");
        }
    }
    if tag_name == "link" {
        let rel_ok = attrs
            .get("rel")
            .is_some_and(|v| v.trim().eq_ignore_ascii_case("stylesheet"));
        let href_ok = attrs
            .get("href")
            .is_some_and(|v| is_valid_theme_stylesheet_href(v));
        if !(rel_ok && href_ok) {
            return Err("C-SEC chỉ cho phép <link rel=\"stylesheet\" href=\"/theme/...\">.");
        }
    }

    for (name, val) in attrs {
        if is_dangerous_attr_name(&name, allow_trusted_theme_markup) {
            return Err("Template chứa thuộc tính HTML bị C-SEC cấm.");
        }
        if !post_render
            && forbids_dynamic_interpolation(&name)
            && (val.contains("{{") || val.contains("{%"))
        {
            return Err("C-SEC cấm nội suy bên trong thuộc tính sự kiện/namespace HTML.");
        }

        if name == "style" {
            if !post_render && (val.contains("{{") || val.contains("{%")) {
                return Err("C-SEC cấm nội suy {{ ... }} bên trong thuộc tính style.");
            }
            validate_safe_css(&val)?;
        }

        if matches!(
            name.as_str(),
            "href" | "src" | "poster" | "cite" | "srcset" | "ping" | "xlink:href"
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
                } else {
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
                } else if (tag_name == "a" && name == "href") || name == "xlink:href" {
                    validate_safe_href_url(&decoded)?;
                } else {
                    validate_safe_url(&decoded)?;
                }
            }
        }
    }

    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PostRenderTagScanState {
    BeforeAttrName,
    AttrName,
    AfterAttrName,
    BeforeAttrValue,
    QuotedValue(u8),
    UnquotedValue,
}

fn find_post_render_tag_end(bytes: &[u8], start: usize) -> Result<usize, &'static str> {
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

fn parse_attributes(
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
        matches!(
            id,
            "options" | "theme_options" | "od_is_preview" | "od_icon" | "od_hook"
        ) || self.richtext_vars.contains(id)
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
    if !matches!(stmt, Stmt::Template(_) | Stmt::EmitRaw(_))
        && states
            .iter()
            .any(|state| matches!(state, HtmlScanState::Script { .. }))
    {
        return Err("C-SEC cấm nội suy và điều khiển Jinja trong <script> của block.");
    }
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
            if parse_od_icon_call(&e.expr)?.is_some() {
                if states.iter().any(|s| *s != HtmlScanState::Body) {
                    return Err("od_icon() chỉ được xuất trong thân HTML của block.");
                }
                return Ok(states.clone());
            }
            if let Some((_, arg_expr)) = parse_od_hook_call(&e.expr)? {
                if states.iter().any(|s| *s != HtmlScanState::Body) {
                    return Err("od_hook() chỉ được xuất trong thân HTML của block.");
                }
                if let Some(arg) = arg_expr {
                    check_block_expr_restrictions(arg, ctx, active_loops)?;
                }
                return Ok(states.clone());
            }
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
            reject_icons_in_set_block(&sb.body)?;
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
        || source.contains(ICON_NONCE_PREFIX)
        || source.contains(HOOK_NONCE_PREFIX)
    {
        return Err("Template block chứa chuỗi nonce nội bộ bị cấm.");
    }

    let ctx = BlockSchemaTemplateContext::from_schema(schema);
    let url_allowlist = ctx.url_allowlist();
    validate_html_and_css_context_with_url_vars(source, false, Some(&url_allowlist), true)?;

    let ast = parse(source, block_id, SyntaxConfig::default())
        .map_err(|_| "Cú pháp MiniJinja của template block không hợp lệ.")?;
    validate_stmt_with_icons(&ast, true, false)?;

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
    render_block_csec_with_options_and_budget_and_config(
        block_id,
        template_source,
        schema,
        values,
        site_options,
        is_preview,
        budget,
        &RenderFormattingConfig::default(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn render_block_csec_with_options_and_budget_and_config(
    block_id: &str,
    template_source: &str,
    schema: &opendash_core::block_schema::BlockSchemaV1,
    values: &BTreeMap<String, serde_json::Value>,
    site_options: Option<&BTreeMap<String, serde_json::Value>>,
    is_preview: bool,
    budget: &mut RenderBudget,
    fmt_cfg: &RenderFormattingConfig,
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

    let mut env = restricted_environment_with_config(fmt_cfg);
    let local_icons = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let local_hooks = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let overflowed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    install_icon_function(
        &mut env,
        &budget.icon_nonces,
        &local_icons,
        budget.html_bytes_used,
        &overflowed,
    );
    install_hook_function(&mut env, &budget.hook_nonces, &local_hooks);
    env.set_fuel(Some(node_fuel));
    env.add_template(block_id, template_source)
        .map_err(|_| "Không thể biên dịch template block.")?;
    let captured = env
        .get_template(block_id)
        .map_err(|_| "Không tìm thấy template block.")?
        .render_captured(Value::from_pairs(ctx_map))
        .map_err(|_| {
            if overflowed.load(std::sync::atomic::Ordering::Relaxed) {
                budget.html_bytes_used = MAX_RENDERED_HTML_BYTES + 1;
            }
            "Lỗi thực thi template block (fuel/recursion/undefined)."
        })?;

    let (consumed, _remaining) = captured
        .state()
        .fuel_levels()
        .ok_or("Thiếu bộ theo dõi fuel cho template block.")?;
    budget.fuel_remaining = budget.fuel_remaining.saturating_sub(consumed);

    let shell = captured.output();
    if shell.len() > MAX_RENDERED_HTML_BYTES {
        return Err("HTML render của block vượt quá giới hạn 512 KiB.");
    }
    validate_html_and_css_context(shell, true, true)?;

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
    let additions = local_icons
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let icon_growth: usize = additions
        .iter()
        .map(|(n, s)| s.len().saturating_sub(n.len()))
        .sum();
    budget.charge_html_bytes(icon_growth)?;
    budget.icon_nonces.extend(additions);
    let hook_additions = local_hooks
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    budget.hook_nonces.extend(hook_additions);
    Ok(out)
}

#[cfg(test)]
mod composition_tests {
    use super::*;

    #[test]
    fn theme_cache_reuses_immutable_compilation_and_regenerates_content_nonce() {
        let cache = ThemeCompositionCache::new();
        let first_digest = "a".repeat(64);
        let second_digest = "b".repeat(64);
        let parts = BTreeMap::from([
            (
                "header".to_owned(),
                "<header>{{ od_part('leaf') }}</header>".to_owned(),
            ),
            ("leaf".to_owned(), "<span>leaf</span>".to_owned()),
        ]);
        let policy = ThemeUrlPolicy::default();
        let body_source = "<main>{{ od_content() }}{{ od_part('header') }}</main>";
        let body = cache
            .get_or_compile(
                &first_digest,
                "page",
                body_source,
                &parts,
                CsecContentKind::Entry,
                &policy,
            )
            .unwrap();
        assert_eq!(body.max_part_depth, 2);
        let cached = cache
            .get_or_compile(
                &first_digest,
                "page",
                body_source,
                &parts,
                CsecContentKind::Entry,
                &policy,
            )
            .unwrap();
        assert!(Arc::ptr_eq(&body, &cached));
        let other_revision = cache
            .get_or_compile(
                &second_digest,
                "page",
                body_source,
                &parts,
                CsecContentKind::Entry,
                &policy,
            )
            .unwrap();
        assert!(!Arc::ptr_eq(&body, &other_revision));

        let header_source = parts.get("header").unwrap();
        let header = cache
            .get_or_compile(
                &first_digest,
                "part:header",
                header_source,
                &parts,
                CsecContentKind::NonEntry,
                &policy,
            )
            .unwrap();
        assert_eq!(header.max_part_depth, 1);
        let cached_header = cache
            .get_or_compile(
                &first_digest,
                "part:header",
                header_source,
                &parts,
                CsecContentKind::NonEntry,
                &policy,
            )
            .unwrap();
        assert!(Arc::ptr_eq(&header, &cached_header));

        // The immutable compiled program retains the internal slot token.
        // Rendering creates a new random 128-bit nonce for every request.
        let render = || {
            body.render(
                json_to_safe_value(&serde_json::json!({})),
                Some("<p>ok</p>"),
            )
        };
        assert_eq!(render().unwrap(), render().unwrap());
        let invalid_digest = cache
            .get_or_compile(
                "invalid",
                "page",
                body_source,
                &parts,
                CsecContentKind::Entry,
                &policy,
            )
            .unwrap();
        let uncached = cache
            .get_or_compile(
                "invalid",
                "page",
                body_source,
                &parts,
                CsecContentKind::Entry,
                &policy,
            )
            .unwrap();
        assert!(!Arc::ptr_eq(&invalid_digest, &uncached));
    }

    #[test]
    fn curated_icon_is_replaced_only_after_document_normalization() {
        let compiled = validate_and_compile_csec_composition(
            "page",
            "<main>{{ od_icon('calendar', 'w-3.5 h-3.5') }}{{ od_content() }}</main>",
            &BTreeMap::new(),
            CsecContentKind::Entry,
        )
        .unwrap();
        let mut budget = RenderBudget::new();
        let shell = compiled
            .render_with_config_and_budget(
                json_to_safe_value(&serde_json::json!({})),
                Some("<p>OK</p>"),
                &mut budget,
                &RenderFormattingConfig::default(),
            )
            .unwrap();
        assert!(shell.contains(ICON_NONCE_PREFIX));
        assert!(!shell.contains("<svg"));
        // Exercise the same document-normalization boundary without coupling
        // this module's unit tests to the server's SEO/content-render modules.
        let composed = format!("<!doctype html><html><body>{shell}</body></html>");
        let final_html = budget
            .icon_nonces
            .substitute_into_document(composed)
            .unwrap();
        assert!(final_html.contains("class=\"w-3.5 h-3.5\""));
        assert!(final_html.contains("<svg xmlns="));
        assert!(!final_html.contains(ICON_NONCE_PREFIX));
    }

    #[test]
    fn icon_security_validation_rejects_unsafe_contexts() {
        let invalid = [
            "<main>OD_ICON_0123456789abcdef0123456789abcdef</main>",
            "<main><iframe src='/x'></iframe></main>",
            "<main>{{ od_icon('not-in-registry') }}</main>",
            "<main>{{ od_icon('calendar', 'w-3 onclick=alert(1)') }}</main>",
            "<main>{{ od_icon('calendar') | upper }}</main>",
            "<main>{{ od_icon(name) }}</main>",
            "<div title=\"{{ od_icon('calendar') }}\">x</div>",
            "<style>{{ od_icon('calendar') }}</style>",
            "{% set foo %}{{ od_icon('calendar') }}{% endset %}<main>x</main>",
            "{% set od_icon = 'fake' %}<main>test</main>",
        ];
        for source in invalid {
            assert!(
                validate_and_compile_csec_composition(
                    "t",
                    source,
                    &BTreeMap::new(),
                    CsecContentKind::NonEntry
                )
                .is_err(),
                "should reject: {source}"
            );
        }
        assert!(
            render_csec(
                "legacy",
                "<main>{{ od_icon('calendar') }}</main>",
                Value::from(())
            )
            .is_err()
        );
    }

    #[test]
    fn icon_nonce_rejects_attribute_and_style_and_duplicate_substitution() {
        let nonce = "OD_ICON_0123456789abcdef0123456789abcdef".to_owned();
        let table = IconNonceTable {
            entries: vec![(nonce.clone(), "<svg/>".to_owned())],
        };
        assert!(
            table
                .substitute_into_document(format!("<p>{nonce}</p>"))
                .unwrap()
                .contains("<svg/>")
        );
        assert!(
            table
                .substitute_into_document(format!("<div title=\"{nonce}\"></div>"))
                .is_err()
        );
        assert!(
            table
                .substitute_into_document(format!("<!-- {nonce} -->"))
                .is_err()
        );
        assert!(
            table
                .substitute_into_document(format!("<style>{nonce}</style>"))
                .is_err()
        );
        assert!(
            table
                .substitute_into_document(format!("<p>{nonce}{nonce}</p>"))
                .is_err()
        );
        assert!(
            table
                .substitute_into_document("<p>nonce missing</p>".to_owned())
                .is_ok()
        );
    }

    #[test]
    fn icon_budget_caps_128_total_across_segments_and_charges_growth() {
        let compiled = validate_and_compile_csec_composition(
            "p",
            "<p>{{ od_icon('check') }}</p>",
            &BTreeMap::new(),
            CsecContentKind::NonEntry,
        )
        .unwrap();
        let mut budget = RenderBudget::new();
        for _ in 0..MAX_ICONS_PER_REQUEST {
            compiled
                .render_with_config_and_budget(
                    Value::from(()),
                    None,
                    &mut budget,
                    &RenderFormattingConfig::default(),
                )
                .unwrap();
        }
        assert_eq!(budget.icon_nonces.entries.len(), MAX_ICONS_PER_REQUEST);
        assert!(budget.html_bytes_used > 0);
        assert!(
            compiled
                .render_with_config_and_budget(
                    Value::from(()),
                    None,
                    &mut budget,
                    &RenderFormattingConfig::default()
                )
                .is_err()
        );

        let mut budget = RenderBudget::new();
        budget.html_bytes_used = MAX_RENDERED_HTML_BYTES - 1;
        assert!(
            compiled
                .render_with_config_and_budget(
                    Value::from(()),
                    None,
                    &mut budget,
                    &RenderFormattingConfig::default()
                )
                .is_err()
        );
        assert!(budget.html_bytes_used > MAX_RENDERED_HTML_BYTES);
        assert!(budget.icon_nonces.is_empty());
    }

    #[test]
    fn declarative_block_icon_renders_once_and_rejects_attribute_spoofing() {
        let source = "<div>{{ od_icon('arrow-right', 'w-3.5') }}</div>";
        let schema: opendash_core::block_schema::BlockSchemaV1 =
            serde_json::from_value(serde_json::json!({
                "schema_version": 1,
                "id": "test-icon",
                "title": "Test",
                "category": "content",
                "render": "render.html",
                "fields": []
            }))
            .unwrap();
        validate_block_template_csec("test-icon", source, &schema).unwrap();
        let mut budget = RenderBudget::new();
        let shell = render_block_csec_with_budget(
            "test-icon",
            source,
            &schema,
            &BTreeMap::new(),
            false,
            &mut budget,
        )
        .unwrap();
        assert!(shell.contains(ICON_NONCE_PREFIX));
        assert!(!shell.contains("<svg"));
        let final_html = budget.icon_nonces.substitute_into_document(shell).unwrap();
        assert!(final_html.contains("<svg "));
        assert!(final_html.contains("class=\"w-3.5\""));
        for invalid in [
            "<div title=\"{{ od_icon('check') }}\">hi</div>",
            "<div>{{ od_icon('missing') }}</div>",
            "{% set captured %}{{ od_icon('check') }}{% endset %}<div>ok</div>",
            "<div>OD_ICON_0123456789abcdef0123456789abcdef</div>",
            "<div><object data='/x'></object></div>",
        ] {
            assert!(
                validate_block_template_csec("test-icon", invalid, &schema).is_err(),
                "{invalid}"
            );
        }
    }

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
        let malicious = parts(&[("header", "<script>{{ entry.title }}</script>")]);
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
            "<!-- --!><iframe src='/x'></iframe><!-- -->",
            "<!--><iframe src='/x'></iframe><!-- -->",
            "<!---><iframe src='/x'></iframe><!-- -->",
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

#[cfg(test)]
mod spec1_s01_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn none_formatter_and_default_wrapper_keep_strict_undefined() {
        let tpl = "<p>{{ entry.values.note }}|{{ entry.values.note | default('N/A') }}|{{ entry.values.note | d('') }}</p>";
        let out = render_csec(
            "none_test",
            tpl,
            json_to_safe_value(&json!({ "entry": { "values": { "note": null } } })),
        )
        .unwrap();
        assert_eq!(decode_html_entities_for_url(&out), "<p>|N/A|</p>");

        assert_eq!(
            render_csec(
                "none_check",
                "{% if x is none and x is defined %}ok{% endif %}",
                json_to_safe_value(&json!({ "x": null })),
            )
            .unwrap(),
            "ok"
        );
        assert!(render_csec("strict_missing", "<p>{{ missing }}</p>", Value::UNDEFINED).is_err());
    }

    #[test]
    fn date_format_supports_civil_rfc3339_naive_and_rejects_bad_inputs() {
        let cfg = RenderFormattingConfig {
            timezone: "America/New_York".to_owned(),
        };
        let tz_hcm = RenderFormattingConfig {
            timezone: "Asia/Ho_Chi_Minh".to_owned(),
        };
        // Civil YYYY-MM-DD must NOT shift to previous day on negative timezone
        assert_eq!(
            decode_html_entities_for_url(
                &render_csec_with_config(
                    "civil",
                    "{{ d | date_format }}",
                    json_to_safe_value(&json!({ "d": "2026-06-15" })),
                    &cfg,
                )
                .unwrap()
            ),
            "15/06/2026"
        );
        // RFC 3339 shifts from UTC midnight-ish to Asia/Ho_Chi_Minh (+7h -> 16/06)
        assert_eq!(
            render_csec_with_config(
                "rfc",
                "{{ d | date_format('%Y-%m-%d %H:%M') }}",
                json_to_safe_value(&json!({ "d": "2026-06-15T23:30:00Z" })),
                &tz_hcm,
            )
            .unwrap(),
            "2026-06-16 06:30"
        );
        // Naive SQLite treated as UTC then shifted to explicit tz arg
        assert_eq!(
            decode_html_entities_for_url(
                &render_csec_with_config(
                    "sqlite",
                    "{{ d | date_format('%d/%m %H:%M', 'Asia/Ho_Chi_Minh') }}",
                    json_to_safe_value(&json!({ "d": "2026-06-15 23:30:00" })),
                    &cfg,
                )
                .unwrap()
            ),
            "16/06 06:30"
        );
        // Malformed / blank / missing -> ""
        assert_eq!(
            render_csec_with_config(
                "bad_date",
                "{{ a | date_format }}|{{ b | date_format }}|{{ n | date_format }}",
                json_to_safe_value(&json!({ "a": "2026-1-5", "b": "not-a-date", "n": null })),
                &cfg,
            )
            .unwrap(),
            "||"
        );
        // Error cases
        for (tpl, val) in [
            ("{{ v | date_format }}", json!(123)),
            ("{{ v | date_format }}", json!("x".repeat(129))),
            ("{{ v | date_format('') }}", json!("2026-06-15")),
            (
                &format!("{{{{ v | date_format('{}') }}}}", "a".repeat(65)),
                json!("2026-06-15"),
            ),
            ("{{ v | date_format('%Z') }}", json!("2026-06-15")),
            (
                "{{ v | date_format('%Y', 'Invalid/Zone') }}",
                json!("2026-06-15T00:00:00Z"),
            ),
        ] {
            assert!(
                render_csec_with_config(
                    "err",
                    tpl,
                    json_to_safe_value(&json!({ "v": val })),
                    &cfg,
                )
                .is_err(),
                "{tpl}"
            );
        }
    }

    #[test]
    fn truncate_and_format_number_filters_are_exact_and_bounded() {
        let out = render_csec(
            "trunc_num",
            "{{ s | truncate(12) }}|{{ small | truncate(12) }}|{{ n | format_number }}|{{ f | format_number(2) }}|{{ neg0 | format_number(2) }}|{{ big | format_number(0, ',', '.') }}",
            json_to_safe_value(&json!({
                "s": "Xin chào thế giới Rust",
                "small": "Ngắn",
                "n": 1234567,
                "f": 1234.5,
                "neg0": -0.001,
                "big": 1.5e20
            })),
        )
        .unwrap();
        assert_eq!(
            out,
            "Xin chào...|Ngắn|1.234.567|1.234,50|0,00|150,000,000,000,000,000,000"
        );

        for (tpl, val) in [
            ("{{ v | truncate(0) }}", json!("abc")),
            ("{{ v | truncate(5, 'x'.repeat(33)) }}", json!("abcdefgh")),
            ("{{ v | truncate(5) }}", json!(123)),
            ("{{ v | truncate(5) }}", json!("a".repeat(128 * 1024 + 1))),
            ("{{ v | format_number }}", json!(true)),
            ("{{ v | format_number }}", json!([1, 2])),
            ("{{ v | format_number(7) }}", json!(12)),
            ("{{ v | format_number(0, '-----') }}", json!(12)),
            ("{{ v | format_number }}", json!("9".repeat(33))),
            ("{{ v | format_number }}", json!(1e35)),
            ("{{ v | format_number }}", json!("NaN")),
        ] {
            assert!(
                render_csec("err", tpl, json_to_safe_value(&json!({ "v": val }))).is_err(),
                "{tpl}"
            );
        }
    }

    #[test]
    fn batch_selectattr_rejectattr_map_and_21_tests_work_with_limits() {
        let ctx = json_to_safe_value(&json!({
            "items": [
                { "id": 1, "name": "Alpha", "active": true, "cat": { "slug": "news" } },
                { "id": 2, "name": "Beta", "active": false, "cat": null },
                { "id": 3, "name": "Alpine", "active": true, "cat": { "slug": "tech" } }
            ]
        }));
        let rendered = render_csec(
            "seq_filters",
            "{% for row in [1,2,3,4] | batch(3, 0) %}[{{ row | join(',') }}]{% endfor %}|\
             {{ items | selectattr('active') | map('name') | join(',') }}|\
             {{ items | rejectattr('id', 'even') | map(attribute='cat.slug', default='none') | join(',') }}|\
             {{ items | selectattr('id', 'even') | map(attribute='cat.slug', default='none') | join(',') }}|\
             {{ items | selectattr('name', 'startingwith', 'Alp') | selectattr('cat.slug', 'in', ['news']) | map('name') | join(',') }}|\
             {% if 2 is even and 'abc' is startingwith('a') and 'abc' is endingwith('c') and 'a' is in(['a','b']) and 5 is gt(3) %}yes{% endif %}",
            ctx,
        )
        .unwrap();
        assert_eq!(
            rendered,
            "[1,2,3][4,0,0]|Alpha,Alpine|news,tech|none|Alpha|yes"
        );

        // Security & OOM bounds
        for bad in [
            "{{ items | selectattr('__class__') }}",
            "{{ items | selectattr('a.b.c.d') }}",
            "{{ items | selectattr('id', 'safe') }}",
            "{{ items | selectattr('id', 'eq') }}",
            "{{ items | selectattr('id', 'even', 1) }}",
            "{{ items | map(attribute='name', filter='upper') }}",
            "{{ items | map('name', attribute='id') }}",
            "{{ items | batch(0) }}",
            "{{ items | batch(101) }}",
            "{{ 'not_seq' | batch(2) }}",
            "{{ items[0]['__class__'] }}",
            "{{ t[dynamic_key] }}",
            "{% if 'x' is unknown_test %}bad{% endif %}",
        ] {
            assert!(validate_template_csec("bad", bad).is_err() || render_csec(
                "bad",
                bad,
                json_to_safe_value(&json!({ "items": [{ "name": "a", "id": 1 }], "t": {}, "dynamic_key": "k" })),
            ).is_err(), "{bad}");
        }

        let too_many = json_to_safe_value(&json!({ "items": vec![json!({"id": 1}); 501] }));
        for tpl in [
            "{{ items | batch(2) }}",
            "{{ items | selectattr('id') }}",
            "{{ items | rejectattr('id') }}",
            "{{ items | map('id') }}",
        ] {
            assert!(render_csec("oom", tpl, too_many.clone()).is_err(), "{tpl}");
        }
    }

    #[test]
    fn walk_theme_ast_enforces_theme_assets_url_scopes_and_antibypass() {
        let digest = "a".repeat(64);
        let policy = ThemeUrlPolicy::new(
            BTreeSet::from(["options.cta_url".to_owned()]),
            BTreeSet::from(["options.logo_ref".to_owned()]),
            BTreeSet::from(["assets/app.css".to_owned(), "assets/logo.png".to_owned()]),
        );

        let full_doc = r#"<!DOCTYPE html><html><head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{{ entry.title }}</title>
<link rel="stylesheet" href="{{ theme_assets['assets/app.css'] }}">
</head><body>
<p>Math: 1 < 2 and 3 > 1</p>
<a href="{{ options.cta_url }}">CTA</a>
<img src="{{ options.logo_ref }}" alt="Prefix {{ entry.title }}">
<img src="{{ theme_assets['assets/logo.png'] }}" alt="Logo">
<a href="{{ locale.home_url }}">Home</a>
<blockquote cite="{{ entry.url }}">Quote</blockquote>
<q cite="{{ options.cta_url }}">Short</q>
{% for item in locale.alternates %}<a href="{{ item.url }}">{{ item.label }}</a>{% endfor %}
{% for row in entries | batch(2) %}{% for entry in row %}<a href="{{ entry.url }}">{{ entry.title }}</a>{% for f in entry.fields %}<img src="{{ f.image_url }}">{% for u in f.gallery_urls %}<img src="{{ u }}">{% endfor %}{% endfor %}{% endfor %}{% endfor %}
{% if entry.primary_category %}<a href="{{ entry.primary_category.url }}">Cat</a>{% endif %}
</body></html>"#;

        validate_template_csec_with_theme_urls("doc", full_doc, &policy).unwrap();
        let compiled = validate_and_compile_csec_composition_with_theme_urls(
            "doc",
            full_doc,
            &BTreeMap::new(),
            CsecContentKind::NonEntry,
            &policy,
        )
        .unwrap();
        assert!(!compiled.uses_related_entries);
        assert!(!compiled.uses_recent_posts);
        let rendered = compiled
            .render(
                json_to_safe_value(&json!({
                    "theme_assets": {
                        "assets/app.css": format!("/theme/{digest}/assets/app.css"),
                        "assets/logo.png": format!("/theme/{digest}/assets/logo.png")
                    },
                    "options": { "cta_url": "/contact", "logo_ref": "/media/ok.png" },
                    "locale": { "home_url": "/en", "alternates": [{ "url": "/vi", "label": "VI" }] },
                    "entry": { "title": "Post", "url": "/posts/1", "primary_category": { "url": "/cat/news" } },
                    "entries": []
                })),
                None,
            )
            .unwrap();
        assert!(rendered.contains("Math: 1 < 2"));

        let rejected = [
            // Provenance & URL slots
            r#"<a href="{{ item.url }}">untrusted item outside loop</a>"#,
            r#"<img src="{{ field.image_url }}">"#,
            r#"<img src="{{ url }}">"#,
            r#"<img src="{{ locale.home_url }}">"#,
            r#"<img src="{{ options.cta_url }}">"#,
            r#"<cite cite="{{ options.cta_url }}">bad cite tag</cite>"#,
            r#"<link rel="stylesheet" href="{{ theme_assets['assets/logo.png'] }}">"#,
            r#"<link rel="stylesheet" href="{{ theme_assets['assets/missing.css'] }}">"#,
            r#"<a href="{{ entry.url }}/tail">split url</a>"#,
            r#"<a href="/prefix{{ entry.url }}">split url</a>"#,
            // Tombstones & loop fixed point
            r#"{% set row = [{"url":"/x"}] %}{% for entry in row %}<a href="{{ entry.url }}">x</a>{% endfor %}"#,
            r#"{% for i, entry in entries %}<a href="{{ entry.url }}">x</a>{% endfor %}"#,
            r#"{% for outer in menus.primary.items %}{% for n in [1,2] %}<a href="{{ outer.url }}">x</a>{% set outer = "x" %}{% endfor %}{% endfor %}"#,
            r#"{% for outer in menus.primary.items %}{% for n in [1,2] %}<a href="{{ outer.url }}">x</a>{% set x %}{% set outer = "bad" %}{% endset %}{% endfor %}{% endfor %}"#,
            r#"{% set unused %}<script>{{ entry.title }}</script>{% endset %}"#,
            r#"{% for x in entries %}{% else %}<div id="open"{% endfor %}"#,
            r#"{% with x = 1 %}{% set y = menus.primary.items %}{% endwith %}{% for a in menus.primary.items %}{% set a = 1 %}<a href="{{ a.url }}">x</a>{% endfor %}"#,
            // HTML/RCDATA/RAWTEXT/Tag phase anti-bypass
            "<!DOCTYPE html><html><head><title></titl{{ site.title }}e><div title=\"</title><script>alert(1)</script>\"></head><body></body></html>",
            "<!DOCTYPE html><html><head><title>x</title ><script>alert(1)</script></head><body></body></html>",
            "<?x <div title=\"><script>alert(1)</script>\">",
            "</1 <div title=\"><script>alert(1)</script>\">",
            "<!doctypex><p>bad</p>",
            "<!CDATA[bad]>",
            "<style>{% if cond %}body{}{% endif %}</style>",
            "<style>body{}</style/>",
            "<style>body{}</style >",
            "<meta charset=\"utf-8\" http-equiv=\"refresh\" content=\"0;url=/x\">",
            "{% if false %}<a href=\"javascript:alert(1)\">x</a>{% endif %}",
            "<{{ 'script' }}>alert(1)</script>",
            "<!-- a{{ options.cta_url }}><script>alert(1)</script> -->",
            "</link>",
            "</meta>",
        ];
        for tpl in rejected {
            assert!(
                validate_template_csec_with_theme_urls("reject", tpl, &policy).is_err(),
                "expected C-SEC reject for: {tpl}"
            );
        }
    }

    #[test]
    fn compiled_composition_reports_lazy_related_and_recent_usage_flags() {
        let parts = BTreeMap::from([
            (
                "sidebar".to_owned(),
                "<aside>{% for p in recent_posts %}<a href=\"{{ p.url }}\">{{ p.title }}</a>{% endfor %}</aside>".to_owned(),
            ),
            (
                "dormant".to_owned(),
                "<div>{{ related_entries | length }}</div>".to_owned(),
            ),
        ]);
        let comp = validate_and_compile_csec_composition(
            "single",
            "<main>{{ od_content() }}{{ od_part('sidebar') }}</main>",
            &parts,
            CsecContentKind::Entry,
        )
        .unwrap();
        assert!(!comp.uses_related_entries);
        assert!(comp.uses_recent_posts);

        let comp_related = validate_and_compile_csec_composition(
            "single",
            "<main>{{ od_content() }}{% if related_entries %}<p>{{ related_entries | length }}</p>{% endif %}</main>",
            &parts,
            CsecContentKind::Entry,
        )
        .unwrap();
        assert!(comp_related.uses_related_entries);
        assert!(!comp_related.uses_recent_posts);
    }

    #[test]
    fn untrusted_extension_html_rejects_jinja_smuggled_attributes() {
        for bad in [
            r#"<a {{ href="javascript:alert(1)" }}>Click</a>"#,
            r#"<a {% href="javascript:alert(1)" %}>Click</a>"#,
            r#"<button {{ formaction="javascript:alert(1)" }}>Submit</button>"#,
            r#"<div {{ style="background:url(https://attacker.test/exfil)" }}>x</div>"#,
            r#"<div {% style="expression(alert(1))" %}>x</div>"#,
            r#"<a href="https://safe.example" href="javascript:alert(1)">dup</a>"#,
            // WHATWG HTML5 unquoted-attribute / attribute-name quote differential payloads (F-01)
            r#"<div data-x=a'><script>alert(1)</script>'></div>"#,
            r#"<div data-x=a'><img/src="x"/onerror="alert(1)">'></div>"#,
            r#"<div a"><script>alert(1)</script>"></div>"#,
        ] {
            assert!(
                validate_untrusted_extension_html_fragment(bad).is_err(),
                "expected untrusted C-SEC validator to reject: {bad}"
            );
        }
        assert!(
            validate_untrusted_extension_html_fragment(
                r#"<div class="ok"><a href="https://safe.example">safe</a></div>"#
            )
            .is_ok()
        );
    }
}
