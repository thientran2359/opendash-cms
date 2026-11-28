use super::*;
use crate::{
    block_schema::{BlockCategory, BlockSchema},
    registry::ResourceOwner,
};
use serde_json::json;

fn active_test_catalog() -> RegistryCatalog<RegisteredBlockSchema> {
    let schemas_json = vec![
        json!({
            "schema_version": 1,
            "id": "core/section",
            "title": "Section",
            "category": "layout",
            "render": "native",
            "supports_children": true,
            "allowed_parents": ["root"],
            "allowed_children": ["*", "!core/section"],
            "fields": []
        }),
        json!({
            "schema_version": 1,
            "id": "core/row",
            "title": "Row",
            "category": "layout",
            "render": "native",
            "supports_children": true,
            "allowed_children": ["core/column"],
            "fields": []
        }),
        json!({
            "schema_version": 1,
            "id": "core/column",
            "title": "Column",
            "category": "layout",
            "render": "native",
            "supports_children": true,
            "allowed_parents": ["core/row"],
            "allowed_children": ["*", "!core/section"],
            "fields": []
        }),
        json!({
            "schema_version": 1,
            "id": "core/stack",
            "title": "Stack",
            "category": "layout",
            "render": "native",
            "supports_children": true,
            "allowed_children": ["*", "!core/section"],
            "fields": [
                { "type": "select", "key": "direction", "label": "Dir", "default": "column", "options": [{ "value": "row", "label": "Row" }, { "value": "column", "label": "Col" }] }
            ]
        }),
        json!({
            "schema_version": 1,
            "id": "core/heading",
            "title": "Heading",
            "category": "content",
            "render": "native",
            "fields": [
                { "type": "select", "key": "level", "label": "Level", "default": "h2", "options": [{ "value": "h2", "label": "H2" }, { "value": "h3", "label": "H3" }] },
                { "type": "text", "key": "text", "label": "Text", "required": true }
            ]
        }),
        json!({
            "schema_version": 1,
            "id": "core/button",
            "title": "Button",
            "category": "content",
            "render": "native",
            "fields": [
                { "type": "link", "key": "cta", "label": "CTA" }
            ]
        }),
        json!({
            "schema_version": 1,
            "id": "core/list",
            "title": "List",
            "category": "content",
            "render": "native",
            "allowed_contexts": ["*", "!pattern", "!theme_pattern"],
            "fields": [
                { "type": "select", "key": "type_key", "label": "Type", "options_source": "content_types", "required": true },
                { "type": "select", "key": "taxonomy_key", "label": "Tax", "options_source": "taxonomies", "allow_null": true },
                { "type": "term_ref", "key": "term_id", "label": "Term", "taxonomy_from_field": "taxonomy_key", "multiple": false },
                { "type": "boolean", "key": "include_descendants", "label": "Sub", "default": false },
                { "type": "number", "key": "page_size", "label": "Size", "min": 1, "max": 24, "default": 6 }
            ]
        }),
        json!({
            "schema_version": 1,
            "id": "core/contact-form",
            "title": "Form",
            "category": "interactive",
            "render": "native",
            "allowed_contexts": ["*", "!theme_pattern"],
            "fields": [
                { "type": "select", "key": "form_id", "label": "Form", "options_source": "contact_forms", "required": true }
            ]
        }),
        json!({
            "schema_version": 1,
            "id": "core/pattern-ref",
            "title": "Pattern",
            "category": "content",
            "render": "native",
            "allowed_contexts": ["entry_body"],
            "fields": [
                { "type": "select", "key": "pattern_id", "label": "Pattern", "options_source": "patterns", "required": true }
            ]
        }),
        json!({
            "schema_version": 1,
            "id": "core/template-part",
            "title": "Part",
            "category": "content",
            "render": "native",
            "allowed_contexts": ["template", "part"],
            "fields": [
                { "type": "text", "key": "part_key", "label": "Key", "required": true }
            ]
        }),
        json!({
            "schema_version": 1,
            "id": "core/entry-content",
            "title": "Entry Content",
            "category": "content",
            "render": "native",
            "allowed_contexts": ["single", "single_custom", "singular", "page", "page_layout", "index"],
            "fields": []
        }),
    ];

    let mut catalog = RegistryCatalog::new();
    for raw in schemas_json {
        let schema: BlockSchema = serde_json::from_value(raw).unwrap();
        schema.validate("core").unwrap();
        catalog
            .register(
                schema.id.clone(),
                ResourceOwner::Core,
                RegisteredBlockSchema { schema },
            )
            .unwrap();
    }
    catalog
}

fn minimal_schema(id: &str, supports_children: bool) -> BlockSchema {
    BlockSchema {
        schema_version: 1,
        id: id.into(),
        title: id.into(),
        category: BlockCategory::Plugin,
        icon: None,
        render: "render.html".into(),
        supports_children,
        allowed_parents: None,
        allowed_children: None,
        allowed_contexts: None,
        default_children: Vec::new(),
        presets: Vec::new(),
        fields: Vec::new(),
    }
}

fn node(id: &str, block_id: &str, props: serde_json::Value, children: Vec<BlockNode>) -> BlockNode {
    let map = props
        .as_object()
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();
    BlockNode {
        id: id.into(),
        block_id: block_id.into(),
        props: map,
        meta: None,
        children,
    }
}

fn entry_ctx() -> BuilderContextTarget {
    BuilderContextTarget::EntryBody {
        content_type: "page".into(),
    }
}

mod suite_01;
mod suite_02;
mod suite_03;
