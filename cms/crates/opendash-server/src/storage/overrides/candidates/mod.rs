//! Site-owned typed templates. Candidate specificity wins before source priority:
//! for each candidate, site-owned Builder AST precedes active theme v3 HTML.

use super::*;

pub(super) use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};

pub(super) use opendash_core::{
    builder::{BuilderDocument, ElementContent, KnownElement},
    content::{
        ContentSupport, ContentTypeDefinition, EffectiveContentSchema, FieldRole,
        valid_content_type_key,
    },
    registry::{DeclarationState, ProviderStatus},
    theme::{
        EnqueueCondition, PageLayoutChrome, ResolvedEnqueuedAssets, TemplateCandidate,
        TemplateCandidateKind, resolve_enqueued_assets,
    },
};

pub(super) use serde::{Deserialize, Serialize};

pub(super) use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

pub(super) use crate::{
    auth::{Role, SessionContext},
    content_render::{self, EntryView, PaginationView, PublicThemeContextInput, SiteContext},
    error::{ApiError, ApiResult},
    page::{self, RenderAnnotationScope, RenderedLayout},
    storage::{
        builder::{self, BuilderRenderContext, check_preview_images, resolve_contact_forms_in_tx},
        builder_binding, content, entries, menu, permalinks, public_content, settings, theme,
        theme::ActiveThemeV3,
    },
    template_sec::{self, PreviewCompositionBudget},
};

mod publictemplatecontext;
pub use publictemplatecontext::*;
mod render_preview_part;
pub use render_preview_part::*;
mod list_studio_targets;
pub use list_studio_targets::*;
mod preview_studio_with_cache_and_locale;
pub use preview_studio_with_cache_and_locale::*;
mod wrap_entry_body_in_preview_shell_in_tx_with_budget;
pub use wrap_entry_body_in_preview_shell_in_tx_with_budget::*;
