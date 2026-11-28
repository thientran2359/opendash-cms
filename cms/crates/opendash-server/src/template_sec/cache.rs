use super::*;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ThemeCompositionCacheKey {
    pub revision_digest: String,
    pub target_key: String,
    pub content_kind: u8,
    pub entry_fields_key: String,
}

#[derive(Debug, Default)]
pub struct ThemeCompositionCache {
    pub(super) inner: RwLock<BTreeMap<ThemeCompositionCacheKey, Arc<CompiledCsecComposition>>>,
}

/// Validated Theme V1 template, flattened into a single MiniJinja program.
/// This ensures every part shares the same context, fuel and output budget.
#[derive(Debug, Clone)]
pub struct CompiledCsecComposition {
    pub(super) source: String,
    pub(super) name: String,
    pub(super) content_count: usize,
    pub(super) part_expansions: usize,
    pub max_part_depth: usize,
    pub uses_related_entries: bool,
    pub uses_recent_posts: bool,
}
