use super::*;

#[test]
fn value_equals_previous_canonicalizes_negative_zero() {
    let a = serde_json::json!(-0.0);
    let b = serde_json::json!(0.0);
    assert!(value_equals_previous(Some(&a), Some(&b)));
    assert!(!value_equals_previous(Some(&a), None));
}

#[test]
fn entry_ref_and_term_ref_reject_multiple_when_configured_single() {
    let mut er = FieldSpec::new(FieldType::EntryRef, "rel", "Rel");
    er.target_types = vec!["post".to_owned()];
    er.max_items = Some(3);
    assert!(validate_field_specs(&[er], FieldContext::Content).is_err());

    let mut tr = FieldSpec::new(FieldType::TermRef, "t", "T");
    tr.taxonomy = Some("category".to_owned());
    tr.min_items = Some(2);
    assert!(validate_field_specs(&[tr], FieldContext::Content).is_err());
}

#[test]
fn entry_ref_and_term_ref_value_validation_checks_positive_unique_ids() {
    let mut er = FieldSpec::new(FieldType::EntryRef, "rel", "Rel");
    er.target_types = vec!["post".to_owned()];
    er.multiple = true;
    er.max_items = Some(2);
    let specs = vec![er];

    let mut vals = BTreeMap::new();
    vals.insert("rel".to_owned(), serde_json::json!([1, 1]));
    assert!(validate_field_values(&specs, FieldContext::Content, &vals, None).is_err());

    vals.insert("rel".to_owned(), serde_json::json!([0]));
    assert!(validate_field_values(&specs, FieldContext::Content, &vals, None).is_err());

    vals.insert("rel".to_owned(), serde_json::json!([1, 2]));
    assert!(validate_field_values(&specs, FieldContext::Content, &vals, None).is_ok());
}

#[test]
fn rejects_unknown_fields_in_submitted_values() {
    let specs = vec![FieldSpec::new(FieldType::Text, "title", "Title")];
    let mut vals = BTreeMap::new();
    vals.insert("injected".to_owned(), Value::String("bad".to_owned()));
    assert!(validate_field_values(&specs, FieldContext::Content, &vals, None).is_err());
}

#[test]
fn default_value_for_field_spec_covers_containers_and_primitives() {
    let mut grp = FieldSpec::new(FieldType::Group, "g", "G");
    grp.fields = vec![FieldSpec::new(FieldType::Boolean, "flag", "Flag")];
    assert_eq!(
        default_value_for_field_spec(&grp),
        serde_json::json!({"flag": false})
    );

    let mut rng = FieldSpec::new(FieldType::Range, "r", "R");
    rng.min = Some(5.0);
    rng.max = Some(10.0);
    assert_eq!(default_value_for_field_spec(&rng), serde_json::json!(5.0));
}
