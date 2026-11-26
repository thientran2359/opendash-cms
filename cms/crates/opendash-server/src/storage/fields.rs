use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use opendash_core::{
    content::{EffectiveFieldStatus, FieldRole, PublicExposure},
    field_schema::SelectOptionsSource,
    id::valid_snake_key,
    registry::ProviderStatus,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{SqliteConnection, SqlitePool};

use crate::{
    error::{ApiError, ApiResult},
    storage::{
        content::load_effective_content_schema_in_tx, menu::extract_declared_menu_slots,
        theme::load_active_theme_v3_in_conn,
    },
};

pub const MAX_DYNAMIC_OPTIONS: usize = 200;
pub const MAX_ENTRY_RESOLVE_IDS: usize = 50;

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
