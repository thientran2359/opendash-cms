use super::*;

#[test]
fn responsive_sparse_zero_round_trip_and_existing_v1_compatibility() {
    let before = fixture();
    let old_json = before.to_json().unwrap();
    assert!(!String::from_utf8_lossy(&old_json).contains("responsive"));
    assert_eq!(BuilderDocument::from_json(&old_json).unwrap(), before);

    let mut current = before.clone();
    current.sections[0].responsive = Some(Responsive {
        desktop: Some(ResponsiveStyle {
            padding_px: Some(0),
            ..Default::default()
        }),
        ..Default::default()
    });
    let row = &mut current.sections[0].children[0];
    row.responsive = Some(Responsive {
        desktop: Some(ResponsiveStyle {
            gap_px: Some(0),
            columns_per_row: Some(3),
            ..Default::default()
        }),
        tablet: Some(ResponsiveStyle {
            columns_per_row: Some(2),
            ..Default::default()
        }),
        mobile: Some(ResponsiveStyle {
            columns_per_row: Some(1),
            ..Default::default()
        }),
    });
    current.sections[0].children[0].children[0].responsive = Some(Responsive {
        desktop: Some(ResponsiveStyle {
            text_align: Some(TextAlign::Start),
            ..Default::default()
        }),
        mobile: Some(ResponsiveStyle {
            text_align: Some(TextAlign::End),
            ..Default::default()
        }),
        ..Default::default()
    });
    current
        .validate_for_save_with_schemas(&registry(), Some(&before))
        .unwrap();
    assert!(
        current.sections[0].children[0]
            .responsive
            .as_ref()
            .unwrap()
            .opts_into_grid()
    );
    assert_eq!(
        BuilderDocument::from_json(&current.to_json().unwrap()).unwrap(),
        current
    );
    let mut blank = current.clone();
    blank.sections[0].responsive = Some(Responsive {
        desktop: Some(ResponsiveStyle::default()),
        ..Default::default()
    });
    let encoded = serde_json::to_value(&blank).unwrap();
    assert!(encoded["sections"][0].get("responsive").is_none());
}

#[test]
fn responsive_rejects_wrong_types_contexts_and_limits() {
    let original = fixture();
    for (node, style) in [
        ("section", json!({"gap_px": 2})),
        ("section", json!({"text_align": "center"})),
        ("column", json!({"columns_per_row": 2})),
        ("row", json!({"text_align": "end"})),
        ("row", json!({"columns_per_row": 0})),
        ("row", json!({"columns_per_row": 7})),
        ("row", json!({"gap_px": 65})),
        ("column", json!({"padding_px": 65})),
    ] {
        let mut value = serde_json::to_value(&original).unwrap();
        let target = match node {
            "section" => &mut value["sections"][0],
            "row" => &mut value["sections"][0]["children"][0],
            _ => &mut value["sections"][0]["children"][0]["children"][0],
        };
        target["responsive"] = json!({"tablet": style});
        let doc = BuilderDocument::from_json(value.to_string().as_bytes()).unwrap();
        assert!(
            doc.validate_for_save_with_schemas(&registry(), Some(&original))
                .is_err(),
            "{node}: {style}"
        );
    }
    for bad in [
        json!({"padding_px": -1}),
        json!({"padding_px": 1.5}),
        json!({"padding_px": "10"}),
        json!({"padding_px": true}),
        json!({"text_align": "diagonal"}),
        json!({"gap_px": 0, "color": "red"}),
    ] {
        let mut value = serde_json::to_value(&original).unwrap();
        value["sections"][0]["children"][0]["responsive"] = json!({"mobile": bad});
        assert!(
            BuilderDocument::from_json(value.to_string().as_bytes()).is_err(),
            "{bad}"
        );
    }
    let mut invalid = serde_json::to_value(&original).unwrap();
    invalid["sections"][0]["responsive"] = json!({"phone": {"padding_px": 3}});
    assert!(BuilderDocument::from_json(invalid.to_string().as_bytes()).is_err());
}

#[test]
fn list_defaults_limits_and_preserved_reference_contract() {
    let original = json!({
        "schema_version": 1,
        "sections": [{"id": "section-1", "children": [{
            "id": "row-1", "children": [{"id": "column-1", "children": [{
                "id": "list-1", "type": "core/list", "props": {"type_key": "post"}
            }]}]
        }]}]
    });
    let document = BuilderDocument::from_json(original.to_string().as_bytes()).unwrap();
    document
        .validate_for_save_with_schemas(&registry(), None)
        .unwrap();
    let ElementContent::Known(KnownElement::List(props)) =
        &document.sections[0].children[0].children[0].children[0].content
    else {
        panic!("valid List must decode into typed core/list")
    };
    assert_eq!(props.page_size, 6);
    assert_eq!(props.sort, ListSort::IdDesc);
    assert!(!props.include_descendants);
    assert_eq!(props.category_id, None);
    assert_eq!(props.taxonomy_key, None);
    assert_eq!(props.term_id, None);
    assert_eq!(
        BuilderDocument::from_json(&document.to_json().unwrap()).unwrap(),
        document
    );

    for bad in [
        json!({"page_size": 0}),
        json!({"page_size": 21}),
        json!({"page_size": "6"}),
        json!({"category_id": 0}),
        json!({"tag_id": -1}),
        json!({"include_descendants": true}),
        json!({"taxonomy_key": "genre"}),
        json!({"term_id": 42}),
        json!({"taxonomy_key": "bad-key", "term_id": 42}),
        json!({"taxonomy_key": "genre", "term_id": 0}),
        json!({"taxonomy_key": "genre", "term_id": 42, "tag_id": 6}),
        json!({"sort": "id_random"}),
        json!({"type_key": "invalid-key"}),
        json!({"extra": "injection"}),
    ] {
        let mut encoded = original.clone();
        for (key, value) in bad.as_object().unwrap() {
            encoded["sections"][0]["children"][0]["children"][0]["children"][0]["props"][key] =
                value.clone();
        }
        let candidate = BuilderDocument::from_json(encoded.to_string().as_bytes()).unwrap();
        assert!(
            candidate
                .validate_for_save_with_schemas(&registry(), None)
                .is_err(),
            "{bad}"
        );
    }
    // Correctly typed descendant filter with valid category and tag IDs.
    let mut filtered = original.clone();
    filtered["sections"][0]["children"][0]["children"][0]["children"][0]["props"] = json!({"type_key": "post", "category_id": 12, "tag_id": 22,
               "include_descendants": true, "sort": "id_asc", "page_size": 20});
    let filtered = BuilderDocument::from_json(filtered.to_string().as_bytes()).unwrap();
    filtered
        .validate_for_save_with_schemas(&registry(), None)
        .unwrap();

    let mut generic = original.clone();
    generic["sections"][0]["children"][0]["children"][0]["children"][0]["props"] = json!({"type_key": "post", "taxonomy_key": "genre", "term_id": 37,
               "include_descendants": true});
    let generic = BuilderDocument::from_json(generic.to_string().as_bytes()).unwrap();
    generic
        .validate_for_save_with_schemas(&registry(), None)
        .unwrap();
    let roundtrip = BuilderDocument::from_json(&generic.to_json().unwrap()).unwrap();
    assert_eq!(roundtrip, generic);

    let mut many = original.clone();
    let children = many["sections"][0]["children"][0]["children"][0]["children"]
        .as_array_mut()
        .unwrap();
    for index in 2..=8 {
        children.push(json!({"id": format!("list-{index}"), "type": "core/list",
                             "props": {"type_key": "post", "page_size": 1}}));
    }
    // End the mutable borrow before serializing the document.
    let _ = children;
    BuilderDocument::from_json(many.to_string().as_bytes())
        .unwrap()
        .validate_for_save_with_schemas(&registry(), None)
        .unwrap();
    many["sections"][0]["children"][0]["children"][0]["children"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "list-9", "type": "core/list", "props": {"type_key": "post"}}));
    assert!(
        BuilderDocument::from_json(many.to_string().as_bytes())
            .unwrap()
            .validate_for_save_with_schemas(&registry(), None)
            .is_err()
    );
}

#[test]
fn route_context_allowed_only_on_single_or_page_and_checks_fields() {
    let raw = json!({
        "schema_version": 1,
        "sections": [{
            "id": "sec-1",
            "children": [{
                "id": "row-1",
                "children": [{
                    "id": "col-1",
                    "children": [{
                        "id": "heading-1",
                        "type": "core/heading",
                        "props": {
                            "text": "Fallback",
                            "level": 1,
                            "binding": {
                                "target": "text",
                                "source": { "kind": "route_context", "field_key": "title" }
                            }
                        }
                    }]
                }]
            }]
        }]
    });
    let doc = BuilderDocument::from_json(raw.to_string().as_bytes()).unwrap();
    doc.validate_for_save_with_schemas(&registry(), None)
        .unwrap();
    assert!(
        doc.validate_route_context_for_role(Some("single"), None)
            .is_ok()
    );
    assert!(
        doc.validate_route_context_for_role(Some("page"), None)
            .is_ok()
    );
    assert!(
        doc.validate_route_context_for_role(Some("home"), None)
            .is_err()
    );
    assert!(
        doc.validate_route_context_for_role(Some("archive"), None)
            .is_err()
    );
    assert!(
        doc.validate_route_context_for_role(Some("single"), Some(&[]))
            .is_ok()
    );

    let custom_field = json!({
        "schema_version": 1,
        "sections": [{
            "id": "sec-1",
            "children": [{
                "id": "row-1",
                "children": [{
                    "id": "col-1",
                    "children": [{
                        "id": "text-1",
                        "type": "core/text",
                        "props": {
                            "text": "Fallback",
                            "binding": {
                                "target": "text",
                                "source": { "kind": "route_context", "field_key": "summary" }
                            }
                        }
                    }]
                }]
            }]
        }]
    });
    let custom_doc = BuilderDocument::from_json(custom_field.to_string().as_bytes()).unwrap();
    assert!(
        custom_doc
            .validate_route_context_for_role(Some("single"), Some(&["summary".to_string()]))
            .is_ok()
    );
    assert!(
        custom_doc
            .validate_route_context_for_role(Some("single"), Some(&["other".to_string()]))
            .is_err()
    );
}
