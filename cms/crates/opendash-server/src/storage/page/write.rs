use super::*;

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
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    match ensure_embedded_core_blocks_in_conn(&mut conn).await {
        Ok(()) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            Ok(())
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(err)
        }
    }
}

pub(in crate::storage) fn parse_owner(
    owner_kind: &str,
    owner_id: &str,
) -> ApiResult<ResourceOwner> {
    match owner_kind {
        "core" => Ok(ResourceOwner::Core),
        "theme" => Ok(ResourceOwner::Theme(owner_id.to_owned())),
        "plugin" => Ok(ResourceOwner::Plugin(owner_id.to_owned())),
        _ => Err(ApiError::internal(
            "Invalid owner_kind in builder_block_definitions",
        )),
    }
}

pub(in crate::storage) fn parse_provider_status(raw: &str) -> ApiResult<ProviderStatus> {
    match raw {
        "active" => Ok(ProviderStatus::Active),
        "provider_inactive" => Ok(ProviderStatus::ProviderInactive),
        _ => Err(ApiError::internal(
            "Invalid provider_status in builder_block_definitions",
        )),
    }
}

pub(in crate::storage) fn parse_declaration_state(raw: &str) -> ApiResult<DeclarationState> {
    match raw {
        "declared" => Ok(DeclarationState::Declared),
        "withdrawn" => Ok(DeclarationState::Withdrawn),
        _ => Err(ApiError::internal(
            "Invalid declaration_state in builder_block_definitions",
        )),
    }
}

pub(in crate::storage) fn format_provider_status(status: ProviderStatus) -> &'static str {
    match status {
        ProviderStatus::Active => "active",
        _ => "provider_inactive",
    }
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
pub(in crate::storage) fn active_document_refs(
    document: &BuilderDocument,
    catalog: &RegistryCatalog<RegisteredBlockSchema>,
) -> ExtractedBlockRefs {
    serialize_builder_document_to_blocks(document)
        .ok()
        .map(|body| extract_block_refs_lenient_with_schemas(&body, catalog))
        .unwrap_or_default()
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
            body.definition.field_type == opendash_core::field_schema::FieldType::Richtext
                && body.definition.effective_acl().2 == PublicExposure::Public
                && body.definition.effective_acl().0 == FieldRole::Editor
        })
}
