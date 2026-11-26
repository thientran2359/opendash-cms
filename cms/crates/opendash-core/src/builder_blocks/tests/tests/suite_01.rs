use super::*;

#[test]
fn roundtrip_all_known_elements_and_unresolved_without_div_wrappers() {
    let mut unresolved = BTreeMap::new();
    unresolved.insert("type".into(), json!("vendor/custom-widget"));
    unresolved.insert("props".into(), json!({"foo": "bar", "nested": [1, 2]}));

    let doc = BuilderDocument {
        schema_version: BUILDER_SCHEMA_VERSION,
        sections: vec![SectionNode {
            id: "sec-1".into(),
            meta: None,
            style: None,
            responsive: Some(Responsive {
                desktop: Some(ResponsiveStyle {
                    padding_px: Some(24),
                    ..Default::default()
                }),
                tablet: None,
                mobile: Some(ResponsiveStyle {
                    padding_px: Some(12),
                    ..Default::default()
                }),
            }),
            children: vec![RowNode {
                id: "row-1".into(),
                meta: None,
                responsive: Some(Responsive {
                    desktop: Some(ResponsiveStyle {
                        gap_px: Some(16),
                        columns_per_row: Some(2),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                children: vec![ColumnNode {
                    id: "col-1".into(),
                    meta: None,
                    responsive: Some(Responsive {
                        mobile: Some(ResponsiveStyle {
                            text_align: Some(TextAlign::Center),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    children: vec![
                        el(
                            "el-heading",
                            KnownElement::Heading(HeadingProps {
                                level: 2,
                                text: "Title with -- and --> and <script>".into(),
                                binding: Some(ElementBinding {
                                    type_key: "post".into(),
                                    entry_slug: "hello".into(),
                                    entry_locale: None,
                                    source: BindingSource::Title,
                                    target: BindingTarget::Text,
                                }),
                            }),
                        ),
                        el(
                            "el-text",
                            KnownElement::Text(TextProps {
                                text: "  Line 1\nLine 2  ".into(),
                                ..Default::default()
                            }),
                        ),
                        el(
                            "el-image",
                            KnownElement::Image(ImageProps {
                                media_id: "a".repeat(64),
                                alt: "Alt".into(),
                                binding: None,
                            }),
                        ),
                        el(
                            "el-video",
                            KnownElement::Video(VideoProps {
                                media_id: "b".repeat(64),
                                poster_image_id: Some("c".repeat(64)),
                                controls: true,
                                muted: true,
                            }),
                        ),
                        el(
                            "el-button",
                            KnownElement::Button(ButtonProps {
                                label: "Click".into(),
                                url: "/contact".into(),
                                ..Default::default()
                            }),
                        ),
                        el(
                            "el-list",
                            KnownElement::List(ListProps {
                                type_key: "post".into(),
                                category_id: None,
                                tag_id: None,
                                taxonomy_key: Some("topic".into()),
                                term_id: Some(7),
                                include_descendants: true,
                                sort: ListSort::IdAsc,
                                page_size: 4,
                            }),
                        ),
                        el(
                            "el-form",
                            KnownElement::ContactForm(ContactFormProps {
                                form_id: 3,
                                submit_label: "Send".into(),
                            }),
                        ),
                        el("el-slot", KnownElement::ContentSlot(ContentSlotProps {})),
                        el(
                            "el-pattern",
                            KnownElement::PatternRef(PatternRefProps { pattern_id: 9 }),
                        ),
                        el(
                            "el-part",
                            KnownElement::TemplatePart(TemplatePartProps {
                                part_key: "header".into(),
                            }),
                        ),
                        el(
                            "el-plugin",
                            KnownElement::PluginBlock(PluginBlockProps {
                                block_id: "demo-plugin/pricing".into(),
                                values: BTreeMap::from([("tier".into(), json!("pro"))]),
                            }),
                        ),
                        el(
                            "el-legacy",
                            KnownElement::Legacy(LegacyBlockProps {
                                block_id: "ocean/hero".into(),
                                values: BTreeMap::from([("title".into(), "Ocean".into())]),
                            }),
                        ),
                        ElementNode {
                            id: "el-unresolved".into(),
                            meta: None,
                            content: ElementContent::Unresolved(unresolved),
                        },
                    ],
                }],
            }],
        }],
    };

    let blocks = serialize_builder_document_to_blocks(&doc).unwrap();
    assert!(!blocks.contains("<div"), "must not emit wrapper <div>s");
    assert!(blocks.contains("<!-- od:section "));
    assert!(blocks.contains("<!-- /od:section -->"));
    // Escaped `--` and `-->` inside heading JSON
    assert!(!blocks.contains("Title with --"));
    assert!(blocks.contains("\\u002d\\u002d"));
    assert!(blocks.contains("\\u003cscript\\u003e"));

    let parsed = parse_blocks_to_builder_document(&blocks).unwrap();
    assert!(!parsed.had_freeform_wrap);
    assert!(!parsed.had_recovery);
    assert!(parsed.diagnostics.is_empty());
    assert_eq!(parsed.document, doc);

    // Validate against RegistryCatalog (keeping legacy and unresolved preserved against `doc`)
    let pricing_schema: crate::block_schema::BlockSchemaV1 = serde_json::from_value(json!({
        "schema_version": 1,
        "id": "demo-plugin/pricing",
        "title": "Pricing",
        "category": "plugin",
        "render": "render.html",
        "fields": [
            { "type": "text", "key": "tier", "label": "Tier" }
        ]
    }))
    .unwrap();
    let mut catalog = crate::registry::RegistryCatalog::new();
    catalog
        .register(
            "demo-plugin/pricing",
            ResourceOwner::Plugin("demo-plugin".into()),
            RegisteredBlockSchema {
                schema: pricing_schema,
            },
        )
        .unwrap();
    parsed
        .document
        .validate_for_save_with_schemas(&catalog, Some(&doc))
        .unwrap();
}

#[test]
fn text_comment_boundary_injection_and_exact_whitespace_roundtrip() {
    let doc = BuilderDocument {
        schema_version: BUILDER_SCHEMA_VERSION,
        sections: vec![SectionNode {
            id: "sec-1".into(),
            meta: None,
            responsive: None,
            style: None,
            children: vec![RowNode {
                id: "row-1".into(),
                meta: None,
                responsive: None,
                children: vec![ColumnNode {
                    id: "col-1".into(),
                    meta: None,
                    responsive: None,
                    children: vec![
                        el(
                            "el-empty",
                            KnownElement::Text(TextProps {
                                text: String::new(),
                                ..Default::default()
                            }),
                        ),
                        el(
                            "el-spaces",
                            KnownElement::Text(TextProps {
                                text: "\n  leading and trailing \n".into(),
                                ..Default::default()
                            }),
                        ),
                        el(
                            "el-comment-break",
                            KnownElement::Text(TextProps {
                                text: "Try <!-- and --> and --!> and --->".into(),
                                ..Default::default()
                            }),
                        ),
                    ],
                }],
            }],
        }],
    };

    let blocks = serialize_builder_document_to_blocks(&doc).unwrap();
    // Self-closing form used only for `el-comment-break`
    assert!(blocks.contains("<!-- od:text {\"id\":\"el-comment-break\""));
    assert!(!blocks.contains("Try <!-- and -->"));
    let parsed = parse_blocks_to_builder_document(&blocks).unwrap();
    assert_eq!(parsed.document, doc);
}

#[test]
fn freeform_over_16k_with_crlf_auto_wraps_into_valid_text_chunks() {
    let line = "Dòng văn bản tiếng Việt dài để kiểm tra chia đoạn.\r\n";
    let raw = line.repeat(500); // > 25,000 chars
    assert!(raw.chars().count() > 16_384);

    let parsed = parse_blocks_to_builder_document(&raw).unwrap();
    assert!(parsed.had_freeform_wrap);
    assert!(!parsed.had_recovery);
    assert_eq!(parsed.document.sections.len(), 1);
    let elements = &parsed.document.sections[0].children[0].children[0].children;
    assert!(elements.len() >= 2);
    let mut combined = String::new();
    for node in elements {
        let ElementContent::Known(KnownElement::Text(t)) = &node.content else {
            panic!("expected text node");
        };
        assert!(t.text.chars().count() <= 16_384);
        assert!(!t.text.contains('\r'));
        combined.push_str(&t.text);
    }
    assert_eq!(combined, raw.replace("\r\n", "\n"));
    parsed
        .document
        .validate_for_save_with_schemas(&crate::registry::RegistryCatalog::new(), None)
        .unwrap();
}

#[test]
fn empty_or_whitespace_only_body_parses_to_empty_document() {
    for raw in ["", "   \n\t\r\n  "] {
        let parsed = parse_blocks_to_builder_document(raw).unwrap();
        assert_eq!(parsed.document, BuilderDocument::empty());
        assert!(!parsed.had_freeform_wrap);
        assert!(!parsed.had_recovery);
    }
}

#[test]
fn malformed_comments_fail_strict_and_recover_losslessly_lenient() {
    let img_id = "d".repeat(64);
    let broken = format!(
        "<!-- od:section {{\"id\":\"sec-1\"}} -->\n\
         <!-- od:row {{\"id\":\"row-1\"}} -->\n\
         <!-- od:column {{\"id\":\"col-1\"}} -->\n\
         <!-- od:image {{\"id\":\"img-ok\",\"media_id\":\"{img_id}\",\"alt\":\"ok\"}} /-->\n\
         <!-- od:heading {{\"id\":\"bad-json\", broken -->\n\
         <!-- od:text {{\"id\":\"txt-unclosed\"}} -->\n\
         Important text preserved"
    );
    assert!(parse_blocks_to_builder_document(&broken).is_err());

    let recovered = parse_blocks_lenient_or_recover(&broken);
    assert!(recovered.had_recovery);
    assert!(!recovered.diagnostics.is_empty());
    assert_eq!(recovered.diagnostics[0].code, "malformed_comment_recovered");
    let visible =
        extract_visible_text(&serialize_builder_document_to_blocks(&recovered.document).unwrap());
    assert!(visible.contains("Important text preserved"));
    assert!(visible.contains("bad-json"));

    // Ref extractor must keep extracting valid refs alongside malformed siblings.
    let refs = extract_block_refs_lenient(&broken);
    assert!(refs.image_ids.contains(&img_id));
}

#[test]
fn extract_visible_text_handles_plain_and_block_content() {
    assert_eq!(extract_visible_text("Plain text"), "Plain text");
    let doc = BuilderDocument {
        schema_version: BUILDER_SCHEMA_VERSION,
        sections: vec![SectionNode {
            id: "s1".into(),
            meta: None,
            responsive: None,
            style: None,
            children: vec![RowNode {
                id: "r1".into(),
                meta: None,
                responsive: None,
                children: vec![ColumnNode {
                    id: "c1".into(),
                    meta: None,
                    responsive: None,
                    children: vec![
                        el(
                            "h1",
                            KnownElement::Heading(HeadingProps {
                                level: 2,
                                text: "Heading One".into(),
                                binding: None,
                            }),
                        ),
                        el(
                            "t1",
                            KnownElement::Text(TextProps {
                                text: "Paragraph One".into(),
                                ..Default::default()
                            }),
                        ),
                        el(
                            "b1",
                            KnownElement::Button(ButtonProps {
                                label: "Read More".into(),
                                url: "/more".into(),
                                ..Default::default()
                            }),
                        ),
                    ],
                }],
            }],
        }],
    };
    let blocks = serialize_builder_document_to_blocks(&doc).unwrap();
    assert_eq!(
        extract_visible_text(&blocks),
        "Heading One\n\nParagraph One\n\nRead More"
    );
}
