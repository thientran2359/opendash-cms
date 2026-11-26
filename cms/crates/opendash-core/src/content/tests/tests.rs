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

fn sample_schema() -> ContentTypeDefinition {
    ContentTypeDefinition {
        key: "portfolio_item".into(),
        label: "Dự án".into(),
        fields: vec![
            ContentFieldDefinition {
                key: "summary".into(),
                label: "Tóm tắt".into(),
                field_type: ContentFieldType::LongText,
                required: false,
                options: vec![],
                ..Default::default()
            },
            ContentFieldDefinition {
                key: "status".into(),
                label: "Trạng thái".into(),
                field_type: ContentFieldType::Select,
                required: true,
                options: vec![
                    SelectOption {
                        value: "planned".into(),
                        label: "Dự kiến".into(),
                    },
                    SelectOption {
                        value: "published".into(),
                        label: "Đã đăng".into(),
                    },
                ],
                ..Default::default()
            },
        ],
        settings: ContentTypeSettings::default(),
    }
}

#[test]
fn content_schema_round_trips_and_validates() {
    let schema = sample_schema();
    assert!(schema.validate().is_ok());
    let json = serde_json::to_string(&schema).unwrap();
    assert!(json.contains("\"type\":\"long_text\""));
    assert!(json.contains("\"public_exposure\":\"public\""));
    let decoded: ContentTypeDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, schema);
    assert!(decoded.validate().is_ok());
}

#[test]
fn context_aware_field_definition_input_normalization() {
    // Case 1: Base CPT field without public_exposure -> Public
    let base_missing: FieldDefinitionInput = serde_json::from_value(json!({
        "key": "subtitle",
        "label": "Phụ đề",
        "type": "short_text"
    }))
    .unwrap();
    let canonical_base = base_missing
        .into_canonical(FieldScopeKind::BaseCptField)
        .unwrap();
    assert_eq!(canonical_base.public_exposure, PublicExposure::Public);

    // Case 2: Extension group field without public_exposure -> Private
    let ext_missing: FieldDefinitionInput = serde_json::from_value(json!({
        "key": "internal_note",
        "label": "Ghi chú nội bộ",
        "type": "long_text"
    }))
    .unwrap();
    let canonical_ext_priv = ext_missing
        .into_canonical(FieldScopeKind::ExtensionGroupField)
        .unwrap();
    assert_eq!(canonical_ext_priv.public_exposure, PublicExposure::Private);

    // Case 3: Extension group field with explicit "public_exposure": "public" -> Public
    let ext_explicit_pub: FieldDefinitionInput = serde_json::from_value(json!({
        "key": "hero_badge",
        "label": "Nhãn Hero",
        "type": "short_text",
        "public_exposure": "public"
    }))
    .unwrap();
    let canonical_ext_pub = ext_explicit_pub
        .into_canonical(FieldScopeKind::ExtensionGroupField)
        .unwrap();
    assert_eq!(canonical_ext_pub.public_exposure, PublicExposure::Public);
}

#[test]
fn supports_all_nine_v1_field_kinds() {
    let kinds = [
        ContentFieldType::ShortText,
        ContentFieldType::LongText,
        ContentFieldType::RichText,
        ContentFieldType::Number,
        ContentFieldType::Boolean,
        ContentFieldType::Date,
        ContentFieldType::Select,
        ContentFieldType::Image,
        ContentFieldType::Gallery,
    ];
    for kind in kinds {
        let mut field = sample_schema().fields[0].clone();
        field.field_type = kind;
        if kind == ContentFieldType::Select {
            field.options = sample_schema().fields[1].options.clone();
        }
        assert!(field.validate().is_ok());
    }
}

#[test]
fn rejects_duplicate_keys_and_invalid_select_options() {
    let mut schema = sample_schema();
    schema.fields.push(schema.fields[0].clone());
    assert!(schema.validate().is_err());

    let mut schema = sample_schema();
    schema.fields[1].options[1].value = "planned".into();
    assert!(schema.validate().is_err());

    schema.fields[1].options.clear();
    assert!(schema.validate().is_err());

    schema.fields[0].options.push(SelectOption {
        value: "wrong".into(),
        label: "Sai loại".into(),
    });
    assert!(schema.validate().is_err());
}

#[test]
fn rejects_invalid_identifiers_and_labels() {
    let mut schema = sample_schema();
    schema.key = "Invalid-Type".into();
    assert!(schema.validate().is_err());

    schema.key = "post".into();
    schema.label = "   ".into();
    assert!(schema.validate().is_err());

    schema.label = "Bài viết".into();
    schema.fields[0].key = "bad-name".into();
    assert!(schema.validate().is_err());
}

fn entry() -> ContentEntryInput {
    ContentEntryInput {
        title: "Dự án đầu tiên".into(),
        slug: "du-an-dau-tien".into(),
        locale: None,
        values: BTreeMap::from([
            ("summary".into(), json!("Nội dung")),
            ("status".into(), json!("planned")),
        ]),
        term_ids: None,
    }
}

#[test]
fn validates_entry_and_all_nine_field_types() {
    let mut schema = sample_schema();
    let cases = [
        ("headline", ContentFieldType::ShortText, json!("Hello")),
        ("body", ContentFieldType::RichText, json!("<b>Text</b>")),
        ("budget", ContentFieldType::Number, json!(18.5)),
        ("featured", ContentFieldType::Boolean, json!(false)),
        ("due_date", ContentFieldType::Date, json!("2024-02-29")),
        ("photo", ContentFieldType::Image, json!("asset_1")),
        (
            "album",
            ContentFieldType::Gallery,
            json!(["asset_1", "asset_2"]),
        ),
    ];
    let mut entry = entry();
    for (key, kind, value) in cases {
        schema.fields.push(ContentFieldDefinition {
            key: key.into(),
            label: "Trường thử".into(),
            field_type: kind,
            required: false,
            options: vec![],
            ..Default::default()
        });
        entry.values.insert(key.into(), value);
    }
    assert!(entry.validate_against(&schema).is_ok());
    let encoded = serde_json::to_string(&entry).unwrap();
    assert_eq!(
        serde_json::from_str::<ContentEntryInput>(&encoded).unwrap(),
        entry
    );
}

#[test]
fn rejects_invalid_custom_field_values_and_missing_required() {
    let schema = sample_schema();
    let mut entry = entry();
    entry.values.remove("status");
    assert!(entry.validate_against(&schema).is_err());
    entry.values.insert("status".into(), json!("unknown"));
    assert!(entry.validate_against(&schema).is_err());
    entry.values.insert("status".into(), json!("planned"));
    entry.values.insert("unregistered".into(), json!("bad"));
    assert!(entry.validate_against(&schema).is_err());
    entry.values.remove("unregistered");
    entry.slug = "Invalid_Slug".into();
    assert!(entry.validate_against(&schema).is_err());
    entry.slug = "valid-slug".into();
    entry.values.insert("summary".into(), json!(true));
    assert!(entry.validate_against(&schema).is_err());
}

#[test]
fn rejects_invalid_dates_galleries_and_media_ids() {
    let mut schema = sample_schema();
    schema.fields.push(ContentFieldDefinition {
        key: "date".into(),
        label: "Ngày".into(),
        field_type: ContentFieldType::Date,
        required: true,
        options: vec![],
        ..Default::default()
    });
    schema.fields.push(ContentFieldDefinition {
        key: "photos".into(),
        label: "Album".into(),
        field_type: ContentFieldType::Gallery,
        required: false,
        options: vec![],
        ..Default::default()
    });
    let mut entry = entry();
    for bad in ["2025-02-29", "2024-13-01", "2024-02-30", "0000-01-01"] {
        entry.values.insert("date".into(), json!(bad));
        assert!(entry.validate_against(&schema).is_err(), "{bad}");
    }
    entry.values.insert("date".into(), json!("2000-02-29"));
    entry
        .values
        .insert("photos".into(), json!(["asset1", "asset1"]));
    assert!(entry.validate_against(&schema).is_err());
    entry.values.insert("photos".into(), json!(["../asset"]));
    assert!(entry.validate_against(&schema).is_err());
    entry
        .values
        .insert("photos".into(), json!(["asset1", "asset2"]));
    assert!(entry.validate_against(&schema).is_ok());
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
    let field = ContentFieldDefinition {
        key: "body".into(),
        label: "Nội dung".into(),
        field_type: ContentFieldType::RichText,
        required: false,
        options: vec![],
        ..Default::default()
    };
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

#[test]
fn cpt_builder_auto_injects_body_rejects_non_richtext_and_emits_body_panel() {
    let builder_settings = ContentTypeSettings {
        public: true,
        has_archive: true,
        publication: ContentPublication::DraftPublish,
        supports: vec![ContentSupport::Builder, ContentSupport::FeaturedImage],
    };
    let base_no_body = OwnedDescriptor {
        owner: ResourceOwner::User,
        status: ProviderStatus::Active,
        declaration_state: DeclarationState::Declared,
        descriptor: ContentTypeDefinition {
            key: "service".into(),
            label: "Dịch vụ".into(),
            fields: vec![],
            settings: builder_settings.clone(),
        },
    };
    let eff = EffectiveContentSchema::assemble(base_no_body.clone(), 1, vec![]).unwrap();
    let injected = eff.effective_fields.get("body").expect("body injected");
    assert_eq!(injected.definition.field_type, ContentFieldType::RichText);
    assert_eq!(injected.owner, ResourceOwner::Core);
    assert!(
        build_core_and_group_panels(&eff, false, &[])
            .iter()
            .any(|p| p.panel_id == "core:body_editor")
    );

    // Disabling Builder on the custom CPT allows removing the synthetic Core `body`
    // even when entries already exist.
    let mut disabled_base = base_no_body;
    disabled_base.descriptor.settings.supports = vec![ContentSupport::FeaturedImage];
    let eff_disabled = EffectiveContentSchema::assemble(disabled_base, 2, vec![]).unwrap();
    assert!(!eff_disabled.effective_fields.contains_key("body"));
    verify_effective_schema_transition(Some(&eff), &eff_disabled, true).unwrap();

    // Non-RichText `body` is rejected when ContentSupport::Builder is enabled.
    let bad_base = OwnedDescriptor {
        owner: ResourceOwner::User,
        status: ProviderStatus::Active,
        declaration_state: DeclarationState::Declared,
        descriptor: ContentTypeDefinition {
            key: "bad_cpt".into(),
            label: "Bad".into(),
            fields: vec![ContentFieldDefinition {
                key: "body".into(),
                label: "Short body".into(),
                field_type: ContentFieldType::ShortText,
                required: false,
                options: vec![],
                ..Default::default()
            }],
            settings: builder_settings,
        },
    };
    assert!(matches!(
        EffectiveContentSchema::assemble(bad_base, 1, vec![]),
        Err(SchemaTransitionError::IncompatibleSchemaChange(_))
    ));
}
