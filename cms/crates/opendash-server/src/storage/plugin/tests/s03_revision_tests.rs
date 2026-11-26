use super::*;

fn sample_schema() -> BlockSchemaV1 {
    serde_json::from_value(serde_json::json!({
        "schema_version": 1, "id": "sample-plugin/notice",
        "title": "Notice", "category": "content", "render": "render.html",
        "fields": []
    }))
    .unwrap()
}

#[test]
fn blockless_plugin_digest_is_backward_compatible() {
    let manifest = "{\"id\":\"sample-plugin\"}";
    let wasm = b"arbitrary wasm fixture";
    let legacy = compute_plugin_revision_digest_with_extensions(manifest, wasm, None);
    assert_eq!(legacy, compute_plugin_revision_digest(manifest, wasm));
    assert_eq!(
        legacy,
        compute_plugin_revision_digest_with_blocks_json(manifest, wasm, None, None)
    );
    let root = std::env::temp_dir().join(format!(
        "od-s03-legacy-plugin-{}",
        crate::auth::generate_random_token()
    ));
    stage_and_publish_plugin_revision(&root, "test", &legacy, manifest, wasm).unwrap();
    assert!(
        !root
            .join("revisions")
            .join(&legacy)
            .join("blocks.json")
            .exists()
    );
    assert!(verify_plugin_revision_durability(&root, &legacy).is_ok());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn canonical_block_revision_digest_detects_disk_tampering() {
    let manifest = r#"{"id":"sample-plugin","version":"1.0.0","api_version":1,"name":"Sample","entrypoint":"plugin.wasm","permissions":{}}"#;
    let wasm = b"test wasm";
    let blocks = vec![(sample_schema(), "<p>Safe</p>".to_string())];
    let json = canonical_blocks_json("sample-plugin", &blocks)
        .unwrap()
        .unwrap();
    let digest = compute_plugin_revision_digest_with_blocks_json(manifest, wasm, None, Some(&json));
    let old_digest = compute_plugin_revision_digest(manifest, wasm);
    assert_ne!(digest, old_digest);

    let root = std::env::temp_dir().join(format!(
        "od-s03-plugin-blocks-{}",
        crate::auth::generate_random_token()
    ));
    stage_and_publish_plugin_revision_with_blocks(
        &root,
        "test",
        &digest,
        manifest,
        wasm,
        None,
        Some(&json),
    )
    .unwrap();
    assert!(verify_plugin_revision_durability(&root, &digest).is_ok());
    fs::write(
        root.join("revisions").join(&digest).join("blocks.json"),
        "{}",
    )
    .unwrap();
    assert!(verify_plugin_revision_durability(&root, &digest).is_err());
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn install_rejects_cross_provider_claim_before_disk_staging() {
    let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("../../migrations").run(&db).await.unwrap();
    let schema = sample_schema();
    let schema_json = serde_json::to_string(&schema).unwrap();
    sqlx::query(
        "INSERT INTO builder_block_definitions (block_id,owner_kind,owner_id,provider_status,declaration_state,schema_json,template,revision_digest)
         VALUES (?, 'theme','other-theme','active','declared',?, '<p>Safe</p>','digest')",
    )
    .bind(&schema.id)
    .bind(&schema_json)
    .execute(&db).await.unwrap();

    let manifest_json = r#"{"id":"sample-plugin","version":"1.0.0","api_version":1,"name":"Sample","entrypoint":"plugin.wasm","permissions":{}}"#;
    let manifest = opendash_core::plugin::PluginManifest::from_json_str(manifest_json).unwrap();
    let root = std::env::temp_dir().join(format!(
        "od-s03-plugin-conflict-{}",
        crate::auth::generate_random_token()
    ));
    let result = install_plugin_revision(
        &db,
        &root,
        &manifest,
        manifest_json,
        b"fixture",
        None,
        &[(schema, "<p>Safe</p>".to_string())],
    )
    .await;
    assert!(matches!(result, Err(PluginStorageError::Conflict(_))));
    assert!(!root.join("revisions").exists());
}
