use std::{collections::BTreeMap, fs, path::Path};

use opendash_core::{
    block_schema::{BlockSchema, RegisteredBlockSchema, validate_block_catalog_templates},
    builder::{
        BlockNode, BuilderContextTarget, BuilderDocument, NodeMeta, inspect_loop_projection,
    },
    registry::{RegistryCatalog, ResourceOwner},
    theme::TemplateCandidateKind,
};
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use super::studio_unified_ast::AdminSession;

#[test]
fn s03_core_block_catalog_and_loop_validation() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let blocks_dir = manifest_dir.join("src/builtin_blocks");
    assert!(
        !blocks_dir.join("content-slot").exists(),
        "core/content-slot must be completely deleted in S03"
    );

    let mut catalog = RegistryCatalog::<RegisteredBlockSchema>::new();
    let mut dirs = Vec::new();
    for entry in fs::read_dir(&blocks_dir).unwrap() {
        let entry = entry.unwrap();
        if entry.path().is_dir() {
            dirs.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    dirs.sort();
    assert_eq!(dirs.len(), 48, "expected 48 core blocks, found {dirs:?}");

    for dir in &dirs {
        let raw = fs::read_to_string(blocks_dir.join(dir).join("block.json")).unwrap();
        let schema: BlockSchema =
            serde_json::from_str(&raw).unwrap_or_else(|e| panic!("invalid {dir}: {e}"));
        schema
            .validate("core")
            .unwrap_or_else(|e| panic!("schema {dir} invalid: {e}"));
        catalog
            .register(
                schema.id.clone(),
                ResourceOwner::Core,
                RegisteredBlockSchema { schema },
            )
            .unwrap();
    }
    validate_block_catalog_templates(&catalog).unwrap();

    let archive_loop = catalog.get("core/archive-loop").unwrap();
    assert_eq!(archive_loop.descriptor.schema.presets.len(), 4);

    let tpl_blocks = [
        "core/site-brand",
        "core/language-switcher",
        "core/menu",
        "core/breadcrumbs",
        "core/entry-title",
        "core/entry-featured-image",
        "core/entry-meta",
        "core/entry-excerpt",
        "core/entry-content",
        "core/entry-terms",
        "core/entry-field",
        "core/entry-related",
        "core/entry-nav",
        "core/archive-title",
        "core/archive-description",
        "core/archive-featured-image",
        "core/archive-loop",
        "core/loop-item",
        "core/archive-pagination",
    ];
    for bid in tpl_blocks {
        let node = BlockNode {
            id: "n1".into(),
            block_id: bid.into(),
            props: BTreeMap::new(),
            children: Vec::new(),
            meta: None,
        };
        let doc = BuilderDocument {
            schema_version: 1,
            blocks: vec![node],
        };
        assert!(
            doc.validate_for_save(
                &catalog,
                &BuilderContextTarget::EntryBody {
                    content_type: "post".into(),
                },
                None,
            )
            .is_err(),
            "Mode 2 must reject template block {bid}"
        );
    }

    let loop_doc = serde_json::from_value::<BuilderDocument>(json!({
        "schema_version": 1,
        "blocks": [{
            "id": "al1",
            "block_id": "core/archive-loop",
            "children": [{
                "id": "li1",
                "block_id": "core/loop-item",
                "children": [
                    { "id": "ef1", "block_id": "core/entry-featured-image" },
                    { "id": "ef2", "block_id": "core/entry-field", "props": { "field_key": "price" } }
                ]
            }]
        }]
    }))
    .unwrap();
    let proj = inspect_loop_projection(&loop_doc);
    assert!(proj.needs_cover);
    assert!(proj.custom_field_keys.contains("price"));

    let bad_html_id_loop = BuilderDocument {
        schema_version: 1,
        blocks: vec![BlockNode {
            id: "al1".into(),
            block_id: "core/archive-loop".into(),
            props: BTreeMap::new(),
            children: vec![BlockNode {
                id: "li1".into(),
                block_id: "core/loop-item".into(),
                props: BTreeMap::new(),
                children: vec![BlockNode {
                    id: "t1".into(),
                    block_id: "core/entry-title".into(),
                    props: BTreeMap::new(),
                    children: Vec::new(),
                    meta: Some(NodeMeta {
                        html_id: Some("dup-id".into()),
                        ..Default::default()
                    }),
                }],
                meta: None,
            }],
            meta: None,
        }],
    };
    assert!(
        bad_html_id_loop
            .validate_for_save(
                &catalog,
                &BuilderContextTarget::Template {
                    kind: TemplateCandidateKind::Archive,
                    content_type: Some("post".into()),
                    taxonomy: None,
                    part_key: None,
                },
                None,
            )
            .is_err()
    );
}

#[tokio::test]
async fn s03_template_blocks_loops_xss_preview_and_public_routes_e2e() {
    let app = AdminSession::boot().await;
    let client = &app.http;

    // 1. Studio targets expose taxonomies group and entry_content_rule
    let targets: Value = app
        .send_json(Method::GET, "/api/admin/studio/targets", None, None)
        .await
        .json()
        .await
        .unwrap();
    let items = targets["items"].as_array().unwrap();
    assert!(items.iter().any(
        |i| i["group"] == "taxonomies" && i["canonical_target_key"] == "taxonomy:key:category"
    ));
    assert_eq!(
        items
            .iter()
            .find(|i| i["canonical_target_key"] == "page")
            .unwrap()["entry_content_rule"],
        "optional_max_one"
    );

    // 2. Mode 2 rejects template blocks
    let page_for_mode2 = app
        .create_entry("page", "Mode2 Reject", "mode2-reject")
        .await;
    let bad_mode2 = app
        .send_json(
            Method::PUT,
            &format!("/api/admin/entries/page/{page_for_mode2}/builder"),
            Some("\"v1\""),
            Some(&json!({
                "document": {
                    "schema_version": 1,
                    "blocks": [{
                        "id": "s1", "block_id": "core/section",
                        "children": [{
                            "id": "r1", "block_id": "core/row",
                            "children": [{
                                "id": "c1", "block_id": "core/column",
                                "children": [{ "id": "et1", "block_id": "core/entry-title" }]
                            }]
                        }]
                    }]
                }
            })),
        )
        .await;
    assert_eq!(bad_mode2.status(), StatusCode::BAD_REQUEST);

    // 3. Create custom CPT product + ACF fields + draft/published entries
    let ct_res = app
        .send_json(
            Method::POST,
            "/api/admin/content-types",
            None,
            Some(&json!({
                "key": "product",
                "label": "Products",
                "singular_label": "Product",
                "description": "San pham thu cong my nghe",
                "rewrite_base": "san-pham",
                "settings": {
                    "public": true,
                    "has_archive": true,
                    "publication": "draft_publish",
                    "supports": ["editor", "featured_image", "excerpt"]
                }
            })),
        )
        .await;
    assert_eq!(ct_res.status(), StatusCode::CREATED);
    let ct_created: Value = ct_res.json().await.unwrap();
    let ct_ver = ct_created["version"].as_i64().unwrap_or(1);
    assert_eq!(
        app.send_json(
            Method::PUT,
            "/api/admin/taxonomy/eligibility/product",
            Some(&format!("\"v{ct_ver}\"")),
            Some(&json!({ "taxonomies": ["category"], "expected_version": ct_ver })),
        )
        .await
        .status(),
        StatusCode::OK
    );
    let cat_term_res = app
        .send_json(
            Method::POST,
            "/api/admin/taxonomy/terms",
            None,
            Some(&json!({ "kind": "category", "slug": "thu-cong", "name": "Thu cong", "locale": "vi" })),
        )
        .await;
    assert_eq!(cat_term_res.status(), StatusCode::CREATED);
    let cat_term_id = cat_term_res.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    let fg_res = app
        .send_json(
            Method::POST,
            "/api/admin/content-field-groups",
            None,
            Some(&json!({
                "local_key": "product_fg",
                "title": "Product Fields",
                "location": [[{ "param": "content_type", "op": "eq", "value": "product" }]],
                "fields": [
                    { "key": "price", "label": "Price", "type": "number" },
                    { "key": "site_url", "label": "Site", "type": "url" },
                    { "key": "tagline_txt", "label": "Note", "type": "text" },
                    {
                        "key": "secret_note",
                        "label": "Secret",
                        "type": "text",
                        "acl": { "min_view_role": "admin", "min_edit_role": "admin", "public_exposure": "private" }
                    },
                    { "key": "long_desc", "label": "Spec", "type": "richtext" }
                ]
            })),
        )
        .await;
    assert_eq!(fg_res.status(), StatusCode::CREATED);

    let draft_img = app.upload_png("draft-product.png").await;
    let create_prod = |title: &str, slug: &str, values: Value| {
        let app = &app;
        let payload =
            json!({ "title": title, "slug": slug, "values": values, "term_ids": [cat_term_id] });
        async move {
            app.send_json(
                Method::POST,
                "/api/admin/content-types/product/entries",
                None,
                Some(&payload),
            )
            .await
        }
    };
    let draft_prod: Value = create_prod(
        "Draft Product",
        "draft-product",
        json!({ "featured_image": draft_img, "price": 99000 }),
    )
    .await
    .json()
    .await
    .unwrap();
    let draft_id = draft_prod["id"].as_i64().unwrap();

    // 4. Archive forbids core/entry-content (400), while single:type:product saves without it using core/entry-* (201)
    let archive_with_content = app
        .send_json(
            Method::PUT,
            "/api/admin/template-overrides",
            None,
            Some(&json!({
                "target": { "kind": "archive", "content_type": "product" },
                "document": {
                    "schema_version": 1,
                    "blocks": [{
                        "id": "a1", "block_id": "core/section",
                        "children": [{ "id": "r1", "block_id": "core/row", "children": [{ "id": "c1", "block_id": "core/column", "children": [{ "id": "ec", "block_id": "core/entry-content" }] }] }]
                    }]
                }
            })),
        )
        .await;
    assert_eq!(archive_with_content.status(), StatusCode::BAD_REQUEST);

    let single_doc = json!({
        "schema_version": 1,
        "blocks": [{
            "id": "s-sec", "block_id": "core/section",
            "children": [{
                "id": "s-row", "block_id": "core/row",
                "children": [{
                    "id": "s-col", "block_id": "core/column",
                    "children": [
                        { "id": "s-bc", "block_id": "core/breadcrumbs" },
                        { "id": "s-title", "block_id": "core/entry-title" },
                        { "id": "s-img", "block_id": "core/entry-featured-image" },
                        { "id": "s-price", "block_id": "core/entry-field", "props": { "field_key": "price", "display_format": "auto" } },
                        { "id": "s-url", "block_id": "core/entry-field", "props": { "field_key": "site_url", "display_format": "link" } },
                        { "id": "s-tag", "block_id": "core/entry-field", "props": { "field_key": "tagline_txt", "display_format": "link" } },
                        { "id": "s-secr", "block_id": "core/entry-field", "props": { "field_key": "secret_note" } },
                        { "id": "s-nav", "block_id": "core/entry-nav" },
                        {
                            "id": "s-rel",
                            "block_id": "core/entry-related",
                            "props": { "limit": 2 },
                            "children": [{
                                "id": "s-rel-li",
                                "block_id": "core/loop-item",
                                "children": [{
                                    "id": "s-rel-t",
                                    "block_id": "core/entry-title",
                                    "props": { "tag": "h3", "link_to_entry": true }
                                }]
                            }]
                        }
                    ]
                }]
            }]
        }]
    });
    assert_eq!(
        app.send_json(
            Method::PUT,
            "/api/admin/template-overrides",
            None,
            Some(&json!({ "target": { "kind": "single", "content_type": "product" }, "document": single_doc })),
        )
        .await
        .status(),
        StatusCode::CREATED
    );

    // 5. Studio preview renders draft media while anonymous GET /media/{id}/file is 404
    let studio_prev = app
        .send_json(
            Method::POST,
            "/api/admin/studio/preview",
            None,
            Some(&json!({
                "target": { "kind": "single", "content_type": "product" },
                "preview_entry": { "type_key": "product", "entry_id": draft_id },
                "preview_locale": "vi"
            })),
        )
        .await;
    assert_eq!(studio_prev.status(), StatusCode::OK);
    let studio_prev_html = studio_prev.json::<Value>().await.unwrap()["html"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        studio_prev_html.contains(&format!("/api/admin/media/{draft_img}/")),
        "expected admin preview media url in {studio_prev_html}"
    );
    assert_eq!(
        client
            .get(format!("{}/media/{draft_img}/file", app.site.2))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    // 6. Publish 3 products and verify XSS / private ACL / no DEFAULT_SINGLE / nav / related
    let long_html = format!("<p>{}</p>", "Kien truc thu cong ".repeat(30));
    let bad_js_url = create_prod(
        "Blocked JS URL",
        "blocked-js-url",
        json!({ "site_url": "javascript:alert(1)" }),
    )
    .await;
    assert_eq!(bad_js_url.status(), StatusCode::BAD_REQUEST);

    for (idx, (title, slug, url)) in [
        ("Product One", "prod-1", "https://example.com/p1"),
        ("Product Two", "prod-2", "https://example.com/p2"),
        ("Product Three", "prod-3", "https://example.com/p3"),
    ]
    .iter()
    .enumerate()
    {
        let resp = create_prod(
            title,
            slug,
            json!({
                "price": 100000 + (idx as i64) * 50000,
                "site_url": url,
                "tagline_txt": "<script>alert(1)</script>",
                "secret_note": "TOP_SECRET_TOKEN",
                "long_desc": long_html
            }),
        )
        .await;
        let status = resp.status();
        let body_text = resp.text().await.unwrap();
        assert_eq!(
            status,
            StatusCode::CREATED,
            "create_prod {slug} failed: {body_text}"
        );
        let created: Value = serde_json::from_str(&body_text).unwrap();
        let pid = created["id"].as_i64().unwrap();
        assert_eq!(
            app.send_json(
                Method::POST,
                &format!("/api/admin/content-types/product/entries/{pid}/publication"),
                Some("\"v1\""),
                Some(&json!({ "status": "published" })),
            )
            .await
            .status(),
            StatusCode::OK
        );
    }

    let pub_p2 = client
        .get(format!("{}/san-pham/prod-2", app.site.2))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        pub_p2.contains("Product Two")
            && (pub_p2.contains("150.000") || pub_p2.contains("150,000")),
        "unexpected single product html: {pub_p2}"
    );
    assert!(!pub_p2.contains("TOP_SECRET_TOKEN"));
    assert!(!pub_p2.contains("<script>alert(1)</script>"));
    assert!(pub_p2.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(
        !pub_p2.contains("<dl>"),
        "unexpected fallback <dl> in {pub_p2}"
    );
    assert!(
        pub_p2.contains("od-entry-nav"),
        "missing od-entry-nav in {pub_p2}"
    );
    assert!(
        pub_p2.contains("od-entry-related"),
        "missing od-entry-related in {pub_p2}"
    );

    // 7. Archive override with CPT description, SVG <symbol> dedupe, od-lt-* CSS, loop clone guard, richtext clamp
    let arch_doc = json!({
        "schema_version": 1,
        "blocks": [{
            "id": "ar-sec", "block_id": "core/section",
            "children": [{
                "id": "ar-row", "block_id": "core/row",
                "children": [{
                    "id": "ar-col", "block_id": "core/column",
                    "children": [
                        { "id": "ar-title", "block_id": "core/archive-title" },
                        { "id": "ar-desc", "block_id": "core/archive-description" },
                        {
                            "id": "ar-loop", "block_id": "core/archive-loop",
                            "children": [{
                                "id": "li-root", "block_id": "core/loop-item",
                                "props": { "layout": "vertical" },
                                "children": [
                                    { "id": "li-title", "block_id": "core/entry-title", "props": { "tag": "h2", "color": "#111111", "link_to_entry": true } },
                                    { "id": "li-meta", "block_id": "core/entry-meta", "props": { "items": ["published_date", "reading_time"], "show_icons": true } },
                                    { "id": "li-spec", "block_id": "core/entry-field", "props": { "field_key": "long_desc", "display_format": "auto" } }
                                ]
                            }]
                        },
                        { "id": "ar-pag", "block_id": "core/archive-pagination" }
                    ]
                }]
            }]
        }]
    });
    assert_eq!(
        app.send_json(
            Method::PUT,
            "/api/admin/template-overrides",
            None,
            Some(&json!({ "target": { "kind": "archive", "content_type": "product" }, "document": arch_doc })),
        )
        .await
        .status(),
        StatusCode::CREATED
    );
    let arch_html = client
        .get(format!("{}/san-pham", app.site.2))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(arch_html.contains("San pham thu cong my nghe"));
    assert_eq!(arch_html.matches("id=\"od-sym-clock\"").count(), 1);
    assert!(arch_html.matches("href=\"#od-sym-clock\"").count() >= 3);
    assert!(arch_html.contains("od-lt-ov-li-root-li-title"));
    assert_eq!(arch_html.matches("data-node-id=\"li-title\"").count(), 1);
    assert!(!arch_html.contains("OD_RICHTEXT_"));

    let arch_prev = app
        .send_json(
            Method::POST,
            "/api/admin/studio/preview",
            None,
            Some(&json!({
                "target": { "kind": "archive", "content_type": "product" },
                "preview_annotations": true,
                "preview_locale": "vi"
            })),
        )
        .await;
    assert_eq!(arch_prev.status(), StatusCode::OK);
    let arch_prev_html = arch_prev.json::<Value>().await.unwrap()["html"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        arch_prev_html.matches("data-node-id=\"li-title\"").count(),
        1
    );
    assert!(arch_prev_html.matches("data-od-loop-clone=\"1\"").count() >= 2);

    // 8. Header part (site-brand + 0-JS menu + language-switcher) on / and core/list with core/loop-item on page
    let menu_res: Value = app
        .send_json(
            Method::POST,
            "/api/admin/menus",
            None,
            Some(&json!({ "menu_key": "nav-header", "title": "Header Nav" })),
        )
        .await
        .json()
        .await
        .unwrap();
    let mid = menu_res["menu"]["id"].as_i64().unwrap();
    assert_eq!(
        app.send_json(
            Method::PUT,
            &format!("/api/admin/menus/{mid}"),
            Some("\"v1\""),
            Some(&json!({
                "title": "Header Nav",
                "items": [{
                    "id": null, "client_key": "m1", "parent_id": null, "parent_client_key": null,
                    "position": 0, "label": "Khuyen Mai", "target_kind": "external",
                    "target_entry_id": null, "target_term_id": null, "external_url": "https://example.com/san-pham"
                }]
            })),
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        app.send_json(
            Method::PUT,
            "/api/admin/template-overrides",
            None,
            Some(&json!({
                "target": { "kind": "part", "part_key": "header" },
                "document": {
                    "schema_version": 1,
                    "blocks": [{
                        "id": "hd-sec", "block_id": "core/section",
                        "children": [{
                            "id": "hd-row", "block_id": "core/row",
                            "children": [{
                                "id": "hd-col", "block_id": "core/column",
                                "children": [
                                    { "id": "hd-brand", "block_id": "core/site-brand" },
                                    { "id": "hd-menu", "block_id": "core/menu", "props": { "source_mode": "menu", "menu_key": "nav-header", "mobile_drawer": true } },
                                    { "id": "hd-lang", "block_id": "core/language-switcher" }
                                ]
                            }]
                        }]
                    }]
                }
            })),
        )
        .await
        .status(),
        StatusCode::CREATED
    );
    let home_html = client
        .get(format!("{}/", app.site.2))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(home_html.contains("od-site-brand") && home_html.contains("Khuyen Mai"));
    assert!(home_html.contains("peer-checked:flex"));

    // Page template override with core/entry-content + core/list (core/loop-item child)
    let page_res: Value = app
        .send_json(
            Method::POST,
            "/api/admin/content-types/page/entries",
            None,
            Some(&json!({
                "title": "Landing Showcase",
                "slug": "landing-showcase",
                "values": { "body": "<p>Landing Intro Fragment</p>" }
            })),
        )
        .await
        .json()
        .await
        .unwrap();
    let landing_id = page_res["id"].as_i64().unwrap();
    assert_eq!(
        app.send_json(
            Method::POST,
            &format!("/api/admin/content-types/page/entries/{landing_id}/publication"),
            Some("\"v1\""),
            Some(&json!({ "status": "published" })),
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        app.send_json(
            Method::PUT,
            "/api/admin/template-overrides",
            None,
            Some(&json!({
                "target": { "kind": "page" },
                "document": {
                    "schema_version": 1,
                    "blocks": [{
                        "id": "pg-sec", "block_id": "core/section",
                        "children": [{
                            "id": "pg-row", "block_id": "core/row",
                            "children": [{
                                "id": "pg-col", "block_id": "core/column",
                                "children": [
                                    { "id": "pg-ec", "block_id": "core/entry-content" },
                                    {
                                        "id": "pg-list", "block_id": "core/list",
                                        "props": { "type_key": "product", "page_size": 2 },
                                        "children": [{
                                            "id": "pg-li", "block_id": "core/loop-item",
                                            "children": [{ "id": "pg-lt", "block_id": "core/entry-title", "props": { "tag": "h3" } }]
                                        }]
                                    }
                                ]
                            }]
                        }]
                    }]
                }
            })),
        )
        .await
        .status(),
        StatusCode::CREATED
    );
    let landing_html = client
        .get(format!("{}/landing-showcase", app.site.2))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(landing_html.contains("Landing Intro Fragment"));
    let body_html = landing_html.split("<body").nth(1).unwrap_or("");
    assert!(
        !body_html.contains("Landing Showcase") && !body_html.contains("<article>"),
        "DEFAULT_SINGLE must not leak into body: {body_html}"
    );
    assert!(landing_html.contains("Product Three") || landing_html.contains("Product One"));
}
