//! Shared storage-owned entry lifecycle model, predicates, and transactional transitions.
//!
//! All ordinary entry lifecycle transitions (Trash, Restore, Purge) execute inside a
//! single caller-reserved SQLite writer transaction with authoritative actor, role,
//! provider, locale, Reading Page, schema, and CAS validation.

use super::*;

pub(super) use std::collections::{BTreeMap, HashMap, HashSet};

pub(super) use opendash_core::{
    content::{
        ContentPublication, EffectiveFieldStatus, valid_content_type_key, valid_field_value,
    },
    id::valid_kebab_segment_with_limit,
};

pub(super) use serde::Serialize;

pub(super) use serde_json::Value;

pub(super) use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

pub(super) use crate::{
    auth::{Role, SessionContext, current_time_secs},
    error::{ApiError, ApiResult},
    storage::entries::PluginActionFence,
};

mod active_entry_predicate;
pub use active_entry_predicate::*;
mod apply_lifecycle_in_tx;
pub use apply_lifecycle_in_tx::*;
mod preflight_plugin_lifecycle_confirmation;
pub use preflight_plugin_lifecycle_confirmation::*;
mod commit_metadata_patch;
pub use commit_metadata_patch::*;
mod restore_compatibility;
pub use restore_compatibility::*;
