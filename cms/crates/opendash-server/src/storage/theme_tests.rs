use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

use super::*;
use crate::storage::{builder, init_db, page};

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "opendash-m5-s01-t01-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

async fn connect_file_pool(db_path: &Path) -> SqlitePool {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .foreign_keys(true);
    SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .unwrap()
}

/// Apply only migrations 0001..=0014 to simulate an existing M3/M4 database before M5.
async fn run_pre_0015_migrations(db: &SqlitePool) {
    let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let mut migrator = sqlx::migrate::Migrator::new(migrations_dir).await.unwrap();
    migrator.migrations.to_mut().retain(|m| m.version <= 14);
    migrator.run(db).await.unwrap();
}

#[tokio::test]
async fn upgrades_m3_db_preserving_null_and_explicit_active_and_prefix_blocks() {
    let tmp = TempDirGuard::new("upgrade");
    let db_null = tmp.path().join("site-null.sqlite");

    // 1. Pre-0015 DB with NULL active theme and two legacy blocks sharing prefix `qa-a`.
    {
        let db = connect_file_pool(&db_null).await;
        run_pre_0015_migrations(&db).await;
        sqlx::query(
            "INSERT INTO theme_blocks (id, compatible_block_id, fields_json, template, content_list_template, content_single_template)
             VALUES
               ('qa-a/banner', NULL, '{}', '<div>A-banner</div>', '<ul>list-a</ul>', '<article>single-a</article>'),
               ('qa-a/hero', NULL, '{}', '<div>A-hero</div>', '<ul>list-hero</ul>', '<article>single-hero</article>')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO home_page (id, block_id, values_json) VALUES (1, 'qa-a/banner', '{}')")
            .execute(&db)
            .await
            .unwrap();
        db.close().await;
    }

    // Upgrade via init_db (runs 0015) and verify NULL selection + distinct mappings + readers.
    {
        let db = init_db(&db_null).await.unwrap();
        let active = get_active_selection(&db).await.unwrap();
        assert_eq!(active.revision_id, None);
        assert_eq!(active.legacy_block_id, None);
        assert_eq!(active.version, 1);

        let pkgs = list_packages(&db).await.unwrap();
        let legacy_pkgs: Vec<_> = pkgs.iter().filter(|p| p.is_legacy).collect();
        assert_eq!(legacy_pkgs.len(), 2);
        assert_ne!(legacy_pkgs[0].theme_id, legacy_pkgs[1].theme_id);

        let list_view = list(&db).await.unwrap();
        assert_eq!(list_view.active_id, None);
        assert_eq!(list_view.version, 1);

        let home_snap = public_home(&db).await.unwrap().unwrap();
        assert_eq!(home_snap.selected.template.as_deref(), Some("<div>A-banner</div>"));

        let (list_tpl, single_tpl) = page::active_content_templates(&db).await.unwrap();
        assert_eq!(list_tpl.as_deref(), Some("<ul>list-a</ul>"));
        assert_eq!(single_tpl.as_deref(), Some("<article>single-a</article>"));

        let home_state = builder::read(&db).await.unwrap();
        assert_eq!(home_state.active_id, None);
        db.close().await;
    }

    // 2. Pre-0015 DB with explicit active `qa-a/hero` at version 5; upgrade + restart.
    let db_explicit = tmp.path().join("site-explicit.sqlite");
    {
        let db = connect_file_pool(&db_explicit).await;
        run_pre_0015_migrations(&db).await;
        sqlx::query(
            "INSERT INTO theme_blocks (id, compatible_block_id, fields_json, template, content_list_template, content_single_template)
             VALUES
               ('qa-a/banner', NULL, '{}', '<div>A-banner</div>', '<ul>list-a</ul>', '<article>single-a</article>'),
               ('qa-a/hero', 'qa-a/banner', '{}', '<div>A-hero</div>', '<ul>list-hero</ul>', '<article>single-hero</article>')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO home_page (id, block_id, values_json) VALUES (1, 'qa-a/banner', '{}')")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("UPDATE site_active_theme SET block_id = 'qa-a/hero', version = 5 WHERE id = 1")
            .execute(&db)
            .await
            .unwrap();
        db.close().await;
    }

    let db = init_db(&db_explicit).await.unwrap();
    let active = get_active_selection(&db).await.unwrap();
    assert_eq!(active.legacy_block_id.as_deref(), Some("qa-a/hero"));
    assert_eq!(active.version, 5);
    let (list_tpl, single_tpl) = page::active_content_templates(&db).await.unwrap();
    assert_eq!(list_tpl.as_deref(), Some("<ul>list-hero</ul>"));
    assert_eq!(single_tpl.as_deref(), Some("<article>single-hero</article>"));
    let home_state = builder::read(&db).await.unwrap();
    assert_eq!(home_state.active_id.as_deref(), Some("qa-a/hero"));
    assert_eq!(home_state.active_template.as_deref(), Some("<div>A-hero</div>"));
    db.close().await;

    // Reopen after restart: state identical.
    let reopened = connect_file_pool(&db_explicit).await;
    let active_after = get_active_selection(&reopened).await.unwrap();
    assert_eq!(active_after.revision_id, active.revision_id);
    assert_eq!(active_after.version, 5);
    reopened.close().await;
}

#[tokio::test]
async fn installs_two_revisions_idempotently_and_enforces_cas_and_durability() {
    let tmp = TempDirGuard::new("revisions");
    let db_path = tmp.path().join("site.sqlite");
    let themes_root = tmp.path().join("themes");
    let db = init_db(&db_path).await.unwrap();

    // Stale staging directory from a crashed run is cleaned without touching revisions/.
    let stale_dir = themes_root.join(".staging").join("crashed-run-1");
    fs::create_dir_all(&stale_dir).unwrap();
    fs::write(stale_dir.join("leftover.txt"), b"junk").unwrap();
    let cleaned = ensure_themes_layout_and_quarantine_stale(&themes_root).unwrap();
    assert_eq!(cleaned, 1);
    assert!(!stale_dir.exists());

    // Reserved legacy prefix must be rejected for v2 theme_id.
    let mut files_v1 = BTreeMap::new();
    files_v1.insert("theme.json".to_string(), br#"{"id":"acme-theme","version":"1.0.0"}"#.to_vec());
    files_v1.insert("templates/home.html".to_string(), b"<h1>v1</h1>".to_vec());

    let bad_id = stage_and_commit_v2_revision(
        &db,
        &themes_root,
        "run-bad-id",
        "legacy-v1-hijack",
        "Hijack",
        "1.0.0",
        "{}",
        "{}",
        &files_v1,
    )
    .await
    .unwrap_err();
    assert_eq!(bad_id.0, axum::http::StatusCode::BAD_REQUEST);

    // Install v1.0.0 (does NOT activate).
    let rev1 = match stage_and_commit_v2_revision(
        &db,
        &themes_root,
        "run-1",
        "acme-theme",
        "Acme Theme",
        "1.0.0",
        "{}",
        "{}",
        &files_v1,
    )
    .await
    .unwrap()
    {
        RevisionInstallOutcome::Installed(r) => r,
        other => panic!("expected Installed, got {other:?}"),
    };
    assert_eq!(get_active_selection(&db).await.unwrap().revision_id, None);
    assert_eq!(
        verify_revision_durability(&themes_root, &rev1),
        RevisionDurabilityStatus::Valid
    );

    // Identical canonical digest is idempotent.
    match stage_and_commit_v2_revision(
        &db,
        &themes_root,
        "run-1-retry",
        "acme-theme",
        "Acme Theme",
        "1.0.0",
        "{}",
        "{}",
        &files_v1,
    )
    .await
    .unwrap()
    {
        RevisionInstallOutcome::AlreadyExists(r) => assert_eq!(r.revision_id, rev1.revision_id),
        other => panic!("expected AlreadyExists, got {other:?}"),
    }

    // Same (theme_id, version) with different digest returns 409 Conflict.
    let mut files_v1_mutated = files_v1.clone();
    files_v1_mutated.insert("templates/home.html".to_string(), b"<h1>tampered</h1>".to_vec());
    let conflict = stage_and_commit_v2_revision(
        &db,
        &themes_root,
        "run-conflict",
        "acme-theme",
        "Acme Theme",
        "1.0.0",
        "{}",
        "{}",
        &files_v1_mutated,
    )
    .await
    .unwrap_err();
    assert_eq!(conflict.0, axum::http::StatusCode::CONFLICT);

    // Install v1.1.0 for the same theme_id.
    let mut files_v2 = BTreeMap::new();
    files_v2.insert("theme.json".to_string(), br#"{"id":"acme-theme","version":"1.1.0"}"#.to_vec());
    files_v2.insert("templates/home.html".to_string(), b"<h1>v1.1</h1>".to_vec());
    let rev2 = match stage_and_commit_v2_revision(
        &db,
        &themes_root,
        "run-2",
        "acme-theme",
        "Acme Theme",
        "1.1.0",
        "{}",
        "{}",
        &files_v2,
    )
    .await
    .unwrap()
    {
        RevisionInstallOutcome::Installed(r) => r,
        other => panic!("expected Installed, got {other:?}"),
    };
    assert_ne!(rev1.revision_id, rev2.revision_id);
    assert_eq!(list_revisions(&db, "acme-theme").await.unwrap().len(), 2);

    // Missing expected version -> 428; stale version -> 412; valid CAS -> Activated.
    assert!(matches!(
        activate_revision(&db, &themes_root, &rev2.revision_id, None)
            .await
            .unwrap(),
        RevisionActivationOutcome::MissingExpectedVersion
    ));
    assert!(matches!(
        activate_revision(&db, &themes_root, &rev2.revision_id, Some(99))
            .await
            .unwrap(),
        RevisionActivationOutcome::Stale
    ));
    let activated = match activate_revision(&db, &themes_root, &rev2.revision_id, Some(1))
        .await
        .unwrap()
    {
        RevisionActivationOutcome::Activated(s) => s,
        other => panic!("expected Activated, got {other:?}"),
    };
    assert_eq!(activated.revision_id.as_deref(), Some(rev2.revision_id.as_str()));
    assert_eq!(activated.version, 2);

    // Corrupting rev1 on disk (.complete removed) blocks rollback activation without mutating active pointer.
    let rev1_marker = themes_root
        .join("revisions")
        .join(&rev1.revision_digest)
        .join(".complete");
    fs::remove_file(&rev1_marker).unwrap();
    let corrupt = activate_revision(&db, &themes_root, &rev1.revision_id, Some(2))
        .await
        .unwrap();
    assert!(matches!(
        corrupt,
        RevisionActivationOutcome::CorruptRevision(RevisionDurabilityStatus::MissingCompleteMarker)
    ));
    assert_eq!(
        get_active_selection(&db).await.unwrap().revision_id.as_deref(),
        Some(rev2.revision_id.as_str())
    );

    // Restore .complete, corrupt file bytes -> DigestMismatch blocks activation.
    fs::write(&rev1_marker, format!("{}\n", rev1.revision_digest)).unwrap();
    let rev1_tpl = themes_root
        .join("revisions")
        .join(&rev1.revision_digest)
        .join("templates/home.html");
    fs::write(&rev1_tpl, b"corrupted").unwrap();
    let mismatch = activate_revision(&db, &themes_root, &rev1.revision_id, Some(2))
        .await
        .unwrap();
    assert!(matches!(
        mismatch,
        RevisionActivationOutcome::CorruptRevision(RevisionDurabilityStatus::DigestMismatch(_))
    ));

    // Restore file bytes -> rollback to rev1 succeeds with CAS version 2 -> 3.
    fs::write(&rev1_tpl, b"<h1>v1</h1>").unwrap();
    let rolled_back = match activate_revision(&db, &themes_root, &rev1.revision_id, Some(2))
        .await
        .unwrap()
    {
        RevisionActivationOutcome::Activated(s) => s,
        other => panic!("expected Activated, got {other:?}"),
    };
    assert_eq!(rolled_back.revision_id.as_deref(), Some(rev1.revision_id.as_str()));
    assert_eq!(rolled_back.version, 3);
    db.close().await;
}
