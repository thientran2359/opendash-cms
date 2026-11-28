use super::*;

#[test]
fn renders_section_style_box_spacing_rich_text_button_flags_and_new_schema_fields() {
    use opendash_core::{
        block_schema::BlockSchema,
        registry::{DeclarationState, ProviderStatus, ResourceOwner},
    };

    let bg_id = "f".repeat(64);
    let schema: BlockSchema = serde_json::from_value(json!({
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
        blocks: vec![
            node(
                "sec-ux",
                "core/section",
                json!({
                    "content_width": "boxed",
                    "bg_color": "#0f172a",
                    "bg_image": bg_id,
                    "padding": {
                        "desktop": { "top": 40, "right": 16, "bottom": 40, "left": 16 }
                    },
                    "margin": {
                        "desktop": { "top": 0, "bottom": 24 }
                    }
                }),
                vec![node(
                    "row-ux",
                    "core/row",
                    json!({
                        "layout_mode": "equal",
                        "columns": { "desktop": 3, "mobile": 1 },
                        "gap": { "desktop": 24, "mobile": 16 }
                    }),
                    vec![node(
                        "col-ux",
                        "core/column",
                        json!({
                            "text_align": { "desktop": "justify" }
                        }),
                        vec![
                            node(
                                "el-rich",
                                "core/text",
                                json!({
                                    "content": "<p>Hello <strong>Rich</strong><script>bad()</script></p>"
                                }),
                                vec![],
                            ),
                            node(
                                "el-btn",
                                "core/button",
                                json!({
                                    "cta": {
                                        "label": "Visit",
                                        "url": "https://opendash.vn",
                                        "new_tab": true,
                                        "nofollow": true
                                    }
                                }),
                                vec![],
                            ),
                            node(
                                "el-ux",
                                "demo/ux-card",
                                json!({
                                    "align": { "desktop": "center", "mobile": "left" },
                                    "pad": { "desktop": { "top": 20, "right": 12, "bottom": 20, "left": 12 } },
                                    "cta": { "label": "Go", "url": "https://opendash.vn/docs", "new_tab": true, "nofollow": false }
                                }),
                                vec![],
                            ),
                        ],
                    )],
                )],
            ),
            node(
                "sec-custom",
                "core/section",
                json!({
                    "padding": { "desktop": { "top": 37 } }
                }),
                vec![node(
                    "row-custom",
                    "core/row",
                    json!({
                        "layout_mode": "equal",
                        "columns": { "desktop": 2, "mobile": 2 },
                        "gap": 19
                    }),
                    vec![],
                )],
            ),
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
        None,
        &BTreeMap::new(),
        false,
        RenderAnnotationScope::EntryBodyOnly,
        "entry",
        &mut budget,
        &cache,
    )
    .unwrap();

    assert!(preview.html.contains("--od-sec-bg:#0f172a;"));
    assert!(preview.html.contains(&format!(
        "--od-sec-bg-img:url(\"/api/admin/media/{bg_id}/file\");"
    )));
    assert!(preview.html.contains("mt-0 mb-6"));
    assert!(preview.html.contains("max-w-7xl mx-auto px-4 py-10"));
    assert!(
        preview
            .html
            .contains("od-row builder-row grid grid-cols-1 md:grid-cols-3 items-stretch justify-start gap-4 md:gap-6")
    );
    assert!(
        preview
            .html
            .contains("od-column builder-column flex flex-col text-justify justify-start")
    );
    assert!(!preview.html.contains("padding-top:40px;"));
    assert!(!preview.html.contains("text-align:justify;"));
    assert!(preview.html.contains("padding:37px 0px 0px 0px;"));
    assert!(preview.html.contains("style=\"gap:19px;\""));
    assert!(preview.html.contains("od-row builder-row grid grid-cols-2"));
    assert!(
        preview
            .html
            .contains("class=\"od-rich-text od-prose text-base\"")
    );
    assert!(preview.html.contains("data-node-id=\"el-rich\""));
    assert!(
        preview
            .html
            .contains("<p>Hello <strong>Rich</strong></p></div>")
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
    let doc = doc_in_section(
        "sec-sf",
        "row-sf",
        "col-sf",
        vec![node(
            "sf-1",
            "core/social-follow",
            json!({
                "heading": "Theo dõi",
                "show_facebook": true,
                "show_youtube": false,
                "display_style": "pill",
                "open_new_tab": true
            }),
            vec![],
        )],
    );

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
