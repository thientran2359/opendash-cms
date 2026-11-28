use super::*;

pub async fn load_effective_content_schema_in_tx(
    conn: &mut SqliteConnection,
    type_key: &str,
) -> ApiResult<Option<EffectiveContentSchema>> {
    let row: Option<(String, String, Option<String>, String, String, i64)> = sqlx::query_as(
        "SELECT definition_json, owner_kind, owner_id, provider_status, declaration_state, version
         FROM content_types WHERE key = ?",
    )
    .bind(type_key)
    .fetch_optional(&mut *conn)
    .await?;

    let Some((raw_def, owner_kind, owner_id, status_str, decl_str, base_version)) = row else {
        return Ok(None);
    };
    let descriptor: ContentTypeDefinition = serde_json::from_str(&raw_def)
        .map_err(|_| ApiError::internal("Dữ liệu loại nội dung không hợp lệ."))?;
    if descriptor.key != type_key || descriptor.validate().is_err() {
        return Err(ApiError::internal("Dữ liệu loại nội dung không hợp lệ."));
    }
    let base = OwnedDescriptor {
        owner: parse_owner(&owner_kind, owner_id)?,
        status: parse_provider_status(&status_str)?,
        declaration_state: parse_declaration_state(&decl_str)?,
        descriptor,
    };

    let attached_groups = load_attached_groups_in_tx(
        conn,
        "content_type",
        type_key,
        opendash_core::content::FieldGroupDomain::Content,
    )
    .await?;

    let mut effective = EffectiveContentSchema::assemble(base, base_version, attached_groups)
        .map_err(|e| map_transition_error(e, true))?;
    effective.schema_token = compute_schema_token_in_tx(conn, &effective).await?;
    Ok(Some(effective))
}

pub async fn list_content_types(
    db: &SqlitePool,
) -> ApiResult<Vec<OwnedDescriptor<ContentTypeDefinition>>> {
    let rows: Vec<(String, String, String, Option<String>, String, String)> = sqlx::query_as(
        "SELECT key, definition_json, owner_kind, owner_id, provider_status, declaration_state
         FROM content_types ORDER BY key ASC",
    )
    .fetch_all(db)
    .await?;

    rows.into_iter()
        .map(
            |(key, definition_json, owner_kind, owner_id, status, decl_state)| {
                let descriptor: ContentTypeDefinition = serde_json::from_str(&definition_json)
                    .map_err(|_| ApiError::internal("Dữ liệu loại nội dung không hợp lệ."))?;
                if descriptor.key != key || descriptor.validate().is_err() {
                    return Err(ApiError::internal("Dữ liệu loại nội dung không hợp lệ."));
                }
                Ok(OwnedDescriptor {
                    owner: parse_owner(&owner_kind, owner_id)?,
                    status: parse_provider_status(&status)?,
                    declaration_state: parse_declaration_state(&decl_state)?,
                    descriptor,
                })
            },
        )
        .collect()
}

pub async fn load_content_type_catalog(
    db: &SqlitePool,
) -> ApiResult<RegistryCatalog<ContentTypeDefinition>> {
    let items = list_content_types(db).await?;
    let mut catalog = RegistryCatalog::new();
    for item in items {
        catalog.insert_loaded(item.descriptor.key.clone(), item);
    }
    Ok(catalog)
}

pub async fn load_content_type_definition(
    db: &SqlitePool,
    key: &str,
) -> ApiResult<Option<ContentTypeDefinition>> {
    let mut conn = db.acquire().await?;
    load_content_type_definition_in_conn(&mut conn, key).await
}

pub(in crate::storage) async fn load_content_type_definition_in_conn(
    conn: &mut SqliteConnection,
    key: &str,
) -> ApiResult<Option<ContentTypeDefinition>> {
    let json: Option<String> =
        sqlx::query_scalar("SELECT definition_json FROM content_types WHERE key = ?")
            .bind(key)
            .fetch_optional(conn)
            .await?;
    json.map(|raw| {
        let definition: ContentTypeDefinition = serde_json::from_str(&raw)
            .map_err(|_| ApiError::internal("Dữ liệu loại nội dung không hợp lệ."))?;
        if definition.key != key || definition.validate().is_err() {
            return Err(ApiError::internal("Dữ liệu loại nội dung không hợp lệ."));
        }
        Ok(definition)
    })
    .transpose()
}

pub(in crate::storage) async fn has_persisted_group_values_for_type_in_tx(
    conn: &mut SqliteConnection,
    type_key: &str,
    fields: &[opendash_core::field_schema::FieldSpec],
) -> ApiResult<bool> {
    has_persisted_group_values_for_target_in_tx(conn, "content_type", type_key, fields).await
}

pub(in crate::storage) async fn has_persisted_group_values_for_target_in_tx(
    conn: &mut SqliteConnection,
    target_kind: &str,
    target_key: &str,
    fields: &[opendash_core::field_schema::FieldSpec],
) -> ApiResult<bool> {
    let field_keys: HashSet<&str> = fields
        .iter()
        .filter(|f| !f.is_ui_only())
        .map(|f| f.key.as_str())
        .collect();
    if field_keys.is_empty() {
        return Ok(false);
    }
    let values_list: Vec<String> = match target_kind {
        "content_type" => {
            if target_key == "*" {
                sqlx::query_scalar(
                    "SELECT values_json FROM content_entries WHERE content_type_key NOT IN ('pattern', 'section')",
                )
                .fetch_all(&mut *conn)
                .await?
            } else {
                sqlx::query_scalar(
                    "SELECT values_json FROM content_entries WHERE content_type_key = ?",
                )
                .bind(target_key)
                .fetch_all(&mut *conn)
                .await?
            }
        }
        "taxonomy_term" => {
            if target_key == "*" {
                sqlx::query_scalar(
                    "SELECT values_json FROM taxonomy_terms WHERE values_json != '{}'",
                )
                .fetch_all(&mut *conn)
                .await?
            } else {
                sqlx::query_scalar(
                    "SELECT values_json FROM taxonomy_terms WHERE kind = ? AND values_json != '{}'",
                )
                .bind(target_key)
                .fetch_all(&mut *conn)
                .await?
            }
        }
        "options_page" => {
            let mut out: Vec<String> = sqlx::query_scalar(
                "SELECT json_object(option_key, json(value_json))
                   FROM theme_option_values
                  WHERE owner_kind = 'options_page' AND owner_id = ?",
            )
            .bind(target_key)
            .fetch_all(&mut *conn)
            .await?;
            let loc: Vec<String> = sqlx::query_scalar(
                "SELECT json_object(option_key, json(value_json))
                   FROM theme_option_locale_values
                  WHERE owner_kind = 'options_page' AND owner_id = ?",
            )
            .bind(target_key)
            .fetch_all(&mut *conn)
            .await?;
            out.extend(loc);
            out
        }
        _ => Vec::new(),
    };
    for raw in values_list {
        if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(&raw)
            && map
                .iter()
                .any(|(k, v)| field_keys.contains(k.as_str()) && !v.is_null())
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(in crate::storage) async fn bump_provider_cpt_cas_in_tx(
    conn: &mut SqliteConnection,
    keys: BTreeSet<String>,
) -> ApiResult<()> {
    // Caller holds the install/activation writer reservation. Even a CPT
    // whose own declaration is withdrawn can have separately declared
    // eligibility, so bump the owning CPT for EITHER changed resource.
    for key in keys {
        sqlx::query(
            "UPDATE content_types SET version = version + 1,
             updated_at = datetime('now') WHERE key = ?",
        )
        .bind(key)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

pub async fn get_editor_layout(db: &SqlitePool, type_key: &str) -> ApiResult<EditorLayoutSnapshot> {
    if !valid_content_type_key(type_key) {
        return Err(ApiError::bad_request("Khóa loại nội dung không hợp lệ."));
    }
    let mut conn = db.acquire().await?;
    let Some(effective) = load_effective_content_schema_in_tx(&mut conn, type_key).await? else {
        return Err(ApiError::not_found(format!(
            "Không tìm thấy loại nội dung '{type_key}'."
        )));
    };
    let taxonomies = load_effective_taxonomies_in_tx(&mut conn, type_key).await?;
    let plugin_action_panels =
        load_plugin_action_panels_for_type_in_tx(&mut conn, type_key).await?;
    let (raw_layout, version) = load_raw_editor_layout_in_tx(&mut conn, type_key).await?;
    let base_panels =
        build_core_and_group_panels(&effective, !taxonomies.is_empty(), &plugin_action_panels);
    let (layout, panels) = resolve_and_validate_editor_layout(
        &effective,
        base_panels,
        raw_layout,
        version.max(0) as u64,
    )
    .map_err(|e| ApiError::unprocessable(e.message))?;

    Ok(EditorLayoutSnapshot {
        layout,
        panels,
        version,
    })
}

pub async fn get_effective_editor_config(
    db: &SqlitePool,
    type_key: &str,
    caller_role: FieldRole,
) -> ApiResult<EffectiveEditorConfigProjection> {
    if !valid_content_type_key(type_key) {
        return Err(ApiError::bad_request("Khóa loại nội dung không hợp lệ."));
    }
    let mut conn = db.acquire().await?;
    let Some(effective) = load_effective_content_schema_in_tx(&mut conn, type_key).await? else {
        return Err(ApiError::not_found(format!(
            "Không tìm thấy loại nội dung '{type_key}'."
        )));
    };
    let taxonomies = load_effective_taxonomies_in_tx(&mut conn, type_key).await?;
    let plugin_action_panels =
        load_plugin_action_panels_for_type_in_tx(&mut conn, type_key).await?;
    let (raw_layout, version) = load_raw_editor_layout_in_tx(&mut conn, type_key).await?;
    let base_panels =
        build_core_and_group_panels(&effective, !taxonomies.is_empty(), &plugin_action_panels);
    let (layout, panels) = resolve_and_validate_editor_layout(
        &effective,
        base_panels,
        raw_layout,
        version.max(0) as u64,
    )
    .map_err(|e| ApiError::unprocessable(e.message))?;

    Ok(project_effective_editor_config(
        &effective,
        &layout,
        &panels,
        &taxonomies,
        caller_role,
    ))
}
