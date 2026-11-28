#![cfg(test)]

use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;

use super::*;
use crate::{
    field_schema::FieldSpec,
    registry::{RegistryCatalog, ResourceOwner},
};

fn sample_hex_id(fill: char) -> String {
    std::iter::repeat_n(fill, 64).collect()
}

fn minimal_block(id: &str, render: &str) -> BlockSchema {
    BlockSchema {
        schema_version: 1,
        id: id.into(),
        title: "Sample".into(),
        category: BlockCategory::Content,
        icon: None,
        render: render.into(),
        supports_children: false,
        allowed_parents: None,
        allowed_children: None,
        allowed_contexts: None,
        default_children: Vec::new(),
        presets: Vec::new(),
        fields: Vec::new(),
    }
}

#[test]
fn validates_unified_v1_block_shapes_and_rejects_legacy_syntax() {
    let showcase: BlockSchema = serde_json::from_value(json!({
        "schema_version": 1,
        "id": "core/showcase",
        "title": "Showcase",
        "category": "content",
        "icon": "sparkles",
        "render": "render.html",
        "fields": [
            { "type": "tab", "key": "tab_main", "label": "Nội dung" },
            { "type": "message", "key": "hint", "label": "Mẹo", "body": "Dùng unified fields." },
            {
                "type": "group",
                "key": "hero_card",
                "label": "Hero Card",
                "fields": [
                    { "type": "media", "key": "cover_image", "label": "Cover", "media_kind": "image" },
                    { "type": "link", "key": "primary_link", "label": "Link", "default": { "url": "/gioi-thieu", "label": "Xem", "new_tab": false, "nofollow": false } },
                    { "type": "icon", "key": "badge_icon", "label": "Badge Icon", "default": "sparkles" }
                ]
            },
            {
                "type": "select",
                "key": "linked_menu",
                "label": "Menu",
                "options_source": "menus",
                "allow_null": true
            },
            {
                "type": "select",
                "key": "target_tax",
                "label": "Taxonomy",
                "options_source": "taxonomies"
            },
            {
                "type": "term_ref",
                "key": "dynamic_terms",
                "label": "Terms",
                "taxonomy_from_field": "target_tax",
                "multiple": true
            },
            {
                "type": "entry_ref",
                "key": "related_any",
                "label": "Related",
                "target_types": ["*"],
                "multiple": true
            },
            {
                "type": "spacing",
                "key": "outer_margin",
                "label": "Margin",
                "property": "margin",
                "min": -64,
                "max": 128,
                "unit": "px",
                "responsive": true
            },
            {
                "type": "repeater",
                "key": "highlights",
                "label": "Highlights",
                "min_items": 1,
                "max_items": 8,
                "fields": [
                    { "type": "text", "key": "title", "label": "Title", "required": true },
                    {
                        "type": "group",
                        "key": "meta",
                        "label": "Meta",
                        "fields": [
                            { "type": "icon", "key": "item_icon", "label": "Icon" }
                        ]
                    }
                ]
            }
        ]
    }))
    .unwrap();
    showcase.validate("core").unwrap();
    assert!(showcase.validate("my-theme").is_err());

    let legacy_show_if = json!({
        "schema_version": 1,
        "id": "core/legacy",
        "title": "Legacy",
        "category": "content",
        "render": "render.html",
        "fields": [
            { "type": "boolean", "key": "flag", "label": "Flag" },
            { "type": "text", "key": "cta", "label": "CTA", "show_if": { "field": "flag", "equals": true } }
        ]
    });
    assert!(serde_json::from_value::<BlockSchema>(legacy_show_if).is_err());

    let mut too_many = minimal_block("core/big", "render.html");
    too_many.fields = (0..33)
        .map(|i| {
            FieldSpec::new(
                crate::field_schema::FieldType::Text,
                format!("f_{i}"),
                format!("Field {i}"),
            )
        })
        .collect();
    assert!(too_many.validate("core").is_err());
}

#[test]
fn validates_values_with_dnf_conditions_upgrade_exemption_and_extraction() {
    let schema: BlockSchema = serde_json::from_value(json!({
        "schema_version": 1,
        "id": "core/feature",
        "title": "Feature",
        "category": "content",
        "render": "render.html",
        "fields": [
            { "type": "boolean", "key": "show_extra", "label": "Show Extra", "default": false },
            {
                "type": "text",
                "key": "extra_title",
                "label": "Extra Title",
                "required": true,
                "conditions": [[{ "field": "show_extra", "op": "eq", "value": true }]]
            },
            { "type": "text", "key": "new_required", "label": "New Required", "required": true },
            { "type": "media", "key": "hero", "label": "Hero", "media_kind": "any" },
            { "type": "media", "key": "manual", "label": "Manual", "media_kind": "file" },
            { "type": "richtext", "key": "body", "label": "Body" },
            { "type": "select", "key": "pat", "label": "Pattern", "options_source": "patterns" },
            { "type": "select", "key": "part", "label": "Part", "options_source": "template_parts" },
            { "type": "select", "key": "form", "label": "Form", "options_source": "contact_forms" },
            { "type": "range", "key": "pad", "label": "Pad", "min": 0, "max": 80, "responsive": true, "unit": "px" },
            {
                "type": "group",
                "key": "card",
                "label": "Card",
                "fields": [
                    { "type": "media", "key": "pic", "label": "Pic", "media_kind": "image" },
                    { "type": "link", "key": "btn", "label": "Btn" }
                ]
            },
            {
                "type": "repeater",
                "key": "items",
                "label": "Items",
                "fields": [
                    { "type": "text", "key": "title", "label": "Title", "required": true },
                    { "type": "text", "key": "newly_required_sub", "label": "Sub", "required": true },
                    { "type": "media", "key": "thumb", "label": "Thumb", "media_kind": "image" },
                    { "type": "richtext", "key": "desc", "label": "Desc" }
                ]
            }
        ]
    }))
    .unwrap();
    schema.validate("core").unwrap();

    let img_id = sample_hex_id('a');
    let any_id = sample_hex_id('b');
    let file_id = sample_hex_id('c');
    let card_pic_id = sample_hex_id('d');

    let prev = BTreeMap::from([
        ("show_extra".into(), json!(false)),
        ("legacy_unknown_field".into(), json!("keep-me")),
        (
            "items".into(),
            json!([
                { "title": "Item 1", "thumb": img_id, "desc": "<p>Chi tiết <b>1</b></p>" }
            ]),
        ),
    ]);
    let mut cur = prev.clone();
    cur.insert("hero".into(), json!(any_id));
    cur.insert("manual".into(), json!(file_id));
    cur.insert("body".into(), json!("<p>Xin chào</p>"));
    cur.insert("pat".into(), json!("77"));
    cur.insert("part".into(), json!("hero-header"));
    cur.insert("form".into(), json!("3"));
    cur.insert(
        "pad".into(),
        json!({ "desktop": 40, "tablet": 24, "mobile": 16 }),
    );
    cur.insert(
        "card".into(),
        json!({
            "pic": card_pic_id,
            "btn": { "url": "/xem", "label": "Đọc thêm", "new_tab": false, "nofollow": false }
        }),
    );

    schema.validate_values(&cur, Some(&prev)).unwrap();
    let media = schema.collect_media_ids(&cur);
    assert_eq!(media.image_ids, BTreeSet::from([img_id, card_pic_id]));
    assert_eq!(media.any_ids, BTreeSet::from([any_id]));
    assert_eq!(media.file_ids, BTreeSet::from([file_id]));
    assert_eq!(schema.collect_pattern_ids(&cur), BTreeSet::from([77]));
    assert!(schema.has_active_contact_form(&cur));

    let mut texts = Vec::new();
    schema.extract_visible_text(&cur, &mut texts);
    assert_eq!(texts, vec!["Xin chào", "Đọc thêm", "Item 1", "Chi tiết 1"]);
}

#[test]
fn enforces_render_native_reserved_keys_sparse_props_and_catalog_graph() {
    minimal_block("core/heading", "native")
        .validate("core")
        .unwrap();
    assert!(
        minimal_block("acme/heading", "native")
            .validate("acme")
            .is_err()
    );

    for reserved in RESERVED_BLOCK_FIELD_KEYS {
        let mut blk = minimal_block("core/bad", "render.html");
        blk.fields.push(FieldSpec::new(
            crate::field_schema::FieldType::Text,
            *reserved,
            "Reserved",
        ));
        assert!(blk.validate("core").is_err(), "{reserved}");
    }

    let mut leaf_with_children = minimal_block("core/leaf", "native");
    leaf_with_children.allowed_children = Some(vec!["*".into()]);
    assert!(leaf_with_children.validate("core").is_err());

    let mut props_blk = minimal_block("core/btn", "native");
    let mut title = FieldSpec::new(crate::field_schema::FieldType::Text, "label", "Label");
    title.default = Some(json!("Xem"));
    props_blk.fields.push(title);

    let mut map = BTreeMap::from([("label".into(), json!("Xem"))]);
    props_blk.strip_default_props(&mut map);
    assert!(map.is_empty(), "strict == default must be stripped");

    let mut keep_empty = BTreeMap::from([("label".into(), json!(""))]);
    props_blk.strip_default_props(&mut keep_empty);
    assert_eq!(keep_empty.get("label"), Some(&json!("")));

    let hydrated = props_blk.apply_field_defaults(&BTreeMap::new());
    assert_eq!(hydrated.get("label"), Some(&json!("Xem")));

    // Phase 2 catalog validation + cycle detection
    let mut row = minimal_block("core/row", "native");
    row.supports_children = true;
    row.allowed_children = Some(vec!["core/column".into()]);
    row.default_children = vec![DefaultChildSpec {
        block_id: "core/column".into(),
        props: BTreeMap::new(),
        children: Vec::new(),
    }];
    row.allowed_contexts = Some(vec!["*".into(), "!loop_item".into()]);
    row.validate("core").unwrap();

    let mut col = minimal_block("core/column", "native");
    col.supports_children = true;
    col.allowed_parents = Some(vec!["core/row".into()]);
    col.validate("core").unwrap();

    let mut ok_cat = RegistryCatalog::new();
    ok_cat
        .register(
            "core/row",
            ResourceOwner::Core,
            RegisteredBlockSchema { schema: row },
        )
        .unwrap();
    ok_cat
        .register(
            "core/column",
            ResourceOwner::Core,
            RegisteredBlockSchema {
                schema: col.clone(),
            },
        )
        .unwrap();
    validate_block_catalog_templates(&ok_cat).unwrap();

    let mut cyc_a = minimal_block("acme/box-a", "render.html");
    cyc_a.supports_children = true;
    cyc_a.default_children = vec![DefaultChildSpec {
        block_id: "acme/box-b".into(),
        props: BTreeMap::new(),
        children: Vec::new(),
    }];
    let mut cyc_b = minimal_block("acme/box-b", "render.html");
    cyc_b.supports_children = true;
    cyc_b.default_children = vec![DefaultChildSpec {
        block_id: "acme/box-a".into(),
        props: BTreeMap::new(),
        children: Vec::new(),
    }];
    let mut bad_cat = RegistryCatalog::new();
    bad_cat
        .register(
            "acme/box-a",
            ResourceOwner::Theme("acme".into()),
            RegisteredBlockSchema { schema: cyc_a },
        )
        .unwrap();
    bad_cat
        .register(
            "acme/box-b",
            ResourceOwner::Theme("acme".into()),
            RegisteredBlockSchema { schema: cyc_b },
        )
        .unwrap();
    assert!(validate_block_catalog_templates(&bad_cat).is_err());
}

#[test]
fn loop_schema_compatibility_enforces_explicit_leaf_and_recursive_field_bans() {
    use crate::field_schema::{FieldType, SelectOptionsSource};

    let cat: BlockCategory = serde_json::from_str("\"template\"").unwrap();
    assert_eq!(cat, BlockCategory::Template);

    let mut blk = minimal_block("acme/card", "render.html");
    blk.allowed_contexts = Some(vec!["*".into()]);
    assert!(!blk.is_loop_schema_compatible());

    blk.allowed_contexts = Some(vec!["loop_item".into()]);
    assert!(blk.is_loop_schema_compatible());

    blk.supports_children = true;
    assert!(!blk.is_loop_schema_compatible());
    blk.supports_children = false;

    for forbidden in [
        FieldType::Repeater,
        FieldType::Richtext,
        FieldType::Gallery,
        FieldType::EntryRef,
        FieldType::TermRef,
    ] {
        let mut bad = blk.clone();
        bad.fields
            .push(FieldSpec::new(forbidden, "bad_field", "Bad"));
        assert!(!bad.is_loop_schema_compatible());

        let mut grp = FieldSpec::new(FieldType::Group, "grp", "Group");
        grp.fields
            .push(FieldSpec::new(forbidden, "nested_bad", "Nested Bad"));
        let mut bad_nested = blk.clone();
        bad_nested.fields.push(grp);
        assert!(!bad_nested.is_loop_schema_compatible());
    }

    let mut sel = FieldSpec::new(FieldType::Select, "menu", "Menu");
    sel.options_source = Some(SelectOptionsSource::Menus);
    let mut grp = FieldSpec::new(FieldType::Group, "grp", "Group");
    grp.fields.push(sel);
    let mut bad_opts = blk;
    bad_opts.fields.push(grp);
    assert!(!bad_opts.is_loop_schema_compatible());
}
