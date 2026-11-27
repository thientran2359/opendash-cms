use super::*;

#[test]
fn parses_v3_theme_templates_parts_layouts_and_pattern_document() {
    let manifest = br#"{
      "manifest_version": 3,
      "theme_id": "sample-theme", "name": "Sample", "version": "1.0.0",
      "templates": [
        {"kind": "index", "file": "index.html"},
        {"kind": "page", "file": "page.html"}
      ],
      "parts": {"header": "header.html"},
      "page_layouts": [{"key":"canvas", "label":"Canvas", "file":"page-layouts/canvas.html"}],
      "patterns": [{"key":"hero", "title":"Hero", "category":"banner", "file":"patterns/hero.json", "preview_asset":"assets/hero.png"}],
      "assets": ["assets/hero.png"]
    }"#;
    let zip = build_zip(&[
        ("theme.json", manifest, CompressionMethod::Stored),
        (
            "index.html",
            b"<main>{{ od_content() }}</main>",
            CompressionMethod::Stored,
        ),
        (
            "page.html",
            b"<main>{{ od_part(\"header\") }}{{ od_content() }}</main>",
            CompressionMethod::Stored,
        ),
        (
            "header.html",
            b"<header>Header</header>",
            CompressionMethod::Stored,
        ),
        (
            "page-layouts/canvas.html",
            b"<main>{{ od_content() }}</main>",
            CompressionMethod::Stored,
        ),
        (
            "patterns/hero.json",
            br#"{"schema_version":1,"blocks":[]}"#,
            CompressionMethod::Stored,
        ),
        (
            "assets/hero.png",
            b"\x89PNG\r\n\x1a\n",
            CompressionMethod::Stored,
        ),
    ]);
    let package = parse_theme_zip_v3(&zip).unwrap();
    assert_eq!(package.manifest.theme_id, "sample-theme");
    assert_eq!(package.files.len(), 7);
    assert!(package.manifest_json.contains("manifest_version"));
}

#[test]
fn v3_d01_accepts_exact_declared_assets_and_raster_preview() {
    let mut manifest = minimal_v3_manifest();
    manifest["assets"] = serde_json::json!([
        "assets/styles.css",
        "assets/images/hero.webp",
        "assets/fonts/body.woff2"
    ]);
    manifest["patterns"] = serde_json::json!([{
        "key": "hero", "title": "Hero", "category": "banner",
        "file": "patterns/hero.json", "preview_asset": "assets/images/hero.webp"
    }]);
    let entries: &[(&str, &[u8])] = &[
        ("patterns/hero.json", br#"{"schema_version":1,"blocks":[]}"#),
        ("assets/styles.css", b"body { color: #123; }"),
        ("assets/images/hero.webp", b"RIFFdemoWEBP"),
        ("assets/fonts/body.woff2", b"woff2"),
    ];
    let pkg = parse_theme_zip_v3(&build_v3_zip(manifest, entries)).unwrap();
    assert_eq!(pkg.manifest.assets.len(), 3);
    assert_eq!(pkg.files.len(), 5);
}

#[test]
fn v3_s01_accepts_declared_utf8_js_modules_and_svg_only_within_text_cap() {
    for (path, source, mime) in [
        (
            "assets/main.js",
            "const title = 'Tiếng Việt';".as_bytes(),
            "text/javascript; charset=utf-8",
        ),
        (
            "assets/module.mjs",
            b"export const enabled = true;".as_slice(),
            "text/javascript; charset=utf-8",
        ),
        (
            "assets/icon.svg",
            br#"<svg xmlns="http://www.w3.org/2000/svg"><title>Icon</title></svg>"#.as_slice(),
            "image/svg+xml",
        ),
    ] {
        let mut manifest = minimal_v3_manifest();
        manifest["assets"] = serde_json::json!([path]);
        let pkg = parse_theme_zip_v3(&build_v3_zip(manifest.clone(), &[(path, source)])).unwrap();
        assert_eq!(pkg.files[path], source);
        assert_eq!(theme_asset_mime(path), Some(mime));

        let invalid_utf8 = [0xff, 0xfe];
        assert!(matches!(
            parse_theme_zip_v3(&build_v3_zip(manifest.clone(), &[(path, &invalid_utf8)])),
            Err(V2ZipError::BadRequest(message)) if message.contains("UTF-8")
        ));
        let boundary = vec![b'a'; V3_MAX_ACTIVE_ASSET_BYTES];
        assert!(parse_theme_zip_v3(&build_v3_zip(manifest.clone(), &[(path, &boundary)])).is_ok());
        let oversized = vec![b'a'; V3_MAX_ACTIVE_ASSET_BYTES + 1];
        assert!(matches!(
            parse_theme_zip_v3(&build_v3_zip(manifest, &[(path, &oversized)])),
            Err(V2ZipError::PayloadTooLarge(_))
        ));
    }
}

#[test]
fn v3_s01_rejects_svg_pattern_preview_and_undeclared_active_assets() {
    let mut manifest = minimal_v3_manifest();
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#;
    for (path, source) in [
        ("assets/icon.svg", svg.as_slice()),
        ("assets/main.js", b"let x = 1;".as_slice()),
        ("assets/module.mjs", b"export {};".as_slice()),
    ] {
        assert!(matches!(
            parse_theme_zip_v3(&build_v3_zip(manifest.clone(), &[(path, source)])),
            Err(V2ZipError::BadRequest(_))
        ));
    }
    manifest["assets"] = serde_json::json!(["assets/icon.svg"]);
    manifest["patterns"] = serde_json::json!([{
        "key": "hero", "title": "Hero", "category": "banner",
        "file": "patterns/hero.json", "preview_asset": "assets/icon.svg"
    }]);
    assert!(matches!(
        parse_theme_zip_v3(&build_v3_zip(
            manifest,
            &[
                ("patterns/hero.json", br#"{"schema_version":1,"blocks":[]}"#),
                ("assets/icon.svg", svg),
            ],
        )),
        Err(V2ZipError::BadRequest(_))
    ));
}

#[test]
fn v3_d01_rejects_missing_undeclared_and_duplicate_assets() {
    let manifest = minimal_v3_manifest();
    let assets = &[("assets/extra.png", b"image".as_slice())];
    assert!(matches!(
        parse_theme_zip_v3(&build_v3_zip(manifest.clone(), assets)),
        Err(V2ZipError::BadRequest(_))
    ));

    let mut missing = manifest.clone();
    missing["assets"] = serde_json::json!(["assets/missing.png"]);
    assert!(matches!(
        parse_theme_zip_v3(&build_v3_zip(missing, &[])),
        Err(V2ZipError::BadRequest(_))
    ));

    let mut declared = manifest.clone();
    declared["assets"] = serde_json::json!(["assets/extra.png"]);
    // ZipWriter refuses duplicate filenames itself. Build a structurally
    // valid archive with two distinct equal-width names, then substitute
    // the second filename in its local and central directory headers.
    // CRC covers entry contents, not filename bytes.
    let mut duplicate_zip = build_v3_zip(
        declared.clone(),
        &[("assets/extra.png", b"a"), ("assets/other.png", b"b")],
    );
    // Prove the original ZIP is structurally valid and both distinctly
    // named regular members are readable before rewriting its headers.
    let mut original = zip::ZipArchive::new(std::io::Cursor::new(duplicate_zip.as_slice()))
        .expect("pre-rewrite ZIP must open");
    assert_eq!(original.len(), 3);
    for (index, expected_name, expected_bytes) in [
        (1, "assets/extra.png", b"a".as_slice()),
        (2, "assets/other.png", b"b".as_slice()),
    ] {
        let mut member = original
            .by_index(index)
            .expect("regular ZIP member must open");
        assert_eq!(
            member.name().expect("pre-rewrite ZIP member name").as_ref(),
            expected_name
        );
        let mut decoded = Vec::new();
        member
            .read_to_end(&mut decoded)
            .expect("pre-rewrite ZIP member must be readable");
        assert_eq!(decoded, expected_bytes);
    }
    drop(original);
    let from = b"assets/other.png";
    let to = b"assets/extra.png";
    assert_eq!(from.len(), to.len());
    let mut replaced = 0_usize;
    for idx in 0..=(duplicate_zip.len() - from.len()) {
        if duplicate_zip[idx..idx + from.len()] == from[..] {
            duplicate_zip[idx..idx + from.len()].copy_from_slice(to);
            replaced += 1;
        }
    }
    assert_eq!(
        replaced, 2,
        "ZIP local and central names must both be rewritten"
    );
    let duplicate_result = parse_theme_zip_v3(&duplicate_zip);
    assert!(
        matches!(&duplicate_result, Err(V2ZipError::BadRequest(_))),
        "duplicate regular ZIP member must be rejected (any safe layer): {duplicate_result:?}"
    );
    declared["assets"] = serde_json::json!(["assets/extra.png", "assets/extra.png"]);
    assert!(matches!(
        parse_theme_zip_v3(&build_v3_zip(declared, assets)),
        Err(V2ZipError::BadRequest(_))
    ));
}

#[test]
fn v3_d01_rejects_hidden_traversal_unsafe_extension_and_long_paths() {
    let manifest = minimal_v3_manifest();
    let too_long = format!("assets/{}.png", "a".repeat(237));
    for path in [
        "assets/.private/hidden.png",
        "assets/icons.v2/logo.png",
        "assets/a..b.png",
        "assets/../escape.png",
        "../escape.png",
        "assets\\escape.png",
        "assets/evil.wasm",
        "assets/evil.zip",
        "assets/evil.html",
        "assets/images/.secret.png",
        too_long.as_str(),
    ] {
        let zip = build_v3_zip(manifest.clone(), &[(path, b"not safe")]);
        assert!(
            matches!(parse_theme_zip_v3(&zip), Err(V2ZipError::BadRequest(_))),
            "unexpected acceptance: {path}"
        );
    }
}

#[test]
fn v3_d01_ignores_safe_empty_directories_but_rejects_hidden_children() {
    let manifest = minimal_v3_manifest();
    let empty_dirs = &[
        ("assets/", b"".as_slice()),
        ("assets/images/", b""),
        ("assets/.hidden/", b""),
    ];
    let package = parse_theme_zip_v3(&build_v3_zip(manifest.clone(), empty_dirs)).unwrap();
    assert_eq!(package.files.len(), 1);
    let hidden_file = &[
        ("assets/.hidden/", b"".as_slice()),
        ("assets/.hidden/secret.png", b"hidden"),
    ];
    assert!(matches!(
        parse_theme_zip_v3(&build_v3_zip(manifest, hidden_file)),
        Err(V2ZipError::BadRequest(_))
    ));
}

#[test]
fn v3_d01_rejects_invalid_preview_css_and_oversized_assets() {
    let mut manifest = minimal_v3_manifest();
    manifest["assets"] = serde_json::json!(["assets/styles.css"]);
    manifest["patterns"] = serde_json::json!([{
        "key": "hero", "title": "Hero", "category": "banner",
        "file": "patterns/hero.json", "preview_asset": "assets/styles.css"
    }]);
    let entries: &[(&str, &[u8])] = &[
        ("patterns/hero.json", br#"{"schema_version":1,"blocks":[]}"#),
        ("assets/styles.css", b"body{}"),
    ];
    assert!(matches!(
        parse_theme_zip_v3(&build_v3_zip(manifest.clone(), entries)),
        Err(V2ZipError::BadRequest(_))
    ));

    manifest.as_object_mut().unwrap().remove("patterns");
    let unsafe_css = &[(
        "assets/styles.css",
        b"@import url(https://evil.test/a.css);".as_slice(),
    )];
    assert!(matches!(
        parse_theme_zip_v3(&build_v3_zip(manifest.clone(), unsafe_css)),
        Err(V2ZipError::BadRequest(_))
    ));

    let giant_css = vec![b'a'; V2_MAX_TEXT_BYTES + 1];
    let giant_zip = build_v3_zip(manifest, &[("assets/styles.css", &giant_css)]);
    assert!(matches!(
        parse_theme_zip_v3(&giant_zip),
        Err(V2ZipError::PayloadTooLarge(_))
    ));
}

#[test]
fn v3_d01_rejects_malformed_declarations_and_field_groups() {
    let manifest = minimal_v3_manifest();
    for bad in [
        serde_json::json!({"declarations":{"content_types":"not an array"}}),
        serde_json::json!({"field_groups":"not an array"}),
        serde_json::json!({"declarations":{"field_groups":[]},"field_groups":[]}),
    ] {
        let mut variant = manifest.clone();
        variant
            .as_object_mut()
            .unwrap()
            .extend(bad.as_object().unwrap().clone());
        assert!(matches!(
            parse_theme_zip_v3(&build_v3_zip(variant, &[])),
            Err(V2ZipError::BadRequest(_))
        ));
    }
}

#[test]
fn rejects_v3_undeclared_templates_and_unsafe_archive_paths() {
    let manifest = br#"{
      "manifest_version":3, "theme_id":"sample-theme", "name":"Sample", "version":"1.0.0",
      "templates":[{"kind":"index", "file":"index.html"}]
    }"#;
    for unwanted in [
        "unlisted.html",
        "../escape.html",
        "assets/inject.js",
        "assets/.private/hidden.png",
        "unknown/foo.html",
    ] {
        let zip = build_zip(&[
            ("theme.json", manifest, CompressionMethod::Stored),
            (
                "index.html",
                b"<main>Static</main>",
                CompressionMethod::Stored,
            ),
            (unwanted, b"should be rejected", CompressionMethod::Stored),
        ]);
        assert!(
            matches!(parse_theme_zip_v3(&zip), Err(V2ZipError::BadRequest(_))),
            "{unwanted}"
        );
    }
}

#[test]
fn rejects_v3_untrusted_template_and_pattern_content() {
    let manifest = br#"{
      "manifest_version":3, "theme_id":"sample-theme", "name":"Sample", "version":"1.0.0",
      "templates":[{"kind":"page", "file":"page.html"}]
    }"#;
    for template in [
        b"<main><object>unsafe</object>{{ od_content() }}</main>".as_slice(),
        b"<main>{% include 'evil.html' %}{{ od_content() }}</main>".as_slice(),
        b"<main>{{ od_content() }}{{ od_content() }}</main>".as_slice(),
        b"<div title=\"{{ od_content() }}\"></div>".as_slice(),
    ] {
        let zip = build_zip(&[
            ("theme.json", manifest, CompressionMethod::Stored),
            ("page.html", template, CompressionMethod::Stored),
        ]);
        assert!(matches!(
            parse_theme_zip_v3(&zip),
            Err(V2ZipError::BadRequest(_))
        ));
    }

    let manifest_with_pattern = br#"{
      "manifest_version":3, "theme_id":"sample-theme", "name":"Sample", "version":"1.0.0",
      "patterns":[{"key":"hero", "title":"Hero", "category":"banner", "file":"patterns/hero.json"}]
    }"#;
    let zip = build_zip(&[
        ("theme.json", manifest_with_pattern, CompressionMethod::Stored),
        ("patterns/hero.json", br#"{"schema_version":1,"blocks":[{"id":"broken","type":"core/section","children":[{"id":"broken","type":"core/row","children":[]}]}]}"#, CompressionMethod::Stored),
    ]);
    assert!(matches!(
        parse_theme_zip_v3(&zip),
        Err(V2ZipError::BadRequest(_))
    ));
}
