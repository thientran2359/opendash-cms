use super::*;
use opendash_core::field_schema::{FieldContext, FieldSpec, FieldType, validate_field_values};
use sqlx::SqlitePool;
use std::collections::BTreeMap;

#[test]
fn typed_url_policy_requires_public_url_schema_and_valid_defaults() {
    let schema = serde_json::json!({"options": [
        {"key":"cta_url","label":"CTA","type":"url","default":"/lien-he","public":true},
        {"key":"private_url","label":"Private","type":"url","default":"#secret","public":false},
        {"key":"non_url_text","label":"Text","type":"text","default":"/old","public":true}
    ]});
    let raw = schema.to_string();
    let policy = theme_url_policy(&raw).unwrap();
    let src = r#"<a href="{{ options.cta_url }}">Link</a>"#;
    assert!(crate::template_sec::validate_template_csec_with_theme_urls("x", src, &policy).is_ok());
    for key in ["private_url", "non_url_text", "unknown_url"] {
        assert!(
            crate::template_sec::validate_template_csec_with_theme_urls(
                "x",
                &format!(r#"<a href="{{{{ options.{key} }}}}">Link</a>"#),
                &policy,
            )
            .is_err()
        );
    }
    let invalid = serde_json::json!({"options": [{
        "key":"cta_url", "label":"CTA", "type":"url", "default":"javascript:alert(1)", "public":true
    }]});
    assert!(parse_theme_options_schema(&invalid.to_string()).is_err());
    let parsed = parse_theme_options_schema(&raw).unwrap();
    let vals = BTreeMap::from([(
        "cta_url".to_owned(),
        serde_json::json!("/changed?from=header#form"),
    )]);
    assert!(validate_field_values(&parsed, FieldContext::Options, &vals, None).is_ok());
    assert!(validate_url_or_link_spec_strict(&parsed[0], &vals["cta_url"]).is_ok());
}

#[test]
fn catalog_is_pinned_sorted_and_restricts_aliases() {
    assert!(timezone_catalog().windows(2).all(|pair| pair[0] < pair[1]));
    assert!(is_current_timezone("UTC"));
    assert!(is_current_timezone("Asia/Ho_Chi_Minh"));
    assert!(is_current_timezone("America/New_York"));
    assert!(!is_current_timezone("US/Eastern"));
    assert!(!is_current_timezone("../UTC"));
}

#[test]
fn unicode_counts_are_scalars_and_controls_are_invalid() {
    let make = |site_title: String, tagline: &str| GeneralSettingsInput {
        site_title,
        tagline: tagline.into(),
        timezone: "UTC".into(),
        cms_locale: None,
    };
    assert!(make(" Xin chào ".into(), "").validate().is_ok());
    assert!(make("🦀".repeat(120), "").validate().is_ok());
    assert!(make("🦀".repeat(121), "").validate().is_err());
    assert!(make(" \t ".into(), "").validate().is_err());
    assert!(make("\tAccepted?".into(), "").validate().is_err());
    assert!(make("ok".into(), "bad\nvalue").validate().is_err());
}

#[tokio::test]
async fn cms_locale_is_global_canonical_and_backward_compatible() {
    let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
    crate::storage::content::preflight_builtin_key_collisions(&db)
        .await
        .unwrap();
    crate::storage::MIGRATOR.run(&db).await.unwrap();
    let initial = get_general(&db).await.unwrap();
    assert_eq!(initial.cms_locale, "en");
    let input = |cms_locale: Option<&str>| GeneralSettingsInput {
        site_title: initial.site_title.clone(),
        tagline: initial.tagline.clone(),
        timezone: initial.timezone.clone(),
        cms_locale: cms_locale.map(str::to_owned),
    };
    assert!(matches!(
        save_general(&db, input(Some("unknown")), initial.version)
            .await
            .unwrap(),
        SaveOutcome::InvalidCmsLocale
    ));
    let SaveOutcome::Saved(saved) = save_general(&db, input(Some(" Vi-VN ")), initial.version)
        .await
        .unwrap()
    else {
        panic!("canonical CMS locale save failed");
    };
    assert_eq!(saved.cms_locale, "vi");
    assert_eq!(
        get_admin_i18n_bootstrap(&db)
            .await
            .unwrap()
            .effective_locale,
        "vi"
    );
    assert!(matches!(
        save_general(&db, input(Some("en")), initial.version)
            .await
            .unwrap(),
        SaveOutcome::Stale
    ));
    let legacy: GeneralSettingsInput = serde_json::from_value(serde_json::json!({
        "site_title": initial.site_title,
        "tagline": initial.tagline,
        "timezone": initial.timezone,
    }))
    .unwrap();
    let SaveOutcome::Saved(saved) = save_general(&db, legacy, saved.version).await.unwrap() else {
        panic!("legacy payload save failed");
    };
    assert_eq!(saved.cms_locale, "vi");
    sqlx::query(
        "INSERT INTO site_locales (code, bcp47_tag, label, direction, is_default, enabled, position)
         VALUES ('en', 'en', 'English', 'ltr', 0, 1, 1)",
    ).execute(&db).await.unwrap();
    let secondary = get_general_for_locale(&db, Some("en")).await.unwrap();
    assert_eq!(secondary.cms_locale, "vi");
    assert!(matches!(
        save_general_for_locale(&db, Some("en"), input(Some("en")), secondary.version)
            .await
            .unwrap(),
        SaveOutcome::SecondaryCmsLocaleLocked
    ));
    let SaveOutcome::Saved(secondary_saved) =
        save_general_for_locale(&db, Some("en"), input(Some(" VI ")), secondary.version)
            .await
            .unwrap()
    else {
        panic!("secondary save preserving CMS locale failed");
    };
    assert_eq!(secondary_saved.cms_locale, "vi");
    let SaveOutcome::Saved(saved) = save_general(&db, input(Some("ar")), saved.version)
        .await
        .unwrap()
    else {
        panic!("uninstalled CMS locale save failed");
    };
    assert_eq!(saved.cms_locale, "ar");
    let boot = get_admin_i18n_bootstrap(&db).await.unwrap();
    assert_eq!(boot.configured_locale, "ar");
    assert!(!boot.configured_installed);
    assert_eq!(
        (
            boot.effective_locale,
            boot.effective_bcp47,
            boot.effective_direction
        ),
        ("en", "en-US", "ltr")
    );
    assert_eq!(boot.available_locales.len(), 32);
}

#[tokio::test]
async fn global_site_options_crud_cas_empty_url_and_unified_theme_options() {
    let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
    crate::storage::content::preflight_builtin_key_collisions(&db)
        .await
        .unwrap();
    crate::storage::MIGRATOR.run(&db).await.unwrap();

    let snap = get_global_site_options(&db).await.unwrap();
    assert_eq!(snap.version, 1);
    assert_eq!(snap.fields.len(), 14);
    assert_eq!(snap.values.get("social_facebook").unwrap(), "");
    assert_eq!(snap.values.get("brand_primary_color").unwrap(), "#2563eb");

    // Stale CAS version returns Stale
    let stale = put_global_site_options(
        &db,
        99,
        BTreeMap::from([(
            "social_facebook".into(),
            serde_json::json!("https://fb.com/od"),
        )]),
    )
    .await
    .unwrap();
    assert!(matches!(stale, GlobalSiteOptionsOutcome::Stale));

    // Unsafe URL fails validation even though empty string is allowed for optional channels
    assert!(
        put_global_site_options(
            &db,
            1,
            BTreeMap::from([(
                "social_facebook".into(),
                serde_json::json!("javascript:alert(1)")
            )]),
        )
        .await
        .is_err()
    );
    assert!(
        put_global_site_options(
            &db,
            1,
            BTreeMap::from([("unknown_global".into(), serde_json::json!("x"))]),
        )
        .await
        .is_err()
    );

    let saved = put_global_site_options(
        &db,
        1,
        BTreeMap::from([
            (
                "social_facebook".into(),
                serde_json::json!("https://fb.com/opendash"),
            ),
            ("social_youtube".into(), serde_json::json!("")),
            ("contact_phone".into(), serde_json::json!("0901234567")),
        ]),
    )
    .await
    .unwrap();
    let GlobalSiteOptionsOutcome::Saved(saved) = saved else {
        panic!("expected Saved outcome");
    };
    assert_eq!(saved.version, 2);
    assert_eq!(
        saved.values.get("social_facebook").unwrap(),
        "https://fb.com/opendash"
    );
    assert_eq!(saved.values.get("contact_phone").unwrap(), "0901234567");

    // Public merged options expose saved global options even without an active theme.
    let (site_opts, public_opts) = get_active_public_theme_options(&db).await.unwrap();
    assert_eq!(
        site_opts.get("social_facebook").unwrap(),
        "https://fb.com/opendash"
    );
    assert_eq!(
        public_opts.get("social_facebook").unwrap(),
        "https://fb.com/opendash"
    );
    assert_eq!(public_opts.get("social_youtube").unwrap(), "");

    // Unified FieldSpec theme.json options parse and validate nested groups cleanly.
    let manifest = serde_json::json!({
        "options": [
            {"key": "headline", "label": "Headline", "type": "text", "default": "ok", "public": true},
            {"key": "accent", "label": "Màu nhấn", "type": "color", "default": "#2563eb", "public": true},
            {
                "key": "layout_group",
                "label": "Bố cục",
                "type": "group",
                "fields": [
                    {
                        "key": "header_style",
                        "label": "Kiểu Header",
                        "type": "radio_buttons",
                        "options": [{"value": "clean", "label": "Gọn"}, {"value": "full", "label": "Đầy"}],
                        "default": "clean",
                        "public": true
                    },
                    {"key": "optional_cta", "label": "CTA phụ", "type": "url", "default": "", "required": false, "public": true}
                ]
            }
        ]
    })
    .to_string();
    let fields = parse_theme_options_schema(&manifest).unwrap();
    assert_eq!(fields.len(), 3);
    let optional_cta_spec = &fields[2].fields[1];
    assert!(validate_url_or_link_spec_strict(optional_cta_spec, &serde_json::json!("")).is_ok());
    assert!(
        validate_url_or_link_spec_strict(
            optional_cta_spec,
            &serde_json::json!("javascript:alert(1)")
        )
        .is_err()
    );
}

#[tokio::test]
async fn public_theme_options_merge_locale_overlays_over_global_defaults() {
    let db = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    crate::storage::MIGRATOR.run(&db).await.unwrap();
    sqlx::query(
        "INSERT INTO site_locales (code, bcp47_tag, label, direction, is_default, enabled, position)
         VALUES ('en', 'en', 'English', 'ltr', 0, 1, 1)",
    )
    .execute(&db)
    .await
    .unwrap();
    let manifest = serde_json::json!({
        "manifest_version": 1,
        "theme_id": "t-l10n",
        "name": "T",
        "version": "1.0.0",
        "templates": [{ "kind": "index", "file": "index.html" }],
        "options": [
            { "key": "hero_title", "label": "Hero", "type": "text", "default": "Mac dinh", "public": true, "translatable": true },
            { "key": "cta_url", "label": "CTA", "type": "url", "default": "/vi", "public": true, "translatable": true },
            { "key": "accent", "label": "Accent", "type": "color", "default": "#111111", "public": true, "translatable": false }
        ]
    })
    .to_string();
    sqlx::query(
        "INSERT INTO theme_packages (theme_id, display_name, source_format)
         VALUES ('t-l10n', 'T', 'v1')",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO theme_revisions (revision_id, theme_id, version, revision_digest, source_format, manifest_json, file_digests_json, provenance_json)
         VALUES ('rev-1', 't-l10n', '1.0.0', 'dig-1', 'v1', ?, '{}', '{}')",
    )
    .bind(&manifest)
    .execute(&db)
    .await
    .unwrap();

    let mut tx = db.begin().await.unwrap();
    put_theme_options_in_conn_for_locale(
        &mut tx,
        "t-l10n",
        1,
        BTreeMap::from([
            ("hero_title".into(), serde_json::json!("Trang chu")),
            ("accent".into(), serde_json::json!("#222222")),
        ]),
        None,
    )
    .await
    .unwrap();
    put_theme_options_in_conn_for_locale(
        &mut tx,
        "t-l10n",
        1,
        BTreeMap::from([("cta_url".into(), serde_json::json!("/en/contact"))]),
        Some("en"),
    )
    .await
    .unwrap();
    let (_, vi_pub) =
        get_public_theme_options_in_conn_for_locale(&mut tx, "t-l10n", &manifest, None)
            .await
            .unwrap();
    let (_, en_pub) =
        get_public_theme_options_in_conn_for_locale(&mut tx, "t-l10n", &manifest, Some("en"))
            .await
            .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(vi_pub["hero_title"], serde_json::json!("Trang chu"));
    assert_eq!(vi_pub["cta_url"], serde_json::json!("/vi"));
    assert_eq!(vi_pub["accent"], serde_json::json!("#222222"));

    assert_eq!(en_pub["hero_title"], serde_json::json!("Trang chu"));
    assert_eq!(en_pub["cta_url"], serde_json::json!("/en/contact"));
    assert_eq!(en_pub["accent"], serde_json::json!("#222222"));
}

#[tokio::test]
async fn translatable_media_gallery_group_repeater_and_orphan_locale_cleanup_work() {
    let db = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    crate::storage::MIGRATOR.run(&db).await.unwrap();
    sqlx::query(
        "INSERT INTO site_locales (code, bcp47_tag, label, direction, is_default, enabled, position)
         VALUES ('en', 'en', 'English', 'ltr', 0, 1, 1)",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query("UPDATE site_locales_state SET multilingual_enabled = 1 WHERE id = 1")
        .execute(&db)
        .await
        .unwrap();

    let img_vi = "1".repeat(64);
    let img_en = "2".repeat(64);
    let img_gal = "3".repeat(64);
    let img_rep = "4".repeat(64);
    for (idx, mid) in [&img_vi, &img_en, &img_gal, &img_rep]
        .into_iter()
        .enumerate()
    {
        sqlx::query(
            "INSERT INTO media_assets (id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height, lifecycle_state)
             VALUES (?, 'image', ?, 'image/png', 'png', 100, 10, 10, 10, 10, 'active')",
        )
        .bind(mid)
        .bind(format!("loc-{idx}.png"))
        .execute(&db)
        .await
        .unwrap();
    }

    let manifest = serde_json::json!({
        "manifest_version": 1,
        "theme_id": "t-media-l10n",
        "name": "TM",
        "version": "1.0.0",
        "templates": [{ "kind": "index", "file": "index.html" }],
        "options": [
            { "key": "hero_img", "label": "HeroImg", "type": "media", "public": true, "translatable": true },
            { "key": "gallery_imgs", "label": "Gallery", "type": "gallery", "public": true, "translatable": true },
            {
                "key": "promo_box",
                "label": "Promo",
                "type": "group",
                "public": true,
                "translatable": true,
                "fields": [
                    { "key": "bg", "label": "BG", "type": "media" },
                    { "key": "cta", "label": "CTA", "type": "link" }
                ]
            },
            {
                "key": "slides",
                "label": "Slides",
                "type": "repeater",
                "public": true,
                "translatable": true,
                "fields": [
                    { "key": "photo", "label": "Photo", "type": "media" }
                ]
            }
        ]
    })
    .to_string();
    sqlx::query(
        "INSERT INTO theme_packages (theme_id, display_name, source_format)
         VALUES ('t-media-l10n', 'TM', 'v1')",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO theme_revisions (revision_id, theme_id, version, revision_digest, source_format, manifest_json, file_digests_json, provenance_json)
         VALUES ('rev-tm', 't-media-l10n', '1.0.0', 'dig-tm', 'v1', ?, '{}', '{}')",
    )
    .bind(&manifest)
    .execute(&db)
    .await
    .unwrap();
    sqlx::query("UPDATE theme_active_selection SET revision_id = 'rev-tm' WHERE id = 1")
        .execute(&db)
        .await
        .unwrap();

    let mut tx = db.begin().await.unwrap();
    put_theme_options_in_conn_for_locale(
        &mut tx,
        "t-media-l10n",
        1,
        BTreeMap::from([("hero_img".into(), serde_json::json!(img_vi))]),
        None,
    )
    .await
    .unwrap();
    put_theme_options_in_conn_for_locale(
        &mut tx,
        "t-media-l10n",
        1,
        BTreeMap::from([
            ("hero_img".into(), serde_json::json!(img_en)),
            ("gallery_imgs".into(), serde_json::json!([img_gal])),
            (
                "promo_box".into(),
                serde_json::json!({ "bg": img_en, "cta": { "url": "/en", "label": "Go", "new_tab": false, "nofollow": false } }),
            ),
            ("slides".into(), serde_json::json!([{ "photo": img_rep }])),
        ]),
        Some("en"),
    )
    .await
    .unwrap();

    let (_, vi_pub) =
        get_public_theme_options_in_conn_for_locale(&mut tx, "t-media-l10n", &manifest, None)
            .await
            .unwrap();
    let (_, en_pub) =
        get_public_theme_options_in_conn_for_locale(&mut tx, "t-media-l10n", &manifest, Some("en"))
            .await
            .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(vi_pub["hero_img"]["id"], serde_json::json!(img_vi));
    assert_eq!(en_pub["hero_img"]["id"], serde_json::json!(img_en));
    assert_eq!(en_pub["gallery_imgs"][0]["id"], serde_json::json!(img_gal));
    assert_eq!(en_pub["promo_box"]["bg"]["id"], serde_json::json!(img_en));
    assert_eq!(
        en_pub["slides"][0]["photo"]["id"],
        serde_json::json!(img_rep)
    );

    let ref_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT option_key, locale FROM theme_option_media_refs WHERE owner_id = 't-media-l10n' AND media_id = ? ORDER BY option_key, locale",
    )
    .bind(&img_en)
    .fetch_all(&db)
    .await
    .unwrap();
    assert_eq!(
        ref_rows,
        vec![
            ("hero_img".to_owned(), "en".to_owned()),
            ("promo_box".to_owned(), "en".to_owned()),
        ]
    );

    // Soft-delete 'en' and verify count_locale_data_in_conn + purge_orphan_locale mirror each other cleanly
    sqlx::query("UPDATE site_locales SET enabled = 0, deleted_at = '2026-01-01T00:00:00Z' WHERE code = 'en'")
        .execute(&db)
        .await
        .unwrap();
    let mut ctx = db.acquire().await.unwrap();
    let before_counts = count_locale_data_in_conn(&mut ctx, "en").await.unwrap();
    drop(ctx);
    assert!(before_counts.settings > 0);
    let orphans = list_orphaned_locales(&db).await.unwrap();
    let purged = purge_orphan_locale(
        &db,
        "en",
        orphans.version,
        PurgeOrphanLocaleInput {
            confirm_code: "en".to_owned(),
        },
    )
    .await
    .unwrap();
    assert_eq!(purged.deleted.settings, before_counts.settings);

    let default_refs_left: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM theme_option_media_refs WHERE owner_id = 't-media-l10n' AND locale = '' AND media_id = ?",
    )
    .bind(&img_vi)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(default_refs_left, 1);
}
