use super::*;

#[test]
fn v3_manifest_accepts_valid_resources_and_rejects_wrong_version_or_unknown_fields() {
    let manifest = v3_manifest();
    manifest.validate().unwrap();
    let decoded: ThemeV3Manifest =
        serde_json::from_value(serde_json::to_value(&manifest).unwrap()).unwrap();
    decoded.validate().unwrap();

    let mut wrong = manifest.clone();
    wrong.manifest_version = 2;
    assert!(wrong.validate().is_err());
    wrong = manifest.clone();
    wrong.theme_id = "../evil".into();
    assert!(wrong.validate().is_err());
    wrong = manifest.clone();
    wrong.version = "01.2.3".into();
    assert!(wrong.validate().is_err());
    let mut unknown = serde_json::to_value(manifest).unwrap();
    unknown["unexpected"] = json!("value");
    assert!(serde_json::from_value::<ThemeV3Manifest>(unknown).is_err());
}

#[test]
fn page_layout_chrome_is_backward_compatible_and_explicit() {
    let mut old = serde_json::to_value(v3_manifest()).unwrap();
    old["page_layouts"][0]
        .as_object_mut()
        .unwrap()
        .remove("chrome");
    let parsed: ThemeV3Manifest = serde_json::from_value(old).unwrap();
    assert_eq!(parsed.page_layouts[0].chrome, PageLayoutChrome::Default);
    let mut declared = serde_json::to_value(parsed).unwrap();
    declared["page_layouts"][0]["chrome"] = json!("none");
    let parsed: ThemeV3Manifest = serde_json::from_value(declared.clone()).unwrap();
    assert_eq!(parsed.page_layouts[0].chrome, PageLayoutChrome::None);
    parsed.validate().unwrap();
    declared["page_layouts"][0]["chrome"] = json!("unknown");
    assert!(serde_json::from_value::<ThemeV3Manifest>(declared).is_err());
}

#[test]
fn v3_assets_missing_field_defaults_to_empty_and_explicit_empty_is_valid() {
    let mut without_assets = serde_json::to_value(v3_manifest()).unwrap();
    without_assets.as_object_mut().unwrap().remove("assets");
    let omitted: ThemeV3Manifest = serde_json::from_value(without_assets).unwrap();
    assert!(omitted.assets.is_empty());
    omitted.validate().unwrap();

    let mut explicit_empty = v3_manifest();
    explicit_empty.assets.clear();
    explicit_empty.validate().unwrap();
    let roundtrip: ThemeV3Manifest =
        serde_json::from_value(serde_json::to_value(&explicit_empty).unwrap()).unwrap();
    assert_eq!(roundtrip.assets, Vec::<String>::new());

    let mut declared = v3_manifest();
    declared.assets = vec![
        "assets/styles.css".into(),
        "assets/images/logo.v2.png".into(),
        "assets/fonts/headline.woff2".into(),
        "assets/README.txt".to_ascii_lowercase(),
    ];
    declared.validate().unwrap();
    let decoded: ThemeV3Manifest =
        serde_json::from_value(serde_json::to_value(&declared).unwrap()).unwrap();
    assert_eq!(decoded.assets, declared.assets);
}

#[test]
fn v3_asset_path_grammar_accepts_only_safe_canonical_files() {
    for path in [
        "assets/x.css",
        "assets/note.txt",
        "assets/other.svg",
        "assets/other.js",
        "assets/other.mjs",
        "assets/photo.png",
        "assets/photo.jpg",
        "assets/photo.jpeg",
        "assets/photo.webp",
        "assets/animation.gif",
        "assets/fonts/theme.woff2",
        "assets/images_2/ui-state.v2.png",
        "assets/_.png",
        "assets/_hidden-style.css",
    ] {
        assert!(valid_v3_asset_path(path), "should accept: {path}");
        assert_eq!(valid_asset_path(path), valid_v3_asset_path(path));
    }
    let exactly_240 = format!("assets/{}.png", "a".repeat(236));
    assert_eq!(exactly_240.len() - "assets/".len(), 240);
    assert!(valid_v3_asset_path(&exactly_240));
    let too_long = format!("assets/{}.png", "a".repeat(237));
    assert!(!valid_v3_asset_path(&too_long));

    for path in [
        "",
        "assets",
        "assets/",
        "assets/logo",
        "assets/.env",
        "assets/.private/logo.png",
        "assets/icons/.logo.png",
        "assets/icons.v2/logo.png",
        "assets/a..b.png",
        "assets/a../logo.png",
        "assets/a..b/logo.png",
        "assets/../logo.png",
        "assets/./logo.png",
        "assets//logo.png",
        "assets/logos//logo.png",
        "assets/logos/",
        "/assets/logo.png",
        "assets\\logo.png",
        "assets/logos\\logo.png",
        "assets/logo.PnG",
        "assets/Logo.png",
        "assets/other.wasm",
        "assets/page.html",
        "assets/archive.zip",
        "assets/img.png?foo=bar",
        "assets/img.png#fragment",
        "https://example.test/assets/logo.png",
        "assets/ü.png",
        "assets/test\n.png",
        "assets/img.png.",
    ] {
        assert!(!valid_v3_asset_path(path), "should reject: {path:?}");
        assert_eq!(valid_asset_path(path), valid_v3_asset_path(path));
    }
}

#[test]
fn v3_manifest_rejects_duplicate_and_noncanonical_asset_declarations() {
    let baseline = v3_manifest();
    baseline.validate().unwrap();
    for bad in [
        "assets/.hidden/logo.png",
        "assets/icons.v2/logo.png",
        "assets/a..b.png",
        "assets/payload.wasm",
        "assets/icons/../../logo.png",
        "assets/icons//logo.png",
        "assets/img.PNG",
    ] {
        let mut manifest = baseline.clone();
        manifest.assets.push(bad.into());
        assert!(manifest.validate().is_err(), "{bad}");
    }
    let mut duplicated = baseline.clone();
    duplicated.assets.push("assets/styles.css".into());
    assert!(duplicated.validate().is_err());

    let mut invalid_preview = baseline.clone();
    invalid_preview.patterns[0].preview_asset = Some("assets/icons.v2/logo.png".into());
    assert!(invalid_preview.validate().is_err());
}

#[test]
fn enqueue_defaults_and_strict_fields_are_backward_compatible() {
    let mut value = serde_json::to_value(v3_manifest()).unwrap();
    assert!(value.get("styles").is_none());
    assert!(value.get("scripts").is_none());
    let legacy: ThemeV3Manifest = serde_json::from_value(value.clone()).unwrap();
    assert!(legacy.styles.is_empty() && legacy.scripts.is_empty());
    legacy.validate().unwrap();
    value["styles"] = json!([{"handle":"main", "src":"assets/styles.css"}]);
    value["scripts"] = json!([{"handle":"main", "src":"assets/main.js"}]);
    let parsed: ThemeV3Manifest = serde_json::from_value(value.clone()).unwrap();
    assert!(parsed.styles[0].conditions.is_empty());
    assert!(parsed.styles[0].media.is_none());
    assert_eq!(parsed.scripts[0].location, ScriptLocation::Footer);
    assert_eq!(parsed.scripts[0].strategy, ScriptStrategy::Defer);
    assert!(!parsed.scripts[0].module);
    value["scripts"][0]["strategy"] = json!("unknown");
    assert!(serde_json::from_value::<ThemeV3Manifest>(value.clone()).is_err());
    value["scripts"][0]["strategy"] = json!("defer");
    value["styles"][0]["unknown"] = json!(true);
    assert!(serde_json::from_value::<ThemeV3Manifest>(value).is_err());
}

#[test]
fn enqueue_resolves_route_transitive_dependencies_and_promotes_head_scripts() {
    let manifest = enqueue_manifest();
    manifest.validate().unwrap();
    let digest = "a".repeat(64);
    let url = |src: &str| format!("/theme/{digest}/assets/{src}");
    let css =
        |src: &str, attrs: &str| format!("<link rel=\"stylesheet\" href=\"{}\"{attrs}>", url(src));
    let js = |src: &str, attrs: &str| format!("<script src=\"{}\"{attrs}></script>", url(src));
    let home = resolve_enqueued_assets(&manifest, &digest, EnqueueCondition::Home);
    assert_eq!(
        home.head_style_urls,
        vec![
            url("print.css"),
            url("base.css"),
            url("layout.css"),
            url("home.css")
        ]
    );
    assert_eq!(
        home.head_styles,
        vec![
            css("print.css", " media=\"(min-aspect-ratio: 16/9)\""),
            css("base.css", ""),
            css("layout.css", ""),
            css("home.css", "")
        ]
    );
    assert_eq!(
        home.head_scripts,
        vec![
            js("base.js", " defer"),
            js("middle.mjs", " type=\"module\""),
            js("head.js", " defer")
        ]
    );
    assert_eq!(
        home.footer_scripts,
        vec![
            js("footer.js", " defer"),
            js("async.mjs", " type=\"module\" async")
        ]
    );
    let archive = resolve_enqueued_assets(&manifest, &digest, EnqueueCondition::Archive);
    assert_eq!(archive.head_style_urls, vec![url("print.css")]);
    assert!(archive.head_scripts.is_empty());
    assert_eq!(
        archive.footer_scripts,
        vec![
            js("base.js", " defer"),
            js("footer.js", " defer"),
            js("async.mjs", " type=\"module\" async"),
            js("other.js", " async")
        ]
    );
    let page = resolve_enqueued_assets(&manifest, &digest, EnqueueCondition::Page);
    assert_eq!(
        page.head_style_urls,
        vec![url("print.css"), url("base.css"), url("layout.css")]
    );
    assert!(page.head_scripts.is_empty());
    assert_eq!(
        page.footer_scripts,
        vec![
            js("base.js", " defer"),
            js("footer.js", " defer"),
            js("middle.mjs", " type=\"module\""),
            js("async.mjs", " type=\"module\" async")
        ]
    );
    for invalid in [
        "a".repeat(63),
        "A".repeat(64),
        "g".repeat(64),
        format!("{}\"", "a".repeat(63)),
    ] {
        assert_eq!(
            resolve_enqueued_assets(&manifest, &invalid, EnqueueCondition::Home),
            ResolvedEnqueuedAssets::default()
        );
    }
}

#[test]
fn enqueue_preserves_execution_order_across_async_defer_and_blocking_dependencies() {
    let digest = "a".repeat(64);
    let mut manifest = enqueue_manifest();
    manifest.scripts[1].strategy = ScriptStrategy::Blocking;
    manifest.scripts[5].deps = vec!["async".into()];
    manifest.validate().unwrap();

    // A blocking footer script cannot execute ahead of its deferred
    // dependency. Connected async scripts must also execute in order.
    let archive = resolve_enqueued_assets(&manifest, &digest, EnqueueCondition::Archive);
    assert_eq!(
        archive.footer_scripts,
        vec![
            format!("<script src=\"/theme/{digest}/assets/base.js\" defer></script>"),
            format!("<script src=\"/theme/{digest}/assets/footer.js\" defer></script>"),
            format!("<script src=\"/theme/{digest}/assets/async.mjs\" type=\"module\"></script>"),
            format!("<script src=\"/theme/{digest}/assets/other.js\" defer></script>"),
        ]
    );

    // The archive-only dependent must not alter the independent async
    // script's strategy on a different route.
    let home = resolve_enqueued_assets(&manifest, &digest, EnqueueCondition::Home);
    assert!(
        home.footer_scripts
            .iter()
            .any(|tag| tag.contains("async.mjs\" type=\"module\" async"))
    );
    assert!(
        home.head_scripts
            .last()
            .is_some_and(|tag| tag.contains("head.js\" defer"))
    );
}

#[test]
fn enqueue_validates_sources_handles_media_and_entire_dependency_graph() {
    let baseline = enqueue_manifest();
    baseline.validate().unwrap();
    let mut cases = Vec::new();
    let mut m = baseline.clone();
    m.styles[0].handle = "opendash-tailwind".into();
    cases.push(m);
    let mut m = baseline.clone();
    m.scripts[0].handle = "opendash-tailwind".into();
    cases.push(m);
    let mut m = baseline.clone();
    m.styles[0].handle = "Invalid_handle".into();
    cases.push(m);
    let mut m = baseline.clone();
    m.scripts[0].handle = "a".repeat(65);
    cases.push(m);
    let mut m = baseline.clone();
    m.styles[0].handle = m.styles[1].handle.clone();
    cases.push(m);
    let mut m = baseline.clone();
    m.scripts[0].handle = m.scripts[1].handle.clone();
    cases.push(m);
    for src in [
        "assets/missing.css",
        "assets/base.js",
        "assets/../base.css",
        "https://example.com/base.css",
    ] {
        let mut m = baseline.clone();
        m.styles[0].src = src.into();
        cases.push(m);
    }
    for src in ["assets/missing.js", "assets/base.css", "assets/icon.svg"] {
        let mut m = baseline.clone();
        m.scripts[0].src = src.into();
        cases.push(m);
    }
    for media in [
        " ".into(),
        "a".repeat(65),
        "screen\"".into(),
        "screen\n".into(),
        "é".into(),
        "a&b".into(),
    ] {
        let mut m = baseline.clone();
        m.styles[0].media = Some(media);
        cases.push(m);
    }
    for deps in [
        vec!["missing"],
        vec!["layout", "layout"],
        vec!["home"],
        vec!["opendash-tailwind", "opendash-tailwind"],
        vec!["footer"],
    ] {
        let mut m = baseline.clone();
        m.styles[0].deps = deps.into_iter().map(String::from).collect();
        cases.push(m);
    }
    for deps in [
        vec!["missing"],
        vec!["base", "base"],
        vec!["head"],
        vec!["opendash-tailwind"],
        vec!["layout"],
    ] {
        let mut m = baseline.clone();
        m.scripts[0].deps = deps.into_iter().map(String::from).collect();
        cases.push(m);
    }
    let mut m = baseline.clone();
    m.styles[3].deps.push("home".into());
    cases.push(m);
    let mut m = baseline.clone();
    m.scripts[3].deps.push("head".into());
    cases.push(m);
    let mut m = baseline.clone();
    m.styles = (0..65)
        .map(|i| ThemeStyleEnqueueDecl {
            handle: format!("style-{i}"),
            src: "assets/base.css".into(),
            deps: vec![],
            media: None,
            conditions: vec![],
        })
        .collect();
    cases.push(m);
    let mut m = baseline.clone();
    m.scripts = (0..65)
        .map(|i| ThemeScriptEnqueueDecl {
            handle: format!("script-{i}"),
            src: "assets/base.js".into(),
            deps: vec![],
            location: ScriptLocation::Footer,
            strategy: ScriptStrategy::Defer,
            module: false,
            conditions: vec![],
        })
        .collect();
    cases.push(m);
    for (i, invalid) in cases.iter().enumerate() {
        assert!(invalid.validate().is_err(), "invalid case {i}");
        assert_eq!(
            resolve_enqueued_assets(invalid, &"0".repeat(64), EnqueueCondition::NotFound),
            ResolvedEnqueuedAssets::default(),
            "invalid case {i}"
        );
    }
    let mut valid = baseline;
    valid.styles[0].media = Some("(min-resolution: 1.5dppx)".into());
    valid.validate().unwrap();
}

#[test]
fn pattern_preview_remains_raster_only() {
    for ext in [
        "png", "jpg", "jpeg", "webp", "gif", "js", "mjs", "svg", "css", "txt", "woff2",
    ] {
        let mut manifest = v3_manifest();
        let path = format!("assets/preview.{ext}");
        manifest.assets.push(path.clone());
        manifest.patterns[0].preview_asset = Some(path);
        assert_eq!(
            manifest.validate().is_ok(),
            matches!(ext, "png" | "jpg" | "jpeg" | "webp" | "gif"),
            "{ext}"
        );
    }
}

#[test]
fn v3_manifest_enforces_unique_canonical_template_targets() {
    let mut manifest = v3_manifest();
    let mut typed = template(TemplateCandidateKind::Single, "single-post.html");
    typed.content_type = Some("post".into());
    manifest.templates.push(typed.clone());
    assert!(manifest.validate().is_ok());
    typed.file = "single-duplicate.html".into();
    manifest.templates.push(typed);
    assert!(manifest.validate().is_err());

    manifest.templates.pop();
    let mut slugged = template(TemplateCandidateKind::Single, "single-post-intro.html");
    slugged.content_type = Some("post".into());
    slugged.selector_slug = Some("intro".into());
    manifest.templates.push(slugged);
    assert!(manifest.validate().is_ok());
    manifest.templates.last_mut().unwrap().selector_slug = Some("../intro".into());
    assert!(manifest.validate().is_err());
}
