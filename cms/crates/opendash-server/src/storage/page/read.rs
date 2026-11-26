use super::*;

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
                    schema: RegisteredBlockSchema { schema },
                    template: (*raw_tpl).to_owned(),
                    revision_digest: digest,
                },
            );
        }
        map
    })
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
        let expected_owner_prefix = match &owner {
            ResourceOwner::Core | ResourceOwner::User => "core",
            ResourceOwner::Theme(id) | ResourceOwner::Plugin(id) => id.as_str(),
        };
        let schema: BlockSchemaV1 = match serde_json::from_str(&schema_json) {
            Ok(schema) => schema,
            Err(err) => {
                if owner_kind != "core" {
                    eprintln!(
                        "[opendash] skipping incompatible block schema {block_id} ({owner_kind}/{owner_id}): {err}"
                    );
                    continue;
                }
                return Err(err.into());
            }
        };
        if owner_kind != "core"
            && let Err(err) = schema.validate(expected_owner_prefix)
        {
            eprintln!(
                "[opendash] skipping invalid block schema {block_id} ({owner_kind}/{owner_id}): {err}"
            );
            continue;
        }
        out.insert(
            block_id.clone(),
            RegisteredBlockDef {
                block_id,
                owner,
                status,
                declaration_state,
                schema: RegisteredBlockSchema { schema },
                template,
                revision_digest,
            },
        );
    }
    if let Some(dev) = crate::dev_mode::DevOverlayConfig::global() {
        if let Ok(Some(theme_snap)) = dev.load_dev_theme() {
            for (schema, template) in theme_snap.blocks {
                out.insert(
                    schema.id.clone(),
                    RegisteredBlockDef {
                        block_id: schema.id.clone(),
                        owner: ResourceOwner::Theme(theme_snap.manifest.theme_id.clone()),
                        status: ProviderStatus::Active,
                        declaration_state: DeclarationState::Declared,
                        schema: RegisteredBlockSchema { schema },
                        template,
                        revision_digest: theme_snap.revision_digest.clone(),
                    },
                );
            }
        }
        let dev_engine = wasmi::Engine::default();
        for plugin_snap in dev.load_all_dev_plugins_best_effort(&dev_engine) {
            for (schema, template) in plugin_snap.blocks {
                out.insert(
                    schema.id.clone(),
                    RegisteredBlockDef {
                        block_id: schema.id.clone(),
                        owner: ResourceOwner::Plugin(plugin_snap.manifest.id.clone()),
                        status: ProviderStatus::Active,
                        declaration_state: DeclarationState::Declared,
                        schema: RegisteredBlockSchema { schema },
                        template,
                        revision_digest: plugin_snap.revision_digest.clone(),
                    },
                );
            }
        }
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

pub(in crate::storage) async fn active_media_roles_in_conn(
    conn: &mut SqliteConnection,
    refs: &ExtractedBlockRefs,
) -> ApiResult<BTreeSet<(String, &'static str)>> {
    let mut roles = BTreeSet::new();
    for (ids, role, required_kind) in [
        (&refs.image_ids, "image", Some("image")),
        (&refs.video_ids, "video", Some("video")),
        (&refs.poster_image_ids, "poster", Some("image")),
        (&refs.any_media_ids, "any", None),
        (&refs.file_ids, "file", None),
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
                let projected_role = if matches!(role, "any" | "file") {
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
