use super::*;

fn compile_theme(source: &str) -> Result<CompiledCsecComposition, &'static str> {
    validate_and_compile_csec_composition_with_theme_urls(
        "search_theme_test",
        source,
        &BTreeMap::new(),
        CsecContentKind::NonEntry,
        ThemeTemplateRole::ThemeMain,
        true,
    )
}

#[test]
fn bounded_get_search_form_in_theme_compiles_and_renders() {
    let source = r#"<search><form method="get" action="{{ locale.search_url }}" role="search"><input type="search" name="s" value="{{ search.query }}" maxlength="200"><select name="type"><option value="">All</option>{% for facet in search.facets %}<option value="{{ facet.slug }}">{{ facet.label }}</option>{% endfor %}</select><button type="submit">{{ od_icon('search', 'w-4 h-4') }}</button></form></search>"#;
    let compiled = compile_theme(source).unwrap();
    let rendered = compiled
        .render(
            minijinja::context! {
                locale => minijinja::context! {
                    home_url => "/vi/",
                    search_url => "/vi/",
                },
                search => minijinja::context! {
                    query => "rust fts5",
                    action_url => "/vi/",
                    facets => vec![minijinja::context! { slug => "post", label => "Posts" }],
                },
            },
            None,
        )
        .unwrap();
    assert!(rendered.contains(r#"<form method="get" action="/vi/" role="search">"#));
    assert!(rendered.contains(r#"<option value="post">Posts</option>"#));
    assert!(rendered.contains(r#"value="rust fts5""#));
}

#[test]
fn theme_search_form_rejects_unsafe_methods_actions_inputs_and_nesting() {
    for bad in [
        r#"<form method="post" action="/"></form>"#,
        r#"<form method="dialog" action="/"></form>"#,
        r#"<form method="GET" action="/"></form><form action="{{ dyn_action }}"></form>"#,
        r#"<form method="{{ dyn_method }}" action="/"></form>"#,
        r#"<form method="get" action="//evil.com/steal"></form>"#,
        r#"<form method="get" action="https://evil.com/steal"></form>"#,
        r#"<form method="get" action="/api/public/search"></form>"#,
        r#"<form method="get" action="/admin"></form>"#,
        r#"<form method="get" action="/?s=injected"></form>"#,
        r#"<form method="get"></form>"#,
        r#"<form method="get" action="/"/>"#,
        r#"<form method="get" action="/">unclosed"#,
        r#"<form method="get" action="/"><form method="get" action="/"></form></form>"#,
        r#"<form method="get" action="/">{{ od_part('header') }}</form>"#,
        r#"<form method="get" action="/">{{ od_content() }}</form>"#,
        r#"<form method="get" action="/"><input type="password" name="pw"></form>"#,
        r#"<form method="get" action="/"><input type="file" name="f"></form>"#,
        r#"<form method="get" action="/"><input type="{{ dyn }}" name="q"></form>"#,
        r#"<form method="get" action="/"><textarea name="s"></textarea></form>"#,
        r#"<form method="get" action="{{ entry.url }}"></form>"#,
        r#"<form method="get" action="/" formaction="/api/x"></form>"#,
        r#"<button formaction="/evil">Go</button>"#,
        r#"<svg><form method="get" action="/"></form></svg>"#,
        r#"<math><form method="get" action="/"></form></math>"#,
    ] {
        assert!(compile_theme(bad).is_err(), "expected err for: {bad}");
    }
}

#[test]
fn non_theme_render_and_blocks_still_forbid_search_forms() {
    let form_src = r#"<form method="get" action="/"><input type="search" name="s"></form>"#;
    assert!(validate_template_csec("legacy_form", form_src).is_err());
    assert!(render_csec("legacy_form", form_src, Value::UNDEFINED).is_err());
    assert!(
        render_csec_with_config(
            "part_like",
            form_src,
            Value::UNDEFINED,
            &RenderFormattingConfig::default(),
            false,
        )
        .is_err()
    );

    let schema: opendash_core::block_schema::BlockSchemaV1 = serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "id": "search/test",
        "title": "Search test",
        "category": "content",
        "render": "render.html",
        "fields": [{ "type": "text", "key": "q", "label": "Q" }]
    }))
    .unwrap();
    assert!(validate_block_template_csec("search/test", form_src, &schema).is_err());
    assert!(validate_html_and_css_context(form_src, true, false, false).is_err());
    assert!(validate_html_and_css_context(form_src, true, true, true).is_ok());
    assert!(
        validate_html_and_css_context(r#"<form method="post" action="/"></form>"#, true, true, true)
            .is_err()
    );
    assert!(
        validate_html_and_css_context(r#"<form action="/api/x"></form>"#, true, true, true).is_err()
    );
}
