use std::collections::BTreeMap;

use serde_json::json;

use crate::builder::{BlockNode, BuilderDocument, NodeMeta};
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

fn block_node(
    id: &str,
    block_id: &str,
    props: serde_json::Value,
    meta: Option<NodeMeta>,
    children: Vec<BlockNode>,
) -> BlockNode {
    let props_map = match props {
        serde_json::Value::Object(m) => m.into_iter().collect(),
        _ => BTreeMap::new(),
    };
    BlockNode {
        id: id.to_owned(),
        block_id: block_id.to_owned(),
        props: props_map,
        meta,
        children,
    }
}

fn sample_doc(elems_a: Vec<BlockNode>, elems_b: Vec<BlockNode>) -> BuilderDocument {
    BuilderDocument {
        schema_version: 1,
        blocks: vec![block_node(
            "sec-1",
            "core/section",
            json!({}),
            None,
            vec![block_node(
                "row-1",
                "core/row",
                json!({}),
                None,
                vec![
                    block_node("col-1", "core/column", json!({}), None, elems_a),
                    block_node("col-2", "core/column", json!({}), None, elems_b),
                ],
            )],
        )],
    }
}

#[test]
fn block_diff_tracks_added_removed_moved_with_props_modified_and_truncation() {
    let h1 = block_node(
        "el-h1",
        "core/heading",
        json!({ "level": "h1", "text": "Old Hero" }),
        Some(NodeMeta {
            name: Some("Hero".to_owned()),
            ..Default::default()
        }),
        vec![],
    );
    let plugin = block_node(
        "el-plug",
        "acme/card",
        json!({ "title": "A" }),
        None,
        vec![],
    );
    let doc_before = sample_doc(vec![h1.clone()], vec![plugin.clone()]);

    let mut h1_moved = h1;
    h1_moved.props = BTreeMap::from([
        ("level".to_owned(), json!("h2")),
        ("text".to_owned(), json!("New Hero")),
    ]);
    let mut plugin_mod = plugin;
    plugin_mod.props = BTreeMap::from([("title".to_owned(), json!("B"))]);
    let added = block_node(
        "el-new",
        "core/heading",
        json!({ "level": "h3", "text": "Sub" }),
        None,
        vec![],
    );
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
            && i.prop_changes.iter().any(|p| p.prop_path == "props.title")
    }));
    assert!(
        summary
            .items
            .iter()
            .any(|i| i.kind == BlockDiffKind::Added && i.node_id == "el-new")
    );

    let many_elems = (0..600)
        .map(|i| {
            block_node(
                &format!("el-{i}"),
                "core/heading",
                json!({ "level": "h2", "text": format!("T{i}") }),
                None,
                vec![],
            )
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
}
