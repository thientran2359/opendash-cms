//! SQLite FTS5 transactional index synchronization and public search queries.

mod index_sync;
mod query;

#[allow(unused_imports)]
pub use index_sync::*;
pub use query::*;

#[cfg(test)]
mod tests;

/// Single source of truth for public search eligibility across indexing,
/// reconciliation, and queries.
pub(crate) const ELIGIBLE_ENTRY_PREDICATE: &str = "\
e.trashed_at IS NULL \
AND e.publication_status = 'published' \
AND e.content_type_key NOT IN ('pattern', 'section') \
AND t.provider_status = 'active' \
AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1 \
AND sl.enabled = 1 \
AND sl.deleted_at IS NULL \
AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)";
