//! F02: sanitized theme revision discovery and explicit activation over native HTTP.
use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

struct Server {
    base: String,
    child: Child,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn spawn(root: &PathBuf) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
        .env("OPENDASH_HOST", "127.0.0.1")
        .env("OPENDASH_PORT", port.to_string())
        .env("OPENDASH_SITE_ROOT", root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let srv = Server {
        base: format!("http://127.0.0.1:{port}"),
        child,
    };
    let client = Client::new();
    for _ in 0..100 {
        if let Ok(resp) = client.get(format!("{}/api/health", srv.base)).send().await
            && resp.status() == StatusCode::OK
        {
            return srv;
        }
        tokio::time::sleep(Duration::from_millis(80)).await;
    }
    panic!("F02 native server did not become healthy");
}

fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut writer = ZipWriter::new(std::io::Cursor::new(&mut out));
        for (name, bytes) in entries {
            writer
                .start_file(
                    *name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
                )
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }
    out
}

async fn login(client: &Client, base: &str, name: &str, pass: &str) -> (String, String) {
    let res = client
        .post(format!("{base}/auth/login"))
        .json(&json!({"username":name,"password":pass}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let cookie = res
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let csrf = res.json::<Value>().await.unwrap()["csrf_token"]
        .as_str()
        .unwrap()
        .to_string();
    (cookie, csrf)
}

async fn list(client: &Client, base: &str, cookie: &str) -> Value {
    let res = client
        .get(format!("{base}/api/admin/themes"))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let etag = res
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let body = res.json::<Value>().await.unwrap();
    assert_eq!(etag, format!("\"v{}\"", body["version"]));
    assert_eq!(body["version"], body["selection"]["version"]);
    body
}

#[tokio::test]
async fn native_v3_revision_projection_and_activation_guards_survive_restart() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("opendash-m5-s11-t01-{nanos}"));
    fs::create_dir_all(&root).unwrap();
    let client = Client::new();
    let srv = spawn(&root).await;

    let unauthed = client
        .get(format!("{}/api/admin/themes", srv.base))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthed.status(), 401);
    assert_eq!(
        client
            .post(format!("{}/auth/setup", srv.base))
            .json(&json!({"username":"admin","password":"AdminPassword123!"}))
            .send()
            .await
            .unwrap()
            .status(),
        201
    );
    let (admin, csrf) = login(&client, &srv.base, "admin", "AdminPassword123!").await;
    let initial = list(&client, &srv.base, &admin).await;
    assert_eq!(initial["items"].as_array().unwrap().len(), 0);
    assert_eq!(initial["revisions"].as_array().unwrap().len(), 0);
    assert_eq!(initial["selection"]["revision_id"], Value::Null);
    assert_eq!(initial["selection"]["version"], 1);

    // Editor has no Admin revision list/activate rights.
    assert_eq!(
        client
            .post(format!("{}/api/admin/users", srv.base))
            .header(header::COOKIE, &admin)
            .header("x-csrf-token", &csrf)
            .json(&json!({"username":"editor","password":"EditorPassword123!","role":"editor"}))
            .send()
            .await
            .unwrap()
            .status(),
        201
    );
    let (editor, editor_csrf) = login(&client, &srv.base, "editor", "EditorPassword123!").await;
    assert_eq!(
        client
            .get(format!("{}/api/admin/themes", srv.base))
            .header(header::COOKIE, &editor)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );

    let legacy_zip = zip(&[
        (
            "block.json",
            br#"{"id":"f02/banner","fields":{"title":"Title"}}"#,
        ),
        ("block.html", b"<h1>{{ values.title }}</h1>"),
    ]);
    // S02 rejects v1 packages without changing revision state.
    let legacy = client
        .post(format!("{}/api/admin/themes", srv.base))
        .header(header::COOKIE, &admin)
        .header("x-csrf-token", &csrf)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(legacy_zip)
        .send()
        .await
        .unwrap();
    assert_eq!(legacy.status(), 400);

    // A syntactically valid v2 package must also be refused at the v3-only
    // upload boundary; rejecting it must not create a historical revision.
    let old_v2 = zip(&[(
        "theme.json",
        br#"{"manifest_version":2,"theme_id":"old-v2-theme","name":"Old V2 Theme","version":"1.0.0","templates":{}}"#,
    )]);
    assert_eq!(
        client
            .post(format!("{}/api/admin/themes", srv.base))
            .header(header::COOKIE, &admin)
            .header("x-csrf-token", &csrf)
            .header(header::CONTENT_TYPE, "application/zip")
            .body(old_v2)
            .send()
            .await
            .unwrap()
            .status(),
        400
    );

    let mkzip = |version: &str, markup: &str| -> Vec<u8> {
        let manifest = json!({
            "manifest_version": 3,
            "theme_id": "f02-editorial",
            "name": "F02 Editorial",
            "version": version,
            "templates": [{"kind": "single", "file": "single.html"}]
        })
        .to_string();
        zip(&[
            ("theme.json", manifest.as_bytes()),
            ("single.html", markup.as_bytes()),
        ])
    };
    let bytes1 = mkzip(
        "1.0.0",
        "<article>{{ entry.title }}{{ od_content() }}</article>",
    );
    let bytes2 = mkzip("2.0.0", "<main>{{ entry.title }}{{ od_content() }}</main>");
    let mut ids = Vec::new();
    for (bytes, expected) in [(&bytes1, 201), (&bytes1, 200), (&bytes2, 201)] {
        let res = client
            .post(format!("{}/api/admin/themes", srv.base))
            .header(header::COOKIE, &admin)
            .header("x-csrf-token", &csrf)
            .header(header::CONTENT_TYPE, "application/zip")
            .body(bytes.clone())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), expected);
        let value = res.json::<Value>().await.unwrap();
        assert_eq!(value["source_format"], "v3");
        assert_eq!(value["activated"], false);
        ids.push(value["revision_id"].as_str().unwrap().to_owned());
    }
    assert_eq!(ids[0], ids[1], "idempotent re-upload retains revision ID");
    assert_ne!(ids[0], ids[2]);
    let unselected = list(&client, &srv.base, &admin).await;
    assert_eq!(unselected["items"].as_array().unwrap().len(), 0);
    assert_eq!(unselected["active_id"], Value::Null);
    assert_eq!(unselected["homepage_compatible"], Value::Null);
    let rows = unselected["revisions"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        2,
        "rejected v1/v2 packages cannot create revisions"
    );
    let keys = [
        "revision_id",
        "theme_id",
        "version",
        "revision_digest",
        "source_format",
    ];
    for rev in rows {
        let mut actual = rev
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        actual.sort();
        let mut expected = keys.to_vec();
        expected.sort();
        assert_eq!(
            actual, expected,
            "no manifest/path/provenance may be exposed"
        );
    }
    assert_eq!(rows[0]["theme_id"], "f02-editorial");
    assert_eq!(rows[0]["version"], "1.0.0");
    assert_eq!(rows[1]["version"], "2.0.0");
    assert_eq!(rows[0]["source_format"], "v3");
    assert_eq!(rows[1]["source_format"], "v3");
    assert_eq!(unselected["selection"]["revision_id"], Value::Null);

    // v3 revision activation requires the typed revision_id; legacy IDs are rejected.
    let denied = client
        .put(format!("{}/api/admin/themes/active", srv.base))
        .header(header::COOKIE, &editor)
        .header("x-csrf-token", &editor_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({"revision_id":ids[0]}))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), 403);
    assert_eq!(
        client
            .put(format!("{}/api/admin/themes/active", srv.base))
            .header(header::COOKIE, &admin)
            .header("x-csrf-token", &csrf)
            .json(&json!({"revision_id":ids[0]}))
            .send()
            .await
            .unwrap()
            .status(),
        428
    );
    assert_eq!(
        client
            .put(format!("{}/api/admin/themes/active", srv.base))
            .header(header::COOKIE, &admin)
            .header("x-csrf-token", &csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({"revision_id":ids[0],"id":"f02/banner"}))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    let legacy_active = client
        .put(format!("{}/api/admin/themes/active", srv.base))
        .header(header::COOKIE, &admin)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({"id":"f02/banner"}))
        .send()
        .await
        .unwrap();
    assert_eq!(legacy_active.status(), 400);
    assert_eq!(list(&client, &srv.base, &admin).await["version"], 1);
    let v3_active = client
        .put(format!("{}/api/admin/themes/active", srv.base))
        .header(header::COOKIE, &admin)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({"revision_id":ids[0]}))
        .send()
        .await
        .unwrap();
    assert_eq!(v3_active.status(), 200);
    let active_body = v3_active.json::<Value>().await.unwrap();
    assert_eq!(active_body["revision_id"], ids[0]);
    assert_eq!(active_body["source_format"], "v3");
    let current = list(&client, &srv.base, &admin).await;
    assert_eq!(current["selection"]["revision_id"], ids[0]);
    assert_eq!(current["selection"]["theme_id"], "f02-editorial");
    assert_eq!(current["selection"]["version"], 2);
    assert_eq!(current["active_id"], Value::Null);
    assert_eq!(
        client
            .put(format!("{}/api/admin/themes/active", srv.base))
            .header(header::COOKIE, &admin)
            .header("x-csrf-token", &csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({"revision_id":ids[2]}))
            .send()
            .await
            .unwrap()
            .status(),
        412
    );
    assert_eq!(
        client
            .put(format!("{}/api/admin/themes/active", srv.base))
            .header(header::COOKIE, &admin)
            .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, "\"v2\"")
            .json(&json!({"revision_id":"rev-missing"}))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );

    // Invalid on-disk revision must be visible as metadata but must not activate.
    let digest = rows[1]["revision_digest"].as_str().unwrap();
    let marker = root
        .join("themes")
        .join("revisions")
        .join(digest)
        .join(".complete");
    let marker_bytes = fs::read(&marker).unwrap();
    fs::remove_file(&marker).unwrap();
    assert_eq!(
        client
            .put(format!("{}/api/admin/themes/active", srv.base))
            .header(header::COOKIE, &admin)
            .header("x-csrf-token", &csrf)
            .header(header::IF_MATCH, "\"v2\"")
            .json(&json!({"revision_id":ids[2]}))
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
    assert_eq!(
        list(&client, &srv.base, &admin).await["selection"]["revision_id"],
        ids[0]
    );
    fs::write(marker, marker_bytes).unwrap();
    let switched = client
        .put(format!("{}/api/admin/themes/active", srv.base))
        .header(header::COOKIE, &admin)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, "\"v2\"")
        .json(&json!({"revision_id":ids[2]}))
        .send()
        .await
        .unwrap();
    assert_eq!(switched.status(), 200);
    assert_eq!(switched.json::<Value>().await.unwrap()["version"], 3);

    drop(srv);
    let srv2 = spawn(&root).await;
    let (admin2, _) = login(&client, &srv2.base, "admin", "AdminPassword123!").await;
    let after = list(&client, &srv2.base, &admin2).await;
    assert_eq!(after["selection"]["revision_id"], ids[2]);
    assert_eq!(after["version"], 3);
    assert_eq!(after["revisions"].as_array().unwrap().len(), 2);
    drop(srv2);
    fs::remove_dir_all(&root).unwrap();
}
