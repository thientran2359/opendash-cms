use super::*;

#[test]
fn scoped_css_emits_negative_margin_fallback_and_nested_group_paths_and_contact_csp_flag() {
    use opendash_core::{
        block_schema::BlockSchemaV1,
        builder::{
            ColumnNode, ElementContent, ElementNode, KnownElement, PluginBlockProps, RowNode,
            SectionNode,
        },
        registry::{DeclarationState, ProviderStatus, ResourceOwner},
    };

    let schema: BlockSchemaV1 = serde_json::from_value(json!({
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

    let doc = BuilderDocument {
        schema_version: 1,
        sections: vec![SectionNode {
            id: "sec-gmf".into(),
            meta: None,
            responsive: None,
            style: None,
            children: vec![RowNode {
                id: "row-gmf".into(),
                meta: None,
                responsive: None,
                children: vec![ColumnNode {
                    id: "col-gmf".into(),
                    meta: None,
                    responsive: None,
                    children: vec![ElementNode {
                        id: "el-gmf".into(),
                        meta: None,
                        content: ElementContent::Known(KnownElement::PluginBlock(
                            PluginBlockProps {
                                block_id: "demo/group-margin-form".into(),
                                values: std::collections::BTreeMap::from([
                                    (
                                        "layout".into(),
                                        json!({
                                            "outer_margin": {
                                                "desktop": { "top": -1.5, "right": 0, "bottom": 2, "left": 0 },
                                                "mobile": { "top": -1, "right": 0, "bottom": 1, "left": 0 }
                                            }
                                        }),
                                    ),
                                    ("form_id".into(), json!("7")),
                                    ("menu_key".into(), json!("main")),
                                ]),
                            },
                        )),
                    }],
                }],
            }],
        }],
    };

    let cache = BlockTemplateCache::new();
    let mut pub_budget = RenderBudget::new();
    let pub_rendered = render_builder_with_budget(
        &state,
        &doc,
        false,
        &BTreeMap::new(),
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
        "[data-od-node-scope=\"entry-el-gmf\"] [data-od-spacing=\"layout.outer_margin\"]{margin:-1.5rem 0rem 2rem 0rem;}"
    ));
    assert!(pub_rendered.html.contains(
        "@media (width < 768px){[data-od-node-scope=\"entry-el-gmf\"] [data-od-spacing=\"layout.outer_margin\"]{margin:-1rem 0rem 1rem 0rem;}}"
    ));

    let mut prev_budget = RenderBudget::new();
    let prev_rendered = render_builder_with_budget(
        &state,
        &doc,
        true,
        &BTreeMap::new(),
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
