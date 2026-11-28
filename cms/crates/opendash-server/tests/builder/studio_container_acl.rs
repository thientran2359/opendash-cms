use std::io::Write;

use reqwest::{Method, StatusCode, header};
use serde_json::{Value, json};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use super::studio_unified_ast::AdminSession;

fn build_csec_wrap_theme_zip() -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let files = [
            (
                "theme.json",
                r#"{"manifest_version":3,"theme_id":"csec-wrap","name":"CSEC Wrap Theme","version":"1.0.0","templates":[{"kind":"home","file":"home.html"},{"kind":"page","file":"page.html"}]}"#,
            ),
            ("home.html", "<main>{{ od_content() }}</main>"),
            ("page.html", "<main>{{ od_content() }}</main>"),
            (
                "blocks/box/block.json",
                r#"{"schema_version":1,"id":"csec-wrap/box","title":"CSEC Box","category":"layout","supports_children":true,"render":"render.html","fields":[]}"#,
            ),
            (
                "blocks/box/render.html",
                r#"<div class="csec-wrap">{{ children_html }}</div>"#,
            ),
        ];
        for (path, content) in files {
            zip.start_file(path, opts).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
    buf
}

#[tokio::test]
async fn theme_container_provider_toggle_revokes_nested_media_and_pattern_acl_preserving_pins() {
    let app = AdminSession::boot().await;
    let themes: Value = app
        .send_json(Method::GET, "/api/admin/themes", None, None)
        .await
        .json()
        .await
        .unwrap();
    let starter_rev = themes["revisions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["theme_id"] == "opendash-starter")
        .unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let inst = app
        .auth(Method::POST, "/api/admin/themes")
        .header(header::CONTENT_TYPE, "application/zip")
        .body(build_csec_wrap_theme_zip())
        .send()
        .await
        .unwrap();
    assert_eq!(inst.status(), StatusCode::CREATED);
    let wrap_rev = inst.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_owned();
    app.activate_theme(&wrap_rev, 1).await;

    let direct_img = app.upload_png("direct.png").await;
    let pattern_img = app.upload_png("pattern.png").await;
    let meta = app
        .send_json(
            Method::GET,
            &format!("/api/admin/media/{direct_img}"),
            None,
            None,
        )
        .await;
    assert_eq!(meta.status(), StatusCode::OK);
    let etag = meta.headers()[header::ETAG].to_str().unwrap().to_owned();

    let pat_id = app.create_pattern("Wrap Pattern", "wrap-pattern").await;
    let pat_doc: Value = serde_json::from_str(&r#"{"schema_version":1,"blocks":[{"id":"pat-sec","block_id":"core/section","children":[{"id":"pat-row","block_id":"core/row","children":[{"id":"pat-col","block_id":"core/column","children":[{"id":"pat-img","block_id":"core/image","props":{"image":"__P__","alt":"Pattern Img"}}]}]}]}]}"#.replace("__P__", &pattern_img)).unwrap();
    assert_eq!(
        app.send_json(
            Method::PUT,
            &format!("/api/admin/entries/pattern/{pat_id}/builder"),
            Some("\"v1\""),
            Some(&pat_doc)
        )
        .await
        .status(),
        StatusCode::OK
    );

    let page_id = app.create_entry("page", "Wrap ACL", "wrap-acl-page").await;
    let page_doc: Value = serde_json::from_str(&r#"{"schema_version":1,"blocks":[{"id":"wrap-sec","block_id":"core/section","children":[{"id":"wrap-box","block_id":"csec-wrap/box","children":[{"id":"wrap-img","block_id":"core/image","props":{"image":"__I__","alt":"Direct Img"}},{"id":"wrap-pat","block_id":"core/pattern-ref","props":{"pattern_id":"__PID__"}}]}]}]}"#.replace("__I__", &direct_img).replace("__PID__", &pat_id.to_string())).unwrap();
    assert_eq!(
        app.send_json(
            Method::PUT,
            &format!("/api/admin/entries/page/{page_id}/builder"),
            Some("\"v1\""),
            Some(&page_doc)
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        app.send_json(
            Method::POST,
            &format!("/api/admin/content-types/page/entries/{page_id}/publication"),
            Some("\"v2\""),
            Some(&json!({ "status": "published" }))
        )
        .await
        .status(),
        StatusCode::OK
    );

    assert_eq!(
        app.pub_status(&format!("/media/{direct_img}/file")).await,
        StatusCode::OK
    );
    assert_eq!(
        app.pub_status(&format!("/media/{pattern_img}/file")).await,
        StatusCode::OK
    );
    assert!(
        app.http
            .get(format!("{}/wrap-acl-page", app.site.2))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap()
            .contains("csec-wrap")
    );

    app.activate_theme(&starter_rev, 2).await;
    assert_eq!(
        app.pub_status(&format!("/media/{direct_img}/file")).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.pub_status(&format!("/media/{pattern_img}/file")).await,
        StatusCode::NOT_FOUND
    );

    assert_eq!(
        app.send_json(
            Method::DELETE,
            &format!("/api/admin/media/{direct_img}"),
            Some(&etag),
            None
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    assert!(
        app.send_json(
            Method::GET,
            &format!("/api/admin/entries/page/{page_id}/builder"),
            None,
            None
        )
        .await
        .text()
        .await
        .unwrap()
        .contains("csec-wrap/box")
    );

    app.activate_theme(&wrap_rev, 3).await;
    assert_eq!(
        app.pub_status(&format!("/media/{direct_img}/file")).await,
        StatusCode::OK
    );
    assert_eq!(
        app.pub_status(&format!("/media/{pattern_img}/file")).await,
        StatusCode::OK
    );
}
