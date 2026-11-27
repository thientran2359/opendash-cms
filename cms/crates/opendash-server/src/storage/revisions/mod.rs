//! Transactional revision capture, baseline initialization, deduplication,
//! pruning, and per-user autosave cleanup.

mod capture;
mod types;

#[cfg(test)]
mod capture_tests;

pub use capture::*;
pub use types::*;
