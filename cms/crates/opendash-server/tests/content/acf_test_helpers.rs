use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct TempDirGuard(pub PathBuf);
impl TempDirGuard {
    pub fn new(label: &str) -> Self {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("opendash-acf-{label}-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub struct RunningServer {
    child: Option<Child>,
    pub base: String,
}
impl RunningServer {
    pub async fn start(root: &Path) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
            .env("OPENDASH_SITE_ROOT", root)
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let client = Client::new();
        for _ in 0..240 {
            if client
                .get(format!("{base}/api/health"))
                .send()
                .await
                .is_ok_and(|r| r.status() == StatusCode::OK)
            {
                return Self {
                    child: Some(child),
                    base,
                };
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("server did not become ready at {base}");
    }
}
impl Drop for RunningServer {
    fn drop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

pub fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zw = ZipWriter::new(std::io::Cursor::new(&mut buf));
        for (name, data) in entries {
            let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            zw.start_file(*name, opts).unwrap();
            zw.write_all(data).unwrap();
        }
        zw.finish().unwrap();
    }
    buf
}

fn png_bytes() -> Vec<u8> {
    let mut img = image::RgbaImage::new(8, 8);
    for px in img.pixels_mut() {
        *px = image::Rgba([20, 140, 220, 255]);
    }
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

pub async fn login(client: &Client, base: &str, user: &str, pass: &str) -> (String, String) {
    let resp = client
        .post(format!("{base}/auth/login"))
        .json(&json!({ "username": user, "password": pass }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = resp
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap().to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = resp.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_owned())
}

pub async fn api_json(
    client: &Client,
    method: reqwest::Method,
    url: &str,
    cookie: &str,
    csrf: Option<&str>,
    if_match: Option<&str>,
    body: Option<Value>,
) -> reqwest::Response {
    let mut req = client.request(method, url).header(header::COOKIE, cookie);
    if let Some(c) = csrf {
        if let Some(origin) = url.find("/api/").map(|i| &url[..i]) {
            req = req.header(header::ORIGIN, origin);
        }
        req = req.header("X-CSRF-Token", c);
    }
    if let Some(m) = if_match {
        req = req.header(header::IF_MATCH, m);
    }
    if let Some(b) = body {
        req = req.json(&b);
    }
    req.send().await.unwrap()
}

pub async fn upload_media(
    client: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    name: &str,
) -> String {
    let resp = client
        .post(format!("{base}/api/admin/media"))
        .header(header::COOKIE, cookie)
        .header(header::ORIGIN, base)
        .header("X-CSRF-Token", csrf)
        .header(header::CONTENT_TYPE, "image/png")
        .header("X-File-Name", name)
        .body(png_bytes())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    resp.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}
