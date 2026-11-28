//! Unified Clean-Slate V1 (`1.0.0`) field schema, validation, canonicalization,
//! DNF conditional logic, ACL invariants (`M7-S09`), schema compatibility
//! checks (`COMPAT-1..4`), and shared helper functions.
//!
//! Strictly pure Rust (`std`, `serde`, `serde_json`, `url`) with zero I/O, HTTP, or SQL.

use super::*;

pub(crate) use std::collections::{BTreeMap, BTreeSet};

pub(crate) use serde::{Deserialize, Serialize};

pub(crate) use serde_json::Value;

pub(crate) use url::Url;

pub(crate) use crate::{
    builder::{BoxSpacing, compile_box_spacing_tailwind},
    builder_blocks::{MAX_BUILDER_BLOCK_BODY_BYTES, contains_blocks},
    icons::lookup_core_icon_paths,
    id::valid_snake_key,
    richtext::{MAX_RICHTEXT_OUTPUT_BYTES, sanitize_richtext, strip_richtext_to_plain},
};

mod limits;
pub use limits::*;
mod validate_context_caps;
pub(crate) use validate_context_caps::*;
mod validate_single_spec;
pub use validate_single_spec::*;
mod validate_non_responsive;
pub use validate_non_responsive::*;
