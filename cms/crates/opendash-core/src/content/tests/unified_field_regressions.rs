use super::*;
use crate::field_schema::{
    FieldAcl, FieldRole, FieldSpec, FieldType, MediaFieldKind, PublicExposure,
    default_value_for_field_spec,
};
use crate::registry::{DeclarationState, ProviderStatus, ResourceOwner};
use serde_json::{Value, json};

#[test]
fn f07_default_value_handles_single_and_multiple_entry_and_term_refs() {
    let mut single_entry = FieldSpec::new(FieldType::EntryRef, "rel", "Liên kết");
    single_entry.multiple = false;
    assert_eq!(default_value_for_field_spec(&single_entry), Value::Null);

    let mut multi_entry = single_entry.clone();
    multi_entry.multiple = true;
    assert_eq!(default_value_for_field_spec(&multi_entry), json!([]));

    let mut single_term = FieldSpec::new(FieldType::TermRef, "cat_ref", "Danh mục");
    single_term.multiple = false;
    assert_eq!(default_value_for_field_spec(&single_term), Value::Null);

    let mut multi_term = single_term;
    multi_term.multiple = true;
    assert_eq!(default_value_for_field_spec(&multi_term), json!([]));
}

#[test]
fn f10_f15_effective_schema_transition_enforces_acl_tightening_and_type_guards() {
    let mut hero = FieldSpec::new(FieldType::Media, "hero", "Ảnh bìa");
    hero.media_kind = Some(MediaFieldKind::Any);
    let mut picks = FieldSpec::new(FieldType::EntryRef, "picks", "Chọn bài");
    picks.multiple = true;
    picks.target_types = vec!["post".into()];
    let mut subtitle = FieldSpec::new(FieldType::Text, "subtitle", "Phụ đề");
    subtitle.required = false;

    let base_v1 = OwnedDescriptor {
        owner: ResourceOwner::User,
        status: ProviderStatus::Active,
        declaration_state: DeclarationState::Declared,
        descriptor: ContentTypeDefinition {
            key: "landing".into(),
            label: "Landing".into(),
            fields: vec![hero.clone(), picks.clone(), subtitle.clone()],
            settings: ContentTypeSettings::default(),
        },
    };
    let eff_v1 = EffectiveContentSchema::assemble(base_v1.clone(), 1, vec![]).unwrap();

    // F-15: Narrowing media_kind Any -> Image fails when entries exist.
    let mut hero_narrow = hero.clone();
    hero_narrow.media_kind = Some(MediaFieldKind::Image);
    let mut narrow_base = base_v1.clone();
    narrow_base.descriptor.fields = vec![hero_narrow, picks.clone(), subtitle.clone()];
    let eff_narrow = EffectiveContentSchema::assemble(narrow_base, 2, vec![]).unwrap();
    assert!(matches!(
        verify_effective_schema_transition(Some(&eff_v1), &eff_narrow, true),
        Err(SchemaTransitionError::IncompatibleSchemaChange(_))
    ));

    // F-15: Flipping multiple on EntryRef fails when entries exist.
    let mut picks_single = picks.clone();
    picks_single.multiple = false;
    let mut flipped_base = base_v1.clone();
    flipped_base.descriptor.fields = vec![hero.clone(), picks_single, subtitle.clone()];
    let eff_flipped = EffectiveContentSchema::assemble(flipped_base, 2, vec![]).unwrap();
    assert!(matches!(
        verify_effective_schema_transition(Some(&eff_v1), &eff_flipped, true),
        Err(SchemaTransitionError::IncompatibleSchemaChange(_))
    ));

    // F-10: Tightening required false -> true fails when entries exist.
    let mut sub_req = subtitle.clone();
    sub_req.required = true;
    let mut tight_base = base_v1.clone();
    tight_base.descriptor.fields = vec![hero.clone(), picks.clone(), sub_req];
    let eff_tight = EffectiveContentSchema::assemble(tight_base, 2, vec![]).unwrap();
    assert!(matches!(
        verify_effective_schema_transition(Some(&eff_v1), &eff_tight, true),
        Err(SchemaTransitionError::IncompatibleSchemaChange(_))
    ));

    // F-10: Admin-only required field without default fails COMPAT-4.
    let mut admin_req = FieldSpec::new(FieldType::Text, "audit_note", "Ghi chú");
    admin_req.required = true;
    admin_req.acl = Some(FieldAcl {
        min_view_role: Some(FieldRole::Admin),
        min_edit_role: Some(FieldRole::Admin),
        public_exposure: Some(PublicExposure::Private),
    });
    let mut bad_acl_base = base_v1;
    bad_acl_base.descriptor.fields = vec![hero, picks, subtitle, admin_req];
    assert!(matches!(
        EffectiveContentSchema::assemble(bad_acl_base, 2, vec![]),
        Err(SchemaTransitionError::AdminRequiredFieldMissingDefault(_))
    ));
}

#[test]
fn f12_project_effective_editor_config_keeps_ui_only_panel_fields_in_order() {
    let tab = FieldSpec::new(FieldType::Tab, "tab_info", "Thông tin");
    let visible_text = FieldSpec::new(FieldType::Text, "tagline", "Khẩu hiệu");
    let mut hidden_admin = FieldSpec::new(FieldType::Text, "internal_id", "Mã nội bộ");
    hidden_admin.acl = Some(FieldAcl {
        min_view_role: Some(FieldRole::Admin),
        min_edit_role: Some(FieldRole::Admin),
        public_exposure: Some(PublicExposure::Private),
    });
    let mut msg = FieldSpec::new(FieldType::Message, "help_msg", "Hướng dẫn");
    msg.body = Some("Nhập nội dung".into());

    let base = OwnedDescriptor {
        owner: ResourceOwner::User,
        status: ProviderStatus::Active,
        declaration_state: DeclarationState::Declared,
        descriptor: ContentTypeDefinition {
            key: "service".into(),
            label: "Dịch vụ".into(),
            fields: vec![tab, visible_text, hidden_admin, msg],
            settings: ContentTypeSettings::default(),
        },
    };
    let eff = EffectiveContentSchema::assemble(base, 1, vec![]).unwrap();
    let panels = build_core_and_group_panels(&eff, false, &[]);
    let (layout, all_panels) = resolve_and_validate_editor_layout(&eff, panels, None, 1).unwrap();
    let projected =
        project_effective_editor_config(&eff, &layout, &all_panels, &[], FieldRole::Editor);
    let base_panel = [
        &projected.regions.main_top,
        &projected.regions.main_primary,
        &projected.regions.main_bottom,
        &projected.regions.sidebar,
    ]
    .into_iter()
    .flat_map(|r| r.iter())
    .find(|p| p.panel.panel_id == "core:base_fields")
    .expect("core:base_fields present");
    let keys: Vec<&str> = base_panel
        .panel_fields
        .iter()
        .map(|f| f.key.as_str())
        .collect();
    assert_eq!(keys, vec!["tab_info", "tagline", "help_msg"]);
}

#[test]
fn f14_content_type_validate_allows_non_public_builder_and_rejects_duplicate_supports() {
    let non_public_builder = ContentTypeDefinition {
        key: "internal_doc".into(),
        label: "Tài liệu nội bộ".into(),
        fields: vec![],
        settings: ContentTypeSettings {
            public: false,
            has_archive: false,
            publication: ContentPublication::DraftPublish,
            supports: vec![ContentSupport::Builder],
        },
    };
    non_public_builder.validate().unwrap();

    let mut dup_supports = non_public_builder;
    dup_supports.settings.supports = vec![ContentSupport::Builder, ContentSupport::Builder];
    assert!(dup_supports.validate().is_err());
}
