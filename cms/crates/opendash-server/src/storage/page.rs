use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

use opendash_core::{
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
use sha2::{Digest, Sha256};
use sqlx::{SqliteConnection, SqlitePool};

use crate::error::{ApiError, ApiResult};

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

const EMBEDDED_CORE_BLOCK_SOURCES: &[(&str, &str)] = &[
    (
        include_str!("../builtin_blocks/spacer/block.json"),
        include_str!("../builtin_blocks/spacer/render.html"),
    ),
    (
        include_str!("../builtin_blocks/divider/block.json"),
        include_str!("../builtin_blocks/divider/render.html"),
    ),
    (
        include_str!("../builtin_blocks/banner/block.json"),
        include_str!("../builtin_blocks/banner/render.html"),
    ),
    (
        include_str!("../builtin_blocks/card/block.json"),
        include_str!("../builtin_blocks/card/render.html"),
    ),
    (
        include_str!("../builtin_blocks/accordion/block.json"),
        include_str!("../builtin_blocks/accordion/render.html"),
    ),
    (
        include_str!("../builtin_blocks/showcase/block.json"),
        include_str!("../builtin_blocks/showcase/render.html"),
    ),
    (
        include_str!("../builtin_blocks/social-follow/block.json"),
        include_str!("../builtin_blocks/social-follow/render.html"),
    ),
];

pub fn compute_block_definition_digest(schema_json: &str, template: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(schema_json.as_bytes());
    hasher.update(b"\0");
    hasher.update(template.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn embedded_core_block_defs() -> &'static BTreeMap<String, RegisteredBlockDef> {
    static DEFS: OnceLock<BTreeMap<String, RegisteredBlockDef>> = OnceLock::new();
    DEFS.get_or_init(|| {
        let mut map = BTreeMap::new();
        for (raw_schema, raw_tpl) in EMBEDDED_CORE_BLOCK_SOURCES {
            let schema: BlockSchemaV1 =
                serde_json::from_str(raw_schema).expect("valid embedded core block.json");
            schema
                .validate("core")
                .expect("embedded core block.json must validate");
            crate::template_sec::validate_block_template_csec(&schema.id, raw_tpl, &schema)
                .expect("embedded core render.html must pass C-SEC");
            let canonical_json =
                serde_json::to_string(&schema).expect("serialize embedded core block.json");
            let digest = compute_block_definition_digest(&canonical_json, raw_tpl);
            let block_id = schema.id.clone();
            map.insert(
                block_id.clone(),
                RegisteredBlockDef {
                    block_id,
                    owner: ResourceOwner::Core,
                    status: ProviderStatus::Active,
                    declaration_state: DeclarationState::Declared,
                    schema: RegisteredBlockSchema {
                        schema,
                        legacy_mode: false,
                    },
                    template: (*raw_tpl).to_owned(),
                    revision_digest: digest,
                },
            );
        }
        map
    })
}

pub async fn ensure_embedded_core_blocks_in_conn(conn: &mut SqliteConnection) -> ApiResult<()> {
    for def in embedded_core_block_defs().values() {
        let schema_json = serde_json::to_string(&def.schema.schema)?;
        sqlx::query(
            "INSERT INTO builder_block_definitions
                (block_id, owner_kind, owner_id, provider_status, declaration_state, schema_json, template, revision_digest, updated_at)
             VALUES (?, 'core', 'core', 'active', 'declared', ?, ?, ?, datetime('now'))
             ON CONFLICT(block_id) DO UPDATE SET
                owner_kind = 'core',
                owner_id = 'core',
                provider_status = 'active',
                declaration_state = 'declared',
                schema_json = excluded.schema_json,
                template = excluded.template,
                revision_digest = excluded.revision_digest,
                updated_at = datetime('now')
             WHERE builder_block_definitions.owner_kind = 'core' AND builder_block_definitions.owner_id = 'core'",
        )
        .bind(&def.block_id)
        .bind(&schema_json)
        .bind(&def.template)
        .bind(&def.revision_digest)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

pub async fn ensure_embedded_core_blocks(db: &SqlitePool) -> ApiResult<()> {
    let mut conn = db.acquire().await?;
    ensure_embedded_core_blocks_in_conn(&mut conn).await
}

fn parse_owner(owner_kind: &str, owner_id: &str) -> ApiResult<ResourceOwner> {
    match owner_kind {
        "core" => Ok(ResourceOwner::Core),
        "theme" => Ok(ResourceOwner::Theme(owner_id.to_owned())),
        "plugin" => Ok(ResourceOwner::Plugin(owner_id.to_owned())),
        _ => Err(ApiError::internal(
            "Invalid owner_kind in builder_block_definitions",
        )),
    }
}

fn parse_provider_status(raw: &str) -> ApiResult<ProviderStatus> {
    match raw {
        "active" => Ok(ProviderStatus::Active),
        "provider_inactive" => Ok(ProviderStatus::ProviderInactive),
        _ => Err(ApiError::internal(
            "Invalid provider_status in builder_block_definitions",
        )),
    }
}

fn parse_declaration_state(raw: &str) -> ApiResult<DeclarationState> {
    match raw {
        "declared" => Ok(DeclarationState::Declared),
        "withdrawn" => Ok(DeclarationState::Withdrawn),
        _ => Err(ApiError::internal(
            "Invalid declaration_state in builder_block_definitions",
        )),
    }
}

fn format_provider_status(status: ProviderStatus) -> &'static str {
    match status {
        ProviderStatus::Active => "active",
        _ => "provider_inactive",
    }
}

pub async fn load_declared_blocks_in_conn(
    conn: &mut SqliteConnection,
) -> ApiResult<BTreeMap<String, RegisteredBlockDef>> {
    let mut out = embedded_core_block_defs().clone();
    let rows: Vec<(String, String, String, String, String, String, String, String)> =
        sqlx::query_as(
            "SELECT block_id, owner_kind, owner_id, provider_status, declaration_state, schema_json, template, revision_digest
             FROM builder_block_definitions
             ORDER BY block_id",
        )
        .fetch_all(&mut *conn)
        .await?;
    for (
        block_id,
        owner_kind,
        owner_id,
        status_raw,
        decl_raw,
        schema_json,
        template,
        revision_digest,
    ) in rows
    {
        let owner = parse_owner(&owner_kind, &owner_id)?;
        let status = parse_provider_status(&status_raw)?;
        let declaration_state = parse_declaration_state(&decl_raw)?;
        let schema: BlockSchemaV1 = serde_json::from_str(&schema_json)?;
        out.insert(
            block_id.clone(),
            RegisteredBlockDef {
                block_id,
                owner,
                status,
                declaration_state,
                schema: RegisteredBlockSchema {
                    schema,
                    legacy_mode: false,
                },
                template,
                revision_digest,
            },
        );
    }
    Ok(out)
}

pub async fn load_unified_block_catalog_in_conn(
    conn: &mut SqliteConnection,
) -> ApiResult<RegistryCatalog<RegisteredBlockSchema>> {
    let declared = load_declared_blocks_in_conn(&mut *conn).await?;
    let mut catalog = RegistryCatalog::new();
    for (block_id, def) in declared {
        catalog.insert_loaded(
            block_id,
            OwnedDescriptor {
                owner: def.owner,
                status: def.status,
                declaration_state: def.declaration_state,
                descriptor: def.schema,
            },
        );
    }
    Ok(catalog)
}

pub async fn sync_owner_blocks_in_tx(
    conn: &mut SqliteConnection,
    owner_kind: &str,
    owner_id: &str,
    provider_status: ProviderStatus,
    revision_digest: &str,
    blocks: &[(BlockSchemaV1, String)],
) -> ApiResult<()> {
    if !matches!(owner_kind, "core" | "theme" | "plugin") {
        return Err(ApiError::bad_request("owner_kind không hợp lệ."));
    }
    for (schema, template) in blocks {
        schema.validate(owner_id).map_err(ApiError::bad_request)?;
        crate::template_sec::validate_block_template_csec(&schema.id, template, schema)
            .map_err(ApiError::bad_request)?;

        let existing: Option<(String, String)> = sqlx::query_as(
            "SELECT owner_kind, owner_id FROM builder_block_definitions WHERE block_id = ?",
        )
        .bind(&schema.id)
        .fetch_optional(&mut *conn)
        .await?;
        if let Some((ex_kind, ex_id)) = existing
            && (ex_kind != owner_kind || ex_id != owner_id)
        {
            return Err(ApiError::conflict(
                "ID block đã thuộc quyền sở hữu của gói khác.",
            ));
        }
    }

    if owner_kind == "theme" && provider_status == ProviderStatus::Active {
        sqlx::query(
            "UPDATE builder_block_definitions
             SET provider_status = 'provider_inactive', updated_at = datetime('now')
             WHERE owner_kind = 'theme' AND owner_id != ? AND provider_status != 'provider_inactive'",
        )
        .bind(owner_id)
        .execute(&mut *conn)
        .await?;
    }

    let existing_for_owner: Vec<String> = sqlx::query_scalar(
        "SELECT block_id FROM builder_block_definitions WHERE owner_kind = ? AND owner_id = ?",
    )
    .bind(owner_kind)
    .bind(owner_id)
    .fetch_all(&mut *conn)
    .await?;

    for old_id in existing_for_owner {
        if !blocks.iter().any(|(s, _)| s.id == old_id) {
            sqlx::query(
                "UPDATE builder_block_definitions
                 SET declaration_state = 'withdrawn', provider_status = 'provider_inactive', updated_at = datetime('now')
                 WHERE block_id = ? AND owner_kind = ? AND owner_id = ?",
            )
            .bind(&old_id)
            .bind(owner_kind)
            .bind(owner_id)
            .execute(&mut *conn)
            .await?;
        }
    }

    let status_str = format_provider_status(provider_status);
    for (schema, template) in blocks {
        let schema_json = serde_json::to_string(schema)?;
        sqlx::query(
            "INSERT INTO builder_block_definitions
                (block_id, owner_kind, owner_id, provider_status, declaration_state, schema_json, template, revision_digest, updated_at)
             VALUES (?, ?, ?, ?, 'declared', ?, ?, ?, datetime('now'))
             ON CONFLICT(block_id) DO UPDATE SET
                provider_status = excluded.provider_status,
                declaration_state = 'declared',
                schema_json = excluded.schema_json,
                template = excluded.template,
                revision_digest = excluded.revision_digest,
                updated_at = datetime('now')
             WHERE builder_block_definitions.owner_kind = excluded.owner_kind
               AND builder_block_definitions.owner_id = excluded.owner_id",
        )
        .bind(&schema.id)
        .bind(owner_kind)
        .bind(owner_id)
        .bind(status_str)
        .bind(&schema_json)
        .bind(template)
        .bind(revision_digest)
        .execute(&mut *conn)
        .await?;
    }

    Ok(())
}

pub async fn set_owner_block_status_in_tx(
    conn: &mut SqliteConnection,
    owner_kind: &str,
    owner_id: &str,
    status: ProviderStatus,
) -> ApiResult<()> {
    let status_str = format_provider_status(status);
    sqlx::query(
        "UPDATE builder_block_definitions
         SET provider_status = ?, updated_at = datetime('now')
         WHERE owner_kind = ? AND owner_id = ? AND declaration_state = 'declared'",
    )
    .bind(status_str)
    .bind(owner_kind)
    .bind(owner_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

// The media-ref tables are public access projections. They must be recomputed in
// the *same transaction* that changes a block provider's availability: removing
// a provider may revoke URLs which were public a moment earlier. Raw delete pins
// are intentionally a different contract (storage::media_refs) and are untouched.
fn active_document_refs(
    document: &BuilderDocument,
    catalog: &RegistryCatalog<RegisteredBlockSchema>,
) -> ExtractedBlockRefs {
    serialize_builder_document_to_blocks(document)
        .ok()
        .map(|body| extract_block_refs_lenient_with_schemas(&body, catalog))
        .unwrap_or_default()
}

async fn active_media_roles_in_conn(
    conn: &mut SqliteConnection,
    refs: &ExtractedBlockRefs,
) -> ApiResult<BTreeSet<(String, &'static str)>> {
    let mut roles = BTreeSet::new();
    for (ids, role, required_kind) in [
        (&refs.image_ids, "image", Some("image")),
        (&refs.video_ids, "video", Some("video")),
        (&refs.poster_image_ids, "poster", Some("image")),
        (&refs.any_media_ids, "any", None),
    ] {
        for media_id in ids {
            if !crate::storage::media::valid_media_id(media_id) {
                // Invalid legacy content must not make a provider toggle fail;
                // ignoring its ACL is the safe fallback.
                continue;
            }
            let kind: Option<String> = sqlx::query_scalar(
                "SELECT kind FROM media_assets WHERE id = ? AND lifecycle_state = 'active'",
            )
            .bind(media_id)
            .fetch_optional(&mut *conn)
            .await?;
            if let Some(kind) = kind {
                if required_kind.is_some_and(|expected| expected != kind) {
                    continue;
                }
                let projected_role = if role == "any" {
                    match kind.as_str() {
                        "image" => "image",
                        "video" => "video",
                        _ => continue,
                    }
                } else {
                    role
                };
                roles.insert((media_id.clone(), projected_role));
            }
        }
    }
    Ok(roles)
}

pub(crate) fn body_can_grant_media(schema: &EffectiveContentSchema) -> bool {
    schema.base.status == ProviderStatus::Active
        && schema.base.declaration_state == DeclarationState::Declared
        && schema.base.descriptor.settings.public
        && schema
            .base
            .descriptor
            .settings
            .supports(ContentSupport::Builder)
        && schema.effective_fields.get("body").is_some_and(|body| {
            body.definition.field_type == ContentFieldType::RichText
                && body.definition.public_exposure == PublicExposure::Public
                && body.definition.min_view_role == FieldRole::Editor
        })
}

/// Rebuilds every media ACL projection on a provider lifecycle transition.
/// This reads the newly committed-in-transaction registry, never a cached
/// catalog from before the toggle. It does not rewrite Entry payloads or
/// `entry_pattern_refs`, so unavailable blocks remain fully recoverable.
pub async fn resync_plugin_block_acls_in_tx(conn: &mut SqliteConnection) -> ApiResult<()> {
    let catalog = load_unified_block_catalog_in_conn(&mut *conn).await?;

    // Entry body blocks (including the private Pattern CPT). A Pattern's refs
    // grant access only through a published Builder parent via public_content.
    let entry_rows: Vec<(i64, String, String, Option<String>)> = sqlx::query_as(
        "SELECT e.id, e.content_type_key, e.values_json, d.document_json
           FROM content_entries e
           LEFT JOIN entry_builder_documents d ON d.entry_id = e.id
          ORDER BY e.id",
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut public_body_policy_cache = BTreeMap::new();
    for (entry_id, type_key, values_json, doc_json) in entry_rows {
        let allowed = if type_key == "pattern" {
            true
        } else if type_key == "section" {
            false
        } else if let Some(allowed) = public_body_policy_cache.get(&type_key) {
            *allowed
        } else {
            let allowed = match crate::storage::content::load_effective_content_schema_in_tx(
                &mut *conn, &type_key,
            )
            .await?
            {
                Some(schema) => body_can_grant_media(&schema),
                None => false,
            };
            public_body_policy_cache.insert(type_key.clone(), allowed);
            allowed
        };
        let refs = if allowed {
            let values: serde_json::Value = serde_json::from_str(&values_json)?;
            let raw_body = values.get("body").and_then(serde_json::Value::as_str);
            if let Some(body) =
                raw_body.filter(|body| opendash_core::builder_blocks::contains_blocks(body))
            {
                extract_block_refs_lenient_with_schemas(body, &catalog)
            } else if let Some(doc) = doc_json
                .as_deref()
                .and_then(|raw| BuilderDocument::from_json(raw.as_bytes()).ok())
            {
                active_document_refs(&doc, &catalog)
            } else {
                ExtractedBlockRefs::default()
            }
        } else {
            ExtractedBlockRefs::default()
        };
        let roles = active_media_roles_in_conn(&mut *conn, &refs).await?;
        sqlx::query("DELETE FROM entry_builder_media_refs WHERE entry_id = ?")
            .bind(entry_id)
            .execute(&mut *conn)
            .await?;
        for (media_id, role) in roles {
            sqlx::query("INSERT INTO entry_builder_media_refs(entry_id, media_id, ref_role) VALUES (?, ?, ?)")
                .bind(entry_id)
                .bind(media_id)
                .bind(role)
                .execute(&mut *conn)
                .await?;
        }
    }

    let overrides: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, ast_json FROM template_overrides ORDER BY id")
            .fetch_all(&mut *conn)
            .await?;
    for (override_id, ast_json) in overrides {
        let refs = BuilderDocument::from_json(ast_json.as_bytes())
            .ok()
            .map(|doc| active_document_refs(&doc, &catalog))
            .unwrap_or_default();
        let roles = active_media_roles_in_conn(&mut *conn, &refs).await?;
        sqlx::query("DELETE FROM template_override_media_refs WHERE override_id = ?")
            .bind(override_id)
            .execute(&mut *conn)
            .await?;
        for (media_id, role) in roles {
            sqlx::query("INSERT INTO template_override_media_refs(override_id, media_id, ref_role) VALUES (?, ?, ?)")
                .bind(override_id)
                .bind(media_id)
                .bind(role)
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod s02_block_registry_tests {
    use super::*;
    use axum::http::StatusCode;
    use serde_json::json;

    #[tokio::test]
    async fn migration_0045_seeds_five_core_blocks_enforces_409_conflict_and_preserves_withdrawn() {
        let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::storage::content::preflight_builtin_key_collisions(&db)
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        ensure_embedded_core_blocks(&db).await.unwrap();

        let schema_cnt: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema")
            .fetch_one(&db)
            .await
            .unwrap();
        assert!(
            schema_cnt < crate::storage::restore::MAX_SCHEMA_OBJECTS,
            "sqlite_schema count {schema_cnt} must stay below 256"
        );

        let core_cnt: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM builder_block_definitions WHERE owner_kind = 'core' AND provider_status = 'active' AND declaration_state = 'declared'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(core_cnt, EMBEDDED_CORE_BLOCK_SOURCES.len() as i64);

        let mut conn = db.acquire().await.unwrap();
        let b1: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "acme-plugin/cta",
            "title": "CTA",
            "category": "marketing",
            "render": "render.html",
            "fields": [
                { "type": "text", "key": "label", "label": "Label" }
            ]
        }))
        .unwrap();
        let b2: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "acme-plugin/old",
            "title": "Old",
            "category": "marketing",
            "render": "render.html",
            "fields": [
                { "type": "text", "key": "label", "label": "Label" }
            ]
        }))
        .unwrap();

        sync_owner_blocks_in_tx(
            &mut conn,
            "plugin",
            "acme-plugin",
            ProviderStatus::Active,
            "rev-1",
            &[
                (b1.clone(), "<div>{{ label }}</div>".into()),
                (b2.clone(), "<div>{{ label }}</div>".into()),
            ],
        )
        .await
        .unwrap();

        // Cross-provider collision on acme-plugin/cta by another owner must return 409 Conflict
        let err = sync_owner_blocks_in_tx(
            &mut conn,
            "theme",
            "acme-plugin",
            ProviderStatus::Active,
            "rev-t1",
            &[(b1.clone(), "<div>steal</div>".into())],
        )
        .await
        .unwrap_err();
        assert_eq!(err.0, StatusCode::CONFLICT);

        // Revision 2 removes acme-plugin/old -> becomes withdrawn + provider_inactive
        sync_owner_blocks_in_tx(
            &mut conn,
            "plugin",
            "acme-plugin",
            ProviderStatus::Active,
            "rev-2",
            &[(b1, "<div>{{ label }}</div>".into())],
        )
        .await
        .unwrap();

        // Disabling then re-enabling acme-plugin must NOT revive withdrawn acme-plugin/old
        set_owner_block_status_in_tx(
            &mut conn,
            "plugin",
            "acme-plugin",
            ProviderStatus::ProviderInactive,
        )
        .await
        .unwrap();
        set_owner_block_status_in_tx(&mut conn, "plugin", "acme-plugin", ProviderStatus::Active)
            .await
            .unwrap();

        let catalog = load_unified_block_catalog_in_conn(&mut conn).await.unwrap();
        let cta_entry = catalog.get("acme-plugin/cta").unwrap();
        assert_eq!(cta_entry.status, ProviderStatus::Active);
        assert_eq!(cta_entry.declaration_state, DeclarationState::Declared);

        let old_entry = catalog.get("acme-plugin/old").unwrap();
        assert_eq!(old_entry.status, ProviderStatus::ProviderInactive);
        assert_eq!(old_entry.declaration_state, DeclarationState::Withdrawn);
    }

    #[tokio::test]
    async fn provider_toggle_resyncs_media_acl_without_erasing_saved_homepage() {
        let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::storage::content::preflight_builtin_key_collisions(&db)
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        ensure_embedded_core_blocks(&db).await.unwrap();

        let image_id = "a".repeat(64);
        sqlx::query(
            "INSERT INTO media_assets(id,kind,filename,mime_type,extension,byte_size,width,height,thumb_width,thumb_height)
             VALUES (?, 'image', 'picture.png', 'image/png', 'png', 16, 1, 1, 1, 1)",
        )
        .bind(&image_id)
        .execute(&db)
        .await
        .unwrap();

        let schema: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "acme-blocks/banner",
            "title": "Banner",
            "category": "media",
            "render": "render.html",
            "fields": [
                { "type": "media", "key": "photo", "label": "Photo", "media_kind": "any" }
            ]
        }))
        .unwrap();
        let document: BuilderDocument = serde_json::from_value(json!({
            "schema_version": opendash_core::builder::BUILDER_SCHEMA_VERSION,
            "sections": [{
                "id": "s1", "children": [{
                    "id": "r1", "children": [{
                        "id": "c1", "children": [{
                            "id": "e1", "type": "plugin/block",
                            "props": { "block_id": "acme-blocks/banner", "values": { "photo": image_id } }
                        }]
                    }]
                }]
            }]
        }))
        .unwrap();
        let original_json = serde_json::to_string(&document).unwrap();
        sqlx::query("UPDATE builder_homepage SET document_json = ? WHERE id = 1")
            .bind(&original_json)
            .execute(&db)
            .await
            .unwrap();

        let mut conn = db.acquire().await.unwrap();
        sync_owner_blocks_in_tx(
            &mut conn,
            "plugin",
            "acme-blocks",
            ProviderStatus::Active,
            "revision-1",
            &[(schema, "<div>Static banner</div>".into())],
        )
        .await
        .unwrap();
        resync_plugin_block_acls_in_tx(&mut conn).await.unwrap();
        let inert_media: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM builder_homepage_media")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(
            inert_media, 0,
            "resync must leave builder_homepage_media inert"
        );

        // A published Page can project its public Builder body, and template
        // overrides track active vs inactive plugin blocks independently of the
        // inert builder_homepage row.
        sqlx::query(
            "INSERT INTO template_overrides(target_kind, canonical_target_key, target_json, ast_json, version)
             VALUES ('index', 'index', '{\"kind\":\"index\"}', ?, 1)",
        )
        .bind(&original_json)
        .execute(&mut *conn)
        .await
        .unwrap();
        resync_plugin_block_acls_in_tx(&mut conn).await.unwrap();
        let ov_initial: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM template_override_media_refs")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(ov_initial, 1);
        let body = serialize_builder_document_to_blocks(&document).unwrap();
        sqlx::query(
            "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, locale, translation_group_id)
             VALUES ('page', 'private-media-test', 'Media test', ?, 'draft', 'vi', 'eg-' || lower(hex(randomblob(8))))",
        )
        .bind(json!({ "body": body }).to_string())
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE content_entries SET publication_status = 'published' WHERE slug = 'private-media-test'",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        resync_plugin_block_acls_in_tx(&mut conn).await.unwrap();
        let public_refs: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM entry_builder_media_refs WHERE media_id = ?")
                .bind(&image_id)
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(public_refs, 1);

        let raw_page_definition: String =
            sqlx::query_scalar("SELECT definition_json FROM content_types WHERE key = 'page'")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        let mut page_definition: opendash_core::content::ContentTypeDefinition =
            serde_json::from_str(&raw_page_definition).unwrap();
        page_definition
            .fields
            .iter_mut()
            .find(|field| field.key == "body")
            .unwrap()
            .public_exposure = PublicExposure::Private;
        sqlx::query("UPDATE content_types SET definition_json = ? WHERE key = 'page'")
            .bind(serde_json::to_string(&page_definition).unwrap())
            .execute(&mut *conn)
            .await
            .unwrap();
        resync_plugin_block_acls_in_tx(&mut conn).await.unwrap();
        let private_refs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entry_builder_media_refs")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(private_refs, 0, "private body must not project media ACL");

        set_owner_block_status_in_tx(
            &mut conn,
            "plugin",
            "acme-blocks",
            ProviderStatus::ProviderInactive,
        )
        .await
        .unwrap();
        resync_plugin_block_acls_in_tx(&mut conn).await.unwrap();
        let revoked: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM template_override_media_refs")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(revoked, 0);

        set_owner_block_status_in_tx(&mut conn, "plugin", "acme-blocks", ProviderStatus::Active)
            .await
            .unwrap();
        resync_plugin_block_acls_in_tx(&mut conn).await.unwrap();
        let restored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM template_override_media_refs")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(restored, 1);
        let saved: String =
            sqlx::query_scalar("SELECT document_json FROM builder_homepage WHERE id=1")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(saved, original_json, "resync must not mutate saved AST");
    }
}
