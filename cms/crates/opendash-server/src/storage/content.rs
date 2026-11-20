//! Persistent CPT definitions and extension field groups. All content SQL stays in the storage layer.

use std::collections::{BTreeSet, HashMap, HashSet};

use opendash_core::{
    content::{
        AttachedFieldGroupSnapshot, ContentFieldDefinition, ContentFieldType,
        ContentTypeDefinition, ContentTypeDefinitionInput, ContentTypeSettings, ContentPublication,
        EditorLayoutConfigV1, EditorRegion,
        EffectiveContentSchema, EffectiveEditorConfigProjection, EffectiveFieldStatus,
        FieldGroupRecord, FieldGroupStatus, FieldGroupTargetBinding, FieldRole, PanelDefinition,
        PanelKind, SchemaTransitionError, UserFieldGroupInput, build_core_and_group_panels,
        check_base_fields_compatibility, merge_package_redeclared_fields,
        project_effective_editor_config, resolve_and_validate_editor_layout, valid_content_type_key,
        verify_effective_schema_transition,
    },
    id::ScopedResourceId,
    plugin::PluginManifest,
    registry::{DeclarationState, OwnedDescriptor, ProviderStatus, RegistryCatalog, ResourceOwner},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

use crate::{
    error::{ApiError, ApiResult},
    theme::ThemeDeclarations,
};

#[derive(Debug, Clone, Serialize)]
pub struct ContentTypeApiItem {
    #[serde(flatten)]
    pub base: OwnedDescriptor<ContentTypeDefinition>,
    pub version: i64,
    pub schema_token: String,
}

pub fn builtin_definition(key: &str) -> Option<ContentTypeDefinition> {
    if key == "section" {
        return Some(ContentTypeDefinition {
            key: "section".into(),
            label: "Section".into(),
            fields: Vec::new(),
            settings: ContentTypeSettings::private_section(),
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
        fields: vec![
            ContentFieldDefinition {
                key: "body".into(),
                label: "Nội dung".into(),
                field_type: ContentFieldType::RichText,
                required: false,
                options: Vec::new(),
                ..Default::default()
            },
            ContentFieldDefinition {
                key: "featured_image".into(),
                label: "Ảnh đại diện".into(),
                field_type: ContentFieldType::Image,
                required: false,
                options: Vec::new(),
                ..Default::default()
            },
        ],
        settings: ContentTypeSettings::builtin_publication(),
    })
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
         FROM content_types WHERE key IN ('post', 'page', 'section')",
    )
    .fetch_all(db)
    .await?;
    for (key, raw, owner, owner_id, status) in rows {
        let matching_definition = serde_json::from_str::<ContentTypeDefinition>(&raw)
            .ok()
            .map(|mut definition| {
                // 0038 backfills the original 0008/0037 core JSON. Before
                // migration, absence of settings must not look like key theft.
                let has_settings = serde_json::from_str::<serde_json::Value>(&raw)
                    .ok()
                    .and_then(|value| value.get("settings").cloned())
                    .is_some();
                if !has_settings {
                    definition.settings = builtin_definition(&key)
                        .expect("known core key")
                        .settings;
                }
                let canonical = builtin_definition(&key).expect("known core key");
                let allowed_settings = if key == "section" {
                    definition.settings == canonical.settings
                } else {
                    definition.settings.publication == ContentPublication::DraftPublish
                };
                definition.key == canonical.key
                    && definition.label == canonical.label
                    && definition.fields == canonical.fields
                    && allowed_settings
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

async fn begin_content_write_tx<'a>(db: &'a SqlitePool) -> ApiResult<Transaction<'a, Sqlite>> {
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

async fn table_exists_in_tx(conn: &mut SqliteConnection, table_name: &str) -> ApiResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?)",
    )
    .bind(table_name)
    .fetch_one(conn)
    .await?)
}

fn parse_owner(owner_kind: &str, owner_id: Option<String>) -> ApiResult<ResourceOwner> {
    match (owner_kind, owner_id) {
        ("core", None) => Ok(ResourceOwner::Core),
        ("user", None) => Ok(ResourceOwner::User),
        ("theme", Some(id)) if !id.is_empty() => Ok(ResourceOwner::Theme(id)),
        ("plugin", Some(id)) if !id.is_empty() => Ok(ResourceOwner::Plugin(id)),
        _ => Err(ApiError::internal("Chủ sở hữu loại nội dung không hợp lệ.")),
    }
}

fn parse_provider_status(raw: &str) -> ApiResult<ProviderStatus> {
    match raw {
        "active" => Ok(ProviderStatus::Active),
        "provider_inactive" => Ok(ProviderStatus::ProviderInactive),
        _ => Err(ApiError::internal("Trạng thái loại nội dung không hợp lệ.")),
    }
}

fn parse_declaration_state(raw: &str) -> ApiResult<DeclarationState> {
    match raw {
        "declared" => Ok(DeclarationState::Declared),
        "withdrawn" => Ok(DeclarationState::Withdrawn),
        _ => Err(ApiError::internal(
            "Trạng thái declaration loại nội dung không hợp lệ.",
        )),
    }
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
        | SchemaTransitionError::RequiredFieldHiddenOrInaccessible(_) => {
            ApiError::unprocessable(err.message())
        }
    }
}

async fn compute_schema_token_in_tx(
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

    let attached_groups = if table_exists_in_tx(conn, "content_field_groups").await?
        && table_exists_in_tx(conn, "content_field_group_targets").await?
    {
        let group_rows: Vec<(
            String,
            String,
            Option<String>,
            String,
            String,
            String,
            String,
            String,
            i32,
            i64,
            String,
            i64,
            String,
        )> = sqlx::query_as(
            "SELECT g.group_id, g.owner_kind, g.owner_id, g.local_key, g.title, g.description,
                    g.fields_json, g.default_region, g.default_order, g.collapsible, g.status,
                    g.version, t.association_state
             FROM content_field_group_targets t
             JOIN content_field_groups g ON g.group_id = t.group_id
             WHERE t.type_key = ?
             ORDER BY g.group_id ASC",
        )
        .bind(type_key)
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
            default_region_str,
            default_order,
            collapsible_int,
            g_status_str,
            version,
            assoc_str,
        ) in group_rows
        {
            let fields: Vec<ContentFieldDefinition> = serde_json::from_str(&fields_json)
                .map_err(|_| ApiError::internal("Dữ liệu fields_json của field group không hợp lệ."))?;
            let default_region = EditorRegion::from_db(&default_region_str)
                .ok_or_else(|| ApiError::internal("default_region của field group không hợp lệ."))?;
            let status = FieldGroupStatus::from_db(&g_status_str)
                .ok_or_else(|| ApiError::internal("Trạng thái field group không hợp lệ."))?;
            snapshots.push(AttachedFieldGroupSnapshot {
                group_id,
                owner: parse_owner(&g_owner_kind, g_owner_id)?,
                local_key,
                title,
                description,
                fields,
                default_region,
                default_order,
                collapsible: collapsible_int != 0,
                status,
                association_state: parse_declaration_state(&assoc_str)?,
                version,
            });
        }
        snapshots
    } else {
        Vec::new()
    };

    let mut effective = EffectiveContentSchema::assemble(base, base_version, attached_groups)
        .map_err(|e| map_transition_error(e, true))?;
    effective.schema_token = compute_schema_token_in_tx(conn, &effective).await?;
    Ok(Some(effective))
}

pub async fn load_effective_content_schema(
    db: &SqlitePool,
    type_key: &str,
) -> ApiResult<Option<EffectiveContentSchema>> {
    let mut conn = db.acquire().await?;
    load_effective_content_schema_in_tx(&mut conn, type_key).await
}

/// Stores a user-created CPT definition (`POST /api/admin/content-types`).
pub async fn insert_user_content_type(
    db: &SqlitePool,
    input: &ContentTypeDefinitionInput,
) -> ApiResult<ContentTypeApiItem> {
    if builtin_definition(&input.key).is_some() {
        return Err(ApiError::conflict("post/page là khóa Core được bảo vệ."));
    }
    let definition = input
        .clone()
        .into_canonical()
        .map_err(map_input_validation_error)?;
    let definition_json = serde_json::to_string(&definition)?;

    let mut tx = begin_content_write_tx(db).await?;
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
            tx.commit().await?;
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
    if type_key == "section" {
        return Err(ApiError::forbidden(
            "Không thể thay đổi Section riêng tư.",
        ));
    }
    let mut incoming = input
        .clone()
        .into_canonical()
        .map_err(map_input_validation_error)?;

    let mut tx = begin_content_write_tx(db).await?;
    let existing: Option<(String, Option<String>, i64)> = sqlx::query_as(
        "SELECT owner_kind, owner_id, version FROM content_types WHERE key = ?",
    )
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
    if is_builtin && (incoming.key != old_effective.base.descriptor.key
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
    tx.commit().await?;
    Ok(ContentTypeApiItem {
        base: new_effective.base,
        version: new_effective.base_version,
        schema_token: new_effective.schema_token,
    })
}

pub async fn list_content_types_with_meta(
    db: &SqlitePool,
) -> ApiResult<Vec<ContentTypeApiItem>> {
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
    let json: Option<String> =
        sqlx::query_scalar("SELECT definition_json FROM content_types WHERE key = ?")
            .bind(key)
            .fetch_optional(db)
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

async fn load_field_group_in_tx(
    conn: &mut SqliteConnection,
    group_id: &str,
) -> ApiResult<Option<FieldGroupRecord>> {
    let row: Option<(
        String,
        String,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
        i32,
        i64,
        String,
        i64,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT group_id, owner_kind, owner_id, local_key, title, description, fields_json,
                default_region, default_order, collapsible, status, version, created_at, updated_at
         FROM content_field_groups WHERE group_id = ?",
    )
    .bind(group_id)
    .fetch_optional(&mut *conn)
    .await?;

    let Some((
        group_id,
        owner_kind,
        owner_id,
        local_key,
        title,
        description,
        fields_json,
        default_region_str,
        default_order,
        collapsible_int,
        status_str,
        version,
        created_at,
        updated_at,
    )) = row
    else {
        return Ok(None);
    };

    let target_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT type_key, association_state
         FROM content_field_group_targets
         WHERE group_id = ?
         ORDER BY type_key ASC",
    )
    .bind(&group_id)
    .fetch_all(&mut *conn)
    .await?;

    let mut targets = Vec::with_capacity(target_rows.len());
    for (type_key, assoc_str) in target_rows {
        targets.push(FieldGroupTargetBinding {
            type_key,
            association_state: parse_declaration_state(&assoc_str)?,
        });
    }

    let fields: Vec<ContentFieldDefinition> = serde_json::from_str(&fields_json)
        .map_err(|_| ApiError::internal("Dữ liệu fields_json của field group không hợp lệ."))?;
    let default_region = EditorRegion::from_db(&default_region_str)
        .ok_or_else(|| ApiError::internal("default_region của field group không hợp lệ."))?;
    let status = FieldGroupStatus::from_db(&status_str)
        .ok_or_else(|| ApiError::internal("Trạng thái field group không hợp lệ."))?;

    Ok(Some(FieldGroupRecord {
        group_id,
        owner: parse_owner(&owner_kind, owner_id)?,
        local_key,
        title,
        description,
        fields,
        targets,
        default_region,
        default_order,
        collapsible: collapsible_int != 0,
        status,
        version,
        created_at,
        updated_at,
    }))
}

pub async fn list_field_groups(db: &SqlitePool) -> ApiResult<Vec<FieldGroupRecord>> {
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT group_id FROM content_field_groups ORDER BY group_id ASC")
            .fetch_all(db)
            .await?;
    let mut conn = db.acquire().await?;
    let mut out = Vec::with_capacity(ids.len());
    for gid in ids {
        if let Some(rec) = load_field_group_in_tx(&mut conn, &gid).await? {
            out.push(rec);
        }
    }
    Ok(out)
}

async fn has_persisted_group_values_for_type_in_tx(
    conn: &mut SqliteConnection,
    type_key: &str,
    fields: &[ContentFieldDefinition],
) -> ApiResult<bool> {
    if fields.is_empty() {
        return Ok(false);
    }
    let values_list: Vec<String> =
        sqlx::query_scalar("SELECT values_json FROM content_entries WHERE content_type_key = ?")
            .bind(type_key)
            .fetch_all(&mut *conn)
            .await?;
    let field_keys: HashSet<&str> = fields.iter().map(|f| f.key.as_str()).collect();
    for raw in values_list {
        if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(&raw) {
            if map
                .iter()
                .any(|(k, v)| field_keys.contains(k.as_str()) && !v.is_null())
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
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
        return Err(ApiError::conflict("group_id hoặc local_key của nhóm trường đã tồn tại."));
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
    tx.commit().await?;
    Ok(created)
}

pub async fn update_user_field_group(
    db: &SqlitePool,
    group_id_param: &str,
    expected_version: i64,
    input: &UserFieldGroupInput,
) -> ApiResult<FieldGroupRecord> {
    let (scoped_id, incoming_fields) = input
        .validate_and_normalize(Some(group_id_param))
        .map_err(map_input_validation_error)?;
    let group_id = scoped_id.to_canonical();
    let fields_json = serde_json::to_string(&incoming_fields)?;

    let mut tx = begin_content_write_tx(db).await?;
    let existing = load_field_group_in_tx(&mut tx, &group_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy nhóm trường."))?;
    if existing.owner != ResourceOwner::User {
        return Err(ApiError::forbidden(
            "Chỉ có thể cập nhật nhóm trường thuộc quyền sở hữu của User.",
        ));
    }
    if existing.status == FieldGroupStatus::Withdrawn {
        return Err(ApiError::conflict(
            "Nhóm trường đã bị rút khai báo; không thể cập nhật.",
        ));
    }
    if existing.version != expected_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản nhóm trường đã thay đổi. Vui lòng tải lại.",
        ));
    }

    let old_targets: Vec<String> = existing.targets.iter().map(|t| t.type_key.clone()).collect();
    let mut all_affected_types = BTreeSet::new();
    for t in &old_targets {
        all_affected_types.insert(t.clone());
    }
    for t in &input.target_types {
        all_affected_types.insert(t.clone());
    }

    let mut old_snapshots = HashMap::new();
    let mut any_target_has_entries = false;
    for t in &all_affected_types {
        let Some(old_eff) = load_effective_content_schema_in_tx(&mut tx, t).await? else {
            return Err(ApiError::not_found("Loại nội dung mục tiêu không tồn tại."));
        };
        let has_entries: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM content_entries WHERE content_type_key = ?)",
        )
        .bind(t)
        .fetch_one(&mut *tx)
        .await?;
        if has_entries {
            any_target_has_entries = true;
        }
        old_snapshots.insert(t.clone(), (old_eff, has_entries));
    }

    // Check if any field key was removed completely when any linked type has entries
    let incoming_keys: HashSet<&str> = incoming_fields.iter().map(|f| f.key.as_str()).collect();
    for old_f in &existing.fields {
        if !incoming_keys.contains(old_f.key.as_str()) && any_target_has_entries {
            return Err(ApiError::conflict(format!(
                "cannot_remove_field_with_data: Không thể xóa hẳn trường '{}' khi đã có dữ liệu; hãy đặt declaration_state = withdrawn.",
                old_f.key
            )));
        }
    }

    sqlx::query(
        "UPDATE content_field_groups
         SET title = ?, description = ?, fields_json = ?, default_region = ?,
             default_order = ?, collapsible = ?, version = version + 1,
             updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
         WHERE group_id = ? AND version = ?",
    )
    .bind(&input.title)
    .bind(&input.description)
    .bind(&fields_json)
    .bind(input.default_region.as_str())
    .bind(input.default_order)
    .bind(if input.collapsible { 1_i64 } else { 0_i64 })
    .bind(&group_id)
    .bind(expected_version)
    .execute(&mut *tx)
    .await?;

    let new_target_set: HashSet<&str> = input.target_types.iter().map(|s| s.as_str()).collect();
    for t in &input.target_types {
        sqlx::query(
            "INSERT INTO content_field_group_targets (group_id, type_key, association_state)
             VALUES (?, ?, 'declared')
             ON CONFLICT(group_id, type_key) DO UPDATE SET
                 association_state = 'declared',
                 updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
        )
        .bind(&group_id)
        .bind(t)
        .execute(&mut *tx)
        .await?;
    }

    for old_t in &old_targets {
        if !new_target_set.contains(old_t.as_str()) {
            let has_values =
                has_persisted_group_values_for_type_in_tx(&mut tx, old_t, &existing.fields).await?;
            let has_layout_ref: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM content_editor_layouts WHERE type_key = ? AND instr(layout_json, ?) > 0)",
            )
            .bind(old_t)
            .bind(&group_id)
            .fetch_one(&mut *tx)
            .await?;

            if has_values || has_layout_ref {
                sqlx::query(
                    "UPDATE content_field_group_targets
                     SET association_state = 'withdrawn',
                         updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                     WHERE group_id = ? AND type_key = ?",
                )
                .bind(&group_id)
                .bind(old_t)
                .execute(&mut *tx)
                .await?;
            } else {
                sqlx::query(
                    "DELETE FROM content_field_group_targets WHERE group_id = ? AND type_key = ?",
                )
                .bind(&group_id)
                .bind(old_t)
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    for t in &all_affected_types {
        let (old_eff, has_entries) = old_snapshots.get(t).expect("snapshot preloaded");
        let new_eff = load_effective_content_schema_in_tx(&mut tx, t)
            .await?
            .ok_or_else(|| ApiError::internal("Không thể tải schema sau cập nhật."))?;
        verify_effective_schema_transition(Some(old_eff), &new_eff, *has_entries)
            .map_err(|e| map_transition_error(e, true))?;
    }

    let updated = load_field_group_in_tx(&mut tx, &group_id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải field group sau cập nhật."))?;
    tx.commit().await?;
    Ok(updated)
}

pub async fn withdraw_user_field_group(
    db: &SqlitePool,
    group_id_param: &str,
    expected_version: i64,
) -> ApiResult<FieldGroupRecord> {
    let parsed = if group_id_param.contains(':') {
        ScopedResourceId::parse(group_id_param).map_err(map_input_validation_error)?
    } else {
        ScopedResourceId::new(ResourceOwner::User, group_id_param.to_owned())
            .map_err(map_input_validation_error)?
    };
    if parsed.owner != ResourceOwner::User {
        return Err(ApiError::forbidden(
            "Chỉ có thể rút nhóm trường thuộc quyền sở hữu của User.",
        ));
    }
    let group_id = parsed.to_canonical();

    let mut tx = begin_content_write_tx(db).await?;
    let existing = load_field_group_in_tx(&mut tx, &group_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy nhóm trường."))?;
    if existing.owner != ResourceOwner::User {
        return Err(ApiError::forbidden(
            "Chỉ có thể rút nhóm trường thuộc quyền sở hữu của User.",
        ));
    }
    if existing.version != expected_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản nhóm trường đã thay đổi. Vui lòng tải lại.",
        ));
    }

    sqlx::query(
        "UPDATE content_field_groups
         SET status = 'withdrawn', version = version + 1,
             updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
         WHERE group_id = ? AND version = ?",
    )
    .bind(&group_id)
    .bind(expected_version)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE content_field_group_targets
         SET association_state = 'withdrawn',
             updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
         WHERE group_id = ?",
    )
    .bind(&group_id)
    .execute(&mut *tx)
    .await?;

    let withdrawn = load_field_group_in_tx(&mut tx, &group_id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải field group sau khi rút."))?;
    tx.commit().await?;
    Ok(withdrawn)
}

/// Checks schema evolution compatibility for same-owner re-declaration.
pub fn check_content_type_compatibility(
    existing: &ContentTypeDefinition,
    incoming: &ContentTypeDefinition,
    has_existing_entries: bool,
) -> Result<(), &'static str> {
    check_base_fields_compatibility(&existing.fields, &incoming.fields, has_existing_entries)
}

async fn bump_provider_cpt_cas_in_tx(
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

/// Synchronizes a theme's CNT declarations (`content_types`, `taxonomy_eligibility`, and `field_groups`)
/// inside an active SQLite write transaction. All errors roll back the caller's transaction.
pub async fn sync_theme_declarations_in_tx(
    conn: &mut SqliteConnection,
    theme_id: &str,
    declarations: &ThemeDeclarations,
) -> ApiResult<()> {
    crate::storage::taxonomy::deactivate_other_theme_definitions_in_tx(conn, theme_id).await?;
    crate::storage::permalinks::deactivate_other_theme_rewrites_in_tx(conn, theme_id).await?;
    // 1. Mark other themes' declared/withdrawn resources as provider_inactive (preserving ownership & declaration_state)
    let mut changed_keys: BTreeSet<String> = sqlx::query_scalar(
        "SELECT key FROM content_types WHERE owner_kind='theme' AND owner_id != ?
         AND provider_status != 'provider_inactive'",
    ).bind(theme_id).fetch_all(&mut *conn).await?.into_iter().collect();
    let elig_keys: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT content_type_key FROM content_type_taxonomy_eligibility
         WHERE owner_kind='theme' AND owner_id != ?
           AND provider_status != 'provider_inactive'",
    ).bind(theme_id).fetch_all(&mut *conn).await?;
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
                fields: ct.fields.clone(),
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
    let desired = if active { "active" } else { "provider_inactive" };
    let declared_only: i64 = i64::from(active);
    let mut changed_keys: BTreeSet<String> = sqlx::query_scalar(
        "SELECT key FROM content_types WHERE owner_kind='plugin' AND owner_id=?
         AND provider_status != ? AND (? = 0 OR declaration_state='declared')",
    )
    .bind(plugin_id).bind(desired).bind(declared_only)
    .fetch_all(&mut *conn).await?.into_iter().collect();
    let changed_eligibility: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT content_type_key FROM content_type_taxonomy_eligibility
         WHERE owner_kind='plugin' AND owner_id=? AND provider_status != ?
           AND (? = 0 OR declaration_state='declared')",
    )
    .bind(plugin_id).bind(desired).bind(declared_only)
    .fetch_all(&mut *conn).await?;
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
    crate::storage::permalinks::set_provider_rewrite_status_in_tx(
        conn, "plugin", plugin_id, active,
    )
    .await?;
    bump_provider_cpt_cas_in_tx(conn, changed_keys).await?;
    crate::storage::permalinks::sync_claims_in_tx(conn).await?;
    Ok(())
}

async fn sync_owner_cnt_declarations_in_tx(
    conn: &mut SqliteConnection,
    owner_kind_str: &str,
    owner_id_str: &str,
    declarations: &opendash_core::content::CntDeclarations,
    incompat_as_conflict: bool,
) -> ApiResult<()> {
    declarations
        .validate()
        .map_err(map_input_validation_error)?;

    crate::storage::taxonomy::sync_provider_definitions_in_tx(
        conn,
        owner_kind_str,
        owner_id_str,
        &declarations.taxonomy_definitions,
    )
    .await?;

    let has_fg_tables = table_exists_in_tx(conn, "content_field_groups").await?
        && table_exists_in_tx(conn, "content_field_group_targets").await?;

    // Snapshot potentially affected types before mutating so verify_effective_schema_transition
    // can compare old_effective vs new_effective symmetrically.
    let mut candidate_types: BTreeSet<String> = BTreeSet::new();
    for ct in &declarations.content_types {
        candidate_types.insert(ct.key.clone());
    }
    for fg in &declarations.field_groups {
        for t in &fg.target_types {
            candidate_types.insert(t.clone());
        }
    }
    let existing_owned_ct_keys: Vec<String> = sqlx::query_scalar(
        "SELECT key FROM content_types WHERE owner_kind = ? AND owner_id = ?",
    )
    .bind(owner_kind_str)
    .bind(owner_id_str)
    .fetch_all(&mut *conn)
    .await?;
    // A new provider CPT has no earlier Admin CAS token. For an existing CPT,
    // provider eligibility changes must advance its CAS version just like a
    // site-owned override, even when the provider's schema JSON is unchanged.
    let existing_owned_ct_set: HashSet<String> =
        existing_owned_ct_keys.iter().cloned().collect();
    for k in &existing_owned_ct_keys {
        candidate_types.insert(k.clone());
    }
    if has_fg_tables {
        let existing_target_types: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT t.type_key
             FROM content_field_group_targets t
             JOIN content_field_groups g ON g.group_id = t.group_id
             WHERE g.owner_kind = ? AND g.owner_id = ?",
        )
        .bind(owner_kind_str)
        .bind(owner_id_str)
        .fetch_all(&mut *conn)
        .await?;
        for k in existing_target_types {
            candidate_types.insert(k);
        }
    }

    let mut old_snapshots: HashMap<String, (EffectiveContentSchema, bool)> = HashMap::new();
    for t in &candidate_types {
        if let Some(old_eff) = load_effective_content_schema_in_tx(conn, t).await? {
            let has_entries: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM content_entries WHERE content_type_key = ?)",
            )
            .bind(t)
            .fetch_one(&mut *conn)
            .await?;
            old_snapshots.insert(t.clone(), (old_eff, has_entries));
        }
    }

    let mut declared_ct_keys: HashSet<String> = HashSet::new();
    let mut declared_eligibility: HashSet<(String, String)> = HashSet::new();
    let mut changed_cpt_types: BTreeSet<String> = BTreeSet::new();

    for ct in &declarations.content_types {
        if builtin_definition(&ct.key).is_some() {
            return Err(ApiError::conflict("post/page là khóa Core được bảo vệ."));
        }
        let existing: Option<(String, String, Option<String>, String, String)> = sqlx::query_as(
            "SELECT definition_json, owner_kind, owner_id, provider_status, declaration_state
             FROM content_types WHERE key = ?",
        )
        .bind(&ct.key)
        .fetch_optional(&mut *conn)
        .await?;

        match existing {
            Some((raw_def, okind, oid, provider_status, declaration_state)) => {
                if okind != owner_kind_str || oid.as_deref() != Some(owner_id_str) {
                    return Err(ApiError::conflict(
                        "Khóa loại nội dung thuộc chủ sở hữu khác (409).",
                    ));
                }
                let existing_def: ContentTypeDefinition = serde_json::from_str(&raw_def)
                    .map_err(|_| ApiError::internal("Dữ liệu loại nội dung không hợp lệ."))?;
                // Preserve omitted fields as withdrawn when package re-declares CPT (270-R1)
                let merged_fields =
                    merge_package_redeclared_fields(&existing_def.fields, &ct.fields);
                let mut incoming_settings = ct.settings.clone().unwrap_or_else(|| existing_def.settings.clone());
                incoming_settings.validate_and_normalize().map_err(map_input_validation_error)?;
                if existing_def.settings.publication == ContentPublication::DraftPublish
                    && incoming_settings.publication == ContentPublication::Immediate
                {
                    let drafts: bool = sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM content_entries WHERE content_type_key = ? AND publication_status = 'draft')",
                    )
                    .bind(&ct.key)
                    .fetch_one(&mut *conn)
                    .await?;
                    if drafts {
                        return Err(ApiError::conflict(
                            "Không thể chuyển sang lưu ngay khi loại nội dung còn bản nháp.",
                        ));
                    }
                }
                let incoming = ContentTypeDefinition {
                    key: ct.key.clone(),
                    label: ct.label.clone(),
                    fields: merged_fields,
                    settings: incoming_settings,
                };
                incoming.validate().map_err(map_input_validation_error)?;
                let incoming_json = serde_json::to_string(&incoming)?;
                if existing_def != incoming || provider_status != "active"
                    || declaration_state != "declared"
                {
                    changed_cpt_types.insert(ct.key.clone());
                }

                sqlx::query(
                    "UPDATE content_types
                     SET definition_json = ?, provider_status = 'active', declaration_state = 'declared', updated_at = datetime('now')
                     WHERE key = ?",
                )
                .bind(&incoming_json)
                .bind(&ct.key)
                .execute(&mut *conn)
                .await?;
            }
            None => {
                let mut settings = ct.settings.clone().unwrap_or_default();
                settings.validate_and_normalize().map_err(map_input_validation_error)?;
                let incoming = ContentTypeDefinition {
                    key: ct.key.clone(),
                    label: ct.label.clone(),
                    fields: ct.fields.clone(),
                    settings,
                };
                incoming.validate().map_err(map_input_validation_error)?;
                let incoming_json = serde_json::to_string(&incoming)?;

                sqlx::query(
                    "INSERT INTO content_types (key, definition_json, owner_kind, owner_id, provider_status, declaration_state)
                     VALUES (?, ?, ?, ?, 'active', 'declared')",
                )
                .bind(&ct.key)
                .bind(&incoming_json)
                .bind(owner_kind_str)
                .bind(owner_id_str)
                .execute(&mut *conn)
                .await?;
            }
        }

        declared_ct_keys.insert(ct.key.clone());
        for tax in &ct.taxonomies {
            declared_eligibility.insert((ct.key.clone(), tax.clone()));
        }
    }

    // Withdraw omitted CPT declarations previously owned by this owner (never deleting schema/entries)
    for key in existing_owned_ct_keys {
        if !declared_ct_keys.contains(&key) {
            let changed = sqlx::query(
                "UPDATE content_types
                 SET declaration_state = 'withdrawn', provider_status = 'active', updated_at = datetime('now')
                 WHERE key = ? AND (declaration_state != 'withdrawn' OR provider_status != 'active')",
            )
            .bind(&key)
            .execute(&mut *conn)
            .await?
            .rows_affected();
            if changed > 0 { changed_cpt_types.insert(key); }
        }
    }

    // Merge explicit taxonomy_eligibility declarations and persist
    for elig in &declarations.taxonomy_eligibility {
        for tax in &elig.taxonomies {
            declared_eligibility.insert((elig.content_type.clone(), tax.clone()));
        }
    }

    for (ct_key, tax_kind) in &declared_eligibility {
        let taxonomy_owner: Option<(String, Option<String>, String, String)> = sqlx::query_as(
            "SELECT owner_kind,owner_id,provider_status,declaration_state
             FROM taxonomy_definitions WHERE key=?",
        )
        .bind(tax_kind)
        .fetch_optional(&mut *conn)
        .await?;
        match taxonomy_owner {
            Some((kind, id, provider_status, state))
                if (kind == "core" || kind == "user"
                    || (kind == owner_kind_str && id.as_deref() == Some(owner_id_str)))
                    && provider_status == "active"
                    && state == "declared" => {}
            Some(_) => {
                return Err(ApiError::conflict(
                    "Không thể liên kết taxonomy thuộc provider khác hoặc không hoạt động.",
                ));
            }
            None => return Err(ApiError::bad_request("Taxonomy eligibility chưa được định nghĩa.")),
        }
        if builtin_definition(ct_key).is_some() {
            return Err(ApiError::conflict(
                "Không thể chiếm quyền taxonomy eligibility của Core post/page.",
            ));
        }
        let ct_owner: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT owner_kind, owner_id FROM content_types WHERE key = ?")
                .bind(ct_key)
                .fetch_optional(&mut *conn)
                .await?;
        match ct_owner {
            Some((okind, oid))
                if okind == owner_kind_str && oid.as_deref() == Some(owner_id_str) => {}
            Some(_) => {
                return Err(ApiError::conflict(
                    "Không thể khai báo taxonomy eligibility cho loại nội dung thuộc chủ sở hữu khác (409).",
                ));
            }
            None => {
                return Err(ApiError::bad_request(
                    "Loại nội dung trong taxonomy_eligibility chưa được khai báo.",
                ));
            }
        }

        let existing_elig: Option<(String, Option<String>, String, String)> = sqlx::query_as(
            "SELECT owner_kind, owner_id, provider_status, declaration_state
             FROM content_type_taxonomy_eligibility
             WHERE content_type_key = ? AND taxonomy_kind = ?",
        )
        .bind(ct_key)
        .bind(tax_kind)
        .fetch_optional(&mut *conn)
        .await?;

        match existing_elig {
            Some((okind, oid, provider_status, declaration_state)) => {
                if okind != owner_kind_str || oid.as_deref() != Some(owner_id_str) {
                    return Err(ApiError::conflict(
                        "Taxonomy eligibility thuộc chủ sở hữu khác (409).",
                    ));
                }
                if provider_status != "active" || declaration_state != "declared" {
                    sqlx::query(
                        "UPDATE content_type_taxonomy_eligibility
                         SET provider_status = 'active', declaration_state = 'declared', updated_at = datetime('now')
                         WHERE content_type_key = ? AND taxonomy_kind = ?",
                    )
                    .bind(ct_key)
                    .bind(tax_kind)
                    .execute(&mut *conn)
                    .await?;
                    changed_cpt_types.insert(ct_key.clone());
                }
            }
            None => {
                sqlx::query(
                    "INSERT INTO content_type_taxonomy_eligibility
                     (content_type_key, taxonomy_kind, owner_kind, owner_id, provider_status, declaration_state)
                     VALUES (?, ?, ?, ?, 'active', 'declared')",
                )
                .bind(ct_key)
                .bind(tax_kind)
                .bind(owner_kind_str)
                .bind(owner_id_str)
                .execute(&mut *conn)
                .await?;
                changed_cpt_types.insert(ct_key.clone());
            }
        }
    }

    // Withdraw omitted taxonomy_eligibility rows owned by this owner (retaining entry_terms)
    let owned_eligs: Vec<(String, String)> = sqlx::query_as(
        "SELECT content_type_key, taxonomy_kind
         FROM content_type_taxonomy_eligibility
         WHERE owner_kind = ? AND owner_id = ?",
    )
    .bind(owner_kind_str)
    .bind(owner_id_str)
    .fetch_all(&mut *conn)
    .await?;

    for (ct_key, tax_kind) in owned_eligs {
        if !declared_eligibility.contains(&(ct_key.clone(), tax_kind.clone())) {
            let changed = sqlx::query(
                "UPDATE content_type_taxonomy_eligibility
                 SET declaration_state = 'withdrawn', provider_status = 'active', updated_at = datetime('now')
                 WHERE content_type_key = ? AND taxonomy_kind = ?
                   AND (declaration_state != 'withdrawn' OR provider_status != 'active')",
            )
            .bind(&ct_key)
            .bind(&tax_kind)
            .execute(&mut *conn)
            .await?
            .rows_affected();
            if changed > 0 {
                changed_cpt_types.insert(ct_key);
            }
        }
    }

    // This function runs inside the install/activation writer transaction.
    // Changing provider defaults or CPT schema/status without advancing the
    // CAS allows an Admin to save a stale taxonomy override with a still-valid
    // If-Match. Bump once per existing CPT affected by this provider install,
    // covering schema, additions, withdrawals and reactivations atomically.
    for ct_key in changed_cpt_types {
        if existing_owned_ct_set.contains(&ct_key) {
            sqlx::query(
                "UPDATE content_types SET version = version + 1,
                 updated_at = datetime('now') WHERE key = ?",
            )
            .bind(&ct_key)
            .execute(&mut *conn)
            .await?;
        }
    }

    // Sync package field_groups (3-level tombstone preservation — 270-R1)
    if has_fg_tables {
        let mut declared_group_ids: HashSet<String> = HashSet::new();
        for fg in &declarations.field_groups {
            let group_id = format!("{owner_kind_str}:{owner_id_str}:{}", fg.local_key);
            let incoming_fields = fg
                .validate_and_normalize()
                .map_err(map_input_validation_error)?;

            for target in &fg.target_types {
                let exists: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM content_types WHERE key = ?)",
                )
                .bind(target)
                .fetch_one(&mut *conn)
                .await?;
                if !exists {
                    return Err(ApiError::unprocessable(format!(
                        "Loại nội dung mục tiêu '{target}' trong field_group '{}' chưa tồn tại.",
                        fg.local_key
                    )));
                }
            }

            if let Some(existing_group) = load_field_group_in_tx(conn, &group_id).await? {
                let merged_fields =
                    merge_package_redeclared_fields(&existing_group.fields, &incoming_fields);
                let fields_json = serde_json::to_string(&merged_fields)?;

                sqlx::query(
                    "UPDATE content_field_groups
                     SET title = ?, description = ?, fields_json = ?, default_region = ?,
                         default_order = ?, collapsible = ?, status = 'active',
                         version = version + 1,
                         updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                     WHERE group_id = ?",
                )
                .bind(&fg.title)
                .bind(&fg.description)
                .bind(&fields_json)
                .bind(fg.default_region.as_str())
                .bind(fg.default_order)
                .bind(if fg.collapsible { 1_i64 } else { 0_i64 })
                .bind(&group_id)
                .execute(&mut *conn)
                .await?;

                let incoming_target_set: HashSet<&str> =
                    fg.target_types.iter().map(|s| s.as_str()).collect();
                for target in &fg.target_types {
                    sqlx::query(
                        "INSERT INTO content_field_group_targets (group_id, type_key, association_state)
                         VALUES (?, ?, 'declared')
                         ON CONFLICT(group_id, type_key) DO UPDATE SET
                             association_state = 'declared',
                             updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                    )
                    .bind(&group_id)
                    .bind(target)
                    .execute(&mut *conn)
                    .await?;
                }
                for old_t in &existing_group.targets {
                    if !incoming_target_set.contains(old_t.type_key.as_str()) {
                        sqlx::query(
                            "UPDATE content_field_group_targets
                             SET association_state = 'withdrawn',
                                 updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                             WHERE group_id = ? AND type_key = ?",
                        )
                        .bind(&group_id)
                        .bind(&old_t.type_key)
                        .execute(&mut *conn)
                        .await?;
                    }
                }
            } else {
                let fields_json = serde_json::to_string(&incoming_fields)?;
                sqlx::query(
                    "INSERT INTO content_field_groups
                     (group_id, owner_kind, owner_id, local_key, title, description, fields_json,
                      default_region, default_order, collapsible, status, version)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', 1)",
                )
                .bind(&group_id)
                .bind(owner_kind_str)
                .bind(owner_id_str)
                .bind(&fg.local_key)
                .bind(&fg.title)
                .bind(&fg.description)
                .bind(&fields_json)
                .bind(fg.default_region.as_str())
                .bind(fg.default_order)
                .bind(if fg.collapsible { 1_i64 } else { 0_i64 })
                .execute(&mut *conn)
                .await?;

                for target in &fg.target_types {
                    sqlx::query(
                        "INSERT INTO content_field_group_targets (group_id, type_key, association_state)
                         VALUES (?, ?, 'declared')",
                    )
                    .bind(&group_id)
                    .bind(target)
                    .execute(&mut *conn)
                    .await?;
                }
            }
            declared_group_ids.insert(group_id);
        }

        // Withdraw omitted field_groups owned by this package (retaining targets & fields)
        let owned_group_ids: Vec<String> = sqlx::query_scalar(
            "SELECT group_id FROM content_field_groups WHERE owner_kind = ? AND owner_id = ?",
        )
        .bind(owner_kind_str)
        .bind(owner_id_str)
        .fetch_all(&mut *conn)
        .await?;

        for gid in owned_group_ids {
            if !declared_group_ids.contains(&gid) {
                sqlx::query(
                    "UPDATE content_field_groups
                     SET status = 'withdrawn', version = version + 1,
                         updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                     WHERE group_id = ?",
                )
                .bind(&gid)
                .execute(&mut *conn)
                .await?;
                sqlx::query(
                    "UPDATE content_field_group_targets
                     SET association_state = 'withdrawn',
                         updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                     WHERE group_id = ?",
                )
                .bind(&gid)
                .execute(&mut *conn)
                .await?;
            }
        }
    }

    // Enforce Rule COMPAT-1..COMPAT-4 across every affected type_key
    for t in &candidate_types {
        if let Some(new_eff) = load_effective_content_schema_in_tx(conn, t).await? {
            let (old_eff_opt, has_entries) = match old_snapshots.get(t) {
                Some((old_eff, has)) => (Some(old_eff), *has),
                None => (None, false),
            };
            verify_effective_schema_transition(old_eff_opt, &new_eff, has_entries)
                .map_err(|e| map_transition_error(e, incompat_as_conflict))?;
        }
    }

    let rewrites: Vec<crate::storage::permalinks::ProviderRewriteDeclaration> = declarations
        .content_types
        .iter()
        .map(|ct| crate::storage::permalinks::ProviderRewriteDeclaration {
            resource_kind: "content_type",
            resource_key: ct.key.clone(),
            rewrite_base: ct.rewrite_base.clone(),
        })
        .chain(declarations.taxonomy_definitions.iter().map(|tax| {
            crate::storage::permalinks::ProviderRewriteDeclaration {
                resource_kind: "taxonomy",
                resource_key: tax.key.clone(),
                rewrite_base: tax.rewrite_base.clone(),
            }
        }))
        .collect();
    crate::storage::permalinks::sync_provider_rewrite_defaults_in_tx(
        conn, owner_kind_str, owner_id_str, &rewrites,
    )
    .await?;

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
        ).bind(type_key).fetch_optional(&mut *conn).await?;
        if let Some(raw) = override_keys {
            let keys: Vec<String> = serde_json::from_str(&raw)
                .map_err(|_| ApiError::internal("Danh sách taxonomy override bị hỏng."))?;
            let mut enabled = Vec::new();
            for key in keys {
                let exists: Option<i64> = sqlx::query_scalar(
                    "SELECT 1 FROM taxonomy_definitions WHERE key=?
                     AND provider_status='active' AND declaration_state='declared'",
                ).bind(&key).fetch_optional(&mut *conn).await?;
                if exists.is_some() { enabled.push(key); }
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
        ).bind(type_key).fetch_all(&mut *conn).await?
    } else {
        sqlx::query_scalar(
            "SELECT taxonomy_kind FROM content_type_taxonomy_eligibility
             WHERE content_type_key=? AND provider_status='active'
               AND declaration_state='declared' ORDER BY taxonomy_kind",
        ).bind(type_key).fetch_all(&mut *conn).await?
    };
    Ok(rows)
}

/// C2 canonical effective site/provider eligibility used by schema tokens,
/// entry writes, Admin table, public lists, and future Builder selectors.
pub async fn load_effective_taxonomies(
    db: &SqlitePool, type_key: &str,
) -> ApiResult<Vec<String>> {
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
    if !valid_content_type_key(type_key) || type_key == "section" {
        return Err(ApiError::bad_request("Loại nội dung không hỗ trợ taxonomy."));
    }
    // Separate autocommit SELECTs can straddle a concurrent eligibility PUT:
    // the old association list and new CPT version would allow a valid CAS
    // write that silently removes another Admin's newly added association.
    // A single read transaction pins BOTH to one SQLite read snapshot.
    let mut tx = db.begin().await?;
    let type_version: Option<i64> = sqlx::query_scalar(
        "SELECT version FROM content_types WHERE key=?",
    )
    .bind(type_key)
    .fetch_optional(&mut *tx)
    .await?;
    let type_version = type_version
        .ok_or_else(|| ApiError::not_found("Loại nội dung không tồn tại."))?;
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

pub async fn put_site_taxonomy_eligibility(
    db: &SqlitePool, actor: &crate::auth::SessionContext, type_key: &str,
    expected_version: i64, taxonomies: &[String],
) -> ApiResult<ContentTypeApiItem> {
    if !opendash_core::content::valid_content_type_key(type_key) || type_key == "section" {
        return Err(ApiError::bad_request("Không thể cấu hình taxonomy cho Section."));
    }
    if taxonomies.len() > 64 {
        return Err(ApiError::bad_request("Tối đa 64 taxonomy cho mỗi CPT."));
    }
    let mut seen = std::collections::HashSet::new();
    for key in taxonomies {
        if !opendash_core::content::valid_content_type_key(key) || !seen.insert(key) {
            return Err(ApiError::bad_request("Danh sách taxonomy chứa khóa không hợp lệ hoặc trùng."));
        }
    }
    let mut tx = begin_content_write_tx(db).await?;
    // Auth at route entry is not a sufficient write fence: a concurrent admin
    // can revoke/demote the actor while this request waits for the writer.
    // Check session expiry, identity and current admin role *after* claiming
    // the SQLite writer and before changing any eligibility or CPT version.
    let current_actor: Option<(i64, String)> = sqlx::query_as(
        "SELECT u.id,u.role FROM sessions s JOIN users u ON u.id=s.user_id
         WHERE s.token_hash=? AND s.expires_at > ?",
    )
    .bind(&actor.token_hash)
    .bind(crate::auth::current_time_secs())
    .fetch_optional(&mut *tx).await?;
    match current_actor {
        Some((id, role)) if id == actor.user.id && role == "admin" => {},
        Some(_) => return Err(ApiError::forbidden("Tài khoản không còn quyền quản trị.")),
        None => return Err(ApiError::unauthorized("Phiên đăng nhập đã bị thu hồi.")),
    }
    let version: Option<i64> = sqlx::query_scalar(
        "SELECT version FROM content_types WHERE key=?",
    ).bind(type_key).fetch_optional(&mut *tx).await?;
    let Some(version) = version else {
        return Err(ApiError::not_found("Loại nội dung không tồn tại."));
    };
    if version != expected_version {
        return Err(ApiError::precondition_failed("Schema CPT đã thay đổi; hãy tải lại."));
    }
    for key in taxonomies {
        let exists: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM taxonomy_definitions WHERE key=?",
        ).bind(key).fetch_optional(&mut *tx).await?;
        if exists.is_none() {
            return Err(ApiError::bad_request("Taxonomy được chọn không tồn tại."));
        }
    }
    let canonical = {
        let mut sorted = taxonomies.to_vec();
        sorted.sort();
        serde_json::to_string(&sorted)?
    };
    sqlx::query(
        "INSERT INTO content_type_taxonomy_overrides(content_type_key,taxonomies_json)
         VALUES (?,?) ON CONFLICT(content_type_key) DO UPDATE SET
           taxonomies_json=excluded.taxonomies_json,updated_at=datetime('now')",
    ).bind(type_key).bind(&canonical).execute(&mut *tx).await?;
    let count = sqlx::query("UPDATE content_types SET version=version+1,
        updated_at=datetime('now') WHERE key=? AND version=?")
        .bind(type_key).bind(expected_version).execute(&mut *tx).await?.rows_affected();
    if count != 1 {
        return Err(ApiError::precondition_failed("Schema CPT đã thay đổi; hãy tải lại."));
    }
    let effective = load_effective_content_schema_in_tx(&mut tx, type_key).await?
        .ok_or_else(|| ApiError::internal("Không thể tải schema CPT."))?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    tx.commit().await?;
    Ok(ContentTypeApiItem {
        base: effective.base,
        version: effective.base_version,
        schema_token: effective.schema_token,
    })
}

async fn load_plugin_action_panels_for_type_in_tx(
    conn: &mut SqliteConnection,
    type_key: &str,
) -> ApiResult<Vec<PanelDefinition>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, manifest_json FROM plugins WHERE enabled = 1 ORDER BY id ASC",
    )
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
            let min_edit_role = ep.min_edit_role.unwrap_or(action_min_role).max(action_min_role);
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
                required_core: false,
                effective_status: EffectiveFieldStatus::Active,
            });
        }
    }
    Ok(out)
}

async fn load_raw_editor_layout_in_tx(
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
    let plugin_action_panels = load_plugin_action_panels_for_type_in_tx(&mut conn, type_key).await?;
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

pub async fn put_editor_layout(
    db: &SqlitePool,
    type_key: &str,
    expected_version: i64,
    updated_by: i64,
    input: EditorLayoutConfigV1,
) -> ApiResult<EditorLayoutSnapshot> {
    if !valid_content_type_key(type_key) {
        return Err(ApiError::bad_request("Khóa loại nội dung không hợp lệ."));
    }
    if expected_version < 0 {
        return Err(ApiError::bad_request("If-Match version không hợp lệ."));
    }
    let mut tx = begin_content_write_tx(db).await?;
    let Some(effective) = load_effective_content_schema_in_tx(&mut tx, type_key).await? else {
        return Err(ApiError::not_found(format!(
            "Không tìm thấy loại nội dung '{type_key}'."
        )));
    };
    let (_, current_version) = load_raw_editor_layout_in_tx(&mut tx, type_key).await?;
    if current_version != expected_version {
        return Err(ApiError::conflict(format!(
            "Xung đột phiên bản bố cục '{type_key}': hiện tại là v{current_version}, If-Match là v{expected_version}."
        )));
    }
    let next_version = current_version + 1;
    let taxonomies = load_effective_taxonomies_in_tx(&mut tx, type_key).await?;
    let plugin_action_panels = load_plugin_action_panels_for_type_in_tx(&mut tx, type_key).await?;
    let base_panels =
        build_core_and_group_panels(&effective, !taxonomies.is_empty(), &plugin_action_panels);
    let (layout, panels) = resolve_and_validate_editor_layout(
        &effective,
        base_panels,
        Some(input),
        next_version as u64,
    )
    .map_err(|e| ApiError::unprocessable(e.message))?;

    let layout_json = serde_json::to_string(&layout)
        .map_err(|e| ApiError::internal(format!("Không thể serialize layout_json: {e}")))?;

    sqlx::query(
        "INSERT INTO content_editor_layouts (type_key, layout_json, version, updated_by, updated_at)
         VALUES (?, ?, ?, ?, (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))
         ON CONFLICT(type_key) DO UPDATE SET
            layout_json = excluded.layout_json,
            version = excluded.version,
            updated_by = excluded.updated_by,
            updated_at = excluded.updated_at",
    )
    .bind(type_key)
    .bind(&layout_json)
    .bind(next_version)
    .bind(updated_by)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(EditorLayoutSnapshot {
        layout,
        panels,
        version: next_version,
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
    let plugin_action_panels = load_plugin_action_panels_for_type_in_tx(&mut conn, type_key).await?;
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

#[cfg(test)]
mod eligibility_provider_cas_tests {
    use super::*;
    use opendash_core::content::{CntDeclarations, ContentTypeDeclaration, TaxonomyEligibilityDeclaration};
    use sqlx::sqlite::SqlitePoolOptions;

    fn provider_declarations(taxonomies: &[&str]) -> CntDeclarations {
        CntDeclarations {
            content_types: vec![ContentTypeDeclaration {
                key: "provider_books_cas".to_owned(),
                label: "Provider books".to_owned(),
                fields: vec![],
                taxonomies: taxonomies.iter().map(|s| (*s).to_owned()).collect(),
                settings: None,
                rewrite_base: None,
            }],
            taxonomy_definitions: vec![],
            taxonomy_eligibility: vec![],
            field_groups: vec![],
        }
    }

    async fn declare(db: &SqlitePool, taxonomies: &[&str]) {
        let mut tx = db.begin().await.unwrap();
        sync_plugin_cnt_declarations_in_tx(
            &mut tx,
            "provider_cas_test",
            &provider_declarations(taxonomies),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }

    #[tokio::test]
    async fn provider_redeclaration_advances_taxonomy_cas_once_only_when_changed() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();

        declare(&db, &["category"]).await;
        let initial = load_configured_taxonomies(&db, "provider_books_cas")
            .await
            .unwrap();
        assert_eq!(initial.taxonomies, vec!["category"]);
        let old_version = initial.type_version;

        // Plugin redeclare commits new default while Admin holds old CAS.
        declare(&db, &["category", "tag"]).await;
        let next = load_configured_taxonomies(&db, "provider_books_cas")
            .await
            .unwrap();
        assert_eq!(next.taxonomies, vec!["category", "tag"]);
        assert_eq!(next.type_version, old_version + 1);
        let stale = sqlx::query(
            "UPDATE content_types SET version=version+1 WHERE key=? AND version=?",
        )
        .bind("provider_books_cas")
        .bind(old_version)
        .execute(&db)
        .await
        .unwrap();
        assert_eq!(stale.rows_affected(), 0);

        declare(&db, &["category", "tag"]).await;
        let same = load_configured_taxonomies(&db, "provider_books_cas")
            .await
            .unwrap();
        assert_eq!(same.type_version, next.type_version);

        // Re-declaring an existing provider CPT with different schema but the
        // same taxonomy defaults also invalidates a stale eligibility CAS.
        let mut renamed_declarations = provider_declarations(&["category", "tag"]);
        renamed_declarations.content_types[0].label = "Provider books renamed".to_owned();
        let mut tx = db.begin().await.unwrap();
        sync_plugin_cnt_declarations_in_tx(&mut tx, "provider_cas_test", &renamed_declarations)
            .await.unwrap();
        tx.commit().await.unwrap();
        let renamed = load_configured_taxonomies(&db, "provider_books_cas")
            .await.unwrap();
        assert_eq!(renamed.type_version, same.type_version + 1);
        // Identical re-declaration must not create perpetual CAS conflicts.
        let mut tx = db.begin().await.unwrap();
        sync_plugin_cnt_declarations_in_tx(&mut tx, "provider_cas_test", &renamed_declarations)
            .await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(load_configured_taxonomies(&db, "provider_books_cas")
            .await.unwrap().type_version, renamed.type_version);

        // Provider enable/disable changes effective eligibility and also
        // fences stale Admin settings/associations from an earlier lifetime.
        let mut tx = db.begin().await.unwrap();
        set_plugin_cnt_provider_status_in_tx(&mut tx, "provider_cas_test", false)
            .await.unwrap();
        tx.commit().await.unwrap();
        let inactive = load_configured_taxonomies(&db, "provider_books_cas")
            .await.unwrap();
        assert_eq!(inactive.type_version, renamed.type_version + 1);
        assert!(inactive.effective.is_empty());
        let mut tx = db.begin().await.unwrap();
        set_plugin_cnt_provider_status_in_tx(&mut tx, "provider_cas_test", true)
            .await.unwrap();
        tx.commit().await.unwrap();
        let reactivated = load_configured_taxonomies(&db, "provider_books_cas")
            .await.unwrap();
        assert_eq!(reactivated.type_version, inactive.type_version + 1);
        assert_eq!(reactivated.effective, vec!["category", "tag"]);

        declare(&db, &["category"]).await;
        let withdrawn = load_configured_taxonomies(&db, "provider_books_cas")
            .await
            .unwrap();
        assert_eq!(withdrawn.type_version, reactivated.type_version + 1);
        assert_eq!(withdrawn.taxonomies, vec!["category", "tag"]);
        assert_eq!(withdrawn.effective, vec!["category"]);

        // Provider may keep explicit eligibility for its owned CPT after
        // omitting the CPT declaration. Enable then changes the eligibility
        // status even though the owning CPT stays 'withdrawn'; that still
        // needs a version bump to reject a stale Admin site override.
        let eligibility_only = CntDeclarations {
            content_types: vec![],
            taxonomy_definitions: vec![],
            taxonomy_eligibility: vec![TaxonomyEligibilityDeclaration {
                content_type: "provider_books_cas".into(),
                taxonomies: vec!["category".into(), "tag".into()],
            }],
            field_groups: vec![],
        };
        let mut tx = db.begin().await.unwrap();
        sync_plugin_cnt_declarations_in_tx(&mut tx, "provider_cas_test", &eligibility_only)
            .await.unwrap();
        tx.commit().await.unwrap();
        let eligible_withdrawn_cpt = load_configured_taxonomies(&db, "provider_books_cas")
            .await.unwrap();
        assert_eq!(eligible_withdrawn_cpt.type_version, withdrawn.type_version + 1);

        let mut tx = db.begin().await.unwrap();
        set_plugin_cnt_provider_status_in_tx(&mut tx, "provider_cas_test", false)
            .await.unwrap();
        tx.commit().await.unwrap();
        let off = load_configured_taxonomies(&db, "provider_books_cas")
            .await.unwrap();
        assert_eq!(off.type_version, eligible_withdrawn_cpt.type_version + 1);

        let mut tx = db.begin().await.unwrap();
        set_plugin_cnt_provider_status_in_tx(&mut tx, "provider_cas_test", true)
            .await.unwrap();
        tx.commit().await.unwrap();
        let on = load_configured_taxonomies(&db, "provider_books_cas")
            .await.unwrap();
        assert_eq!(on.type_version, off.type_version + 1);
        assert_eq!(on.effective, vec!["category", "tag"]);
        let stale = sqlx::query(
            "UPDATE content_types SET version=version+1 WHERE key=? AND version=?",
        ).bind("provider_books_cas").bind(off.type_version).execute(&db).await.unwrap();
        assert_eq!(stale.rows_affected(), 0);
    }
}
