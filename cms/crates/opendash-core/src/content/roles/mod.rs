//! Content-type and custom-field schema for the CMS content module (M3 / M7-S09).
//! Identifier rules come from `crate::id`; resource ownership is recorded by
//! wrapping a definition in `registry::OwnedDescriptor<ContentTypeDefinition>`.

use super::*;

pub(crate) use std::collections::{BTreeMap, HashMap, HashSet};

pub(crate) use serde::{Deserialize, Serialize};

pub(crate) use serde_json::Value;

pub(crate) use crate::{
    id::{ScopedResourceId, valid_kebab_segment_with_limit, valid_snake_key},
    registry::{DeclarationState, OwnedDescriptor, ProviderStatus, ResourceOwner},
};

/// Valid identifiers start with a lowercase ASCII letter and contain only
/// lowercase letters, digits, or underscores (`1..=64` chars).
mod definitions;
pub use definitions::*;
mod effective_schema;
pub use effective_schema::*;
mod declarations;
pub use declarations::*;
mod editor_panels;
pub use editor_panels::*;
