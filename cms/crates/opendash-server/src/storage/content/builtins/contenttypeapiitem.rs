use super::*;

#[derive(Debug, Clone, Serialize)]
pub struct ContentTypeApiItem {
    #[serde(flatten)]
    pub base: OwnedDescriptor<ContentTypeDefinition>,
    pub version: i64,
    pub schema_token: String,
}

pub fn builtin_definition(key: &str) -> Option<ContentTypeDefinition> {
    if key == "pattern" {
        return Some(ContentTypeDefinition {
            key: "pattern".into(),
            label: "Patterns".into(),
            singular_label: None,
            description: None,
            settings: ContentTypeSettings::private_pattern(),
        });
    }
    let label = match key {
        "post" => "Bài viết",
        "page" => "Trang",
        _ => return None,
    };
    Some(ContentTypeDefinition {
        key: key.to_owned(),
        label: label.to_owned(),
        singular_label: None,
        description: None,
        settings: ContentTypeSettings::default_for_type_key(key),
    })
}

/// Only accepted by the pre-migration collision check while upgrading a site
/// from 0037. The 0044 migration removes this deprecated Core registration.
pub(in crate::storage) fn legacy_section_definition() -> ContentTypeDefinition {
    ContentTypeDefinition {
        key: "section".into(),
        label: "Section".into(),
        singular_label: None,
        description: None,
        settings: ContentTypeSettings {
            public: false,
            has_archive: false,
            publication: ContentPublication::DraftPublish,
            supports: Vec::new(),
            ..ContentTypeSettings::default()
        },
    }
}

/// Verify existing reserved keys before SQLx applies the forward migration.
/// No data is rewritten when a user/theme/plugin owns a conflicting key.
pub async fn preflight_builtin_key_collisions(
    db: &SqlitePool,
) -> Result<(), Box<dyn std::error::Error>> {
    let exists: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'content_types'",
    )
    .fetch_optional(db)
    .await?;
    if exists.is_none() {
        return Ok(());
    }
    let rows: Vec<(String, String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT key, definition_json, owner_kind, owner_id, provider_status
         FROM content_types WHERE key IN ('post', 'page', 'pattern', 'section')",
    )
    .fetch_all(db)
    .await?;
    for (key, raw, owner, owner_id, status) in rows {
        let canonical = if key == "section" {
            legacy_section_definition()
        } else {
            builtin_definition(&key).expect("known core key")
        };
        let matching_definition = serde_json::from_str::<serde_json::Value>(&raw)
            .ok()
            .map(|val| {
                let obj = val.as_object();
                let key_ok = obj.and_then(|o| o.get("key")).and_then(|v| v.as_str())
                    == Some(canonical.key.as_str());
                let label_ok = obj.and_then(|o| o.get("label")).and_then(|v| v.as_str())
                    == Some(canonical.label.as_str());
                let fields_ok = match obj.and_then(|o| o.get("fields")) {
                    None | Some(serde_json::Value::Null) => true,
                    Some(serde_json::Value::Array(arr)) => {
                        if matches!(key.as_str(), "section" | "pattern") {
                            arr.is_empty()
                        } else if matches!(key.as_str(), "post" | "page") {
                            arr.is_empty()
                                || (arr.len() == 2
                                    && arr[0].get("key").and_then(|v| v.as_str()) == Some("body")
                                    && matches!(
                                        arr[0].get("type").and_then(|v| v.as_str()),
                                        Some("rich_text" | "richtext")
                                    )
                                    && arr[1].get("key").and_then(|v| v.as_str())
                                        == Some("featured_image")
                                    && matches!(
                                        arr[1].get("type").and_then(|v| v.as_str()),
                                        Some("image" | "media")
                                    ))
                        } else {
                            false
                        }
                    }
                    Some(_) => false,
                };
                let settings_ok = match obj.and_then(|o| o.get("settings")) {
                    None => true,
                    Some(s_val) => {
                        if let Ok(settings) =
                            serde_json::from_value::<ContentTypeSettings>(s_val.clone())
                        {
                            if key == "section" {
                                settings == canonical.settings
                            } else if key == "pattern" {
                                !settings.public
                                    && !settings.has_archive
                                    && settings.publication == ContentPublication::Immediate
                                    && settings.supports
                                        == vec![opendash_core::content::ContentSupport::Builder]
                            } else {
                                settings.publication == ContentPublication::DraftPublish
                            }
                        } else {
                            false
                        }
                    }
                };
                key_ok && label_ok && fields_ok && settings_ok
            })
            == Some(true);
        if owner != "core" || owner_id.is_some() || status != "active" || !matching_definition {
            return Err(format!(
                "M3-S20 preflight: khóa '{key}' đang bị chiếm hoặc sai định nghĩa Core. \
                 Dừng migration, sao lưu DB và xử lý collision offline; không tự đổi dữ liệu."
            )
            .into());
        }
    }
    Ok(())
}

pub(in crate::storage) async fn begin_content_write_tx<'a>(
    db: &'a SqlitePool,
) -> ApiResult<Transaction<'a, Sqlite>> {
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

pub(in crate::storage) async fn compute_schema_token_in_tx(
    conn: &mut SqliteConnection,
    schema: &EffectiveContentSchema,
) -> ApiResult<String> {
    let taxonomies = load_effective_taxonomies_in_tx(conn, &schema.base.descriptor.key).await?;

    let active_theme_revision_id: Option<String> =
        if table_exists_in_tx(conn, "theme_active_selection").await? {
            sqlx::query_scalar("SELECT revision_id FROM theme_active_selection WHERE id = 1")
                .fetch_optional(&mut *conn)
                .await?
                .flatten()
        } else {
            None
        };

    let enabled_plugins: Vec<(String, i64)> = if table_exists_in_tx(conn, "plugins").await? {
        sqlx::query_as(
            "SELECT id, revision_generation FROM plugins WHERE enabled = 1 ORDER BY id ASC",
        )
        .fetch_all(&mut *conn)
        .await?
    } else {
        Vec::new()
    };

    let canonical = schema.canonical_token_payload_json(
        &taxonomies,
        active_theme_revision_id.as_deref(),
        &enabled_plugins,
    );
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{b:02x}");
    }
    Ok(format!("sha256:{hex}"))
}

/// Stores a user-created CPT definition (`POST /api/admin/content-types`).
pub async fn insert_user_content_type(
    db: &SqlitePool,
    input: &ContentTypeDefinitionInput,
) -> ApiResult<ContentTypeApiItem> {
    if builtin_definition(&input.key).is_some() || input.key == "section" {
        return Err(ApiError::conflict("post/page là khóa Core được bảo vệ."));
    }
    if crate::seo::is_reserved_namespace_key(&input.key) {
        return Err(ApiError::conflict(
            "Khóa loại nội dung trùng với tiền tố hệ thống được bảo vệ.",
        ));
    }
    let definition = input
        .clone()
        .into_canonical()
        .map_err(map_input_validation_error)?;
    let definition_json = serde_json::to_string(&definition)?;

    let mut tx = begin_content_write_tx(db).await?;
    ensure_non_reserved_cpt_key_in_tx(&mut tx, &definition.key).await?;
    crate::storage::seo::check_cpt_root_claim_in_tx(&mut tx, &definition.key).await?;
    let result = sqlx::query(
        "INSERT INTO content_types (key, definition_json, owner_kind, owner_id, provider_status, declaration_state, version)
         VALUES (?, ?, 'user', NULL, 'active', 'declared', 1)",
    )
    .bind(&definition.key)
    .bind(&definition_json)
    .execute(&mut *tx)
    .await;

    match result {
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            Err(ApiError::conflict("Khóa loại nội dung đã tồn tại."))
        }
        other => {
            other?;
            apply_cpt_taxonomies_and_rewrite_in_tx(&mut tx, &definition.key, input).await?;
            let effective = load_effective_content_schema_in_tx(&mut tx, &definition.key)
                .await?
                .ok_or_else(|| ApiError::internal("Không thể tải schema vừa tạo."))?;
            verify_effective_schema_transition(None, &effective, false)
                .map_err(|e| map_transition_error(e, true))?;
            crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
            tx.commit().await?;
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(ContentTypeApiItem {
                base: effective.base,
                version: effective.base_version,
                schema_token: effective.schema_token,
            })
        }
    }
}

async fn apply_cpt_taxonomies_and_rewrite_in_tx(
    tx: &mut SqliteConnection,
    type_key: &str,
    input: &ContentTypeDefinitionInput,
) -> ApiResult<()> {
    if let Some(taxonomies) = &input.taxonomies {
        let mut sorted = taxonomies.clone();
        sorted.sort();
        sorted.dedup();
        for tax_key in &sorted {
            let exists: Option<i64> =
                sqlx::query_scalar("SELECT 1 FROM taxonomy_definitions WHERE key = ?")
                    .bind(tax_key)
                    .fetch_optional(&mut *tx)
                    .await?;
            if exists.is_none() {
                return Err(ApiError::bad_request("Taxonomy được chọn không tồn tại."));
            }
        }
        let canonical = serde_json::to_string(&sorted)?;
        sqlx::query(
            "INSERT INTO content_type_taxonomy_overrides(content_type_key, taxonomies_json)
             VALUES (?, ?)
             ON CONFLICT(content_type_key) DO UPDATE SET
               taxonomies_json = excluded.taxonomies_json,
               updated_at = datetime('now')",
        )
        .bind(type_key)
        .bind(&canonical)
        .execute(&mut *tx)
        .await?;
    }
    crate::storage::permalinks::set_site_rewrite_override_in_tx(
        tx,
        "content_type",
        type_key,
        &input.rewrite_base,
    )
    .await?;
    Ok(())
}

/// Updates a `ResourceOwner::User` CPT definition (`PUT /api/admin/content-types/{type_key}`).
pub async fn update_user_content_type(
    db: &SqlitePool,
    type_key: &str,
    expected_version: i64,
    input: &ContentTypeDefinitionInput,
) -> ApiResult<ContentTypeApiItem> {
    if input.key != type_key {
        return Err(ApiError::bad_request(
            "Khóa loại nội dung trong body không khớp với đường dẫn URL.",
        ));
    }
    if matches!(type_key, "pattern" | "section") {
        return Err(ApiError::forbidden("Không thể thay đổi Pattern hệ thống."));
    }
    let mut incoming = input
        .clone()
        .into_canonical()
        .map_err(map_input_validation_error)?;

    let mut tx = begin_content_write_tx(db).await?;
    let existing: Option<(String, Option<String>, i64)> =
        sqlx::query_as("SELECT owner_kind, owner_id, version FROM content_types WHERE key = ?")
            .bind(type_key)
            .fetch_optional(&mut *tx)
            .await?;

    let Some((owner_kind, _owner_id, current_version)) = existing else {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung."));
    };
    let is_builtin = matches!(type_key, "post" | "page");
    if owner_kind != "user" && !(is_builtin && owner_kind == "core") {
        return Err(ApiError::forbidden(
            "Chỉ có thể cập nhật loại nội dung thuộc quyền sở hữu của User.",
        ));
    }
    if current_version != expected_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản loại nội dung đã thay đổi. Vui lòng tải lại.",
        ));
    }

    let old_effective = load_effective_content_schema_in_tx(&mut tx, type_key)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải schema hiện tại."))?;
    if is_builtin
        && (incoming.key != old_effective.base.descriptor.key
            || incoming.label != old_effective.base.descriptor.label)
    {
        return Err(ApiError::forbidden(
            "Post/Page chỉ hỗ trợ đổi cấu hình hành vi; tên và khóa Core phải giữ nguyên.",
        ));
    }
    if is_builtin && !matches!(input.rewrite_base, PatchField::Omitted) {
        return Err(ApiError::forbidden(
            "Post/Page giữ định tuyến chuẩn WordPress, không nhận rewrite_base riêng.",
        ));
    }
    if input.settings.is_none() {
        incoming.settings = old_effective.base.descriptor.settings.clone();
    }
    if old_effective.base.descriptor.settings.hierarchical && !incoming.settings.hierarchical {
        let has_children: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM content_entries WHERE content_type_key = ? AND parent_id IS NOT NULL)",
        )
        .bind(type_key)
        .fetch_one(&mut *tx)
        .await?;
        if has_children {
            return Err(ApiError::conflict(
                "incompatible_schema_change: Không thể tắt phân cấp khi đang có bài viết con (parent_id).",
            ));
        }
    }
    if old_effective.base.descriptor.settings.publication == ContentPublication::DraftPublish
        && incoming.settings.publication == ContentPublication::Immediate
    {
        let drafts: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM content_entries WHERE content_type_key = ? AND publication_status = 'draft')",
        )
        .bind(type_key)
        .fetch_one(&mut *tx)
        .await?;
        if drafts {
            return Err(ApiError::conflict(
                "Không thể chuyển sang lưu ngay khi loại nội dung còn bản nháp.",
            ));
        }
    }
    let incoming_json = serde_json::to_string(&incoming)?;
    let has_entries: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM content_entries WHERE content_type_key = ?)",
    )
    .bind(type_key)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE content_types
         SET definition_json = ?, version = version + 1, updated_at = datetime('now')
         WHERE key = ? AND version = ?",
    )
    .bind(&incoming_json)
    .bind(type_key)
    .bind(expected_version)
    .execute(&mut *tx)
    .await?;

    apply_cpt_taxonomies_and_rewrite_in_tx(&mut tx, type_key, input).await?;
    let new_effective = load_effective_content_schema_in_tx(&mut tx, type_key)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải schema sau cập nhật."))?;
    verify_effective_schema_transition(Some(&old_effective), &new_effective, has_entries)
        .map_err(|e| map_transition_error(e, true))?;

    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(ContentTypeApiItem {
        base: new_effective.base,
        version: new_effective.base_version,
        schema_token: new_effective.schema_token,
    })
}

pub async fn delete_user_content_type(
    db: &SqlitePool,
    type_key: &str,
    expected_version: i64,
    purge: bool,
) -> ApiResult<()> {
    if !valid_content_type_key(type_key) {
        return Err(ApiError::bad_request("Khóa loại nội dung không hợp lệ."));
    }
    if matches!(type_key, "post" | "page" | "pattern" | "section") {
        return Err(ApiError::forbidden("Không thể xóa loại nội dung hệ thống Core."));
    }
    let mut tx = begin_content_write_tx(db).await?;
    let row: Option<(String, i64)> =
        sqlx::query_as("SELECT owner_kind, version FROM content_types WHERE key = ?")
            .bind(type_key)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((owner_kind, current_version)) = row else {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung."));
    };
    if owner_kind != "user" {
        return Err(ApiError::forbidden(
            "Chỉ có thể xóa loại nội dung thuộc quyền sở hữu của User.",
        ));
    }
    if current_version != expected_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản loại nội dung đã thay đổi. Vui lòng tải lại.",
        ));
    }

    let entry_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM content_entries WHERE content_type_key = ?")
            .bind(type_key)
            .fetch_one(&mut *tx)
            .await?;
    if entry_count > 0 && !purge {
        return Err(ApiError::conflict(
            "cannot_delete_cpt_with_entries: Loại nội dung còn dữ liệu; hãy xóa bài viết trước hoặc thêm ?purge=true.",
        ));
    }
    if entry_count > 0 && purge {
        sqlx::query("UPDATE content_entries SET parent_id = NULL WHERE content_type_key = ?")
            .bind(type_key)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "DELETE FROM seo_public_media_refs
              WHERE source_kind = 'entry_seo'
                AND source_id IN (SELECT id FROM content_entries WHERE content_type_key = ?)",
        )
        .bind(type_key)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "DELETE FROM plugin_entry_projections
              WHERE entry_id IN (SELECT id FROM content_entries WHERE content_type_key = ?)",
        )
        .bind(type_key)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM content_entries WHERE content_type_key = ?")
            .bind(type_key)
            .execute(&mut *tx)
            .await?;
    }

    sqlx::query("DELETE FROM content_type_taxonomy_eligibility WHERE content_type_key = ?")
        .bind(type_key)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM content_type_taxonomy_overrides WHERE content_type_key = ?")
        .bind(type_key)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM content_editor_layouts WHERE type_key = ?")
        .bind(type_key)
        .execute(&mut *tx)
        .await?;
    if table_exists_in_tx(&mut tx, "content_field_group_targets").await? {
        sqlx::query(
            "DELETE FROM content_field_group_targets
              WHERE target_kind = 'content_type' AND target_key = ?",
        )
        .bind(type_key)
        .execute(&mut *tx)
        .await?;
    }
    let _ = crate::storage::permalinks::set_site_rewrite_override_in_tx(
        &mut tx,
        "content_type",
        type_key,
        &PatchField::Clear,
    )
    .await;
    sqlx::query("DELETE FROM content_types WHERE key = ? AND version = ?")
        .bind(type_key)
        .bind(expected_version)
        .execute(&mut *tx)
        .await?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(())
}

pub async fn list_content_types_with_meta(db: &SqlitePool) -> ApiResult<Vec<ContentTypeApiItem>> {
    let keys: Vec<String> = sqlx::query_scalar("SELECT key FROM content_types ORDER BY key ASC")
        .fetch_all(db)
        .await?;
    let mut items = Vec::with_capacity(keys.len());
    let mut conn = db.acquire().await?;
    for key in keys {
        if let Some(effective) = load_effective_content_schema_in_tx(&mut conn, &key).await? {
            items.push(ContentTypeApiItem {
                base: effective.base,
                version: effective.base_version,
                schema_token: effective.schema_token,
            });
        }
    }
    Ok(items)
}
