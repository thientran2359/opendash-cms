use super::*;

pub(in crate::storage) fn definition_with_cpts(
    row: DefinitionRow,
    content_types: Vec<String>,
) -> ApiResult<TaxonomyDefinition> {
    let (
        key,
        label,
        hierarchical,
        public,
        settings_json,
        rewrite_base,
        version,
        owner_kind,
        owner_id,
        provider,
        state,
    ) = row;
    let owner = match (owner_kind.as_str(), owner_id) {
        ("core", None) => ResourceOwner::Core,
        ("user", None) => ResourceOwner::User,
        ("plugin", Some(id)) => ResourceOwner::Plugin(id),
        ("theme", Some(id)) => ResourceOwner::Theme(id),
        _ => return Err(ApiError::internal("Chủ sở hữu taxonomy không hợp lệ.")),
    };
    let provider_status = match provider.as_str() {
        "active" => ProviderStatus::Active,
        "provider_inactive" => ProviderStatus::ProviderInactive,
        _ => {
            return Err(ApiError::internal(
                "Trạng thái provider taxonomy không hợp lệ.",
            ));
        }
    };
    let declaration_state = match state.as_str() {
        "declared" => DeclarationState::Declared,
        "withdrawn" => DeclarationState::Withdrawn,
        _ => {
            return Err(ApiError::internal(
                "Trạng thái khai báo taxonomy không hợp lệ.",
            ));
        }
    };
    let settings: opendash_core::content::TaxonomySettings =
        if settings_json == "{}" || settings_json.trim().is_empty() {
            Default::default()
        } else {
            serde_json::from_str(&settings_json)
                .map_err(|_| ApiError::internal("settings_json của taxonomy không hợp lệ."))?
        };
    Ok(TaxonomyDefinition {
        key,
        label,
        hierarchical: hierarchical != 0,
        public: public != 0,
        settings,
        rewrite_base,
        content_types,
        version,
        owner,
        provider_status,
        declaration_state,
    })
}

pub(in crate::storage) fn definition(row: DefinitionRow) -> ApiResult<TaxonomyDefinition> {
    definition_with_cpts(row, Vec::new())
}

const DEF_SELECT: &str = "SELECT key, label, hierarchical, public, settings_json, rewrite_base, version, owner_kind, owner_id, provider_status, declaration_state FROM taxonomy_definitions";

pub(in crate::storage) async fn definition_in_conn(
    conn: &mut SqliteConnection,
    key: &str,
) -> ApiResult<Option<TaxonomyDefinition>> {
    let sql = format!("{DEF_SELECT} WHERE key = ?");
    let row: Option<DefinitionRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(key)
        .fetch_optional(&mut *conn)
        .await
        .map_err(mutation_error)?;
    let Some(r) = row else {
        return Ok(None);
    };
    let mut cpts_map = super::meta::load_taxonomy_content_types_map_in_conn(conn).await?;
    let cpts = cpts_map.remove(key).unwrap_or_default();
    Ok(Some(definition_with_cpts(r, cpts)?))
}

pub(in crate::storage) async fn definition_in_write(
    tx: &mut Transaction<'_, Sqlite>,
    key: &str,
) -> ApiResult<Option<TaxonomyDefinition>> {
    definition_in_conn(&mut **tx, key).await
}

pub async fn list_definitions(db: &SqlitePool) -> ApiResult<Vec<TaxonomyDefinition>> {
    let mut conn = db.acquire().await?;
    let sql = format!("{DEF_SELECT} ORDER BY key");
    let rows: Vec<DefinitionRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .fetch_all(&mut *conn)
        .await?;
    let mut cpts_map = super::meta::load_taxonomy_content_types_map_in_conn(&mut conn).await?;
    rows.into_iter()
        .map(|r| {
            let cpts = cpts_map.remove(&r.0).unwrap_or_default();
            definition_with_cpts(r, cpts)
        })
        .collect()
}

pub async fn get_definition(db: &SqlitePool, key: &str) -> ApiResult<Option<TaxonomyDefinition>> {
    let mut conn = db.acquire().await?;
    definition_in_conn(&mut conn, key).await
}

pub async fn sync_provider_definitions_in_tx(
    conn: &mut SqliteConnection,
    owner_kind: &str,
    owner_id: &str,
    declarations: &[opendash_core::content::TaxonomyDeclaration],
) -> ApiResult<()> {
    if !matches!(owner_kind, "theme" | "plugin") || owner_id.is_empty() {
        return Err(ApiError::bad_request(
            "Provider taxonomy owner không hợp lệ.",
        ));
    }
    let mut declared = std::collections::BTreeSet::new();
    for taxonomy in declarations {
        taxonomy.validate().map_err(ApiError::bad_request)?;
        if !declared.insert(taxonomy.key.as_str()) {
            return Err(ApiError::bad_request(
                "Provider khai báo taxonomy trùng key.",
            ));
        }
        ensure_non_reserved_taxonomy_key_in_tx(&mut *conn, &taxonomy.key).await?;
        let mut settings = taxonomy.settings.clone().unwrap_or_default();
        settings
            .validate_and_normalize()
            .map_err(ApiError::bad_request)?;
        let settings_json = serde_json::to_string(&settings)?;
        if let Some(current) = definition_in_conn(conn, &taxonomy.key).await? {
            if current.owner
                != match owner_kind {
                    "theme" => ResourceOwner::Theme(owner_id.to_owned()),
                    _ => ResourceOwner::Plugin(owner_id.to_owned()),
                }
            {
                return Err(ApiError::conflict("Taxonomy key đã thuộc owner khác."));
            }
            if current.hierarchical && !taxonomy.hierarchical {
                let children: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM taxonomy_terms WHERE kind=? AND parent_id IS NOT NULL",
                )
                .bind(&taxonomy.key)
                .fetch_one(&mut *conn)
                .await?;
                if children != 0 {
                    return Err(ApiError::conflict("Taxonomy còn term có parent."));
                }
            }
            let changed = current.label != taxonomy.label
                || current.hierarchical != taxonomy.hierarchical
                || current.public != taxonomy.public
                || current.settings != settings
                || current.rewrite_base != taxonomy.rewrite_base
                || current.provider_status != ProviderStatus::Active
                || current.declaration_state != DeclarationState::Declared;
            if changed {
                sqlx::query(
                    "UPDATE taxonomy_definitions SET label=?,hierarchical=?,public=?,settings_json=?,rewrite_base=?,
                     provider_status='active',declaration_state='declared',version=version+1
                     WHERE key=?",
                )
                .bind(&taxonomy.label)
                .bind(i64::from(taxonomy.hierarchical))
                .bind(i64::from(taxonomy.public))
                .bind(&settings_json)
                .bind(&taxonomy.rewrite_base)
                .bind(&taxonomy.key)
                .execute(&mut *conn)
                .await?;
            }
        } else {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM taxonomy_definitions")
                .fetch_one(&mut *conn)
                .await?;
            if count >= 64 {
                return Err(ApiError::conflict("Website đạt giới hạn 64 taxonomy."));
            }
            sqlx::query(
                "INSERT INTO taxonomy_definitions
                 (key,label,hierarchical,public,settings_json,rewrite_base,owner_kind,owner_id)
                 VALUES (?,?,?,?,?,?,?,?)",
            )
            .bind(&taxonomy.key)
            .bind(&taxonomy.label)
            .bind(i64::from(taxonomy.hierarchical))
            .bind(i64::from(taxonomy.public))
            .bind(&settings_json)
            .bind(&taxonomy.rewrite_base)
            .bind(owner_kind)
            .bind(owner_id)
            .execute(&mut *conn)
            .await?;
        }
    }
    let owned: Vec<String> = sqlx::query_scalar(
        "SELECT key FROM taxonomy_definitions WHERE owner_kind=? AND owner_id=?",
    )
    .bind(owner_kind)
    .bind(owner_id)
    .fetch_all(&mut *conn)
    .await?;
    for key in owned {
        if !declared.contains(key.as_str()) {
            sqlx::query(
                "UPDATE taxonomy_definitions SET declaration_state='withdrawn',
                 version=version+1 WHERE key=? AND declaration_state!='withdrawn'",
            )
            .bind(key)
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(())
}

pub async fn set_provider_definitions_status_in_tx(
    conn: &mut SqliteConnection,
    owner_kind: &str,
    owner_id: &str,
    active: bool,
) -> ApiResult<()> {
    let status = if active {
        "active"
    } else {
        "provider_inactive"
    };
    sqlx::query(
        "UPDATE taxonomy_definitions SET provider_status=?,version=version+1
         WHERE owner_kind=? AND owner_id=? AND provider_status!=?
           AND (?=0 OR declaration_state='declared')",
    )
    .bind(status)
    .bind(owner_kind)
    .bind(owner_id)
    .bind(status)
    .bind(i64::from(active))
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn deactivate_other_theme_definitions_in_tx(
    conn: &mut SqliteConnection,
    active_theme_id: &str,
) -> ApiResult<()> {
    sqlx::query(
        "UPDATE taxonomy_definitions SET provider_status='provider_inactive',version=version+1
         WHERE owner_kind='theme' AND owner_id!=? AND provider_status!='provider_inactive'",
    )
    .bind(active_theme_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn create_definition(
    db: &SqlitePool,
    actor: &SessionContext,
    input: &TaxonomyDefinitionInput,
) -> ApiResult<TaxonomyDefinition> {
    input.validate().map_err(ApiError::bad_request)?;
    let mut settings = input.settings.clone().unwrap_or_default();
    settings
        .validate_and_normalize()
        .map_err(ApiError::bad_request)?;
    let settings_json = serde_json::to_string(&settings)?;
    let rw_col: Option<String> = match &input.rewrite_base {
        opendash_core::content::PatchField::Set(s) => {
            Some(s.trim().to_owned()).filter(|x| !x.is_empty())
        }
        opendash_core::content::PatchField::Clear | opendash_core::content::PatchField::Omitted => {
            None
        }
    };
    let mut tx = begin_admin_write(db, actor, false).await?;
    ensure_non_reserved_taxonomy_key_in_tx(&mut tx, &input.key).await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM taxonomy_definitions")
        .fetch_one(&mut *tx)
        .await
        .map_err(mutation_error)?;
    if count >= 64 {
        return Err(ApiError::conflict("Website đạt giới hạn 64 taxonomy."));
    }
    if definition_in_write(&mut tx, &input.key).await?.is_some() {
        return Err(ApiError::conflict("Khóa taxonomy đã được đăng ký."));
    }
    if let Some(ref cpts) = input.content_types {
        super::meta::sync_taxonomy_content_types_in_tx(&mut tx, &input.key, cpts).await?;
    }
    sqlx::query(
        "INSERT INTO taxonomy_definitions (key,label,hierarchical,public,settings_json,rewrite_base,owner_kind)
         VALUES (?,?,?,?,?,?, 'user')",
    )
    .bind(&input.key)
    .bind(&input.label)
    .bind(i64::from(input.hierarchical))
    .bind(i64::from(input.public))
    .bind(&settings_json)
    .bind(&rw_col)
    .execute(&mut *tx)
    .await
    .map_err(mutation_error)?;
    crate::storage::permalinks::set_site_rewrite_override_in_tx(
        &mut tx,
        "taxonomy",
        &input.key,
        &input.rewrite_base,
    )
    .await?;
    let created = definition_in_write(&mut tx, &input.key)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể đọc taxonomy vừa tạo."))?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    tx.commit().await.map_err(mutation_error)?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(created)
}

pub async fn patch_definition(
    db: &SqlitePool,
    actor: &SessionContext,
    key: &str,
    expected_version: i64,
    input: &TaxonomyDefinitionPatch,
) -> ApiResult<TaxonomyDefinition> {
    input.validate().map_err(ApiError::bad_request)?;
    let mut tx = begin_admin_write(db, actor, false).await?;
    let current = definition_in_write(&mut tx, key)
        .await?
        .ok_or_else(|| ApiError::not_found("Taxonomy không tồn tại."))?;
    if current.version != expected_version {
        return Err(ApiError::precondition_failed(
            "Taxonomy đã thay đổi, hãy tải lại.",
        ));
    }
    if !matches!(current.owner, ResourceOwner::Core | ResourceOwner::User) {
        return Err(ApiError::conflict(
            "Taxonomy thuộc provider, không thể sửa từ site.",
        ));
    }
    let hierarchical = input.hierarchical.unwrap_or(current.hierarchical);
    if matches!(current.owner, ResourceOwner::Core) && hierarchical != current.hierarchical {
        return Err(ApiError::conflict(
            "Category/Tag có chế độ phân cấp bất biến.",
        ));
    }
    if current.hierarchical && !hierarchical {
        let parents: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM taxonomy_terms WHERE kind=? AND parent_id IS NOT NULL",
        )
        .bind(key)
        .fetch_one(&mut *tx)
        .await
        .map_err(mutation_error)?;
        if parents != 0 {
            return Err(ApiError::conflict("Taxonomy còn term có parent."));
        }
    }
    if let Some(ref cpts) = input.content_types {
        if matches!(current.owner, ResourceOwner::Core) && matches!(key, "category" | "post_tag") {
            // Allow linking additional CPTs to core category/post_tag cleanly
        }
        super::meta::sync_taxonomy_content_types_in_tx(&mut tx, key, cpts).await?;
    }
    let mut next_settings = input
        .settings
        .clone()
        .unwrap_or_else(|| current.settings.clone());
    next_settings
        .validate_and_normalize()
        .map_err(ApiError::bad_request)?;
    if let Some(def_tid) = next_settings.default_term_id {
        let valid_term: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM taxonomy_terms WHERE id = ? AND kind = ?)",
        )
        .bind(def_tid)
        .bind(key)
        .fetch_one(&mut *tx)
        .await
        .map_err(mutation_error)?;
        if !valid_term {
            return Err(ApiError::bad_request(
                "default_term_id không tồn tại hoặc không thuộc taxonomy này.",
            ));
        }
    }
    let settings_json = serde_json::to_string(&next_settings)?;
    let next_rewrite = match &input.rewrite_base {
        opendash_core::content::PatchField::Omitted => current.rewrite_base.clone(),
        opendash_core::content::PatchField::Clear => None,
        opendash_core::content::PatchField::Set(s) => {
            Some(s.trim().to_owned()).filter(|v| !v.is_empty())
        }
    };
    let label = input.label.as_deref().unwrap_or(&current.label);
    let public = input.public.unwrap_or(current.public);
    let rows = sqlx::query(
        "UPDATE taxonomy_definitions
            SET label=?, hierarchical=?, public=?, settings_json=?, rewrite_base=?, version=version+1
          WHERE key=? AND version=?",
    )
    .bind(label)
    .bind(i64::from(hierarchical))
    .bind(i64::from(public))
    .bind(&settings_json)
    .bind(&next_rewrite)
    .bind(key)
    .bind(expected_version)
    .execute(&mut *tx)
    .await
    .map_err(mutation_error)?
    .rows_affected();
    if rows != 1 {
        return Err(ApiError::precondition_failed(
            "Taxonomy đã thay đổi, hãy tải lại.",
        ));
    }
    crate::storage::permalinks::set_site_rewrite_override_in_tx(
        &mut tx,
        "taxonomy",
        key,
        &input.rewrite_base,
    )
    .await?;
    if current.public && !public {
        sqlx::query(
            "DELETE FROM seo_public_media_refs
              WHERE source_kind = 'term_seo'
                AND source_id IN (SELECT id FROM taxonomy_terms WHERE kind = ?)",
        )
        .bind(key)
        .execute(&mut *tx)
        .await?;
    } else if !current.public && public {
        sqlx::query(
            "INSERT OR IGNORE INTO seo_public_media_refs (source_kind, source_id, media_id)
             SELECT 'term_seo', id, seo_og_image_id
               FROM taxonomy_terms
              WHERE kind = ? AND seo_og_image_id IS NOT NULL",
        )
        .bind(key)
        .execute(&mut *tx)
        .await?;
    }
    let updated = definition_in_write(&mut tx, key)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải taxonomy vừa sửa."))?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    tx.commit().await.map_err(mutation_error)?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(updated)
}

pub async fn delete_definition(
    db: &SqlitePool,
    actor: &SessionContext,
    key: &str,
    expected_version: i64,
) -> ApiResult<()> {
    let mut tx = begin_admin_write(db, actor, false).await?;
    let current = definition_in_write(&mut tx, key)
        .await?
        .ok_or_else(|| ApiError::not_found("Taxonomy không tồn tại."))?;
    if current.version != expected_version {
        return Err(ApiError::precondition_failed(
            "Taxonomy đã thay đổi, hãy tải lại.",
        ));
    }
    if current.owner != ResourceOwner::User {
        return Err(ApiError::conflict("Không thể xóa taxonomy Core/provider."));
    }
    let dependencies: i64 = sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM taxonomy_terms WHERE kind=?) +
                (SELECT COUNT(*) FROM content_type_taxonomy_eligibility WHERE taxonomy_kind=?) +
                (SELECT COUNT(*) FROM content_type_taxonomy_overrides
                  WHERE EXISTS (SELECT 1 FROM json_each(taxonomies_json) WHERE value=?))",
    )
    .bind(key)
    .bind(key)
    .bind(key)
    .fetch_one(&mut *tx)
    .await
    .map_err(mutation_error)?;
    if dependencies > 0 {
        return Err(ApiError::conflict(
            "Taxonomy đang được dùng bởi term hoặc content type.",
        ));
    }
    if crate::storage::content::table_exists_in_tx(&mut tx, "content_field_group_targets").await? {
        sqlx::query(
            "DELETE FROM content_field_group_targets WHERE target_kind = 'taxonomy_term' AND target_key = ?",
        )
        .bind(key)
        .execute(&mut *tx)
        .await?;
    }
    let _ = crate::storage::permalinks::set_site_rewrite_override_in_tx(
        &mut tx,
        "taxonomy",
        key,
        &opendash_core::content::PatchField::Clear,
    )
    .await;
    sqlx::query("DELETE FROM taxonomy_definitions WHERE key=? AND version=?")
        .bind(key)
        .bind(expected_version)
        .execute(&mut *tx)
        .await
        .map_err(mutation_error)?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    tx.commit().await.map_err(mutation_error)?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(())
}

/// The first statement in the transaction reserves SQLite's single writer.
/// A deferred transaction that reads the tree first can race with a second
/// reparent and accept both sides of a newly created cycle.
pub(in crate::storage) async fn begin_admin_write<'a>(
    db: &'a SqlitePool,
    actor: &SessionContext,
    allow_editor_extensions: bool,
) -> ApiResult<Transaction<'a, Sqlite>> {
    let mut tx = db.begin().await.map_err(mutation_error)?;
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await
        .map_err(mutation_error)?;

    // Recheck the actor within the serialized write so session revocation and
    // privilege changes cannot race with a previously authenticated request.
    let current: Option<(i64, String)> = sqlx::query_as(
        "SELECT u.id, u.role FROM sessions s
         JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = ? AND s.expires_at > ?",
    )
    .bind(&actor.token_hash)
    .bind(auth::current_time_secs())
    .fetch_optional(&mut *tx)
    .await
    .map_err(mutation_error)?;
    match current {
        None => Err(ApiError::unauthorized("Phiên đăng nhập đã bị thu hồi.")),
        Some((id, _)) if id != actor.user.id => {
            Err(ApiError::unauthorized("Phiên đăng nhập đã bị thu hồi."))
        }
        Some((_, role)) if role != "admin" && !(allow_editor_extensions && role == "editor") => {
            Err(ApiError::forbidden(
                "Chỉ quản trị viên được quản lý taxonomy.",
            ))
        }
        Some(_) => Ok(tx),
    }
}
