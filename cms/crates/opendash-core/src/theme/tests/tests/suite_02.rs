use super::*;

#[test]
fn candidate_keys_cover_all_dimensions_and_reject_ambiguous_targets() {
    use TemplateCandidateKind as Kind;
    for (kind, key) in [
        (Kind::Home, "home"),
        (Kind::Page, "page"),
        (Kind::Single, "single"),
        (Kind::Archive, "archive"),
        (Kind::Taxonomy, "taxonomy"),
        (Kind::Singular, "singular"),
        (Kind::Index, "index"),
        (Kind::NotFound, "not_found"),
    ] {
        assert_eq!(TemplateCandidate::new(kind).canonical_key().unwrap(), key);
    }
    let mut candidate = TemplateCandidate::new(Kind::Single);
    candidate.content_type = Some("post".into());
    assert_eq!(candidate.canonical_key().unwrap(), "single:type:post");
    candidate.selector_slug = Some("hello-world".into());
    assert_eq!(
        candidate.canonical_key().unwrap(),
        "single:type:post:slug:hello-world"
    );
    candidate.selector_slug = None;
    candidate.entry_id = Some(42);
    assert_eq!(candidate.canonical_key().unwrap(), "single:type:post:id:42");
    candidate.selector_slug = Some("hello-world".into());
    assert!(candidate.canonical_key().is_err());
    candidate.entry_id = Some(0);
    candidate.selector_slug = None;
    assert!(candidate.canonical_key().is_err());

    let mut candidate = TemplateCandidate::new(Kind::Archive);
    candidate.content_type = Some("project".into());
    assert_eq!(candidate.canonical_key().unwrap(), "archive:type:project");
    candidate.kind = Kind::Page;
    assert!(candidate.canonical_key().is_err());

    let mut candidate = TemplateCandidate::new(Kind::Taxonomy);
    candidate.taxonomy = Some("category".into());
    assert_eq!(candidate.canonical_key().unwrap(), "taxonomy:key:category");
    candidate.selector_slug = Some("news".into());
    assert_eq!(
        candidate.canonical_key().unwrap(),
        "taxonomy:key:category:slug:news"
    );
    candidate.selector_slug = None;
    candidate.term_id = Some(7);
    assert_eq!(
        candidate.canonical_key().unwrap(),
        "taxonomy:key:category:id:7"
    );
    candidate.content_type = Some("post".into());
    assert!(candidate.canonical_key().is_err());

    let mut candidate = TemplateCandidate::new(Kind::SingleCustom);
    candidate.custom_key = Some("feature".into());
    assert_eq!(candidate.canonical_key().unwrap(), "single_custom:feature");
    let mut candidate = TemplateCandidate::new(Kind::PageLayout);
    candidate.layout_key = Some("full_width".into());
    assert_eq!(candidate.canonical_key().unwrap(), "page_layout:full_width");
    let mut candidate = TemplateCandidate::new(Kind::Part);
    candidate.part_key = Some("post-card".into());
    assert_eq!(candidate.canonical_key().unwrap(), "part:post-card");
    candidate.part_key = Some("header:injected".into());
    assert!(candidate.canonical_key().is_err());
}

#[test]
fn v3_rejects_paths_outside_their_declared_directories() {
    let valid = v3_manifest();
    for path in [
        "templates/index.html",
        "../index.html",
        "/index.html",
        "index.htm",
        "page-layouts/canvas.html",
        "index.html/other",
    ] {
        let mut manifest = valid.clone();
        manifest.templates[0].file = path.into();
        assert!(manifest.validate().is_err(), "template: {path}");
    }
    for (key, path) in [
        ("header", "template-part/header.html"),
        ("footer", "footer-other.html"),
        ("breadcrumbs", "header.html"),
        ("breadcrumbs", "template-part/../secret.html"),
        ("breadcrumbs", "template-part/deep/a.html"),
    ] {
        let mut manifest = valid.clone();
        manifest.parts.insert(key.into(), path.into());
        assert!(manifest.validate().is_err(), "part: {key}/{path}");
    }
    for path in [
        "canvas.html",
        "../canvas.html",
        "page-layouts/sub/canvas.html",
    ] {
        let mut manifest = valid.clone();
        manifest.page_layouts[0].file = path.into();
        assert!(manifest.validate().is_err(), "layout: {path}");
    }
    for path in [
        "hero.json",
        "patterns/../hero.json",
        "patterns/inner/hero.json",
    ] {
        let mut manifest = valid.clone();
        manifest.patterns[0].file = path.into();
        assert!(manifest.validate().is_err(), "pattern: {path}");
    }
    for path in [
        "/assets/hero.webp",
        "assets/../secret",
        "assets/.secret",
        "assets/images//hero.webp",
        "https://evil.test/icon.webp",
    ] {
        let mut manifest = valid.clone();
        manifest.patterns[0].preview_asset = Some(path.into());
        assert!(manifest.validate().is_err(), "asset: {path}");
    }
}

#[test]
fn v3_rejects_duplicate_keys_and_invalid_custom_templates() {
    let valid = v3_manifest();
    let mut manifest = valid.clone();
    manifest.page_layouts.push(manifest.page_layouts[0].clone());
    assert!(manifest.validate().is_err());
    let mut manifest = valid.clone();
    manifest.patterns.push(manifest.patterns[0].clone());
    assert!(manifest.validate().is_err());
    let mut manifest = valid.clone();
    manifest
        .parts
        .insert("bad:key".into(), "template-part/other.html".into());
    assert!(manifest.validate().is_err());

    let mut manifest = valid.clone();
    let mut custom = template(TemplateCandidateKind::SingleCustom, "single-custom.html");
    custom.custom_key = Some("landing".into());
    custom.label = Some("Landing".into());
    custom.allowed_types = vec!["post".into(), "project".into()];
    manifest.templates.push(custom);
    assert!(manifest.validate().is_ok());
    manifest
        .templates
        .last_mut()
        .unwrap()
        .allowed_types
        .push("post".into());
    assert!(manifest.validate().is_err());
    manifest.templates.last_mut().unwrap().allowed_types.pop();
    manifest.templates.last_mut().unwrap().label = None;
    assert!(manifest.validate().is_err());
}

#[test]
fn v3_options_accept_null_and_reject_global_type_overrides() {
    let mut raw = serde_json::to_value(v3_manifest()).unwrap();
    raw["options"] = json!(null);
    let parsed: ThemeV3Manifest = serde_json::from_value(raw.clone()).unwrap();
    assert!(parsed.options.is_empty());
    assert!(parsed.validate().is_ok());

    // Rejects overriding global URL option with non-url type
    raw["options"] = json!([
        { "key": "social_facebook", "label": "FB", "type": "text", "public": true }
    ]);
    let bad_url: ThemeV3Manifest = serde_json::from_value(raw.clone()).unwrap();
    assert!(bad_url.validate().is_err());

    // Rejects overriding site_logo with non-image or text type
    raw["options"] = json!([
        { "key": "site_logo", "label": "Logo", "type": "text", "public": true }
    ]);
    assert!(
        serde_json::from_value::<ThemeV3Manifest>(raw.clone())
            .unwrap()
            .validate()
            .is_err()
    );
    raw["options"] = json!([
        { "key": "site_logo", "label": "Logo", "type": "media", "media_kind": "video", "public": true }
    ]);
    assert!(
        serde_json::from_value::<ThemeV3Manifest>(raw.clone())
            .unwrap()
            .validate()
            .is_err()
    );

    // Accepts compatible global overrides
    raw["options"] = json!([
        { "key": "social_facebook", "label": "FB", "type": "url", "public": true },
        { "key": "site_logo", "label": "Logo", "type": "media", "media_kind": "image", "public": true }
    ]);
    let ok_manifest: ThemeV3Manifest = serde_json::from_value(raw).unwrap();
    assert!(ok_manifest.validate().is_ok());
}
