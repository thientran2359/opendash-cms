use super::*;

#[tokio::test]
async fn s01_serves_revision_scoped_js_and_sandboxed_svg() {
    let root = std::env::temp_dir().join(format!(
        "opendash-theme-assets-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let db = crate::storage::init_db(&root.join("site.sqlite"))
        .await
        .unwrap();
    let themes_root = root.join("themes");
    let manifest = r#"{"manifest_version":3,"theme_id":"asset-test","name":"Asset Test","version":"1.0.0","assets":["assets/main.js","assets/module.mjs","assets/icon.svg","assets/style.css"]}"#;
    let assets: [(&str, &[u8], &str); 4] = [
        (
            "assets/main.js",
            b"console.log('hello');",
            "text/javascript; charset=utf-8",
        ),
        (
            "assets/module.mjs",
            b"export {};",
            "text/javascript; charset=utf-8",
        ),
        (
            "assets/icon.svg",
            br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#,
            "image/svg+xml",
        ),
        ("assets/style.css", b"body{}", "text/css; charset=utf-8"),
    ];
    let mut files =
        std::collections::BTreeMap::from([("theme.json".to_owned(), manifest.as_bytes().to_vec())]);
    for (path, bytes, _) in assets {
        files.insert(path.to_owned(), bytes.to_vec());
    }
    let revision = match theme::stage_and_commit_v3_revision(
        &db,
        &themes_root,
        "s01-assets",
        "asset-test",
        "Asset Test",
        "1.0.0",
        manifest,
        "{}",
        &files,
    )
    .await
    .unwrap()
    {
        RevisionInstallOutcome::Installed(revision) => revision,
        other => panic!("unexpected install result: {other:?}"),
    };
    for (path, bytes, mime) in assets {
        let response = theme_asset(
            State(db.clone()),
            Extension(ThemeStoreRoot(themes_root.clone())),
            Path((revision.revision_digest.clone(), path.to_owned())),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], mime);
        assert_eq!(
            response.headers()[header::X_CONTENT_TYPE_OPTIONS],
            "nosniff"
        );
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "public, max-age=31536000, immutable"
        );
        if mime == "image/svg+xml" {
            assert_eq!(
                response.headers()[header::CONTENT_SECURITY_POLICY],
                "default-src 'none'; style-src 'unsafe-inline'; sandbox"
            );
        } else {
            assert!(
                !response
                    .headers()
                    .contains_key(header::CONTENT_SECURITY_POLICY)
            );
        }
        assert_eq!(
            axum::body::to_bytes(response.into_body(), 1024)
                .await
                .unwrap(),
            bytes
        );
    }
    // The new MIME allowlist must still enforce immutable revision digests.
    fs::write(
        themes_root
            .join("revisions")
            .join(&revision.revision_digest)
            .join("assets/main.js"),
        b"modified",
    )
    .unwrap();
    let response = theme_asset(
        State(db.clone()),
        Extension(ThemeStoreRoot(themes_root)),
        Path((revision.revision_digest, "assets/main.js".into())),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    db.close().await;
}

#[test]
fn requires_canonical_etag() {
    let mut h = HeaderMap::new();
    assert_eq!(
        parse_version(&h).unwrap_err().0,
        StatusCode::PRECONDITION_REQUIRED
    );
    for s in ["v1", "W/\"v1\"", "\"v0\"", "\"v1\",\"v2\""] {
        h.insert(header::IF_MATCH, HeaderValue::from_str(s).unwrap());
        assert_eq!(parse_version(&h).unwrap_err().0, StatusCode::BAD_REQUEST);
    }
    h.insert(header::IF_MATCH, HeaderValue::from_static("\"v42\""));
    assert_eq!(parse_version(&h).unwrap(), 42);
}
