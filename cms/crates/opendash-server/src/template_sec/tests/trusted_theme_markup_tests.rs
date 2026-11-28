use super::*;

fn theme(source: &str) -> Result<CompiledCsecComposition, &'static str> {
    validate_and_compile_csec_composition(
        "trusted_markup",
        source,
        &BTreeMap::new(),
        CsecContentKind::NonEntry,
    )
}

fn block_schema() -> opendash_core::block_schema::BlockSchema {
    serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "id": "trusted/markup",
        "title": "Trusted markup",
        "category": "content",
        "render": "render.html",
        "fields": [{ "type": "text", "key": "label", "label": "Label" }]
    }))
    .unwrap()
}

#[test]
fn trusted_theme_svg_scripts_events_and_raw_text_survive_compile_and_render() {
    let source = r##"<main onclick="this.dataset.clicked='1'"><svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 20 20"><title>Icon</title><use xlink:href="#symbol"/></svg><script>if (window.innerWidth < 768 && 3 > 1) { document.body.dataset.mobile = '1'; }</script></main>"##;
    let compiled = theme(source).unwrap();
    let output = compiled.render(Value::UNDEFINED, None).unwrap();
    assert_eq!(output, source);

    let nested =
        r#"<svg xmlns="http://www.w3.org/2000/svg"><svg><title>Nested</title></svg></svg>"#;
    assert_eq!(
        theme(nested)
            .unwrap()
            .render(Value::UNDEFINED, None)
            .unwrap(),
        nested
    );
}

#[test]
fn legacy_render_and_validation_stay_strict() {
    for source in [
        "<svg><title>Icon</title></svg>",
        "<script>const a = 1 < 2;</script>",
        "<button onclick=\"go()\">Click</button>",
        "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>",
    ] {
        assert!(
            validate_template_csec("legacy", source).is_err(),
            "{source}"
        );
        assert!(
            render_csec("legacy", source, Value::UNDEFINED).is_err(),
            "{source}"
        );
    }
}

#[test]
fn trusted_theme_rejects_dynamic_js_and_malformed_rawtext() {
    for source in [
        r#"<button onclick="{{ entry.title }}">Click</button>"#,
        r#"<button onclick="{% if active %}go(){% endif %}">Click</button>"#,
        r#"<button onclick="{% raw %}{{ entry.title }}{% endraw %}">Click</button>"#,
        r#"<svg onload="{{ title }}"></svg>"#,
        r#"<script>const value = "{{ entry.title }}";</script>"#,
        r#"<script>{% if cond %}go(){% endif %}</script>"#,
        r#"<script>{% for item in items %}go(){% endfor %}</script>"#,
        r#"<script>{% set a = 'x' %}go()</script>"#,
        r#"<script>{{ od_icon('check') }}</script>"#,
        r#"<script>{{ od_part('header') }}</script>"#,
        "<script>\n<!--\n/*<script>*/\n// </script>\n{{ entry.title }}\n<!-- </script> -->",
        "<svg><script><![CDATA[// </script>\n{{ entry.title }}\n//<!-- ]]> </script> --> </svg>",
        r#"<script src="/theme/app.js"/>"#,
        r#"<script>const n = 2 < 3;"#,
        r#"</script><p>orphan</p>"#,
        r#"<svg><title>Unclosed</title>"#,
        r#"</svg>"#,
        r#"<title>Not an SVG title</title>"#,
        r#"<svg><use xlink:href="javascript:alert(1)"/></svg>"#,
    ] {
        assert!(theme(source).is_err(), "accepted unsafe template: {source}");
    }
}

#[test]
fn block_template_allows_static_js_svg_but_rejects_dynamic_handlers_and_scripts() {
    let schema = block_schema();
    let source = r#"<button onclick="this.dataset.ok='1'">{{ label }}</button><svg xmlns="http://www.w3.org/2000/svg"><title>OK</title></svg><script>if (1 < 2 && 3 > 2) { document.body.dataset.ok = '1'; }</script>"#;
    validate_block_template_csec("trusted/markup", source, &schema).unwrap();
    let mut budget = RenderBudget::new();
    let props = BTreeMap::from([("label".to_owned(), serde_json::json!("<safe>"))]);
    let fmt_cfg = RenderFormattingConfig::default();
    let rendered = render_block_csec_with_inputs(
        "trusted/markup",
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
    assert!(rendered.contains("onclick=\"this.dataset.ok='1'\""));
    assert!(rendered.contains("&lt;safe&gt;"));
    assert!(rendered.contains("if (1 < 2 && 3 > 2)"));

    for source in [
        r#"<button onclick="{{ label }}">Click</button>"#,
        r#"<button onclick="{% if label %}go(){% endif %}">Click</button>"#,
        r#"<script>const value = "{{ label }}";</script>"#,
        r#"<script>{% if label %}go(){% endif %}</script>"#,
        "<script>\n<!--\n/*<script>*/\n// </script>\n{{ label }}\n<!-- </script> -->",
        "<svg><script><![CDATA[// </script>\n{{ label }}\n//<!-- ]]> </script> --> </svg>",
        r#"<script/>"#,
        r#"</script>"#,
    ] {
        assert!(
            validate_block_template_csec("trusted/markup", source, &schema).is_err(),
            "accepted unsafe block: {source}"
        );
    }
}

#[test]
fn icon_nonce_replacement_never_inserts_svg_in_script_rawtext() {
    let nonce = "OD_ICON_0123456789abcdef0123456789abcdef";
    let table = IconNonceTable::from_test_entries(vec![(nonce.to_owned(), "<svg/>".to_owned())]);
    let source = format!("<script>if (1 < 2 && 3 > 1) {{ console.log('{nonce}'); }}</script>");
    assert!(table.substitute_into_document(source).is_err());
}
