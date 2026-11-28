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
    let doc = doc_in_section(
        "sec-core",
        "row-core",
        "col-core",
        vec![
            node(
                "sp-1",
                "core/spacer",
                json!({
                    "height": { "desktop": 64, "tablet": 40, "mobile": 24 }
                }),
                vec![],
            ),
            node(
                "div-1",
                "core/divider",
                json!({
                    "line_style": "dashed",
                    "thickness": 2,
                    "width_pct": { "desktop": 80, "mobile": 100 },
                    "color": "var(--od-token-border)"
                }),
                vec![],
            ),
            node(
                "ban-1",
                "core/banner",
                json!({
                    "bg_image": media_id,
                    "bg_overlay": "#00000080",
                    "min_height": { "desktop": 480, "tablet": 360 },
                    "bg_focal_x": 30,
                    "bg_focal_y": 70,
                    "h_align": "start"
                }),
                vec![
                    node(
                        "ban-h",
                        "core/heading",
                        json!({ "level": "h2", "text": "Khuyến mãi Mùa Thu" }),
                        vec![],
                    ),
                    node(
                        "ban-txt",
                        "core/text",
                        json!({ "content": "<p>Giảm <strong>50%</strong><script>evil()</script></p>" }),
                        vec![],
                    ),
                    node(
                        "ban-btn",
                        "core/button",
                        json!({ "cta": { "label": "Mua ngay", "url": "/cua-hang", "new_tab": false, "nofollow": false } }),
                        vec![],
                    ),
                ],
            ),
            node(
                "card-1",
                "core/card",
                json!({
                    "media_position": "left",
                    "image": media_id,
                    "bg_color": "#f8fafc"
                }),
                vec![
                    node(
                        "card-h",
                        "core/heading",
                        json!({ "level": "h3", "text": "Thẻ VIP" }),
                        vec![],
                    ),
                    node(
                        "card-txt",
                        "core/text",
                        json!({ "content": "<p>Quyền lợi <em>đặc biệt</em></p>" }),
                        vec![],
                    ),
                    node(
                        "card-btn",
                        "core/button",
                        json!({ "cta": { "label": "Đăng ký", "url": "#vip", "new_tab": false, "nofollow": false } }),
                        vec![],
                    ),
                ],
            ),
            node(
                "acc-1",
                "core/accordion",
                json!({
                    "style": "bordered",
                    "open_first": true
                }),
                vec![
                    node(
                        "acc-item-1",
                        "core/accordion-item",
                        json!({ "title": "Câu hỏi 1", "open_by_default": false }),
                        vec![node(
                            "acc-txt-1",
                            "core/text",
                            json!({ "content": "<p>Trả lời <b>1</b></p>" }),
                            vec![],
                        )],
                    ),
                    node(
                        "acc-item-2",
                        "core/accordion-item",
                        json!({ "title": "Câu hỏi 2", "open_by_default": false }),
                        vec![node(
                            "acc-txt-2",
                            "core/text",
                            json!({ "content": "<p>Trả lời <i>2</i></p>" }),
                            vec![],
                        )],
                    ),
                ],
            ),
        ],
    );

    state.validate(&doc).expect("all 5 core blocks validate");

    let mut budget = RenderBudget::new();
    let cache = BlockTemplateCache::new();
    let public_rendered = render_builder_with_budget(
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
            .contains("<details class=\"od-accordion-item rounded-xl\" name=\"od-acc-acc-1\" open")
    );
    assert!(public_rendered.html.contains("Câu hỏi 1"));
    assert!(public_rendered.html.contains("Trả lời <b>1</b>"));
    assert!(
        public_rendered
            .html
            .contains("<details class=\"od-accordion-item rounded-xl\" name=\"od-acc-acc-1\"")
    );

    // Preview uses /api/admin/media/{id}/file in both CSS and <img>
    let mut preview_budget = RenderBudget::new();
    let preview_rendered = render_builder_with_budget(
        &state,
        &doc,
        true,
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

    // Exhausted fuel isolates C-SEC blocks (banner, card, accordion) while NativeRust spacer/divider still render
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
        None,
        &BTreeMap::new(),
        false,
        RenderAnnotationScope::None,
        "entry",
        &mut zero_fuel_budget,
        &cache,
    )
    .expect("exhausted fuel must isolate C-SEC blocks without HTTP 500");
    assert_eq!(isolated.diagnostics.len(), 3);
    assert!(
        isolated
            .diagnostics
            .iter()
            .all(|d| d.code == "plugin_block_render_error")
    );
}

#[test]
fn renders_node_meta_attributes_visibility_and_omits_synthetic_wrappers() {
    use opendash_core::builder::{NodeMeta, NodeVisibility};

    let state = BuilderRenderContext {
        declared_blocks: crate::storage::page::embedded_core_block_defs().clone(),
        ..Default::default()
    };
    let cache = BlockTemplateCache::new();
    let mut hero = node(
        "el-hero",
        "core/heading",
        json!({ "level": "h2", "text": "Hello UX Builder" }),
        vec![],
    );
    hero.meta = Some(NodeMeta {
        name: Some("Tiêu đề Hero".into()),
        html_id: Some("hero-anchor".into()),
        class_name: Some("py-8 text-center".into()),
        visibility: Some(NodeVisibility {
            hide_desktop: false,
            hide_tablet: true,
            hide_mobile: false,
        }),
    });
    let doc = BuilderDocument {
        schema_version: 1,
        blocks: vec![hero],
    };

    let mut budget = RenderBudget::new();
    let rendered = render_builder_with_budget(
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
        &mut budget,
        &cache,
    )
    .unwrap();

    assert!(!rendered.html.contains("<section"));
    assert!(!rendered.html.contains("builder-row"));
    assert!(!rendered.html.contains("builder-column"));
    assert!(rendered.html.contains("id=\"hero-anchor\""));
    assert!(rendered.html.contains("py-8 text-center"));
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
