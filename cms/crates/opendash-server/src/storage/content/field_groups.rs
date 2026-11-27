use super::*;

#[derive(sqlx::FromRow)]
struct FieldGroupRow {
    group_id: String,
    owner_kind: String,
    owner_id: Option<String>,
    local_key: String,
    title: String,
    description: String,
    fields_json: String,
    domain: String,
    location_rules_json: String,
    presentation_json: String,
    active: i64,
    default_region: String,
    default_order: i32,
    collapsible: i64,
    status: String,
    version: i64,
    created_at: String,
    updated_at: String,
}

pub(in crate::storage) async fn load_field_group_in_tx(
    conn: &mut SqliteConnection,
    group_id: &str,
) -> ApiResult<Option<FieldGroupRecord>> {
    let row: Option<FieldGroupRow> = sqlx::query_as(
        "SELECT group_id, owner_kind, owner_id, local_key, title, description, fields_json,
                domain, location_rules_json, presentation_json, active,
                default_region, default_order, collapsible, status, version, created_at, updated_at
         FROM content_field_groups WHERE group_id = ?",
    )
    .bind(group_id)
    .fetch_optional(&mut *conn)
    .await?;

    let Some(FieldGroupRow {
        group_id,
        owner_kind,
        owner_id,
        local_key,
        title,
        description,
        fields_json,
        domain: domain_str,
        location_rules_json: location_json,
        presentation_json,
        active: active_int,
        default_region: default_region_str,
        default_order,
        collapsible: collapsible_int,
        status: status_str,
        version,
        created_at,
        updated_at,
    }) = row
    else {
        return Ok(None);
    };

    let target_rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT target_kind, target_key, association_state
         FROM content_field_group_targets
         WHERE group_id = ?
         ORDER BY target_kind ASC, target_key ASC",
    )
    .bind(&group_id)
    .fetch_all(&mut *conn)
    .await?;

    let mut targets = Vec::with_capacity(target_rows.len());
    for (target_kind, target_key, assoc_str) in target_rows {
        targets.push(FieldGroupTargetBinding {
            target_kind,
            target_key,
            association_state: parse_declaration_state(&assoc_str)?,
        });
    }

    let fields: Vec<opendash_core::field_schema::FieldSpec> = serde_json::from_str(&fields_json)
        .map_err(|_| ApiError::internal("Dữ liệu fields_json của field group không hợp lệ."))?;
    let domain = parse_domain(&domain_str)?;
    let location = parse_location_rules(&location_json)?;
    let presentation = parse_presentation(
        &presentation_json,
        &default_region_str,
        default_order,
        collapsible_int != 0,
    )?;
    let status = FieldGroupStatus::from_db(&status_str)
        .ok_or_else(|| ApiError::internal("Trạng thái field group không hợp lệ."))?;

    Ok(Some(FieldGroupRecord {
        group_id,
        owner: parse_owner(&owner_kind, owner_id)?,
        local_key,
        title,
        description,
        domain,
        location,
        presentation,
        active: active_int != 0,
        fields,
        targets,
        status,
        version,
        created_at,
        updated_at,
    }))
}

pub async fn list_field_groups(db: &SqlitePool) -> ApiResult<Vec<FieldGroupRecord>> {
    list_field_groups_by_domain(db, None).await
}

pub async fn list_field_groups_by_domain(
    db: &SqlitePool,
    domain: Option<opendash_core::content::FieldGroupDomain>,
) -> ApiResult<Vec<FieldGroupRecord>> {
    let ids: Vec<String> = match domain {
        Some(dom) => sqlx::query_scalar(
            "SELECT group_id FROM content_field_groups WHERE domain = ? ORDER BY group_id ASC",
        )
        .bind(domain_to_db(dom))
        .fetch_all(db)
        .await?,
        None => sqlx::query_scalar(
            "SELECT group_id FROM content_field_groups ORDER BY group_id ASC",
        )
        .fetch_all(db)
        .await?,
    };
    let mut conn = db.acquire().await?;
    let mut out = Vec::with_capacity(ids.len());
    for gid in ids {
        if let Some(rec) = load_field_group_in_tx(&mut conn, &gid).await? {
            out.push(rec);
        }
    }
    Ok(out)
}

/// Synchronizes a theme's CNT declarations (`content_types`, `taxonomy_eligibility`, and `field_groups`)
/// inside an active SQLite write transaction. All errors roll back the caller's transaction.
pub async fn sync_theme_declarations_in_tx(
    conn: &mut SqliteConnection,
    theme_id: &str,
    declarations: &ThemeDeclarations,
) -> ApiResult<()> {
    crate::storage::taxonomy::deactivate_other_theme_definitions_in_tx(conn, theme_id).await?;
    crate::storage::options_pages::deactivate_other_theme_options_pages_in_tx(conn, theme_id)
        .await?;
    crate::storage::permalinks::deactivate_other_theme_rewrites_in_tx(conn, theme_id).await?;
    // 1. Mark other themes' declared/withdrawn resources as provider_inactive (preserving ownership & declaration_state)
    let mut changed_keys: BTreeSet<String> = sqlx::query_scalar(
        "SELECT key FROM content_types WHERE owner_kind='theme' AND owner_id != ?
         AND provider_status != 'provider_inactive'",
    )
    .bind(theme_id)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .collect();
    let elig_keys: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT content_type_key FROM content_type_taxonomy_eligibility
         WHERE owner_kind='theme' AND owner_id != ?
           AND provider_status != 'provider_inactive'",
    )
    .bind(theme_id)
    .fetch_all(&mut *conn)
    .await?;
    changed_keys.extend(elig_keys);
    sqlx::query(
        "UPDATE content_types
         SET provider_status = 'provider_inactive', updated_at = datetime('now')
         WHERE owner_kind = 'theme' AND owner_id != ?
           AND provider_status != 'provider_inactive'",
    )
    .bind(theme_id)
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        "UPDATE content_type_taxonomy_eligibility
         SET provider_status = 'provider_inactive', updated_at = datetime('now')
         WHERE owner_kind = 'theme' AND owner_id != ?
           AND provider_status != 'provider_inactive'",
    )
    .bind(theme_id)
    .execute(&mut *conn)
    .await?;
    bump_provider_cpt_cas_in_tx(conn, changed_keys).await?;

    if table_exists_in_tx(conn, "content_field_groups").await? {
        sqlx::query(
            "UPDATE content_field_groups
             SET status = 'provider_inactive',
                 updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             WHERE owner_kind = 'theme' AND owner_id != ? AND status != 'withdrawn'",
        )
        .bind(theme_id)
        .execute(&mut *conn)
        .await?;
    }

    let neutral = opendash_core::content::CntDeclarations {
        content_types: declarations
            .content_types
            .iter()
            .map(|ct| opendash_core::content::ContentTypeDeclaration {
                key: ct.key.clone(),
                label: ct.label.clone(),
                singular_label: ct.singular_label.clone(),
                description: ct.description.clone(),
                taxonomies: ct.taxonomies.clone(),
                settings: ct.settings.clone(),
                rewrite_base: ct.rewrite_base.clone(),
            })
            .collect(),
        taxonomy_eligibility: declarations
            .taxonomy_eligibility
            .iter()
            .map(|e| opendash_core::content::TaxonomyEligibilityDeclaration {
                content_type: e.content_type.clone(),
                taxonomies: e.taxonomies.clone(),
            })
            .collect(),
        taxonomy_definitions: declarations.taxonomy_definitions.clone(),
        field_groups: declarations.field_groups.clone(),
        options_pages: declarations.options_pages.clone(),
    };
    sync_owner_cnt_declarations_in_tx(conn, "theme", theme_id, &neutral, false).await?;
    crate::storage::permalinks::sync_claims_in_tx(conn).await
}

/// Synchronizes a plugin's CNT declarations (`content_types`, `taxonomy_eligibility`, and `field_groups`)
/// inside an active SQLite write transaction without touching theme or other plugin declarations.
pub async fn sync_plugin_cnt_declarations_in_tx(
    conn: &mut SqliteConnection,
    plugin_id: &str,
    declarations: &opendash_core::content::CntDeclarations,
) -> ApiResult<()> {
    if declarations.is_empty() && !table_exists_in_tx(conn, "content_types").await? {
        return Ok(());
    }
    sync_owner_cnt_declarations_in_tx(conn, "plugin", plugin_id, declarations, true).await?;
    crate::storage::permalinks::sync_claims_in_tx(conn).await
}

/// Updates `provider_status` for all CNT declarations owned by a plugin without deleting schema or entries.
pub async fn set_plugin_cnt_provider_status_in_tx(
    conn: &mut SqliteConnection,
    plugin_id: &str,
    active: bool,
) -> ApiResult<()> {
    if !table_exists_in_tx(conn, "content_types").await? {
        return Ok(());
    }
    let desired = if active {
        "active"
    } else {
        "provider_inactive"
    };
    let declared_only: i64 = i64::from(active);
    let mut changed_keys: BTreeSet<String> = sqlx::query_scalar(
        "SELECT key FROM content_types WHERE owner_kind='plugin' AND owner_id=?
         AND provider_status != ? AND (? = 0 OR declaration_state='declared')",
    )
    .bind(plugin_id)
    .bind(desired)
    .bind(declared_only)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .collect();
    let changed_eligibility: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT content_type_key FROM content_type_taxonomy_eligibility
         WHERE owner_kind='plugin' AND owner_id=? AND provider_status != ?
           AND (? = 0 OR declaration_state='declared')",
    )
    .bind(plugin_id)
    .bind(desired)
    .bind(declared_only)
    .fetch_all(&mut *conn)
    .await?;
    changed_keys.extend(changed_eligibility);
    if active {
        sqlx::query(
            "UPDATE content_types
             SET provider_status = 'active', updated_at = datetime('now')
             WHERE owner_kind = 'plugin' AND owner_id = ? AND declaration_state = 'declared'
               AND provider_status != 'active'",
        )
        .bind(plugin_id)
        .execute(&mut *conn)
        .await?;

        sqlx::query(
            "UPDATE content_type_taxonomy_eligibility
             SET provider_status = 'active', updated_at = datetime('now')
             WHERE owner_kind = 'plugin' AND owner_id = ? AND declaration_state = 'declared'
               AND provider_status != 'active'",
        )
        .bind(plugin_id)
        .execute(&mut *conn)
        .await?;

        if table_exists_in_tx(conn, "content_field_groups").await? {
            sqlx::query(
                "UPDATE content_field_groups
                 SET status = 'active',
                     updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 WHERE owner_kind = 'plugin' AND owner_id = ? AND status != 'withdrawn'",
            )
            .bind(plugin_id)
            .execute(&mut *conn)
            .await?;
        }
    } else {
        sqlx::query(
            "UPDATE content_types
             SET provider_status = 'provider_inactive', updated_at = datetime('now')
             WHERE owner_kind = 'plugin' AND owner_id = ?
               AND provider_status != 'provider_inactive'",
        )
        .bind(plugin_id)
        .execute(&mut *conn)
        .await?;

        sqlx::query(
            "UPDATE content_type_taxonomy_eligibility
             SET provider_status = 'provider_inactive', updated_at = datetime('now')
             WHERE owner_kind = 'plugin' AND owner_id = ?
               AND provider_status != 'provider_inactive'",
        )
        .bind(plugin_id)
        .execute(&mut *conn)
        .await?;

        if table_exists_in_tx(conn, "content_field_groups").await? {
            sqlx::query(
                "UPDATE content_field_groups
                 SET status = 'provider_inactive',
                     updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 WHERE owner_kind = 'plugin' AND owner_id = ? AND status != 'withdrawn'",
            )
            .bind(plugin_id)
            .execute(&mut *conn)
            .await?;
        }
    }
    crate::storage::taxonomy::set_provider_definitions_status_in_tx(
        conn, "plugin", plugin_id, active,
    )
    .await?;
    crate::storage::options_pages::set_provider_options_pages_status_in_tx(
        conn, "plugin", plugin_id, active,
    )
    .await?;
    crate::storage::permalinks::set_provider_rewrite_status_in_tx(
        conn, "plugin", plugin_id, active,
    )
    .await?;
    bump_provider_cpt_cas_in_tx(conn, changed_keys).await?;
    crate::storage::permalinks::sync_claims_in_tx(conn).await?;
    Ok(())
}
