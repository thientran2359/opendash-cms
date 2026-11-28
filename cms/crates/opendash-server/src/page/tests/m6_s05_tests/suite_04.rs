use super::*;

#[test]
fn scoped_css_emits_negative_margin_fallback_and_nested_group_paths_and_contact_csp_flag() {
    use opendash_core::{
        block_schema::BlockSchema,
        registry::{DeclarationState, ProviderStatus, ResourceOwner},
    };

    let schema: BlockSchema = serde_json::from_value(json!({
        "schema_version": 1,
        "id": "demo/group-margin-form",
        "title": "Group Margin Form",
        "category": "content",
        "render": "render.html",
        "fields": [
            {
                "type": "group",
                "key": "layout",
                "label": "Layout",
                "fields": [
                    {
                        "type": "spacing",
                        "key": "outer_margin",
                        "label": "Margin",
                        "property": "margin",
                        "unit": "rem",
                        "responsive": true
                    }
                ]
            },
            {
                "type": "select",
                "key": "form_id",
                "label": "Contact Form",
                "options_source": "contact_forms",
                "allow_null": true
            },
            {
                "type": "select",
                "key": "menu_key",
                "label": "Menu",
                "options_source": "menus",
                "allow_null": true
            }
        ]
    }))
    .unwrap();

    let template = r#"<section class="demo" data-od-spacing="layout.outer_margin">{% if menu_key and menus[menu_key] is defined %}<nav>{% for item in menus[menu_key].items %}<a href="{{ item.url }}">{{ item.label }}</a>{% endfor %}</nav>{% endif %}</section>"#;
    crate::template_sec::validate_block_template_csec("demo/group-margin-form", template, &schema)
        .unwrap();

    let mut declared = crate::storage::page::embedded_core_block_defs().clone();
    declared.insert(
        "demo/group-margin-form".into(),
        crate::storage::page::RegisteredBlockDef {
            block_id: "demo/group-margin-form".into(),
            owner: ResourceOwner::Theme("demo".into()),
            status: ProviderStatus::Active,
            declaration_state: DeclarationState::Declared,
            schema: opendash_core::block_schema::RegisteredBlockSchema { schema },
            template: template.to_owned(),
            revision_digest: "rev-gmf".into(),
        },
    );
    let state = BuilderRenderContext {
        declared_blocks: declared,
        menus: std::collections::BTreeMap::from([(
            "main".into(),
            json!({
                "menu_id": 1,
                "menu_key": "main",
                "title": "Main",
                "items": [{ "id": 1, "label": "Liên hệ", "url": "/lien-he", "is_active": false, "is_ancestor": false, "is_external": false, "depth": 0, "children": [] }],
                "diagnostics": []
            }),
        )]),
        ..Default::default()
    };

    let doc = doc_in_section(
        "sec-gmf",
        "row-gmf",
        "col-gmf",
        vec![node(
            "el-gmf",
            "demo/group-margin-form",
            json!({
                "layout": {
                    "outer_margin": {
                        "desktop": { "top": -1.5, "right": 0, "bottom": 2, "left": 0 },
                        "mobile": { "top": -1, "right": 0, "bottom": 1, "left": 0 }
                    }
                },
                "form_id": "7",
                "menu_key": "main"
            }),
            vec![],
        )],
    );

    let cache = BlockTemplateCache::new();
    let mut pub_budget = RenderBudget::new();
    let pub_rendered = render_builder_with_budget(
        &state,
        &doc,
        false,
        &BTreeMap::new(),
        &BTreeMap::new(),
        None,
        &BTreeMap::new(),
        false,
        RenderAnnotationScope::None,
        "entry",
        &mut pub_budget,
        &cache,
    )
    .unwrap();
    assert!(pub_rendered.has_active_contact_form);
    assert!(pub_rendered.html.contains("Liên hệ"));
    assert!(pub_rendered.html.contains(
        "[data-od-node-scope=\"entry-el-gmf\"][data-od-spacing=\"layout.outer_margin\"],[data-od-node-scope=\"entry-el-gmf\"] [data-od-spacing=\"layout.outer_margin\"]{margin:-1.5rem 0rem 2rem 0rem;}"
    ));
    assert!(pub_rendered.html.contains(
        "@media (width < 768px){[data-od-node-scope=\"entry-el-gmf\"][data-od-spacing=\"layout.outer_margin\"],[data-od-node-scope=\"entry-el-gmf\"] [data-od-spacing=\"layout.outer_margin\"]{margin:-1rem 0rem 1rem 0rem;}}"
    ));

    let mut prev_budget = RenderBudget::new();
    let prev_rendered = render_builder_with_budget(
        &state,
        &doc,
        true,
        &BTreeMap::new(),
        &BTreeMap::new(),
        None,
        &BTreeMap::new(),
        false,
        RenderAnnotationScope::EntryBodyOnly,
        "entry",
        &mut prev_budget,
        &cache,
    )
    .unwrap();
    assert!(!prev_rendered.has_active_contact_form);
}

#[test]
fn slot_table_and_single_root_wrapper_behave_linearly_and_safely() {
    use crate::template_sec::generate_children_nonce;

    let extra = vec!["px-4".to_owned(), "py-2".to_owned()];
    let attrs = RootAttrs {
        node_id: "el-1",
        block_id: "core/hero",
        node_scope: "s1",
        html_id: Some("hero-id"),
        extra_classes: &extra,
        kind: Some("block"),
        is_loop_clone: false,
    };

    let merged = apply_single_root_wrapper(
        "<section class=\"hero\" id=\"old-id\" data-node-id=\"forged\"><p>Hi</p></section>",
        &attrs,
        true,
    );
    assert!(merged.starts_with("<section "));
    assert_eq!(merged.matches("class=").count(), 1);
    assert_eq!(merged.matches(" id=").count(), 1);
    assert_eq!(merged.matches("data-node-id=").count(), 1);
    assert!(merged.contains("class=\"hero px-4 py-2\""));
    assert!(merged.contains("id=\"hero-id\""));
    assert!(merged.contains("data-node-id=\"el-1\""));
    assert!(merged.contains("data-block-id=\"core/hero\""));
    assert!(merged.contains("data-od-node-scope=\"s1\""));
    assert!(merged.contains("data-od-editable=\"1\""));
    assert!(merged.contains("data-od-kind=\"block\""));

    let multi = apply_single_root_wrapper("<p>A</p><p>B</p>", &attrs, false);
    assert!(multi.starts_with("<div id=\"hero-id\" class=\"px-4 py-2\""));
    assert!(!multi.contains("data-od-editable"));
    assert!(multi.ends_with("<p>A</p><p>B</p></div>"));

    let void_wrapped = apply_single_root_wrapper("<img src=\"/a.png\" alt=\"a\">", &attrs, false);
    assert!(void_wrapped.starts_with("<div "));
    assert!(void_wrapped.ends_with("<img src=\"/a.png\" alt=\"a\"></div>"));

    assert_eq!(apply_single_root_wrapper("   ", &attrs, false), "");
    assert!(
        apply_single_root_wrapper("", &attrs, true)
            .contains("class=\"od-block-empty-preview px-4 py-2\"")
    );

    let n_outer = generate_children_nonce().unwrap();
    let n_inner = generate_children_nonce().unwrap();
    assert!(scan_child_nonce_in_shell(&format!("<section>{n_outer}</section>"), &n_outer).unwrap());
    assert!(!scan_child_nonce_in_shell("<section>static</section>", &n_outer).unwrap());
    assert!(
        scan_child_nonce_in_shell(&format!("<section>{n_outer}{n_outer}</section>"), &n_outer)
            .is_err()
    );
    assert!(
        scan_child_nonce_in_shell(&format!("<div data-x=\"{n_outer}\"></div>"), &n_outer).is_err()
    );

    let mut table = HtmlSlotTable::new();
    table.insert(
        n_outer.clone(),
        format!("<div class=\"inner\">{n_inner}</div>"),
    );
    table.insert(n_inner, "<p>Leaf</p>".into());
    let assembled =
        assemble_slot_table(&format!("<section>{n_outer}</section>"), &mut table).unwrap();
    assert_eq!(
        assembled,
        "<section><div class=\"inner\"><p>Leaf</p></div></section>"
    );
    assert!(table.is_empty());
}
