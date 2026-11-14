//! Selective sample import dry-run planner and logical provenance contract (M5-S05-T01).
//! Dry-run is strictly DB and filesystem read-only.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use axum::http::StatusCode;
use opendash_core::id::{valid_kebab_segment_with_limit, valid_snake_key};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::{
    error::{ApiError, ApiResult},
    package::validate_v2_relative_path,
    storage::theme::{ThemeSourceFormat, find_revision_by_id, get_active_selection},
    theme::ThemeV2Manifest,
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

impl SampleGroup {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pages => "pages",
            Self::Menus => "menus",
            Self::Media => "media",
            Self::Settings => "settings",
        }
    }
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

fn default_sample_key() -> String {
    "default".to_string()
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

fn default_page_type() -> String {
    "page".to_string()
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

fn valid_object_key(raw: &str) -> bool {
    !raw.is_empty()
        && raw.len() <= 128
        && raw
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_'))
}

fn validate_sample_manifest(
    sample: &ThemeSampleManifest,
    file_digests: &BTreeMap<String, String>,
) -> ApiResult<()> {
    if !valid_object_key(&sample.sample_key) || sample.sample_key.len() > 64 {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "sample_key không hợp lệ.".into(),
        ));
    }
    if sample.media.len() > MAX_SAMPLE_MEDIA
        || sample.pages.len() > MAX_SAMPLE_PAGES
        || sample.menus.len() > MAX_SAMPLE_MENUS
        || sample.settings.len() > MAX_SAMPLE_SETTINGS
    {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Số lượng phần tử dữ liệu mẫu vượt quá giới hạn cho phép.".into(),
        ));
    }

    let mut media_keys = HashSet::new();
    for m in &sample.media {
        if !valid_object_key(&m.object_key) || !media_keys.insert(m.object_key.as_str()) {
            return Err(ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "object_key của sample media không hợp lệ hoặc trùng lặp.".into(),
            ));
        }
        let safe_rel = validate_v2_relative_path(&m.path).map_err(|_| {
            ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Đường dẫn media trong sample không hợp lệ.".into(),
            )
        })?;
        if !file_digests.contains_key(&safe_rel) {
            return Err(ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "File media được khai báo trong sample không tồn tại trong revision theme.".into(),
            ));
        }
    }

    let mut page_keys = HashSet::new();
    for p in &sample.pages {
        if !valid_object_key(&p.object_key) || !page_keys.insert(p.object_key.as_str()) {
            return Err(ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "object_key của sample page không hợp lệ hoặc trùng lặp.".into(),
            ));
        }
        if !valid_kebab_segment_with_limit(&p.slug, 64)
            || !valid_snake_key(&p.content_type)
            || p.title.trim().is_empty()
        {
            return Err(ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Thông tin slug/title/content_type của sample page không hợp lệ.".into(),
            ));
        }
        for mref in &p.media_refs {
            if !media_keys.contains(mref.as_str()) {
                return Err(ApiError(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Sample page tham chiếu tới media_ref không tồn tại trong sample.".into(),
                ));
            }
        }
    }

    let mut menu_keys = HashSet::new();
    for menu in &sample.menus {
        if !valid_object_key(&menu.object_key)
            || !menu_keys.insert(menu.object_key.as_str())
            || !valid_snake_key(&menu.menu_key)
            || menu.title.trim().is_empty()
        {
            return Err(ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Khai báo sample menu không hợp lệ hoặc trùng lặp.".into(),
            ));
        }
        for entry in &menu.items {
            match entry.target_kind.as_str() {
                "entry" => {
                    let Some(target) = entry.target_object_key.as_deref() else {
                        return Err(ApiError(
                            StatusCode::UNPROCESSABLE_ENTITY,
                            "Menu entry kiểu 'entry' thiếu target_object_key.".into(),
                        ));
                    };
                    if !page_keys.contains(target) {
                        return Err(ApiError(
                            StatusCode::UNPROCESSABLE_ENTITY,
                            "Sample menu tham chiếu tới page không tồn tại trong sample.".into(),
                        ));
                    }
                }
                "external" => {
                    if entry.url.as_deref().unwrap_or("").trim().is_empty() {
                        return Err(ApiError(
                            StatusCode::UNPROCESSABLE_ENTITY,
                            "Menu entry kiểu 'external' thiếu url.".into(),
                        ));
                    }
                }
                _ => {
                    return Err(ApiError(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "Loại target menu sample không được hỗ trợ.".into(),
                    ));
                }
            }
        }
    }

    let mut setting_keys = HashSet::new();
    for s in &sample.settings {
        if !valid_object_key(&s.object_key)
            || !setting_keys.insert(s.object_key.as_str())
            || !valid_snake_key(&s.option_key)
        {
            return Err(ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Khai báo sample setting không hợp lệ hoặc trùng lặp.".into(),
            ));
        }
        if let Some(mref) = s.media_ref.as_deref() {
            if !media_keys.contains(mref) {
                return Err(ApiError(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Sample setting tham chiếu tới media_ref không tồn tại trong sample.".into(),
                ));
            }
        }
    }

    Ok(())
}

async fn has_tombstone(
    db: &SqlitePool,
    theme_id: &str,
    sample_key: &str,
    kind: SampleGroup,
    object_key: &str,
) -> ApiResult<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM import_tombstones
            WHERE theme_id = ? AND sample_key = ? AND resource_kind = ? AND object_key = ?
        )",
    )
    .bind(theme_id)
    .bind(sample_key)
    .bind(kind.as_str())
    .bind(object_key)
    .fetch_one(db)
    .await
    .map_err(Into::into)
}

async fn has_provenance(
    db: &SqlitePool,
    theme_id: &str,
    sample_key: &str,
    kind: SampleGroup,
    object_key: &str,
) -> ApiResult<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM import_provenance
            WHERE theme_id = ? AND sample_key = ? AND resource_kind = ? AND object_key = ?
        )",
    )
    .bind(theme_id)
    .bind(sample_key)
    .bind(kind.as_str())
    .bind(object_key)
    .fetch_one(db)
    .await
    .map_err(Into::into)
}

pub async fn plan_sample_import(
    db: &SqlitePool,
    theme_id: &str,
    input: &DryRunImportInput,
) -> ApiResult<ImportPlan> {
    if input.groups.is_empty() {
        return Err(ApiError::bad_request(
            "Cần chọn ít nhất một nhóm dữ liệu (pages/menus/media/settings).",
        ));
    }
    let selected_set: BTreeSet<SampleGroup> = input.groups.iter().copied().collect();
    let selected_groups: Vec<SampleGroup> = selected_set.iter().copied().collect();

    let selection = get_active_selection(db).await?;
    let target_rev_id = match (&input.revision_id, &selection.revision_id) {
        (Some(rid), _) => rid.clone(),
        (None, Some(active_rid)) => active_rid.clone(),
        (None, None) => {
            return Err(ApiError::not_found(
                "Chưa có revision theme v2 đang hoạt động.",
            ));
        }
    };

    let rev = find_revision_by_id(db, &target_rev_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Revision theme chưa được cài đặt."))?;
    if rev.theme_id != theme_id || rev.source_format != ThemeSourceFormat::V2 {
        return Err(ApiError::not_found(
            "Revision không thuộc theme yêu cầu hoặc không phải v2.",
        ));
    }

    let manifest: ThemeV2Manifest = serde_json::from_str(&rev.manifest_json)
        .map_err(|_| ApiError::internal("Manifest theme v2 không hợp lệ."))?;
    let Some(raw_sample) = manifest.sample else {
        return Err(ApiError::not_found("Theme không khai báo dữ liệu mẫu (sample)."));
    };
    let sample: ThemeSampleManifest = serde_json::from_value(raw_sample).map_err(|_| {
        ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Khai báo sample trong theme.json không đúng định dạng.".into(),
        )
    })?;
    validate_sample_manifest(&sample, &rev.file_digests)?;

    let mut items = Vec::new();
    let mut unselected_dependencies = Vec::new();
    let mut menu_versions = BTreeMap::new();
    let mut requires_public_ack = false;
    let mut implications_set = BTreeSet::new();

    if selected_set.contains(&SampleGroup::Media) {
        let mut sorted = sample.media.clone();
        sorted.sort_by(|a, b| a.object_key.cmp(&b.object_key));
        for m in sorted {
            let (action, reason) =
                if has_tombstone(db, theme_id, &sample.sample_key, SampleGroup::Media, &m.object_key)
                    .await?
                {
                    (PlanAction::Skip, Some("tombstone_exists".to_string()))
                } else if has_provenance(
                    db,
                    theme_id,
                    &sample.sample_key,
                    SampleGroup::Media,
                    &m.object_key,
                )
                .await?
                {
                    (PlanAction::Skip, Some("already_imported".to_string()))
                } else {
                    (PlanAction::Create, None)
                };
            implications_set.insert("media: private_until_referenced".to_string());
            items.push(ImportPlanItem {
                resource_kind: SampleGroup::Media,
                object_key: m.object_key,
                logical_target: format!("media:{}", m.path),
                action,
                reason,
                publication_implication: "private_until_referenced".to_string(),
                dependencies: Vec::new(),
            });
        }
    }

    if selected_set.contains(&SampleGroup::Pages) {
        let mut sorted = sample.pages.clone();
        sorted.sort_by(|a, b| a.object_key.cmp(&b.object_key));
        for p in sorted {
            let mut missing_deps = Vec::new();
            if !selected_set.contains(&SampleGroup::Media) {
                for mref in &p.media_refs {
                    missing_deps.push(format!("media:{mref}"));
                    unselected_dependencies.push(UnselectedDependency {
                        required_group: SampleGroup::Media,
                        object_key: mref.clone(),
                        referenced_by_group: SampleGroup::Pages,
                        referenced_by_object: p.object_key.clone(),
                    });
                }
            }
            let exists_site: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM content_entries WHERE content_type_key = ? AND slug = ?)",
            )
            .bind(&p.content_type)
            .bind(&p.slug)
            .fetch_one(db)
            .await?;

            let (action, reason) = if !missing_deps.is_empty() {
                (
                    PlanAction::MissingDependency,
                    Some("unselected_dependency:media".to_string()),
                )
            } else if has_tombstone(
                db,
                theme_id,
                &sample.sample_key,
                SampleGroup::Pages,
                &p.object_key,
            )
            .await?
            {
                (PlanAction::Skip, Some("tombstone_exists".to_string()))
            } else if has_provenance(
                db,
                theme_id,
                &sample.sample_key,
                SampleGroup::Pages,
                &p.object_key,
            )
            .await?
            {
                (PlanAction::Skip, Some("already_imported".to_string()))
            } else if exists_site {
                (PlanAction::Skip, Some("site_conflict".to_string()))
            } else {
                (PlanAction::Create, None)
            };

            implications_set.insert("pages: created_as_draft".to_string());
            items.push(ImportPlanItem {
                resource_kind: SampleGroup::Pages,
                object_key: p.object_key,
                logical_target: format!("{}:{}", p.content_type, p.slug),
                action,
                reason,
                publication_implication: "created_as_draft".to_string(),
                dependencies: p.media_refs.iter().map(|r| format!("media:{r}")).collect(),
            });
        }
    }

    if selected_set.contains(&SampleGroup::Menus) {
        let mut sorted = sample.menus.clone();
        sorted.sort_by(|a, b| a.object_key.cmp(&b.object_key));
        for menu in sorted {
            let mut deps = Vec::new();
            let mut missing = false;
            for entry in &menu.items {
                if entry.target_kind == "entry" {
                    if let Some(target) = entry.target_object_key.as_deref() {
                        deps.push(format!("pages:{target}"));
                        if !selected_set.contains(&SampleGroup::Pages) {
                            missing = true;
                            unselected_dependencies.push(UnselectedDependency {
                                required_group: SampleGroup::Pages,
                                object_key: target.to_string(),
                                referenced_by_group: SampleGroup::Menus,
                                referenced_by_object: menu.object_key.clone(),
                            });
                        }
                    }
                }
            }

            let existing_ver: Option<i64> =
                sqlx::query_scalar("SELECT version FROM site_menus WHERE menu_key = ?")
                    .bind(&menu.menu_key)
                    .fetch_optional(db)
                    .await?;
            if let Some(ver) = existing_ver {
                menu_versions.insert(menu.menu_key.clone(), ver);
            }

            let (action, reason) = if missing {
                (
                    PlanAction::MissingDependency,
                    Some("unselected_dependency:pages".to_string()),
                )
            } else if has_tombstone(
                db,
                theme_id,
                &sample.sample_key,
                SampleGroup::Menus,
                &menu.object_key,
            )
            .await?
            {
                (PlanAction::Skip, Some("tombstone_exists".to_string()))
            } else if has_provenance(
                db,
                theme_id,
                &sample.sample_key,
                SampleGroup::Menus,
                &menu.object_key,
            )
            .await?
            {
                (PlanAction::Skip, Some("already_imported".to_string()))
            } else if existing_ver.is_some() {
                (PlanAction::Skip, Some("site_conflict".to_string()))
            } else {
                requires_public_ack = true;
                (PlanAction::Create, None)
            };

            implications_set.insert("menus: public_effect_requires_ack".to_string());
            items.push(ImportPlanItem {
                resource_kind: SampleGroup::Menus,
                object_key: menu.object_key,
                logical_target: format!("menu:{}", menu.menu_key),
                action,
                reason,
                publication_implication: "public_effect_requires_ack".to_string(),
                dependencies: deps,
            });
        }
    }

    if selected_set.contains(&SampleGroup::Settings) {
        let mut sorted = sample.settings.clone();
        sorted.sort_by(|a, b| a.object_key.cmp(&b.object_key));
        for s in sorted {
            let mut deps = Vec::new();
            let mut missing = false;
            if let Some(mref) = s.media_ref.as_deref() {
                deps.push(format!("media:{mref}"));
                if !selected_set.contains(&SampleGroup::Media) {
                    missing = true;
                    unselected_dependencies.push(UnselectedDependency {
                        required_group: SampleGroup::Media,
                        object_key: mref.to_string(),
                        referenced_by_group: SampleGroup::Settings,
                        referenced_by_object: s.object_key.clone(),
                    });
                }
            }

            let exists_opt: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM theme_option_values WHERE theme_id = ? AND option_key = ?)",
            )
            .bind(theme_id)
            .bind(&s.option_key)
            .fetch_one(db)
            .await?;

            let (action, reason) = if missing {
                (
                    PlanAction::MissingDependency,
                    Some("unselected_dependency:media".to_string()),
                )
            } else if has_tombstone(
                db,
                theme_id,
                &sample.sample_key,
                SampleGroup::Settings,
                &s.object_key,
            )
            .await?
            {
                (PlanAction::Skip, Some("tombstone_exists".to_string()))
            } else if has_provenance(
                db,
                theme_id,
                &sample.sample_key,
                SampleGroup::Settings,
                &s.object_key,
            )
            .await?
            {
                (PlanAction::Skip, Some("already_imported".to_string()))
            } else if exists_opt {
                (PlanAction::Skip, Some("site_conflict".to_string()))
            } else {
                requires_public_ack = true;
                (PlanAction::Create, None)
            };

            implications_set.insert("settings: public_effect_requires_ack".to_string());
            items.push(ImportPlanItem {
                resource_kind: SampleGroup::Settings,
                object_key: s.object_key,
                logical_target: format!("option:{}", s.option_key),
                action,
                reason,
                publication_implication: "public_effect_requires_ack".to_string(),
                dependencies: deps,
            });
        }
    }

    let theme_options_version: i64 = sqlx::query_scalar(
        "SELECT COALESCE((SELECT version FROM theme_option_versions WHERE theme_id = ?), 0)",
    )
    .bind(theme_id)
    .fetch_one(db)
    .await?;

    let expected_versions = ExpectedVersions {
        theme_selection_version: selection.version,
        theme_options_version,
        menu_versions,
    };

    let publication_implications: Vec<String> = implications_set.into_iter().collect();

    let canonical_for_digest = serde_json::json!({
        "theme_id": theme_id,
        "revision_id": rev.revision_id,
        "revision_digest": rev.revision_digest,
        "sample_key": sample.sample_key,
        "selected_groups": selected_groups,
        "items": items,
        "unselected_dependencies": unselected_dependencies,
        "expected_versions": expected_versions,
        "requires_public_ack": requires_public_ack,
        "publication_implications": publication_implications,
    });
    let canonical_bytes = serde_json::to_vec(&canonical_for_digest)?;
    let digest_bytes = Sha256::digest(&canonical_bytes);
    let mut plan_digest = String::with_capacity(64);
    for b in digest_bytes {
        plan_digest.push_str(&format!("{b:02x}"));
    }

    Ok(ImportPlan {
        theme_id: theme_id.to_string(),
        revision_id: rev.revision_id,
        revision_digest: rev.revision_digest,
        sample_key: sample.sample_key,
        selected_groups,
        items,
        unselected_dependencies,
        expected_versions,
        requires_public_ack,
        publication_implications,
        plan_digest,
    })
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

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn infer_media_mime(path: &str, declared: Option<&str>) -> ApiResult<&'static str> {
    if let Some(m) = declared {
        return match m {
            "image/png" => Ok("image/png"),
            "image/jpeg" | "image/jpg" => Ok("image/jpeg"),
            "image/webp" => Ok("image/webp"),
            _ => Err(ApiError::bad_request("Định dạng media không được hỗ trợ.")),
        };
    }
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => Ok("image/png"),
        "jpg" | "jpeg" => Ok("image/jpeg"),
        "webp" => Ok("image/webp"),
        _ => Err(ApiError::bad_request("Định dạng media không được hỗ trợ.")),
    }
}

async fn write_staged_and_promote(
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

pub async fn execute_sample_import(
    db: &SqlitePool,
    themes_root: &std::path::Path,
    uploads_root: &std::path::Path,
    theme_id: &str,
    input: &ExecuteImportInput,
) -> ApiResult<ImportExecutionOutcome> {
    execute_sample_import_with_cutpoint(
        db,
        themes_root,
        uploads_root,
        theme_id,
        input,
        ImportCutpoint::None,
    )
    .await
}

pub async fn execute_sample_import_with_cutpoint(
    db: &SqlitePool,
    themes_root: &std::path::Path,
    uploads_root: &std::path::Path,
    theme_id: &str,
    input: &ExecuteImportInput,
    cutpoint: ImportCutpoint,
) -> ApiResult<ImportExecutionOutcome> {
    let dry_input = DryRunImportInput {
        groups: input.groups.clone(),
        revision_id: input.revision_id.clone(),
    };
    let plan = plan_sample_import(db, theme_id, &dry_input).await?;

    let run_id = input.run_id.clone().unwrap_or_else(|| {
        format!(
            "run-{}-{}",
            &plan.plan_digest[..12],
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        )
    });

    // Idempotent retry check for same run_id
    let existing_run: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT state, plan_digest, outcome_json FROM import_runs WHERE run_id = ?",
    )
    .bind(&run_id)
    .fetch_optional(db)
    .await?;
    if let Some((state, old_digest, outcome_json)) = existing_run {
        if state == "committed" && old_digest == input.plan_digest {
            if let Some(raw) = outcome_json {
                let outcome: ImportExecutionOutcome = serde_json::from_str(&raw)?;
                return Ok(outcome);
            }
        }
        return Err(ApiError::conflict(
            "Import run_id đã tồn tại ở trạng thái chưa hoàn tất hoặc khác kế hoạch.",
        ));
    }

    if plan.plan_digest != input.plan_digest || plan.expected_versions != input.expected_versions {
        return Err(ApiError::precondition_failed(
            "Trạng thái site đã thay đổi sau dry-run (412). Vui lòng chạy lại dry-run.",
        ));
    }
    if plan
        .items
        .iter()
        .any(|i| i.action == PlanAction::MissingDependency)
    {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Kế hoạch vẫn còn dependency chưa được chọn.".into(),
        ));
    }
    if plan.requires_public_ack && !input.acknowledge_public_changes {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Cần xác nhận thay đổi hiển thị công khai (menu/options) trước khi thực thi.".into(),
        ));
    }

    let rev = find_revision_by_id(db, &plan.revision_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Revision theme chưa được cài đặt."))?;
    let manifest: ThemeV2Manifest = serde_json::from_str(&rev.manifest_json)?;
    let sample: ThemeSampleManifest = serde_json::from_value(
        manifest
            .sample
            .ok_or_else(|| ApiError::not_found("Theme không có dữ liệu mẫu."))?,
    )?;

    // Phase 1: staged
    {
        let mut conn = db.acquire().await?;
        sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
        sqlx::query(
            "INSERT INTO import_runs (run_id, theme_id, sample_key, revision_id, plan_digest, state, media_manifest_json)
             VALUES (?, ?, ?, ?, ?, 'staged', '[]')",
        )
        .bind(&run_id)
        .bind(theme_id)
        .bind(&plan.sample_key)
        .bind(&plan.revision_id)
        .bind(&plan.plan_digest)
        .execute(&mut *conn)
        .await?;
        sqlx::query("COMMIT").execute(&mut *conn).await?;
    }

    if cutpoint == ImportCutpoint::AfterPhase1Staged {
        return Err(ApiError::internal("Simulated crash after Phase 1 (staged)."));
    }

    // Phase 2: files_verified
    let mut media_entries = Vec::new();
    let mut media_id_by_object = HashMap::<String, String>::new();
    let rev_dir = themes_root.join("revisions").join(&rev.revision_digest);

    for item in &plan.items {
        if item.resource_kind == SampleGroup::Media && item.action == PlanAction::Create {
            let m = sample
                .media
                .iter()
                .find(|x| x.object_key == item.object_key)
                .ok_or_else(|| ApiError::internal("Không tìm thấy media item trong sample."))?;
            let safe_rel = validate_v2_relative_path(&m.path).map_err(ApiError::bad_request)?;
            let mut full_path = rev_dir.clone();
            for seg in safe_rel.split('/') {
                full_path.push(seg);
            }
            let bytes = tokio::fs::read(&full_path)
                .await
                .map_err(|_| ApiError::internal("Không đọc được file media từ revision theme."))?;
            let mime = infer_media_mime(&safe_rel, m.mime_type.as_deref())?;
            let decoded = crate::media::decode_image(bytes, mime)?;
            let already_existed = write_staged_and_promote(
                uploads_root,
                &run_id,
                &decoded.id,
                decoded.extension,
                &decoded.original,
                &decoded.thumbnail,
            )
            .await?;
            let filename = safe_rel.rsplit('/').next().unwrap_or("sample.png").to_string();
            let asset = decoded.metadata(filename);
            media_id_by_object.insert(m.object_key.clone(), asset.id.clone());
            media_entries.push(StagedMediaEntry {
                object_key: m.object_key.clone(),
                media_id: asset.id.clone(),
                extension: asset.extension,
                sha256: asset.id,
                already_existed,
                filename: asset.filename,
                mime_type: asset.mime_type,
                byte_size: asset.byte_size,
                width: asset.width,
                height: asset.height,
                thumb_width: asset.thumb_width,
                thumb_height: asset.thumb_height,
            });
        }
    }

    let _ = tokio::fs::remove_dir_all(uploads_root.join(".staging").join(&run_id)).await;
    let media_manifest_json = serde_json::to_string(&media_entries)?;
    sqlx::query(
        "UPDATE import_runs
         SET state = 'files_verified', media_manifest_json = ?, updated_at = datetime('now')
         WHERE run_id = ?",
    )
    .bind(&media_manifest_json)
    .bind(&run_id)
    .execute(db)
    .await?;

    if cutpoint == ImportCutpoint::AfterPhase2FilesVerified {
        return Err(ApiError::internal(
            "Simulated crash after Phase 2 (files_verified).",
        ));
    }

    // Phase 3: committed (single BEGIN IMMEDIATE transaction)
    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;

    let tx_res: ApiResult<ImportExecutionOutcome> = async {
        let cur_sel_ver: i64 =
            sqlx::query_scalar("SELECT version FROM theme_active_selection WHERE id = 1")
                .fetch_one(&mut *conn)
                .await?;
        let cur_opt_ver: i64 = sqlx::query_scalar(
            "SELECT COALESCE((SELECT version FROM theme_option_versions WHERE theme_id = ?), 0)",
        )
        .bind(theme_id)
        .fetch_one(&mut *conn)
        .await?;
        if cur_sel_ver != input.expected_versions.theme_selection_version
            || cur_opt_ver != input.expected_versions.theme_options_version
        {
            return Err(ApiError::precondition_failed(
                "Trạng thái site đã thay đổi trước khi commit (412).",
            ));
        }

        let mut created = Vec::new();
        let mut skipped = Vec::new();
        let mut page_id_by_object = HashMap::<String, i64>::new();

        for item in &plan.items {
            if item.action == PlanAction::Skip {
                skipped.push(ImportOutcomeItem {
                    resource_kind: item.resource_kind,
                    object_key: item.object_key.clone(),
                    target_id: item.logical_target.clone(),
                    status: "skipped".to_string(),
                    reason: item.reason.clone(),
                });
            }
        }

        // Insert media_assets + provenance
        for m in &media_entries {
            sqlx::query(
                "INSERT INTO media_assets
                 (id, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&m.media_id)
            .bind(&m.filename)
            .bind(&m.mime_type)
            .bind(&m.extension)
            .bind(m.byte_size)
            .bind(m.width)
            .bind(m.height)
            .bind(m.thumb_width)
            .bind(m.thumb_height)
            .execute(&mut *conn)
            .await?;

            sqlx::query(
                "INSERT INTO import_provenance
                 (theme_id, sample_key, resource_kind, object_key, revision_id, revision_digest, target_id, target_hash)
                 VALUES (?, ?, 'media', ?, ?, ?, ?, ?)",
            )
            .bind(theme_id)
            .bind(&plan.sample_key)
            .bind(&m.object_key)
            .bind(&plan.revision_id)
            .bind(&plan.revision_digest)
            .bind(&m.media_id)
            .bind(&m.sha256)
            .execute(&mut *conn)
            .await?;

            created.push(ImportOutcomeItem {
                resource_kind: SampleGroup::Media,
                object_key: m.object_key.clone(),
                target_id: m.media_id.clone(),
                status: "created".to_string(),
                reason: None,
            });
        }

        // Insert pages (always draft) + provenance
        for item in &plan.items {
            if item.resource_kind == SampleGroup::Pages && item.action == PlanAction::Create {
                let p = sample
                    .pages
                    .iter()
                    .find(|x| x.object_key == item.object_key)
                    .ok_or_else(|| ApiError::internal("Không tìm thấy sample page."))?;
                let values_json = if p.values.is_object() {
                    serde_json::to_string(&p.values)?
                } else {
                    "{}".to_string()
                };
                crate::storage::seo::check_default_slug_claim_in_tx(&mut *conn, &p.content_type, &p.slug, None).await?;
                let entry_id: i64 = sqlx::query_scalar(
                    "INSERT INTO content_entries (content_type_key, slug, title, values_json, publication_status, version)
                     VALUES (?, ?, ?, ?, 'draft', 1) RETURNING id",
                )
                .bind(&p.content_type)
                .bind(&p.slug)
                .bind(&p.title)
                .bind(&values_json)
                .fetch_one(&mut *conn)
                .await?;

                page_id_by_object.insert(p.object_key.clone(), entry_id);
                let target_hash = sha256_hex(format!("{}:{}:draft", p.slug, p.title).as_bytes());
                sqlx::query(
                    "INSERT INTO import_provenance
                     (theme_id, sample_key, resource_kind, object_key, revision_id, revision_digest, target_id, target_hash)
                     VALUES (?, ?, 'pages', ?, ?, ?, ?, ?)",
                )
                .bind(theme_id)
                .bind(&plan.sample_key)
                .bind(&p.object_key)
                .bind(&plan.revision_id)
                .bind(&plan.revision_digest)
                .bind(entry_id.to_string())
                .bind(&target_hash)
                .execute(&mut *conn)
                .await?;

                created.push(ImportOutcomeItem {
                    resource_kind: SampleGroup::Pages,
                    object_key: p.object_key.clone(),
                    target_id: entry_id.to_string(),
                    status: "created".to_string(),
                    reason: Some("created_as_draft".to_string()),
                });
            }
        }

        // Insert menus + items + slot assignment + provenance
        for item in &plan.items {
            if item.resource_kind == SampleGroup::Menus && item.action == PlanAction::Create {
                let menu = sample
                    .menus
                    .iter()
                    .find(|x| x.object_key == item.object_key)
                    .ok_or_else(|| ApiError::internal("Không tìm thấy sample menu."))?;
                let menu_id: i64 = sqlx::query_scalar(
                    "INSERT INTO site_menus (menu_key, title, version) VALUES (?, ?, 1) RETURNING id",
                )
                .bind(&menu.menu_key)
                .bind(&menu.title)
                .fetch_one(&mut *conn)
                .await?;

                for (pos, entry) in menu.items.iter().enumerate() {
                    let target_entry_id = if entry.target_kind == "entry" {
                        let key = entry.target_object_key.as_deref().unwrap_or("");
                        page_id_by_object.get(key).copied()
                    } else {
                        None
                    };
                    sqlx::query(
                        "INSERT INTO site_menu_items (menu_id, parent_id, position, label, target_kind, target_entry_id, external_url)
                         VALUES (?, NULL, ?, ?, ?, ?, ?)",
                    )
                    .bind(menu_id)
                    .bind(pos as i64)
                    .bind(&entry.label)
                    .bind(&entry.target_kind)
                    .bind(target_entry_id)
                    .bind(&entry.url)
                    .execute(&mut *conn)
                    .await?;
                }
                if let Some(slot_key) = menu.slot_key.as_deref() {
                    sqlx::query(
                        "INSERT OR IGNORE INTO site_menu_slot_assignments (theme_id, slot_key, menu_id)
                         VALUES (?, ?, ?)",
                    )
                    .bind(theme_id)
                    .bind(slot_key)
                    .bind(menu_id)
                    .execute(&mut *conn)
                    .await?;
                    crate::storage::menu::bump_slot_version_in_tx(&mut conn, theme_id).await?;
                }
                let target_hash = sha256_hex(menu.menu_key.as_bytes());
                sqlx::query(
                    "INSERT INTO import_provenance
                     (theme_id, sample_key, resource_kind, object_key, revision_id, revision_digest, target_id, target_hash)
                     VALUES (?, ?, 'menus', ?, ?, ?, ?, ?)",
                )
                .bind(theme_id)
                .bind(&plan.sample_key)
                .bind(&menu.object_key)
                .bind(&plan.revision_id)
                .bind(&plan.revision_digest)
                .bind(menu_id.to_string())
                .bind(&target_hash)
                .execute(&mut *conn)
                .await?;

                created.push(ImportOutcomeItem {
                    resource_kind: SampleGroup::Menus,
                    object_key: menu.object_key.clone(),
                    target_id: menu_id.to_string(),
                    status: "created".to_string(),
                    reason: None,
                });
            }
        }

        // Insert settings (theme options) + provenance
        let mut any_setting_created = false;
        for item in &plan.items {
            if item.resource_kind == SampleGroup::Settings && item.action == PlanAction::Create {
                let s = sample
                    .settings
                    .iter()
                    .find(|x| x.object_key == item.object_key)
                    .ok_or_else(|| ApiError::internal("Không tìm thấy sample setting."))?;
                let val = if let Some(mref) = s.media_ref.as_deref() {
                    let mid = media_id_by_object
                        .get(mref)
                        .ok_or_else(|| ApiError::internal("Thiếu media_id cho sample setting."))?;
                    sqlx::query(
                        "INSERT INTO theme_option_media_refs (theme_id, option_key, media_id)
                         VALUES (?, ?, ?)
                         ON CONFLICT(theme_id, option_key) DO UPDATE SET media_id = excluded.media_id",
                    )
                    .bind(theme_id)
                    .bind(&s.option_key)
                    .bind(mid)
                    .execute(&mut *conn)
                    .await?;
                    serde_json::Value::String(mid.clone())
                } else {
                    s.value.clone()
                };
                let val_json = serde_json::to_string(&val)?;
                sqlx::query(
                    "INSERT INTO theme_option_values (theme_id, option_key, value_json)
                     VALUES (?, ?, ?)",
                )
                .bind(theme_id)
                .bind(&s.option_key)
                .bind(&val_json)
                .execute(&mut *conn)
                .await?;

                let target_hash = sha256_hex(val_json.as_bytes());
                sqlx::query(
                    "INSERT INTO import_provenance
                     (theme_id, sample_key, resource_kind, object_key, revision_id, revision_digest, target_id, target_hash)
                     VALUES (?, ?, 'settings', ?, ?, ?, ?, ?)",
                )
                .bind(theme_id)
                .bind(&plan.sample_key)
                .bind(&s.object_key)
                .bind(&plan.revision_id)
                .bind(&plan.revision_digest)
                .bind(&s.option_key)
                .bind(&target_hash)
                .execute(&mut *conn)
                .await?;
                any_setting_created = true;
                created.push(ImportOutcomeItem {
                    resource_kind: SampleGroup::Settings,
                    object_key: s.object_key.clone(),
                    target_id: s.option_key.clone(),
                    status: "created".to_string(),
                    reason: None,
                });
            }
        }
        if any_setting_created {
            sqlx::query(
                "INSERT INTO theme_option_versions (theme_id, version) VALUES (?, 1)
                 ON CONFLICT(theme_id) DO UPDATE SET version = version + 1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            )
            .bind(theme_id)
            .execute(&mut *conn)
            .await?;
        }

        let outcome = ImportExecutionOutcome {
            run_id: run_id.clone(),
            theme_id: theme_id.to_string(),
            revision_id: plan.revision_id.clone(),
            sample_key: plan.sample_key.clone(),
            plan_digest: plan.plan_digest.clone(),
            created,
            skipped,
        };
        let outcome_json = serde_json::to_string(&outcome)?;
        sqlx::query(
            "UPDATE import_runs
             SET state = 'committed', outcome_json = ?, updated_at = datetime('now')
             WHERE run_id = ?",
        )
        .bind(&outcome_json)
        .bind(&run_id)
        .execute(&mut *conn)
        .await?;
        Ok(outcome)
    }
    .await;

    match tx_res {
        Ok(outcome) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            Ok(outcome)
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            let _ = sqlx::query(
                "UPDATE import_runs SET state = 'failed', updated_at = datetime('now') WHERE run_id = ?",
            )
            .bind(&run_id)
            .execute(db)
            .await;
            Err(err)
        }
    }
}

/// Transitions any orphaned `staged`/`files_verified` runs to `quarantined` on startup,
/// deletes staging directories, and removes unreferenced media files only when `already_existed == false`
/// and `media_assets` has zero rows for `media_id`.
pub async fn recover_orphaned_import_runs(
    db: &SqlitePool,
    uploads_root: &std::path::Path,
) -> ApiResult<usize> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT run_id, media_manifest_json FROM import_runs WHERE state IN ('staged', 'files_verified')",
    )
    .fetch_all(db)
    .await?;

    let count = rows.len();
    for (run_id, manifest_json) in rows {
        let _ = tokio::fs::remove_dir_all(uploads_root.join(".staging").join(&run_id)).await;
        if let Ok(entries) = serde_json::from_str::<Vec<StagedMediaEntry>>(&manifest_json) {
            for entry in entries {
                if !entry.already_existed {
                    let refs: i64 =
                        sqlx::query_scalar("SELECT COUNT(*) FROM media_assets WHERE id = ?")
                            .bind(&entry.media_id)
                            .fetch_one(db)
                            .await?;
                    if refs == 0 && entry.media_id.len() >= 2 {
                        let dir = uploads_root.join("media").join(&entry.media_id[..2]);
                        let _ = tokio::fs::remove_file(
                            dir.join(format!("{}.{}", entry.media_id, entry.extension)),
                        )
                        .await;
                        let _ = tokio::fs::remove_file(
                            dir.join(format!("{}.thumb.png", entry.media_id)),
                        )
                        .await;
                    }
                }
            }
        }
        sqlx::query(
            "UPDATE import_runs SET state = 'quarantined', updated_at = datetime('now') WHERE run_id = ?",
        )
        .bind(&run_id)
        .execute(db)
        .await?;
    }
    Ok(count)
}
