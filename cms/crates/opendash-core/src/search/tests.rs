use std::collections::BTreeMap;

use serde_json::json;

use crate::{
    block_schema::{BlockSchemaV1, RegisteredBlockSchema},
    builder::{
        BUILDER_SCHEMA_VERSION, BuilderDocument, ColumnNode, ElementContent, ElementNode,
        HeadingProps, KnownElement, PluginBlockProps, RowNode, SectionNode, TextProps,
    },
    builder_blocks::extract_document_visible_text,
    content::{
        AttachedFieldGroupSnapshot, ContentTypeDefinition, ContentTypeSettings, EditorRegion,
        EffectiveContentSchema, EffectiveFieldEntry, EffectiveFieldStatus, FieldGroupDomain,
        FieldGroupPresentation, FieldGroupStatus, LocationRuleGroups,
    },
    field_schema::{
        FieldAcl, FieldDeclarationState, FieldRole, FieldSpec, FieldType, MediaFieldKind,
        PublicExposure,
    },
    registry::{DeclarationState, OwnedDescriptor, ProviderStatus, RegistryCatalog, ResourceOwner},
};

use super::*;

#[test]
fn folds_vietnamese_stroke_and_truncates_on_utf8_boundaries() {
    assert_eq!(
        fold_vietnamese_stroke("Đường dẫn tĩnh Đắk Lắk"),
        "Dường dẫn tĩnh Dắk Lắk"
    );
}

#[test]
fn folds_vietnamese_stroke_preserves_other_chars() {
    assert_eq!(
        fold_vietnamese_stroke("Định hướng & đường đi"),
        "Dịnh hướng & dường di"
    );
    assert_eq!(fold_vietnamese_stroke("Plain ASCII"), "Plain ASCII");

    let text = "Kiến trúc";
    // "Ki" is 2 bytes, 'ế' is 3 bytes (byte offsets 2..5)
    assert_eq!(truncate_utf8_bytes(text, 4), "Ki");
    assert_eq!(truncate_utf8_bytes(text, 100), "Kiến trúc");
}

#[test]
fn sanitizes_search_queries_and_neutralizes_fts_operators() {
    let q = sanitize_search_query("  kiến\ttrúc  OR NOT NEAR  đường  ").unwrap();
    assert!(!q.is_empty);
    assert_eq!(q.display_query, "kiến trúc OR NOT NEAR đường");
    assert_eq!(q.tokens, vec!["kiến", "trúc", "or", "not", "near", "dường"]);
    assert_eq!(
        q.fts_match_expr.as_deref(),
        Some("\"kiến\" \"trúc\" \"or\" \"not\" \"near\" \"dường\"*")
    );

    let short_last = sanitize_search_query("a").unwrap();
    assert_eq!(short_last.fts_match_expr.as_deref(), Some("\"a\""));

    let empty = sanitize_search_query("  \n\t ").unwrap();
    assert!(empty.is_empty);
    assert_eq!(empty.fts_match_expr, None);

    let punct = sanitize_search_query("!!! --- ??? \"():*\"").unwrap();
    assert!(!punct.is_empty);
    assert!(punct.tokens.is_empty());
    assert_eq!(punct.fts_match_expr, None);

    let max_16 =
        sanitize_search_query("t1 t2 t3 t4 t5 t6 t7 t8 t9 t10 t11 t12 t13 t14 t15 t16 t17 t18")
            .unwrap();
    assert_eq!(max_16.tokens.len(), MAX_SEARCH_TOKENS);

    let too_long_chars = "a".repeat(MAX_SEARCH_QUERY_CHARS + 1);
    assert_eq!(
        sanitize_search_query(&too_long_chars),
        Err(SearchQueryError::TooLong)
    );
}

fn spec_with_acl(
    field_type: FieldType,
    key: &str,
    label: &str,
    min_view_role: FieldRole,
    public_exposure: PublicExposure,
) -> FieldSpec {
    let mut spec = FieldSpec::new(field_type, key, label);
    spec.acl = Some(FieldAcl::canonical(
        min_view_role,
        min_view_role,
        public_exposure,
    ));
    spec
}

fn sample_schema_and_blocks() -> (
    EffectiveContentSchema,
    RegistryCatalog<RegisteredBlockSchema>,
) {
    let body_def = spec_with_acl(
        FieldType::Richtext,
        "body",
        "Body",
        FieldRole::Editor,
        PublicExposure::Public,
    );
    let summary_def = spec_with_acl(
        FieldType::Textarea,
        "summary",
        "Summary",
        FieldRole::Editor,
        PublicExposure::Public,
    );
    let priv_def = spec_with_acl(
        FieldType::Text,
        "internal_note",
        "Secret",
        FieldRole::Editor,
        PublicExposure::Private,
    );
    let admin_def = spec_with_acl(
        FieldType::Text,
        "admin_only",
        "Admin",
        FieldRole::Admin,
        PublicExposure::Private,
    );
    let mut withdrawn_def = spec_with_acl(
        FieldType::Text,
        "retired_note",
        "Retired",
        FieldRole::Editor,
        PublicExposure::Public,
    );
    withdrawn_def.declaration_state = Some(FieldDeclarationState::Withdrawn);

    let mut img_def = spec_with_acl(
        FieldType::Media,
        "hero",
        "Hero",
        FieldRole::Editor,
        PublicExposure::Public,
    );
    img_def.media_kind = Some(MediaFieldKind::Image);

    let secret_def = spec_with_acl(
        FieldType::Secret,
        "api_token",
        "API Token",
        FieldRole::Editor,
        PublicExposure::Public,
    );
    let url_def = spec_with_acl(
        FieldType::Url,
        "ref_url",
        "Ref URL",
        FieldRole::Editor,
        PublicExposure::Public,
    );

    let mut group_def = spec_with_acl(
        FieldType::Group,
        "meta_group",
        "Meta Group",
        FieldRole::Editor,
        PublicExposure::Public,
    );
    group_def.fields = vec![
        spec_with_acl(
            FieldType::Text,
            "tagline",
            "Tagline",
            FieldRole::Editor,
            PublicExposure::Public,
        ),
        spec_with_acl(
            FieldType::Text,
            "group_private",
            "Group Private",
            FieldRole::Editor,
            PublicExposure::Private,
        ),
        spec_with_acl(
            FieldType::Url,
            "group_url",
            "Group URL",
            FieldRole::Editor,
            PublicExposure::Public,
        ),
    ];

    let mut repeater_def = spec_with_acl(
        FieldType::Repeater,
        "highlights",
        "Highlights",
        FieldRole::Editor,
        PublicExposure::Public,
    );
    repeater_def.fields = vec![
        spec_with_acl(
            FieldType::Richtext,
            "caption",
            "Caption",
            FieldRole::Editor,
            PublicExposure::Public,
        ),
        spec_with_acl(
            FieldType::Secret,
            "item_secret",
            "Item Secret",
            FieldRole::Editor,
            PublicExposure::Public,
        ),
    ];

    let base_ct = ContentTypeDefinition {
        key: "post".into(),
        label: "Bài viết".into(),
        singular_label: None,
        description: None,
        settings: ContentTypeSettings::builtin_publication(),
    };
    let mut effective_fields = BTreeMap::new();
    for def in [
        summary_def.clone(),
        body_def.clone(),
        priv_def.clone(),
        admin_def.clone(),
        img_def.clone(),
        secret_def.clone(),
        url_def.clone(),
        group_def.clone(),
        repeater_def.clone(),
    ] {
        effective_fields.insert(
            def.key.clone(),
            EffectiveFieldEntry {
                definition: def,
                owner: ResourceOwner::Core,
                source_id: "post".into(),
                effective_status: EffectiveFieldStatus::Active,
            },
        );
    }
    effective_fields.insert(
        withdrawn_def.key.clone(),
        EffectiveFieldEntry {
            definition: withdrawn_def.clone(),
            owner: ResourceOwner::User,
            source_id: "user:fg-1".into(),
            effective_status: EffectiveFieldStatus::Withdrawn,
        },
    );

    let eff = EffectiveContentSchema {
        base: OwnedDescriptor {
            owner: ResourceOwner::Core,
            status: ProviderStatus::Active,
            declaration_state: DeclarationState::Declared,
            descriptor: base_ct,
        },
        base_version: 1,
        attached_groups: vec![AttachedFieldGroupSnapshot {
            group_id: "user:fg-1".into(),
            owner: ResourceOwner::User,
            local_key: "fg-1".into(),
            title: "Meta".into(),
            description: String::new(),
            domain: FieldGroupDomain::Content,
            location: LocationRuleGroups::default(),
            presentation: FieldGroupPresentation {
                region: EditorRegion::MainBottom,
                order: 60,
                collapsible: false,
                ..FieldGroupPresentation::default()
            },
            active: true,
            status: FieldGroupStatus::Active,
            association_state: DeclarationState::Declared,
            version: 1,
            fields: vec![
                summary_def,
                body_def,
                img_def,
                secret_def,
                url_def,
                priv_def,
                admin_def,
                withdrawn_def,
                group_def,
                repeater_def,
            ],
        }],
        effective_fields,
        ui_fields: Vec::new(),
        schema_token: "tok".into(),
    };

    let block_schema: BlockSchemaV1 = serde_json::from_value(json!({
        "schema_version": 1,
        "id": "core/card",
        "title": "Card",
        "category": "content",
        "render": "render.html",
        "fields": [
            { "type": "text", "key": "headline", "label": "Headline" },
            { "type": "richtext", "key": "desc", "label": "Desc" },
            { "type": "url", "key": "ext", "label": "Ext" }
        ]
    }))
    .unwrap();
    let mut blocks = RegistryCatalog::new();
    blocks
        .register(
            "core/card",
            ResourceOwner::Core,
            RegisteredBlockSchema {
                schema: block_schema,
            },
        )
        .unwrap();
    (eff, blocks)
}

#[test]
fn extracts_acl_filtered_fields_and_builder_comment_blocks() {
    let (eff, blocks) = sample_schema_and_blocks();
    let mut card_vals = BTreeMap::new();
    card_vals.insert("headline".into(), json!("Thẻ nổi bật"));
    card_vals.insert(
        "desc".into(),
        json!("<p>Chi tiết</p><script>evil()</script>"),
    );
    card_vals.insert("ext".into(), json!("https://hidden-url.example/secret"));

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
                        ElementNode {
                            id: "h1".into(),
                            meta: None,
                            content: ElementContent::Known(KnownElement::Heading(HeadingProps {
                                level: 2,
                                text: "Tiêu đề Builder".into(),
                                binding: None,
                            })),
                        },
                        ElementNode {
                            id: "t1".into(),
                            meta: None,
                            content: ElementContent::Known(KnownElement::Text(TextProps {
                                text: "<p>Đoạn văn</p><script>bad()</script>".into(),
                                rich: true,
                                binding: None,
                            })),
                        },
                        ElementNode {
                            id: "p1".into(),
                            meta: None,
                            content: ElementContent::Known(KnownElement::PluginBlock(
                                PluginBlockProps {
                                    block_id: "core/card".into(),
                                    values: card_vals,
                                },
                            )),
                        },
                    ],
                }],
            }],
        }],
    };
    let comment_blocks = crate::builder_blocks::serialize_builder_document_to_blocks(&doc).unwrap();
    assert!(extract_document_visible_text(&doc, &blocks).contains("Thẻ nổi bật"));

    let mut values = BTreeMap::new();
    values.insert("summary".into(), json!("Đường dẫn tổng quan"));
    values.insert("body".into(), json!(comment_blocks));
    values.insert("internal_note".into(), json!("secret-private-token"));
    values.insert("admin_only".into(), json!("admin-secret-token"));
    values.insert("retired_note".into(), json!("retired-token"));
    values.insert("hero".into(), json!("image-token-excluded"));
    values.insert("api_token".into(), json!("secret-field-excluded"));
    values.insert("ref_url".into(), json!("https://url-excluded.example"));
    values.insert(
        "meta_group".into(),
        json!({
            "tagline": "Kiến trúc mô-đun",
            "group_private": "nested-private-excluded",
            "group_url": "https://nested-url-excluded.example"
        }),
    );
    values.insert(
        "highlights".into(),
        json!([
            {
                "caption": "<p>Điểm nhấn repeater</p><script>x()</script>",
                "item_secret": "repeater-secret-excluded"
            }
        ]),
    );

    let doc_out = extract_searchable_document(
        "Đồ án Kiến trúc",
        &values,
        &eff,
        None,
        &blocks,
        &["Đô thị".into()],
    );
    assert!(doc_out.title_folded.contains("Dồ án"));
    assert!(doc_out.summary_folded.contains("Dường dẫn"));
    assert!(doc_out.body_folded.contains("Tiêu dề Builder"));
    assert!(doc_out.body_folded.contains("Doạn văn"));
    assert!(doc_out.body_folded.contains("Thẻ nổi bật"));
    assert!(doc_out.body_folded.contains("Kiến trúc mô-dun"));
    assert!(doc_out.body_folded.contains("Diểm nhấn repeater"));
    assert!(!doc_out.body_folded.contains("evil()"));
    assert!(!doc_out.body_folded.contains("bad()"));
    assert!(!doc_out.body_folded.contains("hidden-url"));
    assert!(!doc_out.body_folded.contains("secret-private-token"));
    assert!(!doc_out.body_folded.contains("admin-secret-token"));
    assert!(!doc_out.body_folded.contains("retired-token"));
    assert!(!doc_out.body_folded.contains("image-token-excluded"));
    assert!(!doc_out.body_folded.contains("secret-field-excluded"));
    assert!(!doc_out.body_folded.contains("url-excluded"));
    assert!(!doc_out.body_folded.contains("nested-private-excluded"));
    assert!(!doc_out.body_folded.contains("nested-url-excluded"));
    assert!(!doc_out.body_folded.contains("repeater-secret-excluded"));
    assert!(doc_out.terms_folded.contains("Dô thị"));
    assert!(doc_out.snippet_source_text.contains("Đường dẫn tổng quan"));
}

#[test]
fn builds_structured_snippets_with_vietnamese_folding_and_fallback() {
    let source = "Giới thiệu kiến trúc Đường dẫn tĩnh tốc độ cao và ổn định trong OpenDash.";
    let segments = build_structured_snippet(source, &["kien".into(), "duong".into()], 220);
    assert!(segments.iter().any(|s| s.matched && s.text == "kiến"));
    assert!(segments.iter().any(|s| s.matched && s.text == "Đường"));
    let joined: String = segments.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(joined, source);

    // Fallback excerpt when matched only on title/terms
    let fallback =
        build_structured_snippet("Tóm tắt kỹ thuật hệ thống CMS.", &["rust".into()], 220);
    assert_eq!(
        fallback,
        vec![SnippetSegment {
            text: "Tóm tắt kỹ thuật hệ thống CMS.".into(),
            matched: false,
        }]
    );

    assert!(build_structured_snippet("", &["rust".into()], 220).is_empty());
}
