//! M4-S12-T01 / #374 C2 canonical taxonomy definitions and input contracts.
//! These focused domain tests complement live native API/SQLite/Chrome checks.
use opendash_core::content::{TaxonomyDefinitionInput, TaxonomyDefinitionPatch};
use serde_json::json;

#[test]
fn custom_hierarchical_and_flat_definitions_have_canonical_defaults() {
    let tree: TaxonomyDefinitionInput = serde_json::from_value(json!({
        "key":"genre", "label":"Genre", "hierarchical":true
    }))
    .unwrap();
    assert!(tree.public);
    assert!(tree.hierarchical);
    tree.validate().unwrap();
    let flat: TaxonomyDefinitionInput = serde_json::from_value(json!({
        "key":"brand", "label":"Brand"
    }))
    .unwrap();
    assert!(!flat.hierarchical);
    assert!(flat.public);
    flat.validate().unwrap();
}

#[test]
fn custom_definition_must_not_adopt_reserved_core_key_or_bad_label() {
    for key in [
        "category", "tag", "post", "page", "section", "term_ids", "Title", "1bad", "has-dash",
    ] {
        let input = TaxonomyDefinitionInput {
            key: key.into(),
            label: "Valid".into(),
            hierarchical: false,
            public: true,
            settings: None,
            content_types: None,
            rewrite_base: opendash_core::content::PatchField::Omitted,
        };
        assert!(input.validate().is_err(), "{key} must not be creatable");
    }
    let long_label = TaxonomyDefinitionInput {
        key: "genre".into(),
        label: "a".repeat(101),
        hierarchical: true,
        public: true,
        settings: None,
        content_types: None,
        rewrite_base: opendash_core::content::PatchField::Omitted,
    };
    assert!(long_label.validate().is_err());
}

#[test]
fn definitions_and_patches_reject_unknown_keys_and_empty_patch() {
    assert!(
        serde_json::from_value::<TaxonomyDefinitionInput>(json!({
            "key":"genre", "label":"Genre", "owner":"core"
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<TaxonomyDefinitionPatch>(json!({
            "label":"New", "key":"new_key"
        }))
        .is_err()
    );
    let empty: TaxonomyDefinitionPatch = serde_json::from_value(json!({})).unwrap();
    assert!(empty.validate().is_err());
    let valid: TaxonomyDefinitionPatch = serde_json::from_value(json!({
        "hierarchical":false, "public":false
    }))
    .unwrap();
    valid.validate().unwrap();
}
