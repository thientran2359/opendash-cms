use super::*;
use crate::theme::TemplateCandidateKind;

#[test]
fn context_target_truth_table_enforces_all_allowed_context_rules() {
    let post_entry = BuilderContextTarget::EntryBody {
        content_type: "post".into(),
    };
    let page_entry = BuilderContextTarget::EntryBody {
        content_type: "page".into(),
    };
    let pattern = BuilderContextTarget::Pattern;
    let theme_pattern = BuilderContextTarget::ThemePattern;
    let part_hdr = BuilderContextTarget::Template {
        kind: TemplateCandidateKind::Part,
        content_type: None,
        taxonomy: None,
        part_key: Some("header".into()),
    };
    let single_post = BuilderContextTarget::Template {
        kind: TemplateCandidateKind::Single,
        content_type: Some("post".into()),
        taxonomy: None,
        part_key: None,
    };
    let singular = BuilderContextTarget::Template {
        kind: TemplateCandidateKind::Singular,
        content_type: None,
        taxonomy: None,
        part_key: None,
    };
    let archive_prod = BuilderContextTarget::Template {
        kind: TemplateCandidateKind::Archive,
        content_type: Some("product".into()),
        taxonomy: None,
        part_key: None,
    };
    let tax_cat = BuilderContextTarget::Template {
        kind: TemplateCandidateKind::Taxonomy,
        content_type: None,
        taxonomy: Some("category".into()),
        part_key: None,
    };

    assert!(post_entry.matches_token("entry_body"));
    assert!(post_entry.matches_token("entry_body:post"));
    assert!(!page_entry.matches_token("entry_body:post"));
    assert!(part_hdr.matches_token("template"));
    assert!(part_hdr.matches_token("part"));
    assert!(part_hdr.matches_token("part:header"));
    assert!(!part_hdr.matches_token("part:footer"));
    assert!(single_post.matches_token("single"));
    assert!(single_post.matches_token("single:post"));
    assert!(singular.matches_token("single"));
    assert!(singular.matches_token("singular"));
    assert!(archive_prod.matches_token("archive:product"));
    assert!(tax_cat.matches_token("taxonomy:category"));
    assert!(!post_entry.matches_token("loop_item"));

    // Negation takes precedence & implicit `*` on negation-only lists
    let list_rule = Some(vec!["*".into(), "!pattern".into(), "!theme_pattern".into()]);
    assert!(post_entry.allows_contexts(list_rule.as_deref()));
    assert!(single_post.allows_contexts(list_rule.as_deref()));
    assert!(!pattern.allows_contexts(list_rule.as_deref()));
    assert!(!theme_pattern.allows_contexts(list_rule.as_deref()));

    let excl_only = Some(vec!["!entry_body:post".into()]);
    assert!(!post_entry.allows_contexts(excl_only.as_deref()));
    assert!(page_entry.allows_contexts(excl_only.as_deref()));

    // Verify catalog enforcement on core/list, core/contact-form, core/pattern-ref, core/template-part
    let catalog = active_test_catalog();
    let list_doc = BuilderDocument {
        schema_version: 1,
        blocks: vec![node(
            "list-1",
            "core/list",
            json!({ "type_key": "post" }),
            vec![],
        )],
    };
    list_doc
        .validate_for_save(&catalog, &post_entry, None)
        .unwrap();
    assert!(
        list_doc
            .validate_for_save(&catalog, &pattern, None)
            .is_err()
    );

    let form_doc = BuilderDocument {
        schema_version: 1,
        blocks: vec![node(
            "form-1",
            "core/contact-form",
            json!({ "form_id": "2" }),
            vec![],
        )],
    };
    form_doc
        .validate_for_save(&catalog, &pattern, None)
        .unwrap();
    assert!(
        form_doc
            .validate_for_save(&catalog, &theme_pattern, None)
            .is_err()
    );

    let pat_doc = BuilderDocument {
        schema_version: 1,
        blocks: vec![node(
            "pat-1",
            "core/pattern-ref",
            json!({ "pattern_id": "3" }),
            vec![],
        )],
    };
    pat_doc
        .validate_for_save(&catalog, &post_entry, None)
        .unwrap();
    assert!(
        pat_doc
            .validate_for_save(&catalog, &single_post, None)
            .is_err()
    );

    let part_doc = BuilderDocument {
        schema_version: 1,
        blocks: vec![node(
            "part-1",
            "core/template-part",
            json!({ "part_key": "hero" }),
            vec![],
        )],
    };
    part_doc
        .validate_for_save(&catalog, &part_hdr, None)
        .unwrap();
    assert!(
        part_doc
            .validate_for_save(&catalog, &post_entry, None)
            .is_err()
    );
}

#[test]
fn list_cross_fields_and_positive_i64_parser_enforce_validity() {
    assert_eq!(parse_positive_i64_prop(Some(&json!(42))), Some(42));
    assert_eq!(parse_positive_i64_prop(Some(&json!(" 17 "))), Some(17));
    assert_eq!(parse_positive_i64_prop(Some(&json!(0))), None);
    assert_eq!(parse_positive_i64_prop(Some(&json!("-5"))), None);

    let catalog = active_test_catalog();
    let bad_list = BuilderDocument {
        schema_version: 1,
        blocks: vec![node(
            "bad-list",
            "core/list",
            json!({ "type_key": "post", "term_id": 7 }),
            vec![],
        )],
    };
    assert!(
        bad_list
            .validate_for_save(&catalog, &entry_ctx(), None)
            .is_err()
    );

    let good_list = BuilderDocument {
        schema_version: 1,
        blocks: vec![node(
            "ok-list",
            "core/list",
            json!({ "type_key": "post", "taxonomy_key": "category", "term_id": 7, "include_descendants": true }),
            vec![],
        )],
    };
    good_list
        .validate_for_save(&catalog, &entry_ctx(), None)
        .unwrap();
}

#[test]
fn loop_item_validation_enforces_limits_allowlist_html_id_and_projection() {
    use crate::field_schema::{FieldSpec, FieldType};

    assert!(BuilderContextTarget::LoopItem.matches_token("loop_item"));
    assert!(!BuilderContextTarget::LoopItem.matches_token("template"));

    let mut cat = active_test_catalog();
    let reg = |cat: &mut RegistryCatalog<RegisteredBlockSchema>, raw: serde_json::Value| {
        let s: BlockSchema = serde_json::from_value(raw).unwrap();
        s.validate("core").unwrap();
        cat.register(
            s.id.clone(),
            ResourceOwner::Core,
            RegisteredBlockSchema { schema: s },
        )
        .unwrap();
    };
    reg(
        &mut cat,
        json!({
            "schema_version": 1, "id": "core/archive-loop", "title": "Loop",
            "category": "template", "render": "native", "supports_children": true,
            "allowed_contexts": ["archive", "taxonomy", "index"],
            "allowed_children": ["core/loop-item"],
            "fields": []
        }),
    );
    reg(
        &mut cat,
        json!({
            "schema_version": 1, "id": "core/loop-item", "title": "Item",
            "category": "template", "render": "native", "supports_children": true,
            "allowed_parents": ["core/archive-loop", "core/entry-related", "core/list"],
            "allowed_contexts": ["*"],
            "fields": []
        }),
    );
    for id in [
        "core/entry-title",
        "core/entry-featured-image",
        "core/entry-meta",
        "core/entry-excerpt",
        "core/entry-terms",
        "core/entry-field",
    ] {
        reg(
            &mut cat,
            json!({
                "schema_version": 1, "id": id, "title": id,
                "category": "template", "render": "native",
                "allowed_contexts": ["single", "archive", "index", "loop_item"],
                "fields": []
            }),
        );
    }

    let mut leaf = minimal_schema("acme/card", false);
    leaf.allowed_contexts = Some(vec!["loop_item".into()]);
    cat.register(
        "acme/card",
        ResourceOwner::Theme("acme".into()),
        RegisteredBlockSchema { schema: leaf },
    )
    .unwrap();

    let mut unsafe_leaf = minimal_schema("acme/rt-card", false);
    unsafe_leaf.allowed_contexts = Some(vec!["loop_item".into()]);
    unsafe_leaf
        .fields
        .push(FieldSpec::new(FieldType::Richtext, "rt", "Rich"));
    cat.register(
        "acme/rt-card",
        ResourceOwner::Theme("acme".into()),
        RegisteredBlockSchema {
            schema: unsafe_leaf,
        },
    )
    .unwrap();

    let idx_ctx = BuilderContextTarget::Template {
        kind: TemplateCandidateKind::Index,
        content_type: None,
        taxonomy: None,
        part_key: None,
    };

    // Valid archive-loop -> loop-item with allowlisted + 1 compatible 3p block
    let valid_doc = BuilderDocument {
        schema_version: 1,
        blocks: vec![node(
            "loop-1",
            "core/archive-loop",
            json!({}),
            vec![node(
                "item-1",
                "core/loop-item",
                json!({}),
                vec![
                    node("img-1", "core/entry-featured-image", json!({}), vec![]),
                    node("title-1", "core/entry-title", json!({}), vec![]),
                    node(
                        "meta-1",
                        "core/entry-meta",
                        json!({ "items": ["published_date", "primary_category", "reading_time"] }),
                        vec![],
                    ),
                    node("fld-1", "core/entry-field", json!({}), vec![]),
                ],
            )],
        )],
    };
    valid_doc.validate_for_save(&cat, &idx_ctx, None).unwrap();

    // Missing core/loop-item under archive-loop fails cardinality
    let empty_loop = BuilderDocument {
        schema_version: 1,
        blocks: vec![node("loop-1", "core/archive-loop", json!({}), vec![])],
    };
    assert!(empty_loop.validate_for_save(&cat, &idx_ctx, None).is_err());

    // html_id inside core/loop-item fails
    let mut bad_meta = node(
        "loop-1",
        "core/archive-loop",
        json!({}),
        vec![node(
            "item-1",
            "core/loop-item",
            json!({}),
            vec![node("t1", "core/entry-title", json!({}), vec![])],
        )],
    );
    bad_meta.children[0].children[0].meta = Some(NodeMeta {
        html_id: Some("dup-id".into()),
        ..Default::default()
    });
    let bad_meta_doc = BuilderDocument {
        schema_version: 1,
        blocks: vec![bad_meta],
    };
    assert!(
        bad_meta_doc
            .validate_for_save(&cat, &idx_ctx, None)
            .is_err()
    );

    // Incompatible 3p block (richtext) fails inside loop
    let bad_3p = BuilderDocument {
        schema_version: 1,
        blocks: vec![node(
            "loop-1",
            "core/archive-loop",
            json!({}),
            vec![node(
                "item-1",
                "core/loop-item",
                json!({}),
                vec![node("bad-card", "acme/rt-card", json!({}), vec![])],
            )],
        )],
    };
    assert!(bad_3p.validate_for_save(&cat, &idx_ctx, None).is_err());

    // Disallowed core block inside loop (core/contact-form)
    let bad_core = BuilderDocument {
        schema_version: 1,
        blocks: vec![node(
            "loop-1",
            "core/archive-loop",
            json!({}),
            vec![node(
                "item-1",
                "core/loop-item",
                json!({}),
                vec![node(
                    "cf",
                    "core/contact-form",
                    json!({ "form_id": 1 }),
                    vec![],
                )],
            )],
        )],
    };
    assert!(bad_core.validate_for_save(&cat, &idx_ctx, None).is_err());

    // > 16 descendants inside loop fails
    let too_many_children: Vec<BlockNode> = (0..17)
        .map(|i| node(&format!("n-{i}"), "core/entry-title", json!({}), vec![]))
        .collect();
    let bad_count = BuilderDocument {
        schema_version: 1,
        blocks: vec![node(
            "loop-1",
            "core/archive-loop",
            json!({}),
            vec![node(
                "item-1",
                "core/loop-item",
                json!({}),
                too_many_children,
            )],
        )],
    };
    assert!(bad_count.validate_for_save(&cat, &idx_ctx, None).is_err());

    // LoopProjection inspection omits core/list from outer doc and captures nested signals
    let proj_doc = BuilderDocument {
        schema_version: 1,
        blocks: vec![
            node(
                "loop-1",
                "core/archive-loop",
                json!({}),
                vec![node(
                    "item-1",
                    "core/loop-item",
                    json!({}),
                    vec![
                        node("img-1", "core/entry-featured-image", json!({}), vec![]),
                        node(
                            "fld-1",
                            "core/entry-field",
                            json!({ "field_key": "price" }),
                            vec![],
                        ),
                    ],
                )],
            ),
            node(
                "list-1",
                "core/list",
                json!({ "type_key": "post" }),
                vec![node(
                    "list-item",
                    "core/loop-item",
                    json!({}),
                    vec![node("ex-1", "core/entry-excerpt", json!({}), vec![])],
                )],
            ),
        ],
    };
    let outer_proj = inspect_loop_projection(&proj_doc);
    assert!(outer_proj.needs_cover);
    assert!(!outer_proj.needs_excerpt);
    assert!(outer_proj.custom_field_keys.contains("price"));

    let list_proj = inspect_loop_projection_nodes(&proj_doc.blocks[1].children);
    assert!(list_proj.needs_excerpt);
    assert!(!list_proj.needs_cover);
}
