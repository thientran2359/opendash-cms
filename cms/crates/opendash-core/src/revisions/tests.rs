use std::collections::BTreeMap;

use serde_json::json;

use crate::builder::{
    BuilderDocument, ColumnNode, ElementContent, ElementNode, HeadingProps, KnownElement, NodeMeta,
    PluginBlockProps, RowNode, SectionNode,
};
use crate::content::ContentMode;

use super::*;

#[test]
fn canonical_json_normalizes_object_keys_negative_zero_and_safe_floats() {
    let val = json!({
        "z": -0.0,
        "a": 42.0,
        "b": 3.5,
        "nested": { "k2": 1.0, "k1": "ok" }
    });
    let canonical = canonical_json_value(&val);
    assert_eq!(
        serde_json::to_string(&canonical).unwrap(),
        r#"{"a":42,"b":3.5,"nested":{"k1":"ok","k2":1},"z":0}"#
    );

    let oversized = json!({ "big": 1e25 });
    let canon_big = canonical_json_value(&oversized);
    assert_eq!(canon_big["big"].as_f64(), Some(1e25));
}

#[test]
fn canonical_revision_payload_sorts_terms_and_collapses_empty_seo() {
    let p1 = CanonicalRevisionPayload {
        title: "Bài viết".to_owned(),
        content_mode: ContentMode::Editor,
        values: BTreeMap::from([("body".to_owned(), json!("<p>Xin chào</p>"))]),
        builder_document_json: Some(r#"{"sections":[],"schema_version":1}"#.to_owned()),
        term_ids: vec![5, 2, 5, 1],
        primary_category_term_id: Some(2),
        page_layout_key: None,
        custom_template_key: Some(String::new()),
        seo: Some(CanonicalRevisionSeo {
            title: Some("   ".to_owned()),
            description: Some(String::new()),
            og_image_id: None,
            canonical_url: None,
            noindex: false,
        }),
        parent_id: None,
        menu_order: 0,
    };
    let p2 = CanonicalRevisionPayload {
        title: "Bài viết".to_owned(),
        content_mode: ContentMode::Editor,
        values: BTreeMap::from([("body".to_owned(), json!("<p>Xin chào</p>"))]),
        builder_document_json: Some(r#"{"schema_version":1,"sections":[]}"#.to_owned()),
        term_ids: vec![1, 2, 5],
        primary_category_term_id: Some(2),
        page_layout_key: Some(String::new()),
        custom_template_key: None,
        seo: None,
        parent_id: None,
        menu_order: 0,
    };
    assert_eq!(canonical_revision_json(&p1), canonical_revision_json(&p2));
}

#[test]
fn text_diff_handles_vietnamese_unicode_html_tags_and_word_budget() {
    let before = "<p>Chào thế giới OpenDash.</p><p>Đoạn giữ nguyên.</p>";
    let after = "<p>Chào <strong>Việt Nam</strong> OpenDash!</p><p>Đoạn giữ nguyên.</p>";
    let diff = compute_line_word_diff(before, after);
    assert!(!diff.truncated_diff);
    assert_eq!(diff.hunks.len(), 1);
    let first_line = &diff.hunks[0].lines[0];
    assert_eq!(first_line.kind, DiffOpKind::Modified);
    assert!(
        first_line
            .word_spans
            .iter()
            .any(|s| s.kind == DiffOpKind::Added && s.text.contains("<strong>"))
    );

    let long_a = (0..=MAX_DIFF_TOKENS_PER_LINE)
        .map(|i| format!("w{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    let long_b = format!("{long_a} extra");
    let capped = compute_line_word_diff(&long_a, &long_b);
    assert!(capped.truncated_diff);

    let huge_a = (0..=MAX_DIFF_LINES)
        .map(|i| format!("line-{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let huge_b = format!("{huge_a}\nextra");
    let capped_lines = compute_line_word_diff(&huge_a, &huge_b);
    assert!(capped_lines.truncated_diff);
}

fn sample_doc(elems_a: Vec<ElementNode>, elems_b: Vec<ElementNode>) -> BuilderDocument {
    BuilderDocument {
        schema_version: 1,
        sections: vec![SectionNode {
            id: "sec-1".to_owned(),
            responsive: None,
            style: None,
            meta: None,
            children: vec![RowNode {
                id: "row-1".to_owned(),
                responsive: None,
                meta: None,
                children: vec![
                    ColumnNode {
                        id: "col-1".to_owned(),
                        responsive: None,
                        meta: None,
                        children: elems_a,
                    },
                    ColumnNode {
                        id: "col-2".to_owned(),
                        responsive: None,
                        meta: None,
                        children: elems_b,
                    },
                ],
            }],
        }],
    }
}

#[test]
fn block_diff_tracks_added_removed_moved_with_props_modified_and_truncation() {
    let h1 = ElementNode {
        id: "el-h1".to_owned(),
        meta: Some(NodeMeta {
            name: Some("Hero".to_owned()),
            ..Default::default()
        }),
        content: ElementContent::Known(KnownElement::Heading(HeadingProps {
            level: 1,
            text: "Old Hero".to_owned(),
            binding: None,
        })),
    };
    let plugin = ElementNode {
        id: "el-plug".to_owned(),
        meta: None,
        content: ElementContent::Known(KnownElement::PluginBlock(PluginBlockProps {
            block_id: "acme/card".to_owned(),
            values: BTreeMap::from([("title".to_owned(), json!("A"))]),
        })),
    };
    let doc_before = sample_doc(vec![h1.clone()], vec![plugin.clone()]);

    let mut h1_moved = h1;
    h1_moved.content = ElementContent::Known(KnownElement::Heading(HeadingProps {
        level: 2,
        text: "New Hero".to_owned(),
        binding: None,
    }));
    let mut plugin_mod = plugin;
    plugin_mod.content = ElementContent::Known(KnownElement::PluginBlock(PluginBlockProps {
        block_id: "acme/card".to_owned(),
        values: BTreeMap::from([("title".to_owned(), json!("B"))]),
    }));
    let added = ElementNode {
        id: "el-new".to_owned(),
        meta: None,
        content: ElementContent::Known(KnownElement::Heading(HeadingProps {
            level: 3,
            text: "Sub".to_owned(),
            binding: None,
        })),
    };
    let doc_after = sample_doc(vec![added], vec![plugin_mod, h1_moved]);

    let summary = compute_builder_block_diff(Some(&doc_before), Some(&doc_after));
    assert!(!summary.truncated);
    assert!(summary.unchanged_count >= 3); // sec-1, row-1, col-1, col-2
    assert!(summary.items.iter().any(|i| {
        i.kind == BlockDiffKind::Moved
            && i.node_id == "el-h1"
            && i.prop_changes.iter().any(|p| p.prop_path == "props.text")
    }));
    assert!(summary.items.iter().any(|i| {
        i.kind == BlockDiffKind::Modified
            && i.node_id == "el-plug"
            && i.prop_changes.iter().any(|p| p.prop_path == "values.title")
    }));
    assert!(
        summary
            .items
            .iter()
            .any(|i| i.kind == BlockDiffKind::Added && i.node_id == "el-new")
    );

    let many_elems = (0..600)
        .map(|i| ElementNode {
            id: format!("el-{i}"),
            meta: None,
            content: ElementContent::Known(KnownElement::Heading(HeadingProps {
                level: 2,
                text: format!("T{i}"),
                binding: None,
            })),
        })
        .collect();
    let oversized = sample_doc(many_elems, vec![]);
    let truncated_diff = compute_builder_block_diff(Some(&oversized), Some(&doc_after));
    assert!(truncated_diff.truncated);
}

#[test]
fn missing_refs_blocking_ignores_non_blocking_report_fields() {
    let mut report = MissingRefsReport {
        dropped_field_keys: vec!["legacy".to_owned()],
        dropped_block_field_keys: vec![DroppedBlockFieldKey {
            node_id: "n1".to_owned(),
            block_type: "acme/card".to_owned(),
            field_path: "old".to_owned(),
        }],
        non_editable_fields: vec!["admin_note".to_owned()],
        ..Default::default()
    };
    assert!(!report.has_blocking_refs());
    report.missing_pattern_ids.push(99);
    assert!(report.has_blocking_refs());
    report.missing_pattern_ids.clear();
    assert!(!report.has_blocking_refs());
    report.parent_id = Some(42);
    assert!(report.has_blocking_refs());
}
