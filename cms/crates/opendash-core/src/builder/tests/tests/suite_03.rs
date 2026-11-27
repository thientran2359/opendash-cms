use super::*;
use crate::registry::{DeclarationState, OwnedDescriptor};

#[test]
fn inactive_provider_subtree_allows_rescue_move_and_delete_but_blocks_mutation_or_insertion() {
    let mut catalog = active_test_catalog();
    let mut inactive_box = minimal_schema("acme/box", true);
    let box_field = crate::field_schema::FieldSpec::new(
        crate::field_schema::FieldType::Text,
        "heading",
        "Heading",
    );
    inactive_box.fields.push(box_field);

    catalog.insert_loaded(
        "acme/box",
        OwnedDescriptor {
            owner: ResourceOwner::Theme("acme".into()),
            status: ProviderStatus::ProviderInactive,
            declaration_state: DeclarationState::Declared,
            descriptor: RegisteredBlockSchema {
                schema: inactive_box,
            },
        },
    );

    let prev = BuilderDocument {
        schema_version: 1,
        blocks: vec![node(
            "box-1",
            "acme/box",
            json!({ "heading": "Tạm tắt" }),
            vec![node(
                "child-hd",
                "core/heading",
                json!({ "text": "Con" }),
                vec![],
            )],
        )],
    };

    // Identical preservation passes
    prev.validate_for_save(&catalog, &entry_ctx(), Some(&prev))
        .unwrap();

    // Rescuing active child out of inactive container to root passes
    let rescued = BuilderDocument {
        schema_version: 1,
        blocks: vec![
            node("box-1", "acme/box", json!({ "heading": "Tạm tắt" }), vec![]),
            node(
                "child-hd",
                "core/heading",
                json!({ "text": "Con đã cứu" }),
                vec![],
            ),
        ],
    };
    rescued
        .validate_for_save(&catalog, &entry_ctx(), Some(&prev))
        .unwrap();

    // Moving inactive container into core/section (`allowed_children: ["*", "!core/section"]`) passes
    let moved_into_sec = BuilderDocument {
        schema_version: 1,
        blocks: vec![node(
            "sec-1",
            "core/section",
            json!({}),
            vec![prev.blocks[0].clone()],
        )],
    };
    moved_into_sec
        .validate_for_save(&catalog, &entry_ctx(), Some(&prev))
        .unwrap();

    // Moving inactive container into core/row (`["core/column"]`, not `*`) fails
    let moved_into_row = BuilderDocument {
        schema_version: 1,
        blocks: vec![node(
            "row-1",
            "core/row",
            json!({}),
            vec![prev.blocks[0].clone()],
        )],
    };
    assert!(
        moved_into_row
            .validate_for_save(&catalog, &entry_ctx(), Some(&prev))
            .is_err()
    );

    // Inserting a new child inside an inactive container fails
    let inserted_inside = BuilderDocument {
        schema_version: 1,
        blocks: vec![node(
            "box-1",
            "acme/box",
            json!({ "heading": "Tạm tắt" }),
            vec![
                node("child-hd", "core/heading", json!({ "text": "Con" }), vec![]),
                node("intruder", "core/heading", json!({ "text": "Mới" }), vec![]),
            ],
        )],
    };
    assert!(
        inserted_inside
            .validate_for_save(&catalog, &entry_ctx(), Some(&prev))
            .is_err()
    );

    // Mutating child props while kept inside inactive container fails
    let mutated_inside = BuilderDocument {
        schema_version: 1,
        blocks: vec![node(
            "box-1",
            "acme/box",
            json!({ "heading": "Tạm tắt" }),
            vec![node(
                "child-hd",
                "core/heading",
                json!({ "text": "Sửa lén" }),
                vec![],
            )],
        )],
    };
    assert!(
        mutated_inside
            .validate_for_save(&catalog, &entry_ctx(), Some(&prev))
            .is_err()
    );
}

#[test]
fn strip_default_props_and_tailwind_spacing_helpers_behave_accurately() {
    let mut catalog = active_test_catalog();
    catalog.insert_loaded(
        "acme/inactive-btn",
        OwnedDescriptor {
            owner: ResourceOwner::Plugin("acme".into()),
            status: ProviderStatus::ProviderInactive,
            declaration_state: DeclarationState::Declared,
            descriptor: RegisteredBlockSchema {
                schema: minimal_schema("acme/inactive-btn", false),
            },
        },
    );

    let mut doc = BuilderDocument {
        schema_version: 1,
        blocks: vec![
            node(
                "h1",
                "core/heading",
                json!({ "level": "h2", "text": "Hi" }),
                vec![],
            ),
            node(
                "h2",
                "core/heading",
                json!({ "level": "", "text": "Keep empty" }),
                vec![],
            ),
            node(
                "inact",
                "acme/inactive-btn",
                json!({ "keep": "raw" }),
                vec![],
            ),
        ],
    };
    doc.strip_default_props(&catalog);
    assert!(!doc.blocks[0].props.contains_key("level"));
    assert_eq!(doc.blocks[1].props.get("level"), Some(&json!("")));
    assert_eq!(doc.blocks[2].props.get("keep"), Some(&json!("raw")));

    let tw = compile_box_spacing_tailwind(
        "p",
        Some(BoxSpacing {
            top: Some(24),
            right: Some(16),
            bottom: Some(24),
            left: Some(16),
        }),
        None,
        Some(BoxSpacing {
            top: Some(12),
            right: Some(12),
            bottom: Some(12),
            left: Some(12),
        }),
        false,
    )
    .unwrap();
    assert_eq!(tw, vec!["p-3", "md:px-4", "md:py-6"]);
    assert_eq!(gap_px_to_tw_step(16), Some(4));
    assert!(valid_button_url("https://opendash.vn/docs"));
    assert!(!valid_button_url("javascript:alert(1)"));
}
