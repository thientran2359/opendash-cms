use super::*;

// Reserved only by the block/theme RichText post-render nonce replacer.
pub const BLOCK_RICHTEXT_NONCE_PREFIX: &str = "OD_RICHTEXT_";

// Reserved only for Container `{{ children_html }}` reverse-order slot assembly.
pub const BLOCK_CHILDREN_NONCE_PREFIX: &str = "__OD_CHILDREN_NONCE_";

pub const MAX_BUILDER_LISTS_PER_REQUEST: usize = 8;

pub fn generate_children_nonce() -> Result<String, &'static str> {
    let mut random = [0_u8; 16];
    OsRng
        .try_fill_bytes(&mut random)
        .map_err(|_| "Không tạo được nonce children.")?;
    let mut nonce = String::with_capacity(BLOCK_CHILDREN_NONCE_PREFIX.len() + 32);
    nonce.push_str(BLOCK_CHILDREN_NONCE_PREFIX);
    for byte in random {
        use std::fmt::Write;
        write!(&mut nonce, "{byte:02x}").map_err(|_| "Không tạo được nonce children.")?;
    }
    Ok(nonce)
}

pub(super) fn advance_body_scan_state(
    before: &str,
    state: &mut HtmlScanState,
    in_diagnostic: &mut bool,
    diagnostic_pre: &mut bool,
) {
    let bytes = before.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        // The normalizer's diagnostic markup is server-owned. A nonce
        // printed as escaped diagnostic text must never become active HTML/SVG.
        let suffix = &bytes[index..];
        if *state == HtmlScanState::Body && suffix.starts_with(b"<main data-doc-diagnostic=") {
            *in_diagnostic = true;
        } else if *state == HtmlScanState::Body && suffix.starts_with(b"</main>") {
            *in_diagnostic = false;
            *diagnostic_pre = false;
        } else if *state == HtmlScanState::Body && *in_diagnostic && suffix.starts_with(b"<pre") {
            *diagnostic_pre = true;
        } else if *state == HtmlScanState::Body && suffix.starts_with(b"</pre>") {
            *diagnostic_pre = false;
        }
        *state = state.clone().consume(byte);
    }
}

pub(super) fn substitute_nonces_into_body(
    entries: &[(String, String)],
    html: String,
    max_repeats: usize,
    max_bytes: usize,
    repeat_err: &'static str,
    context_err: &'static str,
    size_err: &'static str,
) -> Result<String, &'static str> {
    if entries.is_empty() {
        return Ok(html);
    }
    let mut replacements = Vec::new();
    for (nonce, payload) in entries {
        let mut count = 0usize;
        for (offset, _) in html.match_indices(nonce) {
            count += 1;
            if count > max_repeats {
                return Err(repeat_err);
            }
            replacements.push((offset, nonce.as_str(), payload.as_str()));
        }
    }
    if replacements.is_empty() {
        return Ok(html);
    }
    replacements.sort_unstable_by_key(|(offset, _, _)| *offset);
    let mut out = String::with_capacity(html.len());
    let mut state = HtmlScanState::Body;
    let mut cursor = 0usize;
    let mut in_diagnostic = false;
    let mut diagnostic_pre = false;
    for (offset, nonce, payload) in replacements {
        let before = html
            .get(cursor..offset)
            .ok_or("Vị trí nonce nội bộ không hợp lệ.")?;
        advance_body_scan_state(before, &mut state, &mut in_diagnostic, &mut diagnostic_pre);
        if state != HtmlScanState::Body || diagnostic_pre || in_diagnostic {
            return Err(context_err);
        }
        out.push_str(before);
        out.push_str(payload);
        cursor = offset + nonce.len();
        if out.len() > max_bytes {
            return Err(size_err);
        }
    }
    out.push_str(&html[cursor..]);
    if out.len() > max_bytes {
        return Err(size_err);
    }
    Ok(out)
}

#[derive(Debug, Clone, Default)]
pub struct RichTextNonceTable {
    pub(super) entries: Vec<(String, String)>,
    pub(super) reserved_count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct IconNonceTable {
    pub(super) entries: Vec<(String, String)>,
    pub(super) meta: std::collections::BTreeMap<String, (String, Option<String>)>,
    pub(super) unique_names: std::collections::BTreeSet<String>,
}

pub const HOOK_NONCE_PREFIX: &str = "OD_HOOK_";

#[derive(Debug, Clone)]
pub struct HookSlotInvocation {
    pub nonce: String,
    pub slot: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Default)]
pub struct HookNonceTable {
    pub(super) entries: Vec<HookSlotInvocation>,
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

pub(super) fn install_hook_function(
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

#[derive(Debug, Clone)]
pub struct RenderBudget {
    pub fuel_remaining: u64,
    pub html_bytes_used: usize,
    pub builder_lists_used: usize,
    pub collection_queries_used: usize,
    pub loop_items_rendered: usize,
    pub menu_loads_used: usize,
    pub locale: String,
    pub icon_nonces: IconNonceTable,
    pub image_nonces: ImageNonceTable,
    pub richtext_nonces: RichTextNonceTable,
    pub hook_nonces: HookNonceTable,
}

/// One budget per private preview. Storage owns graph discovery and records
/// edge occurrences and the active canonical-key stack before rendering.
/// Every contiguous C-SEC segment debits the same remaining fuel.
#[derive(Debug)]
pub(crate) struct PreviewCompositionBudget {
    pub fuel_remaining: u64,
    pub html_bytes_used: usize,
    pub builder_lists_used: usize,
    pub collection_queries_used: usize,
    pub loop_items_rendered: usize,
    pub menu_loads_used: usize,
    pub locale: String,
    pub expansions: usize,
    pub stack: Vec<String>,
    pub icon_nonces: IconNonceTable,
    pub image_nonces: ImageNonceTable,
    pub richtext_nonces: RichTextNonceTable,
    pub hook_nonces: HookNonceTable,
}

impl PreviewCompositionBudget {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_render_budget(rb: RenderBudget) -> Self {
        Self {
            fuel_remaining: rb.fuel_remaining,
            html_bytes_used: rb.html_bytes_used,
            builder_lists_used: rb.builder_lists_used,
            collection_queries_used: rb.collection_queries_used,
            loop_items_rendered: rb.loop_items_rendered,
            menu_loads_used: rb.menu_loads_used,
            locale: rb.locale,
            expansions: 0,
            stack: Vec::new(),
            icon_nonces: rb.icon_nonces,
            image_nonces: rb.image_nonces,
            richtext_nonces: rb.richtext_nonces,
            hook_nonces: rb.hook_nonces,
        }
    }

    pub(crate) fn as_render_budget(&self) -> RenderBudget {
        RenderBudget {
            fuel_remaining: self.fuel_remaining,
            html_bytes_used: self.html_bytes_used,
            builder_lists_used: self.builder_lists_used,
            collection_queries_used: self.collection_queries_used,
            loop_items_rendered: self.loop_items_rendered,
            menu_loads_used: self.menu_loads_used,
            locale: self.locale.clone(),
            icon_nonces: self.icon_nonces.clone(),
            image_nonces: self.image_nonces.clone(),
            richtext_nonces: self.richtext_nonces.clone(),
            hook_nonces: self.hook_nonces.clone(),
        }
    }

    pub(crate) fn sync_from_render_budget(&mut self, rb: &RenderBudget) {
        self.fuel_remaining = rb.fuel_remaining;
        self.html_bytes_used = rb.html_bytes_used;
        self.builder_lists_used = rb.builder_lists_used;
        self.collection_queries_used = rb.collection_queries_used;
        self.loop_items_rendered = rb.loop_items_rendered;
        self.menu_loads_used = rb.menu_loads_used;
        self.locale = rb.locale.clone();
        self.icon_nonces = rb.icon_nonces.clone();
        self.image_nonces = rb.image_nonces.clone();
        self.richtext_nonces = rb.richtext_nonces.clone();
        self.hook_nonces = rb.hook_nonces.clone();
    }
}

pub(super) fn insert_preview_fragments(
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

/// Bounded lexical HTML states for block templates and preview nonce replacement.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum HtmlScanState {
    Body,
    Tag {
        quote: Option<u8>,
        prefix: String,
        trailing_slash: bool,
    },
    Comment(CommentScanSubState),
    Style {
        closing_prefix: usize,
    },
    Script {
        closing_prefix: usize,
    },
    Title {
        closing_prefix: usize,
    },
}
