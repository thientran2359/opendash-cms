//! M7-S03-T01 SQLite storage for custom entry paths, ID-based redirects, and route claims.

use super::*;

pub(super) use std::collections::BTreeMap;

pub(super) use opendash_core::content::PatchField;

pub(super) use serde::Serialize;

pub(super) use sqlx::{SqliteConnection, SqlitePool};

pub(super) use crate::{
    error::{ApiError, ApiResult},
    seo::validate_custom_entry_path,
    storage::entries::ContentEntryRecord,
};

mod is_reserved_route_root;
pub use is_reserved_route_root::*;
mod resolve_custom_or_redirect_path;
pub use resolve_custom_or_redirect_path::*;
