use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use opendash_core::{
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
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{SqliteConnection, SqlitePool};

use crate::{
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
pub struct HydratedMediaRow {
    pub id: String,
    pub kind: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
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

fn collect_menu_slot_labels(menus_val: Option<&Value>) -> BTreeMap<String, String> {
    let mut labels = BTreeMap::new();
    let Some(val) = menus_val else {
        return labels;
    };
    let collect_arr = |arr: &[Value], out: &mut BTreeMap<String, String>| {
        for item in arr {
            if let Some(obj) = item.as_object()
                && let Some(key) = obj
                    .get("key")
                    .or_else(|| obj.get("slot_key"))
                    .and_then(Value::as_str)
                && let Some(label) = obj
                    .get("label")
                    .or_else(|| obj.get("title"))
                    .and_then(Value::as_str)
                && !label.trim().is_empty()
            {
                out.insert(key.to_owned(), label.to_owned());
            }
        }
    };
    if let Some(arr) = val.as_array() {
        collect_arr(arr, &mut labels);
    } else if let Some(obj) = val.as_object() {
        if let Some(arr) = obj.get("slots").and_then(Value::as_array) {
            collect_arr(arr, &mut labels);
        } else {
            for (k, v) in obj {
                if let Some(label) = v.as_str().filter(|s| !s.trim().is_empty()).or_else(|| {
                    v.as_object()
                        .and_then(|o| o.get("label").or_else(|| o.get("title")))
                        .and_then(Value::as_str)
                        .filter(|s| !s.trim().is_empty())
                }) {
                    labels.insert(k.clone(), label.to_owned());
                }
            }
        }
    }
    labels
}

pub async fn list_dynamic_options(
    db: &SqlitePool,
    themes_root: &Path,
    source: SelectOptionsSource,
    type_key: Option<&str>,
    locale: Option<&str>,
    actor_role: FieldRole,
) -> ApiResult<Vec<DynamicSelectOption>> {
    let mut conn = db.acquire().await?;
    list_dynamic_options_in_conn(&mut conn, themes_root, source, type_key, locale, actor_role).await
}

pub async fn list_dynamic_options_in_conn(
    conn: &mut SqliteConnection,
    themes_root: &Path,
    source: SelectOptionsSource,
    type_key: Option<&str>,
    locale: Option<&str>,
    actor_role: FieldRole,
) -> ApiResult<Vec<DynamicSelectOption>> {
    match source {
        SelectOptionsSource::Menus => {
            let rows: Vec<(String, String)> = sqlx::query_as(
                "SELECT menu_key, title FROM site_menus ORDER BY title ASC, menu_key ASC LIMIT ?",
            )
            .bind(MAX_DYNAMIC_OPTIONS as i64)
            .fetch_all(&mut *conn)
            .await?;
            Ok(rows
                .into_iter()
                .map(|(value, label)| DynamicSelectOption {
                    value,
                    label,
                    meta: None,
                })
                .collect())
        }
        SelectOptionsSource::MenuSlots => {
            let Some(active) = load_active_theme_v3_in_conn(conn, themes_root).await? else {
                return Ok(Vec::new());
            };
            let labels = collect_menu_slot_labels(active.manifest.menus.as_ref());
            let Ok(manifest_json) = serde_json::to_string(&active.manifest) else {
                return Ok(Vec::new());
            };
            let slots = extract_declared_menu_slots(&manifest_json);
            Ok(slots
                .into_iter()
                .take(MAX_DYNAMIC_OPTIONS)
                .map(|slot_key| {
                    let label = labels
                        .get(&slot_key)
                        .cloned()
                        .unwrap_or_else(|| slot_key.clone());
                    DynamicSelectOption {
                        value: slot_key,
                        label,
                        meta: None,
                    }
                })
                .collect())
        }
        SelectOptionsSource::ContactForms => {
            let rows: Vec<(i64, String, String)> = sqlx::query_as(
                "SELECT id, slug, title
                   FROM contact_forms
                  WHERE status = 'active'
                  ORDER BY title ASC, id ASC
                  LIMIT ?",
            )
            .bind(MAX_DYNAMIC_OPTIONS as i64)
            .fetch_all(&mut *conn)
            .await?;
            Ok(rows
                .into_iter()
                .map(|(id, slug, title)| DynamicSelectOption {
                    value: id.to_string(),
                    label: title,
                    meta: Some(slug),
                })
                .collect())
        }
        SelectOptionsSource::ContentTypes => {
            let rows: Vec<(String, String)> = sqlx::query_as(
                "SELECT key, definition_json
                   FROM content_types
                  WHERE provider_status = 'active' AND key != 'pattern'
                  ORDER BY key ASC
                  LIMIT ?",
            )
            .bind(MAX_DYNAMIC_OPTIONS as i64)
            .fetch_all(&mut *conn)
            .await?;
            let mut items: Vec<DynamicSelectOption> = rows
                .into_iter()
                .map(|(key, raw_def)| {
                    let label = serde_json::from_str::<Value>(&raw_def)
                        .ok()
                        .and_then(|v| v.get("label").and_then(Value::as_str).map(str::to_owned))
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or_else(|| key.clone());
                    DynamicSelectOption {
                        value: key,
                        label,
                        meta: None,
                    }
                })
                .collect();
            items.sort_by(|a, b| a.label.cmp(&b.label).then_with(|| a.value.cmp(&b.value)));
            Ok(items)
        }
        SelectOptionsSource::Taxonomies => {
            let rows: Vec<(String, String)> = sqlx::query_as(
                "SELECT key, label
                   FROM taxonomy_definitions
                  WHERE provider_status = 'active' AND declaration_state = 'declared'
                  ORDER BY label ASC, key ASC
                  LIMIT ?",
            )
            .bind(MAX_DYNAMIC_OPTIONS as i64)
            .fetch_all(&mut *conn)
            .await?;
            Ok(rows
                .into_iter()
                .map(|(value, label)| DynamicSelectOption {
                    value,
                    label,
                    meta: None,
                })
                .collect())
        }
        SelectOptionsSource::ContextFields => {
            let mut items = vec![
                DynamicSelectOption {
                    value: "title".to_owned(),
                    label: "Tiêu đề".to_owned(),
                    meta: Some("core".to_owned()),
                },
                DynamicSelectOption {
                    value: "slug".to_owned(),
                    label: "Đường dẫn (Slug)".to_owned(),
                    meta: Some("core".to_owned()),
                },
                DynamicSelectOption {
                    value: "url".to_owned(),
                    label: "URL công khai".to_owned(),
                    meta: Some("core".to_owned()),
                },
                DynamicSelectOption {
                    value: "cover_url".to_owned(),
                    label: "Ảnh đại diện (Cover)".to_owned(),
                    meta: Some("core".to_owned()),
                },
            ];
            if let Some(tk) = type_key.filter(|k| !k.is_empty() && *k != "pattern") {
                if !valid_snake_key(tk) {
                    return Err(ApiError::bad_request("Khóa loại nội dung không hợp lệ."));
                }
                if let Some(schema) = load_effective_content_schema_in_tx(conn, tk).await?
                    && schema.base.status == ProviderStatus::Active
                    && schema.base.descriptor.settings.public
                {
                    let core_keys: BTreeSet<&str> = ["title", "slug", "url", "cover_url"].into();
                    for (key, entry) in &schema.effective_fields {
                        if items.len() >= MAX_DYNAMIC_OPTIONS {
                            break;
                        }
                        if core_keys.contains(key.as_str()) {
                            continue;
                        }
                        if entry.effective_status == EffectiveFieldStatus::Active
                            && entry.definition.public_exposure == PublicExposure::Public
                            && entry.definition.min_view_role <= actor_role
                        {
                            items.push(DynamicSelectOption {
                                value: key.clone(),
                                label: entry.definition.label.clone(),
                                meta: Some(format!("field:{}", tk)),
                            });
                        }
                    }
                }
            }
            Ok(items)
        }
        SelectOptionsSource::TemplateParts => {
            let Some(active) = load_active_theme_v3_in_conn(conn, themes_root).await? else {
                return Ok(Vec::new());
            };
            Ok(active
                .manifest
                .parts
                .into_iter()
                .take(MAX_DYNAMIC_OPTIONS)
                .map(|(part_key, file)| DynamicSelectOption {
                    value: part_key.clone(),
                    label: part_key,
                    meta: Some(file),
                })
                .collect())
        }
        SelectOptionsSource::Patterns => {
            let clean_locale = locale.filter(|l| !l.trim().is_empty());
            let rows: Vec<(i64, String, String, String)> = sqlx::query_as(
                "SELECT e.id, e.title, e.slug, e.locale
                   FROM content_entries e
                   JOIN site_locales sl ON sl.code = e.locale AND sl.deleted_at IS NULL
                  WHERE e.content_type_key = 'pattern'
                    AND e.publication_status = 'draft'
                    AND e.trashed_at IS NULL
                    AND (?1 IS NULL OR e.locale = ?1)
                  ORDER BY e.id DESC
                  LIMIT ?2",
            )
            .bind(clean_locale)
            .bind(MAX_DYNAMIC_OPTIONS as i64)
            .fetch_all(&mut *conn)
            .await?;
            Ok(rows
                .into_iter()
                .map(|(id, title, _slug, loc)| DynamicSelectOption {
                    value: id.to_string(),
                    label: title,
                    meta: Some(loc),
                })
                .collect())
        }
    }
}

pub async fn resolve_entries_by_ids(
    db: &SqlitePool,
    ids: &[i64],
) -> ApiResult<Vec<ResolvedEntrySummary>> {
    let mut conn = db.acquire().await?;
    resolve_entries_by_ids_in_conn(&mut conn, ids).await
}

pub async fn resolve_entries_by_ids_in_conn(
    conn: &mut SqliteConnection,
    ids: &[i64],
) -> ApiResult<Vec<ResolvedEntrySummary>> {
    if ids.is_empty() || ids.len() > MAX_ENTRY_RESOLVE_IDS || ids.iter().any(|id| *id <= 0) {
        return Err(ApiError::bad_request("ID nội dung không hợp lệ."));
    }
    let raw_ids = serde_json::to_string(ids)?;
    let rows: Vec<(i64, String, String, String, String, String)> = sqlx::query_as(
        "SELECT e.id, e.content_type_key, e.title, e.slug, e.publication_status, e.locale
           FROM content_entries e
           JOIN content_types ct ON ct.key = e.content_type_key
           JOIN site_locales sl ON sl.code = e.locale AND sl.deleted_at IS NULL
           JOIN json_each(?1) req ON CAST(req.value AS INTEGER) = e.id
          WHERE e.trashed_at IS NULL
            AND ct.provider_status = 'active'
            AND e.content_type_key != 'pattern'
          ORDER BY e.id ASC",
    )
    .bind(raw_ids)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, content_type_key, title, slug, publication_status, locale)| {
                ResolvedEntrySummary {
                    id,
                    content_type_key,
                    title,
                    slug,
                    publication_status,
                    locale,
                }
            },
        )
        .collect())
}

pub async fn load_media_hydration_map_in_conn(
    conn: &mut SqliteConnection,
    media_ids: &BTreeSet<String>,
) -> ApiResult<BTreeMap<String, HydratedMediaRow>> {
    let ids: Vec<&str> = media_ids
        .iter()
        .filter(|id| valid_sha256_media_id(id))
        .take(MAX_HYDRATE_MEDIA_IDS)
        .map(String::as_str)
        .collect();
    let mut out = BTreeMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    for chunk in ids.chunks(HYDRATE_SQL_CHUNK) {
        let raw_json = serde_json::to_string(chunk)?;
        let rows: Vec<(String, String, Option<i64>, Option<i64>)> = sqlx::query_as(
            "SELECT id, kind, width, height
               FROM media_assets
              WHERE lifecycle_state = 'active'
                AND id IN (SELECT value FROM json_each(?1))",
        )
        .bind(raw_json)
        .fetch_all(&mut *conn)
        .await?;
        for (id, kind, width, height) in rows {
            out.insert(
                id.clone(),
                HydratedMediaRow {
                    id,
                    kind,
                    width,
                    height,
                },
            );
        }
    }
    Ok(out)
}

fn truncate_entry_excerpt(plain: &str) -> String {
    let normalized: String = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = normalized.chars();
    let head: String = chars.by_ref().take(160).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

pub async fn load_entry_ref_hydration_map_in_conn(
    conn: &mut SqliteConnection,
    entry_ids: &BTreeSet<i64>,
    is_preview: bool,
    _active_locale: Option<&str>,
) -> ApiResult<BTreeMap<i64, HydratedEntryRefRow>> {
    let ids: Vec<i64> = entry_ids
        .iter()
        .copied()
        .filter(|id| *id > 0)
        .take(MAX_HYDRATE_ENTRY_IDS)
        .collect();
    let mut out = BTreeMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    let preview_flag: i64 = i64::from(is_preview);
    let mut raw_rows: Vec<(
        i64,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
    )> = Vec::new();
    for chunk in ids.chunks(HYDRATE_SQL_CHUNK) {
        let raw_json = serde_json::to_string(chunk)?;
        let rows: Vec<(i64, String, String, String, String, Option<String>, Option<String>)> =
            sqlx::query_as(
                "SELECT e.id, e.content_type_key, e.title, e.slug, e.values_json, e.published_at, c.path
                   FROM content_entries e
                   JOIN content_types t ON t.key = e.content_type_key
                   JOIN site_locales sl ON sl.code = e.locale AND sl.deleted_at IS NULL
                   LEFT JOIN permalink_route_claims c
                     ON c.target_kind = 'entry'
                    AND c.target_id = CAST(e.id AS TEXT)
                    AND c.locale = e.locale
                  WHERE e.id IN (SELECT value FROM json_each(?1))
                    AND e.trashed_at IS NULL
                    AND t.provider_status = 'active'
                    AND e.content_type_key NOT IN ('pattern', 'section')
                    AND (?2 = 1 OR (
                        e.publication_status = 'published'
                        AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
                        AND sl.enabled = 1
                        AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                    ))",
            )
            .bind(raw_json)
            .bind(preview_flag)
            .fetch_all(&mut *conn)
            .await?;
        raw_rows.extend(rows);
    }

    let distinct_types: BTreeSet<String> = raw_rows
        .iter()
        .map(|(_, type_key, ..)| type_key.clone())
        .collect();
    let mut cover_field_by_type: BTreeMap<String, Option<String>> = BTreeMap::new();
    for type_key in distinct_types {
        let cover_key = load_effective_content_schema_in_tx(conn, &type_key)
            .await?
            .and_then(|schema| {
                schema.effective_fields.into_iter().find_map(|(k, fe)| {
                    if fe.effective_status == EffectiveFieldStatus::Active
                        && fe.definition.public_exposure == PublicExposure::Public
                        && fe.definition.min_view_role == FieldRole::Editor
                        && fe.definition.field_type == ContentFieldType::Image
                    {
                        Some(k)
                    } else {
                        None
                    }
                })
            });
        cover_field_by_type.insert(type_key, cover_key);
    }

    let mut cover_media_ids = BTreeSet::new();
    let mut parsed_rows = Vec::with_capacity(raw_rows.len());
    for (id, type_key, title, slug, values_json, published_at, claim_path) in raw_rows {
        let values =
            serde_json::from_str::<BTreeMap<String, Value>>(&values_json).unwrap_or_default();
        let candidate_cover_id = cover_field_by_type
            .get(&type_key)
            .and_then(|opt| opt.as_deref())
            .and_then(|field_key| values.get(field_key))
            .and_then(Value::as_str)
            .filter(|mid| valid_sha256_media_id(mid))
            .map(str::to_owned);
        if let Some(ref mid) = candidate_cover_id {
            cover_media_ids.insert(mid.clone());
        }
        let summary_plain = values
            .get("summary")
            .or_else(|| values.get("body"))
            .and_then(Value::as_str)
            .map(crate::content_render::strip_richtext_to_plain)
            .unwrap_or_default();
        let excerpt = truncate_entry_excerpt(&summary_plain);
        let url = match claim_path {
            Some(path) => path,
            None if is_preview => {
                format!("/api/admin/content-types/{type_key}/entries/{id}/preview")
            }
            None => format!("/{slug}"),
        };
        parsed_rows.push((
            id,
            type_key,
            title,
            slug,
            url,
            candidate_cover_id,
            excerpt,
            published_at,
        ));
    }

    let cover_media_map = load_media_hydration_map_in_conn(conn, &cover_media_ids).await?;
    let media_prefix = if is_preview {
        "/api/admin/media"
    } else {
        "/media"
    };
    for (id, type_key, title, slug, url, candidate_cover_id, excerpt, published_at) in parsed_rows {
        let cover_url = candidate_cover_id.and_then(|mid| {
            cover_media_map
                .get(&mid)
                .filter(|m| m.kind == "image")
                .map(|m| format!("{media_prefix}/{}/file", m.id))
        });
        out.insert(
            id,
            HydratedEntryRefRow {
                id,
                type_key,
                title,
                slug,
                url,
                cover_url,
                excerpt,
                published_at,
            },
        );
    }
    Ok(out)
}

pub async fn load_term_ref_hydration_map_in_conn(
    conn: &mut SqliteConnection,
    term_ids: &BTreeSet<i64>,
    is_preview: bool,
    _active_locale: Option<&str>,
) -> ApiResult<BTreeMap<i64, HydratedTermRefRow>> {
    let ids: Vec<i64> = term_ids
        .iter()
        .copied()
        .filter(|id| *id > 0)
        .take(MAX_HYDRATE_TERM_IDS)
        .collect();
    let mut out = BTreeMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    let preview_flag: i64 = i64::from(is_preview);
    for chunk in ids.chunks(HYDRATE_SQL_CHUNK) {
        let raw_json = serde_json::to_string(chunk)?;
        let rows: Vec<(i64, String, String, String, Option<String>, Option<String>)> =
            sqlx::query_as(
                "SELECT tm.id, tm.kind, tm.name, tm.slug, c.path, tm.featured_image_media_id
                   FROM taxonomy_terms tm
                   JOIN taxonomy_definitions d ON d.key = tm.kind
                   JOIN site_locales sl ON sl.code = tm.locale AND sl.deleted_at IS NULL
                   LEFT JOIN permalink_route_claims c
                     ON c.target_kind = 'term'
                    AND c.target_id = CAST(tm.id AS TEXT)
                    AND c.locale = tm.locale
                  WHERE tm.id IN (SELECT value FROM json_each(?1))
                    AND d.provider_status = 'active'
                    AND d.declaration_state = 'declared'
                    AND (?2 = 1 OR (
                        d.public = 1
                        AND sl.enabled = 1
                        AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                    ))",
            )
            .bind(raw_json)
            .bind(preview_flag)
            .fetch_all(&mut *conn)
            .await?;
        for (id, taxonomy_key, name, slug, claim_path, featured_image_media_id) in rows {
            out.insert(
                id,
                HydratedTermRefRow {
                    id,
                    taxonomy_key,
                    name,
                    slug,
                    url: claim_path.unwrap_or_default(),
                    featured_image_media_id: featured_image_media_id
                        .filter(|mid| valid_sha256_media_id(mid)),
                },
            );
        }
    }
    Ok(out)
}

async fn count_active_media_with_kind_in_conn(
    conn: &mut SqliteConnection,
    ids: &BTreeSet<String>,
    required_kind: Option<&str>,
) -> ApiResult<usize> {
    if ids.is_empty() {
        return Ok(0);
    }
    let vec_ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    let mut found = 0usize;
    for chunk in vec_ids.chunks(HYDRATE_SQL_CHUNK) {
        let raw_json = serde_json::to_string(chunk)?;
        let cnt: i64 = match required_kind {
            Some(kind) => {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM media_assets
                      WHERE lifecycle_state = 'active'
                        AND kind = ?1
                        AND id IN (SELECT value FROM json_each(?2))",
                )
                .bind(kind)
                .bind(raw_json)
                .fetch_one(&mut *conn)
                .await?
            }
            None => {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM media_assets
                      WHERE lifecycle_state = 'active'
                        AND id IN (SELECT value FROM json_each(?1))",
                )
                .bind(raw_json)
                .fetch_one(&mut *conn)
                .await?
            }
        };
        found += cnt.max(0) as usize;
    }
    Ok(found)
}

pub async fn verify_collected_media_ids_in_conn(
    conn: &mut SqliteConnection,
    collected: &CollectedMediaIds,
) -> ApiResult<()> {
    let mut any_and_file = collected.file_ids.clone();
    any_and_file.extend(collected.any_ids.iter().cloned());
    if count_active_media_with_kind_in_conn(conn, &collected.image_ids, Some("image")).await?
        != collected.image_ids.len()
        || count_active_media_with_kind_in_conn(conn, &collected.video_ids, Some("video")).await?
            != collected.video_ids.len()
        || count_active_media_with_kind_in_conn(conn, &any_and_file, None).await?
            != any_and_file.len()
    {
        return Err(ApiError::bad_request("Media không tồn tại."));
    }
    Ok(())
}

fn extract_ids_from_value(val: &Value) -> BTreeSet<i64> {
    let mut out = BTreeSet::new();
    match val {
        Value::Number(n) => {
            if let Some(id) = n.as_i64()
                && id > 0
            {
                out.insert(id);
            }
        }
        Value::Array(arr) => {
            for item in arr {
                if let Some(id) = item.as_i64()
                    && id > 0
                {
                    out.insert(id);
                }
            }
        }
        _ => {}
    }
    out
}

struct PendingEntryCheck {
    target_types: Vec<String>,
    ids: BTreeSet<i64>,
}

struct PendingTermCheck {
    taxonomy: Option<String>,
    ids: BTreeSet<i64>,
}

fn collect_new_relational_checks(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    previous_values: Option<&BTreeMap<String, Value>>,
    root_specs: &[FieldSpec],
    root_values: &BTreeMap<String, Value>,
    entry_checks: &mut Vec<PendingEntryCheck>,
    term_checks: &mut Vec<PendingTermCheck>,
) {
    for spec in specs {
        if spec.is_ui_only() {
            continue;
        }
        let Some(cur_val) = values.get(&spec.key) else {
            continue;
        };
        let prev_val = previous_values.and_then(|p| p.get(&spec.key));
        match spec.field_type {
            FieldType::EntryRef => {
                let mut cur_ids = extract_ids_from_value(cur_val);
                if let Some(pv) = prev_val {
                    for pid in extract_ids_from_value(pv) {
                        cur_ids.remove(&pid);
                    }
                }
                if !cur_ids.is_empty() {
                    entry_checks.push(PendingEntryCheck {
                        target_types: spec.target_types.clone(),
                        ids: cur_ids,
                    });
                }
            }
            FieldType::TermRef => {
                let mut cur_ids = extract_ids_from_value(cur_val);
                if let Some(pv) = prev_val {
                    for pid in extract_ids_from_value(pv) {
                        cur_ids.remove(&pid);
                    }
                }
                if !cur_ids.is_empty() {
                    let resolved_tax = if let Some(ref field_ref) = spec.taxonomy_from_field {
                        values
                            .get(field_ref)
                            .or_else(|| root_values.get(field_ref))
                            .or_else(|| {
                                specs
                                    .iter()
                                    .chain(root_specs.iter())
                                    .find(|s| &s.key == field_ref)
                                    .and_then(|s| s.default.as_ref())
                            })
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                    } else {
                        spec.taxonomy.clone()
                    };
                    term_checks.push(PendingTermCheck {
                        taxonomy: resolved_tax,
                        ids: cur_ids,
                    });
                }
            }
            FieldType::Group => {
                if let Some(cur_obj) = cur_val.as_object() {
                    let cur_map: BTreeMap<String, Value> = cur_obj
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    let prev_map: Option<BTreeMap<String, Value>> = prev_val
                        .and_then(Value::as_object)
                        .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
                    collect_new_relational_checks(
                        &spec.fields,
                        &cur_map,
                        prev_map.as_ref(),
                        root_specs,
                        root_values,
                        entry_checks,
                        term_checks,
                    );
                }
            }
            FieldType::Repeater => {
                let mut prev_entry_pool = BTreeSet::new();
                let mut prev_term_pool = BTreeSet::new();
                if let Some(pv) = prev_val {
                    let mut dummy_prev = BTreeMap::new();
                    dummy_prev.insert(spec.key.clone(), pv.clone());
                    let rel = opendash_core::field_schema::collect_relational_refs(
                        std::slice::from_ref(spec),
                        &dummy_prev,
                    );
                    prev_entry_pool = rel.entry_ids;
                    prev_term_pool = rel.term_ids;
                }
                if let Some(items) = cur_val.as_array() {
                    for item in items {
                        if let Some(obj) = item.as_object() {
                            let item_map: BTreeMap<String, Value> =
                                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                            let before_entries = entry_checks.len();
                            let before_terms = term_checks.len();
                            collect_new_relational_checks(
                                &spec.fields,
                                &item_map,
                                None,
                                root_specs,
                                root_values,
                                entry_checks,
                                term_checks,
                            );
                            for check in &mut entry_checks[before_entries..] {
                                check.ids.retain(|id| !prev_entry_pool.contains(id));
                            }
                            for check in &mut term_checks[before_terms..] {
                                check.ids.retain(|id| !prev_term_pool.contains(id));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

pub async fn validate_relational_refs_in_conn(
    conn: &mut SqliteConnection,
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    previous_values: Option<&BTreeMap<String, Value>>,
    required_locale: Option<&str>,
) -> ApiResult<()> {
    let mut entry_checks = Vec::new();
    let mut term_checks = Vec::new();
    collect_new_relational_checks(
        specs,
        values,
        previous_values,
        specs,
        values,
        &mut entry_checks,
        &mut term_checks,
    );
    entry_checks.retain(|c| !c.ids.is_empty());
    term_checks.retain(|c| !c.ids.is_empty());

    let all_entry_ids: BTreeSet<i64> = entry_checks
        .iter()
        .flat_map(|c| c.ids.iter().copied())
        .collect();
    if !all_entry_ids.is_empty() {
        let mut valid_entries: BTreeMap<i64, (String, String)> = BTreeMap::new();
        let ids_vec: Vec<i64> = all_entry_ids.into_iter().collect();
        for chunk in ids_vec.chunks(HYDRATE_SQL_CHUNK) {
            let raw_json = serde_json::to_string(chunk)?;
            let rows: Vec<(i64, String, String)> = sqlx::query_as(
                "SELECT e.id, e.content_type_key, e.locale
                   FROM content_entries e
                   JOIN content_types ct ON ct.key = e.content_type_key
                   JOIN site_locales sl ON sl.code = e.locale AND sl.deleted_at IS NULL
                  WHERE e.id IN (SELECT value FROM json_each(?1))
                    AND e.trashed_at IS NULL
                    AND ct.provider_status = 'active'
                    AND e.content_type_key NOT IN ('pattern', 'section')",
            )
            .bind(raw_json)
            .fetch_all(&mut *conn)
            .await?;
            for (eid, tk, loc) in rows {
                valid_entries.insert(eid, (tk, loc));
            }
        }
        for check in &entry_checks {
            let is_wildcard = check.target_types.len() == 1 && check.target_types[0] == "*";
            for id in &check.ids {
                let Some((tk, loc)) = valid_entries.get(id) else {
                    return Err(ApiError::bad_request("ID nội dung không hợp lệ."));
                };
                if !is_wildcard && !check.target_types.iter().any(|t| t == tk) {
                    return Err(ApiError::bad_request("ID nội dung không hợp lệ."));
                }
                if let Some(req_loc) = required_locale
                    && loc != req_loc
                {
                    return Err(ApiError::bad_request("ID nội dung không hợp lệ."));
                }
            }
        }
    }

    let all_term_ids: BTreeSet<i64> = term_checks
        .iter()
        .flat_map(|c| c.ids.iter().copied())
        .collect();
    if !all_term_ids.is_empty() {
        for check in &term_checks {
            if check.taxonomy.is_none() {
                return Err(ApiError::bad_request("term_id không hợp lệ."));
            }
        }
        let mut valid_terms: BTreeMap<i64, (String, String)> = BTreeMap::new();
        let ids_vec: Vec<i64> = all_term_ids.into_iter().collect();
        for chunk in ids_vec.chunks(HYDRATE_SQL_CHUNK) {
            let raw_json = serde_json::to_string(chunk)?;
            let rows: Vec<(i64, String, String)> = sqlx::query_as(
                "SELECT tm.id, tm.kind, tm.locale
                   FROM taxonomy_terms tm
                   JOIN taxonomy_definitions d ON d.key = tm.kind
                   JOIN site_locales sl ON sl.code = tm.locale AND sl.deleted_at IS NULL
                  WHERE tm.id IN (SELECT value FROM json_each(?1))
                    AND d.provider_status = 'active'
                    AND d.declaration_state = 'declared'",
            )
            .bind(raw_json)
            .fetch_all(&mut *conn)
            .await?;
            for (tid, kind, loc) in rows {
                valid_terms.insert(tid, (kind, loc));
            }
        }
        for check in &term_checks {
            let Some(ref expected_tax) = check.taxonomy else {
                return Err(ApiError::bad_request("term_id không hợp lệ."));
            };
            for id in &check.ids {
                let Some((kind, loc)) = valid_terms.get(id) else {
                    return Err(ApiError::bad_request("term_id không hợp lệ."));
                };
                if kind != expected_tax {
                    return Err(ApiError::bad_request("term_id không hợp lệ."));
                }
                if let Some(req_loc) = required_locale
                    && loc != req_loc
                {
                    return Err(ApiError::bad_request("term_id không hợp lệ."));
                }
            }
        }
    }
    Ok(())
}

pub async fn load_active_v3_manifest_in_conn(
    conn: &mut SqliteConnection,
) -> ApiResult<Option<ThemeV3Manifest>> {
    if let Some(dev) = DevOverlayConfig::global()
        && dev.dev_theme_root().is_some()
        && let Ok(Some(snap)) = dev.load_dev_theme()
    {
        return Ok(Some(snap.manifest));
    }

    let manifest_json: Option<String> = sqlx::query_scalar(
        "SELECT r.manifest_json
           FROM theme_active_selection a
           JOIN theme_revisions r ON r.revision_id = a.revision_id
          WHERE a.id = 1 AND r.source_format = 'v3'",
    )
    .fetch_optional(&mut *conn)
    .await?;
    let Some(raw) = manifest_json else {
        return Ok(None);
    };
    let Ok(manifest) = serde_json::from_str::<ThemeV3Manifest>(&raw) else {
        return Ok(None);
    };
    Ok(Some(manifest))
}

fn collect_all_options_sources_with_paths(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    prefix: Option<&str>,
    out: &mut Vec<(String, SelectOptionsSource, Vec<String>)>,
) {
    for spec in specs {
        if spec.is_ui_only() || spec.field_type == FieldType::Secret {
            continue;
        }
        let path = match prefix {
            Some(p) => format!("{p}.{}", spec.key),
            None => spec.key.clone(),
        };
        match spec.field_type {
            FieldType::Select => {
                if let Some(src) = spec.options_source {
                    let mut selected = Vec::new();
                    match values.get(spec.key.as_str()).or(spec.default.as_ref()) {
                        Some(Value::String(s)) => {
                            let trimmed = s.trim();
                            if !trimmed.is_empty() {
                                selected.push(trimmed.to_owned());
                            }
                        }
                        Some(Value::Array(arr)) if spec.multiple => {
                            for item in arr {
                                if let Some(s) = item.as_str() {
                                    let trimmed = s.trim();
                                    if !trimmed.is_empty() {
                                        selected.push(trimmed.to_owned());
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    if !selected.is_empty() {
                        out.push((path, src, selected));
                    }
                }
            }
            FieldType::Group => {
                let child_map: BTreeMap<String, Value> = values
                    .get(spec.key.as_str())
                    .and_then(Value::as_object)
                    .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .unwrap_or_default();
                collect_all_options_sources_with_paths(&spec.fields, &child_map, Some(&path), out);
            }
            _ => {}
        }
    }
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

pub async fn validate_document_block_refs_in_conn(
    conn: &mut SqliteConnection,
    document: &BuilderDocument,
    previous: Option<&BuilderDocument>,
    catalog: &RegistryCatalog<RegisteredBlockSchema>,
    required_locale: Option<&str>,
    allowed_context_fields: Option<&[String]>,
) -> ApiResult<()> {
    let mut prev_nodes: BTreeMap<String, (&str, &BTreeMap<String, Value>)> = BTreeMap::new();
    if let Some(prev_doc) = previous {
        for section in &prev_doc.sections {
            for row in &section.children {
                for column in &row.children {
                    for element in &column.children {
                        if let ElementContent::Known(KnownElement::PluginBlock(props)) =
                            &element.content
                        {
                            prev_nodes.insert(
                                element.id.clone(),
                                (props.block_id.as_str(), &props.values),
                            );
                        }
                    }
                }
            }
        }
    }

    for section in &document.sections {
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    let ElementContent::Known(KnownElement::PluginBlock(props)) = &element.content
                    else {
                        continue;
                    };
                    let Some(entry) = catalog.get(&props.block_id) else {
                        continue;
                    };
                    if entry.status != ProviderStatus::Active
                        || entry.declaration_state != DeclarationState::Declared
                    {
                        continue;
                    }
                    let prev_vals =
                        prev_nodes
                            .get(&element.id)
                            .and_then(|(prev_block_id, vals)| {
                                (*prev_block_id == props.block_id.as_str()).then_some(*vals)
                            });
                    validate_relational_refs_in_conn(
                        conn,
                        &entry.descriptor.schema.fields,
                        &props.values,
                        prev_vals,
                        required_locale,
                    )
                    .await?;
                    validate_block_dynamic_options_in_tx(
                        conn,
                        &entry.descriptor.schema.fields,
                        &props.values,
                        prev_vals,
                        allowed_context_fields,
                    )
                    .await?;
                }
            }
        }
    }
    Ok(())
}

pub async fn validate_comment_block_refs_in_conn(
    conn: &mut SqliteConnection,
    raw_body: &str,
    previous_body: Option<&str>,
    catalog: &RegistryCatalog<RegisteredBlockSchema>,
    required_locale: Option<&str>,
    allowed_context_fields: Option<&[String]>,
) -> ApiResult<()> {
    let doc = parse_blocks_lenient_or_recover(raw_body).document;
    let prev_doc = previous_body.map(|b| parse_blocks_lenient_or_recover(b).document);
    validate_document_block_refs_in_conn(
        conn,
        &doc,
        prev_doc.as_ref(),
        catalog,
        required_locale,
        allowed_context_fields,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(prefix: &str) -> Self {
            static SEQ: AtomicU64 = AtomicU64::new(1);
            let dir = std::env::temp_dir().join(format!(
                "opendash-fields-{prefix}-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn lists_all_8_options_sources_and_enforces_visibility_filters() {
        let tmp = TempDir::new("storage-8-sources");
        let db_path = tmp.0.join("database/opendash.sqlite");
        let themes_root = tmp.0.join("themes");
        fs::create_dir_all(&themes_root).unwrap();
        let db = crate::storage::init_db(&db_path).await.unwrap();
        crate::storage::theme::ensure_default_starter_theme(&db, &themes_root)
            .await
            .unwrap();

        // Seed menus & active contact form
        sqlx::query(
            "INSERT INTO site_menus (menu_key, title) VALUES ('primary_nav', 'Primary Navigation')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO contact_forms (slug, title, status, schema_json) VALUES ('support', 'Hỗ trợ', 'active', '[]'), ('archived', 'Cũ', 'disabled', '[]')")
            .execute(&db)
            .await
            .unwrap();

        // Seed active CPT with active/public, admin-only, private, and withdrawn fields, plus inactive CPT
        let active_def = serde_json::json!({
            "key": "product",
            "label": "Sản phẩm",
            "fields": [
                {
                    "key": "price",
                    "label": "Giá bán",
                    "type": "short_text",
                    "min_view_role": "editor",
                    "min_edit_role": "editor",
                    "public_exposure": "public",
                    "declaration_state": "declared"
                },
                {
                    "key": "cost",
                    "label": "Giá vốn",
                    "type": "short_text",
                    "min_view_role": "admin",
                    "min_edit_role": "admin",
                    "public_exposure": "private",
                    "declaration_state": "declared"
                },
                {
                    "key": "internal_note",
                    "label": "Ghi chú nội bộ",
                    "type": "short_text",
                    "min_view_role": "editor",
                    "min_edit_role": "editor",
                    "public_exposure": "private",
                    "declaration_state": "declared"
                }
            ],
            "settings": { "public": true, "has_archive": true, "publication": "draft_publish", "supports": ["editor"] }
        });
        sqlx::query(
            "INSERT INTO content_types (key, definition_json, owner_kind, provider_status)
             VALUES ('product', ?, 'user', 'active'),
                    ('hidden_cpt', '{\"key\":\"hidden_cpt\",\"label\":\"Hidden\",\"fields\":[]}', 'user', 'provider_inactive')",
        )
        .bind(active_def.to_string())
        .execute(&db)
        .await
        .unwrap();

        // Seed soft-deleted locale and patterns (one valid draft, one deleted locale, one trashed)
        let default_locale: String =
            sqlx::query_scalar("SELECT code FROM site_locales WHERE is_default = 1 LIMIT 1")
                .fetch_one(&db)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO site_locales (code, bcp47_tag, label, direction, is_default, enabled, position, deleted_at)
             VALUES ('fr', 'fr', 'Français', 'ltr', 0, 0, 9, '2026-01-01T00:00:00Z')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO content_entries (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, content_mode, trashed_at)
             VALUES ('pattern', ?1, 'tg1', 'hero-pat', 'Hero Pattern', '{}', 'draft', 'builder', NULL),
                    ('pattern', 'fr', 'tg2', 'deleted-loc', 'Deleted Loc Pat', '{}', 'draft', 'builder', NULL),
                    ('pattern', ?1, 'tg3', 'trashed-pat', 'Trashed Pat', '{}', 'draft', 'builder', '2026-01-01T00:00:00Z')",
        )
        .bind(&default_locale)
        .execute(&db)
        .await
        .unwrap();

        let menus = list_dynamic_options(
            &db,
            &themes_root,
            SelectOptionsSource::Menus,
            None,
            None,
            FieldRole::Editor,
        )
        .await
        .unwrap();
        assert!(menus.iter().any(|m| m.value == "primary_nav"));

        let slots = list_dynamic_options(
            &db,
            &themes_root,
            SelectOptionsSource::MenuSlots,
            None,
            None,
            FieldRole::Editor,
        )
        .await
        .unwrap();
        assert!(!slots.is_empty());

        let forms = list_dynamic_options(
            &db,
            &themes_root,
            SelectOptionsSource::ContactForms,
            None,
            None,
            FieldRole::Editor,
        )
        .await
        .unwrap();
        assert_eq!(forms.len(), 1);
        assert_eq!(forms[0].label, "Hỗ trợ");

        let cpts = list_dynamic_options(
            &db,
            &themes_root,
            SelectOptionsSource::ContentTypes,
            None,
            None,
            FieldRole::Editor,
        )
        .await
        .unwrap();
        assert!(cpts.iter().any(|c| c.value == "product"));
        assert!(!cpts.iter().any(|c| c.value == "pattern"));
        assert!(!cpts.iter().any(|c| c.value == "hidden_cpt"));

        let taxes = list_dynamic_options(
            &db,
            &themes_root,
            SelectOptionsSource::Taxonomies,
            None,
            None,
            FieldRole::Editor,
        )
        .await
        .unwrap();
        assert!(taxes.iter().any(|t| t.value == "category"));
        assert!(taxes.iter().any(|t| t.value == "tag"));

        let ctx_fields = list_dynamic_options(
            &db,
            &themes_root,
            SelectOptionsSource::ContextFields,
            Some("product"),
            None,
            FieldRole::Editor,
        )
        .await
        .unwrap();
        let ctx_keys: Vec<&str> = ctx_fields.iter().map(|i| i.value.as_str()).collect();
        assert!(ctx_keys.contains(&"title"));
        assert!(ctx_keys.contains(&"price"));
        assert!(!ctx_keys.contains(&"cost"));
        assert!(!ctx_keys.contains(&"internal_note"));

        let parts = list_dynamic_options(
            &db,
            &themes_root,
            SelectOptionsSource::TemplateParts,
            None,
            None,
            FieldRole::Editor,
        )
        .await
        .unwrap();
        assert!(
            parts
                .iter()
                .any(|p| p.value == "header" || p.value == "footer")
        );

        let pats = list_dynamic_options(
            &db,
            &themes_root,
            SelectOptionsSource::Patterns,
            None,
            Some(&default_locale),
            FieldRole::Editor,
        )
        .await
        .unwrap();
        assert_eq!(pats.len(), 1);
        assert_eq!(pats[0].label, "Hero Pattern");
    }

    #[tokio::test]
    async fn resolve_entries_excludes_trashed_pattern_inactive_and_deleted_locale() {
        let tmp = TempDir::new("resolve-entries");
        let db_path = tmp.0.join("database/opendash.sqlite");
        let db = crate::storage::init_db(&db_path).await.unwrap();
        let default_locale: String =
            sqlx::query_scalar("SELECT code FROM site_locales WHERE is_default = 1 LIMIT 1")
                .fetch_one(&db)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO content_types (key, definition_json, owner_kind, provider_status)
             VALUES ('dead_cpt', '{\"key\":\"dead_cpt\",\"label\":\"Dead\",\"fields\":[]}', 'user', 'provider_inactive')",
        )
        .execute(&db)
        .await
        .unwrap();

        let post_id: i64 = sqlx::query_scalar(
            "INSERT INTO content_entries (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status)
             VALUES ('post', ?1, 't1', 'hello-post', 'Hello Post', '{}', 'draft') RETURNING id",
        )
        .bind(&default_locale)
        .fetch_one(&db)
        .await
        .unwrap();
        sqlx::query("UPDATE content_entries SET publication_status = 'published' WHERE id = ?")
            .bind(post_id)
            .execute(&db)
            .await
            .unwrap();
        let trashed_id: i64 = sqlx::query_scalar(
            "INSERT INTO content_entries (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, trashed_at)
             VALUES ('post', ?1, 't2', 'trashed-post', 'Trashed', '{}', 'draft', '2026-01-01T00:00:00Z') RETURNING id",
        )
        .bind(&default_locale)
        .fetch_one(&db)
        .await
        .unwrap();
        let pattern_id: i64 = sqlx::query_scalar(
            "INSERT INTO content_entries (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, content_mode)
             VALUES ('pattern', ?1, 't3', 'pat', 'Pattern', '{}', 'draft', 'builder') RETURNING id",
        )
        .bind(&default_locale)
        .fetch_one(&db)
        .await
        .unwrap();
        let dead_cpt_id: i64 = sqlx::query_scalar(
            "INSERT INTO content_entries (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status)
             VALUES ('dead_cpt', ?1, 't4', 'dead', 'Dead', '{}', 'published') RETURNING id",
        )
        .bind(&default_locale)
        .fetch_one(&db)
        .await
        .unwrap();

        let resolved = resolve_entries_by_ids(&db, &[post_id, trashed_id, pattern_id, dead_cpt_id])
            .await
            .unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].id, post_id);
        assert_eq!(resolved[0].title, "Hello Post");

        let too_many: Vec<i64> = (1..=51).collect();
        assert!(resolve_entries_by_ids(&db, &too_many).await.is_err());
    }
}
