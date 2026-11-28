use super::*;

fn normalized_description(raw: &str) -> ApiResult<String> {
    let trimmed = raw.trim();
    if trimmed.chars().count() > 500 || trimmed.chars().any(|c| c.is_control() && c != '\n') {
        return Err(ApiError::bad_request(
            "Mô tả term tối đa 500 ký tự và không chứa ký tự điều khiển.",
        ));
    }
    Ok(trimmed.to_owned())
}

#[allow(clippy::too_many_arguments)]
pub async fn create_term_with_locale(
    db: &SqlitePool,
    actor: &SessionContext,
    kind: &str,
    name: &str,
    slug: &str,
    parent_id: Option<i64>,
    locale: Option<&str>,
    source_term_id: Option<i64>,
    featured_image_media_id: Option<&str>,
) -> ApiResult<TaxonomyTerm> {
    create_term_full(
        db,
        actor,
        kind,
        name,
        slug,
        parent_id,
        None,
        None,
        locale,
        source_term_id,
        featured_image_media_id,
        None,
        None,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn create_term_full(
    db: &SqlitePool,
    actor: &SessionContext,
    kind: &str,
    name: &str,
    slug: &str,
    parent_id: Option<i64>,
    description: Option<&str>,
    sort_order: Option<i32>,
    locale: Option<&str>,
    source_term_id: Option<i64>,
    featured_image_media_id: Option<&str>,
    values: Option<&BTreeMap<String, Value>>,
    schema_token: Option<&str>,
    seo: Option<&TermSeoInput>,
) -> ApiResult<TaxonomyTerm> {
    if !opendash_core::content::valid_content_type_key(kind) {
        return Err(ApiError::bad_request("Khóa taxonomy không hợp lệ."));
    }
    let name = normalized_name(name)?;
    let desc = match description {
        Some(raw) => normalized_description(raw)?,
        None => String::new(),
    };
    let sort_order = sort_order.unwrap_or(0);
    if !opendash_core::id::valid_kebab_segment_with_limit(slug, 64) {
        return Err(ApiError::bad_request(
            "Slug phải là kebab-case, tối đa 64 ký tự.",
        ));
    }
    if source_term_id.is_some_and(|sid| sid <= 0) {
        return Err(ApiError::bad_request("source_term_id không hợp lệ."));
    }
    if actor.user.role == crate::auth::Role::Editor && seo.is_some() {
        return Err(ApiError::forbidden(
            "Editor không được chỉnh sửa SEO của term.",
        ));
    }
    let mut tx = begin_admin_write(db, actor, true).await?;
    let taxonomy = definition_in_write(&mut tx, kind)
        .await?
        .ok_or_else(|| ApiError::bad_request("Taxonomy không tồn tại."))?;
    if taxonomy.provider_status != ProviderStatus::Active
        || taxonomy.declaration_state != DeclarationState::Declared
    {
        return Err(ApiError::conflict("Taxonomy không còn khả dụng."));
    }
    if actor.user.role == crate::auth::Role::Editor && !taxonomy.settings.editor_can_create_terms {
        return Err(ApiError::forbidden(
            "Editor không có quyền tạo term cho phân loại này.",
        ));
    }
    if !taxonomy.hierarchical && parent_id.is_some() {
        return Err(ApiError::bad_request(
            "Taxonomy không phân cấp không nhận parent.",
        ));
    }
    let target_locale =
        crate::storage::settings::resolve_requested_locale_in_conn(&mut tx, locale).await?;
    let default_code = crate::storage::settings::get_default_locale_code_in_conn(&mut tx).await?;
    if target_locale != default_code || source_term_id.is_some() {
        crate::storage::settings::require_multilingual_enabled_in_conn(&mut tx).await?;
    }

    let mut effective_parent_id = parent_id;
    let group_id = if let Some(src_id) = source_term_id {
        let src: Option<(String, String, String, Option<i64>)> = sqlx::query_as(
            "SELECT kind, locale, translation_group_id, parent_id
               FROM taxonomy_terms WHERE id = ?
                 AND EXISTS (SELECT 1 FROM site_locales sl
                    WHERE sl.code = taxonomy_terms.locale AND sl.deleted_at IS NULL)",
        )
        .bind(src_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(mutation_error)?;
        let (src_kind, src_locale, src_group, src_parent_id) =
            src.ok_or_else(|| ApiError::not_found("Không tìm thấy term nguồn để dịch."))?;
        if src_kind != kind {
            return Err(ApiError::bad_request(
                "Term nguồn không thuộc cùng loại taxonomy.",
            ));
        }
        if src_locale == target_locale {
            return Err(ApiError::unprocessable(
                "Ngôn ngữ đích phải khác ngôn ngữ của term nguồn.",
            ));
        }
        let dup_group: i64 = sqlx::query_scalar(
            "SELECT EXISTS(
               SELECT 1 FROM taxonomy_terms
                WHERE kind = ? AND translation_group_id = ? AND locale = ?
             )",
        )
        .bind(kind)
        .bind(&src_group)
        .bind(&target_locale)
        .fetch_one(&mut *tx)
        .await
        .map_err(mutation_error)?;
        if dup_group != 0 {
            return Err(ApiError::conflict(
                "Nhóm dịch đã có bản dịch ở ngôn ngữ đích.",
            ));
        }
        if effective_parent_id.is_none()
            && taxonomy.hierarchical
            && let Some(sp_id) = src_parent_id
        {
            effective_parent_id = sqlx::query_scalar(
                "SELECT dst.id
                   FROM taxonomy_terms src
                   JOIN taxonomy_terms dst
                     ON dst.kind = src.kind
                    AND dst.translation_group_id = src.translation_group_id
                    AND dst.locale = ?
                  WHERE src.id = ?",
            )
            .bind(&target_locale)
            .bind(sp_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(mutation_error)?;
        }
        src_group
    } else {
        crate::storage::settings::generate_translation_group_id("tg")
    };

    if taxonomy.hierarchical {
        check_taxonomy_parent(&mut tx, kind, effective_parent_id, None, 1, &target_locale).await?;
    }
    let validated_featured_image = match featured_image_media_id {
        Some(raw) => Some(validate_featured_image_in_tx(&mut tx, raw).await?),
        None => None,
    };
    let id = sqlx::query(
        "INSERT INTO taxonomy_terms(kind, locale, translation_group_id, name, slug, parent_id, featured_image_media_id, description, sort_order) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(kind)
    .bind(&target_locale)
    .bind(&group_id)
    .bind(name)
    .bind(slug)
    .bind(effective_parent_id)
    .bind(validated_featured_image.as_deref())
    .bind(&desc)
    .bind(sort_order)
    .execute(&mut *tx)
    .await
    .map_err(mutation_error)?
    .last_insert_rowid();
    if values.is_some() || seo.is_some() {
        let actor_role = if actor.user.role == crate::auth::Role::Admin {
            FieldRole::Admin
        } else {
            FieldRole::Editor
        };
        let seo_patch = match seo {
            Some(s) => opendash_core::content::PatchField::Set(s.clone()),
            None => opendash_core::content::PatchField::Omitted,
        };
        super::super::meta::write_term_meta_and_seo_in_tx(
            &mut tx,
            id,
            kind,
            &target_locale,
            actor_role,
            schema_token,
            values,
            &seo_patch,
        )
        .await?;
    }
    let created = term_in_write(&mut tx, id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải term vừa tạo."))?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    tx.commit().await.map_err(mutation_error)?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(created)
}

#[allow(clippy::too_many_arguments)]
pub async fn patch_term(
    db: &SqlitePool,
    actor: &SessionContext,
    id: i64,
    expected_version: i64,
    name: Option<&str>,
    slug: Option<&str>,
    parent_id: Option<Option<i64>>,
    featured_image_media_id: Option<Option<&str>>,
    values: Option<&BTreeMap<String, Value>>,
    schema_token: Option<&str>,
) -> ApiResult<TaxonomyTerm> {
    patch_term_full(
        db,
        actor,
        id,
        expected_version,
        name,
        slug,
        parent_id,
        None,
        None,
        featured_image_media_id,
        values,
        schema_token,
        &opendash_core::content::PatchField::Omitted,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn patch_term_full(
    db: &SqlitePool,
    actor: &SessionContext,
    id: i64,
    expected_version: i64,
    name: Option<&str>,
    slug: Option<&str>,
    parent_id: Option<Option<i64>>,
    description: Option<&str>,
    sort_order: Option<i32>,
    featured_image_media_id: Option<Option<&str>>,
    values: Option<&BTreeMap<String, Value>>,
    schema_token: Option<&str>,
    seo_patch: &opendash_core::content::PatchField<TermSeoInput>,
) -> ApiResult<TaxonomyTerm> {
    let name = name.map(normalized_name).transpose()?;
    let next_description = description.map(normalized_description).transpose()?;
    if slug.is_some_and(|s| !opendash_core::id::valid_kebab_segment_with_limit(s, 64)) {
        return Err(ApiError::bad_request(
            "Slug phải là kebab-case, tối đa 64 ký tự.",
        ));
    }
    let editor_extension_only = actor.user.role == crate::auth::Role::Editor
        && name.is_none()
        && slug.is_none()
        && parent_id.is_none()
        && next_description.is_none()
        && sort_order.is_none()
        && featured_image_media_id.is_none()
        && seo_patch.is_omitted()
        && values.is_some();
    let mut tx = begin_admin_write(db, actor, editor_extension_only).await?;
    if actor.user.role != crate::auth::Role::Admin && !editor_extension_only {
        return Err(ApiError::forbidden(
            "Editor không được sửa trường taxonomy lõi.",
        ));
    }
    let existing = current_term(&mut tx, id, expected_version).await?;
    let next_parent = parent_id.unwrap_or(existing.parent_id);
    let next_featured_image = match featured_image_media_id {
        Some(Some(raw)) => Some(validate_featured_image_in_tx(&mut tx, raw).await?),
        Some(None) => None,
        None => existing.featured_image_media_id.clone(),
    };
    let definition = definition_in_write(&mut tx, &existing.kind)
        .await?
        .ok_or_else(|| ApiError::bad_request("Taxonomy không tồn tại."))?;
    if !definition.hierarchical && next_parent.is_some() {
        return Err(ApiError::bad_request(
            "Taxonomy không phân cấp không nhận parent.",
        ));
    }
    if definition.hierarchical && parent_id.is_some() {
        let height = subtree_height(&mut tx, id).await?;
        check_taxonomy_parent(
            &mut tx,
            &existing.kind,
            next_parent,
            Some(id),
            height,
            &existing.locale,
        )
        .await?;
    }
    if values.is_some() || !seo_patch.is_omitted() {
        let actor_role = if actor.user.role == crate::auth::Role::Admin {
            FieldRole::Admin
        } else {
            FieldRole::Editor
        };
        super::super::meta::write_term_meta_and_seo_in_tx(
            &mut tx,
            id,
            &existing.kind,
            &existing.locale,
            actor_role,
            schema_token,
            values,
            seo_patch,
        )
        .await?;
    }
    let updated = sqlx::query(
        "UPDATE taxonomy_terms
            SET name = ?, slug = ?, parent_id = ?, featured_image_media_id = ?,
                description = ?, sort_order = ?, version = version + 1
          WHERE id = ? AND version = ?",
    )
    .bind(name.unwrap_or(&existing.name))
    .bind(slug.unwrap_or(&existing.slug))
    .bind(next_parent)
    .bind(next_featured_image.as_deref())
    .bind(next_description.as_deref().unwrap_or(&existing.description))
    .bind(sort_order.unwrap_or(existing.sort_order))
    .bind(id)
    .bind(expected_version)
    .execute(&mut *tx)
    .await
    .map_err(mutation_error)?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::precondition_failed(
            "Term đã thay đổi, hãy tải lại.",
        ));
    }
    let record = term_in_write(&mut tx, id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải term vừa cập nhật."))?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    crate::storage::search::sync_entries_for_term_in_conn(&mut tx, id).await?;
    tx.commit().await.map_err(mutation_error)?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(record)
}
