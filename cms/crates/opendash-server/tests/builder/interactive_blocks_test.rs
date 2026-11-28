use std::{collections::BTreeMap, fs, path::Path};

use opendash_core::{
    block_schema::{BlockSchema, RegisteredBlockSchema},
    builder::{BlockNode, BuilderContextTarget, BuilderDocument},
    icons::lookup_core_icon_paths,
    registry::{RegistryCatalog, ResourceOwner},
    theme::TemplateCandidateKind,
};
use reqwest::{Method, StatusCode, header};
use serde_json::{Value, json};

use super::studio_unified_ast::AdminSession;

fn core_block_catalog() -> RegistryCatalog<RegisteredBlockSchema> {
    let blocks_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/builtin_blocks");
    let mut dirs: Vec<String> = fs::read_dir(&blocks_dir)
        .unwrap()
        .filter_map(|e| {
            let p = e.ok()?.path();
            p.is_dir()
                .then(|| p.file_name().unwrap().to_string_lossy().into_owned())
        })
        .collect();
    dirs.sort();
    assert_eq!(dirs.len(), 48, "expected 48 core blocks");

    let mut catalog = RegistryCatalog::new();
    for dir in &dirs {
        let raw = fs::read_to_string(blocks_dir.join(dir).join("block.json")).unwrap();
        let schema: BlockSchema =
            serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{dir}: {e}"));
        schema
            .validate("core")
            .unwrap_or_else(|e| panic!("invalid {dir}: {e}"));
        for preset in &schema.presets {
            if let Some(icon) = preset.icon.as_deref() {
                assert!(
                    lookup_core_icon_paths(icon).is_some(),
                    "{} preset {} icon '{icon}' missing from CORE_ICONS",
                    schema.id,
                    preset.key,
                );
            }
        }
        catalog
            .register(
                schema.id.clone(),
                ResourceOwner::Core,
                RegisteredBlockSchema { schema },
            )
            .unwrap();
    }
    catalog
}

fn node(id: &str, block_id: &str, props: Value, children: Vec<BlockNode>) -> BlockNode {
    let props_map = match props {
        Value::Object(map) => map.into_iter().collect(),
        _ => BTreeMap::new(),
    };
    BlockNode {
        id: id.into(),
        block_id: block_id.into(),
        props: props_map,
        children,
        meta: None,
    }
}

#[test]
fn s02_interactive_block_catalog_and_nesting_validation() {
    let catalog = core_block_catalog();
    let page_ctx = BuilderContextTarget::EntryBody {
        content_type: "page".into(),
    };

    for bid in [
        "core/tabs",
        "core/tab-panel",
        "core/slider",
        "core/slide",
        "core/gallery",
        "core/social-share",
        "core/popup",
    ] {
        let doc = BuilderDocument {
            schema_version: 1,
            blocks: vec![node(
                "list-1",
                "core/list",
                json!({ "type_key": "post" }),
                vec![node(
                    "li-1",
                    "core/loop-item",
                    json!({}),
                    vec![node("bad-child", bid, json!({}), vec![])],
                )],
            )],
        };
        assert!(
            doc.validate_for_save(&catalog, &page_ctx, None).is_err(),
            "{bid} must be rejected inside core/loop-item"
        );
    }

    let direct_nested = BuilderDocument {
        schema_version: 1,
        blocks: vec![node(
            "pop-1",
            "core/popup",
            json!({ "popup_id": "outer" }),
            vec![node(
                "pop-2",
                "core/popup",
                json!({ "popup_id": "inner" }),
                vec![],
            )],
        )],
    };
    assert!(
        direct_nested
            .validate_for_save(&catalog, &page_ctx, None)
            .is_err()
    );

    let transitive_nested = BuilderDocument {
        schema_version: 1,
        blocks: vec![node(
            "pop-1",
            "core/popup",
            json!({ "popup_id": "outer" }),
            vec![node(
                "stk-1",
                "core/stack",
                json!({}),
                vec![node(
                    "pop-2",
                    "core/popup",
                    json!({ "popup_id": "inner" }),
                    vec![],
                )],
            )],
        )],
    };
    assert!(
        transitive_nested
            .validate_for_save(&catalog, &page_ctx, None)
            .is_err()
    );

    let popup_doc = BuilderDocument {
        schema_version: 1,
        blocks: vec![node(
            "pop-hdr",
            "core/popup",
            json!({ "popup_id": "mobile-menu", "position": "left" }),
            vec![node(
                "hdr-txt",
                "core/text",
                json!({ "content": "<p>Menu</p>" }),
                vec![],
            )],
        )],
    };
    for part_key in ["header", "footer"] {
        let ctx = BuilderContextTarget::Template {
            kind: TemplateCandidateKind::Part,
            content_type: None,
            taxonomy: None,
            part_key: Some(part_key.into()),
        };
        popup_doc
            .validate_for_save(&catalog, &ctx, None)
            .unwrap_or_else(|e| panic!("popup allowed in part:{part_key}: {e}"));
    }
    popup_doc.validate_for_save(&catalog, &page_ctx, None).unwrap();
}

async fn upload_png_rgb(app: &AdminSession, name: &str, rgba: [u8; 4]) -> String {
    let mut png = Vec::new();
    let img = image::RgbaImage::from_pixel(4, 4, image::Rgba(rgba));
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let res = app
        .auth(Method::POST, "/api/admin/media")
        .header(header::CONTENT_TYPE, "image/png")
        .header("X-File-Name", name)
        .body(png)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    res.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn s02_interactive_blocks_assets_conditional_js_csp_and_popup_http_e2e() {
    let app = AdminSession::boot().await;
    let img1 = upload_png_rgb(&app, "g1.png", [37, 99, 235, 255]).await;
    let img2 = upload_png_rgb(&app, "g2.png", [16, 185, 129, 255]).await;

    let js_res = app
        .http
        .get(format!("{}/assets/opendash-blocks.js", app.site.2))
        .send()
        .await
        .unwrap();
    assert_eq!(js_res.status(), StatusCode::OK);
    assert_eq!(
        js_res.headers()[header::CONTENT_TYPE],
        "text/javascript; charset=utf-8"
    );
    assert_eq!(js_res.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    let cc = js_res.headers()[header::CACHE_CONTROL]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(cc.contains("max-age=3600"), "cache-control={cc}");
    let js_body = js_res.text().await.unwrap();
    assert!(
        js_body.len() < 12288,
        "opendash-blocks.js size {} >= 12288",
        js_body.len()
    );

    let css_res = app
        .http
        .get(format!("{}/assets/opendash-tailwind.css", app.site.2))
        .send()
        .await
        .unwrap();
    assert_eq!(css_res.status(), StatusCode::OK);
    let css_body = css_res.text().await.unwrap();
    assert!(
        css_body.len() < 122880,
        "opendash-tailwind.css size {} >= 122880",
        css_body.len()
    );
    assert!(css_body.contains(".od-popup-dialog"));
    assert!(css_body.contains(".od-slider-track"));

    let no_js_id = app
        .create_entry("page", "No JS Variants", "no-js-interactive")
        .await;
    let no_js_doc = json!({
        "schema_version": 1,
        "blocks": [{
            "id": "sec-nojs",
            "block_id": "core/section",
            "children": [
                {
                    "id": "tabs-1",
                    "block_id": "core/tabs",
                    "children": [{
                        "id": "tp-1",
                        "block_id": "core/tab-panel",
                        "props": { "title": "Solo Tab" },
                        "children": [{ "id": "txt-1", "block_id": "core/text", "props": { "content": "<p>Solo</p>" } }]
                    }]
                },
                {
                    "id": "sld-1",
                    "block_id": "core/slider",
                    "children": [{
                        "id": "sl-1",
                        "block_id": "core/slide",
                        "children": [{ "id": "hd-1", "block_id": "core/heading", "props": { "level": "h3", "text": "Only Slide" } }]
                    }]
                },
                {
                    "id": "gal-no-lb",
                    "block_id": "core/gallery",
                    "props": { "images": [img1], "lightbox": false }
                },
                {
                    "id": "sh-abs",
                    "block_id": "core/social-share",
                    "props": { "custom_url": "https://example.org/p", "show_copy_link": false }
                },
                {
                    "id": "pop-bad",
                    "block_id": "core/popup",
                    "props": { "popup_id": "123-bad" }
                }
            ]
        }]
    });
    let no_js_put = app
        .send_json(
            Method::PUT,
            &format!("/api/admin/entries/page/{no_js_id}/builder"),
            Some("\"v1\""),
            Some(&no_js_doc),
        )
        .await;
    let no_js_status = no_js_put.status();
    let no_js_body = no_js_put.text().await.unwrap_or_default();
    assert_eq!(no_js_status, StatusCode::OK, "no_js PUT failed: {no_js_body}");
    let prev_nojs: Value = app
        .send_json(
            Method::POST,
            &format!("/api/admin/entries/page/{no_js_id}/builder/preview"),
            None,
            Some(&no_js_doc),
        )
        .await
        .json()
        .await
        .unwrap();
    let prev_nojs_html = prev_nojs["html"].as_str().unwrap();
    assert!(prev_nojs_html.contains("od-popup-preview-invalid"));
    assert!(!prev_nojs_html.contains("/assets/opendash-blocks.js"));

    assert_eq!(
        app.send_json(
            Method::POST,
            &format!("/api/admin/content-types/page/entries/{no_js_id}/publication"),
            Some("\"v2\""),
            Some(&json!({ "status": "published" }))
        )
        .await
        .status(),
        StatusCode::OK
    );
    let pub_nojs = app
        .http
        .get(format!("{}/no-js-interactive", app.site.2))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!pub_nojs.contains("/assets/opendash-blocks.js"));
    assert!(!pub_nojs.contains("od-lightbox-dialog"));
    assert!(!pub_nojs.contains("123-bad"));
    assert!(pub_nojs.contains("https%3A%2F%2Fexample.org%2Fp"));

    let pat_id = app.create_pattern("Popup Pattern", "pat-popup").await;
    let pat_doc = json!({
        "schema_version": 1,
        "blocks": [{
            "id": "pat-pop",
            "block_id": "core/popup",
            "props": { "popup_id": "inner-pop" },
            "children": [{ "id": "pat-tx", "block_id": "core/text", "props": { "content": "<p>Inner</p>" } }]
        }]
    });
    let pat_put = app
        .send_json(
            Method::PUT,
            &format!("/api/admin/entries/pattern/{pat_id}/builder"),
            Some("\"v1\""),
            Some(&pat_doc),
        )
        .await;
    let pat_status = pat_put.status();
    let pat_body = pat_put.text().await.unwrap_or_default();
    assert_eq!(pat_status, StatusCode::OK, "pattern PUT failed: {pat_body}");

    let full_id = app
        .create_entry("page", "Full Interactive", "full-interactive")
        .await;
    let full_doc = json!({
        "schema_version": 1,
        "blocks": [{
            "id": "sec-full",
            "block_id": "core/section",
            "children": [
                {
                    "id": "tabs-full",
                    "block_id": "core/tabs",
                    "props": { "style": "pills", "default_tab": 2 },
                    "children": [
                        {
                            "id": "tp-a",
                            "block_id": "core/tab-panel",
                            "props": { "title": "First" },
                            "children": [{ "id": "txt-a", "block_id": "core/text", "props": { "content": "<p>One</p>" } }]
                        },
                        {
                            "id": "tp-b",
                            "block_id": "core/tab-panel",
                            "props": { "title": "Second" },
                            "children": [{ "id": "txt-b", "block_id": "core/text", "props": { "content": "<p>Two</p>" } }]
                        }
                    ]
                },
                {
                    "id": "sld-full",
                    "block_id": "core/slider",
                    "props": { "slides_per_view": "1", "loop": true },
                    "children": [
                        {
                            "id": "s-0",
                            "block_id": "core/slide",
                            "children": [{ "id": "s0-im", "block_id": "core/image", "props": { "image": img1, "alt": "Slide 1" } }]
                        },
                        {
                            "id": "s-1",
                            "block_id": "core/slide",
                            "children": [{ "id": "s1-im", "block_id": "core/image", "props": { "image": img2, "alt": "Slide 2" } }]
                        },
                        {
                            "id": "s-2",
                            "block_id": "core/slide",
                            "children": [{ "id": "s2-hd", "block_id": "core/heading", "props": { "level": "h3", "text": "Slide 3" } }]
                        }
                    ]
                },
                {
                    "id": "gal-full",
                    "block_id": "core/gallery",
                    "props": { "images": [img1, img2], "captions": "First\nSecond", "lightbox": true, "show_captions": true }
                },
                {
                    "id": "sh-rel",
                    "block_id": "core/social-share",
                    "props": { "show_copy_link": true, "custom_url": "" }
                },
                {
                    "id": "pop-full",
                    "block_id": "core/popup",
                    "props": { "popup_id": "#promo-modal", "close_on_backdrop": false, "show_close_button": false },
                    "children": [
                        { "id": "pop-im", "block_id": "core/image", "props": { "image": img2, "alt": "Popup Photo" } },
                        { "id": "pop-ref", "block_id": "core/pattern-ref", "props": { "pattern_id": pat_id.to_string() } }
                    ]
                }
            ]
        }]
    });
    let full_put = app
        .send_json(
            Method::PUT,
            &format!("/api/admin/entries/page/{full_id}/builder"),
            Some("\"v1\""),
            Some(&full_doc),
        )
        .await;
    let put_status = full_put.status();
    let put_body = full_put.text().await.unwrap_or_default();
    assert_eq!(put_status, StatusCode::OK, "PUT failed: {put_body}");

    let full_prev: Value = app
        .send_json(
            Method::POST,
            &format!("/api/admin/entries/page/{full_id}/builder/preview"),
            None,
            Some(&full_doc),
        )
        .await
        .json()
        .await
        .unwrap();
    let diags = full_prev["diagnostics"].as_array().unwrap();
    assert!(
        diags.iter().any(|d| d["code"] == "nested_popup_not_allowed"),
        "expected nested_popup_not_allowed in diagnostics: {diags:?}"
    );
    let full_prev_html = full_prev["html"].as_str().unwrap();
    assert!(!full_prev_html.contains("/assets/opendash-blocks.js"));
    assert!(full_prev_html.contains("od-popup-preview-chip"));
    assert!(full_prev_html.contains("Popup #promo-modal"));

    assert_eq!(
        app.send_json(
            Method::POST,
            &format!("/api/admin/content-types/page/entries/{full_id}/publication"),
            Some("\"v2\""),
            Some(&json!({ "status": "published" }))
        )
        .await
        .status(),
        StatusCode::OK
    );

    let live = app
        .http
        .get(format!("{}/full-interactive", app.site.2))
        .send()
        .await
        .unwrap();
    assert_eq!(live.status(), StatusCode::OK);
    let csp = live.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(csp.contains("form-action 'none'"), "csp={csp}");
    let html = live.text().await.unwrap();
    assert_eq!(html.match_indices("/assets/opendash-blocks.js?v=").count(), 1);
    assert!(!html.contains("<form"), "unexpected <form in HTML: {html}");
    assert_eq!(html.match_indices("<dialog").count(), 2);
    for needle in [
        "data-od-tab-index=\"1\"",
        "aria-selected=\"true\"",
        "data-od-slider-root",
        "fetchpriority=\"low\"",
        "<dialog closedby=\"any\" class=\"od-lightbox-dialog\"",
        "__OD_SHARE_URL__",
        "data-od-popup-dialog=\"promo-modal\"",
        "closedby=\"closerequest\"",
        "data-od-popup-close",
    ] {
        assert!(html.contains(needle), "missing {needle}");
    }
    assert!(!html.contains("inner-pop"));
}
