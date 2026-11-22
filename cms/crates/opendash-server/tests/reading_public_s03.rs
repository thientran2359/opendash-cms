//! S03: native public Reading Settings, publication guards and route authority.
//! Every test owns a disposable site root and an ephemeral loopback port.
use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use sqlx::SqlitePool;

struct Site {
    root: PathBuf,
    child: Child,
    base: String,
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Site {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "opendash-s03-public-{}-{nanos}",
                std::process::id()
            ));
        fs::create_dir_all(&root).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
            .env("OPENDASH_SITE_ROOT", &root)
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let client = Client::new();
        for _ in 0..140 {
            if client
                .get(format!("{base}/api/health"))
                .send()
                .await
                .is_ok_and(|r| r.status() == StatusCode::OK)
            {
                return Self { root, child, base };
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("S03 temporary server did not become healthy");
    }

    async fn db(&self) -> SqlitePool {
        SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(self.root.join("database/opendash.sqlite")),
        )
        .await
        .unwrap()
    }
}

async fn admin(client: &Client, site: &Site) -> (String, String) {
    let setup = client
        .post(format!("{}/auth/setup", site.base))
        .json(&json!({"username":"admin","password":"AdminPassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let login = client
        .post(format!("{}/auth/login", site.base))
        .json(&json!({"username":"admin","password":"AdminPassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .map(|h| h.split(';').next().unwrap().to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = login.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_owned())
}

async fn create_published(
    client: &Client,
    site: &Site,
    cookie: &str,
    csrf: &str,
    kind: &str,
    slug: &str,
    title: &str,
    body: &str,
) -> (i64, i64) {
    let types: Vec<Value> = client
        .get(format!("{}/api/admin/content-types", site.base))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = types
        .iter()
        .find(|r| r["descriptor"]["key"] == kind)
        .unwrap()["schema_token"]
        .as_str()
        .unwrap();
    let response = client
        .post(format!(
            "{}/api/admin/content-types/{kind}/entries/composite",
            site.base
        ))
        .header(header::COOKIE, cookie)
        .header("X-CSRF-Token", csrf)
        .json(
            &json!({"expected_schema_token":token,"slug":slug,"title":title,
            "values":{"body":body},"publication_status":"published",
            "seo":{"title":format!("SEO {title}"),"noindex":false}}),
        )
        .send()
        .await
        .unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{text}");
    let value: Value = serde_json::from_str(&text).unwrap();
    (
        value["entry"]["id"].as_i64().unwrap(),
        value["entry"]["version"].as_i64().unwrap(),
    )
}

#[tokio::test]
async fn home_and_posts_page_use_distinct_templates_and_preserve_deleted_ids() {
    let site = Site::start().await;
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let (cookie, csrf) = admin(&client, &site).await;

    let empty = client.get(format!("{}/", site.base)).send().await.unwrap();
    assert_eq!(empty.status(), StatusCode::OK);
    assert!(
        empty
            .text()
            .await
            .unwrap()
            .contains("Chưa cấu hình trang chủ")
    );

    let (home, home_version) = create_published(
        &client,
        &site,
        &cookie,
        &csrf,
        "page",
        "s03-home",
        "Home Display",
        "Home Content",
    )
    .await;
    let (blog, blog_version) = create_published(
        &client,
        &site,
        &cookie,
        &csrf,
        "page",
        "s03-blog",
        "Blog Not Displayed",
        "Blog Secret Body",
    )
    .await;
    let (_post, _) = create_published(
        &client,
        &site,
        &cookie,
        &csrf,
        "post",
        "s03-post",
        "Visible Post",
        "Post Body",
    )
    .await;

    let public = client
        .get(format!("{}/api/admin/settings/public", site.base))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    let etag = public
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let public_json: Value = public.json().await.unwrap();
    let update = client
        .put(format!("{}/api/admin/settings/public", site.base))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, etag)
        .json(&json!({"public_origin":"https://example.com",
            "content_language":public_json["content_language"],"indexing_enabled":true}))
        .send()
        .await
        .unwrap();
    assert_eq!(update.status(), StatusCode::OK);

    let reading = client
        .get(format!("{}/api/admin/settings/reading", site.base))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(reading.status(), StatusCode::OK);
    let version = reading
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let saved = client
        .put(format!("{}/api/admin/settings/reading", site.base))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, version)
        .json(&json!({"homepage_page_id":home,"posts_page_id":blog}))
        .send()
        .await
        .unwrap();
    let saved_status = saved.status();
    let saved_text = saved.text().await.unwrap();
    assert_eq!(saved_status, StatusCode::OK, "{saved_text}");

    let front = client.get(format!("{}/", site.base)).send().await.unwrap();
    assert_eq!(front.status(), StatusCode::OK);
    let front_html = front.text().await.unwrap();
    assert!(front_html.contains("SEO Home Display"), "{front_html}");
    assert!(!front_html.contains("Blog Secret Body"));
    let old_home = client
        .get(format!("{}/content/page/s03-home", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(old_home.status(), StatusCode::MOVED_PERMANENTLY);
    assert_eq!(old_home.headers().get(header::LOCATION).unwrap(), "/");

    let blog_archive = client
        .get(format!("{}/content/page/s03-blog", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(blog_archive.status(), StatusCode::OK);
    let blog_html = blog_archive.text().await.unwrap();
    assert!(blog_html.contains("Visible Post"), "{blog_html}");
    assert!(!blog_html.contains("Blog Secret Body"));
    assert!(!blog_html.contains("Blog Not Displayed"));

    let sitemap = client
        .get(format!("{}/sitemaps/1.xml", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(sitemap.status(), StatusCode::OK);
    let xml = sitemap.text().await.unwrap();
    assert_eq!(
        xml.matches("<loc>https://example.com/</loc>").count(),
        1,
        "{xml}"
    );
    assert!(!xml.contains("/content/page/s03-home"));
    // The Posts Page is absent as a Page entry but present exactly once as
    // the canonical post archive at the same URL.
    assert_eq!(
        xml.matches("<loc>https://example.com/content/page/s03-blog</loc>")
            .count(),
        1,
        "{xml}"
    );

    // Draft Home cannot fall through to the old Builder homepage or another Page.
    let unpublished = client
        .post(format!(
            "{}/api/admin/content-types/page/entries/{home}/publication",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{home_version}\""))
        .json(&json!({"status":"draft"}))
        .send()
        .await
        .unwrap();
    assert_eq!(unpublished.status(), StatusCode::OK);
    let missing = client.get(format!("{}/", site.base)).send().await.unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(missing.headers().get("X-Robots-Tag").unwrap(), "noindex");

    let db = site.db().await;
    let version_before: i64 =
        sqlx::query_scalar("SELECT version FROM site_reading_settings WHERE id=1")
            .fetch_one(&db)
            .await
            .unwrap();
    let deleted = client
        .delete(format!(
            "{}/api/admin/content-types/page/entries/{blog}",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{blog_version}\""))
        .send()
        .await
        .unwrap();
    assert!(deleted.status().is_success(), "{}", deleted.status());
    let (saved_home, saved_blog, version_after): (Option<i64>, Option<i64>, i64) = sqlx::query_as(
        "SELECT homepage_page_id,posts_page_id,version FROM site_reading_settings WHERE id=1",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!((saved_home, saved_blog), (Some(home), Some(blog)));
    assert_eq!(version_after, version_before + 1);
    let missing_blog = client
        .get(format!("{}/content/page/s03-blog", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(missing_blog.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        missing_blog.headers().get("X-Robots-Tag").unwrap(),
        "noindex"
    );
    let sitemap_after_delete = client
        .get(format!("{}/sitemaps/1.xml", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(sitemap_after_delete.status(), StatusCode::OK);
    let xml_after_delete = sitemap_after_delete.text().await.unwrap();
    assert!(
        !xml_after_delete.contains("/content/page/s03-blog"),
        "{xml_after_delete}"
    );
    db.close().await;
}
