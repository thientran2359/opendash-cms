use super::*;

#[test]
fn unified_recursive_ast_allows_root_blocks_nested_containers_and_depth_limit() {
    let catalog = active_test_catalog();
    let doc = BuilderDocument {
        schema_version: BUILDER_SCHEMA_VERSION,
        blocks: vec![
            node(
                "root-heading",
                "core/heading",
                json!({ "text": "Không cần bọc Section" }),
                vec![],
            ),
            node(
                "sec-1",
                "core/section",
                json!({}),
                vec![node(
                    "row-1",
                    "core/row",
                    json!({}),
                    vec![node(
                        "col-1",
                        "core/column",
                        json!({}),
                        vec![node(
                            "stk-1",
                            "core/stack",
                            json!({ "direction": "row" }),
                            vec![node(
                                "btn-1",
                                "core/button",
                                json!({ "cta": { "url": "/docs", "label": "Docs", "new_tab": false, "nofollow": false } }),
                                vec![],
                            )],
                        )],
                    )],
                )],
            ),
        ],
    };

    doc.validate_for_save(&catalog, &entry_ctx(), None).unwrap();
    let roundtrip = BuilderDocument::from_json(&doc.to_json().unwrap()).unwrap();
    assert_eq!(roundtrip, doc);

    // Depth 8 allowed, Depth 9 rejected
    let make_chain = |depth: usize| -> BuilderDocument {
        let mut cur = node(
            "leaf-deep",
            "core/heading",
            json!({ "text": "Deep" }),
            vec![],
        );
        for d in (1..depth).rev() {
            cur = node(&format!("stk-{d}"), "core/stack", json!({}), vec![cur]);
        }
        BuilderDocument {
            schema_version: 1,
            blocks: vec![cur],
        }
    };
    make_chain(8)
        .validate_for_save(&catalog, &entry_ctx(), None)
        .unwrap();
    assert!(
        make_chain(9)
            .validate_for_save(&catalog, &entry_ctx(), None)
            .is_err()
    );
}

#[test]
fn rejects_legacy_sections_ast_wrong_version_leaf_children_and_constraints() {
    let err = BuilderDocument::from_json(br#"{"schema_version":1,"sections":[]}"#).unwrap_err();
    assert!(err.to_string().contains("tiền-V1"));

    let err_v0 = BuilderDocument::from_json(br#"{"schema_version":0,"blocks":[]}"#).unwrap_err();
    assert!(err_v0.to_string().contains("tiền-V1"));

    let catalog = active_test_catalog();

    // Leaf cannot have children
    let leaf_child = BuilderDocument {
        schema_version: 1,
        blocks: vec![node(
            "hd-1",
            "core/heading",
            json!({ "text": "Bad" }),
            vec![node(
                "hd-2",
                "core/heading",
                json!({ "text": "Inner" }),
                vec![],
            )],
        )],
    };
    assert!(
        leaf_child
            .validate_for_save(&catalog, &entry_ctx(), None)
            .is_err()
    );

    // core/column cannot live at root
    let stray_col = BuilderDocument {
        schema_version: 1,
        blocks: vec![node("col-1", "core/column", json!({}), vec![])],
    };
    assert!(
        stray_col
            .validate_for_save(&catalog, &entry_ctx(), None)
            .is_err()
    );

    // core/row only allows core/column
    let bad_row_child = BuilderDocument {
        schema_version: 1,
        blocks: vec![node(
            "row-1",
            "core/row",
            json!({}),
            vec![node(
                "hd-1",
                "core/heading",
                json!({ "text": "Nope" }),
                vec![],
            )],
        )],
    };
    assert!(
        bad_row_child
            .validate_for_save(&catalog, &entry_ctx(), None)
            .is_err()
    );

    // Duplicate IDs & > 1 content-slot & > 8 lists & > 16 pattern-refs rejected
    let dup = BuilderDocument {
        schema_version: 1,
        blocks: vec![
            node("same-id", "core/heading", json!({ "text": "A" }), vec![]),
            node("same-id", "core/heading", json!({ "text": "B" }), vec![]),
        ],
    };
    assert!(dup.validate_for_save(&catalog, &entry_ctx(), None).is_err());

    let two_slots = BuilderDocument {
        schema_version: 1,
        blocks: vec![
            node("slot-1", "core/content-slot", json!({}), vec![]),
            node("slot-2", "core/content-slot", json!({}), vec![]),
        ],
    };
    let page_tpl_ctx = BuilderContextTarget::Template {
        kind: crate::theme::TemplateCandidateKind::Single,
        content_type: Some("post".into()),
        taxonomy: None,
        part_key: None,
    };
    assert!(
        two_slots
            .validate_for_save(&catalog, &page_tpl_ctx, None)
            .is_err()
    );

    let nine_lists = BuilderDocument {
        schema_version: 1,
        blocks: (1..=9)
            .map(|i| {
                node(
                    &format!("list-{i}"),
                    "core/list",
                    json!({ "type_key": "post" }),
                    vec![],
                )
            })
            .collect(),
    };
    assert!(
        nine_lists
            .validate_for_save(&catalog, &entry_ctx(), None)
            .is_err()
    );

    let seventeen_patterns = BuilderDocument {
        schema_version: 1,
        blocks: (1..=17)
            .map(|i| {
                node(
                    &format!("pat-{i}"),
                    "core/pattern-ref",
                    json!({ "pattern_id": "1" }),
                    vec![],
                )
            })
            .collect(),
    };
    assert!(
        seventeen_patterns
            .validate_for_save(&catalog, &entry_ctx(), None)
            .is_err()
    );
}
