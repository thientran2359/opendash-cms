use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::Client;
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

struct TestServer {
    child: Child,
    site_root: PathBuf,
    base_url: String,
    cleanup_on_drop: bool,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if self.cleanup_on_drop {
            let _ = fs::remove_dir_all(&self.site_root);
        }
    }
}

async fn spawn_server_at(site_root: PathBuf, cleanup_on_drop: bool) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    fs::create_dir_all(&site_root).unwrap();

    let bin = env!("CARGO_BIN_EXE_opendash");
    let child = Command::new(bin)
        .env("OPENDASH_SITE_ROOT", &site_root)
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn opendash server");

    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    for _ in 0..50 {
        if let Ok(resp) = client.get(format!("{base_url}/api/health")).send().await
            && resp.status().is_success()
        {
            return TestServer {
                child,
                site_root,
                base_url,
                cleanup_on_drop,
            };
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("server did not become healthy at {base_url}");
}

fn build_theme_with_menu_header(id: &str, header_html: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let manifest = json!({
            "manifest_version": 2,
            "theme_id": id,
            "name": id,
            "version": "1.0.0",
            "templates": {
                "header": "templates/header.html",
                "single": "templates/single.html"
            }
        });
        zip.start_file("theme.json", opts).unwrap();
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        zip.start_file("templates/header.html", opts).unwrap();
        zip.write_all(header_html.as_bytes()).unwrap();
        zip.start_file("templates/single.html", opts).unwrap();
        zip.write_all(b"<article>{{ entry.title }}</article>").unwrap();
        zip.finish().unwrap();
    }
    buf
}

#[tokio::test]
async fn site_menus_ssr_slug_update_draft_hide_and_theme_switch_restart() {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let site_root = std::env::temp_dir().join(format!("opendash-m5-s09-t01-ssr-{nanos}"));
    let srv = spawn_server_at(site_root.clone(), false).await;
    let client = Client::new();
    let origin = srv.base_url.clone();

    // 1. Setup & login
    client
        .post(format!("{}/auth/setup", srv.base_url))
        .header("Origin", &origin)
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    let login_res = client
        .post(format!("{}/auth/login", srv.base_url))
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    let cookie = login_res
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let csrf = login_res.json::<Value>().await.unwrap()["csrf_token"]
        .as_str()
        .unwrap()
        .to_string();

    // 2. Install & activate theme A and theme B with menu rendering in header.html
    let header_tpl = r#"<header><nav>{% if menus.primary is defined %}{% for item in menus.primary.items %}<a href="{{ item.url }}">{{ item.label }}</a>{% endfor %}{% endif %}</nav></header>"#;
    let zip_a = build_theme_with_menu_header("theme-a", header_tpl);
    let zip_b = build_theme_with_menu_header("theme-b", header_tpl);
    let mut rev_ids = Vec::new();
    for z in [zip_a, zip_b] {
        let r = client
            .post(format!("{}/api/admin/themes", srv.base_url))
            .header("Cookie", &cookie)
            .header("Origin", &origin)
            .header("X-CSRF-Token", &csrf)
            .header("Content-Type", "application/zip")
            .body(z)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 201);
        rev_ids.push(r.json::<Value>().await.unwrap()["revision_id"].as_str().unwrap().to_string());
    }
    let act_a = client
        .put(format!("{}/api/admin/themes/active", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &origin)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({ "revision_id": rev_ids[0] }))
        .send()
        .await
        .unwrap();
    assert!(act_a.status().is_success());

    // 3. Create published post + draft post
    let p1: Value = client
        .post(format!("{}/api/admin/content-types/post/entries", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &origin)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "title": "About", "slug": "about-v1", "values": {} }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let p1_id = p1["id"].as_i64().unwrap();
    client
        .post(format!("{}/api/admin/content-types/post/entries/{p1_id}/publication", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &origin)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap();

    let p2: Value = client
        .post(format!("{}/api/admin/content-types/post/entries", srv.base_url))
        .header("Cookie", &cookie)
        .header("Origin", &origin)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "title": "Draft Page", "slug": "draft-page", "values": {} }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let p2_id = p2["id"].as_i64().unwrap();

    // 4. Populate site menu & assign slot `primary` for theme-a directly in DB, then test slug change + restart
    drop(srv);
    let db_path = site_root.join("database").join("opendash.sqlite");
    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db_path))
        .await
        .unwrap();

    let menu_id: i64 = sqlx::query_scalar(
        "INSERT INTO site_menus (menu_key, title, version) VALUES ('main', 'Main', 1) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO site_menu_items (menu_id, parent_id, position, label, target_kind, target_entry_id)
         VALUES (?, NULL, 0, 'About Link', 'entry', ?),
                (?, NULL, 1, 'Draft Link', 'entry', ?)",
    )
    .bind(menu_id)
    .bind(p1_id)
    .bind(menu_id)
    .bind(p2_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO site_menu_slot_assignments (theme_id, slot_key, menu_id)
         VALUES ('theme-a', 'primary', ?)",
    )
    .bind(menu_id)
    .execute(&pool)
    .await
    .unwrap();

    // Simulate slug change on p1_id
    sqlx::query("UPDATE content_entries SET slug = 'about-v2' WHERE id = ?")
        .bind(p1_id)
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    // 5. Restart server and verify public header renders `/content/post/about-v2` and omits Draft Link
    let srv2 = spawn_server_at(site_root.clone(), false).await;
    let html = client
        .get(format!("{}/content/post/about-v2", srv2.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        html.contains(r#"href="/content/post/about-v2""#)
            || html.contains(r#"href="&#x2f;content&#x2f;post&#x2f;about-v2""#),
        "html was: {html}"
    );
    assert!(html.contains("About Link"));
    assert!(!html.contains("Draft Link"));

    // 6. Switch theme A -> B -> A and verify menu + slot assignment preserved
    let login2 = client
        .post(format!("{}/auth/login", srv2.base_url))
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    let cookie2 = login2
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let csrf2 = login2.json::<Value>().await.unwrap()["csrf_token"]
        .as_str()
        .unwrap()
        .to_string();

    for (ver, rev) in [(2, &rev_ids[1]), (3, &rev_ids[0])] {
        let act = client
            .put(format!("{}/api/admin/themes/active", srv2.base_url))
            .header("Cookie", &cookie2)
            .header("Origin", &srv2.base_url)
            .header("X-CSRF-Token", &csrf2)
            .header("If-Match", format!("\"v{ver}\""))
            .json(&json!({ "revision_id": rev }))
            .send()
            .await
            .unwrap();
        assert!(act.status().is_success());
    }

    let html_after_switch = client
        .get(format!("{}/content/post/about-v2", srv2.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        html_after_switch.contains(r#"href="/content/post/about-v2""#)
            || html_after_switch.contains(r#"href="&#x2f;content&#x2f;post&#x2f;about-v2""#),
        "html_after_switch was: {html_after_switch}"
    );
    assert!(html_after_switch.contains("About Link"));

    // Verify Draft Link DB row was never deleted
    drop(srv2);
    let pool2 = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db_path))
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM site_menu_items WHERE menu_id = ?")
        .bind(menu_id)
        .fetch_one(&pool2)
        .await
        .unwrap();
    assert_eq!(count, 2);
    pool2.close().await;
    let _ = fs::remove_dir_all(&site_root);
}
