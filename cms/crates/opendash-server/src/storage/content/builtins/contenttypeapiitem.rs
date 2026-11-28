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
            fields: Vec::new(),
            settings: ContentTypeSettings::private_pattern(),
        });
    }
    let label = match key {
        "post" => "Bài viết",
        "page" => "Trang",
        _ => return None,
    };
    let mut fi = opendash_core::field_schema::FieldSpec::new(
        opendash_core::field_schema::FieldType::Media,
        "featured_image",
        "Ảnh đại diện",
    );
    fi.media_kind = Some(opendash_core::field_schema::MediaFieldKind::Image);
    let mut fields = vec![
        opendash_core::field_schema::FieldSpec::new(
            opendash_core::field_schema::FieldType::Richtext,
            "body",
            "Nội dung",
        ),
        fi,
    ];
    opendash_core::field_schema::normalize_field_specs_for_scope(
        &mut fields,
        opendash_core::field_schema::FieldContext::Content,
        opendash_core::field_schema::FieldScopeKind::BaseCptField,
    )
    .expect("builtin fields must be valid");
    Some(ContentTypeDefinition {
        key: key.to_owned(),
        label: label.to_owned(),
        fields,
        settings: ContentTypeSettings::default_for_type_key(key),
    })
}

/// Only accepted by the pre-migration collision check while upgrading a site
/// from 0037. The 0044 migration removes this deprecated Core registration.
pub(in crate::storage) fn legacy_section_definition() -> ContentTypeDefinition {
    ContentTypeDefinition {
        key: "section".into(),
        label: "Section".into(),
        fields: Vec::new(),
        settings: ContentTypeSettings {
            public: false,
            has_archive: false,
            publication: ContentPublication::DraftPublish,
            supports: Vec::new(),
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
                let fields_ok = if let Ok(def) =
                    serde_json::from_value::<ContentTypeDefinition>(val.clone())
                {
                    def.fields == canonical.fields
                } else if matches!(key.as_str(), "post" | "page") {
                    obj.and_then(|o| o.get("fields"))
                        .and_then(|v| v.as_array())
                        .is_some_and(|arr| {
                            arr.len() == 2
                                && arr[0].get("key").and_then(|v| v.as_str()) == Some("body")
                                && arr[0].get("type").and_then(|v| v.as_str()) == Some("rich_text")
                                && arr[1].get("key").and_then(|v| v.as_str())
                                    == Some("featured_image")
                                && arr[1].get("type").and_then(|v| v.as_str()) == Some("image")
                        })
                } else {
                    false
                };
                let settings_ok = match obj.and_then(|o| o.get("settings")) {
                    None => true,
                    Some(s_val) => {
                        if let Ok(settings) =
                            serde_json::from_value::<ContentTypeSettings>(s_val.clone())
                        {
                            if matches!(key.as_str(), "section" | "pattern") {
                                settings == canonical.settings
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
            let effective = load_effective_content_schema_in_tx(&mut tx, &definition.key)
                .await?
                .ok_or_else(|| ApiError::internal("Không thể tải schema vừa tạo."))?;
            verify_effective_schema_transition(None, &effective, false)
                .map_err(|e| map_transition_error(e, true))?;
            crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
            crate::storage::search::sync_entries_for_type_in_conn(&mut tx, &definition.key).await?;
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
            || incoming.label != old_effective.base.descriptor.label
            || incoming.fields != old_effective.base.descriptor.fields)
    {
        return Err(ApiError::forbidden(
            "Post/Page chỉ hỗ trợ đổi cấu hình hành vi; schema Core phải giữ nguyên.",
        ));
    }
    if input.settings.is_none() {
        incoming.settings = old_effective.base.descriptor.settings.clone();
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

    let new_effective = load_effective_content_schema_in_tx(&mut tx, type_key)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải schema sau cập nhật."))?;
    verify_effective_schema_transition(Some(&old_effective), &new_effective, has_entries)
        .map_err(|e| map_transition_error(e, true))?;

    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    crate::storage::search::sync_entries_for_type_in_conn(&mut tx, type_key).await?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(ContentTypeApiItem {
        base: new_effective.base,
        version: new_effective.base_version,
        schema_token: new_effective.schema_token,
    })
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

pub async fn create_user_field_group(
    db: &SqlitePool,
    input: &UserFieldGroupInput,
) -> ApiResult<FieldGroupRecord> {
    let (scoped_id, fields) = input
        .validate_and_normalize(None)
        .map_err(map_input_validation_error)?;
    let group_id = scoped_id.to_canonical();
    let fields_json = serde_json::to_string(&fields)?;

    let mut tx = begin_content_write_tx(db).await?;
    let existing: Option<String> =
        sqlx::query_scalar("SELECT group_id FROM content_field_groups WHERE group_id = ?")
            .bind(&group_id)
            .fetch_optional(&mut *tx)
            .await?;
    if existing.is_some() {
        return Err(ApiError::conflict(
            "group_id hoặc local_key của nhóm trường đã tồn tại.",
        ));
    }

    let mut old_snapshots = HashMap::new();
    for target in &input.target_types {
        let Some(old_eff) = load_effective_content_schema_in_tx(&mut tx, target).await? else {
            return Err(ApiError::not_found("Loại nội dung mục tiêu không tồn tại."));
        };
        let has_entries: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM content_entries WHERE content_type_key = ?)",
        )
        .bind(target)
        .fetch_one(&mut *tx)
        .await?;
        old_snapshots.insert(target.clone(), (old_eff, has_entries));
    }

    sqlx::query(
        "INSERT INTO content_field_groups
         (group_id, owner_kind, owner_id, local_key, title, description, fields_json,
          default_region, default_order, collapsible, status, version)
         VALUES (?, 'user', NULL, ?, ?, ?, ?, ?, ?, ?, 'active', 1)",
    )
    .bind(&group_id)
    .bind(&scoped_id.local_key)
    .bind(&input.title)
    .bind(&input.description)
    .bind(&fields_json)
    .bind(input.default_region.as_str())
    .bind(input.default_order)
    .bind(if input.collapsible { 1_i64 } else { 0_i64 })
    .execute(&mut *tx)
    .await?;

    for target in &input.target_types {
        sqlx::query(
            "INSERT INTO content_field_group_targets (group_id, type_key, association_state)
             VALUES (?, ?, 'declared')",
        )
        .bind(&group_id)
        .bind(target)
        .execute(&mut *tx)
        .await?;
    }

    for target in &input.target_types {
        let (old_eff, has_entries) = old_snapshots
            .get(target)
            .expect("target snapshot preloaded");
        let new_eff = load_effective_content_schema_in_tx(&mut tx, target)
            .await?
            .ok_or_else(|| ApiError::internal("Không thể tải schema mục tiêu."))?;
        verify_effective_schema_transition(Some(old_eff), &new_eff, *has_entries)
            .map_err(|e| map_transition_error(e, true))?;
    }

    let created = load_field_group_in_tx(&mut tx, &group_id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải field group vừa tạo."))?;
    for target in &input.target_types {
        crate::storage::search::sync_entries_for_type_in_conn(&mut tx, target).await?;
    }
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(created)
}
