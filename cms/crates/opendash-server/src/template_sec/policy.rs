//! C-SEC MiniJinja 3.0.5 safe subset, AST allowlist, and contextual HTML/URL/CSS validator.

use super::*;

pub(super) use std::collections::{BTreeMap, BTreeSet};

pub(super) use std::sync::{Arc, OnceLock, RwLock};

pub(super) use argon2::password_hash::rand_core::{OsRng, RngCore};

pub(super) use minijinja::{
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

pub const MAX_ICONS_PER_REQUEST: usize = 128;

pub const TEMPLATE_FUEL_LIMIT: u64 = 10_000;

/// Host formatting configuration supplied per request or preview to C-SEC helpers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderFormattingConfig {
    pub timezone: String,
}

impl Default for RenderFormattingConfig {
    fn default() -> Self {
        Self {
            timezone: "UTC".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum FieldTemplateNode {
    Scalar,
    Url,
    Link,
    Media,
    Gallery,
    Icon,
    Richtext,
    EntryRef { multiple: bool },
    TermRef { multiple: bool },
    Group(BTreeMap<String, FieldTemplateNode>),
    Repeater(BTreeMap<String, FieldTemplateNode>),
}

pub fn field_template_node_from_spec(
    spec: &opendash_core::field_schema::FieldSpec,
) -> Option<FieldTemplateNode> {
    use opendash_core::field_schema::FieldType;

    if spec.is_ui_only() || spec.field_type == FieldType::Secret {
        return None;
    }
    match spec.field_type {
        FieldType::Url => Some(FieldTemplateNode::Url),
        FieldType::Link => Some(FieldTemplateNode::Link),
        FieldType::Media => Some(FieldTemplateNode::Media),
        FieldType::Gallery => Some(FieldTemplateNode::Gallery),
        FieldType::Icon => Some(FieldTemplateNode::Icon),
        FieldType::Richtext => Some(FieldTemplateNode::Richtext),
        FieldType::EntryRef => Some(FieldTemplateNode::EntryRef {
            multiple: spec.multiple,
        }),
        FieldType::TermRef => Some(FieldTemplateNode::TermRef {
            multiple: spec.multiple,
        }),
        FieldType::Group => {
            let mut children = BTreeMap::new();
            for child in &spec.fields {
                if let Some(node) = field_template_node_from_spec(child) {
                    children.insert(child.key.clone(), node);
                }
            }
            Some(FieldTemplateNode::Group(children))
        }
        FieldType::Repeater => {
            let mut children = BTreeMap::new();
            for child in &spec.fields {
                if let Some(node) = field_template_node_from_spec(child) {
                    children.insert(child.key.clone(), node);
                }
            }
            Some(FieldTemplateNode::Repeater(children))
        }
        FieldType::Text
        | FieldType::Textarea
        | FieldType::Number
        | FieldType::Range
        | FieldType::Email
        | FieldType::Select
        | FieldType::Checkbox
        | FieldType::RadioButtons
        | FieldType::Boolean
        | FieldType::Color
        | FieldType::Date
        | FieldType::Datetime
        | FieldType::Time
        | FieldType::Spacing => Some(FieldTemplateNode::Scalar),
        FieldType::Secret | FieldType::Tab | FieldType::Accordion | FieldType::Message => None,
    }
}

/// Explicit, manifest-derived theme URL and declared-asset permissions.
#[derive(Debug, Clone, Default)]
pub struct ThemeUrlPolicy {
    pub(super) permitted_href_expressions: BTreeSet<String>,
    pub(super) permitted_src_expressions: BTreeSet<String>,
    pub(super) declared_assets: BTreeSet<String>,
    pub(super) site_options_tree: BTreeMap<String, FieldTemplateNode>,
    pub(super) options_tree: BTreeMap<String, FieldTemplateNode>,
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
            site_options_tree: BTreeMap::new(),
            options_tree: BTreeMap::new(),
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

    pub(crate) fn from_option_specs(
        theme_options: &[opendash_core::field_schema::FieldSpec],
        declared_assets: BTreeSet<String>,
    ) -> Self {
        let mut site_options_tree = BTreeMap::new();
        for spec in opendash_core::field_schema::global_site_option_fields() {
            if spec.is_ui_only() || !spec.public {
                continue;
            }
            if let Some(node) = field_template_node_from_spec(&spec) {
                site_options_tree.insert(spec.key, node);
            }
        }
        let mut options_tree = site_options_tree.clone();
        for spec in theme_options {
            if spec.is_ui_only() || !spec.public {
                continue;
            }
            if let Some(node) = field_template_node_from_spec(spec) {
                options_tree.insert(spec.key.clone(), node);
            }
        }
        Self {
            permitted_href_expressions: BTreeSet::new(),
            permitted_src_expressions: BTreeSet::new(),
            declared_assets,
            site_options_tree,
            options_tree,
        }
    }

    pub(super) fn allows_cite_href(&self, expr: &str) -> bool {
        self.permitted_href_expressions.contains(expr)
    }

    pub(super) fn allows_media_src(&self, expr: &str) -> bool {
        self.permitted_src_expressions.contains(expr)
    }

    pub(super) fn allows_declared_asset(&self, path: &str) -> bool {
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

impl RichTextNonceTable {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.reserved_count == 0
    }

    pub fn total_count(&self) -> usize {
        self.entries.len().saturating_add(self.reserved_count)
    }

    pub fn reserve_count(&mut self, count: usize) -> Result<(), &'static str> {
        if self.total_count().saturating_add(count) > MAX_RICHTEXT_NONCES_PER_REQUEST {
            return Err("Số lượng khối RichText vượt giới hạn 128/request.");
        }
        self.reserved_count = self.reserved_count.saturating_add(count);
        Ok(())
    }

    pub fn try_extend(&mut self, other: Self) -> Result<(), &'static str> {
        if self.total_count().saturating_add(other.total_count()) > MAX_RICHTEXT_NONCES_PER_REQUEST
        {
            return Err("Số lượng khối RichText vượt giới hạn 128/request.");
        }
        self.reserved_count = self.reserved_count.saturating_add(other.reserved_count);
        self.entries.extend(other.entries);
        Ok(())
    }

    pub fn extend(&mut self, other: Self) {
        self.reserved_count = self
            .reserved_count
            .saturating_add(other.reserved_count)
            .min(MAX_RICHTEXT_NONCES_PER_REQUEST);
        for entry in other.entries {
            if self.total_count() < MAX_RICHTEXT_NONCES_PER_REQUEST {
                self.entries.push(entry);
            }
        }
    }

    pub fn register_sanitized(&mut self, sanitized_html: String) -> Result<String, &'static str> {
        if self.total_count() >= MAX_RICHTEXT_NONCES_PER_REQUEST {
            return Err("Số lượng khối RichText vượt giới hạn 128/request.");
        }
        let nonce = generate_richtext_nonce()?;
        self.entries.push((nonce.clone(), sanitized_html));
        Ok(nonce)
    }

    pub fn substitute_into_document(&self, html: String) -> Result<String, &'static str> {
        substitute_nonces_into_body(
            &self.entries,
            html,
            MAX_RICHTEXT_NONCE_REPEATS,
            MAX_RENDERED_HTML_BYTES,
            "Nonce RichText xuất hiện vượt quá số lần cho phép.",
            "Nonce RichText không nằm trong thân HTML an toàn.",
            "HTML sau khi chèn RichText vượt giới hạn 512 KiB.",
        )
    }
}

impl IconNonceTable {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(super) fn extend(&mut self, entries: Vec<(String, String)>) {
        self.entries.extend(entries);
    }

    /// Substitute server-generated SVG only after document normalization. Any
    /// nonce that disappeared during slot fallback is intentionally ignored.
    pub fn substitute_into_document(&self, html: String) -> Result<String, &'static str> {
        substitute_nonces_into_body(
            &self.entries,
            html,
            1,
            MAX_ICON_DOCUMENT_BYTES,
            "Nonce icon xuất hiện nhiều hơn một lần.",
            "Nonce icon không nằm trong thân HTML an toàn.",
            "Document chứa icon vượt giới hạn HTML.",
        )
    }
}

pub(super) fn new_icon_nonce() -> Result<String, &'static str> {
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

pub(super) fn install_icon_function(
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
        move |name: Value, class: Option<&str>| -> Result<String, minijinja::Error> {
            if name.is_none() || name.is_undefined() {
                return Ok(String::new());
            }
            let Some(name_str) = name.as_str() else {
                return Err(filter_invalid_op("od_icon: tên icon phải là chuỗi"));
            };
            if name_str.is_empty() {
                return Ok(String::new());
            }
            let svg = opendash_core::icons::render_core_icon_svg(name_str, class)
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

pub(super) fn new_hook_nonce() -> Result<String, &'static str> {
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

pub(super) fn minijinja_value_to_json(
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

impl Default for RenderBudget {
    fn default() -> Self {
        Self {
            fuel_remaining: TEMPLATE_FUEL_LIMIT,
            html_bytes_used: 0,
            locale: "vi".to_owned(),
            icon_nonces: IconNonceTable::default(),
            richtext_nonces: RichTextNonceTable::default(),
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

impl Default for PreviewCompositionBudget {
    fn default() -> Self {
        Self {
            fuel_remaining: TEMPLATE_FUEL_LIMIT,
            html_bytes_used: 0,
            expansions: 0,
            stack: Vec::new(),
            icon_nonces: IconNonceTable::default(),
            richtext_nonces: RichTextNonceTable::default(),
            hook_nonces: HookNonceTable::default(),
        }
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

pub(super) fn preview_composition_sites(
    name: &str,
    source: &str,
    policy: &ThemeUrlPolicy,
) -> Result<Vec<CompositionSite>, &'static str> {
    if source.len() > MAX_RENDERED_HTML_BYTES {
        return Err("Template hoặc part vượt giới hạn 512 KiB.");
    }
    if source.contains(PREVIEW_PART_NONCE_PREFIX)
        || source.contains(BLOCK_RICHTEXT_NONCE_PREFIX)
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

pub(super) struct PreviewFragmentCompiler<'a> {
    pub(super) theme_sources: &'a BTreeMap<String, String>,
    pub(super) trusted_builder_parts: &'a BTreeMap<String, String>,
    pub(super) nonces: BTreeMap<String, &'a str>,
    pub(super) policy: &'a ThemeUrlPolicy,
}

impl<'a> PreviewFragmentCompiler<'a> {
    pub(super) fn flatten(
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

pub(super) fn append_preview_fragment(
    out: &mut String,
    fragment: &str,
) -> Result<(), &'static str> {
    if fragment.len() > MAX_RENDERED_HTML_BYTES.saturating_sub(out.len()) {
        return Err("HTML preview vượt quá giới hạn 512 KiB.");
    }
    out.push_str(fragment);
    Ok(())
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

pub(super) fn validate_template_csec_with_usage_flags(
    name: &str,
    source: &str,
    policy: &ThemeUrlPolicy,
    allow_od_icon: bool,
    allow_trusted_theme_markup: bool,
) -> Result<ThemeTemplateUsageFlags, &'static str> {
    if source.trim().is_empty() {
        return Err("Template không được để trống.");
    }
    if source.contains(BLOCK_RICHTEXT_NONCE_PREFIX)
        || source.contains(ICON_NONCE_PREFIX)
        || source.contains(HOOK_NONCE_PREFIX)
    {
        return Err("Theme HTML chứa marker icon nội bộ bị cấm.");
    }
    let ast = parse(source, name, SyntaxConfig::default())
        .map_err(|_| "Cú pháp template MiniJinja không hợp lệ.")?;

    validate_stmt_with_icons_ext(&ast, allow_od_icon, allow_od_icon, false)?;
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

#[allow(clippy::too_many_arguments)]
pub(super) fn flatten_composition(
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

pub(super) fn composition_sites(
    name: &str,
    source: &str,
    is_part: bool,
    policy: &ThemeUrlPolicy,
) -> Result<Vec<CompositionSite>, &'static str> {
    if source.trim().is_empty() {
        return Err("Template không được để trống.");
    }
    if source.contains(CONTENT_SLOT_TOKEN)
        || source.contains(BLOCK_RICHTEXT_NONCE_PREFIX)
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum UrlSourceKind {
    TrustedEntry,
    TrustedEntryBatch,
    TrustedLink,
    TrustedField,
    TrustedRawUrl,
    FieldNode(FieldTemplateNode),
}

pub(super) fn step_theme_raw_char(
    state: ThemeHtmlState,
    ch: char,
    opts: HtmlStepOptions,
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
        let next = step_theme_raw_char(
            *inner,
            ch,
            HtmlStepOptions {
                allow_trusted_theme_markup: true,
                ..opts
            },
            depth,
        )?;
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
                if opts.is_part {
                    return Err("Part không được chứa DOCTYPE.");
                }
                if opts.forbid_style_tag {
                    return Err("Template block không được chứa <!DOCTYPE>.");
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
                        check_theme_tag_name(&tag_name, is_close, opts, svg_depth)?;
                        phase = TagPhase::BeforeAttr;
                    }
                    Some(b'/') => {
                        if is_close
                            || tag_name.is_empty()
                            || matches!(tag_name.as_str(), "style" | "title" | "script")
                        {
                            return Err("Dấu tự đóng `/` trên thẻ HTML không hợp lệ.");
                        }
                        check_theme_tag_name(&tag_name, is_close, opts, svg_depth)?;
                        phase = TagPhase::SelfClosingSlash;
                    }
                    Some(b'>') => {
                        return finalize_theme_tag(
                            &tag_name,
                            is_close,
                            link_rel_stylesheet,
                            link_has_valid_href,
                            meta_ok,
                            opts,
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
                                    opts,
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
                                    opts,
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
                            opts.allow_trusted_theme_markup,
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
                            opts.allow_trusted_theme_markup,
                        )?;
                        return finalize_theme_tag(
                            &tag_name,
                            is_close,
                            link_rel_stylesheet,
                            link_has_valid_href,
                            meta_ok,
                            opts,
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
                            opts.allow_trusted_theme_markup,
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
                            opts.allow_trusted_theme_markup,
                        )?;
                        return finalize_theme_tag(
                            &tag_name,
                            is_close,
                            link_rel_stylesheet,
                            link_has_valid_href,
                            meta_ok,
                            opts,
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
                            opts.allow_trusted_theme_markup,
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
                                opts.allow_trusted_theme_markup,
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
                            opts.allow_trusted_theme_markup,
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
                            opts.allow_trusted_theme_markup,
                        )?;
                        return finalize_theme_tag(
                            &tag_name,
                            is_close,
                            link_rel_stylesheet,
                            link_has_valid_href,
                            meta_ok,
                            opts,
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
                            opts,
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

pub(super) fn resolve_field_node_and_tail<'p, 'a, 's>(
    root_id: &str,
    attrs: &'a [&'s str],
    scopes: &'p UrlScopeStack,
    policy: &'p ThemeUrlPolicy,
) -> Option<(&'p FieldTemplateNode, &'a [&'s str])> {
    let (mut node, mut rest): (&FieldTemplateNode, &[&'s str]) = match root_id {
        "options" => {
            let first = *attrs.first()?;
            (policy.options_tree.get(first)?, &attrs[1..])
        }
        "site_options" => {
            let first = *attrs.first()?;
            (policy.site_options_tree.get(first)?, &attrs[1..])
        }
        other => match scopes.lookup(other) {
            Some(UrlSourceKind::FieldNode(node)) => (node, attrs),
            _ => return None,
        },
    };
    while let FieldTemplateNode::Group(children) = node
        && !rest.is_empty()
    {
        let child = children.get(rest[0])?;
        node = child;
        rest = &rest[1..];
    }
    Some((node, rest))
}

pub(super) fn is_richtext_field_expr(
    expr: &Expr<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> bool {
    let Some((root, attrs)) = extract_attr_chain(expr) else {
        return false;
    };
    matches!(
        resolve_field_node_and_tail(root, &attrs, scopes, policy),
        Some((FieldTemplateNode::Richtext, &[]))
    )
}

pub(super) fn resolve_collection_source(
    expr: &Expr<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> Option<UrlSourceKind> {
    match expr {
        Expr::Slice(s) => resolve_collection_source(&s.expr, scopes, policy),
        Expr::Filter(f) => {
            let inner = f.expr.as_ref()?;
            if matches!(f.name, "selectattr" | "rejectattr" | "sort" | "reverse") {
                resolve_collection_source(inner, scopes, policy)
            } else if f.name == "batch" {
                match resolve_collection_source(inner, scopes, policy) {
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
            } else if scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedEntryBatch) {
                Some(UrlSourceKind::TrustedEntry)
            } else {
                None
            }
        }
        Expr::GetAttr(g) => {
            if let Some((root, attrs)) = extract_attr_chain(expr)
                && let Some((node, &[])) = resolve_field_node_and_tail(root, &attrs, scopes, policy)
            {
                return match node {
                    FieldTemplateNode::Repeater(fields) => Some(UrlSourceKind::FieldNode(
                        FieldTemplateNode::Group(fields.clone()),
                    )),
                    FieldTemplateNode::Gallery => {
                        Some(UrlSourceKind::FieldNode(FieldTemplateNode::Media))
                    }
                    FieldTemplateNode::EntryRef { .. } => {
                        Some(UrlSourceKind::FieldNode(FieldTemplateNode::EntryRef {
                            multiple: false,
                        }))
                    }
                    FieldTemplateNode::TermRef { .. } => {
                        Some(UrlSourceKind::FieldNode(FieldTemplateNode::TermRef {
                            multiple: false,
                        }))
                    }
                    _ => None,
                };
            }
            match &g.expr {
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
                        && scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedLink) =>
                {
                    Some(UrlSourceKind::TrustedLink)
                }
                Expr::Var(v)
                    if g.name == "all_terms"
                        && scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedEntry) =>
                {
                    Some(UrlSourceKind::TrustedLink)
                }
                Expr::GetAttr(terms_attr)
                    if terms_attr.name == "terms"
                        && matches!(
                            &terms_attr.expr,
                            Expr::Var(v) if scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedEntry)
                        ) =>
                {
                    Some(UrlSourceKind::TrustedLink)
                }
                Expr::Var(v)
                    if g.name == "fields"
                        && scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedEntry) =>
                {
                    Some(UrlSourceKind::TrustedField)
                }
                Expr::Var(v)
                    if g.name == "gallery_urls"
                        && scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedField) =>
                {
                    Some(UrlSourceKind::TrustedRawUrl)
                }
                _ => None,
            }
        }
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
                            Expr::Var(v) if scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedEntry)
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

pub(super) fn is_href_only_expr(
    expr: &Expr<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> bool {
    if let Some((root, attrs)) = extract_attr_chain(expr)
        && let Some((node, tail)) = resolve_field_node_and_tail(root, &attrs, scopes, policy)
    {
        if match (node, tail) {
            (FieldTemplateNode::Url, &[]) => true,
            (FieldTemplateNode::Link, &["url"]) => true,
            (FieldTemplateNode::EntryRef { multiple: false }, &["url"]) => true,
            (FieldTemplateNode::TermRef { multiple: false }, &["url"]) => true,
            _ => false,
        } {
            return true;
        }
    }
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
                        Expr::Var(v) if scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedEntry)
                    ) =>
            {
                true
            }
            _ => false,
        },
        _ => false,
    }
}

pub(super) fn is_src_and_href_expr(
    expr: &Expr<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> bool {
    if let Some((root, attrs)) = extract_attr_chain(expr)
        && let Some((node, tail)) = resolve_field_node_and_tail(root, &attrs, scopes, policy)
    {
        if match (node, tail) {
            (FieldTemplateNode::Media, &["url" | "thumbnail_url"]) => true,
            (FieldTemplateNode::EntryRef { multiple: false }, &["cover_url"]) => true,
            (
                FieldTemplateNode::TermRef { multiple: false },
                &["featured_image", "url" | "thumbnail_url"],
            ) => true,
            _ => false,
        } {
            return true;
        }
    }
    match expr {
        Expr::Var(v) => {
            v.id == "theme_asset_url" || scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedRawUrl)
        }
        Expr::GetAttr(g) => match &g.expr {
            Expr::Var(v) if v.id == "options" => {
                policy.allows_media_src(&format!("options.{}", g.name))
            }
            Expr::Var(v)
                if g.name == "cover_url"
                    && scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedEntry) =>
            {
                true
            }
            Expr::Var(v)
                if g.name == "image_url"
                    && scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedField) =>
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

pub(super) fn is_valid_theme_url_expr(
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

pub(super) fn walk_theme_ast_with_url_scopes(
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

pub(super) fn walk_theme_stmts_with_url_scopes(
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

pub(super) fn walk_theme_stmt_with_url_scopes(
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
                        HtmlStepOptions {
                            is_part,
                            is_full_doctype_doc,
                            allow_trusted_theme_markup,
                            forbid_style_tag: false,
                        },
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
            if let Some((icon_arg, _)) = parse_od_icon_call_ext(&expr.expr, true)? {
                if states.iter().any(|s| s.inner() != &ThemeHtmlState::Body) {
                    return Err("od_icon() chỉ được xuất trong thân HTML.");
                }
                if let OdIconNameArg::FieldNamePath(root, attrs) = icon_arg {
                    let resolved = resolve_field_node_and_tail(
                        root,
                        &attrs[..attrs.len() - 1],
                        &scopes,
                        policy,
                    );
                    if !matches!(resolved, Some((FieldTemplateNode::Icon, &[]))) {
                        return Err("od_icon() động phải tham chiếu trường kiểu icon.");
                    }
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
            if is_richtext_field_expr(&expr.expr, &scopes, policy)
                && states.iter().any(|s| s.inner() != &ThemeHtmlState::Body)
            {
                return Err("Trường richtext chỉ được xuất trực tiếp trong phần thân HTML.");
            }
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
                    if matches!(ident.as_str(), "entry" | "term" | "field") {
                        return Err("C-SEC cấm gán lại biến ngữ cảnh trong with.");
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
                if matches!(ident.as_str(), "entry" | "term" | "field") {
                    return Err("C-SEC cấm gán lại biến ngữ cảnh qua set.");
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
            if target_idents
                .iter()
                .any(|id| matches!(id.as_str(), "entry" | "term" | "field"))
            {
                return Err("C-SEC cấm gán lại biến ngữ cảnh qua set block.");
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
            let iter_kind = resolve_collection_source(&f.iter, &scopes, policy);
            if target_idents.iter().any(|id| id == "entry") {
                let valid_entry_loop = matches!(&f.target, Expr::Var(v) if v.id == "entry")
                    && matches!(
                        iter_kind,
                        Some(
                            UrlSourceKind::TrustedEntry
                                | UrlSourceKind::FieldNode(FieldTemplateNode::EntryRef { .. })
                        )
                    );
                if !valid_entry_loop {
                    return Err("C-SEC cấm gán lại biến ngữ cảnh `entry` từ nguồn không tin cậy.");
                }
            }
            if target_idents.iter().any(|id| id == "field") {
                let valid_field_loop = matches!(&f.target, Expr::Var(v) if v.id == "field")
                    && iter_kind == Some(UrlSourceKind::TrustedField);
                if !valid_field_loop {
                    return Err("C-SEC cấm gán lại biến ngữ cảnh `field` từ nguồn không tin cậy.");
                }
            }
            if target_idents.iter().any(|id| id == "term") {
                let valid_term_loop = matches!(&f.target, Expr::Var(v) if v.id == "term")
                    && matches!(
                        iter_kind,
                        Some(
                            UrlSourceKind::TrustedLink
                                | UrlSourceKind::FieldNode(FieldTemplateNode::TermRef { .. })
                        )
                    );
                if !valid_term_loop {
                    return Err("C-SEC cấm gán lại biến ngữ cảnh `term` từ nguồn không tin cậy.");
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
                    next_header_scopes.insert_top(k.clone(), v.clone());
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

pub(super) struct BlockSchemaTemplateContext {
    pub(super) policy: ThemeUrlPolicy,
    pub(super) root_scopes: UrlScopeStack,
}

impl BlockSchemaTemplateContext {
    pub(super) fn from_schema(schema: &opendash_core::block_schema::BlockSchemaV1) -> Self {
        let policy = ThemeUrlPolicy::from_option_specs(&[], BTreeSet::new());
        let mut root = BTreeMap::new();
        for field in &schema.fields {
            if let Some(node) = field_template_node_from_spec(field) {
                root.insert(field.key.clone(), Some(UrlSourceKind::FieldNode(node)));
            }
        }
        Self {
            policy,
            root_scopes: UrlScopeStack::from_root_map(root),
        }
    }
}

pub(super) fn is_protected_block_binding(id: &str, scopes: &UrlScopeStack) -> bool {
    if is_forbidden_block_shadow_name(id) {
        return true;
    }
    match scopes.lookup(id) {
        None => false,
        Some(UrlSourceKind::FieldNode(FieldTemplateNode::Scalar)) => false,
        Some(_) => true,
    }
}

pub(super) fn classify_node_and_tail(
    node: &FieldTemplateNode,
    tail: &[&str],
) -> Option<BlockSensitiveKind> {
    match (node, tail) {
        (FieldTemplateNode::Scalar, &[]) => None,
        (FieldTemplateNode::Scalar, &["classes" | "css" | "top" | "right" | "bottom" | "left"]) => {
            None
        }
        (FieldTemplateNode::Scalar, _) => Some(BlockSensitiveKind::ObjectOrNonScalar),
        (FieldTemplateNode::Icon, &["name"]) => None,
        (FieldTemplateNode::Icon, _) => Some(BlockSensitiveKind::ObjectOrNonScalar),
        (FieldTemplateNode::Richtext, &[]) => Some(BlockSensitiveKind::Richtext),
        (FieldTemplateNode::Richtext, _) => Some(BlockSensitiveKind::ObjectOrNonScalar),
        (FieldTemplateNode::Url, &[]) => Some(BlockSensitiveKind::HrefUrl),
        (FieldTemplateNode::Url, _) => Some(BlockSensitiveKind::ObjectOrNonScalar),
        (FieldTemplateNode::Link, &["url"]) => Some(BlockSensitiveKind::HrefUrl),
        (FieldTemplateNode::Link, &["label" | "target" | "rel" | "new_tab" | "nofollow"]) => None,
        (FieldTemplateNode::Link, _) => Some(BlockSensitiveKind::ObjectOrNonScalar),
        (FieldTemplateNode::Media, &["url" | "thumbnail_url"]) => {
            Some(BlockSensitiveKind::MediaUrl)
        }
        (FieldTemplateNode::Media, &["id" | "alt" | "width" | "height" | "kind"]) => None,
        (FieldTemplateNode::Media, _) => Some(BlockSensitiveKind::ObjectOrNonScalar),
        (FieldTemplateNode::Gallery, _)
        | (FieldTemplateNode::Group(_), _)
        | (FieldTemplateNode::Repeater(_), _) => Some(BlockSensitiveKind::ObjectOrNonScalar),
        (FieldTemplateNode::EntryRef { multiple: false }, &["url"]) => {
            Some(BlockSensitiveKind::HrefUrl)
        }
        (FieldTemplateNode::EntryRef { multiple: false }, &["cover_url"]) => {
            Some(BlockSensitiveKind::MediaUrl)
        }
        (
            FieldTemplateNode::EntryRef { multiple: false },
            &["id" | "type_key" | "title" | "slug" | "excerpt"],
        ) => None,
        (FieldTemplateNode::EntryRef { .. }, _) => Some(BlockSensitiveKind::ObjectOrNonScalar),
        (FieldTemplateNode::TermRef { multiple: false }, &["url"]) => {
            Some(BlockSensitiveKind::HrefUrl)
        }
        (
            FieldTemplateNode::TermRef { multiple: false },
            &["featured_image", "url" | "thumbnail_url"],
        ) => Some(BlockSensitiveKind::MediaUrl),
        (
            FieldTemplateNode::TermRef { multiple: false },
            &["featured_image", "id" | "alt" | "width" | "height" | "kind"],
        ) => None,
        (
            FieldTemplateNode::TermRef { multiple: false },
            &["id" | "taxonomy_key" | "name" | "slug" | "description"],
        ) => None,
        (FieldTemplateNode::TermRef { .. }, _) => Some(BlockSensitiveKind::ObjectOrNonScalar),
    }
}

pub(super) fn classify_block_sensitive_expr(
    expr: &Expr<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> Option<BlockSensitiveKind> {
    if let Some((_sub, attrs)) = resolve_menu_get_item_subscript(expr) {
        return match attrs.as_slice() {
            ["title" | "menu_key" | "menu_id"] => None,
            _ => Some(BlockSensitiveKind::ObjectOrNonScalar),
        };
    }
    if let Some(attrs) = resolve_menu_dot_chain(expr) {
        return match attrs.as_slice() {
            [_slot, "title" | "menu_key" | "menu_id"] => None,
            _ => Some(BlockSensitiveKind::ObjectOrNonScalar),
        };
    }
    if let Some((root, attrs)) = extract_attr_chain(expr) {
        if attrs.is_empty()
            && matches!(root, "options" | "site_options" | "menus" | "theme_options")
        {
            return Some(BlockSensitiveKind::ObjectOrNonScalar);
        }
        if matches!(root, "options" | "site_options") {
            return match resolve_field_node_and_tail(root, &attrs, scopes, policy) {
                Some((node, tail)) => classify_node_and_tail(node, tail),
                None => Some(BlockSensitiveKind::ObjectOrNonScalar),
            };
        }
        if let Some(UrlSourceKind::TrustedLink) = scopes.lookup(root) {
            return classify_trusted_link_tail(&attrs);
        }
        if let Some((node, tail)) = resolve_field_node_and_tail(root, &attrs, scopes, policy) {
            return classify_node_and_tail(node, tail);
        }
        if scopes.lookup(root).is_some() {
            return Some(BlockSensitiveKind::ObjectOrNonScalar);
        }
    }
    None
}

pub(super) fn block_expr_touches_sensitive(
    expr: &Expr<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> bool {
    if let Some(attrs) = resolve_menu_dot_chain(expr) {
        return !matches!(attrs.as_slice(), [_slot, "title" | "menu_key" | "menu_id"]);
    }
    if let Some((subscript, attrs)) = resolve_menu_get_item_subscript(expr) {
        return block_expr_touches_sensitive(subscript, scopes, policy)
            || !matches!(attrs.as_slice(), ["title" | "menu_key" | "menu_id"]);
    }
    if let Some((root, attrs)) = extract_attr_chain(expr) {
        if matches!(root, "options" | "site_options") {
            return match resolve_field_node_and_tail(root, &attrs, scopes, policy) {
                Some((node, tail)) => classify_node_and_tail(node, tail).is_some(),
                None => true,
            };
        }
        if root == "theme_options" || root == "menus" {
            return true;
        }
        if let Some(UrlSourceKind::TrustedLink) = scopes.lookup(root) {
            return classify_trusted_link_tail(&attrs).is_some();
        }
        if let Some((node, tail)) = resolve_field_node_and_tail(root, &attrs, scopes, policy) {
            return classify_node_and_tail(node, tail).is_some();
        }
        if scopes.lookup(root).is_some() {
            return true;
        }
    }
    if classify_block_sensitive_expr(expr, scopes, policy).is_some() {
        return true;
    }
    match expr {
        Expr::Var(_) | Expr::Const(_) => false,
        Expr::Slice(s) => {
            block_expr_touches_sensitive(&s.expr, scopes, policy)
                || s.start
                    .as_ref()
                    .is_some_and(|e| block_expr_touches_sensitive(e, scopes, policy))
                || s.stop
                    .as_ref()
                    .is_some_and(|e| block_expr_touches_sensitive(e, scopes, policy))
                || s.step
                    .as_ref()
                    .is_some_and(|e| block_expr_touches_sensitive(e, scopes, policy))
        }
        Expr::UnaryOp(u) => block_expr_touches_sensitive(&u.expr, scopes, policy),
        Expr::BinOp(b) => {
            block_expr_touches_sensitive(&b.left, scopes, policy)
                || block_expr_touches_sensitive(&b.right, scopes, policy)
        }
        Expr::Compare(c) => {
            block_expr_touches_sensitive(&c.expr, scopes, policy)
                || c.ops
                    .iter()
                    .any(|op| block_expr_touches_sensitive(&op.expr, scopes, policy))
        }
        Expr::IfExpr(i) => {
            block_expr_touches_sensitive(&i.test_expr, scopes, policy)
                || block_expr_touches_sensitive(&i.true_expr, scopes, policy)
                || i.false_expr
                    .as_ref()
                    .is_some_and(|e| block_expr_touches_sensitive(e, scopes, policy))
        }
        Expr::GetAttr(g) => block_expr_touches_sensitive(&g.expr, scopes, policy),
        Expr::GetItem(g) => {
            block_expr_touches_sensitive(&g.expr, scopes, policy)
                || block_expr_touches_sensitive(&g.subscript_expr, scopes, policy)
        }
        Expr::List(l) => l
            .items
            .iter()
            .any(|e| block_expr_touches_sensitive(e, scopes, policy)),
        Expr::Tuple(t) => t
            .items
            .iter()
            .any(|e| block_expr_touches_sensitive(e, scopes, policy)),
        Expr::Map(m) => {
            m.keys
                .iter()
                .any(|e| block_expr_touches_sensitive(e, scopes, policy))
                || m.values
                    .iter()
                    .any(|e| block_expr_touches_sensitive(e, scopes, policy))
        }
        Expr::Filter(f) => {
            f.expr
                .as_ref()
                .is_some_and(|e| block_expr_touches_sensitive(e, scopes, policy))
                || f.args
                    .iter()
                    .any(|arg| block_call_arg_touches_sensitive(arg, scopes, policy))
        }
        Expr::Test(t) => {
            block_expr_touches_sensitive(&t.expr, scopes, policy)
                || t.args
                    .iter()
                    .any(|arg| block_call_arg_touches_sensitive(arg, scopes, policy))
        }
        Expr::Call(_) => true,
    }
}

pub(super) fn block_call_arg_touches_sensitive(
    arg: &minijinja::machinery::ast::CallArg<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> bool {
    match arg {
        minijinja::machinery::ast::CallArg::Pos(e)
        | minijinja::machinery::ast::CallArg::Kwarg(_, e)
        | minijinja::machinery::ast::CallArg::PosSplat(e)
        | minijinja::machinery::ast::CallArg::KwargSplat(e) => {
            block_expr_touches_sensitive(e, scopes, policy)
        }
    }
}

pub(super) fn check_block_expr_restrictions(
    expr: &Expr<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> Result<(), &'static str> {
    validate_expr(expr)?;
    match expr {
        Expr::Var(v) => {
            if v.id == "theme_options" {
                return Err("Template block không được truy cập theme_options.");
            }
            Ok(())
        }
        Expr::Const(_) => Ok(()),
        Expr::GetItem(g) => {
            let is_menus_lookup = matches!(&g.expr, Expr::Var(v) if v.id == "menus");
            if !is_menus_lookup {
                return Err("C-SEC cấm truy cập GetItem ([...]) trong template block.");
            }
            check_block_expr_restrictions(&g.subscript_expr, scopes, policy)?;
            if block_expr_touches_sensitive(&g.subscript_expr, scopes, policy) {
                return Err("Khóa tra cứu menus[...] phải là biểu thức scalar an toàn.");
            }
            Ok(())
        }
        Expr::Filter(f) => {
            if f.expr
                .as_ref()
                .is_some_and(|e| block_expr_touches_sensitive(e, scopes, policy))
                || f.args
                    .iter()
                    .any(|arg| block_call_arg_touches_sensitive(arg, scopes, policy))
            {
                return Err("C-SEC cấm áp dụng filter lên biến richtext/url/media/repeater.");
            }
            if let Some(e) = &f.expr {
                check_block_expr_restrictions(e, scopes, policy)?;
            }
            for arg in &f.args {
                check_block_call_arg_restrictions(arg, scopes, policy)?;
            }
            Ok(())
        }
        Expr::Slice(s) => {
            if block_expr_touches_sensitive(&s.expr, scopes, policy) {
                return Err("C-SEC cấm cắt lát (slice) trên biến nhạy cảm.");
            }
            check_block_expr_restrictions(&s.expr, scopes, policy)?;
            if let Some(start) = &s.start {
                check_block_expr_restrictions(start, scopes, policy)?;
            }
            if let Some(stop) = &s.stop {
                check_block_expr_restrictions(stop, scopes, policy)?;
            }
            if let Some(step) = &s.step {
                check_block_expr_restrictions(step, scopes, policy)?;
            }
            Ok(())
        }
        Expr::UnaryOp(u) => check_block_expr_restrictions(&u.expr, scopes, policy),
        Expr::BinOp(b) => {
            check_block_expr_restrictions(&b.left, scopes, policy)?;
            check_block_expr_restrictions(&b.right, scopes, policy)
        }
        Expr::Compare(c) => {
            check_block_expr_restrictions(&c.expr, scopes, policy)?;
            for op in &c.ops {
                check_block_expr_restrictions(&op.expr, scopes, policy)?;
            }
            Ok(())
        }
        Expr::IfExpr(i) => {
            check_block_expr_restrictions(&i.test_expr, scopes, policy)?;
            check_block_expr_restrictions(&i.true_expr, scopes, policy)?;
            if let Some(f) = &i.false_expr {
                check_block_expr_restrictions(f, scopes, policy)?;
            }
            Ok(())
        }
        Expr::GetAttr(g) => check_block_expr_restrictions(&g.expr, scopes, policy),
        Expr::List(l) => {
            for item in &l.items {
                check_block_expr_restrictions(item, scopes, policy)?;
            }
            Ok(())
        }
        Expr::Tuple(t) => {
            for item in &t.items {
                check_block_expr_restrictions(item, scopes, policy)?;
            }
            Ok(())
        }
        Expr::Map(m) => {
            for (k, v) in m.keys.iter().zip(m.values.iter()) {
                check_block_expr_restrictions(k, scopes, policy)?;
                check_block_expr_restrictions(v, scopes, policy)?;
            }
            Ok(())
        }
        Expr::Test(t) => {
            check_block_expr_restrictions(&t.expr, scopes, policy)?;
            for arg in &t.args {
                check_block_call_arg_restrictions(arg, scopes, policy)?;
            }
            Ok(())
        }
        Expr::Call(_) => Err("C-SEC cấm lời gọi hàm Call trong template block."),
    }
}

pub(super) fn check_block_call_arg_restrictions(
    arg: &minijinja::machinery::ast::CallArg<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> Result<(), &'static str> {
    match arg {
        minijinja::machinery::ast::CallArg::Pos(e)
        | minijinja::machinery::ast::CallArg::Kwarg(_, e)
        | minijinja::machinery::ast::CallArg::PosSplat(e)
        | minijinja::machinery::ast::CallArg::KwargSplat(e) => {
            check_block_expr_restrictions(e, scopes, policy)
        }
    }
}

pub(super) fn is_valid_block_url_expr(
    expr: &Expr<'_>,
    slot: UrlAttrSlot,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> bool {
    match classify_block_sensitive_expr(expr, scopes, policy) {
        Some(BlockSensitiveKind::HrefUrl) => {
            matches!(slot, UrlAttrSlot::AnchorHref | UrlAttrSlot::Cite)
        }
        Some(BlockSensitiveKind::MediaUrl) => {
            matches!(slot, UrlAttrSlot::AnchorHref | UrlAttrSlot::SrcAndHref)
        }
        _ => false,
    }
}

pub(super) fn resolve_block_collection_source(
    expr: &Expr<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> Option<UrlSourceKind> {
    if let Some((root, attrs)) = extract_attr_chain(expr)
        && let Some((node, &[])) = resolve_field_node_and_tail(root, &attrs, scopes, policy)
    {
        return match node {
            FieldTemplateNode::Repeater(fields) => Some(UrlSourceKind::FieldNode(
                FieldTemplateNode::Group(fields.clone()),
            )),
            FieldTemplateNode::Gallery => Some(UrlSourceKind::FieldNode(FieldTemplateNode::Media)),
            FieldTemplateNode::EntryRef { .. } => {
                Some(UrlSourceKind::FieldNode(FieldTemplateNode::EntryRef {
                    multiple: false,
                }))
            }
            FieldTemplateNode::TermRef { .. } => {
                Some(UrlSourceKind::FieldNode(FieldTemplateNode::TermRef {
                    multiple: false,
                }))
            }
            _ => None,
        };
    }
    if let Some((_sub, attrs)) = resolve_menu_get_item_subscript(expr)
        && attrs.as_slice() == ["items"]
    {
        return Some(UrlSourceKind::TrustedLink);
    }
    if let Some(attrs) = resolve_menu_dot_chain(expr)
        && matches!(attrs.as_slice(), [_slot, "items"])
    {
        return Some(UrlSourceKind::TrustedLink);
    }
    if let Expr::GetAttr(g) = expr
        && g.name == "children"
        && let Expr::Var(v) = &g.expr
        && scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedLink)
    {
        return Some(UrlSourceKind::TrustedLink);
    }
    None
}

pub(super) fn walk_block_ast_sequence(
    children: &[Stmt<'_>],
    mut states: BTreeSet<ThemeHtmlState>,
    mut scopes: UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> Result<(BTreeSet<ThemeHtmlState>, UrlScopeStack), &'static str> {
    for child in children {
        let (next_states, next_scopes) = walk_block_ast_stmt(child, states, scopes, policy)?;
        states = next_states;
        scopes = next_scopes;
        if states.len() > MAX_THEME_WALKER_STATES {
            return Err("Quá nhiều trạng thái HTML trong template block.");
        }
    }
    Ok((states, scopes))
}

pub(super) fn walk_block_ast_stmt(
    stmt: &Stmt<'_>,
    states: BTreeSet<ThemeHtmlState>,
    mut scopes: UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> Result<(BTreeSet<ThemeHtmlState>, UrlScopeStack), &'static str> {
    match stmt {
        Stmt::Template(t) => walk_block_ast_sequence(&t.children, states, scopes, policy),
        Stmt::EmitRaw(raw) => {
            let mut next_states = BTreeSet::new();
            for initial in states {
                let mut cur = initial;
                for ch in raw.raw.chars() {
                    let depth = cur.trusted_depth();
                    cur = step_theme_raw_char(cur, ch, BLOCK_HTML_STEP_OPTIONS, depth)?;
                }
                next_states.insert(cur);
            }
            if next_states.len() > MAX_THEME_WALKER_STATES {
                return Err("Quá nhiều trạng thái HTML trong template block.");
            }
            Ok((next_states, scopes))
        }
        Stmt::EmitExpr(e) => {
            reject_style_control_flow(&states)?;
            if let Some((icon_arg, _)) = parse_od_icon_call_ext(&e.expr, true)? {
                if states.iter().any(|s| s.inner() != &ThemeHtmlState::Body) {
                    return Err("od_icon() chỉ được xuất trong thân HTML của block.");
                }
                if let OdIconNameArg::FieldNamePath(root, attrs) = icon_arg {
                    let resolved = resolve_field_node_and_tail(
                        root,
                        &attrs[..attrs.len() - 1],
                        &scopes,
                        policy,
                    );
                    if !matches!(resolved, Some((FieldTemplateNode::Icon, &[]))) {
                        return Err("od_icon() động phải tham chiếu trường kiểu icon.");
                    }
                }
                return Ok((states, scopes));
            }
            if let Some((_, arg_expr)) = parse_od_hook_call(&e.expr)? {
                if states.iter().any(|s| s.inner() != &ThemeHtmlState::Body) {
                    return Err("od_hook() chỉ được xuất trong thân HTML của block.");
                }
                if let Some(arg) = arg_expr {
                    check_block_expr_restrictions(arg, &scopes, policy)?;
                }
                return Ok((states, scopes));
            }
            check_block_expr_restrictions(&e.expr, &scopes, policy)?;
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
                    "C-SEC cấm nội suy Jinja ở comment, <style>/<script> hoặc biên thẻ HTML.",
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
                match classify_block_sensitive_expr(&e.expr, &scopes, policy) {
                    Some(BlockSensitiveKind::Richtext) => {
                        if states.iter().any(|s| s.inner() != &ThemeHtmlState::Body) {
                            return Err(
                                "Biến richtext chỉ được phép đặt trong phần thân HTML (ThemeHtmlState::Body).",
                            );
                        }
                    }
                    Some(BlockSensitiveKind::HrefUrl | BlockSensitiveKind::MediaUrl) => {
                        return Err(
                            "Biến url / media url chỉ được xuất bên trong thuộc tính URL được trích dẫn.",
                        );
                    }
                    Some(BlockSensitiveKind::ObjectOrNonScalar) => {
                        return Err(
                            "Không được xuất trực tiếp đối tượng media/link/icon/group/repeater/options/menus.",
                        );
                    }
                    None => {
                        if block_expr_touches_sensitive(&e.expr, &scopes, policy) {
                            return Err(
                                "Biến richtext/url/media không được phép ghép chuỗi hoặc biến đổi trong biểu thức xuất.",
                            );
                        }
                    }
                }
                return Ok((states, scopes));
            }

            if matches!(
                classify_block_sensitive_expr(&e.expr, &scopes, policy),
                Some(BlockSensitiveKind::Richtext | BlockSensitiveKind::ObjectOrNonScalar)
            ) {
                return Err(
                    "C-SEC cấm xuất biến richtext hoặc đối tượng không phải chuỗi bên trong thuộc tính thẻ HTML.",
                );
            }

            let mut expected_url_slot: Option<Option<UrlAttrSlot>> = None;
            let mut updated_states = BTreeSet::new();
            for state in states {
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
                    if !is_valid_block_url_expr(&e.expr, slot, &scopes, policy) {
                        return Err(
                            "URL động trong thuộc tính không nằm trong whitelist typed URL của block.",
                        );
                    }
                    attr_expr_count = 1;
                    if slot == UrlAttrSlot::LinkCss {
                        link_has_valid_href = true;
                    }
                } else {
                    if block_expr_touches_sensitive(&e.expr, &scopes, policy) {
                        return Err(
                            "Biến richtext/url/media không được phép xuất bên trong thuộc tính HTML thường.",
                        );
                    }
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
                    true,
                    svg_depth,
                ));
            }
            Ok((updated_states, scopes))
        }
        Stmt::IfCond(cond) => {
            reject_style_control_flow(&states)?;
            check_block_expr_restrictions(&cond.expr, &scopes, policy)?;
            let (mut true_states, true_scopes) =
                walk_block_ast_sequence(&cond.true_body, states.clone(), scopes.clone(), policy)?;
            let (false_states, false_scopes) =
                walk_block_ast_sequence(&cond.false_body, states, scopes, policy)?;
            true_states.extend(false_states);
            if true_states.len() > MAX_THEME_WALKER_STATES {
                return Err("Quá nhiều trạng thái HTML tại nhánh điều kiện block.");
            }
            Ok((
                true_states,
                UrlScopeStack::intersect(&true_scopes, &false_scopes),
            ))
        }
        Stmt::ForLoop(loop_stmt) => {
            reject_style_control_flow(&states)?;
            validate_expr(&loop_stmt.target)?;
            check_block_expr_restrictions(&loop_stmt.iter, &scopes, policy)?;
            if let Some(cond) = &loop_stmt.filter_expr {
                check_block_expr_restrictions(cond, &scopes, policy)?;
            }

            let target_idents = extract_bound_idents(&loop_stmt.target)?;
            for id in &target_idents {
                if is_protected_block_binding(id, &scopes) {
                    return Err("Biến vòng lặp for không được che khuất tên biến nhạy cảm.");
                }
            }

            let iter_kind = resolve_block_collection_source(&loop_stmt.iter, &scopes, policy);
            if block_expr_touches_sensitive(&loop_stmt.iter, &scopes, policy) {
                let valid_iter = matches!(&loop_stmt.target, Expr::Var(_)) && iter_kind.is_some();
                if !valid_iter {
                    return Err(
                        "Không được lặp trực tiếp trên biến richtext/url/media hoặc sai cú pháp vòng lặp block.",
                    );
                }
            }

            let (else_states, else_scopes) = walk_block_ast_sequence(
                &loop_stmt.else_body,
                states.clone(),
                scopes.clone(),
                policy,
            )?;

            let loop_targets: BTreeMap<String, Option<UrlSourceKind>> =
                if let Expr::Var(v) = &loop_stmt.target {
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
                let (after_states, after_scopes) = walk_block_ast_sequence(
                    &loop_stmt.body,
                    loop_states.clone(),
                    loop_scopes.clone(),
                    policy,
                )?;
                let mut next_states = loop_states.clone();
                next_states.extend(after_states);
                if next_states.len() > MAX_THEME_WALKER_STATES {
                    return Err("Quá nhiều trạng thái HTML trong vòng lặp block.");
                }
                let next_scopes = UrlScopeStack::intersect(&loop_scopes, &after_scopes);
                let mut next_header_scopes = next_scopes.clone();
                for (k, v) in &loop_targets {
                    next_header_scopes.insert_top(k.clone(), v.clone());
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
                return Err("Không thể xác định vị trí HTML sau vòng lặp trong template block.");
            }

            final_loop_scopes.pop_frame();
            let mut combined_states = final_loop_states;
            combined_states.extend(else_states);
            if combined_states.len() > MAX_THEME_WALKER_STATES {
                return Err("Quá nhiều trạng thái HTML sau vòng lặp block.");
            }
            let combined_scopes = UrlScopeStack::intersect(&final_loop_scopes, &else_scopes);
            Ok((combined_states, combined_scopes))
        }
        Stmt::Set(s) => {
            reject_style_control_flow(&states)?;
            validate_expr(&s.target)?;
            check_block_expr_restrictions(&s.expr, &scopes, policy)?;
            if block_expr_touches_sensitive(&s.expr, &scopes, policy) {
                return Err(
                    "C-SEC cấm gán (set) hoặc tạo bí danh cho biến richtext/url/media/repeater.",
                );
            }
            for ident in extract_bound_idents(&s.target)? {
                if is_protected_block_binding(&ident, &scopes) {
                    return Err("C-SEC cấm gán (set) hoặc ghi đè biến nhạy cảm của block.");
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
            if target_idents
                .iter()
                .any(|id| is_protected_block_binding(id, &scopes))
            {
                return Err("C-SEC cấm ghi đè biến nhạy cảm qua khối set.");
            }
            for child in &sb.body {
                if block_stmt_touches_sensitive(child, &scopes, policy) {
                    return Err("C-SEC cấm bọc biến richtext/url/media trong khối set.");
                }
            }
            let (sb_states, sb_scopes) = walk_block_ast_sequence(
                &sb.body,
                BTreeSet::from([ThemeHtmlState::with_trust(ThemeHtmlState::Body, true, 0)]),
                scopes.clone(),
                policy,
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
        Stmt::WithBlock(w) => {
            reject_style_control_flow(&states)?;
            let mut frame = BTreeMap::new();
            for (k, v) in &w.assignments {
                validate_expr(k)?;
                check_block_expr_restrictions(v, &scopes, policy)?;
                if block_expr_touches_sensitive(v, &scopes, policy) {
                    return Err(
                        "C-SEC cấm tạo bí danh cho biến richtext/url/media/repeater qua khối with.",
                    );
                }
                for ident in extract_bound_idents(k)? {
                    if is_protected_block_binding(&ident, &scopes) {
                        return Err("C-SEC cấm ghi đè biến nhạy cảm qua khối with.");
                    }
                    frame.insert(ident, None);
                }
            }
            scopes.push_frame(frame);
            let (out_states, mut out_scopes) =
                walk_block_ast_sequence(&w.body, states, scopes, policy)?;
            out_scopes.pop_frame();
            Ok((out_states, out_scopes))
        }
        _ => Err("Template block sử dụng câu lệnh MiniJinja bị C-SEC cấm."),
    }
}

pub(super) fn block_stmt_touches_sensitive(
    stmt: &Stmt<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> bool {
    match stmt {
        Stmt::Template(t) => t
            .children
            .iter()
            .any(|c| block_stmt_touches_sensitive(c, scopes, policy)),
        Stmt::EmitRaw(_) => false,
        Stmt::EmitExpr(e) => block_expr_touches_sensitive(&e.expr, scopes, policy),
        Stmt::IfCond(i) => {
            block_expr_touches_sensitive(&i.expr, scopes, policy)
                || i.true_body
                    .iter()
                    .any(|c| block_stmt_touches_sensitive(c, scopes, policy))
                || i.false_body
                    .iter()
                    .any(|c| block_stmt_touches_sensitive(c, scopes, policy))
        }
        Stmt::ForLoop(f) => {
            block_expr_touches_sensitive(&f.iter, scopes, policy)
                || f.body
                    .iter()
                    .any(|c| block_stmt_touches_sensitive(c, scopes, policy))
                || f.else_body
                    .iter()
                    .any(|c| block_stmt_touches_sensitive(c, scopes, policy))
        }
        Stmt::Set(s) => {
            block_expr_touches_sensitive(&s.target, scopes, policy)
                || block_expr_touches_sensitive(&s.expr, scopes, policy)
        }
        Stmt::SetBlock(sb) => {
            block_expr_touches_sensitive(&sb.target, scopes, policy)
                || sb
                    .body
                    .iter()
                    .any(|c| block_stmt_touches_sensitive(c, scopes, policy))
        }
        Stmt::WithBlock(w) => {
            w.assignments.iter().any(|(k, v)| {
                block_expr_touches_sensitive(k, scopes, policy)
                    || block_expr_touches_sensitive(v, scopes, policy)
            }) || w
                .body
                .iter()
                .any(|c| block_stmt_touches_sensitive(c, scopes, policy))
        }
        _ => true,
    }
}

/// Validates a declarative `block.json` v1 `render.html` template using the unified
/// `ThemeHtmlState` + MiniJinja AST + `UrlScopeStack` checker.
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
    let ast = parse(source, block_id, SyntaxConfig::default())
        .map_err(|_| "Cú pháp MiniJinja của template block không hợp lệ.")?;
    validate_stmt_with_icons_ext(&ast, true, true, false)?;

    let (final_states, _) = walk_block_ast_stmt(
        &ast,
        BTreeSet::from([ThemeHtmlState::with_trust(ThemeHtmlState::Body, true, 0)]),
        ctx.root_scopes,
        &ctx.policy,
    )?;
    if final_states
        .iter()
        .any(|s| s.inner() != &ThemeHtmlState::Body || s.trusted_depth() != 0)
    {
        return Err(
            "Template block phải kết thúc ở trạng thái thân HTML (đóng đủ thẻ/comment/svg).",
        );
    }

    let mut env = restricted_environment();
    env.add_template(block_id, source)
        .map_err(|_| "Không thể biên dịch template block trong môi trường C-SEC.")?;
    Ok(())
}

pub(super) fn generate_richtext_nonce() -> Result<String, &'static str> {
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

pub(crate) fn extract_spacing_breakpoint_values(
    val: Option<&serde_json::Value>,
    fallback: Option<opendash_core::field_schema::SpacingValue>,
    responsive: bool,
) -> (
    Option<opendash_core::field_schema::SpacingValue>,
    Option<opendash_core::field_schema::SpacingValue>,
    Option<opendash_core::field_schema::SpacingValue>,
) {
    use opendash_core::field_schema::SpacingValue;
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
    fallback: Option<opendash_core::field_schema::SpacingValue>,
    responsive: bool,
    unit: opendash_core::field_schema::CssUnit,
    property: opendash_core::field_schema::SpacingProperty,
) -> Option<Vec<String>> {
    use opendash_core::{
        builder::{BoxSpacing, compile_box_spacing_tailwind},
        field_schema::{CssUnit, SpacingProperty, SpacingValue},
    };
    if unit != CssUnit::Px {
        return None;
    }
    let allow_negative = property == SpacingProperty::Margin;
    let min_val = if allow_negative {
        f64::from(i16::MIN)
    } else {
        0.0
    };
    let to_box = |sv: SpacingValue| -> Option<BoxSpacing> {
        let conv = |opt: Option<f64>| -> Option<Option<i16>> {
            match opt {
                None => Some(None),
                Some(n)
                    if n.is_finite()
                        && n.fract() == 0.0
                        && n >= min_val
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
    let prefix = match property {
        SpacingProperty::Padding => "p",
        SpacingProperty::Margin => "m",
    };
    compile_box_spacing_tailwind(prefix, d_box, t_box, m_box, allow_negative)
}

pub struct BlockRenderInputs<'a> {
    pub hydrated_props: &'a BTreeMap<String, serde_json::Value>,
    pub local_richtext_nonces: Option<RichTextNonceTable>,
    pub merged_options: Option<&'a BTreeMap<String, serde_json::Value>>,
    pub site_options: Option<&'a BTreeMap<String, serde_json::Value>>,
    pub menus: Option<&'a BTreeMap<String, serde_json::Value>>,
    pub is_preview: bool,
    pub fmt_cfg: &'a RenderFormattingConfig,
}

/// Renders a validated `block.json` v1 template from pre-hydrated field values under the
/// shared request `RenderBudget` (`<= 2_000` fuel per node, `<= 10_000` total fuel,
/// `<= 512 KiB` total HTML, `<= 128` RichText blocks per request).
pub fn render_block_csec_with_inputs(
    block_id: &str,
    template_source: &str,
    inputs: BlockRenderInputs<'_>,
    budget: &mut RenderBudget,
) -> Result<String, &'static str> {
    if budget.fuel_remaining == 0 {
        return Err("Đã hết ngân sách fuel render trang.");
    }
    let local_rt_count = inputs
        .local_richtext_nonces
        .as_ref()
        .map_or(0, RichTextNonceTable::total_count);
    if budget
        .richtext_nonces
        .total_count()
        .saturating_add(local_rt_count)
        > MAX_RICHTEXT_NONCES_PER_REQUEST
    {
        return Err("Số lượng khối RichText vượt giới hạn 128/request.");
    }
    let node_fuel = budget.fuel_remaining.min(MAX_BLOCK_FUEL_PER_NODE);

    let mut ctx_map: BTreeMap<String, Value> = BTreeMap::new();
    for (k, v) in inputs.hydrated_props {
        ctx_map.insert(k.clone(), json_to_safe_value(v));
    }

    let mut site_opts = opendash_core::field_schema::global_site_option_defaults();
    if let Some(extra) = inputs.site_options {
        for (k, v) in extra {
            site_opts.insert(k.clone(), v.clone());
        }
    }
    let mut merged_opts = site_opts.clone();
    if let Some(extra) = inputs.merged_options {
        for (k, v) in extra {
            merged_opts.insert(k.clone(), v.clone());
        }
    }

    let options_value = Value::from_pairs(
        merged_opts
            .into_iter()
            .map(|(k, v)| (k, json_to_safe_value(&v))),
    );
    let site_options_value = Value::from_pairs(
        site_opts
            .into_iter()
            .map(|(k, v)| (k, json_to_safe_value(&v))),
    );
    let menus_value = match inputs.menus {
        Some(m) => Value::from_pairs(m.iter().map(|(k, v)| (k.clone(), json_to_safe_value(v)))),
        None => Value::from_pairs(std::iter::empty::<(String, Value)>()),
    };

    ctx_map.insert("options".to_owned(), options_value);
    ctx_map.insert("site_options".to_owned(), site_options_value);
    ctx_map.insert("menus".to_owned(), menus_value);
    ctx_map.insert("od_is_preview".to_owned(), Value::from(inputs.is_preview));

    let mut env = restricted_environment_with_config(inputs.fmt_cfg);
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

    let out = match inputs.local_richtext_nonces {
        Some(rt) if !rt.is_empty() => rt.substitute_into_document(shell.to_owned())?,
        _ => shell.to_owned(),
    };

    budget.charge_html_bytes(out.len())?;
    budget.richtext_nonces.reserve_count(local_rt_count)?;
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
