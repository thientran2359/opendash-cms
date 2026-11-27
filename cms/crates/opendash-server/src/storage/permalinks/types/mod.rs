//! C3: transactional authority for canonical routes and their stable-ID history.
//! All writers share sync_claims_in_tx before committing their source changes.

use super::*;

pub(super) use std::collections::{BTreeMap, HashSet};

pub(super) use opendash_core::content::valid_content_type_key;

pub(super) use opendash_core::id::valid_kebab_segment_with_limit;

pub(super) use serde::{Deserialize, Serialize};

pub(super) use sqlx::{SqliteConnection, SqlitePool};

pub(super) use crate::error::{ApiError, ApiResult};

mod max_claims;
pub use max_claims::*;
mod sync_claims_with_options_and_swap_and_limit;
pub use sync_claims_with_options_and_swap_and_limit::*;
mod permalinkputerror;
pub use permalinkputerror::*;
