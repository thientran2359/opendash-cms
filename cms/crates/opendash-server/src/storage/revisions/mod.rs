//! Transactional revision capture, baseline initialization, deduplication,
//! pruning, inspection/diffing, restore preflight/execution, and per-user autosaves.

mod autosave;
mod autosave_merge;
mod builder_refs;
mod capture;
mod custom_field_refs;
mod diff_builder;
mod field_refs;
mod read_and_diff;
mod ref_cache;
mod restore;
mod restore_preflight;
mod sanitize;
mod snapshot_queries;
mod types;

#[cfg(test)]
mod capture_tests;

pub use autosave::*;
pub use capture::*;
pub use read_and_diff::*;
pub use restore::*;
pub use types::*;
