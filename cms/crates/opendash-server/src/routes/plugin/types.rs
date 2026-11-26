use super::*;

pub(super) use std::sync::Arc;

pub(super) use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};

pub(super) use serde_json::Value;

pub(super) use sqlx::SqlitePool;

pub(super) use crate::{
    auth::{require_admin, require_admin_with_csrf, require_session_with_csrf},
    error::{ApiError, ApiResult},
    hooks::HookDispatcher,
    package::is_valid_zip_content_type,
    plugin::PluginManager,
};

pub const MAX_PLUGIN_ZIP_BYTES: usize = 5 * 1024 * 1024; // 5 MiB

pub const MAX_PLUGIN_JSON_BYTES: usize = 1024 * 1024; // 1 MiB

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmPluginLifecycleInput {
    pub confirmation_id: String,
    pub decision: String,
}

pub const MAX_PLUGIN_SETTINGS_JSON_BYTES: usize = 64 * 1024;

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginSettingsPutInput {
    pub values: serde_json::Map<String, Value>,
}

pub(super) fn map_plugin_storage_err(err: crate::storage::plugin::PluginStorageError) -> ApiError {
    use crate::storage::plugin::PluginStorageError;
    match err {
        PluginStorageError::BadRequest(m) => ApiError::bad_request(m),
        PluginStorageError::NotFound(m) => ApiError::not_found(m),
        PluginStorageError::Conflict(m) => ApiError::conflict(m),
        PluginStorageError::PreconditionRequired(m) => ApiError::precondition_required(m),
        PluginStorageError::PreconditionFailed(m) => ApiError::precondition_failed(m),
        PluginStorageError::Internal(m) => ApiError::internal(m),
    }
}

pub const UI_SCHEMA_VERSION: u32 = 1;

pub const MAX_UI_SCREENS: usize = 20;

pub const MAX_UI_DEPTH: usize = 8;

pub const MAX_UI_NODES: usize = 200;

pub const MAX_UI_PAGE_SIZE: usize = 100;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeclarativeUiSchemaV1 {
    pub plugin_id: String,
    pub schema_version: u32,
    #[serde(default)]
    pub screens: Vec<UiScreen>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UiScreen {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub nodes: Vec<UiNode>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UiNode {
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub field_type: Option<String>,
    #[serde(default)]
    pub action_id: Option<String>,
    #[serde(default)]
    pub page_size: Option<usize>,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub children: Vec<UiNode>,
}
