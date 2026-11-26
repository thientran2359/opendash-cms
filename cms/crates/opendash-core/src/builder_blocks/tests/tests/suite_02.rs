use super::*;

#[test]
fn schema_aware_refs_and_visible_text_gate_on_active_declared_blocks() {
    use crate::{
        block_schema::BlockSchemaV1,
        registry::{OwnedDescriptor, ResourceOwner},
    };

    let img_hex = "a".repeat(64);
    let vid_hex = "b".repeat(64);
    let any_hex = "c".repeat(64);
    let doc_hex = "d".repeat(64);
    let gal_hex = "e".repeat(64);
    let schema: BlockSchemaV1 = serde_json::from_value(json!({
        "schema_version": 1,
        "id": "core/showcase",
        "title": "Showcase",
        "category": "media",
        "render": "render.html",
        "fields": [
            { "type": "text", "key": "title", "label": "Title" },
            { "type": "richtext", "key": "summary", "label": "Summary" },
            { "type": "media", "key": "poster", "label": "Poster", "media_kind": "image" },
            { "type": "media", "key": "clip", "label": "Clip", "media_kind": "video" },
            { "type": "select", "key": "pat", "label": "Pattern", "options_source": "patterns" },
            {
                "type": "group",
                "key": "bundle",
                "label": "Bundle",
                "fields": [
                    { "type": "media", "key": "attachment", "label": "File", "media_kind": "file" },
                    { "type": "gallery", "key": "thumbs", "label": "Thumbs", "media_kind": "image" }
                ]
            },
            {
                "type": "repeater",
                "key": "slides",
                "label": "Slides",
                "fields": [
                    { "type": "text", "key": "caption", "label": "Caption" },
                    { "type": "media", "key": "asset", "label": "Asset", "media_kind": "any" }
                ]
            }
        ]
    }))
    .unwrap();

    let doc = BuilderDocument {
        schema_version: BUILDER_SCHEMA_VERSION,
        sections: vec![SectionNode {
            id: "s1".into(),
            meta: None,
            responsive: None,
            style: None,
            children: vec![RowNode {
                id: "r1".into(),
                meta: None,
                responsive: None,
                children: vec![ColumnNode {
                    id: "c1".into(),
                    meta: None,
                    responsive: None,
                    children: vec![el(
                        "p1",
                        KnownElement::PluginBlock(PluginBlockProps {
                            block_id: "core/showcase".into(),
                            values: BTreeMap::from([
                                ("title".into(), json!("Tiêu đề block")),
                                ("summary".into(), json!("<p>Mô tả <b>hay</b></p>")),
                                ("poster".into(), json!(img_hex)),
                                ("clip".into(), json!(vid_hex)),
                                ("pat".into(), json!("42")),
                                (
                                    "bundle".into(),
                                    json!({ "attachment": doc_hex, "thumbs": [gal_hex] }),
                                ),
                                (
                                    "slides".into(),
                                    json!([{ "caption": "Slide 1", "asset": any_hex }]),
                                ),
                            ]),
                        }),
                    )],
                }],
            }],
        }],
    };
    let body = serialize_builder_document_to_blocks(&doc).unwrap();

    let mut active_catalog = RegistryCatalog::new();
    active_catalog
        .register(
            "core/showcase",
            ResourceOwner::Core,
            RegisteredBlockSchema {
                schema: schema.clone(),
            },
        )
        .unwrap();

    let refs = extract_block_refs_lenient_with_schemas(&body, &active_catalog);
    assert_eq!(refs.image_ids, BTreeSet::from([img_hex, gal_hex]));
    assert_eq!(refs.video_ids, BTreeSet::from([vid_hex]));
    assert_eq!(refs.file_ids, BTreeSet::from([doc_hex]));
    assert_eq!(refs.any_media_ids, BTreeSet::from([any_hex]));
    assert_eq!(refs.pattern_ids, BTreeSet::from([42]));
    assert_eq!(
        extract_visible_text_with_schemas(&body, &active_catalog),
        "Tiêu đề block\n\nMô tả hay\n\nSlide 1"
    );

    let mut inactive_catalog = RegistryCatalog::new();
    inactive_catalog.insert_loaded(
        "core/showcase",
        OwnedDescriptor {
            owner: ResourceOwner::Core,
            status: ProviderStatus::ProviderInactive,
            declaration_state: DeclarationState::Declared,
            descriptor: RegisteredBlockSchema {
                schema: schema.clone(),
            },
        },
    );
    assert_eq!(
        extract_block_refs_lenient_with_schemas(&body, &inactive_catalog),
        ExtractedBlockRefs::default()
    );
    assert_eq!(
        extract_visible_text_with_schemas(&body, &inactive_catalog),
        ""
    );

    let mut withdrawn_catalog = RegistryCatalog::new();
    withdrawn_catalog.insert_loaded(
        "core/showcase",
        OwnedDescriptor {
            owner: ResourceOwner::Core,
            status: ProviderStatus::Active,
            declaration_state: DeclarationState::Withdrawn,
            descriptor: RegisteredBlockSchema { schema },
        },
    );
    assert_eq!(
        extract_block_refs_lenient_with_schemas(&body, &withdrawn_catalog),
        ExtractedBlockRefs::default()
    );
}

#[test]
fn node_meta_and_synthetic_wrappers_roundtrip() {
    use crate::builder::{NodeMeta, NodeVisibility, SectionStyle, SectionWidth};

    let bg_media = "e".repeat(64);
    let doc = BuilderDocument {
        schema_version: BUILDER_SCHEMA_VERSION,
        sections: vec![
            SectionNode {
                id: "sec-syn-1".into(),
                meta: Some(NodeMeta {
                    synthetic: true,
                    ..Default::default()
                }),
                responsive: None,
                style: None,
                children: vec![RowNode {
                    id: "row-syn-1".into(),
                    meta: Some(NodeMeta {
                        synthetic: true,
                        ..Default::default()
                    }),
                    responsive: None,
                    children: vec![ColumnNode {
                        id: "col-syn-1".into(),
                        meta: Some(NodeMeta {
                            synthetic: true,
                            ..Default::default()
                        }),
                        responsive: None,
                        children: vec![ElementNode {
                            id: "el-head-1".into(),
                            meta: Some(NodeMeta {
                                name: Some("Hero Heading".into()),
                                html_id: Some("hero-title".into()),
                                class_name: Some("text-4xl font-bold".into()),
                                visibility: Some(NodeVisibility {
                                    hide_desktop: false,
                                    hide_tablet: false,
                                    hide_mobile: true,
                                }),
                                synthetic: false,
                            }),
                            content: ElementContent::Known(KnownElement::Heading(HeadingProps {
                                level: 1,
                                text: "Welcome".into(),
                                binding: None,
                            })),
                        }],
                    }],
                }],
            },
            SectionNode {
                id: "sec-styled-1".into(),
                meta: None,
                responsive: None,
                style: Some(SectionStyle {
                    background_color: Some("#0f172a".into()),
                    background_media_id: Some(bg_media.clone()),
                    content_width: Some(SectionWidth::Wide),
                }),
                children: vec![RowNode {
                    id: "row-styled-1".into(),
                    meta: None,
                    responsive: None,
                    children: vec![ColumnNode {
                        id: "col-styled-1".into(),
                        meta: None,
                        responsive: None,
                        children: vec![
                            el(
                                "el-rich-1",
                                KnownElement::Text(TextProps {
                                    text: "<p><strong>Rich</strong> copy</p>".into(),
                                    rich: true,
                                    binding: None,
                                }),
                            ),
                            el(
                                "el-btn-1",
                                KnownElement::Button(ButtonProps {
                                    label: "CTA".into(),
                                    url: "https://opendash.vn".into(),
                                    new_tab: true,
                                    nofollow: true,
                                    binding: None,
                                }),
                            ),
                        ],
                    }],
                }],
            },
        ],
    };

    let catalog = RegistryCatalog::new();
    doc.validate_for_save_with_schemas(&catalog, None)
        .expect("synthetic carrier + styled section + rich text must pass validation");
    let serialized = serialize_builder_document_to_blocks(&doc).unwrap();
    let reparsed = parse_blocks_to_builder_document(&serialized).unwrap();
    assert_eq!(reparsed.document, doc);
    let refs = extract_block_refs_lenient(&serialized);
    assert!(refs.image_ids.contains(&bg_media));
}
