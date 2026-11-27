//! Pure Rust canonicalization and 3-layer diff engine for revisions and autosaves.

mod block_diff;
mod canonical;
mod text_diff;
mod types;

#[cfg(test)]
mod tests;

pub use block_diff::*;
pub use canonical::*;
pub use text_diff::*;
pub use types::*;

pub const MAX_REVISIONS_PER_ENTRY: usize = 25;
pub const MAX_DIFF_LINES: usize = 2_000;
pub const MAX_DIFF_TOKENS_PER_LINE: usize = 512;
pub const MAX_DP_CELLS: usize = 500_000;
pub const MAX_TOTAL_WORD_DP_CELLS: usize = 500_000;
pub const MAX_BLOCK_DIFF_NODES: usize = 1_024;
