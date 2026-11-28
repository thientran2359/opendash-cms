use super::*;

#[test]
fn theme_cache_reuses_immutable_compilation_and_regenerates_content_nonce() {
    let cache = ThemeCompositionCache::new();
    let first_digest = "a".repeat(64);
    let second_digest = "b".repeat(64);
    let parts = BTreeMap::from([
        (
            "header".to_owned(),
            "<header>{{ od_part('leaf') }}</header>".to_owned(),
        ),
        ("leaf".to_owned(), "<span>leaf</span>".to_owned()),
    ]);
    let policy = ThemeUrlPolicy::default();
    let body_source = "<main>{{ od_content() }}{{ od_part('header') }}</main>";
    let body = cache
        .get_or_compile(
            &first_digest,
            "page",
            body_source,
            &parts,
            CsecContentKind::Entry,
            &policy,
        )
        .unwrap();
    assert_eq!(body.max_part_depth, 2);
    let cached = cache
        .get_or_compile(
            &first_digest,
            "page",
            body_source,
            &parts,
            CsecContentKind::Entry,
            &policy,
        )
        .unwrap();
    assert!(Arc::ptr_eq(&body, &cached));
    let other_revision = cache
        .get_or_compile(
            &second_digest,
            "page",
            body_source,
            &parts,
            CsecContentKind::Entry,
            &policy,
        )
        .unwrap();
    assert!(!Arc::ptr_eq(&body, &other_revision));

    let header_source = parts.get("header").unwrap();
    let header = cache
        .get_or_compile(
            &first_digest,
            "part:header",
            header_source,
            &parts,
            CsecContentKind::NonEntry,
            &policy,
        )
        .unwrap();
    assert_eq!(header.max_part_depth, 1);
    let cached_header = cache
        .get_or_compile(
            &first_digest,
            "part:header",
            header_source,
            &parts,
            CsecContentKind::NonEntry,
            &policy,
        )
        .unwrap();
    assert!(Arc::ptr_eq(&header, &cached_header));

    // The immutable compiled program retains the internal slot token.
    // Rendering creates a new random 128-bit nonce for every request.
    let render = || {
        body.render(
            json_to_safe_value(&serde_json::json!({})),
            Some("<p>ok</p>"),
        )
    };
    assert_eq!(render().unwrap(), render().unwrap());
    let invalid_digest = cache
        .get_or_compile(
            "invalid",
            "page",
            body_source,
            &parts,
            CsecContentKind::Entry,
            &policy,
        )
        .unwrap();
    let uncached = cache
        .get_or_compile(
            "invalid",
            "page",
            body_source,
            &parts,
            CsecContentKind::Entry,
            &policy,
        )
        .unwrap();
    assert!(!Arc::ptr_eq(&invalid_digest, &uncached));
}

#[test]
fn curated_icon_is_replaced_only_after_document_normalization() {
    let compiled = validate_and_compile_csec_composition(
        "page",
        "<main>{{ od_icon('calendar', 'w-3.5 h-3.5') }}{{ od_content() }}</main>",
        &BTreeMap::new(),
        CsecContentKind::Entry,
    )
    .unwrap();
    let mut budget = RenderBudget::new();
    let shell = compiled
        .render_with_config_and_budget(
            json_to_safe_value(&serde_json::json!({})),
            Some("<p>OK</p>"),
            &mut budget,
            &RenderFormattingConfig::default(),
        )
        .unwrap();
    assert!(shell.contains(ICON_NONCE_PREFIX));
    assert!(!shell.contains("<svg"));
    // Exercise the same document-normalization boundary without coupling
    // this module's unit tests to the server's SEO/content-render modules.
    let composed = format!("<!doctype html><html><body>{shell}</body></html>");
    let final_html = budget
        .icon_nonces
        .substitute_into_document(composed)
        .unwrap();
    assert!(final_html.contains("class=\"w-3.5 h-3.5\""));
    assert!(final_html.contains("<svg xmlns="));
    assert!(!final_html.contains(ICON_NONCE_PREFIX));
}

#[test]
fn icon_security_validation_rejects_unsafe_contexts() {
    let invalid = [
        "<main>OD_ICON_0123456789abcdef0123456789abcdef</main>",
        "<main><iframe src='/x'></iframe></main>",
        "<main>{{ od_icon('not-in-registry') }}</main>",
        "<main>{{ od_icon('calendar', 'w-3 onclick=alert(1)') }}</main>",
        "<main>{{ od_icon('calendar') | upper }}</main>",
        "<main>{{ od_icon(name) }}</main>",
        "<div title=\"{{ od_icon('calendar') }}\">x</div>",
        "<style>{{ od_icon('calendar') }}</style>",
        "{% set foo %}{{ od_icon('calendar') }}{% endset %}<main>x</main>",
        "{% set od_icon = 'fake' %}<main>test</main>",
    ];
    for source in invalid {
        assert!(
            validate_and_compile_csec_composition(
                "t",
                source,
                &BTreeMap::new(),
                CsecContentKind::NonEntry
            )
            .is_err(),
            "should reject: {source}"
        );
    }
    assert!(
        render_csec(
            "legacy",
            "<main>{{ od_icon('calendar') }}</main>",
            Value::from(())
        )
        .is_err()
    );
}

#[test]
fn icon_nonce_rejects_attribute_and_style_and_duplicate_substitution() {
    let nonce = "OD_ICON_0123456789abcdef0123456789abcdef".to_owned();
    let table = IconNonceTable::from_test_entries(vec![(nonce.clone(), "<svg/>".to_owned())]);
    assert!(
        table
            .substitute_into_document(format!("<p>{nonce}</p>"))
            .unwrap()
            .contains("<svg/>")
    );
    assert!(
        table
            .substitute_into_document(format!("<div title=\"{nonce}\"></div>"))
            .is_err()
    );
    assert!(
        table
            .substitute_into_document(format!("<!-- {nonce} -->"))
            .is_err()
    );
    assert!(
        table
            .substitute_into_document(format!("<style>{nonce}</style>"))
            .is_err()
    );
    assert!(
        table
            .substitute_into_document(format!("<p>{nonce}{nonce}</p>"))
            .is_err()
    );
    assert!(
        table
            .substitute_into_document("<p>nonce missing</p>".to_owned())
            .is_ok()
    );
}

#[test]
fn richtext_nonce_html_scan_handles_lt_text_and_nested_lt_tag() {
    let mut table = RichTextNonceTable::default();
    let nonce = table.register_sanitized("<p>safe</p>".to_owned()).unwrap();
    let ok_html = format!("<p>1 < 2 it's ok</p><p>< !doctype></p><div>{nonce}</div>");
    let rendered = table.substitute_into_document(ok_html).unwrap();
    assert!(rendered.contains("<div><p>safe</p></div>"));
    assert!(!rendered.contains(&nonce));

    assert!(
        table
            .substitute_into_document(format!("<<script>{nonce}</script>"))
            .is_err()
    );
    assert!(
        table
            .substitute_into_document(format!("<<div data-x=\"{nonce}\"></div>"))
            .is_err()
    );
}

#[test]
fn icon_budget_caps_128_total_across_segments_and_charges_growth() {
    let compiled = validate_and_compile_csec_composition(
        "p",
        "<p>{{ od_icon('check') }}</p>",
        &BTreeMap::new(),
        CsecContentKind::NonEntry,
    )
    .unwrap();
    let mut budget = RenderBudget::new();
    for _ in 0..MAX_ICONS_PER_REQUEST {
        compiled
            .render_with_config_and_budget(
                Value::from(()),
                None,
                &mut budget,
                &RenderFormattingConfig::default(),
            )
            .unwrap();
    }
    assert_eq!(budget.icon_nonces.entries.len(), MAX_ICONS_PER_REQUEST);
    assert!(budget.html_bytes_used > 0);
    assert!(
        compiled
            .render_with_config_and_budget(
                Value::from(()),
                None,
                &mut budget,
                &RenderFormattingConfig::default()
            )
            .is_err()
    );

    let mut budget = RenderBudget::new();
    budget.html_bytes_used = MAX_RENDERED_HTML_BYTES - 1;
    assert!(
        compiled
            .render_with_config_and_budget(
                Value::from(()),
                None,
                &mut budget,
                &RenderFormattingConfig::default()
            )
            .is_err()
    );
    assert!(budget.html_bytes_used > MAX_RENDERED_HTML_BYTES);
    assert!(budget.icon_nonces.is_empty());
}

#[test]
fn declarative_block_icon_renders_once_and_rejects_attribute_spoofing() {
    let source = "<div>{{ od_icon('arrow-right', 'w-3.5') }}</div>";
    let schema: opendash_core::block_schema::BlockSchema =
        serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "id": "test-icon",
            "title": "Test",
            "category": "content",
            "render": "render.html",
            "fields": []
        }))
        .unwrap();
    validate_block_template_csec("test-icon", source, &schema).unwrap();
    let mut budget = RenderBudget::new();
    let props = BTreeMap::new();
    let fmt_cfg = RenderFormattingConfig::default();
    let shell = render_block_csec_with_inputs(
        "test-icon",
        source,
        BlockRenderInputs {
            hydrated_props: &props,
            local_richtext_nonces: None,
            merged_options: None,
            site_options: None,
            menus: None,
            children_html: None,
            children_meta: None,
            parent_ctx: None,
            is_preview: false,
            fmt_cfg: &fmt_cfg,
        },
        &mut budget,
    )
    .unwrap();
    assert!(shell.contains(ICON_NONCE_PREFIX));
    assert!(!shell.contains("<svg"));
    let final_html = budget.icon_nonces.substitute_into_document(shell).unwrap();
    assert!(final_html.contains("<svg "));
    assert!(final_html.contains("class=\"w-3.5\""));
    for invalid in [
        "<div title=\"{{ od_icon('check') }}\">hi</div>",
        "<div>{{ od_icon('missing') }}</div>",
        "{% set captured %}{{ od_icon('check') }}{% endset %}<div>ok</div>",
        "<div>OD_ICON_0123456789abcdef0123456789abcdef</div>",
        "<div><object data='/x'></object></div>",
    ] {
        assert!(
            validate_block_template_csec("test-icon", invalid, &schema).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn theme_url_option_validation_covers_scalar_boundaries_and_entity_bypasses() {
    for allowed in [
        "/lien-he?from=header#form",
        "https://example.test/path?q=1&x=2",
        "#form",
        "mailto:hello@example.test",
        "tel:+842812345678",
        &format!("/{}", "🦀".repeat(4095)),
    ] {
        assert!(validate_theme_option_url(allowed).is_ok(), "{allowed}");
    }
    for rejected in [
        "",
        " ",
        " /a",
        "/a ",
        "/a b",
        "/a\n",
        "//evil.test",
        "/a\\b",
        "https://user:pass@example.test/",
        "javascript:alert(1)",
        "data:text/html,bad",
        "file:///etc/passwd",
        "vbscript:bad",
        "/a&#10;bad",
        "/a&quot;onmouseover",
        "/a&#x27;bad",
        "/a`bad",
        "http://evil.test@other.test/",
        &format!("/{}", "🦀".repeat(4096)),
    ] {
        assert!(validate_theme_option_url(rejected).is_err(), "{rejected}");
    }
}

#[test]
fn theme_url_capability_is_exact_anchor_only_and_never_inherited_by_blocks() {
    let policy = ThemeUrlPolicy::from_option_keys(["cta_url".to_owned()]);
    let src = r#"<a href="{{ options.cta_url }}">Visit</a>"#;
    assert!(validate_template_csec_with_theme_urls("a", src, &policy).is_ok());
    assert!(validate_template_csec("a", src).is_err());
    let compiled = validate_and_compile_csec_composition_with_theme_urls(
        "root",
        src,
        &BTreeMap::new(),
        CsecContentKind::NonEntry,
        &policy,
    )
    .unwrap();
    let rendered = compiled
        .render(
            json_to_safe_value(&serde_json::json!({
                "options": {"cta_url":"/lien-he?from=header&x=2"}
            })),
            None,
        )
        .unwrap();
    let rendered_href = rendered
        .split("href=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    assert_eq!(
        decode_html_entities_for_url(rendered_href),
        "/lien-he?from=header&x=2"
    );
    // Even a syntactically permitted anchor must fail the post-render
    // URL boundary when supplied an unsafe runtime/context value.
    assert!(
        compiled
            .render(
                json_to_safe_value(&serde_json::json!({
                    "options": {"cta_url": "javascript:alert(1)"}
                })),
                None,
            )
            .is_err()
    );
    for forbidden in [
        r#"<a href="{{ options.other_url }}">bad</a>"#,
        r#"<a href="{{ theme_options.cta_url }}">bad</a>"#,
        r#"<a href="{{ options.cta_url | upper }}">bad</a>"#,
        r#"<a href="{{ options['cta_url'] }}">bad</a>"#,
        r#"<a href="/prefix{{ options.cta_url }}">bad</a>"#,
        r#"<img src="{{ options.cta_url }}">"#,
        r#"<video poster="{{ options.cta_url }}"></video>"#,
        r#"<cite cite="{{ options.cta_url }}">bad</cite>"#,
        r#"<a ping="{{ options.cta_url }}">bad</a>"#,
        r#"<img srcset="{{ options.cta_url }} 1x">"#,
        r#"<div style="background:{{ options.cta_url }}">bad</div>"#,
        r#"{% set options = {'cta_url':'/evil'} %}<a href="{{ options.cta_url }}">bad</a>"#,
        r#"{% with options = {'cta_url':'/evil'} %}<a href="{{ options.cta_url }}">bad</a>{% endwith %}"#,
        r#"{% for options, n in items %}<a href="{{ options.cta_url }}">bad</a>{% endfor %}"#,
        r#"{% set options %}bad{% endset %}<a href="{{ options.cta_url }}">bad</a>"#,
    ] {
        assert!(
            validate_template_csec_with_theme_urls("root", forbidden, &policy).is_err(),
            "{forbidden}"
        );
    }
    // A nested, dormant part is checked even if the public root never visits it.
    let parts = BTreeMap::from([(
        "dormant".into(),
        r#"<a href="{{ options.not_declared }}">bad</a>"#.into(),
    )]);
    assert!(
        validate_and_compile_csec_composition_with_theme_urls(
            "root",
            "<main>Good</main>",
            &parts,
            CsecContentKind::NonEntry,
            &policy,
        )
        .is_err()
    );
}

#[test]
fn block_csec_global_options_bridge_enforces_typed_urls_and_forbids_shadowing() {
    let schema: opendash_core::block_schema::BlockSchema =
        serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "id": "demo/bridge",
            "title": "Bridge Test",
            "category": "content",
            "render": "render.html",
            "fields": [
                { "type": "boolean", "key": "show_fb", "label": "FB", "default": true }
            ]
        }))
        .unwrap();

    let valid_tpl = r#"{% if show_fb and options.social_facebook %}<a href="{{ options.social_facebook }}">FB</a>{% endif %}{% if options.site_logo and options.site_logo.url %}<img src="{{ options.site_logo.url }}" alt="Logo">{% endif %}<p>{{ options.contact_phone }}</p>"#;
    assert!(validate_block_template_csec("demo/bridge", valid_tpl, &schema).is_ok());

    for forbidden in [
        r#"<a href="{{ options.contact_phone }}">bad</a>"#,
        r#"<a href="{{ options.unknown_key }}">bad</a>"#,
        r#"<img src="{{ options.site_logo }}" alt="Logo">"#,
        r#"<a href="{{ theme_options.social_facebook }}">bad</a>"#,
        r#"<p>{{ options.social_facebook }}</p>"#,
        r#"<p>{{ options }}</p>"#,
        r#"<a href="{{ options.social_facebook | upper }}">bad</a>"#,
        r#"{% set options = {'social_facebook': '/evil'} %}<a href="{{ options.social_facebook }}">bad</a>"#,
        r#"{% with options = {'social_facebook': '/evil'} %}<a href="{{ options.social_facebook }}">bad</a>{% endwith %}"#,
        r#"{% for options, x in [] %}<a href="{{ options.social_facebook }}">bad</a>{% endfor %}"#,
    ] {
        assert!(
            validate_block_template_csec("demo/bridge", forbidden, &schema).is_err(),
            "{forbidden}"
        );
    }
}

#[test]
fn icon_symbol_dedupe_and_unique_usage_caps() {
    let compiled = validate_and_compile_csec_composition(
        "p",
        "<main>{{ od_icon('check') }}{{ od_icon('arrow-right') }}{{ od_icon('arrow-right') }}</main>",
        &BTreeMap::new(),
        CsecContentKind::NonEntry,
    )
    .unwrap();
    let mut budget = RenderBudget::new();
    let html = compiled
        .render_with_config_and_budget(
            Value::from(()),
            None,
            &mut budget,
            &RenderFormattingConfig::default(),
        )
        .unwrap();
    let out = budget
        .icon_nonces
        .substitute_into_document(format!("<html><body>{html}</body></html>"))
        .unwrap();
    assert!(!out.contains("id=\"od-sym-check\""));
    assert!(out.contains("<symbol id=\"od-sym-arrow-right\" viewBox=\"0 0 24 24\">"));
    assert_eq!(out.matches("<use href=\"#od-sym-arrow-right\"></use>").count(), 2);

    let mut unique_cap = IconNonceTable::default();
    for idx in 0..MAX_UNIQUE_ICONS_PER_REQUEST {
        unique_cap.unique_names.insert(format!("synthetic-{idx}"));
    }
    let mut bytes_used = 0usize;
    assert!(unique_cap.register_icon("check", None, &mut bytes_used).is_err());
}
