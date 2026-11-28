use super::*;
use crate::builder::{NodeMeta, NodeVisibility};

#[test]
fn roundtrip_recursive_tree_paired_text_and_comment_boundary_injection() {
    let img_hex = "a".repeat(64);
    let vid_hex = "b".repeat(64);
    let poster_hex = "c".repeat(64);

    let mut hero = bnode(
        "ban-1",
        "core/banner",
        json!({ "bg_image": img_hex }),
        vec![
            bnode(
                "hd-1",
                "core/heading",
                json!({ "level": "h2", "text": "Chào mừng" }),
                vec![],
            ),
            bnode(
                "txt-1",
                "core/text",
                json!({ "content": "<p>Đoạn <b>văn</b></p>" }),
                vec![],
            ),
            bnode(
                "txt-bnd",
                "core/text",
                json!({ "content": "literal <!-- --> --!> trap" }),
                vec![],
            ),
            bnode(
                "vid-1",
                "core/video",
                json!({ "video": vid_hex, "poster": poster_hex }),
                vec![],
            ),
            bnode(
                "pat-1",
                "core/pattern-ref",
                json!({ "pattern_id": "42" }),
                vec![],
            ),
        ],
    );
    hero.meta = Some(NodeMeta {
        name: Some("Hero".into()),
        html_id: Some("hero-id".into()),
        class_name: Some("hero-cls".into()),
        visibility: Some(NodeVisibility {
            hide_mobile: true,
            ..Default::default()
        }),
    });

    let doc = BuilderDocument {
        schema_version: BUILDER_SCHEMA_VERSION,
        blocks: vec![hero],
    };
    let encoded = serialize_builder_document_to_blocks(&doc).unwrap();
    assert!(encoded.contains("<!-- od:core/banner "));
    assert!(encoded.contains("<!-- /od:core/banner -->"));
    assert!(encoded.contains(
        "<!-- od:core/text {\"id\":\"txt-1\"} -->\n<p>Đoạn <b>văn</b></p>\n<!-- /od:core/text -->"
    ));
    assert!(encoded.contains("<!-- od:core/text {\"id\":\"txt-bnd\""));
    assert!(!encoded.contains("literal <!--"));

    let decoded = parse_blocks_to_builder_document(&encoded).unwrap();
    assert_eq!(decoded.document, doc);
    assert!(!decoded.had_recovery);
    assert!(!decoded.had_freeform_wrap);
}

#[test]
fn freeform_chunking_empty_body_and_malformed_recovery_work_cleanly() {
    let empty = parse_blocks_lenient_or_recover("  \r\n  ");
    assert!(empty.document.blocks.is_empty());

    let big = format!("{}\r\n{}", "A".repeat(16_384), "B".repeat(500));
    let wrapped = parse_blocks_lenient_or_recover(&big);
    assert!(wrapped.had_freeform_wrap);
    assert_eq!(wrapped.document.blocks.len(), 2);
    assert_eq!(wrapped.document.blocks[0].block_id, "core/text");

    let broken = "<!-- od:core/banner {\"id\":\"b1\"} --><p>Giữ nguyên</p><!-- od:core/unknown {bad-json} /-->";
    assert!(parse_blocks_to_builder_document(broken).is_err());
    let recovered = parse_blocks_lenient_or_recover(broken);
    assert!(recovered.had_recovery);
    assert_eq!(recovered.document.blocks.len(), 1);
    assert_eq!(recovered.document.blocks[0].id, "b1");
    assert_eq!(recovered.document.blocks[0].children.len(), 2);

    assert_eq!(
        extract_visible_text("Plain fallback text"),
        "Plain fallback text"
    );
}
