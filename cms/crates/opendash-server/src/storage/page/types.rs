use super::*;

pub(super) use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

pub(super) use opendash_core::{
    block_schema::{BlockSchema, RegisteredBlockSchema, validate_block_catalog_templates},
    builder::BuilderDocument,
    builder_blocks::{
        ExtractedBlockRefs, extract_block_refs_lenient_with_schemas,
        serialize_builder_document_to_blocks,
    },
    content::{ContentSupport, EffectiveContentSchema, FieldRole, PublicExposure},
    registry::{DeclarationState, OwnedDescriptor, ProviderStatus, RegistryCatalog, ResourceOwner},
};

pub(super) use sha2::{Digest, Sha256};

pub(super) use sqlx::{SqliteConnection, SqlitePool};

pub(super) use crate::error::{ApiError, ApiResult};

#[derive(Debug, Clone)]
pub struct RegisteredBlockDef {
    pub block_id: String,
    pub owner: ResourceOwner,
    pub status: ProviderStatus,
    pub declaration_state: DeclarationState,
    pub schema: RegisteredBlockSchema,
    pub template: String,
    pub revision_digest: String,
}

/// 23 Core Blocks: 18 `NativeRust` (`None` template) + 5 `CompiledCsec` (`Some(render.html)`).
pub(in crate::storage) const EMBEDDED_CORE_BLOCK_SOURCES: &[(&str, Option<&str>)] = &[
    // Layout (4 NativeRust)
    (
        include_str!("../../builtin_blocks/section/block.json"),
        None,
    ),
    (include_str!("../../builtin_blocks/row/block.json"), None),
    (include_str!("../../builtin_blocks/column/block.json"), None),
    (include_str!("../../builtin_blocks/stack/block.json"), None),
    // Core Leaf (7 NativeRust)
    (
        include_str!("../../builtin_blocks/heading/block.json"),
        None,
    ),
    (include_str!("../../builtin_blocks/text/block.json"), None),
    (include_str!("../../builtin_blocks/image/block.json"), None),
    (include_str!("../../builtin_blocks/video/block.json"), None),
    (include_str!("../../builtin_blocks/button/block.json"), None),
    (
        include_str!("../../builtin_blocks/divider/block.json"),
        None,
    ),
    (include_str!("../../builtin_blocks/spacer/block.json"), None),
    // Runtime & Template (7 NativeRust)
    (include_str!("../../builtin_blocks/list/block.json"), None),
    (
        include_str!("../../builtin_blocks/contact-form/block.json"),
        None,
    ),
    (
        include_str!("../../builtin_blocks/search-form/block.json"),
        None,
    ),
    (
        include_str!("../../builtin_blocks/search-results/block.json"),
        None,
    ),
    (
        include_str!("../../builtin_blocks/pattern-ref/block.json"),
        None,
    ),
    (
        include_str!("../../builtin_blocks/template-part/block.json"),
        None,
    ),
    (
        include_str!("../../builtin_blocks/content-slot/block.json"),
        None,
    ),
    // CompiledCsec (5 blocks)
    (
        include_str!("../../builtin_blocks/banner/block.json"),
        Some(include_str!("../../builtin_blocks/banner/render.html")),
    ),
    (
        include_str!("../../builtin_blocks/card/block.json"),
        Some(include_str!("../../builtin_blocks/card/render.html")),
    ),
    (
        include_str!("../../builtin_blocks/accordion/block.json"),
        Some(include_str!("../../builtin_blocks/accordion/render.html")),
    ),
    (
        include_str!("../../builtin_blocks/accordion-item/block.json"),
        Some(include_str!(
            "../../builtin_blocks/accordion-item/render.html"
        )),
    ),
    (
        include_str!("../../builtin_blocks/social-follow/block.json"),
        Some(include_str!(
            "../../builtin_blocks/social-follow/render.html"
        )),
    ),
];
