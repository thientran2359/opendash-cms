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
                "type": "text",
                "acl": {"min_view_role": "editor", "min_edit_role": "editor", "public_exposure": "public"},
                "declaration_state": "declared"
            },
            {
                "key": "cost",
                "label": "Giá vốn",
                "type": "text",
                "acl": {"min_view_role": "admin", "min_edit_role": "admin", "public_exposure": "private"},
                "declaration_state": "declared"
            },
            {
                "key": "internal_note",
                "label": "Ghi chú nội bộ",
                "type": "text",
                "acl": {"min_view_role": "editor", "min_edit_role": "editor", "public_exposure": "private"},
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
