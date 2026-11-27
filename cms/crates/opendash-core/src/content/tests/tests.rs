use super::*;
use serde_json::json;

#[test]
fn c4_provider_legacy_manifest_remains_compatible() {
    let legacy: CntDeclarations = serde_json::from_value(json!({
        "content_types": [{"key": "review", "label": "Reviews", "taxonomies": ["category", "tag"]}]
    }))
    .unwrap();
    legacy.validate().unwrap();
    let serialized = serde_json::to_value(&legacy).unwrap();
    assert!(serialized.get("taxonomy_definitions").is_none());
    assert!(serialized["content_types"][0].get("rewrite_base").is_none());
    assert!(serialized["content_types"][0].get("settings").is_none());
    assert!(CntDeclarations::default().is_empty());
}

#[test]
fn c4_provider_can_declare_taxonomy_and_rewrite_defaults() {
    let payload = json!({
        "taxonomy_definitions": [
            {"key":"genre","label":"Genre","hierarchical":true,"public":false,"rewrite_base":"library/genres"},
            {"key":"series","label":"Series"}
        ],
        "content_types": [{
            "key":"review","label":"Reviews","taxonomies":["genre","category","site_taxonomy"],
            "rewrite_base":"library/reviews",
            "settings":{"public":true,"has_archive":true,"publication":"draft_publish","supports":["editor"]}
        }],
        "taxonomy_eligibility": [{"content_type":"review","taxonomies":["series","tag"]}]
    });
    let input: CntDeclarations = serde_json::from_value(payload).unwrap();
    input.validate().unwrap();
    let actual = serde_json::to_value(&input).unwrap();
    assert_eq!(actual["taxonomy_definitions"][0]["public"], false);
    assert_eq!(actual["taxonomy_definitions"][1]["hierarchical"], false);
    assert_eq!(actual["taxonomy_definitions"][1]["public"], true);
    assert!(
        actual["taxonomy_definitions"][1]
            .get("rewrite_base")
            .is_none()
    );
    assert_eq!(
        actual["content_types"][0]["rewrite_base"],
        "library/reviews"
    );
    assert_eq!(
        serde_json::from_value::<CntDeclarations>(actual).unwrap(),
        input
    );

    let tax_only: CntDeclarations = serde_json::from_value(json!({
        "taxonomy_definitions":[{"key":"genre","label":"Genre"}]
    }))
    .unwrap();
    assert!(!tax_only.is_empty());
}

#[test]
fn c4_provider_rejects_duplicate_or_core_owned_taxonomy_declarations() {
    for key in [
        "category",
        "tag",
        "post",
        "page",
        "section",
        "seo",
        "Invalid-Key",
    ] {
        let declarations: CntDeclarations = serde_json::from_value(json!({
            "taxonomy_definitions":[{"key":key,"label":"A"}]
        }))
        .unwrap();
        assert!(
            declarations.validate().is_err(),
            "reserved or invalid taxonomy: {key}"
        );
    }
    let duplicate: CntDeclarations = serde_json::from_value(json!({
        "taxonomy_definitions":[{"key":"genre","label":"Genre"}, {"key":"genre","label":"Other"}]
    }))
    .unwrap();
    assert!(duplicate.validate().is_err());

    for path in [
        json!({"taxonomy_definitions":[{"key":"genre","label":"Genre","surprise":3}]}),
        json!({"taxonomy_definitions":[{"key":"genre","label":"Genre","owner":{"kind":"plugin","id":"other"}}]}),
        json!({"content_types":[{"key":"review","label":"Reviews","rewrite_base":"a","surprise":3}]}),
        json!({"taxonomy_definitions":[{"key":"genre","label":"Genre","rewrite_base":7}]}),
    ] {
        assert!(serde_json::from_value::<CntDeclarations>(path).is_err());
    }
}

#[test]
fn c4_provider_rejects_duplicate_eligibility_rows_claiming_same_taxonomy() {
    let duplicate: CntDeclarations = serde_json::from_value(json!({
        "taxonomy_eligibility": [
            {"content_type":"review","taxonomies":["genre"]},
            {"content_type":"review","taxonomies":["genre"]}
        ]
    }))
    .unwrap();
    assert!(duplicate.validate().is_err());
}

#[test]
fn c4_provider_rejects_malformed_rewrite_bases_and_associations() {
    for base in [
        "",
        "/reviews",
        "reviews/",
        "a//b",
        "a/b/c/d",
        "../x",
        "X",
        "review_name",
        "news?x=1",
        "a/%term%",
    ] {
        let cpt: CntDeclarations = serde_json::from_value(json!({
            "content_types":[{"key":"review","label":"Reviews","rewrite_base":base}]
        }))
        .unwrap();
        assert!(cpt.validate().is_err(), "invalid CPT rewrite base: {base}");
        let tax: CntDeclarations = serde_json::from_value(json!({
            "taxonomy_definitions":[{"key":"genre","label":"Genre","rewrite_base":base}]
        }))
        .unwrap();
        assert!(
            tax.validate().is_err(),
            "invalid taxonomy rewrite base: {base}"
        );
    }
    for taxonomies in [
        json!(["genre", "genre"]),
        json!(["bad-key"]),
        json!([""]),
        json!(["category", "category"]),
    ] {
        let cpt: CntDeclarations = serde_json::from_value(json!({
            "content_types":[{"key":"review","label":"Reviews","taxonomies":taxonomies}]
        }))
        .unwrap();
        assert!(
            cpt.validate().is_err(),
            "invalid CPT association: {taxonomies}"
        );
        let eligibility: CntDeclarations = serde_json::from_value(json!({
            "taxonomy_eligibility":[{"content_type":"review","taxonomies":taxonomies}]
        }))
        .unwrap();
        assert!(
            eligibility.validate().is_err(),
            "invalid eligibility association: {taxonomies}"
        );
    }
    let builtin: CntDeclarations = serde_json::from_value(json!({
        "content_types":[{
            "key":"post","label":"Post","rewrite_base":"blog",
            "settings":{"public":true,"has_archive":true,"publication":"draft_publish","supports":["editor"]}
        }]
    })).unwrap();
    assert!(
        builtin.validate().is_err(),
        "post URLs belong to global C3 policy"
    );
}

#[test]
fn accepts_custom_post_type_keys() {
    assert!(valid_content_type_key("post"));
    assert!(valid_content_type_key("portfolio_item2"));
}

#[test]
fn rejects_unstable_or_invalid_keys() {
    assert!(!valid_content_type_key(""));
    assert!(!valid_content_type_key("2post"));
    assert!(!valid_content_type_key("Post"));
    assert!(!valid_content_type_key("post/type"));
}

fn sample_schema_with_fields(
    fields: Vec<crate::field_schema::FieldSpec>,
) -> EffectiveContentSchema {
    let base = OwnedDescriptor {
        owner: ResourceOwner::User,
        status: ProviderStatus::Active,
        declaration_state: DeclarationState::Declared,
        descriptor: ContentTypeDefinition {
            key: "portfolio_item".into(),
            label: "Dự án".into(),
            singular_label: None,
            description: None,
            settings: ContentTypeSettings::default(),
        },
    };
    let group = AttachedFieldGroupSnapshot {
        group_id: "user:portfolio_fields".into(),
        owner: ResourceOwner::User,
        local_key: "portfolio_fields".into(),
        title: "Thông tin dự án".into(),
        description: String::new(),
        domain: FieldGroupDomain::Content,
        location: vec![vec![LocationRule {
            param: LocationParam::ContentType,
            op: LocationOp::Eq,
            value: "portfolio_item".into(),
        }]],
        presentation: FieldGroupPresentation::default(),
        active: true,
        fields,
        status: FieldGroupStatus::Active,
        association_state: DeclarationState::Declared,
        version: 1,
    };
    EffectiveContentSchema::assemble(base, 1, vec![group]).unwrap()
}

fn sample_fields() -> Vec<crate::field_schema::FieldSpec> {
    let summary = crate::field_schema::FieldSpec::new(
        crate::field_schema::FieldType::Textarea,
        "summary",
        "Tóm tắt",
    );
    let mut status = crate::field_schema::FieldSpec::new(
        crate::field_schema::FieldType::Select,
        "status_code",
        "Trạng thái",
    );
    status.required = true;
    status.options = vec![
        crate::field_schema::SelectOption {
            value: "planned".into(),
            label: "Dự kiến".into(),
            icon: None,
        },
        crate::field_schema::SelectOption {
            value: "published".into(),
            label: "Đã đăng".into(),
            icon: None,
        },
    ];
    vec![summary, status]
}

fn sample_schema() -> EffectiveContentSchema {
    sample_schema_with_fields(sample_fields())
}

#[test]
fn content_schema_round_trips_and_validates() {
    let schema = sample_schema();
    assert!(schema.base.descriptor.validate().is_ok());
    let json = serde_json::to_string(&schema.base.descriptor).unwrap();
    let decoded: ContentTypeDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, schema.base.descriptor);
    assert!(decoded.validate().is_ok());
}

#[test]
fn context_aware_field_definition_input_normalization() {
    let base_missing: crate::field_schema::FieldSpec = serde_json::from_value(json!({
        "key": "subtitle",
        "label": "Phụ đề",
        "type": "text"
    }))
    .unwrap();
    assert_eq!(base_missing.effective_acl().2, PublicExposure::Public);

    let ext_explicit_pub: crate::field_schema::FieldSpec = serde_json::from_value(json!({
        "key": "hero_badge",
        "label": "Nhãn Hero",
        "type": "text",
        "acl": { "public_exposure": "public" }
    }))
    .unwrap();
    assert_eq!(ext_explicit_pub.effective_acl().2, PublicExposure::Public);
}

#[test]
fn supports_all_nine_v1_field_kinds() {
    let kinds = [
        crate::field_schema::FieldType::Text,
        crate::field_schema::FieldType::Textarea,
        crate::field_schema::FieldType::Richtext,
        crate::field_schema::FieldType::Number,
        crate::field_schema::FieldType::Boolean,
        crate::field_schema::FieldType::Date,
        crate::field_schema::FieldType::Select,
        crate::field_schema::FieldType::Media,
        crate::field_schema::FieldType::Gallery,
    ];
    let fields = sample_fields();
    for kind in kinds {
        let mut field = fields[0].clone();
        field.field_type = kind;
        if kind == crate::field_schema::FieldType::Select {
            field.options = fields[1].options.clone();
        }
        assert!(
            crate::field_schema::validate_field_specs(
                &[field],
                crate::field_schema::FieldContext::Content
            )
            .is_ok()
        );
    }
}

#[test]
fn rejects_duplicate_keys_and_invalid_select_options() {
    let mut fields = sample_fields();
    fields.push(fields[0].clone());
    assert!(
        crate::field_schema::validate_field_specs(
            &fields,
            crate::field_schema::FieldContext::Content
        )
        .is_err()
    );

    let mut fields = sample_fields();
    fields[1].options[1].value = "planned".into();
    assert!(
        crate::field_schema::validate_field_specs(
            &fields,
            crate::field_schema::FieldContext::Content
        )
        .is_err()
    );

    fields[1].options.clear();
    assert!(
        crate::field_schema::validate_field_specs(
            &fields,
            crate::field_schema::FieldContext::Content
        )
        .is_err()
    );

    fields[0].options.push(crate::field_schema::SelectOption {
        value: "wrong".into(),
        label: "Sai loại".into(),
        icon: None,
    });
    assert!(
        crate::field_schema::validate_field_specs(
            &fields,
            crate::field_schema::FieldContext::Content
        )
        .is_err()
    );
}

#[test]
fn rejects_invalid_identifiers_and_labels() {
    let mut def = ContentTypeDefinition {
        key: "Invalid-Type".into(),
        label: "Bài viết".into(),
        singular_label: None,
        description: None,
        settings: ContentTypeSettings::default(),
    };
    assert!(def.validate().is_err());

    def.key = "post".into();
    def.label = "   ".into();
    assert!(def.validate().is_err());

    let mut fields = sample_fields();
    fields[0].key = "bad-name".into();
    assert!(
        crate::field_schema::validate_field_specs(
            &fields,
            crate::field_schema::FieldContext::Content
        )
        .is_err()
    );
}

fn entry() -> ContentEntryInput {
    ContentEntryInput {
        title: "Dự án đầu tiên".into(),
        slug: "du-an-dau-tien".into(),
        locale: None,
        values: BTreeMap::from([
            ("summary".into(), json!("Nội dung")),
            ("status_code".into(), json!("planned")),
        ]),
        term_ids: None,
    }
}

fn entry_ctx() -> LocationEvalContext {
    LocationEvalContext {
        content_type: Some("portfolio_item".into()),
        ..Default::default()
    }
}

#[test]
fn validates_entry_and_all_nine_field_types() {
    let mut fields = sample_fields();
    let cases = [
        (
            "headline",
            crate::field_schema::FieldType::Text,
            json!("Hello"),
        ),
        (
            "details",
            crate::field_schema::FieldType::Richtext,
            json!("<b>Text</b>"),
        ),
        (
            "budget",
            crate::field_schema::FieldType::Number,
            json!(18.5),
        ),
        (
            "featured",
            crate::field_schema::FieldType::Boolean,
            json!(false),
        ),
        (
            "due_date",
            crate::field_schema::FieldType::Date,
            json!("2024-02-29"),
        ),
        (
            "photo",
            crate::field_schema::FieldType::Media,
            json!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        ),
        (
            "album",
            crate::field_schema::FieldType::Gallery,
            json!([
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            ]),
        ),
    ];
    let mut entry = entry();
    for (key, kind, value) in cases {
        fields.push(crate::field_schema::FieldSpec::new(kind, key, "Trường thử"));
        entry.values.insert(key.into(), value);
    }
    let schema = sample_schema_with_fields(fields);
    assert!(
        entry
            .validate_against_effective(&schema, &entry_ctx())
            .is_ok()
    );
    let encoded = serde_json::to_string(&entry).unwrap();
    assert_eq!(
        serde_json::from_str::<ContentEntryInput>(&encoded).unwrap(),
        entry
    );
}

#[test]
fn rejects_invalid_custom_field_values_and_missing_required() {
    let schema = sample_schema();
    let ctx = entry_ctx();
    let mut entry = entry();
    entry.values.remove("status_code");
    assert!(entry.validate_against_effective(&schema, &ctx).is_err());
    entry.values.insert("status_code".into(), json!("unknown"));
    assert!(entry.validate_against_effective(&schema, &ctx).is_err());
    entry.values.insert("status_code".into(), json!("planned"));
    entry.values.insert("unregistered".into(), json!("bad"));
    assert!(entry.validate_against_effective(&schema, &ctx).is_err());
    entry.values.remove("unregistered");
    entry.slug = "Invalid_Slug".into();
    assert!(entry.validate_against_effective(&schema, &ctx).is_err());
    entry.slug = "valid-slug".into();
    entry.values.insert("summary".into(), json!(true));
    assert!(entry.validate_against_effective(&schema, &ctx).is_err());
}

#[test]
fn rejects_invalid_dates_galleries_and_media_ids() {
    let mut fields = sample_fields();
    let mut date_spec =
        crate::field_schema::FieldSpec::new(crate::field_schema::FieldType::Date, "date", "Ngày");
    date_spec.required = true;
    fields.push(date_spec);
    fields.push(crate::field_schema::FieldSpec::new(
        crate::field_schema::FieldType::Gallery,
        "photos",
        "Album",
    ));
    let schema = sample_schema_with_fields(fields);
    let ctx = entry_ctx();
    let mut entry = entry();
    for bad in ["2025-02-29", "2024-13-01", "2024-02-30", "0000-01-01"] {
        entry.values.insert("date".into(), json!(bad));
        assert!(
            entry.validate_against_effective(&schema, &ctx).is_err(),
            "{bad}"
        );
    }
    entry.values.insert("date".into(), json!("2000-02-29"));
    entry.values.insert(
        "photos".into(),
        json!([
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ]),
    );
    assert!(entry.validate_against_effective(&schema, &ctx).is_err());
    entry.values.insert("photos".into(), json!(["../asset"]));
    assert!(entry.validate_against_effective(&schema, &ctx).is_err());
    entry.values.insert(
        "photos".into(),
        json!([
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        ]),
    );
    assert!(entry.validate_against_effective(&schema, &ctx).is_ok());
}

#[test]
fn patch_field_distinguishes_omitted_clear_and_set() {
    #[derive(Debug, Deserialize)]
    struct Probe {
        #[serde(default, deserialize_with = "deserialize_patch_field")]
        path: PatchField<String>,
    }
    let omitted: Probe = serde_json::from_str("{}").unwrap();
    assert_eq!(omitted.path, PatchField::Omitted);

    let cleared: Probe = serde_json::from_str(r#"{"path": null}"#).unwrap();
    assert_eq!(cleared.path, PatchField::Clear);

    let set: Probe = serde_json::from_str(r#"{"path": "/hello"}"#).unwrap();
    assert_eq!(set.path, PatchField::Set("/hello".to_string()));
}

#[test]
fn richtext_allows_384kib_when_containing_blocks_and_keeps_200k_for_plain() {
    let field = crate::field_schema::FieldSpec::new(
        crate::field_schema::FieldType::Richtext,
        "body",
        "Nội dung",
    );
    let plain_over_200k = "a".repeat(200_001);
    assert!(!valid_field_value(&field, &json!(plain_over_200k)));

    let block_250k = format!(
        "<!-- od:text {{\"id\":\"t1\"}} -->\n{}\n<!-- /od:text -->",
        "a".repeat(250_000)
    );
    assert!(valid_field_value(&field, &json!(block_250k)));

    let block_over_384k = format!(
        "<!-- od:text {{\"id\":\"t1\"}} -->\n{}\n<!-- /od:text -->",
        "a".repeat(crate::builder_blocks::MAX_BUILDER_BLOCK_BODY_BYTES)
    );
    assert!(!valid_field_value(&field, &json!(block_over_384k)));
}
