use std::collections::BTreeMap;

use opendash_core::builder::BuilderDocument;
use opendash_core::content::{
    ContentMode, ContentPublication, EffectiveContentSchema, EffectiveFieldEntry,
    EffectiveFieldStatus, FieldRole, PatchField, deserialize_patch_field,
};
use opendash_core::field_schema::FieldType;
use opendash_core::revisions::{
    CanonicalRevisionSeo, MissingRefsReport, RevisionDiffPayload, RevisionSource, RevisionTermRef,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::auth::SessionContext;
use crate::error::ApiError;
use crate::hooks::ContentBeforeSaveVeto;
use crate::seo::{EntrySeoSnapshot, SeoMetadataInput};
use crate::storage::entries::{ContentEntryRecord, OwnerFence, PluginActionFence};

/// Attribution metadata recorded alongside each persisted revision row.
#[derive(Debug, Clone, Copy)]
pub enum RevisionActor<'a> {
    User(&'a SessionContext),
    Plugin(&'a str),
    Theme(&'a str),
    Baseline,
    Import,
    System,
}

impl RevisionActor<'_> {
    pub fn into_parts(self) -> (Option<i64>, String) {
        match self {
            Self::User(ctx) => (Some(ctx.user.id), ctx.user.username.clone()),
            Self::Plugin(id) => (None, format!("plugin:{id}")),
            Self::Theme(id) => (None, format!("theme:{id}")),
            Self::Baseline => (None, "baseline".to_owned()),
            Self::Import => (None, "import".to_owned()),
            Self::System => (None, "system".to_owned()),
        }
    }
}

pub fn actor_from_owner_fence<'a>(fence: &'a OwnerFence) -> RevisionActor<'a> {
    match fence {
        OwnerFence::Plugin { plugin_id, .. } => RevisionActor::Plugin(plugin_id.as_str()),
        OwnerFence::Theme { theme_id, .. } => RevisionActor::Theme(theme_id.as_str()),
    }
}

pub fn actor_from_plugin_fence<'a>(fence: &'a PluginActionFence) -> RevisionActor<'a> {
    actor_from_owner_fence(&fence.owner_fence)
}

#[derive(Debug, Clone, Serialize)]
pub struct RevisionSummaryDto {
    pub id: i64,
    pub revision_number: i64,
    pub content_hash: String,
    pub source: RevisionSource,
    pub restored_from_revision_number: Option<i64>,
    pub author_user_id: Option<i64>,
    pub author_username: String,
    pub title: String,
    pub content_mode: ContentMode,
    pub entry_version: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevisionsListResponse {
    pub items: Vec<RevisionSummaryDto>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    pub current_version: i64,
    pub has_autosave: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevisionFieldView {
    pub key: String,
    pub label: String,
    pub field_type: String,
    pub value: Value,
    pub editable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevisionDetailDto {
    pub revision: RevisionSummaryDto,
    pub current_version: i64,
    pub slug: String,
    pub title: String,
    pub content_mode: ContentMode,
    pub fields: Vec<RevisionFieldView>,
    pub builder_document: Option<BuilderDocument>,
    pub term_ids: Vec<i64>,
    pub terms: Vec<RevisionTermRef>,
    pub primary_category_term_id: Option<i64>,
    pub page_layout_key: Option<String>,
    pub custom_template_key: Option<String>,
    pub seo: Option<CanonicalRevisionSeo>,
    pub has_hidden_admin_fields: bool,
    pub missing_refs: MissingRefsReport,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CompareTarget {
    Revision { rev_id: i64 },
    Current,
    Autosave,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompareTargetMeta {
    pub target: CompareTarget,
    pub revision_number: Option<i64>,
    pub label: String,
    pub updated_at: Option<String>,
    pub entry_version: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevisionCompareResponse {
    pub entry_id: i64,
    pub content_type_key: String,
    pub current_version: i64,
    pub from_meta: CompareTargetMeta,
    pub to_meta: CompareTargetMeta,
    pub diff: RevisionDiffPayload,
    pub from_missing_refs: MissingRefsReport,
    pub to_missing_refs: MissingRefsReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutosaveSurface {
    Editor,
    Studio,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutosaveSnapshotDto {
    pub source_surface: AutosaveSurface,
    pub base_version: i64,
    pub is_stale_base: bool,
    pub updated_at: String,
    pub client_seq: i64,
    pub title: String,
    pub content_mode: ContentMode,
    pub values: BTreeMap<String, Value>,
    pub cleared_fields: Vec<String>,
    pub primary_category_touched: bool,
    pub primary_category_term_id: Option<i64>,
    pub page_layout_touched: bool,
    pub page_layout_key: Option<String>,
    pub custom_template_touched: bool,
    pub custom_template_key: Option<String>,
    pub seo_touched: bool,
    pub seo: Option<CanonicalRevisionSeo>,
    pub builder_document: Option<BuilderDocument>,
    pub term_ids: Vec<i64>,
    pub has_hidden_admin_fields: bool,
    pub missing_refs: MissingRefsReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutosaveSnapshotResponse {
    pub current_version: i64,
    pub autosave: Option<AutosaveSnapshotDto>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutosaveUpsertInput {
    pub source_surface: AutosaveSurface,
    pub base_version: i64,
    pub client_seq: i64,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub title: PatchField<String>,
    pub content_mode: Option<ContentMode>,
    pub values: Option<BTreeMap<String, Value>>,
    #[serde(default)]
    pub cleared_fields: Vec<String>,
    pub builder_document: Option<BuilderDocument>,
    pub term_ids: Option<Vec<i64>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub primary_category_term_id: PatchField<i64>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub page_layout_key: PatchField<String>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub custom_template_key: PatchField<String>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub seo: PatchField<SeoMetadataInput>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutosaveUpsertResponse {
    pub applied: bool,
    pub saved_at: String,
    pub base_version: i64,
    pub current_version: i64,
    pub is_stale_base: bool,
    pub client_seq: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionRestoreRequest {
    #[serde(default)]
    pub acknowledge_missing_refs: bool,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum RestoreRevisionOutcome {
    Entry {
        entry: Box<ContentEntryRecord>,
        custom_path: Option<String>,
        seo: Option<EntrySeoSnapshot>,
        schema_token: String,
        revision: RevisionSummaryDto,
    },
    Pattern {
        entry_id: i64,
        type_key: String,
        slug: String,
        title: String,
        content_mode: ContentMode,
        version: i64,
        updated_at: String,
        document: Option<BuilderDocument>,
        publication_status: String,
        publication_policy: ContentPublication,
        diagnostics: Vec<opendash_core::builder_blocks::BlockDiagnostic>,
        had_freeform_wrap: bool,
        had_recovery: bool,
        revision: RevisionSummaryDto,
    },
}

#[derive(Debug)]
pub enum RestoreError {
    MissingRefs(MissingRefsReport),
    ValidationVeto(ContentBeforeSaveVeto),
    Api(ApiError),
}

impl From<ApiError> for RestoreError {
    fn from(err: ApiError) -> Self {
        Self::Api(err)
    }
}

impl From<sqlx::Error> for RestoreError {
    fn from(err: sqlx::Error) -> Self {
        Self::Api(err.into())
    }
}

impl From<serde_json::Error> for RestoreError {
    fn from(err: serde_json::Error) -> Self {
        Self::Api(err.into())
    }
}

/// Returns `true` when `caller_role` may view and edit an active field.
pub fn is_field_editable(field: &EffectiveFieldEntry, caller_role: FieldRole) -> bool {
    field.effective_status == EffectiveFieldStatus::Active
        && field.definition.effective_acl().0 <= caller_role
        && field.definition.effective_acl().1 <= caller_role
}

pub fn can_view_body(
    effective: &EffectiveContentSchema,
    type_key: &str,
    caller_role: FieldRole,
) -> bool {
    if type_key == "pattern" {
        return caller_role == FieldRole::Admin;
    }
    effective
        .effective_fields
        .get("body")
        .is_some_and(|f| caller_role >= f.definition.effective_acl().0)
}

pub fn is_body_editable(
    effective: &EffectiveContentSchema,
    type_key: &str,
    caller_role: FieldRole,
) -> bool {
    if type_key == "pattern" {
        return caller_role == FieldRole::Admin;
    }
    effective.effective_fields.get("body").is_some_and(|f| {
        f.definition.field_type == FieldType::Richtext && is_field_editable(f, caller_role)
    })
}

pub(super) fn caller_field_role(actor: &SessionContext) -> FieldRole {
    crate::storage::entries::actor_field_role(actor.user.role)
}
