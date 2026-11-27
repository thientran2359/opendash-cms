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
            "l1",
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
            "f1",
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
            "p1",
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
            "tp1",
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
            json!({ "type_key": "post", "term_id": "7" }),
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
            json!({ "type_key": "post", "taxonomy_key": "category", "term_id": "7", "include_descendants": true }),
            vec![],
        )],
    };
    good_list
        .validate_for_save(&catalog, &entry_ctx(), None)
        .unwrap();
}
