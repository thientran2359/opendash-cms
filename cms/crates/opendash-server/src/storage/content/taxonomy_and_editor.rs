use super::*;

pub(in crate::storage) async fn table_exists_in_tx(
    conn: &mut SqliteConnection,
    table_name: &str,
) -> ApiResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?)",
    )
    .bind(table_name)
    .fetch_one(conn)
    .await?)
}

pub(in crate::storage) fn parse_owner(
    owner_kind: &str,
    owner_id: Option<String>,
) -> ApiResult<ResourceOwner> {
    match (owner_kind, owner_id) {
        ("core", None) => Ok(ResourceOwner::Core),
        ("user", None) => Ok(ResourceOwner::User),
        ("theme", Some(id)) if !id.is_empty() => Ok(ResourceOwner::Theme(id)),
        ("plugin", Some(id)) if !id.is_empty() => Ok(ResourceOwner::Plugin(id)),
        _ => Err(ApiError::internal("Chủ sở hữu loại nội dung không hợp lệ.")),
    }
}

pub(in crate::storage) fn parse_provider_status(raw: &str) -> ApiResult<ProviderStatus> {
    match raw {
        "active" => Ok(ProviderStatus::Active),
        "provider_inactive" => Ok(ProviderStatus::ProviderInactive),
        _ => Err(ApiError::internal("Trạng thái loại nội dung không hợp lệ.")),
    }
}

pub(in crate::storage) fn parse_declaration_state(raw: &str) -> ApiResult<DeclarationState> {
    match raw {
        "declared" => Ok(DeclarationState::Declared),
        "withdrawn" => Ok(DeclarationState::Withdrawn),
        _ => Err(ApiError::internal(
            "Trạng thái declaration loại nội dung không hợp lệ.",
        )),
    }
}

pub(in crate::storage) fn parse_domain(
    raw: &str,
) -> ApiResult<opendash_core::content::FieldGroupDomain> {
    match raw {
        "content" => Ok(opendash_core::content::FieldGroupDomain::Content),
        "term" => Ok(opendash_core::content::FieldGroupDomain::Term),
        "options_page" => Ok(opendash_core::content::FieldGroupDomain::OptionsPage),
        _ => Err(ApiError::internal("domain của field group không hợp lệ.")),
    }
}

pub(in crate::storage) fn domain_to_db(
    domain: opendash_core::content::FieldGroupDomain,
) -> &'static str {
    match domain {
        opendash_core::content::FieldGroupDomain::Content => "content",
        opendash_core::content::FieldGroupDomain::Term => "term",
        opendash_core::content::FieldGroupDomain::OptionsPage => "options_page",
    }
}

pub(in crate::storage) fn parse_location_rules(
    raw: &str,
) -> ApiResult<opendash_core::content::LocationRuleGroups> {
    serde_json::from_str(raw)
        .map_err(|_| ApiError::internal("location_rules_json của field group không hợp lệ."))
}

pub(in crate::storage) fn parse_presentation(
    raw: &str,
    legacy_region: &str,
    legacy_order: i32,
    legacy_collapsible: bool,
) -> ApiResult<opendash_core::content::FieldGroupPresentation> {
    if raw == "{}" || raw.trim().is_empty() {
        let region = EditorRegion::from_db(legacy_region)
            .ok_or_else(|| ApiError::internal("default_region của field group không hợp lệ."))?;
        return Ok(opendash_core::content::FieldGroupPresentation {
            region,
            order: legacy_order,
            collapsible: legacy_collapsible,
            ..Default::default()
        });
    }
    serde_json::from_str(raw)
        .map_err(|_| ApiError::internal("presentation_json của field group không hợp lệ."))
}

pub(in crate::storage) async fn load_attached_groups_in_tx(
    conn: &mut SqliteConnection,
    target_kind: &str,
    target_key: &str,
    domain: opendash_core::content::FieldGroupDomain,
) -> ApiResult<Vec<AttachedFieldGroupSnapshot>> {
    if !table_exists_in_tx(conn, "content_field_groups").await?
        || !table_exists_in_tx(conn, "content_field_group_targets").await?
    {
        return Ok(Vec::new());
    }
    let domain_str = domain_to_db(domain);
    let group_rows: Vec<(
        String,
        String,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        i32,
        i64,
        String,
        i64,
        String,
    )> = sqlx::query_as(
        "SELECT g.group_id, g.owner_kind, g.owner_id, g.local_key, g.title, g.description,
                g.fields_json, g.location_rules_json, g.presentation_json, g.active,
                g.default_region, g.default_order, g.collapsible, g.status, g.version,
                t.association_state
           FROM content_field_group_targets t
           JOIN content_field_groups g ON g.group_id = t.group_id
          WHERE t.target_kind = ? AND t.target_key IN (?, '*') AND g.domain = ?
          ORDER BY g.group_id ASC",
    )
    .bind(target_kind)
    .bind(target_key)
    .bind(domain_str)
    .fetch_all(&mut *conn)
    .await?;

    let mut snapshots = Vec::with_capacity(group_rows.len());
    for (
        group_id,
        g_owner_kind,
        g_owner_id,
        local_key,
        title,
        description,
        fields_json,
        loc_json,
        pres_json,
        active_int,
        def_region_str,
        def_order,
        collapsible_int,
        g_status_str,
        version,
        assoc_str,
    ) in group_rows
    {
        let fields: Vec<opendash_core::field_schema::FieldSpec> =
            serde_json::from_str(&fields_json).map_err(|_| {
                ApiError::internal("Dữ liệu fields_json của field group không hợp lệ.")
            })?;
        let location = parse_location_rules(&loc_json)?;
        let presentation =
            parse_presentation(&pres_json, &def_region_str, def_order, collapsible_int != 0)?;
        let status = FieldGroupStatus::from_db(&g_status_str)
            .ok_or_else(|| ApiError::internal("Trạng thái field group không hợp lệ."))?;
        snapshots.push(AttachedFieldGroupSnapshot {
            group_id,
            owner: parse_owner(&g_owner_kind, g_owner_id)?,
            local_key,
            title,
            description,
            domain,
            location,
            presentation,
            active: active_int != 0,
            fields,
            status,
            association_state: parse_declaration_state(&assoc_str)?,
            version,
        });
    }
    Ok(snapshots)
}

pub fn map_input_validation_error(msg: &'static str) -> ApiError {
    if msg.starts_with("field_key_collision:") {
        ApiError::conflict(msg)
    } else if msg.starts_with("invalid_field_acl:")
        || msg.starts_with("invalid_field_default:")
        || msg.starts_with("admin_required_field_missing_default:")
        || msg.starts_with("reserved_panel_namespace:")
        || msg.starts_with("invalid_panel_region:")
        || msg.starts_with("duplicate_panel_id:")
        || msg.starts_with("sync_entry_terms_location_cycle:")
        || msg.starts_with("duplicate_sync_entry_terms_taxonomy:")
        || msg.starts_with("invalid_location_rules_domain:")
    {
        ApiError::unprocessable(msg)
    } else {
        ApiError::bad_request(msg)
    }
}

pub fn map_transition_error(err: SchemaTransitionError, incompat_as_conflict: bool) -> ApiError {
    match err {
        SchemaTransitionError::FieldKeyCollision(_)
        | SchemaTransitionError::CannotRemoveFieldWithData(_) => ApiError::conflict(err.message()),
        SchemaTransitionError::IncompatibleSchemaChange(_) => {
            if incompat_as_conflict {
                ApiError::conflict(err.message())
            } else {
                ApiError::unprocessable(err.message())
            }
        }
        SchemaTransitionError::InvalidFieldAcl(_)
        | SchemaTransitionError::InvalidFieldDefault(_)
        | SchemaTransitionError::AdminRequiredFieldMissingDefault(_)
        | SchemaTransitionError::RequiredFieldHiddenOrInaccessible(_)
        | SchemaTransitionError::InvalidLocationRulesDomain(_)
        | SchemaTransitionError::SyncEntryTermsLocationCycle(_)
        | SchemaTransitionError::DuplicateSyncEntryTermsTaxonomy(_) => {
            ApiError::unprocessable(err.message())
        }
    }
}

pub async fn load_effective_content_schema(
    db: &SqlitePool,
    type_key: &str,
) -> ApiResult<Option<EffectiveContentSchema>> {
    let mut conn = db.acquire().await?;
    load_effective_content_schema_in_tx(&mut conn, type_key).await
}

pub(in crate::storage) async fn ensure_non_reserved_cpt_key_in_tx(
    conn: &mut SqliteConnection,
    key: &str,
) -> ApiResult<()> {
    if crate::seo::is_reserved_namespace_key(key) {
        return Err(ApiError::conflict(
            "Khóa loại nội dung trùng với tiền tố hệ thống được bảo vệ.",
        ));
    }
    let tax_conflict: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM taxonomy_definitions WHERE key = ? LIMIT 1")
            .bind(key)
            .fetch_optional(&mut *conn)
            .await?;
    if tax_conflict.is_some() {
        return Err(ApiError::conflict(
            "Khóa loại nội dung trùng với khóa taxonomy đang tồn tại.",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ContentTypeTaxonomyEligibilityRow {
    pub content_type_key: String,
    pub taxonomy_kind: String,
    pub owner_kind: String,
    pub owner_id: Option<String>,
    pub provider_status: String,
    pub declaration_state: String,
}

pub async fn list_content_type_taxonomy_eligibility(
    db: &SqlitePool,
    content_type_key: &str,
) -> ApiResult<Vec<ContentTypeTaxonomyEligibilityRow>> {
    let rows: Vec<(String, String, String, Option<String>, String, String)> = sqlx::query_as(
        "SELECT content_type_key, taxonomy_kind, owner_kind, owner_id, provider_status, declaration_state
         FROM content_type_taxonomy_eligibility
         WHERE content_type_key = ?
         ORDER BY taxonomy_kind ASC",
    )
    .bind(content_type_key)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(
                content_type_key,
                taxonomy_kind,
                owner_kind,
                owner_id,
                provider_status,
                declaration_state,
            )| ContentTypeTaxonomyEligibilityRow {
                content_type_key,
                taxonomy_kind,
                owner_kind,
                owner_id,
                provider_status,
                declaration_state,
            },
        )
        .collect())
}

#[derive(Debug, Clone, Serialize)]
pub struct EditorLayoutSnapshot {
    pub layout: EditorLayoutConfigV1,
    pub panels: Vec<PanelDefinition>,
    pub version: i64,
}

pub(crate) async fn load_effective_taxonomies_in_tx(
    conn: &mut SqliteConnection,
    type_key: &str,
) -> ApiResult<Vec<String>> {
    if !table_exists_in_tx(conn, "content_type_taxonomy_eligibility").await? {
        return Ok(Vec::new());
    }
    let overrides_exist = table_exists_in_tx(conn, "content_type_taxonomy_overrides").await?;
    if overrides_exist {
        let override_keys: Option<String> = sqlx::query_scalar(
            "SELECT taxonomies_json FROM content_type_taxonomy_overrides WHERE content_type_key=?",
        )
        .bind(type_key)
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(raw) = override_keys {
            let keys: Vec<String> = serde_json::from_str(&raw)
                .map_err(|_| ApiError::internal("Danh sách taxonomy override bị hỏng."))?;
            let mut enabled = Vec::new();
            for key in keys {
                let exists: Option<i64> = sqlx::query_scalar(
                    "SELECT 1 FROM taxonomy_definitions WHERE key=?
                     AND provider_status='active' AND declaration_state='declared'",
                )
                .bind(&key)
                .fetch_optional(&mut *conn)
                .await?;
                if exists.is_some() {
                    enabled.push(key);
                }
            }
            enabled.sort();
            return Ok(enabled);
        }
    }
    let rows: Vec<String> = if table_exists_in_tx(conn, "taxonomy_definitions").await? {
        sqlx::query_scalar(
            "SELECT e.taxonomy_kind FROM content_type_taxonomy_eligibility e
             JOIN taxonomy_definitions d ON d.key=e.taxonomy_kind
             WHERE e.content_type_key=? AND e.provider_status='active'
               AND e.declaration_state='declared' AND d.provider_status='active'
               AND d.declaration_state='declared' ORDER BY e.taxonomy_kind",
        )
        .bind(type_key)
        .fetch_all(&mut *conn)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT taxonomy_kind FROM content_type_taxonomy_eligibility
             WHERE content_type_key=? AND provider_status='active'
               AND declaration_state='declared' ORDER BY taxonomy_kind",
        )
        .bind(type_key)
        .fetch_all(&mut *conn)
        .await?
    };
    Ok(rows)
}

/// C2 canonical effective site/provider eligibility used by schema tokens,
/// entry writes, Admin table, public lists, and future Builder selectors.
pub async fn load_effective_taxonomies(db: &SqlitePool, type_key: &str) -> ApiResult<Vec<String>> {
    let mut conn = db.acquire().await?;
    load_effective_taxonomies_in_tx(&mut conn, type_key).await
}

/// The complete persisted choice for Admin editing. Effective taxonomy keys
/// intentionally omit inactive providers, whereas the choice retained in a
/// site override must survive unrelated edits and later provider reactivation.
#[derive(Debug, Clone, Serialize)]
pub struct ConfiguredContentTypeTaxonomies {
    pub taxonomies: Vec<String>,
    pub has_override: bool,
    pub effective: Vec<String>,
    /// CPT CAS version read from the *same SQLite snapshot* as taxonomies.
    /// A client must never pair these keys with a separately fetched version.
    pub type_version: i64,
}

pub async fn load_configured_taxonomies(
    db: &SqlitePool,
    type_key: &str,
) -> ApiResult<ConfiguredContentTypeTaxonomies> {
    if !valid_content_type_key(type_key) || matches!(type_key, "pattern" | "section") {
        return Err(ApiError::bad_request(
            "Loại nội dung không hỗ trợ taxonomy.",
        ));
    }
    // Separate autocommit SELECTs can straddle a concurrent eligibility PUT:
    // the old association list and new CPT version would allow a valid CAS
    // write that silently removes another Admin's newly added association.
    // A single read transaction pins BOTH to one SQLite read snapshot.
    let mut tx = db.begin().await?;
    let type_version: Option<i64> =
        sqlx::query_scalar("SELECT version FROM content_types WHERE key=?")
            .bind(type_key)
            .fetch_optional(&mut *tx)
            .await?;
    let type_version =
        type_version.ok_or_else(|| ApiError::not_found("Loại nội dung không tồn tại."))?;
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT taxonomies_json FROM content_type_taxonomy_overrides WHERE content_type_key=?",
    )
    .bind(type_key)
    .fetch_optional(&mut *tx)
    .await?;
    let has_override = raw.is_some();
    let mut taxonomies: Vec<String> = if let Some(raw) = raw {
        serde_json::from_str(&raw)
            .map_err(|_| ApiError::internal("Danh sách taxonomy override bị hỏng."))?
    } else {
        // A provider may be temporarily inactive or withdraw a declaration.
        // Preserve its configured default association for a future reactivation.
        sqlx::query_scalar(
            "SELECT e.taxonomy_kind FROM content_type_taxonomy_eligibility e
             JOIN taxonomy_definitions d ON d.key=e.taxonomy_kind
             WHERE e.content_type_key=? ORDER BY e.taxonomy_kind",
        )
        .bind(type_key)
        .fetch_all(&mut *tx)
        .await?
    };
    taxonomies.sort();
    taxonomies.dedup();
    let effective = load_effective_taxonomies_in_tx(&mut tx, type_key).await?;
    tx.commit().await?;
    Ok(ConfiguredContentTypeTaxonomies {
        taxonomies,
        has_override,
        effective,
        type_version,
    })
}

pub(in crate::storage) async fn load_plugin_action_panels_for_type_in_tx(
    conn: &mut SqliteConnection,
    type_key: &str,
) -> ApiResult<Vec<PanelDefinition>> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id, manifest_json FROM plugins WHERE enabled = 1 ORDER BY id ASC")
            .fetch_all(&mut *conn)
            .await?;

    let mut out = Vec::new();
    for (plugin_id, raw_manifest) in rows {
        let Ok(manifest) = serde_json::from_str::<PluginManifest>(&raw_manifest) else {
            continue;
        };
        for ep in &manifest.editor_panels {
            if !ep.applies_to.iter().any(|t| t == type_key) {
                continue;
            }
            let Some(action) = manifest.action(&ep.action_id) else {
                continue;
            };
            let action_min_role = if action.required_role == "admin" {
                FieldRole::Admin
            } else {
                FieldRole::Editor
            };
            let min_edit_role = ep
                .min_edit_role
                .unwrap_or(action_min_role)
                .max(action_min_role);
            let min_view_role = ep.min_view_role.unwrap_or(FieldRole::Editor);
            out.push(PanelDefinition {
                panel_id: format!("plugin:{plugin_id}:{}", ep.local_key),
                owner: ResourceOwner::Plugin(plugin_id.clone()),
                title: ep.title.clone(),
                kind: PanelKind::Action {
                    action_id: ep.action_id.clone(),
                    button_label: ep.button_label.clone(),
                    confirm_message: ep.confirm_message.clone(),
                    public_effect: ep.public_effect,
                },
                applies_to: ep.applies_to.clone(),
                default_region: ep.default_region,
                allowed_regions: ep.allowed_regions.clone(),
                default_order: ep.default_order,
                min_view_role,
                min_edit_role,
                collapsible: ep.collapsible,
                presentation: Default::default(),
                required_core: false,
                effective_status: EffectiveFieldStatus::Active,
            });
        }
    }
    Ok(out)
}

pub(in crate::storage) async fn load_raw_editor_layout_in_tx(
    conn: &mut SqliteConnection,
    type_key: &str,
) -> ApiResult<(Option<EditorLayoutConfigV1>, i64)> {
    if !table_exists_in_tx(conn, "content_editor_layouts").await? {
        return Ok((None, 0));
    }
    let row: Option<(String, i64)> = sqlx::query_as(
        "SELECT layout_json, version FROM content_editor_layouts WHERE type_key = ?",
    )
    .bind(type_key)
    .fetch_optional(&mut *conn)
    .await?;
    match row {
        Some((raw_json, version)) => {
            let parsed: EditorLayoutConfigV1 = serde_json::from_str(&raw_json).map_err(|_| {
                ApiError::internal(format!(
                    "Dữ liệu content_editor_layouts của '{type_key}' không hợp lệ."
                ))
            })?;
            Ok((Some(parsed), version))
        }
        None => Ok((None, 0)),
    }
}
