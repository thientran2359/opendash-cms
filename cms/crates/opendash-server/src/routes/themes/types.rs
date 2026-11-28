//! Admin-only Theme Format V1 revision install/activation/rollback,
//! and revision-scoped `/theme/{revision_digest}/{*path}` asset serving.

use super::*;

pub(super) use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) use axum::{
    Extension, Json,
    body::Bytes,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};

pub(super) use opendash_core::theme::TemplateCandidate;

pub(super) use serde::{Deserialize, Serialize};

pub(super) use sha2::{Digest, Sha256};

pub(super) use sqlx::SqlitePool;

pub(super) use crate::{
    auth::{
        Role, require_admin, require_admin_with_csrf, require_session, require_session_with_csrf,
    },
    error::{ApiError, ApiResult},
    package::{V2ZipError, is_valid_zip_content_type, validate_theme_relative_path},
    storage::{
        overrides, patterns,
        theme::{
            self, RevisionActivationOutcome, RevisionInstallOutcome, ThemeList, ThemeRevisionRecord,
        },
    },
    theme::{parse_theme_zip, theme_asset_mime},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateOverrideSaveInput {
    pub(super) target: TemplateCandidate,
    pub(super) document: opendash_core::builder::BuilderDocument,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateOverridePreviewInput {
    pub(super) target: TemplateCandidate,
    pub(super) document: opendash_core::builder::BuilderDocument,
    #[serde(default)]
    pub(super) preview_entry: Option<overrides::TemplatePreviewEntry>,
    #[serde(default)]
    pub(super) preview_term: Option<overrides::TemplatePreviewTerm>,
    #[serde(default)]
    pub(super) preview_locale: Option<String>,
}

pub const MAX_STUDIO_PREVIEW_JSON_BYTES: usize = 512 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StudioPreviewRequest {
    pub(super) target: TemplateCandidate,
    #[serde(default)]
    pub(super) document: Option<opendash_core::builder::BuilderDocument>,
    #[serde(default)]
    pub(super) preview_entry: Option<overrides::TemplatePreviewEntry>,
    #[serde(default)]
    pub(super) preview_term: Option<overrides::TemplatePreviewTerm>,
    #[serde(default)]
    pub(super) preview_annotations: bool,
    #[serde(default)]
    pub(super) transient: Option<overrides::TransientStudioOverrides>,
    #[serde(default)]
    pub(super) preview_locale: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ThemeStoreRoot(pub PathBuf);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivateInput {
    pub(super) revision_id: String,
}

#[derive(Serialize)]
pub(super) struct ThemeInstallResponse {
    pub(super) revision_id: String,
    pub(super) theme_id: String,
    pub(super) version: String,
    pub(super) revision_digest: String,
    pub(super) source_format: &'static str,
    pub(super) activated: bool,
}

#[derive(Debug, Serialize)]
pub struct ThemeFunctionsSettingsResponse {
    pub theme_id: Option<String>,
    pub revision_id: Option<String>,
    pub has_functions: bool,
    pub version: i64,
    pub fields: Vec<opendash_core::field_schema::FieldSpec>,
    pub values: serde_json::Map<String, serde_json::Value>,
    pub actions: Vec<opendash_core::plugin::PluginActionDeclaration>,
}

#[derive(Debug, Deserialize)]
pub struct ThemeFunctionsLogsQuery {
    #[serde(default)]
    pub limit: Option<i64>,
}
