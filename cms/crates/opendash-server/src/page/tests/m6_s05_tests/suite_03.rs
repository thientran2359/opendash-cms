use super::*;

#[test]
fn renders_section_style_box_spacing_rich_text_button_flags_and_new_schema_fields() {
    use opendash_core::{
        block_schema::BlockSchemaV1,
        builder::{
            BoxSpacing, ButtonProps, ColumnNode, ElementContent, ElementNode, KnownElement,
            PluginBlockProps, Responsive, ResponsiveStyle, RowNode, SectionNode, SectionStyle,
            SectionWidth, TextAlign, TextProps,
        },
        registry::{DeclarationState, ProviderStatus, ResourceOwner},
    };

    let bg_id = "f".repeat(64);
    let schema: BlockSchemaV1 = serde_json::from_value(json!({
        "schema_version": 1,
        "id": "demo/ux-card",
        "title": "UX Card",
        "category": "content",
        "render": "render.html",
        "fields": [
            {
                "type": "radio_buttons",
                "key": "align",
                "label": "Align",
                "options": [{ "value": "left", "label": "Trái" }, { "value": "center", "label": "Giữa" }],
                "css_var": "--od-card-align",
                "responsive": true
            },
            {
                "type": "spacing",
                "key": "pad",
                "label": "Padding",
                "unit": "px",
                "css_var": "--od-card-pad",
                "responsive": true
            },
            { "type": "link", "key": "cta", "label": "CTA" }
        ]
    }))
    .unwrap();
    let template = "<div class=\"ux-card\">{% if cta %}<a href=\"{{ cta.url }}\" target=\"{{ cta.target }}\" rel=\"{{ cta.rel }}\">{{ cta.label }}</a>{% endif %}</div>".to_string();
    crate::template_sec::validate_block_template_csec("demo/ux-card", &template, &schema)
        .expect("C-SEC template using Link synthetic attrs must pass validation");

    let mut declared = crate::storage::page::embedded_core_block_defs().clone();
    declared.insert(
        "demo/ux-card".into(),
        crate::storage::page::RegisteredBlockDef {
            block_id: "demo/ux-card".into(),
            owner: ResourceOwner::Theme("demo".into()),
            status: ProviderStatus::Active,
            declaration_state: DeclarationState::Declared,
            schema: opendash_core::block_schema::RegisteredBlockSchema {
                schema: schema.clone(),
            },
            template,
            revision_digest: "rev-demo-1".into(),
        },
    );
    let state = BuilderRenderContext {
        declared_blocks: declared,
        ..Default::default()
    };

    let doc = BuilderDocument {
        schema_version: 1,
        sections: vec![
            SectionNode {
                id: "sec-ux".into(),
                meta: None,
                responsive: Some(Responsive {
                    desktop: Some(ResponsiveStyle {
                        padding: Some(BoxSpacing {
                            top: Some(40),
                            right: Some(16),
                            bottom: Some(40),
                            left: Some(16),
                        }),
                        margin: Some(BoxSpacing {
                            top: Some(0),
                            bottom: Some(24),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                style: Some(SectionStyle {
                    background_color: Some("#0f172a".into()),
                    background_media_id: Some(bg_id.clone()),
                    content_width: Some(SectionWidth::Boxed),
                }),
                children: vec![RowNode {
                    id: "row-ux".into(),
                    meta: None,
                    responsive: Some(Responsive {
                        desktop: Some(ResponsiveStyle {
                            columns_per_row: Some(3),
                            gap_px: Some(24),
                            ..Default::default()
                        }),
                        mobile: Some(ResponsiveStyle {
                            columns_per_row: Some(1),
                            gap_px: Some(16),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    children: vec![ColumnNode {
                        id: "col-ux".into(),
                        meta: None,
                        responsive: Some(Responsive {
                            desktop: Some(ResponsiveStyle {
                                text_align: Some(TextAlign::Justify),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        children: vec![
                            ElementNode {
                                id: "el-rich".into(),
                                meta: None,
                                content: ElementContent::Known(KnownElement::Text(TextProps {
                                    text:
                                        "<p>Hello <strong>Rich</strong><script>bad()</script></p>"
                                            .into(),
                                    rich: true,
                                    binding: None,
                                })),
                            },
                            ElementNode {
                                id: "el-btn".into(),
                                meta: None,
                                content: ElementContent::Known(KnownElement::Button(ButtonProps {
                                    label: "Visit".into(),
                                    url: "https://opendash.vn".into(),
                                    new_tab: true,
                                    nofollow: true,
                                    binding: None,
                                })),
                            },
                            ElementNode {
                                id: "el-ux".into(),
                                meta: None,
                                content: ElementContent::Known(KnownElement::PluginBlock(
                                    PluginBlockProps {
                                        block_id: "demo/ux-card".into(),
                                        values: BTreeMap::from([
                                            (
                                                "align".into(),
                                                json!({ "desktop": "center", "mobile": "left" }),
                                            ),
                                            (
                                                "pad".into(),
                                                json!({ "desktop": { "top": 20, "right": 12, "bottom": 20, "left": 12 } }),
                                            ),
                                            (
                                                "cta".into(),
                                                json!({ "label": "Go", "url": "https://opendash.vn/docs", "new_tab": true, "nofollow": false }),
                                            ),
                                        ]),
                                    },
                                )),
                            },
                        ],
                    }],
                }],
            },
            SectionNode {
                id: "sec-custom".into(),
                meta: None,
                responsive: Some(Responsive {
                    desktop: Some(ResponsiveStyle {
                        padding: Some(BoxSpacing {
                            top: Some(37),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                style: None,
                children: vec![RowNode {
                    id: "row-custom".into(),
                    meta: None,
                    responsive: Some(Responsive {
                        desktop: Some(ResponsiveStyle {
                            columns_per_row: Some(2),
                            gap_px: Some(19),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    children: vec![],
                }],
            },
        ],
    };

    let mut budget = RenderBudget::new();
    let cache = BlockTemplateCache::new();
    let preview = render_builder_with_budget(
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
        &mut budget,
        &cache,
    )
    .unwrap();

    assert!(preview.html.contains("background-color:#0f172a;"));
    assert!(preview.html.contains(&format!(
        "background-image:url(\"/api/admin/media/{bg_id}/file\");"
    )));
    assert!(preview.html.contains("class=\"px-4 py-10 mt-0 mb-6\""));
    assert!(
        preview
            .html
            .contains("class=\"builder-row w-full max-w-7xl mx-auto grid grid-cols-1 md:grid-cols-3 gap-4 md:gap-6\"")
    );
    assert!(
        preview
            .html
            .contains("class=\"builder-column text-justify\"")
    );
    assert!(!preview.html.contains("padding-top:40px;"));
    assert!(!preview.html.contains("text-align:justify;"));
    assert!(preview.html.contains("padding-top:37px;"));
    assert!(preview.html.contains("gap:19px;"));
    assert!(
        preview
            .html
            .contains("class=\"builder-row grid grid-cols-2\"")
    );
    assert!(
        preview
            .html
            .contains("<div class=\"od-rich-text\"><p>Hello <strong>Rich</strong></p></div>")
    );
    assert!(!preview.html.contains("<script>"));
    assert!(
        preview
            .html
            .contains("target=\"_blank\" rel=\"noopener noreferrer nofollow\"")
    );
    assert!(preview.html.contains("--od-card-align:center;"));
    assert!(preview.html.contains("--od-card-pad:20px 12px 20px 12px;"));
    assert!(
        preview
            .html
            .contains("target=\"_blank\" rel=\"noopener noreferrer\">Go</a>")
    );
}

#[test]
fn renders_social_follow_block_from_site_options_for_public_and_preview() {
    use opendash_core::builder::{
        ColumnNode, ElementContent, ElementNode, KnownElement, PluginBlockProps, RowNode,
        SectionNode,
    };

    let doc = BuilderDocument {
        schema_version: 1,
        sections: vec![SectionNode {
            id: "sec-sf".into(),
            meta: None,
            responsive: None,
            style: None,
            children: vec![RowNode {
                id: "row-sf".into(),
                meta: None,
                responsive: None,
                children: vec![ColumnNode {
                    id: "col-sf".into(),
                    meta: None,
                    responsive: None,
                    children: vec![ElementNode {
                        id: "sf-1".into(),
                        meta: None,
                        content: ElementContent::Known(KnownElement::PluginBlock(
                            PluginBlockProps {
                                block_id: "core/social-follow".into(),
                                values: BTreeMap::from([
                                    ("heading".into(), json!("Theo dõi")),
                                    ("show_facebook".into(), json!(true)),
                                    ("show_youtube".into(), json!(false)),
                                    ("display_style".into(), json!("pill")),
                                    ("open_new_tab".into(), json!(true)),
                                ]),
                            },
                        )),
                    }],
                }],
            }],
        }],
    };

    let cache = BlockTemplateCache::new();
    let state_with_links = BuilderRenderContext {
        declared_blocks: crate::storage::page::embedded_core_block_defs().clone(),
        site_options: BTreeMap::from([
            (
                "social_facebook".into(),
                json!("https://facebook.com/opendash"),
            ),
            (
                "social_youtube".into(),
                json!("https://youtube.com/@opendash"),
            ),
        ]),
        ..Default::default()
    };
    let mut budget = RenderBudget::new();
    let rendered = render_builder_with_budget(
        &state_with_links,
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
        &mut budget,
        &cache,
    )
    .unwrap();
    assert!(
        rendered.diagnostics.is_empty(),
        "{:?}",
        rendered.diagnostics
    );
    let decoded_html = opendash_core::richtext::decode_html5_entities(&rendered.html);
    assert!(
        decoded_html.contains("href=\"https://facebook.com/opendash\""),
        "{decoded_html}"
    );
    assert!(!decoded_html.contains("youtube.com/@opendash"));
    assert!(decoded_html.contains("target=\"_blank\" rel=\"noopener noreferrer\""));

    // Empty global links: public omits output, preview shows configuration hint.
    let state_empty = BuilderRenderContext {
        declared_blocks: crate::storage::page::embedded_core_block_defs().clone(),
        ..Default::default()
    };
    let mut pub_budget = RenderBudget::new();
    let pub_empty = render_builder_with_budget(
        &state_empty,
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
    assert!(!pub_empty.html.contains("od-social-follow"));

    let mut prev_budget = RenderBudget::new();
    let prev_empty = render_builder_with_budget(
        &state_empty,
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
    assert!(prev_empty.html.contains("od-social-follow-empty"));
    assert!(prev_empty.html.contains("Tùy chỉnh giao diện"));
}
