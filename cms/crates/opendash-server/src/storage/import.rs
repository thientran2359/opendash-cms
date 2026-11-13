//! Selective sample import dry-run planner and logical provenance contract (M5-S05-T01).
//! Dry-run is strictly DB and filesystem read-only.

use std::collections::{BTreeMap, BTreeSet, HashSet};

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
