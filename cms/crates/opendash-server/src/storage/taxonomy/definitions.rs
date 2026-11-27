use super::*;

pub(in crate::storage) fn definition(row: DefinitionRow) -> ApiResult<TaxonomyDefinition> {
    let (key, label, hierarchical, public, version, owner_kind, owner_id, provider, state) = row;
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
    Ok(TaxonomyDefinition {
        key,
        label,
        hierarchical: hierarchical != 0,
        public: public != 0,
        version,
        owner,
        provider_status,
        declaration_state,
    })
}

pub(in crate::storage) async fn definition_in_write(
    tx: &mut Transaction<'_, Sqlite>,
    key: &str,
) -> ApiResult<Option<TaxonomyDefinition>> {
    let row: Option<DefinitionRow> = sqlx::query_as(
        "SELECT key, label, hierarchical, public, version, owner_kind, owner_id,
                provider_status, declaration_state FROM taxonomy_definitions WHERE key = ?",
    )
    .bind(key)
    .fetch_optional(&mut **tx)
    .await
    .map_err(mutation_error)?;
    row.map(definition).transpose()
}

pub async fn list_definitions(db: &SqlitePool) -> ApiResult<Vec<TaxonomyDefinition>> {
    let rows: Vec<DefinitionRow> = sqlx::query_as(
        "SELECT key, label, hierarchical, public, version, owner_kind, owner_id,
                provider_status, declaration_state FROM taxonomy_definitions ORDER BY key",
    )
    .fetch_all(db)
    .await?;
    rows.into_iter().map(definition).collect()
}

pub async fn get_definition(db: &SqlitePool, key: &str) -> ApiResult<Option<TaxonomyDefinition>> {
    let row: Option<DefinitionRow> = sqlx::query_as(
        "SELECT key, label, hierarchical, public, version, owner_kind, owner_id,
                provider_status, declaration_state FROM taxonomy_definitions WHERE key = ?",
    )
    .bind(key)
    .fetch_optional(db)
    .await?;
    row.map(definition).transpose()
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
    let mut changed_keys = std::collections::BTreeSet::new();
    for taxonomy in declarations {
        let input = TaxonomyDefinitionInput {
            key: taxonomy.key.clone(),
            label: taxonomy.label.clone(),
            hierarchical: taxonomy.hierarchical,
            public: taxonomy.public,
        };
        input.validate().map_err(ApiError::bad_request)?;
        if !declared.insert(taxonomy.key.as_str()) {
            return Err(ApiError::bad_request(
                "Provider khai báo taxonomy trùng key.",
            ));
        }
        ensure_non_reserved_taxonomy_key_in_tx(&mut *conn, &taxonomy.key).await?;
        let existing: Option<DefinitionRow> = sqlx::query_as(
            "SELECT key,label,hierarchical,public,version,owner_kind,owner_id,
                    provider_status,declaration_state FROM taxonomy_definitions WHERE key=?",
        )
        .bind(&taxonomy.key)
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(row) = existing {
            let current = definition(row)?;
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
                || current.provider_status != ProviderStatus::Active
                || current.declaration_state != DeclarationState::Declared;
            if changed {
                sqlx::query(
                    "UPDATE taxonomy_definitions SET label=?,hierarchical=?,public=?,
                     provider_status='active',declaration_state='declared',version=version+1
                     WHERE key=?",
                )
                .bind(&taxonomy.label)
                .bind(i64::from(taxonomy.hierarchical))
                .bind(i64::from(taxonomy.public))
                .bind(&taxonomy.key)
                .execute(&mut *conn)
                .await?;
                changed_keys.insert(taxonomy.key.clone());
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
                 (key,label,hierarchical,public,owner_kind,owner_id)
                 VALUES (?,?,?,?,?,?)",
            )
            .bind(&taxonomy.key)
            .bind(&taxonomy.label)
            .bind(i64::from(taxonomy.hierarchical))
            .bind(i64::from(taxonomy.public))
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
            .bind(&key)
            .execute(&mut *conn)
            .await?;
            changed_keys.insert(key);
        }
    }
    for tax_key in &changed_keys {
        crate::storage::search::sync_entries_for_taxonomy_in_conn(conn, tax_key).await?;
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
    let affected_keys: Vec<String> = sqlx::query_scalar(
        "SELECT key FROM taxonomy_definitions
          WHERE owner_kind=? AND owner_id=? AND provider_status!=?
            AND (?=0 OR declaration_state='declared')",
    )
    .bind(owner_kind)
    .bind(owner_id)
    .bind(status)
    .bind(i64::from(active))
    .fetch_all(&mut *conn)
    .await?;
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
    for key in &affected_keys {
        crate::storage::search::sync_entries_for_taxonomy_in_conn(conn, key).await?;
    }
    Ok(())
}

pub async fn deactivate_other_theme_definitions_in_tx(
    conn: &mut SqliteConnection,
    active_theme_id: &str,
) -> ApiResult<()> {
    let affected_keys: Vec<String> = sqlx::query_scalar(
        "SELECT key FROM taxonomy_definitions
          WHERE owner_kind='theme' AND owner_id!=? AND provider_status!='provider_inactive'",
    )
    .bind(active_theme_id)
    .fetch_all(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE taxonomy_definitions SET provider_status='provider_inactive',version=version+1
         WHERE owner_kind='theme' AND owner_id!=? AND provider_status!='provider_inactive'",
    )
    .bind(active_theme_id)
    .execute(&mut *conn)
    .await?;
    for key in &affected_keys {
        crate::storage::search::sync_entries_for_taxonomy_in_conn(conn, key).await?;
    }
    Ok(())
}

pub async fn create_definition(
    db: &SqlitePool,
    actor: &SessionContext,
    input: &TaxonomyDefinitionInput,
) -> ApiResult<TaxonomyDefinition> {
    input.validate().map_err(ApiError::bad_request)?;
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
    sqlx::query(
        "INSERT INTO taxonomy_definitions (key,label,hierarchical,public,owner_kind)
         VALUES (?,?,?,?, 'user')",
    )
    .bind(&input.key)
    .bind(&input.label)
    .bind(i64::from(input.hierarchical))
    .bind(i64::from(input.public))
    .execute(&mut *tx)
    .await
    .map_err(mutation_error)?;
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
    let label = input.label.as_deref().unwrap_or(&current.label);
    let public = input.public.unwrap_or(current.public);
    let rows = sqlx::query(
        "UPDATE taxonomy_definitions
            SET label=?, hierarchical=?, public=?, version=version+1
          WHERE key=? AND version=?",
    )
    .bind(label)
    .bind(i64::from(hierarchical))
    .bind(i64::from(public))
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
    let updated = definition_in_write(&mut tx, key)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải taxonomy vừa sửa."))?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    crate::storage::search::sync_entries_for_taxonomy_in_conn(&mut tx, key).await?;
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
    sqlx::query("DELETE FROM taxonomy_definitions WHERE key=? AND version=?")
        .bind(key)
        .bind(expected_version)
        .execute(&mut *tx)
        .await
        .map_err(mutation_error)?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    crate::storage::search::sync_entries_for_taxonomy_in_conn(&mut tx, key).await?;
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

pub async fn delete_term(
    db: &SqlitePool,
    actor: &SessionContext,
    id: i64,
    expected_version: i64,
) -> ApiResult<()> {
    let mut tx = begin_admin_write(db, actor, false).await?;
    current_term(&mut tx, id, expected_version).await?;
    let children: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM taxonomy_terms WHERE parent_id = ?")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(mutation_error)?;
    if children != 0 {
        return Err(ApiError::conflict(
            "Category còn mục con. Hãy chuyển mục con trước.",
        ));
    }
    // P10 creates entry_terms in its own migration. After that migration the
    // same deletion path checks dependencies explicitly, while FK RESTRICT
    // remains the database's final guard against concurrent entry updates.
    let has_memberships: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'entry_terms'",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(mutation_error)?;
    if has_memberships != 0 {
        let linked_entries: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM entry_terms WHERE term_id = ?")
                .bind(id)
                .fetch_one(&mut *tx)
                .await
                .map_err(mutation_error)?;
        if linked_entries != 0 {
            return Err(ApiError::conflict(
                "Term đang được gắn vào nội dung. Hãy gỡ liên kết trước.",
            ));
        }
    }
    let affected = sqlx::query("DELETE FROM taxonomy_terms WHERE id = ? AND version = ?")
        .bind(id)
        .bind(expected_version)
        .execute(&mut *tx)
        .await
        .map_err(mutation_error)?
        .rows_affected();
    if affected != 1 {
        return Err(ApiError::precondition_failed(
            "Term đã thay đổi, hãy tải lại.",
        ));
    }
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    crate::storage::search::sync_entries_for_term_in_conn(&mut tx, id).await?;
    tx.commit().await.map_err(mutation_error)?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(())
}
