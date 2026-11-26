use super::*;

#[test]
fn validate_for_save_with_schemas_enforces_status_declaration_and_schema_evolution() {
    use crate::{
        block_schema::BlockSchemaV1,
        registry::{OwnedDescriptor, ResourceOwner},
    };

    let media_hex = "a".repeat(64);
    let schema_v1: BlockSchemaV1 = serde_json::from_value(json!({
        "schema_version": 1,
        "id": "core/faq",
        "title": "FAQ",
        "category": "interactive",
        "render": "render.html",
        "fields": [
            { "type": "text", "key": "heading", "label": "Heading", "required": true },
            { "type": "media", "key": "icon_img", "label": "Icon", "media_kind": "image" },
            {
                "type": "repeater",
                "key": "items",
                "label": "Items",
                "fields": [
                    { "type": "text", "key": "question", "label": "Question", "required": true }
                ]
            }
        ]
    }))
    .unwrap();
    schema_v1.validate("core").unwrap();

    let doc_raw = json!({
        "schema_version": 1,
        "sections": [{
            "id": "sec-1",
            "children": [{
                "id": "row-1",
                "children": [
                    {
                        "id": "col-1",
                        "children": [{
                            "id": "faq-1",
                            "type": "plugin/block",
                            "props": {
                                "block_id": "core/faq",
                                "values": {
                                    "heading": "Hỏi đáp",
                                    "icon_img": media_hex,
                                    "items": [{ "question": "Câu 1" }]
                                }
                            }
                        }]
                    },
                    {
                        "id": "col-2",
                        "children": []
                    }
                ]
            }]
        }]
    });
    let doc_v1 = BuilderDocument::from_json(doc_raw.to_string().as_bytes()).unwrap();

    let mut catalog = RegistryCatalog::new();
    catalog
        .register(
            "core/faq",
            ResourceOwner::Core,
            RegisteredBlockSchema {
                schema: schema_v1.clone(),
            },
        )
        .unwrap();
    doc_v1
        .validate_for_save_with_schemas(&catalog, None)
        .unwrap();

    // Reject unsafe URL / color / richtext in active schema
    let mut bad_doc = doc_v1.clone();
    let bad_schema: BlockSchemaV1 = serde_json::from_value(json!({
        "schema_version": 1,
        "id": "core/faq",
        "title": "FAQ",
        "category": "interactive",
        "render": "render.html",
        "fields": [
            { "type": "url", "key": "cta_url", "label": "URL" }
        ]
    }))
    .unwrap();
    catalog
        .redeclare(
            "core/faq",
            ResourceOwner::Core,
            ProviderStatus::Active,
            RegisteredBlockSchema { schema: bad_schema },
        )
        .unwrap();
    if let ElementContent::Known(KnownElement::PluginBlock(ref mut p)) =
        bad_doc.sections[0].children[0].children[0].children[0].content
    {
        p.values
            .insert("cta_url".into(), json!("javascript:alert(1)"));
    }
    assert!(
        bad_doc
            .validate_for_save_with_schemas(&catalog, None)
            .is_err()
    );

    // ProviderInactive & Withdrawn: preserved identically, rejected on edit/move/fresh insert
    let mut inactive_catalog = RegistryCatalog::new();
    inactive_catalog.insert_loaded(
        "core/faq",
        OwnedDescriptor {
            owner: ResourceOwner::Core,
            status: ProviderStatus::ProviderInactive,
            declaration_state: DeclarationState::Declared,
            descriptor: RegisteredBlockSchema {
                schema: schema_v1.clone(),
            },
        },
    );
    doc_v1
        .validate_for_save_with_schemas(&inactive_catalog, Some(&doc_v1))
        .unwrap();
    assert!(
        doc_v1
            .validate_for_save_with_schemas(&inactive_catalog, None)
            .is_err()
    );

    let mut withdrawn_catalog = RegistryCatalog::new();
    withdrawn_catalog.insert_loaded(
        "core/faq",
        OwnedDescriptor {
            owner: ResourceOwner::Core,
            status: ProviderStatus::Active,
            declaration_state: DeclarationState::Withdrawn,
            descriptor: RegisteredBlockSchema { schema: schema_v1 },
        },
    );
    doc_v1
        .validate_for_save_with_schemas(&withdrawn_catalog, Some(&doc_v1))
        .unwrap();
    assert!(
        doc_v1
            .validate_for_save_with_schemas(&withdrawn_catalog, None)
            .is_err()
    );

    // Schema evolution on Active block (new required top-level + repeater field) passes even when moved to col-2
    let schema_v2: BlockSchemaV1 = serde_json::from_value(json!({
        "schema_version": 1,
        "id": "core/faq",
        "title": "FAQ",
        "category": "interactive",
        "render": "render.html",
        "fields": [
            { "type": "text", "key": "heading", "label": "Heading", "required": true },
            { "type": "text", "key": "newly_required_subtitle", "label": "Subtitle", "required": true },
            {
                "type": "repeater",
                "key": "items",
                "label": "Items",
                "fields": [
                    { "type": "text", "key": "question", "label": "Question", "required": true },
                    { "type": "text", "key": "newly_required_answer", "label": "Answer", "required": true }
                ]
            }
        ]
    }))
    .unwrap();
    let mut v2_catalog = RegistryCatalog::new();
    v2_catalog
        .register(
            "core/faq",
            ResourceOwner::Core,
            RegisteredBlockSchema { schema: schema_v2 },
        )
        .unwrap();
    let mut moved_doc = doc_v1.clone();
    let node = moved_doc.sections[0].children[0].children[0]
        .children
        .remove(0);
    moved_doc.sections[0].children[0].children[1]
        .children
        .push(node);
    moved_doc
        .validate_for_save_with_schemas(&v2_catalog, Some(&doc_v1))
        .unwrap();
    assert!(
        moved_doc
            .validate_for_save_with_schemas(&v2_catalog, None)
            .is_err()
    );
}

#[test]
fn section_style_box_spacing_rich_text_and_button_flags_validate() {
    let mut doc = fixture();
    doc.sections[0].style = Some(SectionStyle {
        background_color: Some("#f8fafc".into()),
        background_media_id: Some("b".repeat(64)),
        content_width: Some(SectionWidth::Boxed),
    });
    doc.sections[0].responsive = Some(Responsive {
        desktop: Some(ResponsiveStyle {
            padding: Some(BoxSpacing {
                top: Some(48),
                right: Some(16),
                bottom: Some(48),
                left: Some(16),
            }),
            margin: Some(BoxSpacing {
                top: Some(-12),
                right: None,
                bottom: Some(24),
                left: None,
            }),
            ..Default::default()
        }),
        ..Default::default()
    });
    doc.sections[0].children[0].children[0].responsive = Some(Responsive {
        desktop: Some(ResponsiveStyle {
            text_align: Some(TextAlign::Justify),
            ..Default::default()
        }),
        ..Default::default()
    });
    doc.sections[0].children[0].children[0].children[1].content =
        ElementContent::Known(KnownElement::Text(TextProps {
            text: "<p>Xin chào <strong>OpenDash</strong></p>".into(),
            rich: true,
            binding: None,
        }));
    doc.sections[0].children[0].children[0].children[3].content =
        ElementContent::Known(KnownElement::Button(ButtonProps {
            label: "Đọc thêm".into(),
            url: "https://opendash.vn".into(),
            new_tab: true,
            nofollow: true,
            binding: None,
        }));

    doc.validate_for_save_with_schemas(&registry(), None)
        .unwrap();
    let encoded = doc.to_json().unwrap();
    let decoded = BuilderDocument::from_json(&encoded).unwrap();
    assert_eq!(decoded, doc);

    // Synthetic section cannot carry style
    let mut syn = doc.clone();
    syn.sections[0].meta = Some(NodeMeta {
        synthetic: true,
        ..Default::default()
    });
    assert!(
        syn.validate_for_save_with_schemas(&registry(), None)
            .is_err()
    );

    // Bad section background color fails
    let mut bad_color = doc.clone();
    bad_color.sections[0]
        .style
        .as_mut()
        .unwrap()
        .background_color = Some("expression(alert(1))".into());
    assert!(
        bad_color
            .validate_for_save_with_schemas(&registry(), None)
            .is_err()
    );
}

#[test]
fn compiles_box_spacing_to_minimal_mobile_first_tailwind_classes() {
    let uniform = BoxSpacing {
        top: Some(32),
        right: Some(32),
        bottom: Some(32),
        left: Some(32),
    };
    assert_eq!(
        compile_box_spacing_tailwind("p", Some(uniform), None, None, false),
        Some(vec!["p-8".into()])
    );

    let d_xy = BoxSpacing {
        top: Some(48),
        right: Some(24),
        bottom: Some(48),
        left: Some(24),
    };
    let m_xy = BoxSpacing {
        top: Some(32),
        right: Some(16),
        bottom: Some(32),
        left: Some(16),
    };
    assert_eq!(
        compile_box_spacing_tailwind("p", Some(d_xy), None, Some(m_xy), false),
        Some(vec![
            "px-4".into(),
            "py-8".into(),
            "md:px-6".into(),
            "md:py-12".into(),
        ])
    );

    let mobile_only = BoxSpacing {
        top: Some(16),
        right: Some(16),
        bottom: Some(16),
        left: Some(16),
    };
    assert_eq!(
        compile_box_spacing_tailwind("p", None, None, Some(mobile_only), false),
        Some(vec!["p-4".into(), "md:p-0".into()])
    );

    let neg_margin = BoxSpacing {
        top: Some(-24),
        right: None,
        bottom: None,
        left: None,
    };
    assert_eq!(
        compile_box_spacing_tailwind("m", None, None, Some(neg_margin), true),
        Some(vec!["-mt-6".into(), "md:mt-0".into()])
    );

    let custom_px = BoxSpacing {
        top: Some(37),
        right: None,
        bottom: None,
        left: None,
    };
    assert_eq!(
        compile_box_spacing_tailwind("p", Some(custom_px), None, None, false),
        None
    );
}
