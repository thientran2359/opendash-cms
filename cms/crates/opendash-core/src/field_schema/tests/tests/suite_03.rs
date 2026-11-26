use super::*;

#[test]
fn validates_number_and_range_finiteness_and_steps() {
    let mut r = FieldSpec::new(FieldType::Range, "r", "R");
    r.min = Some(10.0);
    assert!(validate_field_specs(std::slice::from_ref(&r), FieldContext::Content).is_err());
    r.max = Some(5.0);
    assert!(validate_field_specs(std::slice::from_ref(&r), FieldContext::Content).is_err());
    r.max = Some(20.0);
    r.step = Some(0.0);
    assert!(validate_field_specs(&[r], FieldContext::Content).is_err());
}

#[test]
fn rejects_default_on_media_gallery_and_ui_only() {
    let mut m = FieldSpec::new(FieldType::Media, "m", "M");
    m.default = Some(Value::String("a".repeat(64)));
    assert!(validate_field_specs(&[m], FieldContext::Content).is_err());

    let mut g = FieldSpec::new(FieldType::Gallery, "g", "G");
    g.default = Some(serde_json::json!([]));
    assert!(validate_field_specs(&[g], FieldContext::Content).is_err());

    let mut t = FieldSpec::new(FieldType::Tab, "tab", "Tab");
    t.default = Some(Value::String("x".to_owned()));
    assert!(validate_field_specs(&[t], FieldContext::Content).is_err());
}

#[test]
fn link_default_sanitizes_url_and_label_length() {
    let mut l = FieldSpec::new(FieldType::Link, "cta", "CTA");
    l.default = Some(serde_json::json!({"url": "javascript:evil()", "label": "Click"}));
    assert!(validate_field_specs(std::slice::from_ref(&l), FieldContext::Content).is_err());

    l.default = Some(serde_json::json!({"url": "/ok", "label": "x".repeat(161)}));
    assert!(validate_field_specs(&[l], FieldContext::Content).is_err());
}

#[test]
fn checkbox_and_radio_layouts_and_option_caps() {
    let mut cb = FieldSpec::new(FieldType::Checkbox, "cb", "CB");
    cb.options = vec![opt("a", "A")];
    cb.layout = Some("table".to_owned());
    assert!(validate_field_specs(&[cb], FieldContext::Content).is_err());

    let mut rb = FieldSpec::new(FieldType::RadioButtons, "rb", "RB");
    rb.options = (0..13)
        .map(|i| opt(&format!("v{i}"), &format!("V{i}")))
        .collect();
    assert!(validate_field_specs(&[rb], FieldContext::Content).is_err());
}

#[test]
fn icon_requires_valid_core_icon_key() {
    let mut ic = FieldSpec::new(FieldType::Icon, "ic", "Icon");
    ic.default = Some(Value::String("nonexistent-icon".to_owned()));
    assert!(validate_field_specs(std::slice::from_ref(&ic), FieldContext::Content).is_err());
    ic.default = Some(Value::String("star".to_owned()));
    assert!(validate_field_specs(&[ic], FieldContext::Content).is_ok());
}

#[test]
fn date_min_max_bounds_checked_in_schema_and_values() {
    let mut d = FieldSpec::new(FieldType::Date, "ev", "Event");
    d.min_date = Some("2026-05-01".to_owned());
    d.max_date = Some("2026-04-01".to_owned());
    assert!(validate_field_specs(std::slice::from_ref(&d), FieldContext::Content).is_err());

    d.max_date = Some("2026-12-31".to_owned());
    assert!(validate_field_specs(std::slice::from_ref(&d), FieldContext::Content).is_ok());

    let mut vals = BTreeMap::new();
    vals.insert("ev".to_owned(), Value::String("2026-01-01".to_owned()));
    assert!(validate_field_values(&[d], FieldContext::Content, &vals, None).is_err());
}

#[test]
fn spacing_rejected_in_content_term_and_owner_settings() {
    let sp = FieldSpec::new(FieldType::Spacing, "sp", "Spacing");
    assert!(validate_field_specs(std::slice::from_ref(&sp), FieldContext::Content).is_err());
    assert!(validate_field_specs(std::slice::from_ref(&sp), FieldContext::Term).is_err());
    assert!(validate_field_specs(std::slice::from_ref(&sp), FieldContext::OwnerSettings).is_err());
    assert!(validate_field_specs(std::slice::from_ref(&sp), FieldContext::Options).is_ok());
    assert!(validate_field_specs(&[sp], FieldContext::StudioBlock).is_ok());
}

#[test]
fn context_attribute_isolation_blocks_misplaced_flags() {
    let mut f_acl = FieldSpec::new(FieldType::Text, "a", "A");
    f_acl.acl = Some(FieldAcl::canonical(
        FieldRole::Editor,
        FieldRole::Editor,
        PublicExposure::Private,
    ));
    assert!(validate_field_specs(&[f_acl], FieldContext::Options).is_err());

    let mut f_pub = FieldSpec::new(FieldType::Text, "p", "P");
    f_pub.public = true;
    assert!(validate_field_specs(&[f_pub], FieldContext::Content).is_err());

    let mut f_trans = FieldSpec::new(FieldType::Text, "tr", "TR");
    f_trans.translatable = true;
    assert!(validate_field_specs(std::slice::from_ref(&f_trans), FieldContext::Content).is_err());
    assert!(validate_field_specs(&[f_trans], FieldContext::Options).is_ok());

    let mut f_list = FieldSpec::new(FieldType::Richtext, "rt", "RT");
    f_list.show_in_list = true;
    assert!(validate_field_specs(&[f_list], FieldContext::Content).is_err());
}

#[test]
fn studio_block_css_var_rules_and_uniqueness() {
    let mut a = FieldSpec::new(FieldType::Color, "c1", "Color 1");
    a.css_var = Some("--od-brand-bg".to_owned());
    let mut b = FieldSpec::new(FieldType::Color, "c2", "Color 2");
    b.css_var = Some("--od-brand-bg".to_owned());
    assert!(validate_field_specs(&[a.clone(), b], FieldContext::StudioBlock).is_err());

    let mut bad_media = FieldSpec::new(FieldType::Media, "vid", "Video");
    bad_media.media_kind = Some(MediaFieldKind::Video);
    bad_media.css_var = Some("--od-video-url".to_owned());
    assert!(validate_field_specs(&[bad_media], FieldContext::StudioBlock).is_err());

    let mut bad_sel = FieldSpec::new(FieldType::Select, "s", "Select");
    bad_sel.options = vec![opt("Bad Value!", "Bad")];
    bad_sel.css_var = Some("--od-sel".to_owned());
    assert!(validate_field_specs(&[bad_sel], FieldContext::StudioBlock).is_err());
}

#[test]
fn responsive_flag_restricted_to_top_level_allowed_types() {
    let mut ok_n = FieldSpec::new(FieldType::Number, "cols", "Cols");
    ok_n.responsive = true;
    assert!(validate_field_specs(std::slice::from_ref(&ok_n), FieldContext::StudioBlock).is_ok());

    let mut nested_g = FieldSpec::new(FieldType::Group, "g", "G");
    nested_g.fields = vec![ok_n];
    assert!(validate_field_specs(&[nested_g], FieldContext::StudioBlock).is_err());

    let mut bad_text = FieldSpec::new(FieldType::Text, "t", "T");
    bad_text.responsive = true;
    assert!(validate_field_specs(&[bad_text], FieldContext::StudioBlock).is_err());
}

#[test]
fn responsive_value_validation_checks_desktop_tablet_mobile() {
    let mut cols = FieldSpec::new(FieldType::Number, "cols", "Cols");
    cols.responsive = true;
    cols.min = Some(1.0);
    cols.max = Some(6.0);
    let specs = vec![cols];

    let mut vals = BTreeMap::new();
    vals.insert(
        "cols".to_owned(),
        serde_json::json!({"desktop": 4, "mobile": 1}),
    );
    assert!(validate_field_values(&specs, FieldContext::StudioBlock, &vals, None).is_ok());

    vals.insert("cols".to_owned(), serde_json::json!({"desktop": 12}));
    assert!(validate_field_values(&specs, FieldContext::StudioBlock, &vals, None).is_err());

    vals.insert("cols".to_owned(), serde_json::json!({"ultrawide": 4}));
    assert!(validate_field_values(&specs, FieldContext::StudioBlock, &vals, None).is_err());
}

#[test]
fn studio_block_field_count_and_repeater_budget_caps() {
    let too_many_top: Vec<FieldSpec> = (0..33)
        .map(|i| FieldSpec::new(FieldType::Text, format!("f_{i}"), format!("F{i}")))
        .collect();
    assert!(validate_field_specs(&too_many_top, FieldContext::StudioBlock).is_err());

    let mut r1 = FieldSpec::new(FieldType::Repeater, "r1", "R1");
    r1.max_items = Some(32);
    r1.fields = vec![FieldSpec::new(FieldType::Text, "a", "A")];
    let mut r2 = FieldSpec::new(FieldType::Repeater, "r2", "R2");
    r2.max_items = Some(32);
    r2.fields = vec![FieldSpec::new(FieldType::Text, "b", "B")];
    let mut r3 = FieldSpec::new(FieldType::Repeater, "r3", "R3");
    r3.max_items = Some(1);
    r3.fields = vec![FieldSpec::new(FieldType::Text, "c", "C")];
    assert!(validate_field_specs(&[r1.clone(), r2.clone()], FieldContext::StudioBlock).is_ok());
    assert!(validate_field_specs(&[r1, r2, r3], FieldContext::StudioBlock).is_err());
}

#[test]
fn studio_block_leaf_field_cap_48_enforced_across_groups() {
    let mut groups = Vec::new();
    for g_idx in 0..3 {
        let mut g = FieldSpec::new(FieldType::Group, format!("g_{g_idx}"), format!("G{g_idx}"));
        g.fields = (0..17)
            .map(|i| FieldSpec::new(FieldType::Text, format!("leaf_{i}"), format!("L{i}")))
            .collect();
        groups.push(g);
    }
    // 3 * 17 = 51 leaves > 48
    assert!(validate_field_specs(&groups, FieldContext::StudioBlock).is_err());
}

#[test]
fn non_block_schema_cap_64_enforced() {
    let too_many: Vec<FieldSpec> = (0..65)
        .map(|i| FieldSpec::new(FieldType::Text, format!("k_{i}"), format!("K{i}")))
        .collect();
    assert!(validate_field_specs(&too_many, FieldContext::Content).is_err());
}

#[test]
fn dependency_dfs_detects_transitive_3_node_cycle() {
    let mut a = FieldSpec::new(FieldType::Text, "a", "A");
    let mut b = FieldSpec::new(FieldType::Text, "b", "B");
    let mut c = FieldSpec::new(FieldType::Text, "c", "C");
    a.conditions = Some(vec![vec![rule("b", ConditionOp::NotEmpty, None)]]);
    b.conditions = Some(vec![vec![rule("c", ConditionOp::NotEmpty, None)]]);
    c.conditions = Some(vec![vec![rule("a", ConditionOp::NotEmpty, None)]]);
    assert!(validate_field_specs(&[a, b, c], FieldContext::Options).is_err());
}

#[test]
fn conditions_rejects_self_ref_group_spacing_secret_and_responsive_targets() {
    let mut self_f = FieldSpec::new(FieldType::Text, "s", "S");
    self_f.conditions = Some(vec![vec![rule("s", ConditionOp::NotEmpty, None)]]);
    assert!(validate_field_specs(&[self_f], FieldContext::Content).is_err());

    let mut sp = FieldSpec::new(FieldType::Spacing, "pad", "Pad");
    sp.key = "pad".to_owned();
    let mut dep = FieldSpec::new(FieldType::Text, "dep", "Dep");
    dep.conditions = Some(vec![vec![rule("pad", ConditionOp::Empty, None)]]);
    assert!(validate_field_specs(&[sp, dep], FieldContext::StudioBlock).is_err());
}

#[test]
fn conditions_evaluate_all_8_operators() {
    let mut sel = FieldSpec::new(FieldType::Select, "mode", "Mode");
    sel.options = vec![opt("a", "A"), opt("b", "B"), opt("c", "C")];
    let num = FieldSpec::new(FieldType::Number, "count", "Count");
    let txt = FieldSpec::new(FieldType::Text, "note", "Note");
    let mut chk = FieldSpec::new(FieldType::Checkbox, "tags", "Tags");
    chk.options = vec![opt("x", "X"), opt("y", "Y")];
    let specs = vec![sel, num, txt, chk];

    let mut vals = BTreeMap::new();
    vals.insert("mode".to_owned(), Value::String("b".to_owned()));
    vals.insert("count".to_owned(), serde_json::json!(15));
    vals.insert("note".to_owned(), Value::String("hello world".to_owned()));
    vals.insert("tags".to_owned(), serde_json::json!(["x"]));

    assert!(evaluate_conditions(
        Some(&vec![vec![rule(
            "mode",
            ConditionOp::Eq,
            Some(Value::String("b".into()))
        )]]),
        &specs,
        &vals,
        &specs,
        &vals
    ));
    assert!(evaluate_conditions(
        Some(&vec![vec![rule(
            "mode",
            ConditionOp::Neq,
            Some(Value::String("a".into()))
        )]]),
        &specs,
        &vals,
        &specs,
        &vals
    ));
    assert!(evaluate_conditions(
        Some(&vec![vec![rule(
            "mode",
            ConditionOp::In,
            Some(serde_json::json!(["b", "c"]))
        )]]),
        &specs,
        &vals,
        &specs,
        &vals
    ));
    assert!(evaluate_conditions(
        Some(&vec![vec![rule(
            "tags",
            ConditionOp::In,
            Some(serde_json::json!(["x", "z"]))
        )]]),
        &specs,
        &vals,
        &specs,
        &vals
    ));
    assert!(evaluate_conditions(
        Some(&vec![vec![rule(
            "tags",
            ConditionOp::Contains,
            Some(Value::String("x".into()))
        )]]),
        &specs,
        &vals,
        &specs,
        &vals
    ));
    assert!(evaluate_conditions(
        Some(&vec![vec![rule("note", ConditionOp::NotEmpty, None)]]),
        &specs,
        &vals,
        &specs,
        &vals
    ));
    assert!(evaluate_conditions(
        Some(&vec![vec![rule(
            "count",
            ConditionOp::Gt,
            Some(serde_json::json!(10))
        )]]),
        &specs,
        &vals,
        &specs,
        &vals
    ));
    assert!(evaluate_conditions(
        Some(&vec![vec![rule(
            "count",
            ConditionOp::Lt,
            Some(serde_json::json!(20))
        )]]),
        &specs,
        &vals,
        &specs,
        &vals
    ));

    vals.insert("note".to_owned(), Value::String("   ".to_owned()));
    assert!(evaluate_conditions(
        Some(&vec![vec![rule("note", ConditionOp::Empty, None)]]),
        &specs,
        &vals,
        &specs,
        &vals
    ));
}

#[test]
fn compat_blocks_flipping_multiple_and_deleting_options_with_data() {
    let mut s_single = FieldSpec::new(FieldType::Select, "cat", "Cat");
    s_single.options = vec![opt("a", "A"), opt("b", "B")];
    let mut s_multi = s_single.clone();
    s_multi.multiple = true;
    assert!(
        check_schema_compatibility(
            std::slice::from_ref(&s_single),
            std::slice::from_ref(&s_multi),
            true
        )
        .is_err()
    );

    let mut s_drop = s_single.clone();
    s_drop.options = vec![opt("a", "A")];
    assert!(check_schema_compatibility(&[s_single], &[s_drop], true).is_err());
}

#[test]
fn compat_blocks_optional_to_required_and_new_required_with_data() {
    let f_opt = FieldSpec::new(FieldType::Text, "a", "A");
    let mut f_req = f_opt.clone();
    f_req.required = true;
    assert!(
        check_schema_compatibility(
            std::slice::from_ref(&f_opt),
            std::slice::from_ref(&f_req),
            true
        )
        .is_err()
    );
    assert!(check_schema_compatibility(&[], std::slice::from_ref(&f_req), true).is_err());

    // Withdrawn required is allowed (effective_required == false)
    f_req.declaration_state = Some(FieldDeclarationState::Withdrawn);
    assert!(check_schema_compatibility(&[f_opt], &[f_req], true).is_ok());
}

#[test]
fn compat_allows_moving_fields_between_tabs_and_accordions() {
    let tab = FieldSpec::new(FieldType::Tab, "t1", "Tab 1");
    let f = FieldSpec::new(FieldType::Text, "title", "Title");
    assert!(check_schema_compatibility(&[f.clone()], &[tab, f], true).is_ok());
}

#[test]
fn compat_recurses_into_nested_group_subfields() {
    let mut g1 = FieldSpec::new(FieldType::Group, "g", "G");
    g1.fields = vec![FieldSpec::new(FieldType::Text, "sub", "Sub")];
    let mut g2 = FieldSpec::new(FieldType::Group, "g", "G");
    g2.fields = vec![FieldSpec::new(FieldType::Number, "sub", "Sub")];
    assert!(check_schema_compatibility(&[g1], &[g2], true).is_err());
}

#[test]
fn collect_media_recurses_through_repeater_and_nested_group() {
    let img1 = "c".repeat(64);
    let vid1 = "d".repeat(64);
    let mut m_img = FieldSpec::new(FieldType::Media, "thumb", "Thumb");
    m_img.media_kind = Some(MediaFieldKind::Image);
    let mut gal = FieldSpec::new(FieldType::Gallery, "vids", "Videos");
    gal.media_kind = Some(MediaFieldKind::Video);
    let mut rep = FieldSpec::new(FieldType::Repeater, "cards", "Cards");
    rep.fields = vec![m_img, gal];
    let specs = vec![rep];

    let mut vals = BTreeMap::new();
    vals.insert(
        "cards".to_owned(),
        serde_json::json!([{"thumb": img1, "vids": [vid1]}]),
    );
    let collected = collect_media_ids(&specs, &vals);
    assert!(collected.image_ids.contains(&img1));
    assert!(collected.video_ids.contains(&vid1));
}

#[test]
fn collect_relational_refs_recurses_through_group_and_repeater() {
    let mut er = FieldSpec::new(FieldType::EntryRef, "rel", "Rel");
    er.target_types = vec!["post".to_owned()];
    let mut tr = FieldSpec::new(FieldType::TermRef, "cat", "Cat");
    tr.taxonomy = Some("category".to_owned());
    tr.multiple = true;
    let mut grp = FieldSpec::new(FieldType::Group, "meta", "Meta");
    grp.fields = vec![er];
    let mut rep = FieldSpec::new(FieldType::Repeater, "sections", "Sections");
    rep.fields = vec![tr];
    let specs = vec![grp, rep];

    let mut vals = BTreeMap::new();
    vals.insert("meta".to_owned(), serde_json::json!({"rel": 42}));
    vals.insert("sections".to_owned(), serde_json::json!([{"cat": [7, 9]}]));
    let refs = collect_relational_refs(&specs, &vals);
    assert_eq!(refs.entry_ids, BTreeSet::from([42]));
    assert_eq!(refs.term_ids, BTreeSet::from([7, 9]));
}

#[test]
fn extract_visible_text_ignores_secrets_and_reads_links() {
    let s = FieldSpec::new(FieldType::Secret, "api_key", "API Key");
    let l = FieldSpec::new(FieldType::Link, "cta", "CTA");
    let mut rep = FieldSpec::new(FieldType::Repeater, "items", "Items");
    rep.fields = vec![FieldSpec::new(FieldType::Text, "t", "T")];
    let specs = vec![s, l, rep];

    let mut vals = BTreeMap::new();
    vals.insert(
        "api_key".to_owned(),
        Value::String("super-secret-token".to_owned()),
    );
    vals.insert(
        "cta".to_owned(),
        serde_json::json!({"url": "/go", "label": "Learn More"}),
    );
    vals.insert("items".to_owned(), serde_json::json!([{"t": "Row 1"}]));
    let mut out = Vec::new();
    extract_visible_text(&specs, &vals, &mut out);
    assert_eq!(out, vec!["Learn More".to_owned(), "Row 1".to_owned()]);
}
