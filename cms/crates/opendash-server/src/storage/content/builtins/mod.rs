//! Persistent CPT definitions and extension field groups. All content SQL stays in the storage layer.

use super::*;

pub(super) use std::collections::{BTreeSet, HashMap, HashSet};

pub(super) use opendash_core::{
    content::{
        AttachedFieldGroupSnapshot, ContentPublication, ContentTypeDefinition,
        ContentTypeDefinitionInput, ContentTypeSettings, EditorLayoutConfigV1, EditorRegion,
        EffectiveContentSchema, EffectiveEditorConfigProjection, EffectiveFieldStatus,
        EffectiveTermSchema, FieldGroupDomain, FieldGroupPresentation, FieldGroupRecord,
        FieldGroupStatus, FieldGroupTargetBinding, FieldRole, LocationRuleGroups,
        NormalizedFieldGroup, PanelDefinition, PanelKind, PatchField, SchemaTransitionError,
        UserFieldGroupInput, build_core_and_group_panels, extract_materialized_targets,
        merge_package_redeclared_fields, project_effective_editor_config,
        resolve_and_validate_editor_layout, valid_content_type_key,
        verify_effective_schema_transition, verify_effective_term_schema_transition,
    },
    id::ScopedResourceId,
    plugin::PluginManifest,
    registry::{DeclarationState, OwnedDescriptor, ProviderStatus, RegistryCatalog, ResourceOwner},
};

pub(super) use serde::Serialize;

pub(super) use sha2::{Digest, Sha256};

pub(super) use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

pub(super) use crate::{
    error::{ApiError, ApiResult},
    theme::ThemeDeclarations,
};

mod contenttypeapiitem;
pub use contenttypeapiitem::*;
mod sync_field_groups;
mod sync_owner_cnt_declarations_in_tx;
pub use sync_owner_cnt_declarations_in_tx::*;
mod update_user_field_group;
pub use update_user_field_group::*;
mod put_site_taxonomy_eligibility;
pub use put_site_taxonomy_eligibility::*;
