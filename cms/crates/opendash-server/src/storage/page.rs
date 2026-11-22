use std::{collections::BTreeMap, sync::OnceLock};

use opendash_core::{
    block_schema::{BlockSchemaV1, RegisteredBlockSchema, upcast_legacy_descriptor},
    registry::{DeclarationState, OwnedDescriptor, ProviderStatus, RegistryCatalog, ResourceOwner},
    theme::BlockDescriptor,
};
use sha2::{Digest, Sha256};
use sqlx::{SqliteConnection, SqlitePool};

use crate::{
    error::{ApiError, ApiResult},
    theme::{Block, Fields, Theme},
};

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
    let legacy_rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id, fields_json FROM theme_blocks ORDER BY id")
            .fetch_all(&mut *conn)
            .await?;
    for (id, fields_json) in legacy_rows {
        if catalog.get(&id).is_some() {
            continue;
        }
        let Some((namespace, _)) = id.split_once('/') else {
            continue;
        };
        let owner = ResourceOwner::Theme(namespace.to_owned());
        let fields: Fields = serde_json::from_str(&fields_json)?;
        let desc = BlockDescriptor {
            id: id.clone(),
            fields,
        };
        catalog.insert_loaded(
            id,
            OwnedDescriptor {
                owner,
                status: ProviderStatus::Active,
                declaration_state: DeclarationState::Declared,
                descriptor: upcast_legacy_descriptor(&desc),
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

        let legacy_exists: Option<String> =
            sqlx::query_scalar("SELECT id FROM theme_blocks WHERE id = ?")
                .bind(&schema.id)
                .fetch_optional(&mut *conn)
                .await?;
        if legacy_exists.is_some()
            && (owner_kind != "theme" || !schema.id.starts_with(&format!("{owner_id}/")))
        {
            return Err(ApiError::conflict(
                "ID block xung đột với theme_blocks của gói khác.",
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

#[derive(Debug, Clone)]
pub struct BlockDefinition {
    pub fields: Fields,
    pub template: String,
}

/// SQLite is authoritative. A fresh snapshot avoids stale state across restart/install.
pub async fn load_block_catalog(db: &SqlitePool) -> ApiResult<RegistryCatalog<BlockDefinition>> {
    let rows: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id, fields_json, template FROM theme_blocks ORDER BY id")
            .fetch_all(db)
            .await?;
    let mut catalog = RegistryCatalog::new();
    for (id, fields, template) in rows {
        if !opendash_core::id::valid_namespaced_block_id(&id) {
            return Err(ApiError::internal("Invalid stored block ID"));
        }
        let owner = ResourceOwner::Theme(id.split_once('/').unwrap().0.to_owned());
        catalog
            .register(
                id,
                owner,
                BlockDefinition {
                    fields: serde_json::from_str(&fields)?,
                    template,
                },
            )
            .map_err(|e| ApiError::conflict(e.to_string()))?;
    }
    Ok(catalog)
}

pub async fn list_theme_blocks(db: &SqlitePool) -> ApiResult<BTreeMap<String, Fields>> {
    Ok(load_block_catalog(db)
        .await?
        .iter()
        .map(|(id, entry)| (id.clone(), entry.descriptor.fields.clone()))
        .collect())
}

pub async fn insert_theme_block(db: &SqlitePool, theme: &Theme) -> ApiResult<()> {
    let mut catalog = load_block_catalog(db).await?;
    let owner = ResourceOwner::Theme(
        theme
            .descriptor
            .id
            .split_once('/')
            .ok_or_else(|| ApiError::bad_request("Invalid block namespace"))?
            .0
            .into(),
    );
    catalog
        .register(
            theme.descriptor.id.clone(),
            owner,
            BlockDefinition {
                fields: theme.descriptor.fields.clone(),
                template: theme.template.clone(),
            },
        )
        .map_err(|e| ApiError::conflict(e.to_string()))?;
    // Explicit aliases must resolve to an already-installed block with the
    // EXACT field keys. Labels may differ; missing/extra fields cannot be
    // silently dropped. Validate before creating any persistent state.
    if let Some(alias) = &theme.compatible_block_id {
        let source = catalog
            .get(alias)
            .ok_or_else(|| ApiError::bad_request("Alias trỏ đến block chưa được cài đặt."))?;
        if source
            .descriptor
            .fields
            .keys()
            .ne(theme.descriptor.fields.keys())
        {
            return Err(ApiError::bad_request(
                "Field keys của alias không khớp block nguồn.",
            ));
        }
    }
    // The UNIQUE constraint still arbitrates concurrent installs after this snapshot check.
    let result = sqlx::query(
        "INSERT INTO theme_blocks
                     (id, fields_json, template, content_list_template, content_single_template, compatible_block_id)
                     VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&theme.descriptor.id)
    .bind(serde_json::to_string(&theme.descriptor.fields)?)
    .bind(&theme.template)
    .bind(&theme.content_list_template)
    .bind(&theme.content_single_template)
    .bind(&theme.compatible_block_id)
    .execute(db)
    .await;
    match result {
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            Err(ApiError::conflict("ID block đã có trong dữ liệu."))
        }
        result => {
            result?;
            Ok(())
        }
    }
}

/// Explicit active theme controls CPT templates. Without an explicit activation
/// preserve the pre-0011 home_page-selected template for migrated websites.
pub async fn active_content_templates(
    db: &SqlitePool,
) -> ApiResult<(Option<String>, Option<String>)> {
    let templates: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT
            CASE WHEN r.legacy_block_id IS NULL THEN legacy.content_list_template
                 ELSE active.content_list_template END,
            CASE WHEN r.legacy_block_id IS NULL THEN legacy.content_single_template
                 ELSE active.content_single_template END
         FROM theme_active_selection AS a
         LEFT JOIN theme_revisions AS r ON r.revision_id = a.revision_id
         LEFT JOIN theme_blocks AS active ON active.id = r.legacy_block_id
         LEFT JOIN home_page AS p ON p.id = 1
         LEFT JOIN theme_blocks AS legacy ON legacy.id = p.block_id
         WHERE a.id = 1",
    )
    .fetch_optional(db)
    .await?;
    Ok(templates.unwrap_or((None, None)))
}

pub async fn load_theme_block(db: &SqlitePool, block_id: &str) -> ApiResult<(String, Fields)> {
    let catalog = load_block_catalog(db).await?;
    let entry = catalog
        .get(block_id)
        .ok_or_else(|| ApiError::bad_request("Block not found"))?;
    if entry.status != opendash_core::registry::ProviderStatus::Active {
        return Err(ApiError::bad_request("Block provider is inactive"));
    }
    Ok((
        entry.descriptor.template.clone(),
        entry.descriptor.fields.clone(),
    ))
}

pub async fn load_home_page(db: &SqlitePool) -> ApiResult<Option<Block>> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT block_id, values_json FROM home_page WHERE id = 1")
            .fetch_optional(db)
            .await?;
    row.map(|(kind, values)| {
        Ok(Block {
            kind,
            values: serde_json::from_str(&values)?,
        })
    })
    .transpose()
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
        assert_eq!(core_cnt, 5);

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
}
