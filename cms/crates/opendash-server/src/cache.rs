//! Process-wide cache invalidation epoch (`SiteCacheEpoch::global()`) and
//! bounded public LRU fragment cache (`ExtensionRenderCache`) for Spec B
//! Render Hooks (`od_hook`, `filter:render/head`, `filter:render/footer`).

use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use opendash_core::{hooks::ExtensionOwnerKind, plugin::RenderCachePolicy};
use sha2::{Digest, Sha256};

use crate::public_context::{QueryScope, parse_query};

pub const EXTENSION_RENDER_CACHE_MAX_ENTRIES: usize = 512;
pub const EXTENSION_RENDER_CACHE_MAX_ENTRY_BYTES: usize = 256 * 1024;

/// Single monotonically increasing data-epoch counter bumped on every
/// SQLite mutation commit across all storage modules and post-commit plugin writes.
#[derive(Debug)]
pub struct SiteCacheEpoch {
    epoch: AtomicU64,
}

impl SiteCacheEpoch {
    pub fn global() -> &'static Arc<Self> {
        static GLOBAL: OnceLock<Arc<SiteCacheEpoch>> = OnceLock::new();
        GLOBAL.get_or_init(|| {
            Arc::new(SiteCacheEpoch {
                epoch: AtomicU64::new(1),
            })
        })
    }

    pub fn current(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    pub fn bump(&self) -> u64 {
        self.epoch.fetch_add(1, Ordering::AcqRel) + 1
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenderCacheKey {
    pub owner_kind: ExtensionOwnerKind,
    pub owner_id: String,
    pub revision_digest_or_generation: String,
    pub hook_or_slot_name: String,
    pub arg_hash: String,
    pub locale: String,
    pub canonical_route_and_query_or_global: String,
    pub origin_host: String,
    pub site_data_epoch: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CachedRenderFragment {
    pub html: String,
    pub authorized_styles: Vec<String>,
    pub authorized_head_scripts: Vec<String>,
}

impl CachedRenderFragment {
    pub fn approx_bytes(&self) -> usize {
        self.html.len()
            + self
                .authorized_styles
                .iter()
                .map(String::len)
                .sum::<usize>()
            + self
                .authorized_head_scripts
                .iter()
                .map(String::len)
                .sum::<usize>()
    }
}

struct LruState {
    entries: HashMap<RenderCacheKey, CachedRenderFragment>,
    order: VecDeque<RenderCacheKey>,
}

/// Bounded LRU cache for normalized public render-hook outputs (before icon
/// nonce substitution). Never stores preview/admin/safe-mode or error outputs.
pub struct ExtensionRenderCache {
    max_entries: usize,
    max_entry_bytes: usize,
    inner: Mutex<LruState>,
}

impl Default for ExtensionRenderCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtensionRenderCache {
    pub fn new() -> Self {
        Self::with_limits(
            EXTENSION_RENDER_CACHE_MAX_ENTRIES,
            EXTENSION_RENDER_CACHE_MAX_ENTRY_BYTES,
        )
    }

    pub fn with_limits(max_entries: usize, max_entry_bytes: usize) -> Self {
        Self {
            max_entries: max_entries.max(1),
            max_entry_bytes,
            inner: Mutex::new(LruState {
                entries: HashMap::new(),
                order: VecDeque::new(),
            }),
        }
    }

    pub fn get(&self, key: &RenderCacheKey) -> Option<CachedRenderFragment> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let val = guard.entries.get(key).cloned()?;
        if let Some(pos) = guard.order.iter().position(|k| k == key) {
            guard.order.remove(pos);
        }
        guard.order.push_back(key.clone());
        Some(val)
    }

    pub fn insert(&self, key: RenderCacheKey, fragment: CachedRenderFragment) {
        if fragment.approx_bytes() > self.max_entry_bytes {
            return;
        }
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if guard.entries.contains_key(&key) {
            if let Some(pos) = guard.order.iter().position(|k| k == &key) {
                guard.order.remove(pos);
            }
        } else {
            while guard.entries.len() >= self.max_entries {
                if let Some(oldest) = guard.order.pop_front() {
                    guard.entries.remove(&oldest);
                } else {
                    break;
                }
            }
        }
        guard.entries.insert(key.clone(), fragment);
        guard.order.push_back(key);
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entries
            .len()
    }
}

/// Canonicalizes recognized public query params via `public_context::parse_query`
/// so arbitrary tracking params (`?utm_source=...`) never fragment cache keys.
pub fn canonicalize_public_query(raw_query: Option<&str>, scope: QueryScope) -> String {
    let Ok(parsed) = parse_query(raw_query, scope) else {
        return String::new();
    };
    let mut parts = Vec::new();
    if parsed.page > 1 {
        parts.push(format!("page={}", parsed.page));
    }
    if let Some(cat) = parsed.filters.category_id {
        parts.push(format!("category_id={cat}"));
    }
    if let Some(tag) = parsed.filters.tag_id {
        parts.push(format!("tag_id={tag}"));
    }
    if let Some(ref tk) = parsed.filters.taxonomy_key {
        parts.push(format!("taxonomy_key={tk}"));
    }
    if let Some(tid) = parsed.filters.term_id {
        parts.push(format!("term_id={tid}"));
    }
    if parsed.filters.descendants {
        parts.push("descendants=true".to_owned());
    }
    parts.join("&")
}

/// Computes the canonical route cache key segment according to `RenderCachePolicy`.
/// All 404 responses collapse onto `"not_found"` to prevent LRU thrashing.
pub fn canonical_route_key(
    policy: RenderCachePolicy,
    route_kind: &str,
    route_path: &str,
    canonical_query: &str,
    is_not_found: bool,
) -> Option<String> {
    match policy {
        RenderCachePolicy::None => None,
        RenderCachePolicy::Global => Some("global".to_owned()),
        RenderCachePolicy::ByRoute => {
            if is_not_found {
                Some("not_found".to_owned())
            } else if route_kind == "search" {
                None
            } else if canonical_query.is_empty() {
                Some(format!("{route_kind}:{route_path}"))
            } else {
                Some(format!("{route_kind}:{route_path}?{canonical_query}"))
            }
        }
    }
}

pub fn hash_render_args(args: Option<&serde_json::Value>) -> String {
    let Some(val) = args else {
        return "none".to_owned();
    };
    if val.is_null() {
        return "none".to_owned();
    }
    let bytes = serde_json::to_vec(val).unwrap_or_default();
    let digest = Sha256::digest(&bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_key(route: &str, epoch: u64) -> RenderCacheKey {
        RenderCacheKey {
            owner_kind: ExtensionOwnerKind::Theme,
            owner_id: "starter".into(),
            revision_digest_or_generation: "rev1".into(),
            hook_or_slot_name: "slot:hero".into(),
            arg_hash: "none".into(),
            locale: "vi".into(),
            canonical_route_and_query_or_global: route.into(),
            origin_host: "127.0.0.1:8080".into(),
            site_data_epoch: epoch,
        }
    }

    #[test]
    fn site_cache_epoch_bumps_monotonically() {
        let e1 = SiteCacheEpoch::global().current();
        let e2 = SiteCacheEpoch::global().bump();
        let e3 = SiteCacheEpoch::global().current();
        assert!(e2 > e1);
        assert_eq!(e3, e2);
    }

    #[test]
    fn render_cache_evicts_lru_and_enforces_entry_byte_cap() {
        let cache = ExtensionRenderCache::with_limits(2, 64);
        let k1 = sample_key("single:/a", 1);
        let k2 = sample_key("single:/b", 1);
        let k3 = sample_key("single:/c", 1);

        cache.insert(
            k1.clone(),
            CachedRenderFragment {
                html: "<p>a</p>".into(),
                ..Default::default()
            },
        );
        cache.insert(
            k2.clone(),
            CachedRenderFragment {
                html: "<p>b</p>".into(),
                ..Default::default()
            },
        );
        // Touch k1 so k2 becomes least-recently-used.
        assert!(cache.get(&k1).is_some());
        cache.insert(
            k3.clone(),
            CachedRenderFragment {
                html: "<p>c</p>".into(),
                ..Default::default()
            },
        );
        assert_eq!(cache.len(), 2);
        assert!(cache.get(&k1).is_some());
        assert!(cache.get(&k2).is_none());
        assert!(cache.get(&k3).is_some());

        // Oversized entry (> 64 bytes) is rejected without evicting existing entries.
        let k4 = sample_key("single:/huge", 1);
        cache.insert(
            k4.clone(),
            CachedRenderFragment {
                html: "x".repeat(100),
                ..Default::default()
            },
        );
        assert!(cache.get(&k4).is_none());
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn canonical_query_strips_tracking_params_and_collapses_404() {
        assert_eq!(
            canonicalize_public_query(
                Some("utm_source=fb&page=2&fbclid=xyz"),
                QueryScope::ContentArchive
            ),
            "page=2"
        );
        assert_eq!(
            canonicalize_public_query(Some("utm_source=google&ref=home"), QueryScope::Single),
            ""
        );
        assert_eq!(
            canonical_route_key(
                RenderCachePolicy::ByRoute,
                "not_found",
                "/random-404-a",
                "",
                true
            ),
            Some("not_found".into())
        );
        assert_eq!(
            canonical_route_key(
                RenderCachePolicy::ByRoute,
                "not_found",
                "/random-404-b",
                "",
                true
            ),
            Some("not_found".into())
        );
        assert_eq!(
            canonical_route_key(RenderCachePolicy::Global, "single", "/post-1", "", false),
            Some("global".into())
        );
        assert_eq!(
            canonical_route_key(RenderCachePolicy::None, "single", "/post-1", "", false),
            None
        );
        assert_eq!(
            canonical_route_key(
                RenderCachePolicy::ByRoute,
                "search",
                "/",
                "s=arch",
                false
            ),
            None
        );
    }
}
