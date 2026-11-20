//! #375 — Builder taxonomy contract at the server's pinned Core dependency.
//! HTTP + SQLite + Chrome acceptance is exercised by the isolated T02 runner.

use opendash_core::builder::{BuilderDocument, ElementContent, KnownElement};
use opendash_core::registry::RegistryCatalog;
use opendash_core::theme::BlockDescriptor;
use serde_json::{json, Value};

fn document(props: Value) -> BuilderDocument {
    let input = json!({
        "schema_version": 1,
        "sections": [{"id":"section-1", "children": [{"id":"row-1", "children": [
            {"id":"column-1", "children": [
                {"id":"list-1", "type":"core/list", "props":props}
            ]}
        ]}]}]
    });
    BuilderDocument::from_json(input.to_string().as_bytes()).unwrap()
}

#[test]
fn generic_term_pair_survives_public_list_ast_serialization() {
    let catalog = RegistryCatalog::<BlockDescriptor>::new();
    let source = document(json!({
        "type_key":"books_t02", "taxonomy_key":"genre_t02", "term_id":41,
        "include_descendants":true, "page_size":7, "sort":"id_asc"
    }));
    source.validate_for_save(&catalog, None).unwrap();
    let restored = BuilderDocument::from_json(&source.to_json().unwrap()).unwrap();
    assert_eq!(source, restored);
    let ElementContent::Known(KnownElement::List(props)) =
        &restored.sections[0].children[0].children[0].children[0].content
    else { panic!("custom taxonomy must decode into a typed list") };
    assert_eq!(props.taxonomy_key.as_deref(), Some("genre_t02"));
    assert_eq!(props.term_id, Some(41));
    assert_eq!(props.category_id, None);
    assert_eq!(props.tag_id, None);
}

#[test]
fn incompatible_or_incomplete_generic_pair_is_rejected_without_legacy_regression() {
    let catalog = RegistryCatalog::<BlockDescriptor>::new();
    for invalid in [
        json!({"type_key":"post", "taxonomy_key":"genre_t02"}),
        json!({"type_key":"post", "term_id":41}),
        json!({"type_key":"post", "taxonomy_key":"genre_t02", "term_id":0}),
        json!({"type_key":"post", "taxonomy_key":"genre_t02", "term_id":41,
               "category_id":7}),
    ] {
        let result = document(invalid).validate_for_save(&catalog, None);
        assert!(result.is_err(), "invalid mixed or partial filter must not be saved");
    }
    for legacy in [
        json!({"type_key":"post", "category_id":7, "include_descendants":true}),
        json!({"type_key":"post", "tag_id":11}),
        json!({"type_key":"post", "category_id":7, "tag_id":11}),
    ] {
        document(legacy).validate_for_save(&catalog, None).unwrap();
    }
}
