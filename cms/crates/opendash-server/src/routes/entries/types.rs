//! Authenticated content entry CRUD for CPTs (M3.3 backend).

use super::*;

pub(super) use std::{collections::BTreeSet, sync::Arc};

pub(super) use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, RawQuery, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};

pub(super) use opendash_core::content::{ContentEntryInput, valid_content_type_key};

pub(super) use serde::Deserialize;

pub(super) use serde_json::json;

pub(super) use sqlx::SqlitePool;

pub(super) use crate::{
    auth::{require_session, require_session_with_csrf},
    content_render,
    error::{ApiError, ApiResult},
    hooks::HookDispatcher,
    storage::{
        content::load_content_type_definition, entries as entry_storage, entry_table,
        media as media_storage, permalinks, public_content, settings,
    },
};

pub const MAX_CONTENT_ENTRY_JSON_BYTES: usize = 768 * 1024;

pub const MAX_METADATA_PATCH_JSON_BYTES: usize = 64 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationInput {
    pub status: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyLifecycleInput {}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RestoreEntryInput {
    #[serde(default)]
    pub confirm_public_restore: bool,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PurgeEntryQuery {
    #[serde(default)]
    pub retained_trash: bool,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct EntryPagination {
    pub(super) limit: Option<i64>,
    pub(super) offset: Option<i64>,
    pub(super) category_id: Option<i64>,
    pub(super) tag_id: Option<i64>,
    pub(super) include_descendants: Option<bool>,
    pub(super) locale: Option<String>,
}
