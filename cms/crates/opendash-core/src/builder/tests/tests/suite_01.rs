use super::*;

#[test]
fn v3_template_composition_blocks_roundtrip_and_validate() {
    let mut document = fixture();
    let children = &mut document.sections[0].children[0].children[0].children;
    children.push(element(
        "content-slot-1",
        KnownElement::ContentSlot(ContentSlotProps {}),
    ));
    children.push(element(
        "template-part-1",
        KnownElement::TemplatePart(TemplatePartProps {
            part_key: "post-card".into(),
        }),
    ));
    document
        .validate_for_save_with_schemas(&registry(), None)
        .unwrap();

    let encoded = serde_json::to_value(&document).unwrap();
    assert_eq!(
        encoded["sections"][0]["children"][0]["children"][0]["children"][5],
        json!({"id": "content-slot-1", "type": "core/content_slot", "props": {}})
    );
    assert_eq!(
        encoded["sections"][0]["children"][0]["children"][0]["children"][6],
        json!({"id": "template-part-1", "type": "core/template_part",
               "props": {"part_key": "post-card"}})
    );
    let decoded = BuilderDocument::from_json(&document.to_json().unwrap()).unwrap();
    assert_eq!(decoded, document);
    decoded
        .validate_for_save_with_schemas(&registry(), None)
        .unwrap();
}

#[test]
fn template_part_rejects_invalid_key_and_unknown_properties() {
    let mut document = fixture();
    document.sections[0].children[0].children[0]
        .children
        .push(element(
            "template-part-1",
            KnownElement::TemplatePart(TemplatePartProps {
                part_key: "header".into(),
            }),
        ));
    let valid = serde_json::to_value(&document).unwrap();
    let overly_long = "a".repeat(65);
    for bad_key in [
        "",
        "../header",
        "header:footer",
        "Header",
        overly_long.as_str(),
    ] {
        let mut changed = document.clone();
        let ElementContent::Known(KnownElement::TemplatePart(props)) =
            &mut changed.sections[0].children[0].children[0].children[5].content
        else {
            panic!("expected template part");
        };
        props.part_key = bad_key.into();
        assert!(
            changed
                .validate_for_save_with_schemas(&registry(), None)
                .is_err(),
            "{bad_key}"
        );
    }
    let mut bad = valid.clone();
    bad["sections"][0]["children"][0]["children"][0]["children"][5]["props"]["extra"] =
        json!("forbidden");
    let decoded = BuilderDocument::from_json(bad.to_string().as_bytes()).unwrap();
    assert!(
        decoded
            .validate_for_save_with_schemas(&registry(), None)
            .is_err()
    );

    let mut content_slot = valid;
    content_slot["sections"][0]["children"][0]["children"][0]["children"][5] = json!({"id": "content-slot-1", "type": "core/content_slot",
               "props": {"html": "<script>unsafe</script>"}});
    let decoded = BuilderDocument::from_json(content_slot.to_string().as_bytes()).unwrap();
    assert!(
        decoded
            .validate_for_save_with_schemas(&registry(), None)
            .is_err()
    );
}

#[test]
fn round_trip_typed_elements_and_reorder_keep_node_ids() {
    let before = fixture();
    before
        .validate_for_save_with_schemas(&registry(), None)
        .unwrap();
    let after = BuilderDocument::from_json(&before.to_json().unwrap()).unwrap();
    assert_eq!(after, before);
    let mut reordered = after;
    reordered.sections[0].children[0].children[0]
        .children
        .swap(0, 4);
    reordered
        .validate_for_save_with_schemas(&registry(), Some(&before))
        .unwrap();
    assert_eq!(
        reordered.sections[0].children[0].children[0].children[0].id,
        "hero-1"
    );
}

#[test]
fn rejects_unknown_schema_parent_shapes_duplicate_ids_and_limits() {
    let mut wrong_version = fixture();
    wrong_version.schema_version = 2;
    assert!(
        wrong_version
            .validate_for_save_with_schemas(&registry(), None)
            .is_err()
    );

    let mut duplicate = fixture();
    duplicate.sections[0].children[0].id = "section-1".into();
    assert!(
        duplicate
            .validate_for_save_with_schemas(&registry(), None)
            .is_err()
    );

    let mut invalid_id = fixture();
    invalid_id.sections[0].id = "../../etc".into();
    assert!(
        invalid_id
            .validate_for_save_with_schemas(&registry(), None)
            .is_err()
    );

    let wrong_nesting = json!({
        "schema_version": 1,
        "sections": [{"id": "section-1", "children": [{
            "id": "element-1", "type": "core/text", "props": {"text": "oops"}
        }]}]
    });
    assert!(BuilderDocument::from_json(wrong_nesting.to_string().as_bytes()).is_err());

    let mut too_many = fixture();
    too_many.sections[0].children[0].children[0].children = (0..510)
        .map(|index| {
            element(
                &format!("node-{index}"),
                KnownElement::Text(TextProps {
                    text: "x".into(),
                    ..Default::default()
                }),
            )
        })
        .collect();
    assert!(
        too_many
            .validate_for_save_with_schemas(&registry(), None)
            .is_err()
    );
    assert!(BuilderDocument::from_json(&vec![b' '; MAX_BUILDER_JSON_BYTES + 1]).is_err());

    let mut oversized = fixture();
    oversized.sections = (0..100)
        .map(|s| SectionNode {
            id: format!("section-{s}"),
            meta: None,
            responsive: None,
            style: None,
            children: vec![RowNode {
                id: format!("row-{s}"),
                meta: None,
                responsive: None,
                children: vec![ColumnNode {
                    id: format!("column-{s}"),
                    meta: None,
                    responsive: None,
                    children: vec![element(
                        &format!("text-{s}"),
                        KnownElement::Text(TextProps {
                            text: "x".repeat(5000),
                            ..Default::default()
                        }),
                    )],
                }],
            }],
        })
        .collect();
    assert!(
        oversized
            .validate_for_save_with_schemas(&registry(), None)
            .is_err()
    );
}

#[test]
fn typed_props_and_url_policy_reject_invalid_values() {
    let registry = registry();
    for invalid in [
        KnownElement::Heading(HeadingProps {
            level: 0,
            text: "Title".into(),
            binding: None,
        }),
        KnownElement::Heading(HeadingProps {
            level: 7,
            text: "Title".into(),
            binding: None,
        }),
        KnownElement::Image(ImageProps {
            media_id: "../file".into(),
            alt: "x".into(),
            binding: None,
        }),
        KnownElement::Button(ButtonProps {
            label: "Go".into(),
            url: "javascript:alert(1)".into(),
            ..Default::default()
        }),
    ] {
        let mut doc = fixture();
        doc.sections[0].children[0].children[0].children[0].content =
            ElementContent::Known(invalid);
        assert!(doc.validate_for_save_with_schemas(&registry, None).is_err());
    }
    for invalid in [
        "//evil.test",
        "https://user:pass@example.test",
        "data:text/html,hi",
        "http://x.test/a\\b",
        "https://x.test/space here",
        "#anchor",
        "ftp://host/path",
    ] {
        assert!(!valid_button_url(invalid), "{invalid}");
    }
    for allowed in [
        "/",
        "/page?q=1#anchor",
        "https://example.test/path",
        "http://localhost:8080/",
    ] {
        assert!(valid_button_url(allowed), "{allowed}");
    }
    let malformed = json!({
        "schema_version": 1,
        "sections": [{"id": "section-1", "children": [{"id": "row-1", "children": [{
            "id": "column-1", "children": [{"id": "heading-1", "type": "core/heading", "props": {"level": "not-a-number", "text": "x"}}]
        }]}]}]
    });
    let decoded = BuilderDocument::from_json(malformed.to_string().as_bytes()).unwrap();
    assert!(
        decoded
            .validate_for_save_with_schemas(&registry, None)
            .is_err()
    );
}

#[test]
fn unavailable_legacy_and_unknown_saved_payload_preserved_only_unchanged() {
    let mut existing = fixture();
    existing.sections[0].children[0].children[0].children[4] = element(
        "legacy-1",
        KnownElement::Legacy(LegacyBlockProps {
            block_id: "ocean/hero".into(),
            values: BTreeMap::from([("title".into(), "Sea".into())]),
        }),
    );
    // Per Phase 2 Decision D3, legacy/block cannot be newly inserted or edited
    // even when a block schema of the same ID exists, and survives only when
    // preserved verbatim from trusted persisted state.
    let active = registry();
    assert!(
        existing
            .validate_for_save_with_schemas(&active, Some(&existing))
            .is_ok()
    );
    assert!(
        existing
            .validate_for_save_with_schemas(&active, None)
            .is_err()
    );

    let mut edited = existing.clone();
    let ElementContent::Known(KnownElement::Legacy(props)) =
        &mut edited.sections[0].children[0].children[0].children[4].content
    else {
        panic!()
    };
    props.values.insert("title".into(), "Changed".into());
    assert!(
        edited
            .validate_for_save_with_schemas(&active, Some(&existing))
            .is_err()
    );

    let mut moved = existing.clone();
    moved.sections[0].children[0].children[0]
        .children
        .swap(3, 4);
    assert!(
        moved
            .validate_for_save_with_schemas(&active, Some(&existing))
            .is_err()
    );

    let unknown = json!({"type": "theme/missing", "props": {"opaque": [1, {"a": true}]}, "original": "retained"});
    let mut persisted = fixture();
    persisted.sections[0].children[0].children[0]
        .children
        .push(ElementNode {
            id: "unknown-1".into(),
            meta: None,
            content: ElementContent::Unresolved(serde_json::from_value(unknown).unwrap()),
        });
    assert!(
        persisted
            .validate_for_save_with_schemas(&registry(), None)
            .is_err()
    );
    let decoded = BuilderDocument::from_json(&persisted.to_json().unwrap()).unwrap();
    assert_eq!(decoded, persisted);
    assert!(
        decoded
            .validate_for_save_with_schemas(&registry(), Some(&persisted))
            .is_ok()
    );
    let mut updated = decoded;
    updated.sections[0].children[0].children[0].children[5].id = "unknown-2".into();
    assert!(
        updated
            .validate_for_save_with_schemas(&registry(), Some(&persisted))
            .is_err()
    );
}

#[test]
fn validates_field_binding_protocol_and_distinct_source_cap() {
    let mut doc = fixture();
    let elements = &mut doc.sections[0].children[0].children[0].children;
    elements.clear();
    for index in 0..32 {
        elements.push(element(
            &format!("binding-{index}"),
            KnownElement::Text(TextProps {
                text: String::new(),
                binding: Some(ElementBinding {
                    type_key: "post".into(),
                    entry_slug: format!("post-{index}"),
                    entry_locale: None,
                    source: BindingSource::Title,
                    target: BindingTarget::Text,
                }),
                ..Default::default()
            }),
        ));
    }
    doc.validate_for_save_with_schemas(&registry(), None)
        .unwrap();
    assert_eq!(
        BuilderDocument::from_json(&doc.to_json().unwrap()).unwrap(),
        doc
    );
    let bound = &mut doc.sections[0].children[0].children[0].children[0].content;
    let ElementContent::Known(KnownElement::Text(props)) = bound else {
        panic!()
    };
    let binding = props.binding.as_mut().unwrap();
    binding.target = BindingTarget::Url;
    assert!(
        doc.validate_for_save_with_schemas(&registry(), None)
            .is_err()
    );
    let ElementContent::Known(KnownElement::Text(props)) =
        &mut doc.sections[0].children[0].children[0].children[0].content
    else {
        panic!()
    };
    props.binding.as_mut().unwrap().target = BindingTarget::Text;
    props.binding.as_mut().unwrap().entry_slug = "../broken".into();
    assert!(
        doc.validate_for_save_with_schemas(&registry(), None)
            .is_err()
    );
    let ElementContent::Known(KnownElement::Text(props)) =
        &mut doc.sections[0].children[0].children[0].children[0].content
    else {
        panic!()
    };
    props.binding.as_mut().unwrap().entry_slug = "post-0".into();
    doc.sections[0].children[0].children[0]
        .children
        .push(element(
            "binding-33",
            KnownElement::Text(TextProps {
                text: String::new(),
                binding: Some(ElementBinding {
                    type_key: "post".into(),
                    entry_slug: "extra".into(),
                    entry_locale: None,
                    source: BindingSource::Title,
                    target: BindingTarget::Text,
                }),
                ..Default::default()
            }),
        ));
    assert!(
        doc.validate_for_save_with_schemas(&registry(), None)
            .is_err()
    );
}

#[test]
fn legacy_block_rejected_for_new_inserts_even_with_active_schema() {
    let mut doc = fixture();
    doc.sections[0].children[0].children[0].children[4] = element(
        "legacy-1",
        KnownElement::Legacy(LegacyBlockProps {
            block_id: "ocean/hero".into(),
            values: BTreeMap::from([("title".into(), "Sea".into())]),
        }),
    );
    assert!(
        doc.validate_for_save_with_schemas(&registry(), None)
            .is_err()
    );
}

#[test]
fn unknown_known_element_fields_cannot_be_silently_dropped() {
    let mut doc = serde_json::to_value(fixture()).unwrap();
    doc["sections"][0]["children"][0]["children"][0]["children"][0]["extra"] =
        json!({"must_keep": 3});
    let restored = BuilderDocument::from_json(doc.to_string().as_bytes()).unwrap();
    assert!(
        restored
            .validate_for_save_with_schemas(&registry(), None)
            .is_err()
    );
    // A historical node is losslessly preserved while the type is unavailable.
    let raw = restored.to_json().unwrap();
    assert!(String::from_utf8(raw).unwrap().contains("must_keep"));
}
