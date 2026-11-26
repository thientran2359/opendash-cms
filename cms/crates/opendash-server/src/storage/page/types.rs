use super::*;

pub(super) use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

pub(super) use opendash_core::{
    block_schema::{BlockSchemaV1, RegisteredBlockSchema},
    builder::BuilderDocument,
    builder_blocks::{
        ExtractedBlockRefs, extract_block_refs_lenient_with_schemas,
        serialize_builder_document_to_blocks,
    },
    content::{
        ContentFieldType, ContentSupport, EffectiveContentSchema, FieldRole, PublicExposure,
    },
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

pub(in crate::storage) const EMBEDDED_CORE_BLOCK_SOURCES: &[(&str, &str)] = &[
    (
        include_str!("../../builtin_blocks/spacer/block.json"),
        include_str!("../../builtin_blocks/spacer/render.html"),
    ),
    (
        include_str!("../../builtin_blocks/divider/block.json"),
        include_str!("../../builtin_blocks/divider/render.html"),
    ),
    (
        include_str!("../../builtin_blocks/banner/block.json"),
        include_str!("../../builtin_blocks/banner/render.html"),
    ),
    (
        include_str!("../../builtin_blocks/card/block.json"),
        include_str!("../../builtin_blocks/card/render.html"),
    ),
    (
        include_str!("../../builtin_blocks/accordion/block.json"),
        include_str!("../../builtin_blocks/accordion/render.html"),
    ),
    (
        include_str!("../../builtin_blocks/showcase/block.json"),
        include_str!("../../builtin_blocks/showcase/render.html"),
    ),
    (
        include_str!("../../builtin_blocks/social-follow/block.json"),
        include_str!("../../builtin_blocks/social-follow/render.html"),
    ),
];
