use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use reqwest::Client;
use serde_json::{Value, json};
use zip::{ZipWriter, write::SimpleFileOptions};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}_{}_{}", std::process::id(), nanos));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct ServerHandle {
    child: Child,
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn build_plugin_zip(manifest_json: &str, wasm_bytes: &[u8]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default();
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(manifest_json.as_bytes()).unwrap();
        zip.start_file("plugin.wasm", opts).unwrap();
        zip.write_all(wasm_bytes).unwrap();
        zip.finish().unwrap();
    }
    buf.into_inner()
}

fn minimal_plugin_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "opendash_run") (result i32)
                i32.const 0)
        )"#,
    )
    .unwrap()
}

async fn spawn_server(site_root: &Path, port: u16) -> ServerHandle {
    let exe = env!("CARGO_BIN_EXE_opendash");
    let base_url = format!("http://127.0.0.1:{port}");
    let child = Command::new(exe)
        .env("OPENDASH_SITE_ROOT", site_root)
        .env("OPENDASH_HOST", "127.0.0.1")
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn opendash server");

    let client = Client::new();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(resp) = client.get(format!("{base_url}/api/health")).send().await {
            if resp.status().is_success() {
                break;
            }
        }
        assert!(Instant::now() < deadline, "server failed to become healthy");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    ServerHandle { child }
}

async fn setup_and_login(client: &Client, base_url: &str) -> (String, String) {
    let resp = client
        .post(format!("{base_url}/auth/setup"))
        .header("Origin", base_url)
        .json(&json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());

    let login = client
        .post(format!("{base_url}/auth/login"))
        .header("Origin", base_url)
        .json(&json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert!(login.status().is_success());
    let cookie = login
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    let me: Value = client
        .get(format!("{base_url}/auth/me"))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let csrf = me["csrf_token"].as_str().unwrap().to_string();
    (cookie, csrf)
}

#[tokio::test]
async fn m6_s02_t02_plugin_taxonomy_eligibility_and_draft_isolation() {
    let temp = TempDir::new("m6_s02_t02");
    let port = free_port();
    let mut server = Some(spawn_server(&temp.path, port).await);
    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let (cookie, csrf) = setup_and_login(&client, &base_url).await;
    let wasm = minimal_plugin_wasm();

    // 1. Reject custom taxonomy kind outside category/tag -> 400
    let custom_tax_manifest = json!({
        "id": "bad-tax",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "Bad Tax",
        "declarations": {
            "content_types": [
                { "key": "book", "label": "Book", "fields": [], "taxonomies": ["genre"] }
            ]
        }
    })
    .to_string();
    let r_bad_tax = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("content-type", "application/zip")
        .header("x-csrf-token", &csrf)
        .body(build_plugin_zip(&custom_tax_manifest, &wasm))
        .send()
        .await
        .unwrap();
    assert_eq!(r_bad_tax.status(), 400);

    // 2. Reject claiming taxonomy eligibility on builtin `post` -> 409
    let hijack_post_tax = json!({
        "id": "hijack-tax",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "Hijack Tax",
        "declarations": {
            "taxonomy_eligibility": [
                { "content_type": "post", "taxonomies": ["category"] }
            ]
        }
    })
    .to_string();
    let r_hijack = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("content-type", "application/zip")
        .header("x-csrf-token", &csrf)
        .body(build_plugin_zip(&hijack_post_tax, &wasm))
        .send()
        .await
        .unwrap();
    assert_eq!(r_hijack.status(), 409);

    // 3. Valid plugin declares `listing` CPT with `category` and `tag` eligibility
    let listing_v1 = json!({
        "id": "directory",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "Directory",
        "declarations": {
            "content_types": [
                {
                    "key": "listing",
                    "label": "Listing",
                    "fields": [],
                    "taxonomies": ["category", "tag"]
                }
            ]
        }
    })
    .to_string();
    let r_install = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("content-type", "application/zip")
        .header("x-csrf-token", &csrf)
        .body(build_plugin_zip(&listing_v1, &wasm))
        .send()
        .await
        .unwrap();
    assert_eq!(r_install.status(), 201);

    let elig_rows: Vec<Value> = client
        .get(format!("{base_url}/api/admin/taxonomy/eligibility/listing"))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(elig_rows.len(), 2);
    assert_eq!(elig_rows[0]["taxonomy_kind"], "category");
    assert_eq!(elig_rows[0]["owner_kind"], "plugin");
    assert_eq!(elig_rows[0]["owner_id"], "directory");
    assert_eq!(elig_rows[0]["declaration_state"], "declared");
    assert_eq!(elig_rows[0]["provider_status"], "active");

    // 4. Create category & tag terms and a DRAFT listing entry linked to them
    let cat_resp = client
        .post(format!("{base_url}/api/admin/taxonomy/terms"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({ "kind": "category", "name": "Engineering", "slug": "engineering" }))
        .send()
        .await
        .unwrap();
    assert_eq!(cat_resp.status(), 201);
    let cat_id = cat_resp.json::<Value>().await.unwrap()["id"].as_i64().unwrap();

    let tag_resp = client
        .post(format!("{base_url}/api/admin/taxonomy/terms"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({ "kind": "tag", "name": "Remote", "slug": "remote" }))
        .send()
        .await
        .unwrap();
    assert_eq!(tag_resp.status(), 201);
    let tag_id = tag_resp.json::<Value>().await.unwrap()["id"].as_i64().unwrap();

    // Create a draft `post` linked to `cat_id` and a `listing` entry linked to `cat_id` + `tag_id`
    let draft_post = client
        .post(format!("{base_url}/api/admin/content-types/post/entries"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "slug": "secret-draft-post",
            "title": "Secret Draft Post",
            "values": {},
            "term_ids": [cat_id]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(draft_post.status(), 201);

    let entry_resp = client
        .post(format!("{base_url}/api/admin/content-types/listing/entries"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "slug": "published-listing",
            "title": "Published Listing",
            "values": {},
            "term_ids": [cat_id, tag_id]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(entry_resp.status(), 201);
    let entry: Value = entry_resp.json().await.unwrap();
    let entry_id = entry["id"].as_i64().unwrap();

    // Draft `post` must NOT leak through /taxonomy/category/engineering, while published `listing` does appear
    let archive_draft = client
        .get(format!("{base_url}/taxonomy/category/engineering"))
        .send()
        .await
        .unwrap();
    let archive_status = archive_draft.status();
    let archive_draft_body = archive_draft.text().await.unwrap();
    assert!(
        !archive_draft_body.contains("Secret Draft Post"),
        "draft post leaked into taxonomy archive (status={archive_status}): {archive_draft_body}"
    );
    assert!(
        archive_draft_body.contains("Published Listing"),
        "published listing missing from taxonomy archive"
    );

    // 5. Disable plugin -> eligibility becomes provider_inactive, entry_terms preserved & editable
    let dis = client
        .post(format!("{base_url}/api/admin/plugins/directory/disable"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &csrf)
        .header("if-match", "1")
        .send()
        .await
        .unwrap();
    assert_eq!(dis.status(), 200);

    let elig_disabled: Vec<Value> = client
        .get(format!("{base_url}/api/admin/taxonomy/eligibility/listing"))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(elig_disabled[0]["provider_status"], "provider_inactive");

    // Entry still retains term_ids and can be edited
    let fetched_entry: Value = client
        .get(format!("{base_url}/api/admin/content-types/listing/entries/{entry_id}"))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(fetched_entry["term_ids"], json!([cat_id, tag_id]));

    let upd = client
        .put(format!("{base_url}/api/admin/content-types/listing/entries/{entry_id}"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({
            "slug": "draft-listing",
            "title": "Updated Listing",
            "values": {},
            "term_ids": [cat_id]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(upd.status(), 200);

    // 6. Upgrade plugin to v1.1.0 withdrawing `tag`, then disable/re-enable -> `tag` stays withdrawn
    let listing_v1_1 = json!({
        "id": "directory",
        "version": "1.1.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "Directory",
        "declarations": {
            "content_types": [
                {
                    "key": "listing",
                    "label": "Listing",
                    "fields": [],
                    "taxonomies": ["category"]
                }
            ]
        }
    })
    .to_string();
    let r_up = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("content-type", "application/zip")
        .header("x-csrf-token", &csrf)
        .body(build_plugin_zip(&listing_v1_1, &wasm))
        .send()
        .await
        .unwrap();
    assert_eq!(r_up.status(), 201);
    let cas_after_up = r_up.json::<Value>().await.unwrap()["cas_version"]
        .as_i64()
        .unwrap();

    let dis2 = client
        .post(format!("{base_url}/api/admin/plugins/directory/disable"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &csrf)
        .header("if-match", cas_after_up.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(dis2.status(), 200);
    let cas_dis2 = dis2.json::<Value>().await.unwrap()["cas_version"]
        .as_i64()
        .unwrap();

    let en2 = client
        .post(format!("{base_url}/api/admin/plugins/directory/enable"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &csrf)
        .header("if-match", cas_dis2.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(en2.status(), 200);

    // 7. Restart server and verify `category` is active/declared, `tag` stayed withdrawn, no duplicate terms
    drop(server.take());
    let port2 = free_port();
    let _server2 = spawn_server(&temp.path, port2).await;
    let base_url2 = format!("http://127.0.0.1:{port2}");
    let (cookie2, _csrf2) = {
        let login = client
            .post(format!("{base_url2}/auth/login"))
            .header("Origin", &base_url2)
            .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
            .send()
            .await
            .unwrap();
        let c = login
            .headers()
            .get(reqwest::header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        (c, String::new())
    };

    let elig_after_restart: Vec<Value> = client
        .get(format!("{base_url2}/api/admin/taxonomy/eligibility/listing"))
        .header("Cookie", &cookie2)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(elig_after_restart.len(), 2);
    let cat_row = elig_after_restart
        .iter()
        .find(|r| r["taxonomy_kind"] == "category")
        .unwrap();
    let tag_row = elig_after_restart
        .iter()
        .find(|r| r["taxonomy_kind"] == "tag")
        .unwrap();
    assert_eq!(cat_row["declaration_state"], "declared");
    assert_eq!(cat_row["provider_status"], "active");
    assert_eq!(tag_row["declaration_state"], "withdrawn");
    assert_eq!(tag_row["provider_status"], "provider_inactive");
}
