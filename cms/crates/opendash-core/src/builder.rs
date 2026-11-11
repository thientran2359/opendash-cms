//! Versioned, typed Builder document for M4-S01-T01.
//!
//! Storage, rendering, media permissions, and the M3 homepage projection are
//! implemented in later slices. Existing elements whose provider/descriptor is
//! unavailable can be round-tripped without letting clients introduce or edit
//! such elements.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use crate::{
    id::{valid_namespaced_block_id, valid_package_id},
    registry::{ProviderStatus, RegistryCatalog},
    theme::{Block, BlockDescriptor, Fields},
};

pub const BUILDER_SCHEMA_VERSION: u32 = 1;
pub const MAX_BUILDER_NODES: usize = 512;
pub const MAX_BUILDER_JSON_BYTES: usize = 256 * 1024;

/// Children preserve render order, while IDs remain independent of position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuilderDocument {
    pub schema_version: u32,
    pub sections: Vec<SectionNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionNode {
    pub id: String,
    pub children: Vec<RowNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowNode {
    pub id: String,
    pub children: Vec<ColumnNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnNode {
    pub id: String,
    pub children: Vec<ElementNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementNode {
    pub id: String,
    #[serde(flatten)]
    pub content: ElementContent,
}

/// Known v1 elements have typed props. Unknown or malformed saved elements
/// retain their original JSON map, but are only accepted by validation when
/// proven to be unchanged from the trusted previous saved document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ElementContent {
    Known(KnownElement),
    Unresolved(BTreeMap<String, Value>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "props", deny_unknown_fields)]
pub enum KnownElement {
    #[serde(rename = "core/heading")]
    Heading(HeadingProps),
    #[serde(rename = "core/text")]
    Text(TextProps),
    #[serde(rename = "core/image")]
    Image(ImageProps),
    #[serde(rename = "core/button")]
    Button(ButtonProps),
    #[serde(rename = "legacy/block")]
    Legacy(LegacyBlockProps),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeadingProps {
    pub level: u8,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextProps {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageProps {
    pub media_id: String,
    pub alt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ButtonProps {
    pub label: String,
    pub url: String,
}

/// Preserves the exact M3 block ID and field-value map, including after a
/// theme switch. An active descriptor is required to add or change a block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyBlockProps {
    pub block_id: String,
    pub values: Fields,
}

#[derive(Debug)]
pub enum BuilderError {
    Json(serde_json::Error),
    Invalid(&'static str),
}

impl std::fmt::Display for BuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(error) => write!(f, "Invalid Builder JSON: {error}"),
            Self::Invalid(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for BuilderError {}

impl From<serde_json::Error> for BuilderError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl BuilderDocument {
    pub fn empty() -> Self {
        Self {
            schema_version: BUILDER_SCHEMA_VERSION,
            sections: Vec::new(),
        }
    }

    /// Reject oversized input before JSON decoding; call validate_for_save
    /// afterwards, as deserialization alone does not validate the tree.
    pub fn from_json(bytes: &[u8]) -> Result<Self, BuilderError> {
        if bytes.len() > MAX_BUILDER_JSON_BYTES {
            return Err(BuilderError::Invalid("Builder JSON exceeds 256 KiB"));
        }
        Ok(serde_json::from_slice(bytes)?)
    }

    pub fn to_json(&self) -> Result<Vec<u8>, BuilderError> {
        let bytes = serde_json::to_vec(self)?;
        if bytes.len() > MAX_BUILDER_JSON_BYTES {
            return Err(BuilderError::Invalid("Builder JSON exceeds 256 KiB"));
        }
        Ok(bytes)
    }

    /// `previous` MUST come from trusted persisted state, never the client.
    /// Unsupported payloads may survive only at their original position under
    /// the same ancestor IDs and with identical content, including the node ID.
    pub fn validate_for_save(
        &self,
        catalog: &RegistryCatalog<BlockDescriptor>,
        previous: Option<&BuilderDocument>,
    ) -> Result<(), BuilderError> {
        if self.schema_version != BUILDER_SCHEMA_VERSION {
            return Err(BuilderError::Invalid("Unsupported Builder schema version"));
        }
        let mut ids = HashSet::new();
        let mut count = 0;
        for (s, section) in self.sections.iter().enumerate() {
            check_node_id(&section.id, &mut ids, &mut count)?;
            for (r, row) in section.children.iter().enumerate() {
                check_node_id(&row.id, &mut ids, &mut count)?;
                for (c, column) in row.children.iter().enumerate() {
                    check_node_id(&column.id, &mut ids, &mut count)?;
                    for (e, element) in column.children.iter().enumerate() {
                        check_node_id(&element.id, &mut ids, &mut count)?;
                        let result = match &element.content {
                            ElementContent::Known(known) => validate_known(known, catalog),
                            ElementContent::Unresolved(_) => {
                                Err("Unknown or malformed Builder element")
                            }
                        };
                        if let Err(message) = result {
                            if !is_preserved(previous, (s, r, c, e), section, row, column, element)
                            {
                                return Err(BuilderError::Invalid(message));
                            }
                        }
                    }
                }
            }
        }
        // Enforce the encoded limit for Rust-constructed documents as well.
        self.to_json()?;
        Ok(())
    }
}

fn check_node_id<'a>(
    id: &'a str,
    ids: &mut HashSet<&'a str>,
    count: &mut usize,
) -> Result<(), BuilderError> {
    *count += 1;
    if *count > MAX_BUILDER_NODES {
        return Err(BuilderError::Invalid("Builder exceeds 512 nodes"));
    }
    if !valid_package_id(id) {
        return Err(BuilderError::Invalid("Invalid Builder node ID"));
    }
    if !ids.insert(id) {
        return Err(BuilderError::Invalid("Duplicate Builder node ID"));
    }
    Ok(())
}

fn is_preserved(
    previous: Option<&BuilderDocument>,
    (s, r, c, e): (usize, usize, usize, usize),
    section: &SectionNode,
    row: &RowNode,
    column: &ColumnNode,
    element: &ElementNode,
) -> bool {
    previous
        .and_then(|old| old.sections.get(s))
        .filter(|old| old.id == section.id)
        .and_then(|old| old.children.get(r))
        .filter(|old| old.id == row.id)
        .and_then(|old| old.children.get(c))
        .filter(|old| old.id == column.id)
        .and_then(|old| old.children.get(e))
        .is_some_and(|old| old == element)
}

fn validate_known(
    element: &KnownElement,
    catalog: &RegistryCatalog<BlockDescriptor>,
) -> Result<(), &'static str> {
    match element {
        KnownElement::Heading(props) => {
            if !(1..=6).contains(&props.level) || !valid_text(&props.text, 512, true) {
                return Err("Invalid Heading level or text");
            }
        }
        KnownElement::Text(props) => {
            if !valid_text(&props.text, 16_384, false) {
                return Err("Invalid Text content");
            }
        }
        KnownElement::Image(props) => {
            if !valid_media_id(&props.media_id) || !valid_text(&props.alt, 512, false) {
                return Err("Invalid Image media ID or alt text");
            }
        }
        KnownElement::Button(props) => {
            if !valid_text(&props.label, 200, true) || !valid_button_url(&props.url) {
                return Err("Invalid Button label or URL");
            }
        }
        KnownElement::Legacy(props) => {
            if !valid_namespaced_block_id(&props.block_id) {
                return Err("Invalid legacy block ID");
            }
            let descriptor = catalog
                .get(&props.block_id)
                .filter(|entry| entry.status == ProviderStatus::Active)
                .ok_or("Legacy block descriptor unavailable")?;
            let block = Block {
                kind: props.block_id.clone(),
                values: props.values.clone(),
            };
            block
                .validate_against_fields(&descriptor.descriptor.fields)
                .map_err(|_| "Legacy field map does not match descriptor")?;
        }
    }
    Ok(())
}

fn valid_media_id(value: &str) -> bool {
    // Matches the M3 media ID *syntax*; P02 checks actual existence and ACL.
    (1..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn valid_text(value: &str, max_chars: usize, required: bool) -> bool {
    value.chars().count() <= max_chars
        && (!required || !value.trim().is_empty())
        && !value
            .chars()
            .any(|ch| ch == '\0' || (ch.is_control() && ch != '\n' && ch != '\t'))
}

/// URL policy from ADR-0007: local path or absolute HTTP(S), no credentials.
pub fn valid_button_url(value: &str) -> bool {
    if value.is_empty()
        || value.len() > 2048
        || value
            .chars()
            .any(|ch| ch.is_whitespace() || ch.is_control())
        || value
            .chars()
            .any(|ch| matches!(ch, '\\' | '<' | '>' | '"' | '\''))
    {
        return false;
    }
    if value.starts_with('/') {
        return !value.starts_with("//")
            && Url::parse(&format!("https://builder.invalid{value}")).is_ok();
    }
    Url::parse(value).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::ResourceOwner;
    use serde_json::json;

    fn registry() -> RegistryCatalog<BlockDescriptor> {
        let mut catalog = RegistryCatalog::new();
        catalog
            .register(
                "ocean/hero",
                ResourceOwner::Theme("ocean".into()),
                BlockDescriptor {
                    id: "ocean/hero".into(),
                    fields: Fields::from([("title".into(), "Tiêu đề".into())]),
                },
            )
            .unwrap();
        catalog
    }

    fn element(id: &str, content: KnownElement) -> ElementNode {
        ElementNode {
            id: id.into(),
            content: ElementContent::Known(content),
        }
    }

    fn fixture() -> BuilderDocument {
        BuilderDocument {
            schema_version: BUILDER_SCHEMA_VERSION,
            sections: vec![SectionNode {
                id: "section-1".into(),
                children: vec![RowNode {
                    id: "row-1".into(),
                    children: vec![ColumnNode {
                        id: "column-1".into(),
                        children: vec![
                            element(
                                "heading-1",
                                KnownElement::Heading(HeadingProps {
                                    level: 2,
                                    text: "Ocean".into(),
                                }),
                            ),
                            element(
                                "text-1",
                                KnownElement::Text(TextProps {
                                    text: "<script>plain text</script>".into(),
                                }),
                            ),
                            element(
                                "image-1",
                                KnownElement::Image(ImageProps {
                                    media_id: "a".repeat(64),
                                    alt: "Anemone".into(),
                                }),
                            ),
                            element(
                                "button-1",
                                KnownElement::Button(ButtonProps {
                                    label: "Explore".into(),
                                    url: "/species?order=latest".into(),
                                }),
                            ),
                            element(
                                "legacy-1",
                                KnownElement::Legacy(LegacyBlockProps {
                                    block_id: "ocean/hero".into(),
                                    values: Fields::from([("title".into(), "Sea".into())]),
                                }),
                            ),
                        ],
                    }],
                }],
            }],
        }
    }

    #[test]
    fn round_trip_typed_elements_and_reorder_keep_node_ids() {
        let before = fixture();
        before.validate_for_save(&registry(), None).unwrap();
        let after = BuilderDocument::from_json(&before.to_json().unwrap()).unwrap();
        assert_eq!(after, before);
        let mut reordered = after;
        reordered.sections[0].children[0].children[0]
            .children
            .swap(0, 4);
        reordered
            .validate_for_save(&registry(), Some(&before))
            .unwrap();
        assert_eq!(
            reordered.sections[0].children[0].children[0].children[0].id,
            "legacy-1"
        );
    }

    #[test]
    fn rejects_unknown_schema_parent_shapes_duplicate_ids_and_limits() {
        let mut wrong_version = fixture();
        wrong_version.schema_version = 2;
        assert!(wrong_version.validate_for_save(&registry(), None).is_err());

        let mut duplicate = fixture();
        duplicate.sections[0].children[0].id = "section-1".into();
        assert!(duplicate.validate_for_save(&registry(), None).is_err());

        let mut invalid_id = fixture();
        invalid_id.sections[0].id = "../../etc".into();
        assert!(invalid_id.validate_for_save(&registry(), None).is_err());

        let wrong_nesting = json!({
            "schema_version": 1,
            "sections": [{"id": "section-1", "children": [{
                "id": "element-1", "type": "core/text", "props": {"text": "oops"}
            }]}]
        });
        assert!(BuilderDocument::from_json(wrong_nesting.to_string().as_bytes()).is_err());

        let mut too_many = fixture();
        too_many.sections[0].children[0].children[0].children = (0..510)
            .map(|index| {
                element(
                    &format!("node-{index}"),
                    KnownElement::Text(TextProps { text: "x".into() }),
                )
            })
            .collect();
        assert!(too_many.validate_for_save(&registry(), None).is_err());
        assert!(BuilderDocument::from_json(&vec![b' '; MAX_BUILDER_JSON_BYTES + 1]).is_err());

        let mut oversized = fixture();
        oversized.sections = (0..100)
            .map(|s| SectionNode {
                id: format!("section-{s}"),
                children: vec![RowNode {
                    id: format!("row-{s}"),
                    children: vec![ColumnNode {
                        id: format!("column-{s}"),
                        children: vec![element(
                            &format!("text-{s}"),
                            KnownElement::Text(TextProps {
                                text: "x".repeat(5000),
                            }),
                        )],
                    }],
                }],
            })
            .collect();
        assert!(oversized.validate_for_save(&registry(), None).is_err());
    }

    #[test]
    fn typed_props_and_url_policy_reject_invalid_values() {
        let registry = registry();
        for invalid in [
            KnownElement::Heading(HeadingProps {
                level: 0,
                text: "Title".into(),
            }),
            KnownElement::Heading(HeadingProps {
                level: 7,
                text: "Title".into(),
            }),
            KnownElement::Image(ImageProps {
                media_id: "../file".into(),
                alt: "x".into(),
            }),
            KnownElement::Button(ButtonProps {
                label: "Go".into(),
                url: "javascript:alert(1)".into(),
            }),
        ] {
            let mut doc = fixture();
            doc.sections[0].children[0].children[0].children[0].content =
                ElementContent::Known(invalid);
            assert!(doc.validate_for_save(&registry, None).is_err());
        }
        for invalid in [
            "//evil.test",
            "https://user:pass@example.test",
            "data:text/html,hi",
            "http://x.test/a\\b",
            "https://x.test/space here",
            "#anchor",
            "ftp://host/path",
        ] {
            assert!(!valid_button_url(invalid), "{invalid}");
        }
        for allowed in [
            "/",
            "/page?q=1#anchor",
            "https://example.test/path",
            "http://localhost:8080/",
        ] {
            assert!(valid_button_url(allowed), "{allowed}");
        }
        let malformed = json!({
            "schema_version": 1,
            "sections": [{"id": "section-1", "children": [{"id": "row-1", "children": [{
                "id": "column-1", "children": [{"id": "heading-1", "type": "core/heading", "props": {"level": "not-a-number", "text": "x"}}]
            }]}]}]
        });
        let decoded = BuilderDocument::from_json(malformed.to_string().as_bytes()).unwrap();
        assert!(decoded.validate_for_save(&registry, None).is_err());
    }

    #[test]
    fn unavailable_legacy_and_unknown_saved_payload_preserved_only_unchanged() {
        let existing = fixture();
        let mut unavailable = registry();
        unavailable.deactivate_owner(&ResourceOwner::Theme("ocean".into()));
        assert!(
            existing
                .validate_for_save(&unavailable, Some(&existing))
                .is_ok()
        );
        assert!(existing.validate_for_save(&unavailable, None).is_err());

        let mut edited = existing.clone();
        let ElementContent::Known(KnownElement::Legacy(props)) =
            &mut edited.sections[0].children[0].children[0].children[4].content
        else {
            panic!()
        };
        props.values.insert("title".into(), "Changed".into());
        assert!(
            edited
                .validate_for_save(&unavailable, Some(&existing))
                .is_err()
        );

        let mut moved = existing.clone();
        moved.sections[0].children[0].children[0]
            .children
            .swap(3, 4);
        assert!(
            moved
                .validate_for_save(&unavailable, Some(&existing))
                .is_err()
        );

        let unknown = json!({"type": "theme/missing", "props": {"opaque": [1, {"a": true}]}, "original": "retained"});
        let mut persisted = fixture();
        persisted.sections[0].children[0].children[0]
            .children
            .push(ElementNode {
                id: "unknown-1".into(),
                content: ElementContent::Unresolved(serde_json::from_value(unknown).unwrap()),
            });
        assert!(persisted.validate_for_save(&registry(), None).is_err());
        let decoded = BuilderDocument::from_json(&persisted.to_json().unwrap()).unwrap();
        assert_eq!(decoded, persisted);
        assert!(
            decoded
                .validate_for_save(&registry(), Some(&persisted))
                .is_ok()
        );
        let mut updated = decoded;
        updated.sections[0].children[0].children[0].children[5].id = "unknown-2".into();
        assert!(
            updated
                .validate_for_save(&registry(), Some(&persisted))
                .is_err()
        );
    }

    #[test]
    fn legacy_descriptor_matches_m3_block_schema() {
        let mut invalid = fixture();
        let ElementContent::Known(KnownElement::Legacy(props)) =
            &mut invalid.sections[0].children[0].children[0].children[4].content
        else {
            panic!()
        };
        props.values.insert("unknown".into(), "value".into());
        assert!(invalid.validate_for_save(&registry(), None).is_err());
    }

    #[test]
    fn unknown_known_element_fields_cannot_be_silently_dropped() {
        let mut doc = serde_json::to_value(fixture()).unwrap();
        doc["sections"][0]["children"][0]["children"][0]["children"][0]["extra"] =
            json!({"must_keep": 3});
        let restored = BuilderDocument::from_json(doc.to_string().as_bytes()).unwrap();
        assert!(restored.validate_for_save(&registry(), None).is_err());
        // A historical node is losslessly preserved while the type is unavailable.
        let raw = restored.to_json().unwrap();
        assert!(String::from_utf8(raw).unwrap().contains("must_keep"));
    }
}
