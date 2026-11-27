//! Pure-Rust Gutenberg-style Comment Block (`<!-- od:... -->`) codec for unified
//! entry `values_json["body"]` storage.
//!
//! Contains zero I/O, HTTP, or SQL. Matches `crate::builder::BuilderDocument`
//! 1:1 while preserving `#[serde(deny_unknown_fields)]` fidelity on all node
//! payloads.

use super::*;

pub(crate) use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fmt,
};

pub(crate) use serde::{Deserialize, Serialize};

pub(crate) use serde_json::Value;

pub(crate) use crate::{
    block_schema::RegisteredBlockSchema,
    builder::{
        BUILDER_SCHEMA_VERSION, BindingTarget, BuilderDocument, ButtonProps, ColumnNode,
        ContactFormProps, ContentSlotProps, ElementBinding, ElementContent, ElementNode,
        HeadingProps, ImageProps, KnownElement, LegacyBlockProps, ListProps, NodeMeta,
        PatternRefProps, PluginBlockProps, Responsive, RowNode, SectionNode, SectionStyle,
        TemplatePartProps, TextProps, VideoProps, empty_node_meta_option,
        empty_section_style_option,
    },
    id::valid_package_id,
    registry::{DeclarationState, ProviderStatus, RegistryCatalog},
};

/// Maximum serialized UTF-8 byte length of a `values_json["body"]` string
/// containing `<!-- od:... -->` blocks (`384 KiB`).
mod serialize;
pub use serialize::*;
mod parse;
pub use parse::*;
mod refs;
pub use refs::*;
