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
    let doc = BuilderDocument {
        schema_version: 1,
        sections: vec![SectionNode {
            id: "sec-1".into(),
            meta: None,
            responsive: None,
            style: None,
            children: vec![RowNode {
                id: "row-1".into(),
                meta: None,
                responsive: None,
                children: vec![ColumnNode {
                    id: "col-1".into(),
                    meta: None,
                    responsive: None,
                    children: vec![ElementNode {
                        id: "el-1".into(),
                        meta: None,
                        content: ElementContent::Known(KnownElement::PluginBlock(
                            PluginBlockProps {
                                block_id: "acme-plugin/hero".into(),
                                values: vals,
                            },
                        )),
                    }],
                }],
            }],
        }],
    };

    let mut declared = BTreeMap::new();
    let schema: opendash_core::block_schema::BlockSchemaV1 =
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

    let rendered_active = render_builder_with_lists(
        &active_state,
        &doc,
        false,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
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
    let disabled_state = BuilderRenderContext::default();
    let rendered_disabled = render_builder_with_lists(
        &disabled_state,
        &doc,
        false,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("disabled fallback render must not fail");
    assert!(rendered_disabled.html.contains("builder-block-unavailable"));
    assert_eq!(
        rendered_disabled.diagnostics[0].code,
        "plugin_block_unavailable"
    );
}

#[test]
fn builder_list_uses_snapshot_canonical_href_not_legacy_slug() {
    let props: opendash_core::builder::ListProps =
        serde_json::from_value(serde_json::json!({"type_key": "post"})).unwrap();
    let doc = BuilderDocument {
        schema_version: 1,
        sections: vec![SectionNode {
            id: "sec-1".into(),
            meta: None,
            responsive: None,
            style: None,
            children: vec![RowNode {
                id: "row-1".into(),
                meta: None,
                responsive: None,
                children: vec![ColumnNode {
                    id: "col-1".into(),
                    meta: None,
                    responsive: None,
                    children: vec![ElementNode {
                        id: "list-1".into(),
                        meta: None,
                        content: ElementContent::Known(KnownElement::List(props)),
                    }],
                }],
            }],
        }],
    };
    let state = BuilderRenderContext::default();
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
                },
                crate::storage::builder::ListItem {
                    entry_id: 18,
                    title: "Unlinked".into(),
                    slug: "hidden".into(),
                    href: None,
                },
            ],
            has_next: false,
            out_of_range: false,
        },
    );
    let rendered =
        render_builder_with_lists(&state, &doc, false, &BTreeMap::new(), &lists).unwrap();
    assert!(rendered.html.contains("/blog/new-slug"));
    assert!(!rendered.html.contains("/old-slug"));
    assert!(!rendered.html.contains("href=\"/hidden\""));
    assert!(!rendered.html.contains("href=\"/content/post/hidden\""));
    assert!(rendered.html.contains("&lt;unsafe&gt;"));
    assert!(!rendered.html.contains("<unsafe>"));
}

#[test]
fn preview_annotation_scope_isolates_public_pattern_parts_and_slots() {
    use opendash_core::builder::{
        ColumnNode, ContentSlotProps, ElementNode, HeadingProps, PatternRefProps, RowNode,
        SectionNode, TemplatePartProps,
    };
    let state = BuilderRenderContext::default();
    let pattern_doc = BuilderDocument {
        schema_version: 1,
        sections: vec![SectionNode {
            id: "psec-1".into(),
            meta: None,
            responsive: None,
            style: None,
            children: vec![RowNode {
                id: "prow-1".into(),
                meta: None,
                responsive: None,
                children: vec![ColumnNode {
                    id: "pcol-1".into(),
                    meta: None,
                    responsive: None,
                    children: vec![ElementNode {
                        id: "phead-1".into(),
                        meta: None,
                        content: ElementContent::Known(KnownElement::Heading(HeadingProps {
                            level: 3,
                            text: "Inner pattern".into(),
                            binding: None,
                        })),
                    }],
                }],
            }],
        }],
    };
    // Pattern AST is always rendered with pattern_document = true and scope None.
    let inner_pattern = render_builder_with_annotations(
        &state,
        &pattern_doc,
        true,
        &BTreeMap::new(),
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

    let outer_doc = BuilderDocument {
        schema_version: 1,
        sections: vec![SectionNode {
            id: "sec-1".into(),
            meta: None,
            responsive: None,
            style: None,
            children: vec![RowNode {
                id: "row-1".into(),
                meta: None,
                responsive: None,
                children: vec![ColumnNode {
                    id: "col-1".into(),
                    meta: None,
                    responsive: None,
                    children: vec![
                        ElementNode {
                            id: "part-1".into(),
                            meta: None,
                            content: ElementContent::Known(KnownElement::TemplatePart(
                                TemplatePartProps {
                                    part_key: "header".into(),
                                },
                            )),
                        },
                        ElementNode {
                            id: "pat-1".into(),
                            meta: None,
                            content: ElementContent::Known(KnownElement::PatternRef(
                                PatternRefProps { pattern_id: 7 },
                            )),
                        },
                        ElementNode {
                            id: "slot-1".into(),
                            meta: None,
                            content: ElementContent::Known(KnownElement::ContentSlot(
                                ContentSlotProps {},
                            )),
                        },
                    ],
                }],
            }],
        }],
    };
    let mut patterns = PatternResults::new();
    patterns.insert(7, inner_pattern);
    let mut parts = BTreeMap::new();
    parts.insert(
        "header".into(),
        "<section data-node-id=\"hdr-sec\"><p>Header</p></section>".into(),
    );
    let ctx = BuilderTemplateContext {
        content_html: Some("<main><p data-node-id=\"entry-inner\">Entry</p></main>"),
        parts: &parts,
    };

    // Public render (is_preview = false) and Scope::None emit zero data-od-editable attributes.
    let public_out = render_builder_with_annotations(
        &state,
        &outer_doc,
        false,
        &BTreeMap::new(),
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
        &BTreeMap::new(),
        Some(&ctx),
        &patterns,
        false,
        RenderAnnotationScope::TemplateAll,
    )
    .unwrap();
    assert!(scoped.html.contains(
        "<section data-node-id=\"sec-1\" data-od-editable=\"1\" data-od-kind=\"section\">"
    ));
    assert!(scoped.html.contains(
        "<div class=\"builder-row\" data-node-id=\"row-1\" data-od-editable=\"1\" data-od-kind=\"row\">"
    ));
    assert!(scoped.html.contains(
        "<div class=\"builder-column\" data-node-id=\"col-1\" data-od-editable=\"1\" data-od-kind=\"column\">"
    ));
    assert!(scoped.html.contains(
        "<div class=\"builder-element\" data-node-id=\"pat-1\" data-od-editable=\"1\" data-od-kind=\"element\">"
    ));
    assert!(scoped.html.contains(
        "<section data-node-id=\"psec-1\"><div class=\"builder-row\" data-node-id=\"prow-1\">"
    ));
    assert!(scoped.html.contains("<section data-node-id=\"hdr-sec\">"));
    assert!(scoped.html.contains("<p data-node-id=\"entry-inner\">"));
}
