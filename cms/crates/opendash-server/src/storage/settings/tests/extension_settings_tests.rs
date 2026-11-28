use super::*;
use opendash_core::field_schema::{FieldContext, FieldSpec, FieldType, validate_field_values};
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(prefix: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}_{}_{}", std::process::id(), nanos));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn count_files_in_dir(dir: &std::path::Path) -> usize {
    if !dir.exists() {
        return 0;
    }
    fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .count()
}

fn sample_plugin_schema() -> Vec<FieldSpec> {
    use opendash_core::field_schema::SelectOption;

    let mut api_key = FieldSpec::new(FieldType::Secret, "api_key", "API Key");
    api_key.required = true;

    let optional_token = FieldSpec::new(FieldType::Secret, "optional_token", "Optional Token");

    let mut endpoint = FieldSpec::new(FieldType::Url, "endpoint", "Endpoint");
    endpoint.required = true;
    endpoint.default = Some(serde_json::json!("https://api.example.com"));

    let mut mode = FieldSpec::new(FieldType::Select, "mode", "Mode");
    mode.default = Some(serde_json::json!("safe"));
    mode.options = vec![
        SelectOption {
            value: "safe".into(),
            label: "Safe".into(),
            icon: None,
        },
        SelectOption {
            value: "fast".into(),
            label: "Fast".into(),
            icon: None,
        },
    ];

    let badge_logo = FieldSpec::new(FieldType::Media, "badge_logo", "Badge Logo");

    vec![api_key, optional_token, endpoint, mode, badge_logo]
}

#[tokio::test]
async fn migration_0055_schema_delta_is_zero() {
    let temp = TempDirGuard::new("s02_m55_delta");
    let db = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let pre_dir = temp.0.join("pre_0055");
    let with_55_dir = temp.0.join("with_0055");
    fs::create_dir_all(&pre_dir).unwrap();
    fs::create_dir_all(&with_55_dir).unwrap();
    for entry in fs::read_dir(&migrations_dir)
        .unwrap()
        .filter_map(Result::ok)
    {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".sql") && name.as_str() < "0055_" {
            fs::copy(entry.path(), pre_dir.join(entry.file_name())).unwrap();
            fs::copy(entry.path(), with_55_dir.join(entry.file_name())).unwrap();
        } else if name.ends_with(".sql") && name.starts_with("0055_") {
            fs::copy(entry.path(), with_55_dir.join(entry.file_name())).unwrap();
        }
    }

    let pre_migrator = sqlx::migrate::Migrator::new(pre_dir.as_path())
        .await
        .unwrap();
    pre_migrator.run(&db).await.unwrap();

    let before_user_objects: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'")
            .fetch_one(&db)
            .await
            .unwrap();

    let with_55_migrator = sqlx::migrate::Migrator::new(with_55_dir.as_path())
        .await
        .unwrap();
    with_55_migrator.run(&db).await.unwrap();

    let after_user_objects: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'")
            .fetch_one(&db)
            .await
            .unwrap();

    crate::storage::MIGRATOR.run(&db).await.unwrap();
    let total_objects: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema")
        .fetch_one(&db)
        .await
        .unwrap();

    assert_eq!(after_user_objects - before_user_objects, 0);
    assert!(total_objects < 256, "sqlite_schema total={total_objects}");
}

#[tokio::test]
async fn migration_0050_schema_delta_is_plus_two_and_under_object_limit() {
    let temp = TempDirGuard::new("s02_m50_delta");
    let db = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let pre_dir = temp.0.join("pre_0050");
    let with_50_dir = temp.0.join("with_0050");
    fs::create_dir_all(&pre_dir).unwrap();
    fs::create_dir_all(&with_50_dir).unwrap();
    for entry in fs::read_dir(&migrations_dir)
        .unwrap()
        .filter_map(Result::ok)
    {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".sql") && name.as_str() < "0050_" {
            fs::copy(entry.path(), pre_dir.join(entry.file_name())).unwrap();
            fs::copy(entry.path(), with_50_dir.join(entry.file_name())).unwrap();
        } else if name.ends_with(".sql") && name.starts_with("0050_") {
            fs::copy(entry.path(), with_50_dir.join(entry.file_name())).unwrap();
        }
    }

    let pre_migrator = sqlx::migrate::Migrator::new(pre_dir.as_path())
        .await
        .unwrap();
    pre_migrator.run(&db).await.unwrap();

    let before_user_objects: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'")
            .fetch_one(&db)
            .await
            .unwrap();

    let with_50_migrator = sqlx::migrate::Migrator::new(with_50_dir.as_path())
        .await
        .unwrap();
    with_50_migrator.run(&db).await.unwrap();

    let after_user_objects: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'")
            .fetch_one(&db)
            .await
            .unwrap();

    crate::storage::MIGRATOR.run(&db).await.unwrap();
    let total_objects: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema")
        .fetch_one(&db)
        .await
        .unwrap();

    assert_eq!(after_user_objects - before_user_objects, 2);
    assert!(total_objects < 256, "sqlite_schema total={total_objects}");
}

#[tokio::test]
async fn migration_0053_schema_delta_is_zero() {
    let temp = TempDirGuard::new("s01_m53_delta");
    let db = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let pre_dir = temp.0.join("pre_0053");
    let with_53_dir = temp.0.join("with_0053");
    fs::create_dir_all(&pre_dir).unwrap();
    fs::create_dir_all(&with_53_dir).unwrap();
    for entry in fs::read_dir(&migrations_dir)
        .unwrap()
        .filter_map(Result::ok)
    {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".sql") && name.as_str() < "0053_" {
            fs::copy(entry.path(), pre_dir.join(entry.file_name())).unwrap();
            fs::copy(entry.path(), with_53_dir.join(entry.file_name())).unwrap();
        } else if name.ends_with(".sql") && name.starts_with("0053_") {
            fs::copy(entry.path(), with_53_dir.join(entry.file_name())).unwrap();
        }
    }

    let pre_migrator = sqlx::migrate::Migrator::new(pre_dir.as_path())
        .await
        .unwrap();
    pre_migrator.run(&db).await.unwrap();

    let before_user_objects: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'")
            .fetch_one(&db)
            .await
            .unwrap();

    let with_53_migrator = sqlx::migrate::Migrator::new(with_53_dir.as_path())
        .await
        .unwrap();
    with_53_migrator.run(&db).await.unwrap();

    let after_user_objects: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'")
            .fetch_one(&db)
            .await
            .unwrap();

    crate::storage::MIGRATOR.run(&db).await.unwrap();
    let total_objects: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema")
        .fetch_one(&db)
        .await
        .unwrap();

    assert_eq!(after_user_objects - before_user_objects, 0);
    assert!(total_objects < 256, "sqlite_schema total={total_objects}");
}

#[tokio::test]
async fn theme_and_plugin_with_identical_ids_are_strictly_isolated() {
    let temp = TempDirGuard::new("s02_isolation");
    let db = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    crate::storage::MIGRATOR.run(&db).await.unwrap();

    let theme_media = "a".repeat(64);
    let plugin_media = "b".repeat(64);
    for (idx, mid) in [&theme_media, &plugin_media].into_iter().enumerate() {
        sqlx::query(
            "INSERT INTO media_assets (id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height, lifecycle_state)
             VALUES (?, 'image', ?, 'image/png', 'png', 100, 10, 10, 10, 10, 'active')",
        )
        .bind(mid)
        .bind(format!("img-{idx}.png"))
        .execute(&db)
        .await
        .unwrap();
    }

    let theme_manifest = serde_json::json!({
        "manifest_version": 1,
        "theme_id": "shared-id",
        "name": "Shared Theme",
        "version": "1.0.0",
        "templates": [{ "kind": "index", "file": "index.html" }],
        "options": [
            { "key": "hero_title", "label": "Hero", "type": "text", "default": "Init Theme", "public": true },
            { "key": "badge_logo", "label": "Logo", "type": "media", "public": true }
        ]
    })
    .to_string();
    sqlx::query(
        "INSERT INTO theme_packages (theme_id, display_name, source_format)
         VALUES ('shared-id', 'Shared Theme', 'v1')",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO theme_revisions (revision_id, theme_id, version, revision_digest, source_format, manifest_json, file_digests_json, provenance_json)
         VALUES ('rev-shared', 'shared-id', '1.0.0', 'dig-shared', 'v1', ?, '{}', '{}')",
    )
    .bind(&theme_manifest)
    .execute(&db)
    .await
    .unwrap();
    sqlx::query("UPDATE theme_active_selection SET revision_id = 'rev-shared' WHERE id = 1")
        .execute(&db)
        .await
        .unwrap();

    let plugin_schema = sample_plugin_schema();
    let plugin_manifest = serde_json::json!({
        "id": "shared-id",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "name": "Shared Plugin",
        "settings_schema": plugin_schema
    })
    .to_string();
    sqlx::query(
        "INSERT INTO plugins (id, version, name, description, manifest_json, wasm_blob, enabled)
         VALUES ('shared-id', '1.0.0', 'Shared Plugin', '', ?, X'0061736d01000000', 1)",
    )
    .bind(&plugin_manifest)
    .execute(&db)
    .await
    .unwrap();

    let mut tx = db.begin().await.unwrap();
    let theme_outcome = put_theme_options_in_conn_for_locale(
        &mut tx,
        "shared-id",
        1,
        BTreeMap::from([
            ("hero_title".into(), serde_json::json!("Theme Value")),
            ("badge_logo".into(), serde_json::json!(theme_media)),
        ]),
        None,
    )
    .await
    .unwrap();
    assert!(matches!(theme_outcome, ThemeOptionsOutcome::Saved(ref s) if s.version == 2));
    tx.commit().await.unwrap();

    let mut plugin_submit = serde_json::Map::new();
    plugin_submit.insert("api_key".into(), serde_json::json!("plugin_secret_value"));
    plugin_submit.insert(
        "endpoint".into(),
        serde_json::json!("https://plugin.example.com"),
    );
    plugin_submit.insert("badge_logo".into(), serde_json::json!(plugin_media));

    let plugin_outcome = put_plugin_settings(
        &db,
        temp.0.as_path(),
        "shared-id",
        1,
        &sample_plugin_schema(),
        &plugin_submit,
    )
    .await
    .unwrap();
    assert_eq!(plugin_outcome.snapshot.version, 2);

    let theme_snap = get_theme_options_for_locale(&db, "shared-id", None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(theme_snap.version, 2);
    assert_eq!(
        theme_snap.values.get("hero_title"),
        Some(&serde_json::json!("Theme Value"))
    );
    assert_eq!(
        theme_snap.values.get("badge_logo"),
        Some(&serde_json::json!(theme_media))
    );

    let plugin_snap = get_plugin_settings_redacted(&db, "shared-id", &sample_plugin_schema())
        .await
        .unwrap();
    assert_eq!(plugin_snap.version, 2);
    assert_eq!(
        plugin_snap.values.get("endpoint"),
        Some(&serde_json::json!("https://plugin.example.com"))
    );
    assert_eq!(
        plugin_snap.values.get("badge_logo"),
        Some(&serde_json::json!(plugin_media))
    );
    assert_eq!(
        plugin_snap.values.get("api_key"),
        Some(&serde_json::json!({ "configured": true }))
    );

    let mut pin_tx = db.begin().await.unwrap();
    let theme_pins = crate::storage::media_refs::collect_pins_in_tx(&mut pin_tx, &theme_media)
        .await
        .unwrap();
    let plugin_pins = crate::storage::media_refs::collect_pins_in_tx(&mut pin_tx, &plugin_media)
        .await
        .unwrap();
    pin_tx.commit().await.unwrap();
    assert!(
        theme_pins
            .pins
            .iter()
            .any(|p| p.source_kind == "theme_option")
    );
    assert!(
        !theme_pins
            .pins
            .iter()
            .any(|p| p.source_kind == "plugin_option")
    );
    assert!(
        plugin_pins
            .pins
            .iter()
            .any(|p| p.source_kind == "plugin_option")
    );
    assert!(
        !plugin_pins
            .pins
            .iter()
            .any(|p| p.source_kind == "theme_option")
    );

    assert!(
        crate::storage::public_content::media_is_referenced(&db, &theme_media)
            .await
            .unwrap()
    );
    assert!(
        !crate::storage::public_content::media_is_referenced(&db, &plugin_media)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn plugin_settings_storage_handles_redaction_cas_preserve_rotate_clear_and_required_secret() {
    let temp = TempDirGuard::new("s02_secrets");
    let db = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    crate::storage::MIGRATOR.run(&db).await.unwrap();

    let schema = sample_plugin_schema();
    let plugin_manifest = serde_json::json!({
        "id": "sec-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "name": "Secret Plugin",
        "settings_schema": schema
    })
    .to_string();
    sqlx::query(
        "INSERT INTO plugins (id, version, name, description, manifest_json, wasm_blob, enabled)
         VALUES ('sec-plugin', '1.0.0', 'Secret Plugin', '', ?, X'0061736d01000000', 1)",
    )
    .bind(&plugin_manifest)
    .execute(&db)
    .await
    .unwrap();

    let plugin_secret_dir = temp.0.join("plugins").join("sec-plugin");
    let store = crate::secrets::SecretStore::for_plugin_dir(plugin_secret_dir.clone());

    // 1. Initial GET is redacted and unconfigured.
    let init_snap = get_plugin_settings_redacted(&db, "sec-plugin", &schema)
        .await
        .unwrap();
    assert_eq!(init_snap.version, 1);
    assert_eq!(
        init_snap.values.get("api_key"),
        Some(&serde_json::json!({ "configured": false }))
    );
    assert_eq!(
        init_snap.values.get("optional_token"),
        Some(&serde_json::json!({ "configured": false }))
    );

    // 2. Stale CAS (412) leaves 0 secret blob files on disk.
    let mut stale_submit = serde_json::Map::new();
    stale_submit.insert("api_key".into(), serde_json::json!("should_not_persist"));
    stale_submit.insert(
        "endpoint".into(),
        serde_json::json!("https://v1.example.com"),
    );
    let stale_err = put_plugin_settings(
        &db,
        temp.0.as_path(),
        "sec-plugin",
        99,
        &schema,
        &stale_submit,
    )
    .await
    .unwrap_err();
    assert_eq!(stale_err.0, axum::http::StatusCode::PRECONDITION_FAILED);
    assert_eq!(count_files_in_dir(&plugin_secret_dir), 0);

    // 3. Initial valid save writes 2 secrets + non-secret URL, returns redacted snapshot v2.
    let mut save1 = serde_json::Map::new();
    save1.insert("api_key".into(), serde_json::json!("sk_live_first"));
    save1.insert("optional_token".into(), serde_json::json!("opt_first"));
    save1.insert(
        "endpoint".into(),
        serde_json::json!("https://v1.example.com"),
    );
    let res1 = put_plugin_settings(&db, temp.0.as_path(), "sec-plugin", 1, &schema, &save1)
        .await
        .unwrap();
    assert!(res1.changed);
    assert_eq!(res1.snapshot.version, 2);
    assert_eq!(count_files_in_dir(&plugin_secret_dir), 2);
    assert_eq!(
        res1.snapshot.values.get("api_key"),
        Some(&serde_json::json!({ "configured": true }))
    );
    assert!(
        !serde_json::to_string(&res1.snapshot)
            .unwrap()
            .contains("sk_live_first")
    );
    assert!(
        !serde_json::to_string(&res1.snapshot)
            .unwrap()
            .contains("secret_ref")
    );

    let raw1 = crate::storage::plugin::read_raw_plugin_option_map(&db, "sec-plugin")
        .await
        .unwrap();
    let ref1 = raw1["api_key"]["secret_ref"].as_str().unwrap().to_owned();
    assert_eq!(store.read_plugin_secret(&ref1).unwrap(), "sk_live_first");

    // 4. Updating endpoint with null secrets preserves existing secrets without churn.
    let mut save2 = serde_json::Map::new();
    save2.insert("api_key".into(), serde_json::Value::Null);
    save2.insert("optional_token".into(), serde_json::Value::Null);
    save2.insert(
        "endpoint".into(),
        serde_json::json!("https://v2.example.com"),
    );
    let res2 = put_plugin_settings(&db, temp.0.as_path(), "sec-plugin", 2, &schema, &save2)
        .await
        .unwrap();
    assert!(res2.changed);
    assert_eq!(res2.snapshot.version, 3);
    assert_eq!(count_files_in_dir(&plugin_secret_dir), 2);
    assert_eq!(store.read_plugin_secret(&ref1).unwrap(), "sk_live_first");

    // 5. Rotating api_key removes old secret blob on commit; clearing optional_token deletes both DB & blob.
    let mut save3 = serde_json::Map::new();
    save3.insert("api_key".into(), serde_json::json!("sk_live_rotated"));
    save3.insert(
        "optional_token".into(),
        serde_json::json!({ "clear": true }),
    );
    let res3 = put_plugin_settings(&db, temp.0.as_path(), "sec-plugin", 3, &schema, &save3)
        .await
        .unwrap();
    assert!(res3.changed);
    assert_eq!(res3.snapshot.version, 4);
    assert_eq!(count_files_in_dir(&plugin_secret_dir), 1);
    assert_eq!(
        res3.snapshot.values.get("api_key"),
        Some(&serde_json::json!({ "configured": true }))
    );
    assert_eq!(
        res3.snapshot.values.get("optional_token"),
        Some(&serde_json::json!({ "configured": false }))
    );
    assert!(store.read_plugin_secret(&ref1).is_err());

    let raw3 = crate::storage::plugin::read_raw_plugin_option_map(&db, "sec-plugin")
        .await
        .unwrap();
    assert!(!raw3.contains_key("optional_token"));
    let ref2 = raw3["api_key"]["secret_ref"].as_str().unwrap().to_owned();
    assert_eq!(store.read_plugin_secret(&ref2).unwrap(), "sk_live_rotated");

    // 6. Reject clearing a required secret without deleting existing secret.
    let mut bad_clear = serde_json::Map::new();
    bad_clear.insert("api_key".into(), serde_json::json!({ "clear": true }));
    let clear_err =
        put_plugin_settings(&db, temp.0.as_path(), "sec-plugin", 4, &schema, &bad_clear)
            .await
            .unwrap_err();
    assert_eq!(clear_err.0, axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(count_files_in_dir(&plugin_secret_dir), 1);
    assert_eq!(store.read_plugin_secret(&ref2).unwrap(), "sk_live_rotated");
}
