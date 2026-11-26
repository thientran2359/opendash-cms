use super::*;

#[test]
fn conditions_evaluate_dnf_and_datetime_time_and_cascade() {
    let mut ts = FieldSpec::new(FieldType::Datetime, "publish_at", "Publish At");
    ts.default = Some(Value::String("2026-11-26T10:00:00.000Z".to_owned()));
    let mut tm = FieldSpec::new(FieldType::Time, "start_time", "Start Time");
    tm.default = Some(Value::String("09:30".to_owned()));
    let mut flag_a = FieldSpec::new(FieldType::Boolean, "a", "A");
    flag_a.default = Some(Value::Bool(false));
    let mut b = FieldSpec::new(FieldType::Text, "b", "B");
    b.conditions = Some(vec![vec![rule(
        "a",
        ConditionOp::Eq,
        Some(Value::Bool(true)),
    )]]);
    let mut c = FieldSpec::new(FieldType::Text, "c", "C");
    c.conditions = Some(vec![vec![
        rule(
            "b",
            ConditionOp::Contains,
            Some(Value::String("keep".to_owned())),
        ),
        rule(
            "publish_at",
            ConditionOp::Gt,
            Some(Value::String("2026-11-26T09:59:59Z".to_owned())),
        ),
    ]]);
    let mut d = FieldSpec::new(FieldType::Text, "d", "D");
    d.conditions = Some(vec![vec![rule(
        "start_time",
        ConditionOp::Lt,
        Some(Value::String("09:30:01".to_owned())),
    )]]);
    let specs = vec![
        ts.clone(),
        tm.clone(),
        flag_a.clone(),
        b.clone(),
        c.clone(),
        d.clone(),
    ];
    assert!(validate_field_specs(&specs, FieldContext::Options).is_ok());

    let mut vals = BTreeMap::new();
    vals.insert("a".to_owned(), Value::Bool(false));
    vals.insert("b".to_owned(), Value::String("keep_value".to_owned()));
    vals.insert(
        "publish_at".to_owned(),
        Value::String("2026-11-26T10:00:00Z".to_owned()),
    );
    vals.insert(
        "start_time".to_owned(),
        Value::String("09:30:00".to_owned()),
    );

    // B is hidden because a == false, yet B's preserved value is still read by C!
    assert!(!evaluate_conditions(
        b.conditions.as_ref(),
        &specs,
        &vals,
        &specs,
        &vals
    ));
    assert!(evaluate_conditions(
        c.conditions.as_ref(),
        &specs,
        &vals,
        &specs,
        &vals
    ));
    assert!(evaluate_conditions(
        d.conditions.as_ref(),
        &specs,
        &vals,
        &specs,
        &vals
    ));

    // `.000Z` vs `Z` equal -> Gt must be false
    let eq_cond = vec![vec![rule(
        "publish_at",
        ConditionOp::Gt,
        Some(Value::String("2026-11-26T10:00:00.000Z".to_owned())),
    )]];
    assert!(!evaluate_conditions(
        Some(&eq_cond),
        &specs,
        &vals,
        &specs,
        &vals
    ));
}

#[test]
fn hidden_required_skipped_on_save_but_xss_validated() {
    let toggle = FieldSpec::new(FieldType::Boolean, "show", "Show");
    let mut detail = FieldSpec::new(FieldType::Url, "detail_url", "Detail URL");
    detail.required = true;
    detail.conditions = Some(vec![vec![rule(
        "show",
        ConditionOp::Eq,
        Some(Value::Bool(true)),
    )]]);
    let specs = vec![toggle, detail];

    let mut vals = BTreeMap::new();
    vals.insert("show".to_owned(), Value::Bool(false));
    assert!(validate_field_values(&specs, FieldContext::Content, &vals, None).is_ok());

    // Hidden field updated with unsafe javascript: URL must still fail!
    vals.insert(
        "detail_url".to_owned(),
        Value::String("javascript:alert(1)".to_owned()),
    );
    assert!(validate_field_values(&specs, FieldContext::Content, &vals, None).is_err());
}

#[test]
fn canonicalizes_negative_zero_and_compat_matrix() {
    let mut n = FieldSpec::new(FieldType::Number, "n", "Number");
    n.min = Some(-0.0);
    n.default = Some(serde_json::json!(-0.0));
    let mut list = vec![n];
    canonicalize_field_specs(&mut list);
    assert!(list[0].min.unwrap().is_sign_positive());
    assert_eq!(
        serde_json::to_value(list[0].default.as_ref()).unwrap(),
        serde_json::json!(0.0)
    );

    let mut f_text = FieldSpec::new(FieldType::Text, "title", "Title");
    f_text.acl = Some(FieldAcl::canonical(
        FieldRole::Editor,
        FieldRole::Editor,
        PublicExposure::Public,
    ));
    let mut f_textarea = FieldSpec::new(FieldType::Textarea, "title", "Title");
    f_textarea.acl = f_text.acl.clone();
    assert!(
        check_schema_compatibility(
            std::slice::from_ref(&f_text),
            std::slice::from_ref(&f_textarea),
            true
        )
        .is_ok()
    );
    assert!(
        check_schema_compatibility(
            std::slice::from_ref(&f_textarea),
            std::slice::from_ref(&f_text),
            true
        )
        .is_err()
    );

    // Removing key with existing data fails
    assert!(check_schema_compatibility(std::slice::from_ref(&f_text), &[], true).is_err());

    // UI-only changes are compatible even with existing data
    let msg = {
        let mut m = FieldSpec::new(FieldType::Message, "m", "Note");
        m.body = Some("Info".to_owned());
        m
    };
    assert!(check_schema_compatibility(&[f_text.clone()], &[msg, f_text.clone()], true).is_ok());

    // Media narrowing blocked, widening allowed
    let mut m_img = FieldSpec::new(FieldType::Media, "cov", "Cover");
    m_img.media_kind = Some(MediaFieldKind::Image);
    let mut m_any = FieldSpec::new(FieldType::Media, "cov", "Cover");
    m_any.media_kind = Some(MediaFieldKind::Any);
    assert!(check_schema_compatibility(&[m_img.clone()], &[m_any.clone()], true).is_ok());
    assert!(check_schema_compatibility(&[m_any], &[m_img], true).is_err());
}

#[test]
fn collects_media_relational_visible_text_and_global_site_options() {
    let globals = global_site_option_fields();
    assert!(validate_field_specs(&globals, FieldContext::Options).is_ok());
    let defaults = global_site_option_defaults();
    assert!(defaults.contains_key("social_facebook"));
    assert!(defaults.contains_key("site_logo"));
    assert!(!defaults.contains_key("acc_social_links"));

    let id1 = "a".repeat(64);
    let id2 = "b".repeat(64);
    let mut m_pub = FieldSpec::new(FieldType::Media, "cover", "Cover");
    m_pub.acl = Some(FieldAcl::canonical(
        FieldRole::Editor,
        FieldRole::Editor,
        PublicExposure::Public,
    ));
    let mut m_priv = FieldSpec::new(FieldType::Media, "priv_doc", "Private");
    m_priv.media_kind = Some(MediaFieldKind::File);
    m_priv.acl = Some(FieldAcl::canonical(
        FieldRole::Editor,
        FieldRole::Editor,
        PublicExposure::Private,
    ));
    let mut er = FieldSpec::new(FieldType::EntryRef, "rel", "Rel");
    er.target_types = vec!["post".to_owned()];
    er.multiple = true;
    let mut grp = FieldSpec::new(FieldType::Group, "g", "G");
    grp.fields = vec![FieldSpec::new(FieldType::Text, "note", "Note")];
    let specs = vec![m_pub, m_priv, er, grp];

    let mut vals = BTreeMap::new();
    vals.insert("cover".to_owned(), Value::String(id1.clone()));
    vals.insert("priv_doc".to_owned(), Value::String(id2.clone()));
    vals.insert("rel".to_owned(), serde_json::json!([11, 22]));
    vals.insert("g".to_owned(), serde_json::json!({"note": "Xin chào"}));

    let all_media = collect_media_ids(&specs, &vals);
    assert!(all_media.image_ids.contains(&id1));
    assert!(all_media.file_ids.contains(&id2));

    let pub_media = collect_public_media_ids(&specs, &vals, true);
    assert!(pub_media.image_ids.contains(&id1));
    assert!(!pub_media.file_ids.contains(&id2));

    let rels = collect_relational_refs(&specs, &vals);
    assert_eq!(rels.entry_ids, BTreeSet::from([11, 22]));

    let mut text = Vec::new();
    extract_visible_text(&specs, &vals, &mut text);
    assert_eq!(text, vec!["Xin chào".to_owned()]);
}

#[test]
fn validates_email_url_date_time_color_safely() {
    assert!(valid_email_address("alice@example.com"));
    assert!(!valid_email_address("alice@localhost"));
    assert!(!valid_email_address("alice@@example.com"));
    assert!(!valid_email_address("alice @example.com"));

    assert!(valid_safe_url("https://example.com/path?a=1"));
    assert!(valid_safe_url("/gioi-thieu"));
    assert!(valid_safe_url("mailto:hello@example.com"));
    assert!(!valid_safe_url("javascript:alert(1)"));
    assert!(!valid_safe_url("//evil.example"));

    assert!(valid_iso_date("2024-02-29"));
    assert!(!valid_iso_date("2025-02-29"));
    assert_eq!(
        parse_utc_datetime_millis("2026-11-26T00:00:00Z"),
        parse_utc_datetime_millis("2026-11-26T00:00:00.000Z")
    );
    assert_eq!(parse_time_seconds("01:02"), parse_time_seconds("01:02:00"));
    assert!(valid_color_value("var(--od-token-primary)"));
    assert!(valid_color_value("#123456aa"));
    assert!(!valid_color_value("rgb(1,2,3)"));
}

#[test]
fn nested_child_condition_reads_top_level_and_hidden_parent_group_skips_required() {
    let toggle = FieldSpec::new(FieldType::Boolean, "enable_grp", "Enable Group");
    let show_sub = FieldSpec::new(FieldType::Boolean, "show_sub", "Show Sub");
    let mut child_req = FieldSpec::new(FieldType::Text, "headline", "Headline");
    child_req.required = true;
    child_req.conditions = Some(vec![vec![rule(
        "show_sub",
        ConditionOp::Eq,
        Some(Value::Bool(true)),
    )]]);
    let mut grp = FieldSpec::new(FieldType::Group, "card", "Card");
    grp.conditions = Some(vec![vec![rule(
        "enable_grp",
        ConditionOp::Eq,
        Some(Value::Bool(true)),
    )]]);
    grp.fields = vec![child_req];
    let specs = vec![toggle, show_sub, grp];
    assert!(validate_field_specs(&specs, FieldContext::StudioBlock).is_ok());

    // Parent group hidden even if show_sub=true -> required child omitted should pass
    let mut vals = BTreeMap::new();
    vals.insert("enable_grp".to_owned(), Value::Bool(false));
    vals.insert("show_sub".to_owned(), Value::Bool(true));
    vals.insert("card".to_owned(), serde_json::json!({}));
    assert!(validate_field_values(&specs, FieldContext::StudioBlock, &vals, None).is_ok());

    // Parent group visible & show_sub=true -> required child missing must fail
    vals.insert("enable_grp".to_owned(), Value::Bool(true));
    assert!(validate_field_values(&specs, FieldContext::StudioBlock, &vals, None).is_err());

    // Parent group visible & show_sub=false -> child hidden by top-level condition -> ok
    vals.insert("show_sub".to_owned(), Value::Bool(false));
    assert!(validate_field_values(&specs, FieldContext::StudioBlock, &vals, None).is_ok());
}

#[test]
fn public_group_does_not_leak_private_nested_media_ids() {
    let pub_id = "a".repeat(64);
    let priv_id = "b".repeat(64);
    let mut pub_m = FieldSpec::new(FieldType::Media, "pub_img", "Public Img");
    pub_m.acl = Some(FieldAcl::canonical(
        FieldRole::Editor,
        FieldRole::Editor,
        PublicExposure::Public,
    ));
    let mut priv_m = FieldSpec::new(FieldType::Media, "priv_img", "Private Img");
    priv_m.acl = Some(FieldAcl::canonical(
        FieldRole::Editor,
        FieldRole::Editor,
        PublicExposure::Private,
    ));
    let mut grp = FieldSpec::new(FieldType::Group, "hero", "Hero");
    grp.acl = Some(FieldAcl::canonical(
        FieldRole::Editor,
        FieldRole::Editor,
        PublicExposure::Public,
    ));
    grp.fields = vec![pub_m, priv_m];
    let specs = vec![grp];

    let mut vals = BTreeMap::new();
    vals.insert(
        "hero".to_owned(),
        serde_json::json!({ "pub_img": pub_id, "priv_img": priv_id }),
    );
    let all = collect_media_ids(&specs, &vals);
    assert_eq!(all.image_ids.len(), 2);
    let public_only = collect_public_media_ids(&specs, &vals, true);
    assert!(public_only.image_ids.contains(&pub_id));
    assert!(!public_only.image_ids.contains(&priv_id));
}

#[test]
fn group_repeater_nesting_rejects_three_levels() {
    let leaf = FieldSpec::new(FieldType::Text, "item", "Item");
    let mut inner_grp = FieldSpec::new(FieldType::Group, "inner_g", "Inner");
    inner_grp.fields = vec![leaf.clone()];
    let mut mid_rep = FieldSpec::new(FieldType::Repeater, "mid_r", "Mid");
    mid_rep.fields = vec![inner_grp.clone()];
    let mut outer_grp = FieldSpec::new(FieldType::Group, "outer_g", "Outer");
    outer_grp.fields = vec![mid_rep.clone()];
    assert!(validate_field_specs(&[outer_grp], FieldContext::Content).is_err());

    // Two levels: top-level Repeater -> flat Group is valid
    assert!(validate_field_specs(&[mid_rep], FieldContext::Content).is_ok());

    // Two levels: top-level Group -> Repeater (primitives) is valid
    let mut ok_rep = FieldSpec::new(FieldType::Repeater, "rows", "Rows");
    ok_rep.fields = vec![leaf];
    let mut ok_outer = FieldSpec::new(FieldType::Group, "outer", "Outer");
    ok_outer.fields = vec![ok_rep];
    assert!(validate_field_specs(&[ok_outer], FieldContext::Content).is_ok());
}

#[test]
fn core_entry_body_accepts_large_plain_text_and_blocks() {
    let body = FieldSpec::new(FieldType::Richtext, "body", "Nội dung");
    let specs = vec![body];
    let big_plain = "<p>".to_owned() + &"a".repeat(70_000) + "</p>";
    let mut vals = BTreeMap::new();
    vals.insert("body".to_owned(), Value::String(big_plain));
    assert!(validate_field_values(&specs, FieldContext::Content, &vals, None).is_ok());

    // Too huge (> 200k chars) rejected
    let too_big = "a".repeat(200_005);
    vals.insert("body".to_owned(), Value::String(too_big));
    assert!(validate_field_values(&specs, FieldContext::Content, &vals, None).is_err());

    // Non-body richtext remains capped at 32 KiB
    let other = FieldSpec::new(FieldType::Richtext, "notes", "Notes");
    let mut other_vals = BTreeMap::new();
    other_vals.insert(
        "notes".to_owned(),
        Value::String("<p>".to_owned() + &"a".repeat(40_000) + "</p>"),
    );
    assert!(validate_field_values(&[other], FieldContext::Content, &other_vals, None).is_err());
}

#[test]
fn spacing_format_classes_delegates_to_shorthand_tailwind() {
    let eq_pad = SpacingValue {
        top: Some(16.0),
        right: Some(16.0),
        bottom: Some(16.0),
        left: Some(16.0),
    };
    assert_eq!(
        eq_pad.format_classes(SpacingProperty::Padding, CssUnit::Px),
        "p-4"
    );

    let sym_mar = SpacingValue {
        top: Some(-16.0),
        right: Some(8.0),
        bottom: Some(-16.0),
        left: Some(8.0),
    };
    assert_eq!(
        sym_mar.format_classes(SpacingProperty::Margin, CssUnit::Px),
        "mx-2 -my-4"
    );
}

#[test]
fn rejects_invalid_snake_keys_and_duplicate_keys() {
    let bad = FieldSpec::new(FieldType::Text, "1bad", "Bad");
    assert!(validate_field_specs(&[bad], FieldContext::Content).is_err());
    let f1 = FieldSpec::new(FieldType::Text, "dup", "One");
    let f2 = FieldSpec::new(FieldType::Text, "dup", "Two");
    assert!(validate_field_specs(&[f1, f2], FieldContext::Content).is_err());
}

#[test]
fn rejects_empty_and_control_and_overlong_labels() {
    let empty = FieldSpec::new(FieldType::Text, "k", "   ");
    assert!(validate_field_specs(&[empty], FieldContext::Content).is_err());
    let ctrl = FieldSpec::new(FieldType::Text, "k", "Hi\x07");
    assert!(validate_field_specs(&[ctrl], FieldContext::Content).is_err());
    let long = FieldSpec::new(FieldType::Text, "k", "a".repeat(101));
    assert!(validate_field_specs(&[long], FieldContext::Content).is_err());
}

#[test]
fn accordion_endpoint_allows_empty_label_only_outside_repeater() {
    let mut acc = FieldSpec::new(FieldType::Accordion, "acc_end", "");
    acc.endpoint = true;
    assert!(validate_field_specs(std::slice::from_ref(&acc), FieldContext::StudioBlock).is_ok());

    let mut rep = FieldSpec::new(FieldType::Repeater, "rep", "Rep");
    rep.fields = vec![acc];
    assert!(validate_field_specs(&[rep], FieldContext::StudioBlock).is_err());
}

#[test]
fn message_requires_plain_non_empty_no_html_body() {
    let mut msg = FieldSpec::new(FieldType::Message, "m", "Note");
    assert!(validate_field_specs(std::slice::from_ref(&msg), FieldContext::Options).is_err());
    msg.body = Some("<b>No</b>".to_owned());
    assert!(validate_field_specs(std::slice::from_ref(&msg), FieldContext::Options).is_err());
    msg.body = Some("Read docs carefully.".to_owned());
    assert!(validate_field_specs(&[msg], FieldContext::Options).is_ok());
}

#[test]
fn validates_width_affixes_and_instructions_limits() {
    let mut f = FieldSpec::new(FieldType::Text, "k", "L");
    f.width = Some(50);
    f.prepend = Some("https://".to_owned());
    f.instructions = Some("Line 1\nLine 2".to_owned());
    assert!(validate_field_specs(std::slice::from_ref(&f), FieldContext::Content).is_ok());

    f.width = Some(40);
    assert!(validate_field_specs(std::slice::from_ref(&f), FieldContext::Content).is_err());
    f.width = Some(50);
    f.prepend = Some("x".repeat(33));
    assert!(validate_field_specs(std::slice::from_ref(&f), FieldContext::Content).is_err());
}

#[test]
fn prepend_and_append_rejected_on_non_affix_types() {
    let mut b = FieldSpec::new(FieldType::Boolean, "flag", "Flag");
    b.prepend = Some("pre".to_owned());
    assert!(validate_field_specs(&[b], FieldContext::Content).is_err());
}

#[test]
fn validates_text_and_textarea_character_bounds() {
    let mut t = FieldSpec::new(FieldType::Text, "t", "T");
    t.max_chars = Some(2001);
    assert!(validate_field_specs(&[t], FieldContext::Content).is_err());

    let mut ta = FieldSpec::new(FieldType::Textarea, "ta", "TA");
    ta.rows = Some(17);
    assert!(validate_field_specs(&[ta], FieldContext::Content).is_err());
}
