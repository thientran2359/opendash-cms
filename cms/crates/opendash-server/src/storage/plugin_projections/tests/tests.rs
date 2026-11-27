use super::*;

#[tokio::test]
async fn projection_locale_gate_preserves_parked_rows_and_rejects_stale_writes() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    crate::storage::MIGRATOR.run(&pool).await.unwrap();
    sqlx::query("INSERT INTO site_locales(code,bcp47_tag,label) VALUES('en','en','English')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE site_locales_state SET multilingual_enabled = 1 WHERE id = 1")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO plugins (id,name,version,manifest_json,wasm_blob,enabled)
         VALUES('projection-test','Projection Test','1.0.0','{}',X'00',1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO content_entries
         (id,content_type_key,locale,translation_group_id,slug,title,values_json,publication_status)
         VALUES(1,'post','en','grp-en','example','Example','{}','draft')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE content_entries SET publication_status = 'published' WHERE id = 1")
        .execute(&pool)
        .await
        .unwrap();
    let source = load_published_entry_source(&pool, 1)
        .await
        .unwrap()
        .unwrap();
    let items = vec![ComputedPluginProjection {
        owner_kind: ExtensionOwnerKind::Plugin,
        plugin_id: "projection-test".into(),
        theme_fence: None,
        seo_json: None,
        ext_json: Some(r#"{"badge":"original"}"#.into()),
    }];
    let mut tx = pool.begin().await.unwrap();
    write_entry_projections_in_tx(
        &mut tx,
        1,
        &source.locale,
        source.version,
        &source.plugin_generations,
        &items,
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    for (toggle, locale_enabled, deleted_at) in
        [(0, 1, None), (1, 0, None), (1, 0, Some("2026-01-01"))]
    {
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("UPDATE site_locales_state SET multilingual_enabled = ? WHERE id = 1")
            .bind(toggle)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("UPDATE site_locales SET enabled = ?, deleted_at = ? WHERE code = 'en'")
            .bind(locale_enabled)
            .bind(deleted_at)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert!(
            load_published_entry_source(&pool, 1)
                .await
                .unwrap()
                .is_none()
        );

        let mut tx = pool.begin().await.unwrap();
        // Neither the missing-source cleanup nor an in-flight empty
        // computation may erase parked projections.
        assert!(
            !projection_write_allowed_in_tx(&mut tx, 1, None, None)
                .await
                .unwrap()
        );
        write_entry_projections_in_tx(
            &mut tx,
            1,
            "en",
            source.version,
            &source.plugin_generations,
            &[],
            None,
        )
        .await
        .unwrap();
        write_entry_projections_in_tx(
            &mut tx,
            1,
            "en",
            source.version,
            &source.plugin_generations,
            &[],
            Some((ExtensionOwnerKind::Plugin, "projection-test", None)),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let stored: String =
            sqlx::query_scalar("SELECT ext_json FROM plugin_entry_projections WHERE entry_id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stored, r#"{"badge":"original"}"#);
    }

    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE site_locales SET enabled = 1, deleted_at = NULL WHERE code = 'en'")
        .execute(&mut *tx)
        .await
        .unwrap();
    // A stale computed locale must not overwrite or erase the current one.
    write_entry_projections_in_tx(
        &mut tx,
        1,
        "vi",
        source.version,
        &source.plugin_generations,
        &[],
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert!(
        load_published_entry_source(&pool, 1)
            .await
            .unwrap()
            .is_some()
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plugin_entry_projections")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);

    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE content_entries SET publication_status = 'draft' WHERE id = 1")
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(
        !projection_write_allowed_in_tx(&mut tx, 1, None, None)
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plugin_entry_projections")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn stale_projection_cannot_reinsert_during_trash_or_overwrite_after_restore() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    crate::storage::MIGRATOR.run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO plugins (id,name,version,manifest_json,wasm_blob,enabled,revision_generation)
         VALUES('projection-race','Projection Race','1.0.0','{}',X'00',1,1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO content_entries
         (id,content_type_key,locale,translation_group_id,slug,title,values_json,publication_status,version)
         VALUES(1,'post','vi','grp-race','race-post','Race Post','{}','draft',1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE content_entries SET publication_status = 'published', version = 2 WHERE id = 1",
    )
    .execute(&pool)
    .await
    .unwrap();

    let v2_source = load_published_entry_source(&pool, 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(v2_source.version, 2);
    let v2_items = vec![ComputedPluginProjection {
        owner_kind: ExtensionOwnerKind::Plugin,
        plugin_id: "projection-race".into(),
        theme_fence: None,
        seo_json: None,
        ext_json: Some(r#"{"ver":"v2"}"#.into()),
    }];

    // Entry is trashed (v3): pre-trash v2 computation arrives late and must NOT write.
    sqlx::query(
        "UPDATE content_entries SET trashed_at = datetime('now'), version = 3 WHERE id = 1",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        load_published_entry_source(&pool, 1)
            .await
            .unwrap()
            .is_none()
    );
    {
        let mut tx = pool.begin().await.unwrap();
        write_entry_projections_in_tx(
            &mut tx,
            1,
            &v2_source.locale,
            v2_source.version,
            &v2_source.plugin_generations,
            &v2_items,
            None,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }
    let count_trashed: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM plugin_entry_projections WHERE entry_id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count_trashed, 0);

    // Entry is restored (v4) and republished (v5) with fresh projection {"ver":"v5"}.
    sqlx::query(
        "UPDATE content_entries SET trashed_at = NULL, publication_status = 'published', version = 5 WHERE id = 1",
    )
    .execute(&pool)
    .await
    .unwrap();
    let v5_source = load_published_entry_source(&pool, 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(v5_source.version, 5);
    let v5_items = vec![ComputedPluginProjection {
        owner_kind: ExtensionOwnerKind::Plugin,
        plugin_id: "projection-race".into(),
        theme_fence: None,
        seo_json: None,
        ext_json: Some(r#"{"ver":"v5"}"#.into()),
    }];
    {
        let mut tx = pool.begin().await.unwrap();
        write_entry_projections_in_tx(
            &mut tx,
            1,
            &v5_source.locale,
            v5_source.version,
            &v5_source.plugin_generations,
            &v5_items,
            None,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }

    // Pre-trash v2 computation arrives after restore+republish: must NEITHER overwrite NOR erase v5!
    {
        let mut tx = pool.begin().await.unwrap();
        write_entry_projections_in_tx(
            &mut tx,
            1,
            &v2_source.locale,
            v2_source.version,
            &v2_source.plugin_generations,
            &v2_items,
            None,
        )
        .await
        .unwrap();
        write_entry_projections_in_tx(
            &mut tx,
            1,
            &v2_source.locale,
            v2_source.version,
            &v2_source.plugin_generations,
            &[],
            Some((ExtensionOwnerKind::Plugin, "projection-race", None)),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }
    let stored_v5: String = sqlx::query_scalar(
        "SELECT ext_json FROM plugin_entry_projections WHERE entry_id = 1 AND plugin_id = 'projection-race'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored_v5, r#"{"ver":"v5"}"#);

    // Plugin generation advances from 1 to 2: stale gen-1 compute for v5 must not overwrite gen-2 row.
    sqlx::query("UPDATE plugins SET revision_generation = 2 WHERE id = 'projection-race'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE plugin_entry_projections SET ext_json = '{\"ver\":\"v5-gen2\"}' WHERE entry_id = 1",
    )
    .execute(&pool)
    .await
    .unwrap();
    {
        let mut tx = pool.begin().await.unwrap();
        write_entry_projections_in_tx(
            &mut tx,
            1,
            &v5_source.locale,
            v5_source.version,
            &v5_source.plugin_generations,
            &v5_items,
            Some((ExtensionOwnerKind::Plugin, "projection-race", None)),
        )
        .await
        .unwrap();
        write_entry_projections_in_tx(
            &mut tx,
            1,
            &v5_source.locale,
            v5_source.version,
            &v5_source.plugin_generations,
            &v5_items,
            None,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }
    let stored_gen2: String = sqlx::query_scalar(
        "SELECT ext_json FROM plugin_entry_projections WHERE entry_id = 1 AND plugin_id = 'projection-race'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored_gen2, r#"{"ver":"v5-gen2"}"#);
}

#[tokio::test]
async fn public_seo_read_uses_projections_with_zero_wasm_and_monotonic_noindex() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    crate::storage::MIGRATOR.run(&pool).await.unwrap();

    let trap_wasm = wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "opendash_run") (result i32)
                unreachable)
        )"#,
    )
    .unwrap();
    let manifest_json = json!({
        "id": "trap-seo-ext",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "name": "Trap SEO Ext",
        "filters": ["filter:seo/head", "filter:entry/view"],
        "permissions": { "content_read": ["post"] }
    })
    .to_string();
    sqlx::query(
        "INSERT INTO plugins (id, name, version, manifest_json, wasm_blob, enabled)
         VALUES (?, ?, ?, ?, ?, 1)",
    )
    .bind("trap-seo-ext")
    .bind("Trap SEO Ext")
    .bind("1.0.0")
    .bind(&manifest_json)
    .bind(&trap_wasm)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO content_entries
         (id, content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, version)
         VALUES (1, 'post', 'vi', 'grp-1', 'zero-wasm-post', 'Zero Wasm Post', '{}', 'draft', 1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE content_entries SET publication_status = 'published' WHERE id = 1")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO entry_seo (entry_id, noindex) VALUES (1, 1)")
        .execute(&pool)
        .await
        .unwrap();
    let stored_seo = json!({
        "title": "Plugin SEO Title",
        "description": "Plugin Desc",
        "noindex": false,
        "json_ld": [{
            "@context": "https://schema.org",
            "@type": "Article",
            "headline": "Bad</script><script>"
        }]
    })
    .to_string();
    let stored_ext = json!({ "badge": "hot" }).to_string();
    sqlx::query(
        "INSERT INTO plugin_entry_projections (entry_id, plugin_id, locale, seo_json, ext_json)
         VALUES (1, 'trap-seo-ext', 'vi', ?, ?)",
    )
    .bind(&stored_seo)
    .bind(&stored_ext)
    .execute(&pool)
    .await
    .unwrap();

    // Reading SEO & projections never spawns Wasm (trap_wasm would fail if invoked).
    let seo_view = crate::storage::seo::build_entry_seo_view_for_locale(
        &pool,
        1,
        "Zero Wasm Post",
        Some("vi"),
        "/zero-wasm-post",
    )
    .await
    .unwrap();
    assert_eq!(seo_view.title, "Plugin SEO Title");
    assert_eq!(seo_view.description.as_deref(), Some("Plugin Desc"));
    assert_eq!(seo_view.robots, "noindex,nofollow");
    assert_eq!(seo_view.json_ld.len(), 1);
    let escaped = crate::content_render::serialize_json_ld_safe(&seo_view.json_ld[0]);
    assert!(escaped.contains(r"\u003c/script\u003e"));
    assert!(!escaped.contains("</script>"));

    let loaded = batch_load_entry_projections(&pool, &[1]).await.unwrap();
    assert_eq!(
        loaded[&1].ext["trap-seo-ext"]["badge"],
        serde_json::json!("hot")
    );
}
