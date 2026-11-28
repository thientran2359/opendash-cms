use super::*;

#[tokio::test]
async fn override_atomic_cas_reset_source_and_part_cycle_and_slot_contract() {
    let tmp = TempDirGuard::new("cas");
    let db = init_db(&tmp.path().join("site.sqlite")).await.unwrap();
    let mut header = target(Kind::Part);
    header.part_key = Some("header".into());
    let empty = BuilderDocument::empty();
    let first = save_typed_override(&db, &header, &empty, None)
        .await
        .unwrap();
    assert_eq!(first.version, 1);
    assert_eq!(first.canonical_target_key, "part:header");
    assert_eq!(
        get_typed_override(&db, &header)
            .await
            .unwrap()
            .unwrap()
            .version,
        1
    );
    assert_eq!(
        save_typed_override(&db, &header, &empty, None)
            .await
            .unwrap_err()
            .0,
        axum::http::StatusCode::PRECONDITION_REQUIRED
    );
    let second = save_typed_override(&db, &header, &empty, Some(1))
        .await
        .unwrap();
    assert_eq!(second.version, 2);
    assert_eq!(
        save_typed_override(&db, &header, &empty, Some(1))
            .await
            .unwrap_err()
            .0,
        axum::http::StatusCode::PRECONDITION_FAILED
    );
    let mut parts = BTreeMap::new();
    parts.insert("header".into(), "<header>Base</header>".into());
    let chain = build_candidates(&PublicTemplateContext::Part {
        part_key: "header".into(),
    })
    .unwrap();
    assert_eq!(
        reset_typed_override(&db, first.id, None, &chain, None, Some(&parts), None)
            .await
            .unwrap_err()
            .0,
        axum::http::StatusCode::PRECONDITION_REQUIRED
    );
    assert_eq!(
        reset_typed_override(&db, first.id, Some(1), &chain, None, Some(&parts), None)
            .await
            .unwrap_err()
            .0,
        axum::http::StatusCode::PRECONDITION_FAILED
    );
    let reset = reset_typed_override(&db, first.id, Some(2), &chain, None, Some(&parts), None)
        .await
        .unwrap();
    assert_eq!(reset.next_effective_source.kind, "active_theme");
    assert_eq!(
        reset.next_effective_source.target_key.as_deref(),
        Some("part:header")
    );
    assert!(get_typed_override(&db, &header).await.unwrap().is_none());

    let mut single = target(Kind::Single);
    single.content_type = Some("post".into());
    assert_eq!(
        save_typed_override(&db, &single, &empty, None)
            .await
            .unwrap_err()
            .0,
        axum::http::StatusCode::BAD_REQUEST
    );
    assert_eq!(
        save_typed_override(&db, &header, &one_part("header"), None)
            .await
            .unwrap_err()
            .0,
        axum::http::StatusCode::BAD_REQUEST
    );
    let mut footer = target(Kind::Part);
    footer.part_key = Some("footer".into());
    let footer_base = save_typed_override(&db, &footer, &empty, None)
        .await
        .unwrap();
    save_typed_override(&db, &header, &one_part("footer"), None)
        .await
        .unwrap();
    assert_eq!(
        save_typed_override(&db, &footer, &one_part("header"), Some(footer_base.version))
            .await
            .unwrap_err()
            .0,
        axum::http::StatusCode::BAD_REQUEST
    );
    assert_eq!(
        get_typed_override(&db, &footer)
            .await
            .unwrap()
            .unwrap()
            .version,
        1
    );
    db.close().await;
}

#[tokio::test]
async fn unpublished_id_specific_override_is_pinned_but_not_publicly_readable() {
    let tmp = TempDirGuard::new("acl");
    let db = init_db(&tmp.path().join("site.sqlite")).await.unwrap();
    let media_id = "a".repeat(64);
    sqlx::query(
        "INSERT INTO media_assets (id,filename,mime_type,extension,byte_size,width,height,thumb_width,thumb_height)
         VALUES (?, 'asset.png','image/png','png',12,1,1,1,1)",
    ).bind(&media_id).execute(&db).await.unwrap();
    let inserted = sqlx::query(
        "INSERT INTO content_entries (content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
         VALUES ('post','unpublished-selector','Draft','{}','draft','vi','eg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let mut specific = target(Kind::Single);
    specific.content_type = Some("post".into());
    specific.entry_id = Some(inserted);
    let override_id = insert_override(&db, &specific).await;
    sqlx::query(
        "INSERT INTO template_override_media_refs (override_id,media_id,ref_role)
         VALUES (?,?,'image')",
    )
    .bind(override_id)
    .bind(&media_id)
    .execute(&db)
    .await
    .unwrap();
    assert!(
        !public_content::media_is_referenced(&db, &media_id)
            .await
            .unwrap()
    );
    let mut conn = db.acquire().await.unwrap();
    let pins = media_refs::collect_pins_in_tx(&mut conn, &media_id)
        .await
        .unwrap();
    assert_eq!(pins.total_pins, 1);
    drop(conn);
    sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
        .bind(inserted)
        .execute(&db)
        .await
        .unwrap();
    assert!(
        public_content::media_is_referenced(&db, &media_id)
            .await
            .unwrap()
    );
    sqlx::query("UPDATE content_entries SET publication_status='draft' WHERE id=?")
        .bind(inserted)
        .execute(&db)
        .await
        .unwrap();
    assert!(
        !public_content::media_is_referenced(&db, &media_id)
            .await
            .unwrap()
    );
    db.close().await;
}

#[test]
fn builtin_starter_v1_validates_with_canvas_chrome_and_v1_options() {
    use crate::builtin_themes::{STARTER_THEME_VERSION, builtin_starter_theme_files};
    use crate::storage::theme::compute_canonical_file_map_digest;

    let files = builtin_starter_theme_files();
    let (digest, _) = compute_canonical_file_map_digest(&files);
    assert_eq!(STARTER_THEME_VERSION, "1.0.0");
    assert_eq!(digest.len(), 64);

    let validated = crate::theme::validate_theme_files(files)
        .expect("Starter V1 1.0.0 must pass v3 validation");
    assert_eq!(validated.manifest.version, "1.0.0");
    let canvas = validated
        .manifest
        .page_layouts
        .iter()
        .find(|layout| layout.key == "canvas")
        .expect("Starter V1 must define canvas layout");
    assert_eq!(
        serde_json::to_value(canvas).unwrap()["chrome"],
        json!("none")
    );
    let specs = crate::storage::settings::parse_theme_options_schema(&validated.manifest_json)
        .expect("Starter V1 options must validate under Unified Field Engine");
    assert!(!specs.is_empty());
}

#[tokio::test]
async fn starter_v1_cleanly_replaces_pre_v1_starter_without_conflict_or_fk_error() {
    use crate::builtin_themes::{STARTER_THEME_ID, builtin_starter_theme_files};
    use crate::storage::theme::{
        RevisionActivationOutcome, RevisionInstallOutcome, activate_revision,
        compute_canonical_file_map_digest, ensure_default_starter_theme, get_active_selection,
        list_revisions, stage_and_commit_revision,
    };

    // Part A: pre-V1 active starter with same version "1.0.0" (different digest) -> clean replace & activate V1
    let tmp = TempDirGuard::new("starter-v1-replace-active");
    let db_path = tmp.path().join("database").join("opendash.sqlite");
    fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let themes_root = tmp.path().join("themes");
    let db = init_db(&db_path).await.unwrap();

    let mut pre_v1_files = builtin_starter_theme_files();
    let mut manifest_val: serde_json::Value =
        serde_json::from_slice(pre_v1_files.get("theme.json").unwrap()).unwrap();
    manifest_val["name"] = json!("OpenDash Starter Pre-V1");
    pre_v1_files.insert(
        "theme.json".to_owned(),
        serde_json::to_vec_pretty(&manifest_val).unwrap(),
    );
    let (pre_digest, _) = compute_canonical_file_map_digest(&pre_v1_files);
    let pre_pkg = crate::theme::validate_theme_files(pre_v1_files.clone()).unwrap();
    let pre_rev = match stage_and_commit_revision(
        &db,
        &themes_root,
        "seed-pre-v1",
        STARTER_THEME_ID,
        "OpenDash Starter Pre-V1",
        "1.0.0",
        &pre_pkg.manifest_json,
        r#"{"source":"builtin_starter"}"#,
        &pre_v1_files,
    )
    .await
    .unwrap()
    {
        RevisionInstallOutcome::Installed(rec) | RevisionInstallOutcome::AlreadyExists(rec) => rec,
    };
    let RevisionActivationOutcome::Activated(_) =
        activate_revision(&db, &themes_root, &pre_rev.revision_id, Some(1))
            .await
            .unwrap()
    else {
        panic!("expected activation of pre-V1 starter");
    };

    let installed = ensure_default_starter_theme(&db, &themes_root)
        .await
        .expect("replacing pre-V1 active starter must not return 409 Conflict or FK error");
    assert_eq!(installed.version, "1.0.0");
    assert_ne!(installed.revision_digest, pre_digest);
    let after_selection = get_active_selection(&db).await.unwrap();
    assert_eq!(
        after_selection.revision_id.as_deref(),
        Some(installed.revision_id.as_str())
    );
    let versions: Vec<String> = list_revisions(&db, STARTER_THEME_ID)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.version)
        .collect();
    assert_eq!(versions, vec!["1.0.0"]);
    db.close().await;

    // Part B: pre-V1 inactive starter "1.1.1" when another custom theme is active -> preserves custom active selection
    let tmp_custom = TempDirGuard::new("starter-v1-preserve-custom");
    let db_path_c = tmp_custom.path().join("database").join("opendash.sqlite");
    fs::create_dir_all(db_path_c.parent().unwrap()).unwrap();
    let themes_root_c = tmp_custom.path().join("themes");
    let db_c = init_db(&db_path_c).await.unwrap();

    manifest_val["version"] = json!("1.1.1");
    pre_v1_files.insert(
        "theme.json".to_owned(),
        serde_json::to_vec_pretty(&manifest_val).unwrap(),
    );
    let pre_111_pkg = crate::theme::validate_theme_files(pre_v1_files.clone()).unwrap();
    stage_and_commit_revision(
        &db_c,
        &themes_root_c,
        "seed-pre-111",
        STARTER_THEME_ID,
        "OpenDash Starter Pre-V1",
        "1.1.1",
        &pre_111_pkg.manifest_json,
        r#"{"source":"builtin_starter"}"#,
        &pre_v1_files,
    )
    .await
    .unwrap();

    let custom_files = BTreeMap::from([
        (
            "theme.json".to_owned(),
            br#"{"manifest_version":1,"theme_id":"custom-active","name":"Custom","version":"1.0.0","templates":[{"kind":"index","file":"index.html"}]}"#.to_vec(),
        ),
        ("index.html".to_owned(), b"<main>Custom</main>".to_vec()),
    ]);
    let custom_pkg = crate::theme::validate_theme_files(custom_files.clone()).unwrap();
    let RevisionInstallOutcome::Installed(custom_rev) = stage_and_commit_revision(
        &db_c,
        &themes_root_c,
        "seed-custom",
        "custom-active",
        "Custom",
        "1.0.0",
        &custom_pkg.manifest_json,
        "{}",
        &custom_files,
    )
    .await
    .unwrap() else {
        panic!("expected custom install");
    };
    activate_revision(&db_c, &themes_root_c, &custom_rev.revision_id, Some(1))
        .await
        .unwrap();

    let installed_c = ensure_default_starter_theme(&db_c, &themes_root_c)
        .await
        .unwrap();
    assert_eq!(installed_c.version, "1.0.0");
    let sel_c = get_active_selection(&db_c).await.unwrap();
    assert_eq!(
        sel_c.revision_id.as_deref(),
        Some(custom_rev.revision_id.as_str())
    );
    let versions_c: Vec<String> = list_revisions(&db_c, STARTER_THEME_ID)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.version)
        .collect();
    assert_eq!(versions_c, vec!["1.0.0"]);
    db_c.close().await;
}

#[tokio::test]
async fn migration_0063_clean_slate_and_restore_pre_v1_rejection() {
    let tmp = TempDirGuard::new("mig-0063-clean-slate");
    let db_path = tmp.path().join("upgrade_0063.sqlite");
    {
        let db = connect_file_pool(&db_path).await;
        let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        let migrator = sqlx::migrate::Migrator::new(migrations).await.unwrap();
        migrator.run_to(62, &db).await.unwrap();

        sqlx::query(
            "INSERT INTO theme_packages (theme_id,display_name,source_format,is_legacy)
             VALUES ('pre-v1-pkg','PreV1','v3',0)",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO theme_revisions (
                revision_id,theme_id,version,revision_digest,source_format,
                manifest_json,file_digests_json,provenance_json
             ) VALUES (
                'pre-v1-rev','pre-v1-pkg','1.0.0','digest-pre-v1','v3',
                '{\"manifest_version\":3,\"theme_id\":\"pre-v1-pkg\",\"name\":\"PreV1\",\"version\":\"1.0.0\"}',
                '{}','{}'
             )",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO theme_active_selection (id,revision_id,version)
             VALUES (1,'pre-v1-rev',11)
             ON CONFLICT(id) DO UPDATE SET revision_id=excluded.revision_id,version=11",
        )
        .execute(&db)
        .await
        .unwrap();
        let doc_json = serde_json::to_string(&BuilderDocument::empty()).unwrap();
        sqlx::query(
            "INSERT INTO template_overrides (target_kind,canonical_target_key,target_json,ast_json)
             VALUES ('home','home','{\"kind\":\"home\"}',?),('search','search','{\"kind\":\"search\"}',?)",
        )
        .bind(&doc_json)
        .bind(&doc_json)
        .execute(&db)
        .await
        .unwrap();
        db.close().await;
    }

    // 1. Pre-migration backup validation must reject pre-V1 DB with 400 Bad Request before running 0063.
    let err = crate::storage::restore::validate_restored_db_read_only(&db_path)
        .await
        .expect_err("restore validation must reject pre-V1 theme revisions with 400");
    assert_eq!(err.0, axum::http::StatusCode::BAD_REQUEST);

    // 2. Running 0063 via init_db drops pre-V1 themes + 'home' override, keeps 'search' & CAS version, adds idx_content_entries_pub_order.
    let db = init_db(&db_path).await.unwrap();
    let pkg_cnt: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM theme_packages")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(pkg_cnt, 0);
    let sel: (Option<String>, i64) =
        sqlx::query_as("SELECT revision_id,version FROM theme_active_selection WHERE id=1")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(sel, (None, 11));
    let remaining_overrides: Vec<String> = sqlx::query_scalar(
        "SELECT canonical_target_key FROM template_overrides ORDER BY canonical_target_key",
    )
    .fetch_all(&db)
    .await
    .unwrap();
    assert_eq!(remaining_overrides, vec!["search"]);
    let has_adj_idx: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_schema WHERE type='index' AND name='idx_content_entries_pub_order'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(has_adj_idx, 1);
    let schema_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_schema WHERE name != '_sqlx_migrations' AND name != 'sqlite_sequence' AND name NOT LIKE 'sqlite_stat%'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(schema_rows <= 256, "sqlite_schema count {schema_rows} > 256");
    db.close().await;
}
