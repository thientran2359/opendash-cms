use super::*;

pub(super) use std::{collections::BTreeMap, sync::Arc};

pub(super) use opendash_core::hooks::{ExtensionOwnerKind, HookEvent};

pub(super) use serde::{Deserialize, Serialize};

pub(super) use serde_json::{Value, json};

pub(super) use sqlx::{Row, SqliteConnection, SqlitePool};

pub(super) use tokio::sync::watch;

pub(super) use crate::{
    error::{ApiError, ApiResult},
    hooks::HookDispatcher,
    operations::SiteCoordinator,
    seo::{
        MAX_JSON_LD_BLOCKS, MAX_JSON_LD_TOTAL_BYTES, SeoMetadataInput, json_value_depth,
        validate_json_ld_block, validate_json_ld_blocks, validate_seo_input,
    },
    storage::{plugin as plugin_storage, public_content},
};

pub const MAX_ENTRY_EXT_BYTES: usize = 32 * 1024;

pub const MAX_ENTRY_EXT_DEPTH: usize = 6;

pub const MAX_ENTRY_EXT_KEYS: usize = 64;

pub(in crate::storage) const BATCH_RECOMPUTE_LIMIT: i64 = 100;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeoHeadFilterOutput {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub og_image_id: Option<String>,
    #[serde(default)]
    pub canonical_url: Option<String>,
    #[serde(default)]
    pub noindex: Option<bool>,
    #[serde(default)]
    pub json_ld: Vec<Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntrySeoPluginOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub og_image_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_url: Option<String>,
    #[serde(default)]
    pub noindex: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(in crate::storage) struct StoredSeoProjection {
    #[serde(flatten)]
    pub(in crate::storage) meta: EntrySeoPluginOverride,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(in crate::storage) json_ld: Vec<Value>,
}

#[derive(Debug, Clone, Default)]
pub struct MergedEntryProjection {
    pub seo_override: Option<EntrySeoPluginOverride>,
    pub json_ld: Vec<Value>,
    pub ext: BTreeMap<String, Value>,
    pub theme_ext: BTreeMap<String, Value>,
}

pub(in crate::storage) struct EntryProjectionSource {
    pub(in crate::storage) entry_id: i64,
    pub(in crate::storage) version: i64,
    pub(in crate::storage) content_type_key: String,
    pub(in crate::storage) locale: String,
    pub(in crate::storage) payload: Value,
    pub(in crate::storage) plugin_generations: BTreeMap<String, i64>,
}

pub(in crate::storage) struct ComputedPluginProjection {
    pub(in crate::storage) owner_kind: ExtensionOwnerKind,
    pub(in crate::storage) plugin_id: String,
    pub(in crate::storage) theme_fence: Option<(String, i64)>,
    pub(in crate::storage) seo_json: Option<String>,
    pub(in crate::storage) ext_json: Option<String>,
}
