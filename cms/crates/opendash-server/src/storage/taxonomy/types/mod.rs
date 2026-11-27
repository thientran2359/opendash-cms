//! Site-owned taxonomy. All category tree validation and writes share one
//! SQLite writer reservation; no parent/subtree read precedes that reservation.

use super::*;

pub(super) use std::collections::BTreeMap;

pub(super) use opendash_core::{
    content::{TaxonomyDefinition, TaxonomyDefinitionInput, TaxonomyDefinitionPatch},
    registry::{DeclarationState, ProviderStatus, ResourceOwner},
};

pub(super) use serde::Serialize;

pub(super) use serde_json::Value;

pub(super) use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

pub(super) use crate::{
    auth::{self, SessionContext},
    error::{ApiError, ApiResult},
};

mod max_category_depth;
pub use max_category_depth::*;
mod create_term_with_locale;
pub use create_term_with_locale::*;
