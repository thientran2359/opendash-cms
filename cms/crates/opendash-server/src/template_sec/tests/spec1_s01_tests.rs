use super::*;
use serde_json::json;

#[test]
fn none_formatter_and_default_wrapper_keep_strict_undefined() {
    let tpl = "<p>{{ entry.fields.note }}|{{ entry.fields.note | default('N/A') }}|{{ entry.fields.note | d('') }}</p>";
    let out = render_csec(
        "none_test",
        tpl,
        json_to_safe_value(&json!({ "entry": { "fields": { "note": null } } })),
    )
    .unwrap();
    assert_eq!(decode_html_entities_for_url(&out), "<p>|N/A|</p>");

    assert_eq!(
        render_csec(
            "none_check",
            "{% if x is none and x is defined %}ok{% endif %}",
            json_to_safe_value(&json!({ "x": null })),
        )
        .unwrap(),
        "ok"
    );
    assert!(render_csec("strict_missing", "<p>{{ missing }}</p>", Value::UNDEFINED).is_err());
}

#[test]
fn date_format_supports_civil_rfc3339_naive_and_rejects_bad_inputs() {
    let cfg = RenderFormattingConfig {
        timezone: "America/New_York".to_owned(),
        ..Default::default()
    };
    let tz_hcm = RenderFormattingConfig {
        timezone: "Asia/Ho_Chi_Minh".to_owned(),
        ..Default::default()
    };
    // Civil YYYY-MM-DD must NOT shift to previous day on negative timezone
    assert_eq!(
        decode_html_entities_for_url(
            &render_csec_with_config(
                "civil",
                "{{ d | date_format }}",
                json_to_safe_value(&json!({ "d": "2026-06-15" })),
                &cfg,
            )
            .unwrap()
        ),
        "15/06/2026"
    );
    // RFC 3339 shifts from UTC midnight-ish to Asia/Ho_Chi_Minh (+7h -> 16/06)
    assert_eq!(
        render_csec_with_config(
            "rfc",
            "{{ d | date_format('%Y-%m-%d %H:%M') }}",
            json_to_safe_value(&json!({ "d": "2026-06-15T23:30:00Z" })),
            &tz_hcm,
        )
        .unwrap(),
        "2026-06-16 06:30"
    );
    // Naive SQLite treated as UTC then shifted to explicit tz arg
    assert_eq!(
        decode_html_entities_for_url(
            &render_csec_with_config(
                "sqlite",
                "{{ d | date_format('%d/%m %H:%M', 'Asia/Ho_Chi_Minh') }}",
                json_to_safe_value(&json!({ "d": "2026-06-15 23:30:00" })),
                &cfg,
            )
            .unwrap()
        ),
        "16/06 06:30"
    );
    // Malformed / blank / missing -> ""
    assert_eq!(
        render_csec_with_config(
            "bad_date",
            "{{ a | date_format }}|{{ b | date_format }}|{{ n | date_format }}",
            json_to_safe_value(&json!({ "a": "2026-1-5", "b": "not-a-date", "n": null })),
            &cfg,
        )
        .unwrap(),
        "||"
    );
    // Error cases
    for (tpl, val) in [
        ("{{ v | date_format }}", json!(123)),
        ("{{ v | date_format }}", json!("x".repeat(129))),
        ("{{ v | date_format('') }}", json!("2026-06-15")),
        (
            &format!("{{{{ v | date_format('{}') }}}}", "a".repeat(65)),
            json!("2026-06-15"),
        ),
        ("{{ v | date_format('%Z') }}", json!("2026-06-15")),
        (
            "{{ v | date_format('%Y', 'Invalid/Zone') }}",
            json!("2026-06-15T00:00:00Z"),
        ),
    ] {
        assert!(
            render_csec_with_config("err", tpl, json_to_safe_value(&json!({ "v": val })), &cfg,)
                .is_err(),
            "{tpl}"
        );
    }
}

#[test]
fn truncate_and_format_number_filters_are_exact_and_bounded() {
    let out = render_csec(
        "trunc_num",
        "{{ s | truncate(12) }}|{{ small | truncate(12) }}|{{ n | format_number }}|{{ f | format_number(2) }}|{{ neg0 | format_number(2) }}|{{ big | format_number(0, ',', '.') }}",
        json_to_safe_value(&json!({
            "s": "Xin chào thế giới Rust",
            "small": "Ngắn",
            "n": 1234567,
            "f": 1234.5,
            "neg0": -0.001,
            "big": 1.5e20
        })),
    )
    .unwrap();
    assert_eq!(
        out,
        "Xin chào...|Ngắn|1.234.567|1.234,50|0,00|150,000,000,000,000,000,000"
    );

    for (tpl, val) in [
        ("{{ v | truncate(0) }}", json!("abc")),
        ("{{ v | truncate(5, 'x'.repeat(33)) }}", json!("abcdefgh")),
        ("{{ v | truncate(5) }}", json!(123)),
        ("{{ v | truncate(5) }}", json!("a".repeat(128 * 1024 + 1))),
        ("{{ v | format_number }}", json!(true)),
        ("{{ v | format_number }}", json!([1, 2])),
        ("{{ v | format_number(7) }}", json!(12)),
        ("{{ v | format_number(0, '-----') }}", json!(12)),
        ("{{ v | format_number }}", json!("9".repeat(33))),
        ("{{ v | format_number }}", json!(1e35)),
        ("{{ v | format_number }}", json!("NaN")),
    ] {
        assert!(
            render_csec("err", tpl, json_to_safe_value(&json!({ "v": val }))).is_err(),
            "{tpl}"
        );
    }
}

#[test]
fn batch_selectattr_rejectattr_map_and_21_tests_work_with_limits() {
    let ctx = json_to_safe_value(&json!({
        "items": [
            { "id": 1, "name": "Alpha", "active": true, "cat": { "slug": "news" } },
            { "id": 2, "name": "Beta", "active": false, "cat": null },
            { "id": 3, "name": "Alpine", "active": true, "cat": { "slug": "tech" } }
        ]
    }));
    let rendered = render_csec(
        "seq_filters",
        "{% for row in [1,2,3,4] | batch(3, 0) %}[{{ row | join(',') }}]{% endfor %}|\
         {{ items | selectattr('active') | map('name') | join(',') }}|\
         {{ items | rejectattr('id', 'even') | map(attribute='cat.slug', default='none') | join(',') }}|\
         {{ items | selectattr('id', 'even') | map(attribute='cat.slug', default='none') | join(',') }}|\
         {{ items | selectattr('name', 'startingwith', 'Alp') | selectattr('cat.slug', 'in', ['news']) | map('name') | join(',') }}|\
         {% if 2 is even and 'abc' is startingwith('a') and 'abc' is endingwith('c') and 'a' is in(['a','b']) and 5 is gt(3) %}yes{% endif %}",
        ctx,
    )
    .unwrap();
    assert_eq!(
        rendered,
        "[1,2,3][4,0,0]|Alpha,Alpine|news,tech|none|Alpha|yes"
    );

    // Security & OOM bounds
    for bad in [
        "{{ items | selectattr('__class__') }}",
        "{{ items | selectattr('a.b.c.d') }}",
        "{{ items | selectattr('id', 'safe') }}",
        "{{ items | selectattr('id', 'eq') }}",
        "{{ items | selectattr('id', 'even', 1) }}",
        "{{ items | map(attribute='name', filter='upper') }}",
        "{{ items | map('name', attribute='id') }}",
        "{{ items | batch(0) }}",
        "{{ items | batch(101) }}",
        "{{ 'not_seq' | batch(2) }}",
        "{{ items[0]['__class__'] }}",
        "{{ t[dynamic_key] }}",
        "{% if 'x' is unknown_test %}bad{% endif %}",
    ] {
        assert!(
            validate_template_csec("bad", bad).is_err()
                || render_csec(
                    "bad",
                    bad,
                    json_to_safe_value(
                        &json!({ "items": [{ "name": "a", "id": 1 }], "t": {}, "dynamic_key": "k" })
                    ),
                )
                .is_err(),
            "{bad}"
        );
    }

    let too_many = json_to_safe_value(&json!({ "items": vec![json!({"id": 1}); 501] }));
    for tpl in [
        "{{ items | batch(2) }}",
        "{{ items | selectattr('id') }}",
        "{{ items | rejectattr('id') }}",
        "{{ items | map('id') }}",
    ] {
        assert!(render_csec("oom", tpl, too_many.clone()).is_err(), "{tpl}");
    }
}

#[test]
fn walk_theme_ast_enforces_theme_assets_url_scopes_and_antibypass() {
    let digest = "a".repeat(64);
    let policy = ThemeUrlPolicy::new(
        BTreeSet::from(["options.cta_url".to_owned()]),
        BTreeSet::from(["options.logo_ref".to_owned()]),
        BTreeSet::from(["assets/app.css".to_owned(), "assets/logo.png".to_owned()]),
    );

    let full_doc = r#"<!DOCTYPE html><html><head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{{ entry.title }}</title>
<link rel="stylesheet" href="{{ theme_assets['assets/app.css'] }}">
</head><body>
<p>Math: 1 < 2 and 3 > 1</p>
<a href="{{ options.cta_url }}">CTA</a>
<img src="{{ options.logo_ref }}" alt="Prefix {{ entry.title }}">
<img src="{{ theme_assets['assets/logo.png'] }}" alt="Logo">
<a href="{{ locale.home_url }}">Home</a>
<blockquote cite="{{ entry.url }}">Quote</blockquote>
<q cite="{{ options.cta_url }}">Short</q>
{% for item in locale.alternates %}<a href="{{ item.url }}">{{ item.label }}</a>{% endfor %}
{% for row in entries | batch(2) %}{% for entry in row %}<a href="{{ entry.url }}">{{ entry.title }}</a>{% for f in entry.field_list %}<img src="{{ f.media.url }}">{% for u in f.gallery %}<img src="{{ u.url }}">{% endfor %}{% if f.link %}<a href="{{ f.link.url }}">{{ f.link.label }}</a>{% endif %}{% endfor %}{% endfor %}{% endfor %}
{% if entry.primary_category %}<a href="{{ entry.primary_category.url }}">Cat</a>{% endif %}
</body></html>"#;

    validate_template_csec_with_theme_urls("doc", full_doc, &policy).unwrap();
    let compiled = validate_and_compile_csec_composition_with_theme_urls(
        "doc",
        full_doc,
        &BTreeMap::new(),
        CsecContentKind::NonEntry,
        &policy,
    )
    .unwrap();
    assert!(!compiled.uses_related_entries);
    assert!(!compiled.uses_recent_posts);
    let rendered = compiled
        .render(
            json_to_safe_value(&json!({
                "theme_assets": {
                    "assets/app.css": format!("/theme/{digest}/assets/app.css"),
                    "assets/logo.png": format!("/theme/{digest}/assets/logo.png")
                },
                "options": { "cta_url": "/contact", "logo_ref": "/media/ok.png" },
                "locale": { "home_url": "/en", "alternates": [{ "url": "/vi", "label": "VI" }] },
                "entry": { "title": "Post", "url": "/posts/1", "primary_category": { "url": "/cat/news" } },
                "entries": []
            })),
            None,
        )
        .unwrap();
    assert!(rendered.contains("Math: 1 < 2"));

    let rejected = [
        // Provenance & URL slots
        r#"<a href="{{ item.url }}">untrusted item outside loop</a>"#,
        r#"<img src="{{ field.media.url }}">"#,
        r#"<img src="{{ url }}">"#,
        r#"<img src="{{ locale.home_url }}">"#,
        r#"<img src="{{ options.cta_url }}">"#,
        r#"<cite cite="{{ options.cta_url }}">bad cite tag</cite>"#,
        r#"<link rel="stylesheet" href="{{ theme_assets['assets/logo.png'] }}">"#,
        r#"<link rel="stylesheet" href="{{ theme_assets['assets/missing.css'] }}">"#,
        r#"<a href="{{ entry.url }}/tail">split url</a>"#,
        r#"<a href="/prefix{{ entry.url }}">split url</a>"#,
        // Tombstones & loop fixed point
        r#"{% set row = [{"url":"/x"}] %}{% for entry in row %}<a href="{{ entry.url }}">x</a>{% endfor %}"#,
        r#"{% for i, entry in entries %}<a href="{{ entry.url }}">x</a>{% endfor %}"#,
        r#"{% for outer in menus.primary.items %}{% for n in [1,2] %}<a href="{{ outer.url }}">x</a>{% set outer = "x" %}{% endfor %}{% endfor %}"#,
        r#"{% for outer in menus.primary.items %}{% for n in [1,2] %}<a href="{{ outer.url }}">x</a>{% set x %}{% set outer = "bad" %}{% endset %}{% endfor %}{% endfor %}"#,
        r#"{% set unused %}<script>{{ entry.title }}</script>{% endset %}"#,
        r#"{% for x in entries %}{% else %}<div id="open"{% endfor %}"#,
        r#"{% with x = 1 %}{% set y = menus.primary.items %}{% endwith %}{% for a in menus.primary.items %}{% set a = 1 %}<a href="{{ a.url }}">x</a>{% endfor %}"#,
        // HTML/RCDATA/RAWTEXT/Tag phase anti-bypass
        "<!DOCTYPE html><html><head><title></titl{{ site.title }}e><div title=\"</title><script>alert(1)</script>\"></head><body></body></html>",
        "<!DOCTYPE html><html><head><title>x</title ><script>alert(1)</script></head><body></body></html>",
        "<?x <div title=\"><script>alert(1)</script>\">",
        "</1 <div title=\"><script>alert(1)</script>\">",
        "<!doctypex><p>bad</p>",
        "<!CDATA[bad]>",
        "<style>{% if cond %}body{}{% endif %}</style>",
        "<style>body{}</style/>",
        "<style>body{}</style >",
        "<meta charset=\"utf-8\" http-equiv=\"refresh\" content=\"0;url=/x\">",
        "{% if false %}<a href=\"javascript:alert(1)\">x</a>{% endif %}",
        "<{{ 'script' }}>alert(1)</script>",
        "<!-- a{{ options.cta_url }}><script>alert(1)</script> -->",
        "</link>",
        "</meta>",
    ];
    for tpl in rejected {
        assert!(
            validate_template_csec_with_theme_urls("reject", tpl, &policy).is_err(),
            "expected C-SEC reject for: {tpl}"
        );
    }
}

#[test]
fn compiled_composition_reports_lazy_related_and_recent_usage_flags() {
    let parts = BTreeMap::from([
        (
            "sidebar".to_owned(),
            "<aside>{% for p in recent_posts %}<a href=\"{{ p.url }}\">{{ p.title }}</a>{% endfor %}</aside>".to_owned(),
        ),
        (
            "dormant".to_owned(),
            "<div>{{ related_entries | length }}</div>".to_owned(),
        ),
    ]);
    let comp = validate_and_compile_csec_composition(
        "single",
        "<main>{{ od_content() }}{{ od_part('sidebar') }}</main>",
        &parts,
        CsecContentKind::Entry,
    )
    .unwrap();
    assert!(!comp.uses_related_entries);
    assert!(comp.uses_recent_posts);

    let comp_related = validate_and_compile_csec_composition(
        "single",
        "<main>{{ od_content() }}{% if related_entries %}<p>{{ related_entries | length }}</p>{% endif %}</main>",
        &parts,
        CsecContentKind::Entry,
    )
    .unwrap();
    assert!(comp_related.uses_related_entries);
    assert!(!comp_related.uses_recent_posts);
}

#[test]
fn untrusted_extension_html_rejects_jinja_smuggled_attributes() {
    for bad in [
        r#"<a {{ href="javascript:alert(1)" }}>Click</a>"#,
        r#"<a {% href="javascript:alert(1)" %}>Click</a>"#,
        r#"<button {{ formaction="javascript:alert(1)" }}>Submit</button>"#,
        r#"<div {{ style="background:url(https://attacker.test/exfil)" }}>x</div>"#,
        r#"<div {% style="expression(alert(1))" %}>x</div>"#,
        r#"<a href="https://safe.example" href="javascript:alert(1)">dup</a>"#,
        // WHATWG HTML5 unquoted-attribute / attribute-name quote differential payloads (F-01)
        r#"<div data-x=a'><script>alert(1)</script>'></div>"#,
        r#"<div data-x=a'><img/src="x"/onerror="alert(1)">'></div>"#,
        r#"<div a"><script>alert(1)</script>"></div>"#,
    ] {
        assert!(
            validate_untrusted_extension_html_fragment(bad).is_err(),
            "expected untrusted C-SEC validator to reject: {bad}"
        );
    }
    assert!(
        validate_untrusted_extension_html_fragment(
            r#"<div class="ok"><a href="https://safe.example">safe</a></div>"#
        )
        .is_ok()
    );
}

#[test]
fn f03_cpt_scoped_entry_fields_pass_in_cpt_template_and_fail_in_parts() {
    use opendash_core::field_schema::{FieldSpec, FieldType, MediaFieldKind};

    let mut hero = FieldSpec::new(FieldType::Media, "hero_img", "Ảnh chính");
    hero.public = true;
    hero.media_kind = Some(MediaFieldKind::Image);
    let mut cta = FieldSpec::new(FieldType::Link, "cta", "Nút");
    cta.public = true;

    let cpt_policy = ThemeUrlPolicy::default().with_cpt_entry_fields(&[hero, cta]);
    let tpl = r#"<img src="{{ entry.fields.hero_img.url }}"><a href="{{ entry.fields.cta.url }}">{{ entry.fields.cta.label }}</a>"#;
    assert!(
        validate_template_csec_with_theme_urls("single:type:product", tpl, &cpt_policy).is_ok()
    );

    let part_policy = cpt_policy.without_cpt_entry_fields();
    assert!(validate_template_csec_with_theme_urls("part:header", tpl, &part_policy).is_err());
}

#[test]
fn f09_block_template_allows_term_ref_kind() {
    use opendash_core::block_schema::{BlockCategory, BlockSchemaV1};
    use opendash_core::field_schema::{FieldSpec, FieldType};

    let mut tax_ref = FieldSpec::new(FieldType::TermRef, "topic", "Chủ đề");
    tax_ref.multiple = false;
    let schema = BlockSchemaV1 {
        schema_version: 1,
        id: "demo/topic-badge".into(),
        title: "Topic Badge".into(),
        category: BlockCategory::Content,
        icon: None,
        render: "render.html".into(),
        allowed_parents: None,
        allowed_children: None,
        fields: vec![tax_ref],
    };
    assert!(
        validate_block_template_csec(
            "demo/topic-badge",
            r#"<a href="{{ topic.url }}">{{ topic.kind }}: {{ topic.name }}</a>"#,
            &schema,
        )
        .is_ok()
    );
}
