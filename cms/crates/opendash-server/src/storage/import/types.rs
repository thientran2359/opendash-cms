//! Selective sample import dry-run planner and logical provenance contract (M5-S05-T01).
//! Dry-run is strictly DB and filesystem read-only.

use super::*;

pub(super) use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub(super) use axum::http::StatusCode;

pub(super) use opendash_core::field_schema::{
    FieldContext, FieldSpec, FieldType, validate_field_values,
};

pub(super) use opendash_core::id::{valid_kebab_segment_with_limit, valid_snake_key};

pub(super) use opendash_core::theme::ThemeV3Manifest;

pub(super) use serde::{Deserialize, Serialize};

pub(super) use sha2::{Digest, Sha256};

pub(super) use sqlx::SqlitePool;

pub(super) use crate::{
    error::{ApiError, ApiResult},
    package::validate_theme_relative_path,
    storage::{
        settings,
        theme::{ThemeSourceFormat, find_revision_by_id, get_active_selection},
    },
};

pub const MAX_SAMPLE_MEDIA: usize = 50;

pub const MAX_SAMPLE_PAGES: usize = 50;

pub const MAX_SAMPLE_MENUS: usize = 20;

pub const MAX_SAMPLE_SETTINGS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SampleGroup {
    Pages,
    Menus,
    Media,
    Settings,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DryRunImportInput {
    pub groups: Vec<SampleGroup>,
    #[serde(default)]
    pub revision_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeSampleManifest {
    #[serde(default = "default_sample_key")]
    pub sample_key: String,
    #[serde(default)]
    pub media: Vec<SampleMediaItem>,
    #[serde(default)]
    pub pages: Vec<SamplePageItem>,
    #[serde(default)]
    pub menus: Vec<SampleMenuItem>,
    #[serde(default)]
    pub settings: Vec<SampleSettingItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SampleMediaItem {
    pub object_key: String,
    pub path: String,
    #[serde(default)]
    pub alt: String,
    #[serde(default)]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamplePageItem {
    pub object_key: String,
    pub slug: String,
    pub title: String,
    #[serde(default = "default_page_type")]
    pub content_type: String,
    #[serde(default)]
    pub values: serde_json::Value,
    #[serde(default)]
    pub media_refs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SampleMenuItem {
    pub object_key: String,
    pub menu_key: String,
    pub title: String,
    #[serde(default)]
    pub slot_key: Option<String>,
    #[serde(default)]
    pub items: Vec<SampleMenuEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SampleMenuEntry {
    pub label: String,
    pub target_kind: String,
    #[serde(default)]
    pub target_object_key: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SampleSettingItem {
    pub object_key: String,
    pub option_key: String,
    pub value: serde_json::Value,
    #[serde(default)]
    pub media_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanAction {
    Create,
    Skip,
    MissingDependency,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportPlanItem {
    pub resource_kind: SampleGroup,
    pub object_key: String,
    pub logical_target: String,
    pub action: PlanAction,
    pub reason: Option<String>,
    pub publication_implication: String,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnselectedDependency {
    pub required_group: SampleGroup,
    pub object_key: String,
    pub referenced_by_group: SampleGroup,
    pub referenced_by_object: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExpectedVersions {
    pub theme_selection_version: i64,
    pub theme_options_version: i64,
    pub menu_versions: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportPlan {
    pub theme_id: String,
    pub revision_id: String,
    pub revision_digest: String,
    pub sample_key: String,
    pub selected_groups: Vec<SampleGroup>,
    pub items: Vec<ImportPlanItem>,
    pub unselected_dependencies: Vec<UnselectedDependency>,
    pub expected_versions: ExpectedVersions,
    pub requires_public_ack: bool,
    pub publication_implications: Vec<String>,
    pub plan_digest: String,
}

/// Pure, pinned-schema validation; use before any filesystem staging or DB
/// mutation on install, dry-run and execute.
pub(crate) fn validate_sample_option_values(
    sample: &ThemeSampleManifest,
    fields: &[FieldSpec],
) -> Result<(), &'static str> {
    let definitions: BTreeMap<&str, &FieldSpec> = fields
        .iter()
        .filter(|s| !s.is_ui_only())
        .map(|spec| (spec.key.as_str(), spec))
        .collect();
    let image_keys: HashSet<&str> = sample
        .media
        .iter()
        .map(|item| item.object_key.as_str())
        .collect();
    for setting in &sample.settings {
        let spec = definitions
            .get(setting.option_key.as_str())
            .ok_or("Sample setting có option_key ngoài schema revision.")?;
        if let Some(media_ref) = setting.media_ref.as_deref() {
            if spec.field_type != FieldType::Media
                || !setting.value.is_null()
                || !image_keys.contains(media_ref)
            {
                return Err("Sample media_ref chỉ dùng với media, null và asset có khai báo.");
            }
        } else {
            let map = BTreeMap::from([(setting.option_key.clone(), setting.value.clone())]);
            validate_field_values(
                std::slice::from_ref(spec),
                FieldContext::Options,
                &map,
                None,
            )?;
            settings::validate_url_or_link_spec_strict(spec, &setting.value)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteImportInput {
    pub groups: Vec<SampleGroup>,
    #[serde(default)]
    pub revision_id: Option<String>,
    pub plan_digest: String,
    pub expected_versions: ExpectedVersions,
    #[serde(default)]
    pub acknowledge_public_changes: bool,
    #[serde(default)]
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportOutcomeItem {
    pub resource_kind: SampleGroup,
    pub object_key: String,
    pub target_id: String,
    pub status: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportExecutionOutcome {
    pub run_id: String,
    pub theme_id: String,
    pub revision_id: String,
    pub sample_key: String,
    pub plan_digest: String,
    pub created: Vec<ImportOutcomeItem>,
    pub skipped: Vec<ImportOutcomeItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedMediaEntry {
    pub object_key: String,
    pub media_id: String,
    pub extension: String,
    pub sha256: String,
    pub already_existed: bool,
    pub filename: String,
    pub mime_type: String,
    pub byte_size: i64,
    pub width: i64,
    pub height: i64,
    pub thumb_width: i64,
    pub thumb_height: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportCutpoint {
    None,
    AfterPhase1Staged,
    AfterPhase2FilesVerified,
}

pub(in crate::storage) async fn write_staged_and_promote(
    uploads_root: &std::path::Path,
    run_id: &str,
    id: &str,
    ext: &str,
    original: &[u8],
    thumbnail: &[u8],
) -> ApiResult<bool> {
    use tokio::io::AsyncWriteExt;

    let media_dir = uploads_root.join("media").join(&id[..2]);
    let orig_dest = media_dir.join(format!("{id}.{ext}"));
    let thumb_dest = media_dir.join(format!("{id}.thumb.png"));

    if let (Ok(orig_bytes), Ok(thumb_bytes)) = (
        tokio::fs::read(&orig_dest).await,
        tokio::fs::read(&thumb_dest).await,
    ) {
        if sha256_hex(&orig_bytes) == id && !thumb_bytes.is_empty() {
            return Ok(true);
        }
    }

    let staging_dir = uploads_root.join(".staging").join(run_id);
    tokio::fs::create_dir_all(&staging_dir)
        .await
        .map_err(|_| ApiError::internal("Không tạo được thư mục staging."))?;
    tokio::fs::create_dir_all(&media_dir)
        .await
        .map_err(|_| ApiError::internal("Không tạo được thư mục media."))?;

    let stage_orig = staging_dir.join(format!("{id}.{ext}"));
    let stage_thumb = staging_dir.join(format!("{id}.thumb.png"));

    for (stage_path, bytes) in [(&stage_orig, original), (&stage_thumb, thumbnail)] {
        let mut f = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(stage_path)
            .await
            .map_err(|_| ApiError::internal("Không mở được file staging."))?;
        f.write_all(bytes)
            .await
            .map_err(|_| ApiError::internal("Không ghi được file staging."))?;
        f.sync_all()
            .await
            .map_err(|_| ApiError::internal("Không đồng bộ được file staging."))?;
        drop(f);
    }

    let mut already = false;
    if !tokio::fs::try_exists(&orig_dest).await.unwrap_or(false) {
        tokio::fs::rename(&stage_orig, &orig_dest)
            .await
            .map_err(|_| ApiError::internal("Không thể di chuyển file media từ staging."))?;
    } else {
        already = true;
    }
    if !tokio::fs::try_exists(&thumb_dest).await.unwrap_or(false) {
        tokio::fs::rename(&stage_thumb, &thumb_dest)
            .await
            .map_err(|_| ApiError::internal("Không thể di chuyển thumbnail từ staging."))?;
    }
    Ok(already)
}
