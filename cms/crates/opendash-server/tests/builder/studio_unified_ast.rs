use std::{
    collections::BTreeMap,
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use opendash_core::{
    block_schema::{BlockSchema, RegisteredBlockSchema, validate_block_catalog_templates},
    builder::{BlockNode, BuilderContextTarget, BuilderDocument, MAX_BUILDER_DEPTH},
    builder_blocks::{parse_blocks_to_builder_document, serialize_builder_document_to_blocks},
    registry::{RegistryCatalog, ResourceOwner},
};
use reqwest::{Client, Method, RequestBuilder, Response, StatusCode, header};
use serde_json::{Value, json};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

const CORE_DIRS: &[&str] = &[
    "section",
    "row",
    "column",
    "stack",
    "heading",
    "text",
    "image",
    "video",
    "button",
    "list",
    "contact-form",
    "pattern-ref",
    "template-part",
    "spacer",
    "divider",
    "banner",
    "card",
    "accordion",
    "accordion-item",
    "tabs",
    "tab-panel",
    "slider",
    "slide",
    "gallery",
    "social-share",
    "popup",
    "social-follow",
    "search-form",
    "search-results",
    "site-brand",
    "language-switcher",
    "menu",
    "breadcrumbs",
    "entry-title",
    "entry-featured-image",
    "entry-meta",
    "entry-excerpt",
    "entry-content",
    "entry-terms",
    "entry-field",
    "entry-related",
    "entry-nav",
    "archive-title",
    "archive-description",
    "archive-featured-image",
    "archive-loop",
    "loop-item",
    "archive-pagination",
];

pub(super) struct Site(Child, PathBuf, pub(super) String);

impl Drop for Site {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
        let _ = fs::remove_dir_all(&self.1);
    }
}

pub(super) struct AdminSession {
    pub(super) site: Site,
    pub(super) http: Client,
    cookie: String,
    csrf: String,
}

impl AdminSession {
    pub(super) async fn boot() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "opendash-studio-ast-{}-{port}-{seq}-{nanos}",
                std::process::id()
            ));
        fs::create_dir_all(&root).unwrap();
        let log_file = fs::File::create(root.join("server.log")).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
            .env("OPENDASH_SITE_ROOT", &root)
            .env("OPENDASH_HOST", "127.0.0.1")
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log_file))
            .spawn()
            .unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let http = Client::new();
        for _ in 0..400 {
            if http
                .get(format!("{base}/api/health"))
                .send()
                .await
                .is_ok_and(|r| r.status() == 200)
            {
                let creds = json!({ "username": "admin", "password": "AdminPassword123!" });
                let setup = http
                    .post(format!("{base}/auth/setup"))
                    .header(header::ORIGIN, &base)
                    .json(&creds)
                    .send()
                    .await
                    .unwrap();
                assert_eq!(setup.status(), StatusCode::CREATED);
                let login = http
                    .post(format!("{base}/auth/login"))
                    .header(header::ORIGIN, &base)
                    .json(&creds)
                    .send()
                    .await
                    .unwrap();
                assert_eq!(login.status(), StatusCode::OK);
                let cookie = login
                    .headers()
                    .get_all(header::SET_COOKIE)
                    .iter()
                    .filter_map(|v| v.to_str().ok())
                    .map(|s| s.split(';').next().unwrap_or(s).to_owned())
                    .collect::<Vec<_>>()
                    .join("; ");
                let body: Value = login.json().await.unwrap();
                return Self {
                    site: Site(child, root, base),
                    http,
                    cookie,
                    csrf: body["csrf_token"].as_str().unwrap().to_owned(),
                };
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!(
            "server timeout: {}",
            fs::read_to_string(root.join("server.log")).unwrap_or_default()
        );
    }

    pub(super) fn auth(&self, method: Method, path: &str) -> RequestBuilder {
        self.http
            .request(method, format!("{}{path}", self.site.2))
            .header(header::COOKIE, &self.cookie)
            .header(header::ORIGIN, &self.site.2)
            .header("X-CSRF-Token", &self.csrf)
    }

    pub(super) async fn send_json(
        &self,
        method: Method,
        path: &str,
        if_match: Option<&str>,
        body: Option<&Value>,
    ) -> Response {
        let mut r = self.auth(method, path);
        if let Some(etag) = if_match {
            r = r.header(header::IF_MATCH, etag);
        }
        if let Some(v) = body {
            r = r.json(v);
        }
        r.send().await.unwrap()
    }

    pub(super) async fn upload_png(&self, name: &str) -> String {
        let mut png = Vec::new();
        let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([37, 99, 235, 255]));
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let res = self
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

    pub(super) async fn create_entry(&self, type_key: &str, title: &str, slug: &str) -> i64 {
        let p = format!("/api/admin/content-types/{type_key}/entries");
        let res = self
            .send_json(
                Method::POST,
                &p,
                None,
                Some(&json!({ "title": title, "slug": slug, "values": {} })),
            )
            .await;
        assert_eq!(res.status(), StatusCode::CREATED);
        res.json::<Value>().await.unwrap()["id"].as_i64().unwrap()
    }

    pub(super) async fn create_pattern(&self, title: &str, slug: &str) -> i64 {
        let res = self
            .send_json(
                Method::POST,
                "/api/admin/patterns",
                None,
                Some(&json!({ "title": title, "slug": slug })),
            )
            .await;
        assert_eq!(res.status(), StatusCode::CREATED);
        res.json::<Value>().await.unwrap()["id"].as_i64().unwrap()
    }

    pub(super) async fn activate_theme(&self, revision_id: &str, ver: i64) {
        let etag = format!("\"v{ver}\"");
        assert_eq!(
            self.send_json(
                Method::PUT,
                "/api/admin/themes/active",
                Some(&etag),
                Some(&json!({ "revision_id": revision_id }))
            )
            .await
            .status(),
            StatusCode::OK
        );
    }

    pub(super) async fn pub_status(&self, path: &str) -> StatusCode {
        self.http
            .get(format!("{}{path}", self.site.2))
            .send()
            .await
            .unwrap()
            .status()
    }
}

fn node(id: &str, block_id: &str, props: Value, children: Vec<BlockNode>) -> BlockNode {
    let props_map: BTreeMap<String, Value> = match props {
        Value::Object(m) => m.into_iter().collect(),
        _ => BTreeMap::new(),
    };
    BlockNode {
        id: id.into(),
        block_id: block_id.into(),
        props: props_map,
        meta: None,
        children,
    }
}

#[test]
fn core_catalog_tailwind_budget_and_ast_v1_invariants() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut catalog = RegistryCatalog::new();
    let (mut native_count, mut csec_count) = (0usize, 0usize);
    for dir in CORE_DIRS {
        let raw = fs::read_to_string(
            crate_dir
                .join("src/builtin_blocks")
                .join(dir)
                .join("block.json"),
        )
        .unwrap();
        let schema: BlockSchema =
            serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{dir}: {e}"));
        schema
            .validate("core")
            .unwrap_or_else(|e| panic!("schema invalid for {dir}: {e}"));
        if schema.render == "native" {
            native_count += 1;
        } else {
            assert_eq!(schema.render, "render.html");
            csec_count += 1;
        }
        catalog
            .register(
                schema.id.clone(),
                ResourceOwner::Core,
                RegisteredBlockSchema { schema },
            )
            .unwrap();
    }
    assert_eq!(
        (native_count, csec_count, catalog.iter().count()),
        (43, 5, 48)
    );
    validate_block_catalog_templates(&catalog).unwrap();

    let tw =
        fs::read_to_string(crate_dir.join("src/builtin_themes/opendash-tailwind.css")).unwrap();
    assert!(
        tw.len() < 120 * 1024,
        "tailwind CSS exceeds 120 KiB: {}",
        tw.len()
    );
    for tok in [
        "grid-cols-12",
        "md\\:col-span-8",
        "md\\:col-span-4",
        "sticky",
        "top-4",
        "self-start",
        "flex-col-reverse",
        "after\\:absolute",
        "after\\:inset-0",
        "after\\:z-10",
        "divide-y",
        "rounded-2xl",
        "aspect-video",
    ] {
        assert!(tw.contains(tok), "missing Tailwind token {tok}");
    }

    let doc: BuilderDocument = serde_json::from_str(
        r#"{"schema_version":1,"blocks":[{"id":"sec-rt","block_id":"core/section","children":[{"id":"row-rt","block_id":"core/row","children":[{"id":"col-rt","block_id":"core/column","children":[{"id":"hd-rt","block_id":"core/heading","props":{"level":"h2","text":"Roundtrip"}}]}]}]}]}"#,
    ).unwrap();
    assert_eq!(
        parse_blocks_to_builder_document(&serialize_builder_document_to_blocks(&doc).unwrap())
            .unwrap()
            .document,
        doc
    );

    let entry_ctx = BuilderContextTarget::EntryBody {
        content_type: "page".into(),
    };
    assert_eq!(MAX_BUILDER_DEPTH, 8);
    let make_stack = |depth: usize| -> BuilderDocument {
        let mut cur = node(&format!("stk-{depth}"), "core/stack", json!({}), vec![]);
        for d in (2..depth).rev() {
            cur = node(&format!("stk-{d}"), "core/stack", json!({}), vec![cur]);
        }
        BuilderDocument {
            schema_version: 1,
            blocks: vec![node("sec-depth", "core/section", json!({}), vec![cur])],
        }
    };
    assert!(
        make_stack(8)
            .validate_for_save(&catalog, &entry_ctx, None)
            .is_ok()
    );
    assert!(
        make_stack(9)
            .validate_for_save(&catalog, &entry_ctx, None)
            .is_err()
    );

    let acc_bad: BuilderDocument = serde_json::from_str(
        r#"{"schema_version":1,"blocks":[{"id":"sec-bad","block_id":"core/section","children":[{"id":"row-bad","block_id":"core/row","children":[{"id":"acc-bad","block_id":"core/accordion-item","props":{"title":"Illegal"}}]}]}]}"#,
    ).unwrap();
    assert!(
        acc_bad
            .validate_for_save(&catalog, &entry_ctx, None)
            .is_err()
    );

    let col_root = BuilderDocument {
        schema_version: 1,
        blocks: vec![node("col-root", "core/column", json!({}), vec![])],
    };
    assert!(
        col_root
            .validate_for_save(&catalog, &entry_ctx, None)
            .is_err()
    );
    assert!(
        serde_json::from_value::<BuilderDocument>(json!({"schema_version": 1, "sections": []}))
            .is_err()
    );
}

#[tokio::test]
async fn studio_unified_ast_http_e2e_containers_accordion_card_and_pre_v1_reject() {
    let app = AdminSession::boot().await;
    let banner_bg = app.upload_png("banner.png").await;

    let cat_res = app
        .send_json(Method::GET, "/api/admin/builder/blocks", None, None)
        .await;
    assert_eq!(cat_res.status(), StatusCode::OK);
    let items = cat_res.json::<Value>().await.unwrap()["items"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(
        items
            .iter()
            .filter(|i| i["block_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("core/"))
                && i["status"] == "active")
            .count(),
        48
    );
    assert!(!items.iter().any(|i| i["block_id"] == "core/showcase"));

    let page_id = app
        .create_entry("page", "Studio V2 E2E", "studio-v2-e2e")
        .await;
    let put_path = format!("/api/admin/entries/page/{page_id}/builder");
    assert_eq!(
        app.send_json(
            Method::PUT,
            &put_path,
            Some("\"v1\""),
            Some(&json!({ "schema_version": 1, "sections": [] }))
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );

    let doc: Value = serde_json::from_str(&r#"{"schema_version":1,"blocks":[
      {"id":"ban-1","block_id":"core/banner","props":{"bg_image":"__BG__"},"children":[
        {"id":"ban-h1","block_id":"core/heading","props":{"level":"h1","text":"Hero Banner"}},
        {"id":"ban-stk","block_id":"core/stack","children":[
          {"id":"btn-1","block_id":"core/button","props":{"cta":{"label":"Bắt đầu","url":"/start","new_tab":false,"nofollow":false}}},
          {"id":"btn-2","block_id":"core/button","props":{"cta":{"label":"Tài liệu","url":"/docs","new_tab":false,"nofollow":false}}}
        ]}
      ]},
      {"id":"sec-main","block_id":"core/section","children":[
        {"id":"row-main","block_id":"core/row","props":{"layout_mode":"grid_12","reverse_mobile":true},"children":[
          {"id":"col-l","block_id":"core/column","props":{"span":{"desktop":8}},"children":[
            {"id":"card-1","block_id":"core/card","props":{"card_link":{"url":"/promo","label":"Xem ưu đãi","new_tab":false,"nofollow":false}},"children":[
              {"id":"card-h","block_id":"core/heading","props":{"level":"h3","text":"Thẻ VIP"}},
              {"id":"card-b","block_id":"core/button","props":{"cta":{"label":"Đăng ký","url":"/vip","new_tab":false,"nofollow":false}}}
            ]}
          ]},
          {"id":"col-r","block_id":"core/column","props":{"span":{"desktop":4},"sticky":true},"children":[
            {"id":"acc-1","block_id":"core/accordion","props":{"open_first":true,"allow_multiple":false},"children":[
              {"id":"ai1","block_id":"core/accordion-item","props":{"title":"Câu hỏi 1"},"children":[{"id":"at1","block_id":"core/text","props":{"content":"<p>Đáp án 1</p>"}}]},
              {"id":"ai2","block_id":"core/accordion-item","props":{"title":"Câu hỏi 2"},"children":[{"id":"at2","block_id":"core/text","props":{"content":"<p>Đáp án 2</p>"}}]}
            ]}
          ]}
        ]}
      ]}
    ]}"#.replace("__BG__", &banner_bg)).unwrap();
    let put_res = app
        .send_json(Method::PUT, &put_path, Some("\"v1\""), Some(&doc))
        .await;
    let put_status = put_res.status();
    let put_body = put_res.text().await.unwrap_or_default();
    assert_eq!(put_status, StatusCode::OK, "PUT failed: {put_body}");

    let prev = app
        .send_json(
            Method::POST,
            &format!("{put_path}/preview"),
            None,
            Some(&doc),
        )
        .await;
    assert_eq!(prev.status(), StatusCode::OK);
    let prev_html = prev.json::<Value>().await.unwrap()["html"]
        .as_str()
        .unwrap()
        .to_owned();
    for m in [
        "data-od-editable=\"1\"",
        "data-od-kind=\"section\"",
        "data-od-kind=\"container\"",
        "data-od-kind=\"block\"",
    ] {
        assert!(prev_html.contains(m), "preview missing {m}");
    }

    let pub_path = format!("/api/admin/content-types/page/entries/{page_id}/publication");
    assert_eq!(
        app.send_json(
            Method::POST,
            &pub_path,
            Some("\"v2\""),
            Some(&json!({ "status": "published" }))
        )
        .await
        .status(),
        StatusCode::OK
    );

    let live = app
        .http
        .get(format!("{}/studio-v2-e2e", app.site.2))
        .send()
        .await
        .unwrap();
    assert_eq!(live.status(), StatusCode::OK);
    let html = live.text().await.unwrap();
    for req in [
        "data-builder-version=\"1\"",
        "od-card-stretched-link",
        "<details class=\"od-accordion-item rounded-xl\" name=\"od-acc-acc-1\" open",
        "sticky top-4 self-start",
        "md:col-span-8",
        "md:col-span-4",
    ] {
        assert!(html.contains(req), "public missing {req}: {html}");
    }
    for ban in ["data-od-editable", "data-od-kind", "<script"] {
        assert!(!html.contains(ban), "public leaked {ban}");
    }
    assert_eq!(
        app.pub_status(&format!("/media/{banner_bg}/file")).await,
        StatusCode::OK
    );
}
