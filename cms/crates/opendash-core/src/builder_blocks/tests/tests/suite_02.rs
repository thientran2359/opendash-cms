use super::*;
use crate::{
    block_schema::BlockSchema,
    registry::{DeclarationState, OwnedDescriptor, ProviderStatus, RegistryCatalog, ResourceOwner},
};

fn build_codec_catalog() -> RegistryCatalog<RegisteredBlockSchema> {
    let mut catalog = RegistryCatalog::new();
    for raw in [
        json!({
            "schema_version": 1,
            "id": "core/section",
            "title": "Section",
            "category": "layout",
            "render": "native",
            "supports_children": true,
            "allowed_children": ["*", "!core/section"],
            "fields": [{ "type": "media", "key": "bg_image", "label": "BG", "media_kind": "image" }]
        }),
        json!({
            "schema_version": 1,
            "id": "core/row",
            "title": "Row",
            "category": "layout",
            "render": "native",
            "supports_children": true,
            "allowed_children": ["core/column"],
            "fields": []
        }),
        json!({
            "schema_version": 1,
            "id": "core/text",
            "title": "Text",
            "category": "content",
            "render": "native",
            "fields": [{ "type": "richtext", "key": "content", "label": "Content" }]
        }),
        json!({
            "schema_version": 1,
            "id": "core/image",
            "title": "Image",
            "category": "media",
            "render": "native",
            "fields": [{ "type": "media", "key": "image", "label": "Image", "media_kind": "image" }]
        }),
        json!({
            "schema_version": 1,
            "id": "core/video",
            "title": "Video",
            "category": "media",
            "render": "native",
            "fields": [
                { "type": "media", "key": "video", "label": "Video", "media_kind": "video" },
                { "type": "media", "key": "poster", "label": "Poster", "media_kind": "image" }
            ]
        }),
        json!({
            "schema_version": 1,
            "id": "core/pattern-ref",
            "title": "Pattern Ref",
            "category": "content",
            "render": "native",
            "fields": [{ "type": "select", "key": "pattern_id", "label": "Pattern", "options_source": "patterns" }]
        }),
        json!({
            "schema_version": 1,
            "id": "acme/box",
            "title": "Box",
            "category": "plugin",
            "render": "render.html",
            "supports_children": true,
            "fields": [{ "type": "media", "key": "cover", "label": "Cover", "media_kind": "image" }]
        }),
    ] {
        let schema: BlockSchema = serde_json::from_value(raw).unwrap();
        let owner = if schema.id.starts_with("core/") {
            ResourceOwner::Core
        } else {
            ResourceOwner::Theme("acme".into())
        };
        catalog
            .register(schema.id.clone(), owner, RegisteredBlockSchema { schema })
            .unwrap();
    }
    catalog
}

#[test]
fn stack_aware_refs_hide_children_inside_inactive_container_and_distinguish_poster() {
    let img_active = "a".repeat(64);
    let img_in_box = "b".repeat(64);
    let box_cover = "c".repeat(64);
    let vid_hex = "d".repeat(64);
    let poster_hex = "e".repeat(64);

    let doc = BuilderDocument {
        schema_version: BUILDER_SCHEMA_VERSION,
        blocks: vec![
            bnode(
                "img-top",
                "core/image",
                json!({ "image": img_active }),
                vec![],
            ),
            bnode(
                "vid-1",
                "core/video",
                json!({ "video": vid_hex, "poster": poster_hex }),
                vec![],
            ),
            bnode(
                "box-1",
                "acme/box",
                json!({ "cover": box_cover }),
                vec![
                    bnode(
                        "img-child",
                        "core/image",
                        json!({ "image": img_in_box }),
                        vec![],
                    ),
                    bnode(
                        "pat-child",
                        "core/pattern-ref",
                        json!({ "pattern_id": "99" }),
                        vec![],
                    ),
                    bnode(
                        "txt-child",
                        "core/text",
                        json!({ "content": "<p>Hidden text</p>" }),
                        vec![],
                    ),
                ],
            ),
        ],
    };
    let body = serialize_builder_document_to_blocks(&doc).unwrap();

    let active_cat = build_codec_catalog();
    let active_refs = extract_block_refs_lenient_with_schemas(&body, &active_cat);
    assert_eq!(
        active_refs.image_ids,
        BTreeSet::from([img_active.clone(), box_cover, img_in_box])
    );
    assert_eq!(active_refs.video_ids, BTreeSet::from([vid_hex.clone()]));
    assert_eq!(
        active_refs.poster_image_ids,
        BTreeSet::from([poster_hex.clone()])
    );
    assert_eq!(active_refs.pattern_ids, BTreeSet::from([99]));
    assert_eq!(
        extract_visible_text_with_schemas(&body, &active_cat),
        "Hidden text"
    );

    let mut inactive_cat = build_codec_catalog();
    inactive_cat.insert_loaded(
        "acme/box",
        OwnedDescriptor {
            owner: ResourceOwner::Theme("acme".into()),
            status: ProviderStatus::ProviderInactive,
            declaration_state: DeclarationState::Declared,
            descriptor: active_cat.get("acme/box").unwrap().descriptor.clone(),
        },
    );
    let inactive_refs = extract_block_refs_lenient_with_schemas(&body, &inactive_cat);
    assert_eq!(inactive_refs.image_ids, BTreeSet::from([img_active]));
    assert_eq!(inactive_refs.video_ids, BTreeSet::from([vid_hex]));
    assert_eq!(inactive_refs.poster_image_ids, BTreeSet::from([poster_hex]));
    assert!(inactive_refs.pattern_ids.is_empty());
    assert_eq!(extract_visible_text_with_schemas(&body, &inactive_cat), "");
}

#[test]
fn parser_with_schemas_hoists_freeform_html_out_of_restricted_containers() {
    let catalog = build_codec_catalog();
    let raw = r#"<!-- od:core/section {"id":"sec-1"} -->
<!-- od:core/row {"id":"row-1"} -->
<p>Stray HTML kept safely</p>
<!-- /od:core/row -->
<!-- /od:core/section -->"#;

    let parsed = parse_blocks_lenient_with_schemas(raw, &catalog);
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|d| d.code == "hoisted_freeform_html")
    );
    assert_eq!(parsed.document.blocks[0].id, "sec-1");
    assert_eq!(parsed.document.blocks[0].children.len(), 2);
    assert_eq!(parsed.document.blocks[0].children[0].block_id, "core/row");
    assert!(parsed.document.blocks[0].children[0].children.is_empty());
    assert_eq!(parsed.document.blocks[0].children[1].block_id, "core/text");
    assert_eq!(
        parsed.document.blocks[0].children[1].props.get("content"),
        Some(&json!("<p>Stray HTML kept safely</p>"))
    );
}
