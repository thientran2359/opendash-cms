use super::*;

#[test]
fn plugin_block_csec_escapes_values_blocks_xss_and_falls_back_safely() {
    // 1. C-SEC rejects dangerous template constructs
    assert!(
        render_plugin_block_csec(
            "acme-plugin/hero",
            "<script>alert(1)</script>",
            &BTreeMap::new()
        )
        .is_err()
    );

    // 2. C-SEC auto-escapes prop values
    let mut vals = BTreeMap::new();
    vals.insert(
        "title".into(),
        serde_json::Value::String("<script>xss()</script>Hello".into()),
    );
    let ok_html = render_plugin_block_csec(
        "acme-plugin/hero",
        "<div class=\"card\">{{ title }}</div>",
        &vals,
    )
    .expect("safe C-SEC template renders");
    assert!(!ok_html.contains("<script>"));
    assert!(ok_html.contains("&lt;script&gt;xss()"));
    assert!(ok_html.contains("Hello"));

    // 3. Builder render with active plugin block vs disabled plugin block fallback
    let doc = doc_in_section(
        "sec-1",
        "row-1",
        "col-1",
        vec![BlockNode {
            id: "el-1".into(),
            block_id: "acme-plugin/hero".into(),
            props: vals,
            children: vec![],
            meta: None,
        }],
    );

    let mut declared = crate::storage::page::embedded_core_block_defs().clone();
    let schema: opendash_core::block_schema::BlockSchema =
        serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "id": "acme-plugin/hero",
            "title": "Hero",
            "category": "content",
            "render": "render.html",
            "fields": [
                { "type": "text", "key": "title", "label": "Title" }
            ]
        }))
        .unwrap();
    declared.insert(
        "acme-plugin/hero".to_owned(),
        crate::storage::page::RegisteredBlockDef {
            block_id: "acme-plugin/hero".to_owned(),
            owner: opendash_core::registry::ResourceOwner::Plugin("acme-plugin".into()),
            status: opendash_core::registry::ProviderStatus::Active,
            declaration_state: opendash_core::registry::DeclarationState::Declared,
            schema: opendash_core::block_schema::RegisteredBlockSchema { schema },
            template: "<section class=\"hero\">{{ title }}</section>".to_owned(),
            revision_digest: "rev-test-1".to_owned(),
        },
    );
    let active_state = BuilderRenderContext {
        declared_blocks: declared,
        ..Default::default()
    };

    let rendered_active = render_builder_with_lists(&active_state, &doc, false, &BTreeMap::new())
        .expect("active render");
    assert!(
        rendered_active
            .html
            .contains("data-block-id=\"acme-plugin/hero\"")
    );
    assert!(rendered_active.html.contains("&lt;script&gt;xss()"));
    assert!(rendered_active.html.contains("Hello"));
    assert!(rendered_active.diagnostics.is_empty());

    // Disabled plugin: block absent from declared_blocks -> safe fallback without error
    let disabled_state = BuilderRenderContext {
        declared_blocks: crate::storage::page::embedded_core_block_defs().clone(),
        ..Default::default()
    };
    let rendered_disabled =
        render_builder_with_lists(&disabled_state, &doc, false, &BTreeMap::new())
            .expect("disabled fallback render must not fail");
    assert!(rendered_disabled.html.contains("builder-block-unavailable"));
    assert_eq!(
        rendered_disabled.diagnostics[0].code,
        "plugin_block_unavailable"
    );
}

#[test]
fn builder_list_uses_snapshot_canonical_href_not_legacy_slug() {
    let doc = doc_in_section(
        "sec-1",
        "row-1",
        "col-1",
        vec![node(
            "list-1",
            "core/list",
            json!({"type_key": "post"}),
            vec![],
        )],
    );
    let state = BuilderRenderContext {
        declared_blocks: crate::storage::page::embedded_core_block_defs().clone(),
        ..Default::default()
    };
    let mut lists = ListResults::new();
    lists.insert(
        "list-1".into(),
        ResolvedList::Page {
            page: 1,
            items: vec![
                crate::storage::builder::ListItem {
                    entry_id: 17,
                    title: "<unsafe>".into(),
                    slug: "old-slug".into(),
                    href: Some("/blog/new-slug".into()),
                    ..Default::default()
                },
                crate::storage::builder::ListItem {
                    entry_id: 18,
                    title: "Unlinked".into(),
                    slug: "hidden".into(),
                    href: None,
                    ..Default::default()
                },
            ],
            has_next: false,
            out_of_range: false,
        },
    );
    let rendered = render_builder_with_lists(&state, &doc, false, &lists).unwrap();
    assert!(rendered.html.contains("/blog/new-slug"));
    assert!(!rendered.html.contains("/old-slug"));
    assert!(!rendered.html.contains("href=\"/hidden\""));
    assert!(!rendered.html.contains("href=\"/content/post/hidden\""));
    assert!(rendered.html.contains("&lt;unsafe&gt;"));
    assert!(!rendered.html.contains("<unsafe>"));
}

#[test]
fn preview_annotation_scope_isolates_public_pattern_parts_and_slots() {
    let state = BuilderRenderContext {
        declared_blocks: crate::storage::page::embedded_core_block_defs().clone(),
        ..Default::default()
    };
    let pattern_doc = doc_in_section(
        "psec-1",
        "prow-1",
        "pcol-1",
        vec![node(
            "phead-1",
            "core/heading",
            json!({"level": "h3", "text": "Inner pattern"}),
            vec![],
        )],
    );
    // Pattern AST is always rendered with pattern_document = true and scope None.
    let inner_pattern = render_builder_with_annotations(
        &state,
        &pattern_doc,
        true,
        &BTreeMap::new(),
        &BTreeMap::new(),
        None,
        &BTreeMap::new(),
        true,
        RenderAnnotationScope::TemplateAll,
    )
    .unwrap();
    assert!(!inner_pattern.html.contains("data-od-editable"));
    assert!(!inner_pattern.html.contains("data-od-kind"));

    let outer_doc = doc_in_section(
        "sec-1",
        "row-1",
        "col-1",
        vec![
            node(
                "part-1",
                "core/template-part",
                json!({"part_key": "header"}),
                vec![],
            ),
            node(
                "pat-1",
                "core/pattern-ref",
                json!({"pattern_id": "7"}),
                vec![],
            ),
            node("slot-1", "core/content-slot", json!({}), vec![]),
        ],
    );
    let mut patterns = PatternResults::new();
    patterns.insert("pat-1".into(), inner_pattern);
    let mut parts = BTreeMap::new();
    parts.insert(
        "header".into(),
        "<section data-node-id=\"hdr-sec\"><p>Header</p></section>".into(),
    );
    let ctx = BuilderTemplateContext {
        content_html: Some("<main><p data-node-id=\"entry-inner\">Entry</p></main>"),
        parts: &parts,
        search: None,
    };

    // Public render (is_preview = false) and Scope::None emit zero data-od-editable attributes.
    let public_out = render_builder_with_annotations(
        &state,
        &outer_doc,
        false,
        &BTreeMap::new(),
        &BTreeMap::new(),
        Some(&ctx),
        &patterns,
        false,
        RenderAnnotationScope::TemplateAll,
    )
    .unwrap();
    assert!(!public_out.html.contains("data-od-editable"));
    assert!(!public_out.html.contains("data-od-kind"));

    let unscoped_preview = render_builder_with_patterns(
        &state,
        &outer_doc,
        true,
        &BTreeMap::new(),
        &BTreeMap::new(),
        Some(&ctx),
        &patterns,
        false,
    )
    .unwrap();
    assert!(!unscoped_preview.html.contains("data-od-editable"));

    // Scoped preview annotates only outer template nodes, never embedded pattern/part/slot internals.
    let scoped = render_builder_with_annotations(
        &state,
        &outer_doc,
        true,
        &BTreeMap::new(),
        &BTreeMap::new(),
        Some(&ctx),
        &patterns,
        false,
        RenderAnnotationScope::TemplateAll,
    )
    .unwrap();
    assert!(scoped.html.contains(
        "data-node-id=\"sec-1\" data-block-id=\"core/section\" data-od-node-scope=\"entry-sec-1\" data-od-editable=\"1\" data-od-kind=\"section\""
    ));
    assert!(scoped.html.contains(
        "data-node-id=\"row-1\" data-block-id=\"core/row\" data-od-node-scope=\"entry-row-1\" data-od-editable=\"1\" data-od-kind=\"container\""
    ));
    assert!(scoped.html.contains(
        "data-node-id=\"col-1\" data-block-id=\"core/column\" data-od-node-scope=\"entry-col-1\" data-od-editable=\"1\" data-od-kind=\"container\""
    ));
    assert!(scoped.html.contains(
        "data-node-id=\"pat-1\" data-block-id=\"core/pattern-ref\" data-od-node-scope=\"entry-pat-1\" data-od-editable=\"1\" data-od-kind=\"block\""
    ));
    assert!(scoped.html.contains(
        "data-node-id=\"psec-1\" data-block-id=\"core/section\" data-od-node-scope=\"pat_0-psec-1\">"
    ));
    assert!(scoped.html.contains(
        "data-node-id=\"prow-1\" data-block-id=\"core/row\" data-od-node-scope=\"pat_0-prow-1\">"
    ));
    assert!(scoped.html.contains("<section data-node-id=\"hdr-sec\">"));
    assert!(scoped.html.contains("<p data-node-id=\"entry-inner\">"));
}
