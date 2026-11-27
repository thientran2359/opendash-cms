//! #375 — Builder taxonomy contract at the server's pinned Core dependency.
//! HTTP + SQLite + Chrome acceptance is exercised by the isolated T02 runner.

use opendash_core::block_schema::{BlockSchema, RegisteredBlockSchema};
use opendash_core::builder::{BuilderContextTarget, BuilderDocument};
use opendash_core::registry::{RegistryCatalog, ResourceOwner};
use serde_json::{Value, json};

fn list_catalog() -> RegistryCatalog<RegisteredBlockSchema> {
    let schema: BlockSchema =
        serde_json::from_str(include_str!("../../src/builtin_blocks/list/block.json")).unwrap();
    let mut catalog = RegistryCatalog::<RegisteredBlockSchema>::new();
    catalog
        .register(
            "core/list",
            ResourceOwner::Core,
            RegisteredBlockSchema { schema },
        )
        .unwrap();
    catalog
}

fn document(props: Value) -> BuilderDocument {
    let input = json!({
        "schema_version": 1,
        "blocks": [
            {"id":"list-1", "block_id":"core/list", "props":props}
        ]
    });
    BuilderDocument::from_json(input.to_string().as_bytes()).unwrap()
}

#[test]
fn generic_term_pair_survives_public_list_ast_serialization() {
    let catalog = list_catalog();
    let ctx = BuilderContextTarget::EntryBody {
        content_type: "page".into(),
    };
    let source = document(json!({
        "type_key":"books_t02", "taxonomy_key":"genre_t02", "term_id":41,
        "include_descendants":true, "page_size":7, "sort":"id_asc"
    }));
    source.validate_for_save(&catalog, &ctx, None).unwrap();
    let restored = BuilderDocument::from_json(&source.to_json().unwrap()).unwrap();
    assert_eq!(source, restored);
    let node = &restored.blocks[0];
    assert_eq!(node.block_id, "core/list");
    assert_eq!(
        node.props.get("taxonomy_key").and_then(Value::as_str),
        Some("genre_t02")
    );
    assert_eq!(node.props.get("term_id").and_then(Value::as_i64), Some(41));
    assert!(!node.props.contains_key("category_id"));
    assert!(!node.props.contains_key("tag_id"));
}

#[test]
fn incompatible_or_incomplete_generic_pair_is_rejected_without_legacy_regression() {
    let catalog = list_catalog();
    let ctx = BuilderContextTarget::EntryBody {
        content_type: "page".into(),
    };
    for invalid in [
        json!({"type_key":"post", "taxonomy_key":"genre_t02"}),
        json!({"type_key":"post", "term_id":41}),
        json!({"type_key":"post", "taxonomy_key":"genre_t02", "term_id":0}),
        json!({"type_key":"post", "taxonomy_key":"genre_t02", "term_id":41,
               "category_id":7}),
        json!({"type_key":"post", "category_id":7, "include_descendants":true}),
        json!({"type_key":"post", "tag_id":11}),
    ] {
        let result = document(invalid).validate_for_save(&catalog, &ctx, None);
        assert!(
            result.is_err(),
            "invalid mixed, incomplete, or pre-V1 filter must not be saved"
        );
    }
    for valid in [
        json!({"type_key":"post", "taxonomy_key":"category", "term_id":7, "include_descendants":true}),
        json!({"type_key":"post", "taxonomy_key":"tag", "term_id":11}),
    ] {
        document(valid)
            .validate_for_save(&catalog, &ctx, None)
            .unwrap();
    }
}
