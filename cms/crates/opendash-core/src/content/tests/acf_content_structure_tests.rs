use super::*;
use crate::field_schema::{
    FieldAcl, FieldContext, FieldRole, FieldScopeKind, FieldSpec, FieldType, HydrationVisibility,
    PublicExposure, normalize_field_specs_for_scope, validate_field_specs,
};
use crate::plugin::PluginManifest;
use serde_json::json;

fn rule(param: LocationParam, op: LocationOp, value: &str) -> LocationRule {
    LocationRule {
        param,
        op,
        value: value.to_owned(),
    }
}

#[test]
fn location_rules_validate_domain_isolation_and_constraints() {
    let content_loc = vec![
        vec![
            rule(LocationParam::ContentType, LocationOp::Eq, "docs"),
            rule(LocationParam::UserRole, LocationOp::Eq, "admin"),
        ],
        vec![
            rule(LocationParam::PageLayout, LocationOp::Eq, "landing"),
            rule(LocationParam::SpecialPage, LocationOp::Eq, "homepage"),
        ],
    ];
    assert_eq!(
        validate_location_rules_and_infer_domain(&content_loc).unwrap(),
        FieldGroupDomain::Content
    );

    let term_loc = vec![vec![
        rule(LocationParam::TaxonomyTerm, LocationOp::Eq, "topic"),
        rule(LocationParam::UserRole, LocationOp::Neq, "editor"),
    ]];
    assert_eq!(
        validate_location_rules_and_infer_domain(&term_loc).unwrap(),
        FieldGroupDomain::Term
    );

    let opt_loc = vec![vec![rule(
        LocationParam::OptionsPage,
        LocationOp::Eq,
        "custom:company_info",
    )]];
    assert_eq!(
        validate_location_rules_and_infer_domain(&opt_loc).unwrap(),
        FieldGroupDomain::OptionsPage
    );

    // Reject mixing domains in same branch or across OR branches
    let mixed_branch = vec![vec![
        rule(LocationParam::TaxonomyTerm, LocationOp::Eq, "category"),
        rule(LocationParam::ContentType, LocationOp::Eq, "post"),
    ]];
    assert!(matches!(
        validate_location_rules_and_infer_domain(&mixed_branch),
        Err(SchemaTransitionError::InvalidLocationRulesDomain(_))
    ));
    let mixed_or = vec![
        vec![rule(
            LocationParam::TaxonomyTerm,
            LocationOp::Eq,
            "category",
        )],
        vec![rule(
            LocationParam::OptionsPage,
            LocationOp::Eq,
            "site_global",
        )],
    ];
    assert!(matches!(
        validate_location_rules_and_infer_domain(&mixed_or),
        Err(SchemaTransitionError::InvalidLocationRulesDomain(_))
    ));

    // Reject pattern/section and invalid options_page Neq
    for bad in [
        vec![vec![rule(
            LocationParam::ContentType,
            LocationOp::Eq,
            "pattern",
        )]],
        vec![vec![rule(
            LocationParam::ContentType,
            LocationOp::Eq,
            "section",
        )]],
        vec![vec![rule(
            LocationParam::OptionsPage,
            LocationOp::Neq,
            "site_global",
        )]],
        vec![vec![rule(
            LocationParam::PageLayout,
            LocationOp::Eq,
            "Bad-Key",
        )]],
        vec![vec![rule(
            LocationParam::EntryTaxonomy,
            LocationOp::Eq,
            "cat:0",
        )]],
        vec![vec![rule(LocationParam::EntryId, LocationOp::Eq, "05")]],
    ] {
        assert!(validate_location_rules_and_infer_domain(&bad).is_err());
    }
}

#[test]
fn location_rules_materialize_targets_evaluate_and_check_admin_only() {
    let groups = vec![
        vec![rule(LocationParam::PageLayout, LocationOp::Eq, "landing")],
        vec![
            rule(LocationParam::ContentType, LocationOp::Eq, "docs"),
            rule(LocationParam::EntryTaxonomy, LocationOp::Eq, "topic:7"),
            rule(LocationParam::UserRole, LocationOp::Eq, "admin"),
        ],
    ];
    assert_eq!(
        extract_materialized_targets(&groups).unwrap(),
        vec![
            ("content_type".to_owned(), "docs".to_owned()),
            ("content_type".to_owned(), "page".to_owned()),
        ]
    );
    assert!(!is_restricted_to_admin_only(&groups));
    assert!(!group_allows_role_for_target(
        &groups,
        "content_type",
        "docs",
        FieldRole::Editor
    ));
    assert!(group_allows_role_for_target(
        &groups,
        "content_type",
        "page",
        FieldRole::Editor
    ));

    let admin_only = vec![
        vec![rule(LocationParam::UserRole, LocationOp::Eq, "admin")],
        vec![rule(LocationParam::UserRole, LocationOp::Neq, "editor")],
    ];
    assert!(is_restricted_to_admin_only(&admin_only));

    let mut ctx = LocationEvalContext {
        content_type: Some("docs".into()),
        entry_term_pairs: BTreeSet::from([("topic".to_owned(), 7)]),
        user_role: FieldRole::Admin,
        ..Default::default()
    };
    assert!(evaluate_location_rules(&groups, &ctx));
    ctx.user_role = FieldRole::Editor;
    assert!(!evaluate_location_rules(&groups, &ctx));
}

#[test]
fn field_group_input_enforces_xor_location_presentation_acl2_and_cycles() {
    // Reject mixing presentation + legacy flat keys
    let mixed_pres: UserFieldGroupInput = serde_json::from_value(json!({
        "local_key": "g1",
        "title": "G1",
        "target_types": ["post"],
        "presentation": { "region": "sidebar" },
        "default_order": 10,
        "fields": [{ "type": "text", "key": "note", "label": "Note" }]
    }))
    .unwrap();
    let err = mixed_pres.validate_and_normalize(None).unwrap_err();
    assert!(err.starts_with("invalid_field_group_presentation:"));

    // Target types ["*"] maps to content_type == all
    let wildcard: UserFieldGroupInput = serde_json::from_value(json!({
        "local_key": "g_all",
        "title": "All",
        "target_types": ["*"],
        "fields": [{ "type": "text", "key": "note", "label": "Note" }]
    }))
    .unwrap();
    let (_, norm) = wildcard.validate_and_normalize(None).unwrap();
    assert_eq!(
        extract_materialized_targets(&norm.location).unwrap(),
        vec![("content_type".to_owned(), "*".to_owned())]
    );

    // Reject public field inside admin-only group (ACL-2)
    let admin_pub: UserFieldGroupInput = serde_json::from_value(json!({
        "local_key": "g_admin",
        "title": "Admin",
        "location": [[
            { "param": "content_type", "op": "eq", "value": "post" },
            { "param": "user_role", "op": "eq", "value": "admin" }
        ]],
        "fields": [{ "type": "text", "key": "secret_note", "label": "Note" }]
    }))
    .unwrap();
    assert!(
        admin_pub
            .validate_and_normalize(None)
            .unwrap_err()
            .starts_with("invalid_field_acl:")
    );

    // Reject sync_entry_terms + entry_taxonomy cycle
    let cycle: UserFieldGroupInput = serde_json::from_value(json!({
        "local_key": "g_cycle",
        "title": "Cycle",
        "location": [[
            { "param": "content_type", "op": "eq", "value": "post" },
            { "param": "entry_taxonomy", "op": "eq", "value": "category:1" }
        ]],
        "fields": [{
            "type": "term_ref",
            "key": "cats",
            "label": "Cats",
            "taxonomy": "category",
            "sync_entry_terms": true
        }]
    }))
    .unwrap();
    assert!(
        cycle
            .validate_and_normalize(None)
            .unwrap_err()
            .starts_with("sync_entry_terms_location_cycle:")
    );

    // Reject site_global key collision
    let site_global_col: UserFieldGroupInput = serde_json::from_value(json!({
        "local_key": "g_opt",
        "title": "Opt",
        "location": [[{ "param": "options_page", "op": "eq", "value": "site_global" }]],
        "fields": [{ "type": "url", "key": "social_facebook", "label": "FB", "public": true }]
    }))
    .unwrap();
    assert!(
        site_global_col
            .validate_and_normalize(None)
            .unwrap_err()
            .starts_with("field_key_collision:")
    );
}

#[test]
fn options_acl_normalization_and_hydration_visibility_stay_1_to_1() {
    let mut specs = vec![{
        let mut s = FieldSpec::new(FieldType::Text, "hotline", "Hotline");
        s.public = true;
        s
    }];
    normalize_field_specs_for_scope(
        &mut specs,
        FieldContext::Options,
        FieldScopeKind::UserFieldGroup,
    )
    .unwrap();
    assert!(HydrationVisibility::OptionsPublicOnly.allows(&specs[0]));

    let mut priv_opt = FieldSpec::new(FieldType::Text, "internal", "Internal");
    priv_opt.public = false;
    priv_opt.acl = Some(FieldAcl::canonical(
        FieldRole::Admin,
        FieldRole::Admin,
        PublicExposure::Private,
    ));
    assert!(!HydrationVisibility::OptionsPublicOnly.allows(&priv_opt));
    assert!(validate_field_specs(&[priv_opt], FieldContext::Options).is_ok());

    // Mismatch public: true with Admin view must fail
    let mut bad_pub = FieldSpec::new(FieldType::Text, "bad", "Bad");
    bad_pub.public = true;
    bad_pub.acl = Some(FieldAcl::canonical(
        FieldRole::Admin,
        FieldRole::Admin,
        PublicExposure::Private,
    ));
    assert!(validate_field_specs(&[bad_pub], FieldContext::Options).is_err());
}

#[test]
fn cpt_settings_taxonomy_settings_options_pages_and_plugin_v3_gates() {
    let mut settings = ContentTypeSettings {
        hierarchical: true,
        supports: vec![ContentSupport::CustomTemplate, ContentSupport::MenuOrder],
        menu_icon: Some("book-open".into()),
        list_columns: vec![
            ContentListColumn {
                kind: ContentListColumnKind::Field,
                key: "difficulty".into(),
                label: None,
            },
            ContentListColumn {
                kind: ContentListColumnKind::MenuOrder,
                key: String::new(),
                label: Some("Thứ tự".into()),
            },
        ],
        ..ContentTypeSettings::default()
    };
    settings.validate_and_normalize().unwrap();

    // `post` forbids hierarchical; `page` forbids CustomTemplate
    let post_bad = ContentTypeDefinition {
        key: "post".into(),
        label: "Posts".into(),
        singular_label: None,
        description: None,
        settings: ContentTypeSettings {
            hierarchical: true,
            ..ContentTypeSettings::builtin_publication()
        },
    };
    assert!(post_bad.validate().is_err());

    let page_bad = ContentTypeDefinition {
        key: "page".into(),
        label: "Pages".into(),
        singular_label: None,
        description: None,
        settings: ContentTypeSettings {
            supports: vec![ContentSupport::CustomTemplate],
            ..ContentTypeSettings::builtin_page()
        },
    };
    assert!(page_bad.validate().is_err());

    // Reserved options page key
    assert!(
        valid_custom_options_page_key("site_global")
            .unwrap_err()
            .starts_with("options_page_key_reserved:")
    );
    assert!(valid_custom_options_page_key("company_info").is_ok());

    // Plugin manifest declaring options_pages requires manifest_schema >= 3 + min_host_version
    let v2_plugin = json!({
        "id": "acme-opt",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "Acme Opt",
        "declarations": {
            "options_pages": [{
                "page_key": "company_info",
                "title": "Company Info",
                "menu_label": "Company"
            }]
        }
    })
    .to_string();
    assert!(PluginManifest::from_json_str(&v2_plugin).is_err());

    let v3_plugin = json!({
        "id": "acme-opt",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "min_host_version": "0.1.1",
        "name": "Acme Opt",
        "declarations": {
            "options_pages": [{
                "page_key": "company_info",
                "title": "Company Info",
                "menu_label": "Company"
            }]
        }
    })
    .to_string();
    assert!(PluginManifest::from_json_str(&v3_plugin).is_ok());

    // EffectiveTermSchema scopes schema_token by role & blocks reserved term keys
    let tax = TaxonomyDefinition {
        key: "topic".into(),
        label: "Chủ đề".into(),
        hierarchical: false,
        public: true,
        settings: TaxonomySettings::default(),
        rewrite_base: None,
        content_types: vec!["post".into()],
        version: 1,
        owner: ResourceOwner::User,
        provider_status: ProviderStatus::Active,
        declaration_state: DeclarationState::Declared,
    };
    let admin_grp = AttachedFieldGroupSnapshot {
        group_id: "user:topic_admin".into(),
        owner: ResourceOwner::User,
        local_key: "topic_admin".into(),
        title: "Admin Only".into(),
        description: String::new(),
        domain: FieldGroupDomain::Term,
        location: vec![vec![
            rule(LocationParam::TaxonomyTerm, LocationOp::Eq, "topic"),
            rule(LocationParam::UserRole, LocationOp::Eq, "admin"),
        ]],
        presentation: FieldGroupPresentation::default(),
        active: true,
        fields: vec![{
            let mut s = FieldSpec::new(FieldType::Text, "internal_tag", "Internal");
            s.acl = Some(FieldAcl::canonical(
                FieldRole::Admin,
                FieldRole::Admin,
                PublicExposure::Private,
            ));
            s
        }],
        status: FieldGroupStatus::Active,
        association_state: DeclarationState::Declared,
        version: 1,
    };
    let eff_term = EffectiveTermSchema::assemble(tax, vec![admin_grp]).unwrap();
    let admin_tok = eff_term.canonical_token_payload_json(None, &[], Some(FieldRole::Admin));
    let editor_tok = eff_term.canonical_token_payload_json(None, &[], Some(FieldRole::Editor));
    assert!(admin_tok.contains("internal_tag"));
    assert!(!editor_tok.contains("internal_tag"));
}

#[test]
fn cpt_builder_and_supports_inject_core_fields_and_forbid_field_group_collisions() {
    let builder_settings = ContentTypeSettings {
        public: true,
        has_archive: true,
        publication: ContentPublication::DraftPublish,
        supports: vec![
            ContentSupport::Builder,
            ContentSupport::FeaturedImage,
            ContentSupport::Excerpt,
        ],
        ..ContentTypeSettings::default()
    };
    let base_no_body = OwnedDescriptor {
        owner: ResourceOwner::User,
        status: ProviderStatus::Active,
        declaration_state: DeclarationState::Declared,
        descriptor: ContentTypeDefinition {
            key: "service".into(),
            label: "Dịch vụ".into(),
            singular_label: None,
            description: None,
            settings: builder_settings.clone(),
        },
    };
    let eff = EffectiveContentSchema::assemble(base_no_body.clone(), 1, vec![]).unwrap();
    let injected = eff.effective_fields.get("body").expect("body injected");
    assert_eq!(
        injected.definition.field_type,
        crate::field_schema::FieldType::Richtext
    );
    assert_eq!(injected.owner, ResourceOwner::Core);
    assert!(eff.effective_fields.contains_key("featured_image"));
    assert!(eff.effective_fields.contains_key("excerpt"));
    let panels = build_core_and_group_panels(&eff, false, &[]);
    assert!(panels.iter().any(|p| p.panel_id == "core:body_editor"));
    assert!(panels.iter().any(|p| p.panel_id == "core:excerpt"));
    assert!(!panels.iter().any(|p| p.panel_id == "core:base_fields"));

    // Disabling Builder, FeaturedImage, and Excerpt allows removing Core fields even with entries.
    let mut disabled_base = base_no_body.clone();
    disabled_base.descriptor.settings.supports = vec![];
    let eff_disabled = EffectiveContentSchema::assemble(disabled_base, 2, vec![]).unwrap();
    assert!(!eff_disabled.effective_fields.contains_key("body"));
    assert!(!eff_disabled.effective_fields.contains_key("featured_image"));
    assert!(!eff_disabled.effective_fields.contains_key("excerpt"));
    verify_effective_schema_transition(Some(&eff), &eff_disabled, true).unwrap();

    // Field Group declaring reserved Core-managed key `body` is rejected with FieldKeyCollision.
    let colliding_group = AttachedFieldGroupSnapshot {
        group_id: "user:bad_group".into(),
        owner: ResourceOwner::User,
        local_key: "bad_group".into(),
        title: "Bad".into(),
        description: String::new(),
        domain: FieldGroupDomain::Content,
        location: vec![vec![LocationRule {
            param: LocationParam::ContentType,
            op: LocationOp::Eq,
            value: "service".into(),
        }]],
        presentation: FieldGroupPresentation::default(),
        active: true,
        fields: vec![crate::field_schema::FieldSpec::new(
            crate::field_schema::FieldType::Text,
            "body",
            "Short body",
        )],
        status: FieldGroupStatus::Active,
        association_state: DeclarationState::Declared,
        version: 1,
    };
    assert!(matches!(
        EffectiveContentSchema::assemble(base_no_body, 1, vec![colliding_group]),
        Err(SchemaTransitionError::FieldKeyCollision(_))
    ));
}
