use super::*;

#[test]
fn validates_all_25_data_types_and_3_layouts_in_studio_block() {
    let mut tab1 = FieldSpec::new(FieldType::Tab, "tab_content", "Nội dung");
    tab1.placement = Some(FieldLayoutPlacement::Top);
    let mut acc1 = FieldSpec::new(FieldType::Accordion, "acc_meta", "Chi tiết");
    acc1.open_by_default = true;
    acc1.multi_expand = true;
    let mut msg = FieldSpec::new(FieldType::Message, "msg_hint", "Lưu ý");
    msg.body = Some("Điền đầy đủ trước khi lưu.".to_owned());
    msg.tone = Some(MessageTone::Info);

    let text = FieldSpec::new(FieldType::Text, "title", "Tiêu đề");
    let textarea = FieldSpec::new(FieldType::Textarea, "summary", "Tóm tắt");
    let num = FieldSpec::new(FieldType::Number, "cols", "Số cột");
    let mut range = FieldSpec::new(FieldType::Range, "opacity", "Độ mờ");
    range.min = Some(0.0);
    range.max = Some(100.0);
    range.default = Some(serde_json::json!(80));
    let email = FieldSpec::new(FieldType::Email, "contact_email", "Email");
    let url = FieldSpec::new(FieldType::Url, "website", "Website");
    let rt = FieldSpec::new(FieldType::Richtext, "body_copy", "Nội dung chi tiết");
    let media = FieldSpec::new(FieldType::Media, "hero_image", "Ảnh chính");
    let gallery = FieldSpec::new(FieldType::Gallery, "photos", "Thư viện ảnh");
    let link = FieldSpec::new(FieldType::Link, "cta_link", "Nút CTA");

    let mut sel = FieldSpec::new(FieldType::Select, "tone", "Tông màu");
    sel.options = vec![opt("brand", "Brand"), opt("neutral", "Neutral")];
    let mut sel_dyn = FieldSpec::new(FieldType::Select, "nav_menu", "Menu");
    sel_dyn.options_source = Some(SelectOptionsSource::Menus);

    let mut chk = FieldSpec::new(FieldType::Checkbox, "features", "Tính năng");
    chk.options = vec![opt("fast", "Nhanh"), opt("safe", "An toàn")];
    chk.layout = Some("horizontal".to_owned());

    let mut radio = FieldSpec::new(FieldType::RadioButtons, "align", "Căn lề");
    radio.options = vec![opt("left", "Trái"), opt("right", "Phải")];
    radio.layout = Some("buttons".to_owned());

    let boolean = FieldSpec::new(FieldType::Boolean, "show_badge", "Hiện nhãn");
    let color = FieldSpec::new(FieldType::Color, "accent_color", "Màu nhấn");
    let icon = FieldSpec::new(FieldType::Icon, "badge_icon", "Biểu tượng");
    let date = FieldSpec::new(FieldType::Date, "launch_date", "Ngày ra mắt");
    let datetime = FieldSpec::new(FieldType::Datetime, "publish_at", "Thời điểm");
    let time = FieldSpec::new(FieldType::Time, "open_time", "Giờ mở");
    let mut spacing = FieldSpec::new(FieldType::Spacing, "outer_margin", "Lề ngoài");
    spacing.property = Some(SpacingProperty::Margin);

    let mut entry_ref = FieldSpec::new(FieldType::EntryRef, "related_posts", "Bài liên quan");
    entry_ref.target_types = vec!["*".to_owned()];
    entry_ref.multiple = true;

    let mut term_tax = FieldSpec::new(FieldType::Select, "tax_kind", "Loại phân loại");
    term_tax.options_source = Some(SelectOptionsSource::Taxonomies);
    let mut term_ref = FieldSpec::new(FieldType::TermRef, "chosen_terms", "Thẻ chọn");
    term_ref.taxonomy_from_field = Some("tax_kind".to_owned());
    term_ref.multiple = true;

    let mut group = FieldSpec::new(FieldType::Group, "card_group", "Nhóm thẻ");
    group.fields = vec![FieldSpec::new(FieldType::Text, "heading", "Tiêu đề thẻ")];

    let mut rep = FieldSpec::new(FieldType::Repeater, "items", "Danh sách");
    rep.fields = vec![FieldSpec::new(FieldType::Text, "item_title", "Tên mục")];
    rep.item_label_field = Some("item_title".to_owned());

    let endpoint = {
        let mut a = FieldSpec::new(FieldType::Accordion, "acc_end", "");
        a.endpoint = true;
        a
    };

    let specs = vec![
        tab1, acc1, msg, text, textarea, num, range, email, url, rt, media, gallery, link, sel,
        sel_dyn, chk, radio, boolean, color, icon, date, datetime, time, spacing, entry_ref,
        term_tax, term_ref, group, rep, endpoint,
    ];
    assert!(validate_field_specs(&specs, FieldContext::StudioBlock).is_ok());
}

#[test]
fn owner_settings_allows_exact_13_data_types_plus_secret_and_rejects_others() {
    let mut secret = FieldSpec::new(FieldType::Secret, "api_key", "API Key");
    secret.required = true;
    let specs = vec![
        FieldSpec::new(FieldType::Text, "a", "A"),
        FieldSpec::new(FieldType::Textarea, "b", "B"),
        FieldSpec::new(FieldType::Number, "c", "C"),
        {
            let mut r = FieldSpec::new(FieldType::Range, "d", "D");
            r.min = Some(1.0);
            r.max = Some(10.0);
            r
        },
        FieldSpec::new(FieldType::Boolean, "e", "E"),
        {
            let mut s = FieldSpec::new(FieldType::Select, "f", "F");
            s.options = vec![opt("one", "One")];
            s
        },
        {
            let mut rb = FieldSpec::new(FieldType::RadioButtons, "g", "G");
            rb.options = vec![opt("r1", "R1")];
            rb
        },
        {
            let mut cb = FieldSpec::new(FieldType::Checkbox, "h", "H");
            cb.options = vec![opt("c1", "C1")];
            cb
        },
        FieldSpec::new(FieldType::Email, "i", "I"),
        FieldSpec::new(FieldType::Url, "j", "J"),
        FieldSpec::new(FieldType::Media, "k", "K"),
        secret,
        {
            let mut g = FieldSpec::new(FieldType::Group, "l", "L");
            g.fields = vec![FieldSpec::new(FieldType::Text, "sub", "Sub")];
            g
        },
    ];
    assert!(validate_field_specs(&specs, FieldContext::OwnerSettings).is_ok());

    for forbidden in [
        FieldType::Richtext,
        FieldType::Gallery,
        FieldType::Link,
        FieldType::Color,
        FieldType::Icon,
        FieldType::Date,
        FieldType::Datetime,
        FieldType::Time,
        FieldType::Spacing,
        FieldType::EntryRef,
        FieldType::TermRef,
        FieldType::Repeater,
    ] {
        let mut bad = FieldSpec::new(forbidden, "bad_field", "Bad");
        if matches!(forbidden, FieldType::Repeater) {
            bad.fields = vec![FieldSpec::new(FieldType::Text, "s", "S")];
        }
        if matches!(forbidden, FieldType::EntryRef) {
            bad.target_types = vec!["post".to_owned()];
        }
        if matches!(forbidden, FieldType::TermRef) {
            bad.taxonomy = Some("category".to_owned());
        }
        assert!(
            validate_field_specs(&[bad], FieldContext::OwnerSettings).is_err(),
            "OwnerSettings must reject {forbidden:?}"
        );
    }
}

#[test]
fn forbids_secret_in_other_contexts_or_inside_group_or_with_default() {
    let mut s = FieldSpec::new(FieldType::Secret, "tok", "Token");
    assert!(validate_field_specs(std::slice::from_ref(&s), FieldContext::Options).is_err());
    assert!(validate_field_specs(std::slice::from_ref(&s), FieldContext::Content).is_err());

    s.default = Some(Value::String("plaintext".to_owned()));
    assert!(validate_field_specs(std::slice::from_ref(&s), FieldContext::OwnerSettings).is_err());

    let mut g = FieldSpec::new(FieldType::Group, "grp", "Group");
    g.fields = vec![FieldSpec::new(FieldType::Secret, "tok", "Token")];
    assert!(validate_field_specs(&[g], FieldContext::OwnerSettings).is_err());
}

#[test]
fn forbids_secret_ref_subkey_inside_any_group() {
    let mut g = FieldSpec::new(FieldType::Group, "grp", "Group");
    g.fields = vec![FieldSpec::new(FieldType::Text, "secret_ref", "Bad Subkey")];
    assert!(validate_field_specs(&[g], FieldContext::Options).is_err());
}

#[test]
fn forbids_sync_entry_terms_in_wave_1() {
    let mut tr = FieldSpec::new(FieldType::TermRef, "cats", "Categories");
    tr.taxonomy = Some("category".to_owned());
    tr.sync_entry_terms = true;
    let err = validate_field_specs(&[tr], FieldContext::Content).unwrap_err();
    assert!(err.contains("Đợt 2"));
}

#[test]
fn forbids_bindable_unknown_json_property() {
    let json = r#"{"type":"text","key":"title","label":"Title","bindable":true}"#;
    assert!(serde_json::from_str::<FieldSpec>(json).is_err());
}

#[test]
fn spacing_padding_vs_margin_bounds_and_tailwind_classes() {
    let mut pad = FieldSpec::new(FieldType::Spacing, "pad", "Padding");
    pad.min = Some(-4.0);
    assert!(validate_field_specs(&[pad], FieldContext::StudioBlock).is_err());

    let mut margin = FieldSpec::new(FieldType::Spacing, "mar", "Margin");
    margin.property = Some(SpacingProperty::Margin);
    margin.min = Some(-64.0);
    margin.max = Some(128.0);
    assert!(validate_field_specs(&[margin], FieldContext::StudioBlock).is_ok());

    let mut margin_oob = FieldSpec::new(FieldType::Spacing, "mar", "Margin");
    margin_oob.property = Some(SpacingProperty::Margin);
    margin_oob.min = Some(-300.0);
    assert!(validate_field_specs(&[margin_oob], FieldContext::StudioBlock).is_err());

    let val = SpacingValue {
        top: Some(-16.0),
        right: Some(8.0),
        bottom: Some(24.0),
        left: Some(0.0),
    };
    assert_eq!(
        val.format_classes(SpacingProperty::Margin, CssUnit::Px),
        "mr-2 ml-0 -mt-4 mb-6"
    );
    let pad_val = SpacingValue {
        top: Some(16.0),
        right: Some(8.0),
        bottom: Some(24.0),
        left: Some(0.0),
    };
    assert_eq!(
        pad_val.format_classes(SpacingProperty::Padding, CssUnit::Px),
        "pr-2 pl-0 pt-4 pb-6"
    );
    assert_eq!(
        pad_val.format_classes(SpacingProperty::Padding, CssUnit::Rem),
        ""
    );
}

#[test]
fn gallery_forbids_file_media_kind() {
    let mut g = FieldSpec::new(FieldType::Gallery, "docs", "Docs");
    g.media_kind = Some(MediaFieldKind::File);
    assert!(validate_field_specs(&[g], FieldContext::Content).is_err());
}

#[test]
fn select_options_source_constraints() {
    let mut s = FieldSpec::new(FieldType::Select, "menu", "Menu");
    s.options_source = Some(SelectOptionsSource::Menus);
    assert!(validate_field_specs(std::slice::from_ref(&s), FieldContext::StudioBlock).is_ok());
    assert!(validate_field_specs(std::slice::from_ref(&s), FieldContext::Content).is_err());

    let mut with_opts = s.clone();
    with_opts.options = vec![opt("a", "A")];
    assert!(validate_field_specs(&[with_opts], FieldContext::StudioBlock).is_err());

    let mut with_resp = s.clone();
    with_resp.responsive = true;
    assert!(validate_field_specs(&[with_resp], FieldContext::StudioBlock).is_err());

    let mut with_css = s.clone();
    with_css.css_var = Some("--od-menu".to_owned());
    assert!(validate_field_specs(&[with_css], FieldContext::StudioBlock).is_err());

    let mut rep = FieldSpec::new(FieldType::Repeater, "rep", "Repeater");
    rep.fields = vec![s];
    assert!(validate_field_specs(&[rep], FieldContext::StudioBlock).is_err());
}

#[test]
fn entry_ref_wildcard_rules() {
    let mut e = FieldSpec::new(FieldType::EntryRef, "items", "Items");
    e.target_types = vec!["*".to_owned()];
    assert!(validate_field_specs(std::slice::from_ref(&e), FieldContext::StudioBlock).is_ok());
    assert!(validate_field_specs(std::slice::from_ref(&e), FieldContext::Content).is_err());
    assert!(validate_field_specs(std::slice::from_ref(&e), FieldContext::Options).is_err());

    e.target_types = vec!["*".to_owned(), "post".to_owned()];
    assert!(validate_field_specs(std::slice::from_ref(&e), FieldContext::StudioBlock).is_err());

    e.target_types = vec!["pattern".to_owned()];
    assert!(validate_field_specs(&[e], FieldContext::StudioBlock).is_err());
}

#[test]
fn term_ref_taxonomy_from_field_rules() {
    let mut tax_sel = FieldSpec::new(FieldType::Select, "tax_field", "Taxonomy");
    tax_sel.options_source = Some(SelectOptionsSource::Taxonomies);
    let mut tr = FieldSpec::new(FieldType::TermRef, "terms", "Terms");
    tr.taxonomy_from_field = Some("tax_field".to_owned());
    assert!(
        validate_field_specs(&[tax_sel.clone(), tr.clone()], FieldContext::StudioBlock).is_ok()
    );
    assert!(validate_field_specs(&[tax_sel.clone(), tr.clone()], FieldContext::Content).is_err());

    // mutually exclusive with taxonomy
    let mut both = tr.clone();
    both.taxonomy = Some("category".to_owned());
    assert!(validate_field_specs(&[tax_sel.clone(), both], FieldContext::StudioBlock).is_err());

    // target must not be responsive
    let mut resp_sel = FieldSpec::new(FieldType::Select, "tax_field", "Taxonomy");
    resp_sel.options = vec![opt("category", "Cat")];
    resp_sel.responsive = true;
    assert!(validate_field_specs(&[resp_sel, tr.clone()], FieldContext::StudioBlock).is_err());

    // target must not be multiple
    let mut mult_sel = tax_sel.clone();
    mult_sel.multiple = true;
    assert!(validate_field_specs(&[mult_sel, tr.clone()], FieldContext::StudioBlock).is_err());

    // forbidden inside repeater
    let mut rep = FieldSpec::new(FieldType::Repeater, "rep", "Rep");
    rep.fields = vec![tr];
    assert!(validate_field_specs(&[tax_sel, rep], FieldContext::StudioBlock).is_err());
}

#[test]
fn normalizes_acl_and_enforces_m7_s09_invariants() {
    let f = FieldSpec::new(FieldType::Text, "k", "L");
    let mut user_specs = vec![f.clone()];
    normalize_field_specs_for_scope(
        &mut user_specs,
        FieldContext::Content,
        FieldScopeKind::UserFieldGroup,
    )
    .unwrap();
    let acl = user_specs[0].acl.as_ref().unwrap();
    assert_eq!(acl.public_exposure, Some(PublicExposure::Public));
    assert_eq!(acl.min_view_role, Some(FieldRole::Editor));
    assert_eq!(acl.min_edit_role, Some(FieldRole::Editor));

    let mut term_panel = vec![f.clone()];
    normalize_field_specs_for_scope(
        &mut term_panel,
        FieldContext::Term,
        FieldScopeKind::TermPanelField,
    )
    .unwrap();
    assert_eq!(
        term_panel[0].acl.as_ref().unwrap().public_exposure,
        Some(PublicExposure::Private)
    );

    let mut pub_term_panel = vec![f.clone()];
    pub_term_panel[0].acl = Some(FieldAcl::canonical(
        FieldRole::Editor,
        FieldRole::Editor,
        PublicExposure::Public,
    ));
    assert!(
        normalize_field_specs_for_scope(
            &mut pub_term_panel,
            FieldContext::Term,
            FieldScopeKind::TermPanelField
        )
        .is_err()
    );

    // M7-S09 #1: min_edit_role < min_view_role rejected
    let mut bad1 = vec![f.clone()];
    bad1[0].acl = Some(FieldAcl::canonical(
        FieldRole::Admin,
        FieldRole::Editor,
        PublicExposure::Private,
    ));
    assert!(
        normalize_field_specs_for_scope(
            &mut bad1,
            FieldContext::Content,
            FieldScopeKind::UserFieldGroup
        )
        .is_err()
    );

    // M7-S09 #2: Public with Admin view rejected
    let mut bad2 = vec![f.clone()];
    bad2[0].acl = Some(FieldAcl::canonical(
        FieldRole::Admin,
        FieldRole::Admin,
        PublicExposure::Public,
    ));
    assert!(
        normalize_field_specs_for_scope(
            &mut bad2,
            FieldContext::Content,
            FieldScopeKind::UserFieldGroup
        )
        .is_err()
    );

    // M7-S09 #4: required + Admin edit requires non-empty default
    let mut bad4 = vec![f.clone()];
    bad4[0].required = true;
    bad4[0].acl = Some(FieldAcl::canonical(
        FieldRole::Editor,
        FieldRole::Admin,
        PublicExposure::Public,
    ));
    assert!(
        normalize_field_specs_for_scope(
            &mut bad4,
            FieldContext::Content,
            FieldScopeKind::UserFieldGroup
        )
        .is_err()
    );

    let mut ok4 = vec![f];
    ok4[0].required = true;
    ok4[0].default = Some(Value::String("approved".to_owned()));
    ok4[0].acl = Some(FieldAcl::canonical(
        FieldRole::Editor,
        FieldRole::Admin,
        PublicExposure::Public,
    ));
    assert!(
        normalize_field_specs_for_scope(
            &mut ok4,
            FieldContext::Content,
            FieldScopeKind::UserFieldGroup
        )
        .is_ok()
    );
}

#[test]
fn conditions_dfs_cycle_and_acl_oracle_rejected() {
    let mut a = FieldSpec::new(FieldType::Text, "a", "A");
    let mut b = FieldSpec::new(FieldType::Text, "b", "B");
    a.conditions = Some(vec![vec![rule("b", ConditionOp::NotEmpty, None)]]);
    b.conditions = Some(vec![vec![rule("a", ConditionOp::NotEmpty, None)]]);
    assert!(validate_field_specs(&[a, b], FieldContext::Content).is_err());

    let mut secret_admin = FieldSpec::new(FieldType::Text, "admin_flag", "Admin Flag");
    secret_admin.acl = Some(FieldAcl::canonical(
        FieldRole::Admin,
        FieldRole::Admin,
        PublicExposure::Private,
    ));
    let mut editor_dep = FieldSpec::new(FieldType::Text, "public_hint", "Hint");
    editor_dep.acl = Some(FieldAcl::canonical(
        FieldRole::Editor,
        FieldRole::Editor,
        PublicExposure::Public,
    ));
    editor_dep.conditions = Some(vec![vec![rule(
        "admin_flag",
        ConditionOp::Eq,
        Some(Value::String("yes".to_owned())),
    )]]);
    assert!(validate_field_specs(&[secret_admin, editor_dep], FieldContext::Content).is_err());
}
