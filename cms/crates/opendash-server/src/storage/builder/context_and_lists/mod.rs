//! Entry & Studio Builder persistence, validation, and projection helpers.

use super::*;

pub(super) use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(super) use opendash_core::{
    block_schema::RegisteredBlockSchema,
    builder::{BindingSource, BuilderDocument, ElementContent, KnownElement},
    builder_blocks::BlockDiagnostic,
    content::{
        ContentMode, ContentPublication, ContentSupport, EffectiveContentSchema,
        EffectiveFieldStatus, FieldRole, PublicExposure,
    },
    id::valid_namespaced_block_id,
    registry::{DeclarationState, ProviderStatus, RegistryCatalog},
};

pub(super) use serde::Serialize;

pub(super) use serde_json::{Value, json};

pub(super) use sqlx::{QueryBuilder, Sqlite, SqliteConnection, SqlitePool, Transaction};

pub(super) use crate::{
    auth::{Role, SessionContext},
    error::{ApiError, ApiResult},
    hooks::{ContentBeforeSaveVeto, HookDispatcher},
    page::{self, RenderAnnotationScope, RenderedLayout},
    storage::media::valid_media_id,
    storage::{
        builder_binding,
        entries::{self, EntryFilters, PublishedListResult, PublishedListSort},
    },
};

mod builderrendercontext;
pub use builderrendercontext::*;
mod media_hydration;
pub use media_hydration::*;
mod render_entry_builder_preview;
pub use render_entry_builder_preview::*;
