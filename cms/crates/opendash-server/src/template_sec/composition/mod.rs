use super::*;

pub const ICON_NONCE_PREFIX: &str = "OD_ICON_";

pub const TEMPLATE_RECURSION_LIMIT: usize = 16;

pub(crate) const MAX_COMPOSITION_PART_DEPTH: usize = 3;

pub(crate) const MAX_COMPOSITION_EXPANSIONS: usize = 32;

pub(crate) const MAX_THEME_CACHE_ENTRIES: usize = 256;

// A compiler-only marker. It is never passed to MiniJinja as a callable or
// trusted HTML variable. A fresh 128-bit random value replaces it per render.
pub(crate) const CONTENT_SLOT_TOKEN: &str = "OD_CONTENT_SLOT_COMPILE_MARKER";

// Reserved only by the preview compiler, never a template variable or helper.
pub(crate) const PREVIEW_PART_NONCE_PREFIX: &str = "OD_PREVIEW_PART_";

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
pub(crate) enum CompositionCall {
    Part(String),
    Content,
}

#[derive(Debug, Clone)]
pub(crate) struct CompositionSite {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) call: CompositionCall,
}

mod allowlists;
pub use allowlists::*;
mod composition_calls;
pub use composition_calls::*;
mod html_scan_state;
pub use html_scan_state::*;
mod url_and_css;
pub use url_and_css::*;
mod ast_validation;
pub use ast_validation::*;
mod html_attributes;
pub use html_attributes::*;
mod fragment_validation;
pub use fragment_validation::*;
