use super::*;
use crate::storage::init_db;
use std::time::{SystemTime, UNIX_EPOCH};

fn item_ext(label: &str, pos: i64, url: &str) -> MenuItemInput {
    MenuItemInput {
        id: None,
        client_key: None,
        parent_id: None,
        parent_client_key: None,
        position: pos,
        label: label.to_string(),
        target_kind: MenuTargetKind::External,
        target_entry_id: None,
        target_term_id: None,
        external_url: Some(url.to_string()),
    }
}

#[tokio::test]
async fn site_menus_bounds_cycles_urls_cas_and_cross_menu_parent() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("opendash-m5-s09-t01-unit-{nanos}"));
    let db = init_db(&dir.join("cms.db")).await.unwrap();

    let mut tx = db.begin().await.unwrap();
    let m1 = create_menu_in_tx(
        &mut tx,
        &CreateMenuInput {
            menu_key: "primary".into(),
            title: "Primary Menu".into(),
            origin_provenance: Some("theme-a:sample:primary".into()),
        },
    )
    .await
    .unwrap();
    let m2 = create_menu_in_tx(
        &mut tx,
        &CreateMenuInput {
            menu_key: "footer".into(),
            title: "Footer Menu".into(),
            origin_provenance: None,
        },
    )
    .await
    .unwrap();
    let dup_origin = create_menu_in_tx(
        &mut tx,
        &CreateMenuInput {
            menu_key: "primary-2".into(),
            title: "Dup Origin".into(),
            origin_provenance: Some("theme-a:sample:primary".into()),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(dup_origin.0, axum::http::StatusCode::CONFLICT);

    // > 200 items
    let too_many: Vec<MenuItemInput> = (0..201)
        .map(|i| item_ext(&format!("Item {i}"), i, "https://example.com"))
        .collect();
    assert_eq!(
        replace_menu_items_in_tx(&mut tx, m1.id, 1, &too_many)
            .await
            .unwrap_err()
            .0,
        axum::http::StatusCode::BAD_REQUEST
    );

    // depth > 8
    let too_deep: Vec<MenuItemInput> = (1..=9)
        .map(|d| MenuItemInput {
            id: None,
            client_key: Some(format!("k{d}")),
            parent_id: None,
            parent_client_key: if d > 1 {
                Some(format!("k{}", d - 1))
            } else {
                None
            },
            position: 0,
            label: format!("Depth {d}"),
            target_kind: MenuTargetKind::External,
            target_entry_id: None,
            target_term_id: None,
            external_url: Some("https://example.com".into()),
        })
        .collect();
    assert_eq!(
        replace_menu_items_in_tx(&mut tx, m1.id, 1, &too_deep)
            .await
            .unwrap_err()
            .0,
        axum::http::StatusCode::BAD_REQUEST
    );

    // cycle
    let cycle = vec![
        MenuItemInput {
            id: None,
            client_key: Some("a".into()),
            parent_id: None,
            parent_client_key: Some("b".into()),
            position: 0,
            label: "A".into(),
            target_kind: MenuTargetKind::External,
            target_entry_id: None,
            target_term_id: None,
            external_url: Some("https://example.com".into()),
        },
        MenuItemInput {
            id: None,
            client_key: Some("b".into()),
            parent_id: None,
            parent_client_key: Some("a".into()),
            position: 0,
            label: "B".into(),
            target_kind: MenuTargetKind::External,
            target_entry_id: None,
            target_term_id: None,
            external_url: Some("https://example.com".into()),
        },
    ];
    assert_eq!(
        replace_menu_items_in_tx(&mut tx, m1.id, 1, &cycle)
            .await
            .unwrap_err()
            .0,
        axum::http::StatusCode::BAD_REQUEST
    );

    // duplicate sibling position
    let dup_pos = vec![
        item_ext("One", 0, "https://example.com/1"),
        item_ext("Two", 0, "https://example.com/2"),
    ];
    assert_eq!(
        replace_menu_items_in_tx(&mut tx, m1.id, 1, &dup_pos)
            .await
            .unwrap_err()
            .0,
        axum::http::StatusCode::BAD_REQUEST
    );

    // unsafe URLs
    for bad_url in [
        "javascript:alert(1)",
        "//evil.example.com/phish",
        "https://user:pass@example.com/secret",
        "https://example.com/\nheader",
    ] {
        assert_eq!(
            replace_menu_items_in_tx(&mut tx, m1.id, 1, &[item_ext("Bad", 0, bad_url)])
                .await
                .unwrap_err()
                .0,
            axum::http::StatusCode::BAD_REQUEST
        );
    }

    // stale CAS
    assert_eq!(
        replace_menu_items_in_tx(
            &mut tx,
            m1.id,
            99,
            &[item_ext("Ok", 0, "https://example.com")]
        )
        .await
        .unwrap_err()
        .0,
        axum::http::StatusCode::PRECONDITION_FAILED
    );

    // valid hierarchy + cross-menu parent check
    let saved = replace_menu_items_in_tx(
        &mut tx,
        m1.id,
        1,
        &[
            MenuItemInput {
                id: None,
                client_key: Some("root".into()),
                parent_id: None,
                parent_client_key: None,
                position: 0,
                label: "Docs".into(),
                target_kind: MenuTargetKind::External,
                target_entry_id: None,
                target_term_id: None,
                external_url: Some("https://example.com/docs".into()),
            },
            MenuItemInput {
                id: None,
                client_key: Some("child".into()),
                parent_id: None,
                parent_client_key: Some("root".into()),
                position: 0,
                label: "Guide".into(),
                target_kind: MenuTargetKind::External,
                target_entry_id: None,
                target_term_id: None,
                external_url: Some("https://example.com/guide".into()),
            },
        ],
    )
    .await
    .unwrap();
    assert_eq!(saved.menu.version, 2);
    assert_eq!(saved.items.len(), 2);

    // preserve stable ID on update
    let root_id = saved.items[0].id;
    let child_id = saved.items[1].id;
    let saved2 = replace_menu_items_in_tx(
        &mut tx,
        m1.id,
        2,
        &[
            MenuItemInput {
                id: Some(root_id),
                client_key: None,
                parent_id: None,
                parent_client_key: None,
                position: 0,
                label: "Docs Updated".into(),
                target_kind: MenuTargetKind::External,
                target_entry_id: None,
                target_term_id: None,
                external_url: Some("https://example.com/docs".into()),
            },
            MenuItemInput {
                id: Some(child_id),
                client_key: None,
                parent_id: Some(root_id),
                parent_client_key: None,
                position: 0,
                label: "Guide Updated".into(),
                target_kind: MenuTargetKind::External,
                target_entry_id: None,
                target_term_id: None,
                external_url: Some("https://example.com/guide".into()),
            },
        ],
    )
    .await
    .unwrap();
    assert_eq!(saved2.items[0].id, root_id);
    assert_eq!(saved2.items[1].id, child_id);

    let cross_parent = MenuItemInput {
        id: None,
        client_key: None,
        parent_id: Some(root_id),
        parent_client_key: None,
        position: 0,
        label: "Cross".into(),
        target_kind: MenuTargetKind::External,
        target_entry_id: None,
        target_term_id: None,
        external_url: Some("https://example.com/cross".into()),
    };
    assert_eq!(
        replace_menu_items_in_tx(&mut tx, m2.id, 1, &[cross_parent])
            .await
            .unwrap_err()
            .0,
        axum::http::StatusCode::BAD_REQUEST
    );

    tx.commit().await.unwrap();
    drop(db);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn menu_view_computes_normalize_path_active_bottom_up_ancestor_external_and_depth() {
    assert_eq!(normalize_menu_path("/"), "/");
    assert_eq!(normalize_menu_path("///"), "/");
    assert_eq!(normalize_menu_path("/posts/?page=2#top"), "/posts");
    assert_eq!(normalize_menu_path("/docs/guide/"), "/docs/guide");

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("opendash-menu-s02-unit-{nanos}"));
    let db = init_db(&dir.join("cms.db")).await.unwrap();

    let mut tx = db.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO content_entries (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, version)
         VALUES ('page', 'vi', 'tg_m1', 'parent-pg', 'Parent Page', '{}', 'draft', 1),
                ('page', 'vi', 'tg_m2', 'child-pg', 'Child Page', '{}', 'draft', 1)",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE content_entries SET publication_status = 'published', published_at = datetime('now')
          WHERE content_type_key = 'page' AND slug IN ('parent-pg', 'child-pg') AND locale = 'vi'",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    let p_id: i64 = sqlx::query_scalar(
        "SELECT id FROM content_entries WHERE slug = 'parent-pg' AND locale = 'vi'",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let c_id: i64 = sqlx::query_scalar(
        "SELECT id FROM content_entries WHERE slug = 'child-pg' AND locale = 'vi'",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    crate::storage::permalinks::sync_claims_in_tx(&mut tx)
        .await
        .unwrap();

    let m = create_menu_in_tx(
        &mut tx,
        &CreateMenuInput {
            menu_key: "main".into(),
            title: "Main".into(),
            origin_provenance: None,
        },
    )
    .await
    .unwrap();
    replace_menu_items_in_tx(
        &mut tx,
        m.id,
        1,
        &[
            MenuItemInput {
                id: None,
                client_key: Some("root_page".into()),
                parent_id: None,
                parent_client_key: None,
                position: 0,
                label: "Parent".into(),
                target_kind: MenuTargetKind::Entry,
                target_entry_id: Some(p_id),
                target_term_id: None,
                external_url: None,
            },
            MenuItemInput {
                id: None,
                client_key: Some("child_page".into()),
                parent_id: None,
                parent_client_key: Some("root_page".into()),
                position: 0,
                label: "Child".into(),
                target_kind: MenuTargetKind::Entry,
                target_entry_id: Some(c_id),
                target_term_id: None,
                external_url: None,
            },
            item_ext("Ext", 1, "https://example.com/out"),
        ],
    )
    .await
    .unwrap();

    let vm = resolve_menu_view_for_path_in_tx(&mut tx, m.id, Some("/child-pg/?utm=1"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(vm.items.len(), 2);
    let root_item = &vm.items[0];
    assert_eq!(root_item.depth, 1);
    assert!(!root_item.is_external);
    assert!(!root_item.is_active);
    assert!(root_item.is_ancestor);
    assert_eq!(root_item.children.len(), 1);
    assert_eq!(root_item.children[0].depth, 2);
    assert!(root_item.children[0].is_active);
    assert!(!root_item.children[0].is_ancestor);

    let ext_item = &vm.items[1];
    assert!(ext_item.is_external);
    assert!(!ext_item.is_active);
    assert!(!ext_item.is_ancestor);

    let preview_vm = resolve_menu_view_for_path_in_tx(&mut tx, m.id, None)
        .await
        .unwrap()
        .unwrap();
    assert!(!preview_vm.items[0].is_ancestor);
    assert!(!preview_vm.items[0].children[0].is_active);

    tx.commit().await.unwrap();
    drop(db);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn resolve_menus_by_keys_filters_locale_and_enforces_eight_menu_cap() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("opendash-s02-menu-cap-{nanos}"));
    let db = init_db(&dir.join("cms.db")).await.unwrap();

    sqlx::query("UPDATE site_locales_state SET multilingual_enabled = 1 WHERE id = 1")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("UPDATE site_locales SET enabled = 1 WHERE code = 'en'")
        .execute(&db)
        .await
        .unwrap();

    let vi_entry_id: i64 = sqlx::query_scalar(
        "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, locale, translation_group_id)
         VALUES('page', 'gioi-thieu', 'Giới thiệu', '{}', 'published', 'vi', 'grp-m1') RETURNING id",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    let en_entry_id: i64 = sqlx::query_scalar(
        "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, locale, translation_group_id)
         VALUES('page', 'about', 'About', '{}', 'published', 'en', 'grp-m1') RETURNING id",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO permalink_route_claims(path, target_kind, target_id, locale)
         VALUES('/gioi-thieu', 'entry', CAST(? AS TEXT), 'vi'), ('/en/about', 'entry', CAST(? AS TEXT), 'en')",
    )
    .bind(vi_entry_id)
    .bind(en_entry_id)
    .execute(&db)
    .await
    .unwrap();

    let mut tx = db.begin().await.unwrap();
    let m = create_menu_in_tx(
        &mut tx,
        &CreateMenuInput {
            menu_key: "top-nav".into(),
            title: "Top Nav".into(),
            origin_provenance: None,
        },
    )
    .await
    .unwrap();
    replace_menu_items_in_tx(
        &mut tx,
        m.id,
        m.version,
        &[
            MenuItemInput {
                id: None,
                client_key: None,
                parent_id: None,
                parent_client_key: None,
                position: 0,
                label: "VI Link".into(),
                target_kind: MenuTargetKind::Entry,
                target_entry_id: Some(vi_entry_id),
                target_term_id: None,
                external_url: None,
            },
            MenuItemInput {
                id: None,
                client_key: None,
                parent_id: None,
                parent_client_key: None,
                position: 1,
                label: "EN Link".into(),
                target_kind: MenuTargetKind::Entry,
                target_entry_id: Some(en_entry_id),
                target_term_id: None,
                external_url: None,
            },
        ],
    )
    .await
    .unwrap();

    let en_menus = resolve_menus_by_keys_for_path_in_tx(
        &mut tx,
        &["top-nav".into()],
        Some("/en/about"),
        Some("en"),
        0,
    )
    .await
    .unwrap();
    let top_en = en_menus.get("top-nav").unwrap();
    assert_eq!(top_en.items.len(), 1);
    assert_eq!(top_en.items[0].label, "EN Link");
    assert_eq!(top_en.items[0].url, "/en/about");
    assert!(top_en.items[0].is_active);

    let overflow_keys: Vec<String> = (1..=8).map(|i| format!("m-{i}")).collect();
    let err = resolve_menus_by_keys_for_path_in_tx(&mut tx, &overflow_keys, None, Some("en"), 1)
        .await
        .unwrap_err();
    assert!(format!("{err:?}").contains("8"));

    tx.commit().await.unwrap();
    drop(db);
    let _ = std::fs::remove_dir_all(&dir);
}
