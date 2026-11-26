use super::*;
use crate::registry::ResourceOwner;
use serde_json::json;

fn registry() -> RegistryCatalog<RegisteredBlockSchema> {
    let schema: crate::block_schema::BlockSchemaV1 = serde_json::from_value(json!({
        "schema_version": 1,
        "id": "ocean/hero",
        "title": "Hero",
        "category": "content",
        "render": "render.html",
        "fields": [
            { "type": "text", "key": "title", "label": "Tiêu đề", "required": true }
        ]
    }))
    .unwrap();
    let mut catalog = RegistryCatalog::new();
    catalog
        .register(
            "ocean/hero",
            ResourceOwner::Theme("ocean".into()),
            RegisteredBlockSchema { schema },
        )
        .unwrap();
    catalog
}

fn element(id: &str, content: KnownElement) -> ElementNode {
    ElementNode {
        id: id.into(),
        meta: None,
        content: ElementContent::Known(content),
    }
}

fn fixture() -> BuilderDocument {
    BuilderDocument {
        schema_version: BUILDER_SCHEMA_VERSION,
        sections: vec![SectionNode {
            id: "section-1".into(),
            meta: None,
            responsive: None,
            style: None,
            children: vec![RowNode {
                id: "row-1".into(),
                meta: None,
                responsive: None,
                children: vec![ColumnNode {
                    id: "column-1".into(),
                    meta: None,
                    responsive: None,
                    children: vec![
                        element(
                            "heading-1",
                            KnownElement::Heading(HeadingProps {
                                level: 2,
                                text: "Ocean".into(),
                                binding: None,
                            }),
                        ),
                        element(
                            "text-1",
                            KnownElement::Text(TextProps {
                                text: "<script>plain text</script>".into(),
                                rich: false,
                                binding: None,
                            }),
                        ),
                        element(
                            "image-1",
                            KnownElement::Image(ImageProps {
                                media_id: "a".repeat(64),
                                alt: "Anemone".into(),
                                binding: None,
                            }),
                        ),
                        element(
                            "button-1",
                            KnownElement::Button(ButtonProps {
                                label: "Explore".into(),
                                url: "/species?order=latest".into(),
                                new_tab: false,
                                nofollow: false,
                                binding: None,
                            }),
                        ),
                        element(
                            "hero-1",
                            KnownElement::PluginBlock(PluginBlockProps {
                                block_id: "ocean/hero".into(),
                                values: BTreeMap::from([("title".into(), json!("Sea"))]),
                            }),
                        ),
                    ],
                }],
            }],
        }],
    }
}

mod suite_01;
mod suite_02;
mod suite_03;
