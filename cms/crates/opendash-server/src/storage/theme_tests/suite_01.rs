use super::*;

#[tokio::test]
async fn migration_0042_upgrades_legacy_schema_and_preserves_deleted_reading_reference() {
    let tmp = TempDirGuard::new("migration");
    let path = tmp.path().join("site.sqlite");
    {
        let db = connect_file_pool(&path).await;
        let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        let mut migrator = sqlx::migrate::Migrator::new(migrations).await.unwrap();
        migrator.migrations.to_mut().retain(|m| m.version <= 41);
        migrator.run(&db).await.unwrap();
        sqlx::query("INSERT INTO template_overrides (role, ast_json) VALUES ('header', ?)")
            .bind(serde_json::to_string(&BuilderDocument::empty()).unwrap())
            .execute(&db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO theme_packages (theme_id,display_name,source_format,is_legacy)
             VALUES ('old-site-theme','Legacy','v2',0)",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO theme_revisions (
                revision_id,theme_id,version,revision_digest,source_format,
                manifest_json,file_digests_json,provenance_json
             ) VALUES ('old-site-rev','old-site-theme','2.0.0','digest-v2','v2','{}','{}','{}')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO theme_active_selection (id,revision_id,version)
             VALUES (1,'old-site-rev',17)
             ON CONFLICT(id) DO UPDATE SET revision_id=excluded.revision_id,version=17",
        )
        .execute(&db)
        .await
        .unwrap();
        db.close().await;
    }
    let db = init_db(&path).await.unwrap();
    let settings: (Option<i64>, Option<i64>, i64) = sqlx::query_as(
        "SELECT homepage_page_id,posts_page_id,version FROM site_reading_settings WHERE locale = 'vi'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(settings, (None, None, 1));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM template_overrides")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let (revision_id, selection_cas): (String, i64) =
        sqlx::query_as("SELECT revision_id,version FROM theme_active_selection WHERE id=1")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!((revision_id.as_str(), selection_cas), ("old-site-rev", 17));
    let preserved: (String, String, String) = sqlx::query_as(
        "SELECT p.source_format,r.source_format,r.revision_digest
           FROM theme_packages p JOIN theme_revisions r ON r.theme_id=p.theme_id
          WHERE p.theme_id='old-site-theme'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(preserved, ("v2".into(), "v2".into(), "digest-v2".into()));
    sqlx::query(
        "INSERT INTO theme_packages (theme_id,display_name,source_format,is_legacy)
         VALUES ('new-site-theme','V3','v3',0)",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO theme_revisions (
            revision_id,theme_id,version,revision_digest,source_format,
            manifest_json,file_digests_json,provenance_json
         ) VALUES ('new-site-rev','new-site-theme','3.0.0','digest-v3','v3','{}','{}','{}')",
    )
    .execute(&db)
    .await
    .unwrap();
    let foreign_keys: Vec<(String, i64, String, i64)> = sqlx::query_as("PRAGMA foreign_key_check")
        .fetch_all(&db)
        .await
        .unwrap();
    assert!(foreign_keys.is_empty(), "{foreign_keys:?}");
    // No foreign-key cascade: deleting a Page preserves the historical ID and
    // increments the reading CAS independently at the entries writer layer.
    sqlx::query(
        "UPDATE site_reading_settings SET homepage_page_id=12345,version=version+1 WHERE locale='vi'",
    )
    .execute(&db)
    .await
    .unwrap();
    let stale: (Option<i64>, i64) = sqlx::query_as(
        "SELECT homepage_page_id,version FROM site_reading_settings WHERE locale='vi'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(stale, (Some(12345), 2));
    for column in ["page_layout_key", "custom_template_key"] {
        let cols: Vec<(i64, String, String, i64, Option<String>, i64)> =
            sqlx::query_as("PRAGMA table_info(content_entries)")
                .fetch_all(&db)
                .await
                .unwrap();
        assert!(cols.iter().any(|col| col.1 == column));
    }
    db.close().await;
    let reopened = connect_file_pool(&path).await;
    let version: i64 =
        sqlx::query_scalar("SELECT version FROM site_reading_settings WHERE locale='vi'")
            .fetch_one(&reopened)
            .await
            .unwrap();
    assert_eq!(version, 2);
}

#[tokio::test]
async fn pre_0042_legacy_theme_rows_list_cleanly_without_http_500() {
    let tmp = TempDirGuard::new("legacy_theme_list");
    let path = tmp.path().join("site.sqlite");
    {
        let db = connect_file_pool(&path).await;
        let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        let mut migrator = sqlx::migrate::Migrator::new(migrations).await.unwrap();
        migrator.migrations.to_mut().retain(|m| m.version <= 41);
        migrator.run(&db).await.unwrap();
        sqlx::query(
            "INSERT INTO theme_packages (theme_id,display_name,source_format,is_legacy)
             VALUES ('legacy-v1-a','Legacy V1','v1_legacy',1),('old-v2-theme','Legacy V2','v2',0)",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO theme_revisions (
                revision_id,theme_id,version,revision_digest,source_format,
                manifest_json,file_digests_json,provenance_json
             ) VALUES
                ('legacy-v1-rev','legacy-v1-a','1.0.0','digest-v1','v1_legacy','{}','{}','{}'),
                ('old-v2-rev','old-v2-theme','2.0.0','digest-v2','v2','{}','{}','{}')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO theme_active_selection (id,revision_id,version)
             VALUES (1,'old-v2-rev',9)
             ON CONFLICT(id) DO UPDATE SET revision_id=excluded.revision_id,version=9",
        )
        .execute(&db)
        .await
        .unwrap();
        db.close().await;
    }
    let db = init_db(&path).await.unwrap();
    let listed = list(&db)
        .await
        .expect("pre-0042 theme rows must list without 500");
    assert_eq!(listed.version, 9);
    assert_eq!(listed.selection.revision_id.as_deref(), Some("old-v2-rev"));
    assert_eq!(listed.selection.source_format, Some(ThemeSourceFormat::V2));
    assert!(listed.revisions.iter().any(
        |r| r.revision_id == "legacy-v1-rev" && r.source_format == ThemeSourceFormat::V1Legacy
    ));
    assert!(
        listed
            .revisions
            .iter()
            .any(|r| r.revision_id == "old-v2-rev" && r.source_format == ThemeSourceFormat::V2)
    );
}

#[tokio::test]
async fn two_axis_specific_theme_beats_generic_db_and_same_candidate_db_wins() {
    let tmp = TempDirGuard::new("resolver");
    let db = init_db(&tmp.path().join("site.sqlite")).await.unwrap();
    let ctx = PublicTemplateContext::Single {
        content_type: "project".into(),
        entry_id: 9,
        slug: "hello-world".into(),
        custom_template_key: None,
    };
    let candidates = build_candidates(&ctx).unwrap();
    let keys: Vec<_> = candidates
        .iter()
        .map(|candidate| candidate.canonical_key().unwrap())
        .collect();
    assert_eq!(
        keys,
        vec![
            "single:type:project:id:9",
            "single:type:project:slug:hello-world",
            "single:type:project",
            "single",
            "singular",
            "index",
        ]
    );
    let generic = target(Kind::Single);
    insert_override(&db, &generic).await;
    let mut specific = target(Kind::Single);
    specific.content_type = Some("project".into());
    let mut templates = BTreeMap::new();
    templates.insert("single:type:project".into(), "<main>Project</main>".into());
    match resolve_template_2axis(&db, &candidates, Some(&templates), None, None)
        .await
        .unwrap()
    {
        TemplateResolution::ActiveTheme { target_key, html } => {
            assert_eq!(target_key, "single:type:project");
            assert_eq!(html, "<main>Project</main>");
        }
        other => panic!("Expected specific theme source, got {other:?}"),
    }
    let id = insert_override(&db, &specific).await;
    match resolve_template_2axis(&db, &candidates, Some(&templates), None, None)
        .await
        .unwrap()
    {
        TemplateResolution::DbOverride(row) => {
            assert_eq!(row.id, id);
            assert_eq!(row.canonical_target_key, "single:type:project");
        }
        other => panic!("Expected exact DB source, got {other:?}"),
    }
    assert!(sqlx::query(
        "INSERT INTO template_overrides (target_kind, canonical_target_key, target_json, ast_json)
         VALUES ('single','single:type:project','{}','{}')"
    ).execute(&db).await.is_err());
    db.close().await;
}

#[tokio::test]
async fn candidate_order_covers_layout_home_taxonomy_not_found_and_parts() {
    let home = build_candidates(&PublicTemplateContext::Home {
        page_layout_key: Some("canvas".into()),
    })
    .unwrap();
    assert_eq!(
        home.iter()
            .map(|c| c.canonical_key().unwrap())
            .collect::<Vec<_>>(),
        ["home", "page_layout:canvas", "page", "index"]
    );
    let taxonomy = build_candidates(&PublicTemplateContext::Taxonomy {
        taxonomy: "category".into(),
        term_id: 17,
        slug: "tips".into(),
    })
    .unwrap();
    assert_eq!(
        taxonomy
            .iter()
            .map(|c| c.canonical_key().unwrap())
            .collect::<Vec<_>>(),
        [
            "taxonomy:key:category:id:17",
            "taxonomy:key:category:slug:tips",
            "taxonomy:key:category",
            "taxonomy",
            "archive",
            "index"
        ]
    );
    let absent = build_candidates(&PublicTemplateContext::NotFound).unwrap();
    assert_eq!(
        absent
            .iter()
            .map(|c| c.canonical_key().unwrap())
            .collect::<Vec<_>>(),
        ["not_found", "index"]
    );
    let search = build_candidates(&PublicTemplateContext::Search).unwrap();
    assert_eq!(
        search
            .iter()
            .map(|c| c.canonical_key().unwrap())
            .collect::<Vec<_>>(),
        ["search", "index"]
    );
    let tmp = TempDirGuard::new("parts");
    let db = init_db(&tmp.path().join("site.sqlite")).await.unwrap();
    let mut parts = BTreeMap::new();
    parts.insert("header".into(), "<header>Theme</header>".into());
    let result = resolve_template_2axis(
        &db,
        &build_candidates(&PublicTemplateContext::Part {
            part_key: "header".into(),
        })
        .unwrap(),
        None,
        Some(&parts),
        None,
    )
    .await
    .unwrap();
    assert!(matches!(result, TemplateResolution::ActiveTheme { .. }));
    let mut layouts = BTreeMap::new();
    layouts.insert("canvas".into(), "<main>Canvas</main>".into());
    let result = resolve_template_2axis(&db, &home, None, None, Some(&layouts))
        .await
        .unwrap();
    assert!(
        matches!(result, TemplateResolution::ActiveTheme { ref target_key, .. }
        if target_key == "page_layout:canvas")
    );
    assert!(matches!(
        resolve_template_2axis(&db, &absent, None, None, None)
            .await
            .unwrap(),
        TemplateResolution::CoreFallback
    ));
    db.close().await;
}
