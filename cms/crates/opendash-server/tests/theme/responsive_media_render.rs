#[path = "responsive_media_helpers.rs"]
mod helpers;

use helpers::{
    admin_json, build_valid_mp4, encode_wide_jpeg, setup_admin_and_login, spawn_server,
    wait_for_variants_version,
};
use reqwest::{Client, Method, StatusCode, header, redirect::Policy};
use serde_json::{Value, json};

async fn fetch_ok_text(client: &Client, url: String, cookie: Option<&str>) -> String {
    let mut req = client.get(url);
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    let res = req.send().await.unwrap();
    let status = res.status();
    let text = res.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "unexpected status {status}: {text}");
    text
}

#[tokio::test]
async fn responsive_picture_builder_seo_preview_and_replace_version_bump_end_to_end() {
    let srv = spawn_server().await;
    let base = &srv.base_url;
    let client = Client::builder().redirect(Policy::none()).build().unwrap();
    let (cookie, csrf) = setup_admin_and_login(&client, base).await;

    let up_img = client
        .post(format!("{base}/api/admin/media"))
        .header(header::ORIGIN, base)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(header::CONTENT_TYPE, "image/jpeg")
        .header("x-file-name", "hero-v1.jpg")
        .body(encode_wide_jpeg(1600, 900, 1))
        .send()
        .await
        .unwrap();
    assert_eq!(up_img.status(), StatusCode::CREATED);
    let img_id = up_img.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let up_vid = client
        .post(format!("{base}/api/admin/media/video"))
        .header(header::ORIGIN, base)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(header::CONTENT_TYPE, "video/mp4")
        .body(build_valid_mp4())
        .send()
        .await
        .unwrap();
    let vid_status = up_vid.status();
    let vid_txt = up_vid.text().await.unwrap();
    assert_eq!(
        vid_status,
        StatusCode::CREATED,
        "video upload failed: {vid_txt}"
    );
    let vid_id = serde_json::from_str::<Value>(&vid_txt).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let v1_meta = wait_for_variants_version(&client, base, &cookie, &img_id, 1).await;
    let v1_etag = v1_meta["etag"].as_str().unwrap().to_string();
    assert!(v1_meta["webp_srcset"].as_str().is_some(), "{v1_meta}");

    let post_json = admin_json(
        &client,
        Method::POST,
        format!("{base}/api/admin/content-types/post/entries"),
        base,
        &cookie,
        &csrf,
        None,
        Some(json!({
            "title": "Responsive Hero Post",
            "slug": "responsive-hero-post",
            "values": {
                "featured_image": img_id,
                "body": "<p>Post body</p>"
            }
        })),
    )
    .await;
    let post_id = post_json["id"].as_i64().unwrap();
    let post_ver = post_json["version"].as_i64().unwrap();
    let seo_res = admin_json(
        &client,
        Method::PUT,
        format!("{base}/api/admin/content-types/post/entries/{post_id}/seo"),
        base,
        &cookie,
        &csrf,
        Some(&format!("\"v{post_ver}\"")),
        Some(json!({
            "title": "SEO Responsive Hero",
            "description": "Hero desc",
            "og_image_id": img_id,
            "canonical_url": null,
            "noindex": false
        })),
    )
    .await;
    let post_ver = seo_res["entry_version"].as_i64().unwrap();
    admin_json(
        &client,
        Method::POST,
        format!("{base}/api/admin/content-types/post/entries/{post_id}/publication"),
        base,
        &cookie,
        &csrf,
        Some(&format!("\"v{post_ver}\"")),
        Some(json!({ "status": "published" })),
    )
    .await;

    let page_json = admin_json(
        &client,
        Method::POST,
        format!("{base}/api/admin/content-types/page/entries"),
        base,
        &cookie,
        &csrf,
        None,
        Some(json!({
            "title": "Responsive Builder Home",
            "slug": "responsive-builder-home",
            "values": { "body": "init" }
        })),
    )
    .await;
    let page_id = page_json["id"].as_i64().unwrap();
    let get_builder = client
        .get(format!("{base}/api/admin/entries/page/{page_id}/builder"))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(get_builder.status(), StatusCode::OK);
    let builder_etag = get_builder.headers()[header::ETAG]
        .to_str()
        .unwrap()
        .to_string();
    let saved_builder = admin_json(
        &client,
        Method::PUT,
        format!("{base}/api/admin/entries/page/{page_id}/builder"),
        base,
        &cookie,
        &csrf,
        Some(&builder_etag),
        Some(json!({
            "schema_version": 1,
            "blocks": [{
                "id": "sec1",
                "block_id": "core/section",
                "props": { "bg_image": img_id },
                "children": [{
                    "id": "row1",
                    "block_id": "core/row",
                    "props": {},
                    "children": [{
                        "id": "col1",
                        "block_id": "core/column",
                        "props": {},
                        "children": [
                            { "id": "img1", "block_id": "core/image", "props": { "image": img_id, "alt": "Builder Core Img" } },
                            { "id": "vid1", "block_id": "core/video", "props": { "video": vid_id, "poster": img_id, "controls": true, "muted": true } },
                            {
                                "id": "crd1",
                                "block_id": "core/card",
                                "props": { "image": img_id, "media_position": "top" },
                                "children": [
                                    { "id": "crd1-h", "block_id": "core/heading", "props": { "level": "h3", "text": "Card Block" } },
                                    { "id": "crd1-p", "block_id": "core/text", "props": { "content": "<p>Body</p>" } }
                                ]
                            }
                        ]
                    }]
                }]
            }]
        })),
    )
    .await;
    let page_ver = saved_builder["version"].as_i64().unwrap();
    admin_json(
        &client,
        Method::POST,
        format!("{base}/api/admin/content-types/page/entries/{page_id}/publication"),
        base,
        &cookie,
        &csrf,
        Some(&format!("\"v{page_ver}\"")),
        Some(json!({ "status": "published" })),
    )
    .await;

    let arch_json = admin_json(
        &client,
        Method::POST,
        format!("{base}/api/admin/content-types/page/entries"),
        base,
        &cookie,
        &csrf,
        None,
        Some(json!({ "title": "Tin Tuc", "slug": "tin-tuc", "values": { "body": "Archive" } })),
    )
    .await;
    let arch_id = arch_json["id"].as_i64().unwrap();
    let arch_ver = arch_json["version"].as_i64().unwrap();
    admin_json(
        &client,
        Method::POST,
        format!("{base}/api/admin/content-types/page/entries/{arch_id}/publication"),
        base,
        &cookie,
        &csrf,
        Some(&format!("\"v{arch_ver}\"")),
        Some(json!({ "status": "published" })),
    )
    .await;

    let reading = client
        .get(format!("{base}/api/admin/settings/reading"))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    let r_etag = reading.headers()[header::ETAG]
        .to_str()
        .unwrap()
        .to_string();
    admin_json(
        &client,
        Method::PUT,
        format!("{base}/api/admin/settings/reading"),
        base,
        &cookie,
        &csrf,
        Some(&r_etag),
        Some(json!({ "homepage_page_id": page_id, "posts_page_id": arch_id })),
    )
    .await;

    let expected_v1_pic = format!(
        "<picture><source type=\"image/webp\" srcset=\"/media/{img_id}/variant/medium.webp?v=1 768w"
    );
    let archive_html = fetch_ok_text(&client, format!("{base}/tin-tuc"), None).await;
    assert!(
        archive_html.contains(&expected_v1_pic),
        "archive missing responsive picture: {archive_html}"
    );
    assert!(
        archive_html.contains("width=\"1600\" height=\"900\""),
        "{archive_html}"
    );
    assert!(!archive_html.contains("OD_IMAGE_") && !archive_html.contains("__od_media_provenance"));

    let post_html = fetch_ok_text(&client, format!("{base}/responsive-hero-post"), None).await;
    assert!(
        post_html.contains(&expected_v1_pic),
        "post missing responsive picture: {post_html}"
    );
    assert!(
        post_html.contains(&format!(
            "<meta property=\"og:image\" content=\"/media/{img_id}/file?v=1\">"
        )),
        "{post_html}"
    );
    assert!(!post_html.contains("OD_IMAGE_"));

    let home_html = fetch_ok_text(&client, format!("{base}/"), None).await;
    assert!(
        home_html.contains(&expected_v1_pic),
        "builder home missing <picture>: {home_html}"
    );
    assert!(home_html.contains(&format!("poster=\"/media/{img_id}/file?v=1\"")));
    assert!(home_html.contains(&format!("<video src=\"/media/{vid_id}/file?v=1\"")));
    assert!(
        home_html.contains(&format!(
            "--od-sec-bg-img:url(\"/media/{img_id}/file?v=1\")"
        )),
        "{home_html}"
    );
    assert!(!home_html.contains("OD_IMAGE_"));

    let preview_html = fetch_ok_text(
        &client,
        format!("{base}/api/admin/content-types/post/entries/{post_id}/preview"),
        Some(&cookie),
    )
    .await;
    assert!(
        preview_html.contains(&format!(
            "/api/admin/media/{img_id}/variant/medium.webp?v=1 768w"
        )),
        "{preview_html}"
    );
    assert!(!preview_html.contains("OD_IMAGE_"));

    let rep = client
        .put(format!("{base}/api/admin/media/{img_id}/file"))
        .header(header::ORIGIN, base)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, &v1_etag)
        .header(header::CONTENT_TYPE, "image/jpeg")
        .header("x-file-name", "hero-v2.jpg")
        .body(encode_wide_jpeg(1700, 960, 42))
        .send()
        .await
        .unwrap();
    assert_eq!(rep.status(), StatusCode::OK);
    wait_for_variants_version(&client, base, &cookie, &img_id, 2).await;

    let var_v2 = client
        .get(format!("{base}/media/{img_id}/variant/medium.webp?v=2"))
        .send()
        .await
        .unwrap();
    assert_eq!(var_v2.status(), StatusCode::OK);

    let post_v2 = fetch_ok_text(&client, format!("{base}/responsive-hero-post"), None).await;
    assert!(
        post_v2.contains(&format!("/media/{img_id}/variant/medium.webp?v=2 768w")),
        "{post_v2}"
    );
    assert!(
        post_v2.contains(&format!(
            "<meta property=\"og:image\" content=\"/media/{img_id}/file?v=2\">"
        )),
        "{post_v2}"
    );

    let home_v2 = fetch_ok_text(&client, format!("{base}/"), None).await;
    assert!(home_v2.contains(&format!("/media/{img_id}/variant/medium.webp?v=2 768w")));
    assert!(home_v2.contains(&format!("poster=\"/media/{img_id}/file?v=2\"")));
    assert!(home_v2.contains(&format!(
        "--od-sec-bg-img:url(\"/media/{img_id}/file?v=2\")"
    )));
    assert!(!home_v2.contains(&format!("/media/{img_id}/variant/medium.webp?v=1")));
}
