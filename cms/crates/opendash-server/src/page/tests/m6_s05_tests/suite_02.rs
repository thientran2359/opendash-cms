use super::*;

#[test]
fn renders_all_five_core_blocks_with_scoped_css_responsive_breakpoints_and_budget_isolation() {
    let media_id = "d".repeat(64);
    let state = BuilderRenderContext {
        declared_blocks: crate::storage::page::embedded_core_block_defs().clone(),
        hydration_media: std::collections::BTreeMap::from([(
            media_id.clone(),
            crate::storage::fields::HydratedMediaRow {
                id: media_id.clone(),
                kind: "image".into(),
                mime_type: "image/jpeg".into(),
                version: 1,
                width: Some(800),
                height: Some(600),
                variants: Vec::new(),
            },
        )]),
        ..Default::default()
    };
    let doc = BuilderDocument {
        schema_version: 1,
        sections: vec![SectionNode {
            id: "sec-core".into(),
            meta: None,
            responsive: None,
            style: None,
            children: vec![RowNode {
                id: "row-core".into(),
                meta: None,
                responsive: None,
                children: vec![ColumnNode {
                    id: "col-core".into(),
                    meta: None,
                    responsive: None,
                    children: vec![
                        ElementNode {
                            id: "sp-1".into(),
                            meta: None,
                            content: ElementContent::Known(KnownElement::PluginBlock(
                                PluginBlockProps {
                                    block_id: "core/spacer".into(),
                                    values: BTreeMap::from([(
                                        "height".into(),
                                        json!({ "desktop": 64, "tablet": 40, "mobile": 24 }),
                                    )]),
                                },
                            )),
                        },
                        ElementNode {
                            id: "div-1".into(),
                            meta: None,
                            content: ElementContent::Known(KnownElement::PluginBlock(
                                PluginBlockProps {
                                    block_id: "core/divider".into(),
                                    values: BTreeMap::from([
                                        ("line_style".into(), json!("dashed")),
                                        ("thickness".into(), json!(2)),
                                        (
                                            "width_pct".into(),
                                            json!({ "desktop": 80, "mobile": 100 }),
                                        ),
                                        ("color".into(), json!("var(--od-token-border)")),
                                    ]),
                                },
                            )),
                        },
                        ElementNode {
                            id: "ban-1".into(),
                            meta: None,
                            content: ElementContent::Known(KnownElement::PluginBlock(
                                PluginBlockProps {
                                    block_id: "core/banner".into(),
                                    values: BTreeMap::from([
                                        ("bg_media".into(), json!(media_id)),
                                        ("overlay_color".into(), json!("#00000080")),
                                        (
                                            "min_height".into(),
                                            json!({ "desktop": 480, "tablet": 360 }),
                                        ),
                                        ("focus_x".into(), json!(30)),
                                        ("focus_y".into(), json!(70)),
                                        (
                                            "align".into(),
                                            json!({ "desktop": "left", "mobile": "center" }),
                                        ),
                                        ("heading".into(), json!("Khuyến mãi Mùa Thu")),
                                        (
                                            "body".into(),
                                            json!(
                                                "<p>Giảm <strong>50%</strong><script>evil()</script></p>"
                                            ),
                                        ),
                                        ("show_cta".into(), json!(true)),
                                        ("cta_label".into(), json!("Mua ngay")),
                                        ("cta_url".into(), json!("/cua-hang")),
                                    ]),
                                },
                            )),
                        },
                        ElementNode {
                            id: "card-1".into(),
                            meta: None,
                            content: ElementContent::Known(KnownElement::PluginBlock(
                                PluginBlockProps {
                                    block_id: "core/card".into(),
                                    values: BTreeMap::from([
                                        ("layout".into(), json!("horizontal")),
                                        ("image".into(), json!(media_id)),
                                        ("title".into(), json!("Thẻ VIP")),
                                        (
                                            "body".into(),
                                            json!("<p>Quyền lợi <em>đặc biệt</em></p>"),
                                        ),
                                        ("bg_color".into(), json!("#f8fafc")),
                                        ("show_cta".into(), json!(true)),
                                        ("cta_label".into(), json!("Đăng ký")),
                                        ("cta_url".into(), json!("#vip")),
                                    ]),
                                },
                            )),
                        },
                        ElementNode {
                            id: "acc-1".into(),
                            meta: None,
                            content: ElementContent::Known(KnownElement::PluginBlock(
                                PluginBlockProps {
                                    block_id: "core/accordion".into(),
                                    values: BTreeMap::from([
                                        ("style".into(), json!("bordered")),
                                        ("open_first".into(), json!(true)),
                                        (
                                            "items".into(),
                                            json!([
                                                { "title": "Câu hỏi 1", "content": "<p>Trả lời <b>1</b></p>", "open": false },
                                                { "title": "Câu hỏi 2", "content": "<p>Trả lời <i>2</i></p>", "open": false }
                                            ]),
                                        ),
                                    ]),
                                },
                            )),
                        },
                    ],
                }],
            }],
        }],
    };

    state.validate(&doc).expect("all 5 core blocks validate");

    let mut budget = RenderBudget::new();
    let cache = BlockTemplateCache::new();
    let public_rendered = render_builder_with_budget(
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
        &mut budget,
        &cache,
    )
    .unwrap();

    assert!(public_rendered.diagnostics.is_empty());
    assert!(
        public_rendered
            .html
            .contains("<style data-od-block-styles>")
    );
    assert!(
        public_rendered
            .html
            .contains("[data-od-node-scope=\"entry-sp-1\"]{--od-spacer-h:64px;}")
    );
    assert!(public_rendered.html.contains(
        "@media (width < 1024px){[data-od-node-scope=\"entry-sp-1\"]{--od-spacer-h:40px;}"
    ));
    assert!(public_rendered.html.contains(
        "@media (width < 768px){[data-od-node-scope=\"entry-sp-1\"]{--od-spacer-h:24px;}"
    ));
    assert!(public_rendered.html.contains(&format!(
        "--od-banner-bg:url(\"/media/{media_id}/file?v=1\");"
    )));
    assert!(
        public_rendered
            .html
            .contains("--od-banner-fx:30%;--od-banner-fy:70%;")
    );
    assert!(
        public_rendered
            .html
            .contains("<p>Giảm <strong>50%</strong></p>")
    );
    assert!(!public_rendered.html.contains("<script>"));
    assert!(
        public_rendered
            .html
            .contains("<details class=\"od-accordion-item\" open>")
    );
    assert!(
        public_rendered
            .html
            .contains("<details class=\"od-accordion-item\">")
    );

    // Preview uses /api/admin/media/{id}/file in both CSS and <img>
    let mut preview_budget = RenderBudget::new();
    let preview_rendered = render_builder_with_budget(
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
        "pat_42",
        &mut preview_budget,
        &cache,
    )
    .unwrap();
    assert!(
        preview_rendered
            .html
            .contains("[data-od-node-scope=\"pat_42-ban-1\"]")
    );
    assert!(preview_rendered.html.contains(&format!(
        "--od-banner-bg:url(\"/api/admin/media/{media_id}/file?v=1\");"
    )));
    let finalized_preview = crate::content_render::finalize_document_images(
        preview_rendered.html,
        &preview_budget.image_nonces,
    )
    .unwrap();
    let decoded_preview = opendash_core::richtext::decode_html5_entities(&finalized_preview);
    assert!(decoded_preview.contains(&format!("src=\"/api/admin/media/{media_id}/file?v=1\"")));

    // Exhausted fuel isolates block with plugin_block_render_error instead of failing request
    let mut zero_fuel_budget = RenderBudget {
        fuel_remaining: 0,
        html_bytes_used: 0,
        ..RenderBudget::default()
    };
    let isolated = render_builder_with_budget(
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
        &mut zero_fuel_budget,
        &cache,
    )
    .expect("exhausted fuel must isolate plugin blocks without HTTP 500");
    assert_eq!(isolated.diagnostics.len(), 5);
    assert!(
        isolated
            .diagnostics
            .iter()
            .all(|d| d.code == "plugin_block_render_error")
    );
}

#[test]
fn renders_node_meta_attributes_visibility_and_omits_synthetic_wrappers() {
    use opendash_core::builder::{
        ColumnNode, ElementContent, ElementNode, HeadingProps, KnownElement, NodeMeta,
        NodeVisibility, RowNode, SectionNode,
    };

    let state = BuilderRenderContext::default();
    let cache = BlockTemplateCache::new();
    let doc = BuilderDocument {
        schema_version: 1,
        sections: vec![SectionNode {
            id: "sec-syn".into(),
            meta: Some(NodeMeta {
                synthetic: true,
                ..Default::default()
            }),
            responsive: None,
            style: None,
            children: vec![RowNode {
                id: "row-syn".into(),
                meta: Some(NodeMeta {
                    synthetic: true,
                    ..Default::default()
                }),
                responsive: None,
                children: vec![ColumnNode {
                    id: "col-syn".into(),
                    meta: Some(NodeMeta {
                        synthetic: true,
                        ..Default::default()
                    }),
                    responsive: None,
                    children: vec![ElementNode {
                        id: "el-hero".into(),
                        meta: Some(NodeMeta {
                            name: Some("Tiêu đề Hero".into()),
                            html_id: Some("hero-anchor".into()),
                            class_name: Some("py-8 text-center".into()),
                            visibility: Some(NodeVisibility {
                                hide_desktop: false,
                                hide_tablet: true,
                                hide_mobile: false,
                            }),
                            synthetic: false,
                        }),
                        content: ElementContent::Known(KnownElement::Heading(HeadingProps {
                            level: 2,
                            text: "Hello UX Builder".into(),
                            binding: None,
                        })),
                    }],
                }],
            }],
        }],
    };

    let mut budget = RenderBudget::new();
    let rendered = render_builder_with_budget(
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
        &mut budget,
        &cache,
    )
    .unwrap();

    assert!(!rendered.html.contains("<section"));
    assert!(!rendered.html.contains("builder-row"));
    assert!(!rendered.html.contains("builder-column"));
    assert!(rendered.html.contains("id=\"hero-anchor\""));
    assert!(
        rendered
            .html
            .contains("class=\"builder-element py-8 text-center\"")
    );
    assert!(
        rendered
            .html
            .contains("@media (width >= 768px) and (width < 1024px)")
    );
    assert!(
        rendered
            .html
            .contains("[data-node-id=\"el-hero\"]{display:none !important;}")
    );
}
