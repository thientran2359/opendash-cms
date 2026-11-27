use super::*;

pub(super) use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(super) use opendash_core::{
    block_schema::RegisteredBlockSchema,
    builder::{BuilderDocument, ElementContent, KnownElement},
    builder_blocks::parse_blocks_lenient_or_recover,
    content::{ContentFieldType, EffectiveFieldStatus, FieldRole, PublicExposure},
    field_schema::{
        CollectedMediaIds, FieldSpec, FieldType, SelectOptionsSource, valid_sha256_media_id,
    },
    id::valid_snake_key,
    registry::{DeclarationState, ProviderStatus, RegistryCatalog},
    theme::ThemeV3Manifest,
};

pub(super) use serde::{Deserialize, Serialize};

pub(super) use serde_json::Value;

pub(super) use sqlx::{SqliteConnection, SqlitePool};

pub(super) use crate::{
    dev_mode::DevOverlayConfig,
    error::{ApiError, ApiResult},
    storage::{
        content::load_effective_content_schema_in_tx, menu::extract_declared_menu_slots,
        theme::load_active_theme_v3_in_conn,
    },
};

pub const MAX_DYNAMIC_OPTIONS: usize = 200;

pub const MAX_ENTRY_RESOLVE_IDS: usize = 50;

pub const MAX_HYDRATE_MEDIA_IDS: usize = 512;

pub const MAX_HYDRATE_ENTRY_IDS: usize = 128;

pub const MAX_HYDRATE_TERM_IDS: usize = 128;

pub const HYDRATE_SQL_CHUNK: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydratedMediaVariantRow {
    pub spec: opendash_core::media::MediaVariantSpec,
    pub format: opendash_core::media::MediaVariantFormat,
    pub width: u32,
    pub height: u32,
    pub byte_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydratedMediaRow {
    pub id: String,
    pub kind: String,
    pub mime_type: String,
    pub version: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub variants: Vec<HydratedMediaVariantRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydratedEntryRefRow {
    pub id: i64,
    pub type_key: String,
    pub title: String,
    pub slug: String,
    pub url: String,
    pub cover_url: Option<String>,
    pub excerpt: String,
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydratedTermRefRow {
    pub id: i64,
    pub taxonomy_key: String,
    pub name: String,
    pub slug: String,
    pub url: String,
    pub featured_image_media_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DynamicSelectOption {
    pub value: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionsSourceResponse {
    pub source: SelectOptionsSource,
    pub items: Vec<DynamicSelectOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedEntrySummary {
    pub id: i64,
    pub content_type_key: String,
    pub title: String,
    pub slug: String,
    pub publication_status: String,
    pub locale: String,
}

pub(in crate::storage) struct PendingEntryCheck {
    pub(in crate::storage) target_types: Vec<String>,
    pub(in crate::storage) ids: BTreeSet<i64>,
}

pub(in crate::storage) struct PendingTermCheck {
    pub(in crate::storage) taxonomy: Option<String>,
    pub(in crate::storage) ids: BTreeSet<i64>,
}

pub async fn validate_block_dynamic_options_in_tx(
    conn: &mut SqliteConnection,
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    previous_values: Option<&BTreeMap<String, Value>>,
    allowed_context_fields: Option<&[String]>,
) -> ApiResult<()> {
    let mut current = Vec::new();
    collect_all_options_sources_with_paths(specs, values, None, &mut current);
    if current.is_empty() {
        return Ok(());
    }

    let mut prev_map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    if let Some(prev) = previous_values {
        let mut prev_entries = Vec::new();
        collect_all_options_sources_with_paths(specs, prev, None, &mut prev_entries);
        for (path, _src, vals) in prev_entries {
            prev_map.entry(path).or_default().extend(vals);
        }
    }

    let mut manifest_loaded = false;
    let mut active_manifest: Option<ThemeV3Manifest> = None;

    for (field_path, source, selected_values) in current {
        let prev_set = prev_map.get(&field_path);
        let new_values: Vec<String> = selected_values
            .into_iter()
            .filter(|v| !prev_set.is_some_and(|s| s.contains(v)))
            .collect();
        if new_values.is_empty() {
            continue;
        }

        if matches!(
            source,
            SelectOptionsSource::MenuSlots | SelectOptionsSource::TemplateParts
        ) && !manifest_loaded
        {
            active_manifest = load_active_v3_manifest_in_conn(conn).await?;
            manifest_loaded = true;
        }

        match source {
            SelectOptionsSource::Menus => {
                for val in new_values {
                    let exists: i64 = sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM site_menus WHERE menu_key = ?)",
                    )
                    .bind(&val)
                    .fetch_one(&mut *conn)
                    .await?;
                    if exists == 0 {
                        return Err(ApiError::bad_request("Menu tham chiếu không tồn tại."));
                    }
                }
            }
            SelectOptionsSource::MenuSlots => {
                let declared_slots: BTreeSet<String> = match active_manifest.as_ref() {
                    Some(manifest) => serde_json::to_string(manifest)
                        .ok()
                        .map(|raw| extract_declared_menu_slots(&raw))
                        .unwrap_or_default()
                        .into_iter()
                        .collect(),
                    None => BTreeSet::new(),
                };
                for val in new_values {
                    if !declared_slots.contains(&val) {
                        return Err(ApiError::bad_request(
                            "Vị trí menu không được khai báo trong theme đang kích hoạt.",
                        ));
                    }
                }
            }
            SelectOptionsSource::ContactForms => {
                for val in new_values {
                    let Ok(form_id) = val.parse::<i64>() else {
                        return Err(ApiError::bad_request(
                            "ID form liên hệ tham chiếu không hợp lệ.",
                        ));
                    };
                    if form_id <= 0 {
                        return Err(ApiError::bad_request(
                            "ID form liên hệ tham chiếu không hợp lệ.",
                        ));
                    }
                    let exists: i64 = sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM contact_forms WHERE id = ? AND status = 'active')",
                    )
                    .bind(form_id)
                    .fetch_one(&mut *conn)
                    .await?;
                    if exists == 0 {
                        return Err(ApiError::bad_request(
                            "Form liên hệ tham chiếu không tồn tại hoặc chưa kích hoạt.",
                        ));
                    }
                }
            }
            SelectOptionsSource::ContentTypes => {
                for val in new_values {
                    if val == "pattern" {
                        return Err(ApiError::bad_request(
                            "Loại nội dung tham chiếu không hợp lệ.",
                        ));
                    }
                    let exists: i64 = sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM content_types WHERE key = ? AND provider_status = 'active' AND key != 'pattern')",
                    )
                    .bind(&val)
                    .fetch_one(&mut *conn)
                    .await?;
                    if exists == 0 {
                        return Err(ApiError::bad_request(
                            "Loại nội dung tham chiếu không tồn tại hoặc không hoạt động.",
                        ));
                    }
                }
            }
            SelectOptionsSource::Taxonomies => {
                for val in new_values {
                    let exists: i64 = sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM taxonomy_definitions WHERE key = ? AND provider_status = 'active' AND declaration_state = 'declared')",
                    )
                    .bind(&val)
                    .fetch_one(&mut *conn)
                    .await?;
                    if exists == 0 {
                        return Err(ApiError::bad_request(
                            "Phân loại tham chiếu không tồn tại hoặc không hoạt động.",
                        ));
                    }
                }
            }
            SelectOptionsSource::ContextFields => {
                const CORE_CONTEXT_FIELDS: &[&str] = &["title", "slug", "url", "cover_url"];
                for val in new_values {
                    let is_core = CORE_CONTEXT_FIELDS.contains(&val.as_str());
                    let is_custom =
                        allowed_context_fields.is_some_and(|list| list.iter().any(|k| k == &val));
                    if !is_core && !is_custom {
                        return Err(ApiError::bad_request(
                            "Trường ngữ cảnh (context field) tham chiếu không hợp lệ.",
                        ));
                    }
                }
            }
            SelectOptionsSource::TemplateParts => {
                for val in new_values {
                    let in_manifest = active_manifest
                        .as_ref()
                        .is_some_and(|m| m.parts.contains_key(&val));
                    let in_overrides = if in_manifest {
                        true
                    } else {
                        let canonical = format!("part:{val}");
                        let exists: i64 = sqlx::query_scalar(
                            "SELECT EXISTS(SELECT 1 FROM template_overrides WHERE target_kind = 'part' AND canonical_target_key = ?)",
                        )
                        .bind(canonical)
                        .fetch_one(&mut *conn)
                        .await?;
                        exists != 0
                    };
                    if !in_overrides {
                        return Err(ApiError::bad_request(
                            "Template part tham chiếu không tồn tại.",
                        ));
                    }
                }
            }
            SelectOptionsSource::Patterns => {
                for val in new_values {
                    let Ok(pid) = val.parse::<i64>() else {
                        return Err(ApiError::bad_request("ID Pattern tham chiếu không hợp lệ."));
                    };
                    if pid <= 0 {
                        return Err(ApiError::bad_request("ID Pattern tham chiếu không hợp lệ."));
                    }
                    let exists: i64 = sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM content_entries WHERE id = ? AND content_type_key = 'pattern' AND trashed_at IS NULL)",
                    )
                    .bind(pid)
                    .fetch_one(&mut *conn)
                    .await?;
                    if exists == 0 {
                        return Err(ApiError::bad_request("Pattern tham chiếu không tồn tại."));
                    }
                }
            }
        }
    }

    Ok(())
}
