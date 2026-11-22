#[path = "../src/public_context.rs"]
mod public_context;

use std::collections::BTreeMap;

use public_context::{
    CanonicalUrlBuilder as Urls, PublicFilters, PublicQuery, QueryScope, TaxonomyKind, parse_query,
    validate_single_query,
};

#[test]
fn public_canonical_uses_current_typed_identity_and_reserved_roots() {
    assert_eq!(Urls::home(), "/");
    assert_eq!(Urls::archive("post").as_deref(), Some("/posts"));
    assert_eq!(Urls::archive("page"), None);
    assert_eq!(
        Urls::entry_from_id(42, "page", "hello").as_deref(),
        Some("/hello")
    );
    assert_eq!(Urls::entry_from_id(0, "page", "hello"), None);
    assert_eq!(Urls::entry("news", "first").as_deref(), Some("/news/first"));
    assert_eq!(Urls::entry("admin", "first"), None);
    assert_eq!(Urls::entry("post", "first%2Fother"), None);
    assert_eq!(Urls::entry("post", "first/other"), None);
    assert_eq!(Urls::entry("post", "first%252Fother"), None);
    assert_eq!(Urls::entry("post", ".."), None);
    assert_eq!(Urls::entry("post", "old-slug?next=1"), None);
    assert_eq!(
        Urls::taxonomy(TaxonomyKind::Category, "new").as_deref(),
        Some("/category/new")
    );
    assert_eq!(
        Urls::term_from_id(5, TaxonomyKind::Tag, "rust").as_deref(),
        Some("/tag/rust")
    );
}

#[test]
fn archive_query_is_typed_bounded_and_single_valued() {
    let parsed = parse_query(
        Some("page=10000&category_id=12&tag_id=7&include_descendants=true&utm_source=ignored"),
        QueryScope::ContentArchive,
    )
    .unwrap();
    assert_eq!(parsed.page, 10_000);
    assert_eq!(parsed.filters.category_id, Some(12));
    assert_eq!(parsed.filters.tag_id, Some(7));
    assert!(parsed.filters.descendants);
    assert_eq!(
        Urls::content_page("post", parsed).as_deref(),
        Some("/posts?page=10000&category_id=12&tag_id=7&include_descendants=true")
    );
    for raw in [
        "page=0",
        "page=10001",
        "page=01",
        "page=-1",
        "page=+1",
        "page=123456789123456789123",
        "page=abc",
        "page=2&page=3",
        "pa%67e=2&page=3",
        "category_id=0",
        "category_id=01",
        "tag_id=-2",
        "tag_id=1&tag%5Fid=2",
        "include_descendants=1",
        "include_descendants=TRUE",
        "include_descendants=true",
        "page=1%2",
        "page=1%ZZ",
    ] {
        if raw == "include_descendants=true" {
            // Requires the category to which descendants is applied.
        }
        assert!(
            parse_query(Some(raw), QueryScope::ContentArchive).is_err(),
            "{raw}"
        );
    }
    assert_eq!(
        parse_query(Some("utm=x&utm=y"), QueryScope::ContentArchive)
            .unwrap()
            .page,
        1
    );
}

#[test]
fn taxonomy_page_filters_are_and_composed_without_tracking() {
    let exact = parse_query(
        Some("page=2&foo=1"),
        QueryScope::TaxonomyArchive(TaxonomyKind::Category),
    )
    .unwrap();
    assert_eq!(exact.filters, PublicFilters::default());
    assert_eq!(
        Urls::taxonomy_page(TaxonomyKind::Category, "wildlife", exact).as_deref(),
        Some("/category/wildlife?page=2")
    );
    let descendants = parse_query(
        Some("descendants=true&tag_id=8&category_id=3&garbage=%2f"),
        QueryScope::TaxonomyArchive(TaxonomyKind::Tag),
    )
    .unwrap();
    assert_eq!(
        Urls::taxonomy_page(TaxonomyKind::Tag, "reef", descendants).as_deref(),
        Some("/tag/reef?category_id=3&tag_id=8&descendants=true")
    );
    assert!(
        parse_query(
            Some("descendants=true"),
            QueryScope::TaxonomyArchive(TaxonomyKind::Tag)
        )
        .is_err()
    );
    assert!(
        parse_query(
            Some("descendants=true&descendants=false"),
            QueryScope::TaxonomyArchive(TaxonomyKind::Category)
        )
        .is_err()
    );
    assert!(
        parse_query(
            Some("include_descendants=true"),
            QueryScope::TaxonomyArchive(TaxonomyKind::Category)
        )
        .is_err()
    );
}

#[test]
fn single_urls_ignore_tracking_but_reject_known_archive_parameters() {
    assert!(validate_single_query(Some("utm_source=abc&ref=home")).is_ok());
    assert!(validate_single_query(Some("page=1")).is_err());
    assert_eq!(
        Urls::normalize_internal_link("/new?utm_source=x").as_deref(),
        Some("/new?utm_source=x")
    );
    assert_eq!(
        Urls::normalize_internal_link("/posts?page=2").as_deref(),
        Some("/posts?page=2")
    );
    assert_eq!(Urls::normalize_internal_link("//evil.com/x"), None);
    assert_eq!(Urls::normalize_internal_link("/a\\b"), None);
    assert_eq!(Urls::normalize_internal_link("/post%2Fnew"), None);
    assert_eq!(Urls::normalize_internal_link("/post/%252F"), None);
    assert_eq!(
        Urls::normalize_internal_link("/tag/rust?foo=x").as_deref(),
        Some("/tag/rust?foo=x")
    );
    assert_eq!(
        Urls::normalize_internal_link("https://example.com/x").as_deref(),
        Some("https://example.com/x")
    );
}

#[test]
fn builder_home_pages_obey_separate_m4_limits() {
    let mut pages = BTreeMap::new();
    pages.insert("valid-node".into(), 2);
    pages.insert("another-node".into(), 100);
    pages.insert("invalid/node".into(), 7);
    pages.insert("too-high".into(), 101);
    pages.insert("first".into(), 1);
    assert_eq!(
        Urls::home_list_pages(&pages),
        "/?list_page_another-node=100&list_page_valid-node=2"
    );
    assert_eq!(Urls::home_list_pages(&BTreeMap::new()), "/");
    let query = PublicQuery {
        page: 101,
        filters: PublicFilters::default(),
    };
    assert_eq!(
        Urls::content_page("post", query),
        Some("/posts?page=101".into())
    );
}

/// Exercises actual Axum routing and raw URL decoding, beyond pure parser
/// tests. The site directory is isolated and removed after this process exits.
#[tokio::test]
async fn public_http_routes_reject_ambiguous_paths_and_filter_private_content() {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::{
        fs,
        net::TcpListener,
        process::{Command, Stdio},
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    let port_socket = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = port_socket.local_addr().unwrap().port();
    drop(port_socket);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "opendash-m5-s02-t01-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    struct Cleanup {
        child: std::process::Child,
        root: std::path::PathBuf,
    }
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
        .env("OPENDASH_SITE_ROOT", &root)
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _guard = Cleanup {
        child,
        root: root.clone(),
    };
    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");
    let mut ready = false;
    for _ in 0..50 {
        if let Ok(response) = client.get(format!("{base}/api/health")).send().await
            && response.status().is_success()
        {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(ready, "OpenDash binary must boot before HTTP matrix");

    let db_path = root.join("database/opendash.sqlite");
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(db_path))
        .await
        .unwrap();
    let category =
        sqlx::query("INSERT INTO taxonomy_terms(kind,name,slug) VALUES('category','Sea','sea')")
            .execute(&db)
            .await
            .unwrap()
            .last_insert_rowid();
    let child_category = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,parent_id) VALUES('category','Coral','coral',?)",
    )
    .bind(category)
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let tag = sqlx::query("INSERT INTO taxonomy_terms(kind,name,slug) VALUES('tag','Fish','fish')")
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
    let posted = sqlx::query(
        "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
         VALUES('post','public-article','Visible Article','{}','draft')",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
        .bind(posted)
        .execute(&db)
        .await
        .unwrap();
    let draft = sqlx::query(
        "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
         VALUES('page','secret-page','Private Draft','{}','draft')",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    for entry_id in [posted, draft] {
        for term_id in [child_category, tag] {
            sqlx::query("INSERT INTO entry_terms(entry_id,term_id) VALUES(?,?)")
                .bind(entry_id)
                .bind(term_id)
                .execute(&db)
                .await
                .unwrap();
        }
    }

    // Legacy direct-SQL setup bypasses C3's production writer. A same-value
    // permalink PUT exercises the normal CAS-checked claim sync before the
    // existing public visibility and malformed-route assertion matrix.
    let setup = client
        .post(format!("{base}/auth/setup"))
        .json(&serde_json::json!({"username":"public_context_admin","password":"Strong-Public-Context-Password-123!"}))
        .send().await.unwrap();
    assert_eq!(setup.status(), reqwest::StatusCode::CREATED);
    let login = client
        .post(format!("{base}/auth/login"))
        .json(&serde_json::json!({"username":"public_context_admin","password":"Strong-Public-Context-Password-123!"}))
        .send().await.unwrap();
    assert_eq!(login.status(), reqwest::StatusCode::OK);
    let cookie = login
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap().to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let csrf = login.json::<serde_json::Value>().await.unwrap()["csrf_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let settings = client
        .get(format!("{base}/api/admin/settings/permalinks"))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(settings.status(), reqwest::StatusCode::OK);
    let settings: serde_json::Value = settings.json().await.unwrap();
    let version = settings["version"].as_i64().unwrap();
    let input = serde_json::json!({
        "post_structure":settings["post_structure"],
        "content_type_bases":settings["content_type_bases"],
        "taxonomy_bases":settings["taxonomy_bases"]
    });
    let sync = client
        .put(format!("{base}/api/admin/settings/permalinks"))
        .header("Cookie", &cookie)
        .header("x-csrf-token", &csrf)
        .header("If-Match", format!("\"v{version}\""))
        .header("Origin", &base)
        .json(&input)
        .send()
        .await
        .unwrap();
    let status = sync.status();
    let body = sync.text().await.unwrap();
    assert_eq!(
        status,
        reqwest::StatusCode::OK,
        "fixture permalink sync: {body}"
    );

    for (path, expected_status) in [
        ("/", 200),
        ("/posts", 200),
        ("/content/post", 404),
        // EGM S01: untouched Core Page defaults to a singular-only CPT.
        ("/content/page", 404),
        ("/page/", 404),
        ("/public-article?utm_source=test", 200),
        ("/public-article?page=1", 400),
        ("/secret-page", 404),
        ("/old-slug", 404),
        ("/posts?page=2&page=3", 400),
        ("/posts?pa%67e=2&page=3", 400),
        ("/posts?page=10001", 400),
        ("/posts?category_id=0", 400),
        ("/posts?include_descendants=true", 400),
        ("/posts?utm_campaign=ignored", 200),
        // C3 CONTENT_FOUNDATION_2026_11: malformed token/encoding/traversal
        // must return 400, unlike a valid missing/private target (404).
        ("/public%2Farticle", 400),
        ("/public%252Farticle", 400),
        ("/public%5Carticle", 400),
        ("/category/missing", 404),
        ("/tag/sea", 404),
        ("/taxonomy/category/sea", 404),
        ("/category/sea", 200),
        ("/category/sea?descendants=true", 200),
        ("/category/sea?descendants=true&tag_id=999999", 400),
        ("/tag/fish", 200),
    ] {
        let response = client.get(format!("{base}{path}")).send().await.unwrap();
        assert_eq!(response.status().as_u16(), expected_status, "path: {path}");
    }
    let exact = client
        .get(format!("{base}/category/sea"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        !exact.contains("Visible Article"),
        "category is exact by default"
    );
    let descendants = client
        .get(format!("{base}/category/sea?descendants=true"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(descendants.contains("Visible Article"));
    assert!(!descendants.contains("Private Draft"));
    let tagged = client
        .get(format!("{base}/tag/fish"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(tagged.contains("Visible Article"));
    assert!(!tagged.contains("Private Draft"));
    db.close().await;
}
