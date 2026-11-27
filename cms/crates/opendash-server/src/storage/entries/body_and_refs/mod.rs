//! CRUD for saved content entries. Never puts SQL inside HTTP routes.

use super::*;

pub(super) use std::collections::BTreeMap;

pub(super) use opendash_core::content::{
    ContentEntryInput, ContentMode, ContentPublication, ContentSupport, ContentTypeDefinition,
    ContentTypeSettings, EffectiveContentSchema, EffectiveFieldStatus, FieldRole, PatchField,
    PublicExposure, deserialize_patch_field,
};

pub(super) use serde::Serialize;

pub(super) use serde_json::Value;

pub(super) use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

pub(super) use crate::{
    auth::{self, SessionContext},
    error::{ApiError, ApiResult},
};

mod forbid_generic_pattern_write;
pub use forbid_generic_pattern_write::*;
mod schema_fenced;
pub use schema_fenced::*;
mod field_media_refs;
pub use field_media_refs::*;
mod prepare_composite_entry_save;
pub use prepare_composite_entry_save::*;
mod commit_composite_entry_save;
pub use commit_composite_entry_save::*;
mod set_publication_status;
pub use set_publication_status::*;
