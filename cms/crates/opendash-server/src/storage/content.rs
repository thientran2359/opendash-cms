//! Persistent CPT definitions. All content SQL stays in the storage layer.

use std::collections::{HashMap, HashSet};

use axum::http::StatusCode;
use opendash_core::{
    content::{ContentFieldDefinition, ContentFieldType, ContentTypeDefinition},
    registry::{DeclarationState, OwnedDescriptor, ProviderStatus, RegistryCatalog, ResourceOwner},
};
use sqlx::{SqliteConnection, SqlitePool};

use crate::{
    error::{ApiError, ApiResult},
    theme::ThemeDeclarations,
};

pub fn builtin_definition(key: &str) -> Option<ContentTypeDefinition> {
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
            },
            ContentFieldDefinition {
                key: "featured_image".into(),
                label: "Ảnh đại diện".into(),
                field_type: ContentFieldType::Image,
                required: false,
                options: Vec::new(),
            },
        ],
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
         FROM content_types WHERE key IN ('post', 'page')",
    )
    .fetch_all(db)
    .await?;
    for (key, raw, owner, owner_id, status) in rows {
        let matching_definition = serde_json::from_str::<ContentTypeDefinition>(&raw)
            .ok()
            .as_ref()
            == builtin_definition(&key).as_ref();
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

/// Stores a user-created CPT definition.
pub async fn insert_user_content_type(
    db: &SqlitePool,
    definition: &ContentTypeDefinition,
) -> ApiResult<OwnedDescriptor<ContentTypeDefinition>> {
    if builtin_definition(&definition.key).is_some() {
        return Err(ApiError::conflict("post/page là khóa Core được bảo vệ."));
    }
    let definition_json = serde_json::to_string(definition)?;
    let result = sqlx::query(
        "INSERT INTO content_types (key, definition_json, owner_kind, owner_id, provider_status, declaration_state)
         VALUES (?, ?, 'user', NULL, 'active', 'declared')",
    )
    .bind(&definition.key)
    .bind(&definition_json)
    .execute(db)
    .await;

    match result {
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            Err(ApiError::conflict("Khóa loại nội dung đã tồn tại."))
        }
        other => {
            other?;
            Ok(OwnedDescriptor {
                owner: ResourceOwner::User,
                status: ProviderStatus::Active,
                declaration_state: DeclarationState::Declared,
                descriptor: definition.clone(),
            })
        }
    }
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

                let owner = match (owner_kind.as_str(), owner_id) {
                    ("core", None) => ResourceOwner::Core,
                    ("user", None) => ResourceOwner::User,
                    ("theme", Some(id)) if !id.is_empty() => ResourceOwner::Theme(id),
                    ("plugin", Some(id)) if !id.is_empty() => ResourceOwner::Plugin(id),
                    _ => return Err(ApiError::internal("Chủ sở hữu loại nội dung không hợp lệ.")),
                };
                let status = match status.as_str() {
                    "active" => ProviderStatus::Active,
                    "provider_inactive" => ProviderStatus::ProviderInactive,
                    _ => {
                        return Err(ApiError::internal(
                            "Trạng thái loại nội dung không hợp lệ.",
                        ))
                    }
                };
                let declaration_state = match decl_state.as_str() {
                    "declared" => DeclarationState::Declared,
                    "withdrawn" => DeclarationState::Withdrawn,
                    _ => {
                        return Err(ApiError::internal(
                            "Trạng thái declaration loại nội dung không hợp lệ.",
                        ))
                    }
                };
                Ok(OwnedDescriptor {
                    owner,
                    status,
                    declaration_state,
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

/// Checks schema evolution compatibility for same-owner re-declaration.
/// Allows label updates, adding optional fields, and adding select options.
/// Rejects changing field types, changing/removing field keys with existing entries,
/// turning optional fields required / adding required fields, or removing select options
/// when existing entries are present.
pub fn check_content_type_compatibility(
    existing: &ContentTypeDefinition,
    incoming: &ContentTypeDefinition,
    has_existing_entries: bool,
) -> Result<(), &'static str> {
    let incoming_by_key: HashMap<&str, &ContentFieldDefinition> = incoming
        .fields
        .iter()
        .map(|f| (f.key.as_str(), f))
        .collect();
    let existing_by_key: HashMap<&str, &ContentFieldDefinition> = existing
        .fields
        .iter()
        .map(|f| (f.key.as_str(), f))
        .collect();

    for old in &existing.fields {
        let Some(new) = incoming_by_key.get(old.key.as_str()) else {
            if has_existing_entries {
                return Err("Không thể xóa hoặc đổi key field khi đã có dữ liệu cũ.");
            }
            continue;
        };
        if old.field_type != new.field_type {
            return Err("Không thể đổi kiểu dữ liệu (type) của custom field đã tồn tại.");
        }
        if !old.required && new.required {
            return Err("Không thể đổi field tùy chọn thành bắt buộc (required) khi nâng cấp.");
        }
        if old.field_type == ContentFieldType::Select && has_existing_entries {
            let new_values: HashSet<&str> =
                new.options.iter().map(|o| o.value.as_str()).collect();
            for old_opt in &old.options {
                if !new_values.contains(old_opt.value.as_str()) {
                    return Err(
                        "Không thể xóa lựa chọn (select option) khi đã có dữ liệu cũ.",
                    );
                }
            }
        }
    }

    for new in &incoming.fields {
        if !existing_by_key.contains_key(new.key.as_str()) && new.required {
            return Err("Không thể thêm field bắt buộc (required) mới khi nâng cấp schema.");
        }
    }

    Ok(())
}

/// Synchronizes a theme's CNT declarations (`content_types` and `taxonomy_eligibility`)
/// inside an active SQLite write transaction. All errors roll back the caller's transaction.
pub async fn sync_theme_declarations_in_tx(
    conn: &mut SqliteConnection,
    theme_id: &str,
    declarations: &ThemeDeclarations,
) -> ApiResult<()> {
    // 1. Mark other themes' declared/withdrawn resources as provider_inactive (preserving ownership & declaration_state)
    sqlx::query(
        "UPDATE content_types
         SET provider_status = 'provider_inactive', updated_at = datetime('now')
         WHERE owner_kind = 'theme' AND owner_id != ?",
    )
    .bind(theme_id)
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        "UPDATE content_type_taxonomy_eligibility
         SET provider_status = 'provider_inactive', updated_at = datetime('now')
         WHERE owner_kind = 'theme' AND owner_id != ?",
    )
    .bind(theme_id)
    .execute(&mut *conn)
    .await?;

    let neutral = opendash_core::content::CntDeclarations {
        content_types: declarations
            .content_types
            .iter()
            .map(|ct| opendash_core::content::ContentTypeDeclaration {
                key: ct.key.clone(),
                label: ct.label.clone(),
                fields: ct.fields.clone(),
                taxonomies: ct.taxonomies.clone(),
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
    };
    sync_owner_cnt_declarations_in_tx(conn, "theme", theme_id, &neutral, false).await
}

/// Synchronizes a plugin's CNT declarations (`content_types` and `taxonomy_eligibility`)
/// inside an active SQLite write transaction without touching theme or other plugin declarations.
pub async fn sync_plugin_cnt_declarations_in_tx(
    conn: &mut SqliteConnection,
    plugin_id: &str,
    declarations: &opendash_core::content::CntDeclarations,
) -> ApiResult<()> {
    if declarations.is_empty() {
        let has_cnt: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='content_types')",
        )
        .fetch_one(&mut *conn)
        .await?;
        if !has_cnt {
            return Ok(());
        }
    }
    sync_owner_cnt_declarations_in_tx(conn, "plugin", plugin_id, declarations, true).await
}

/// Updates `provider_status` for all CNT declarations owned by a plugin without deleting schema or entries.
pub async fn set_plugin_cnt_provider_status_in_tx(
    conn: &mut SqliteConnection,
    plugin_id: &str,
    active: bool,
) -> ApiResult<()> {
    let has_cnt: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='content_types')",
    )
    .fetch_one(&mut *conn)
    .await?;
    if !has_cnt {
        return Ok(());
    }
    if active {
        sqlx::query(
            "UPDATE content_types
             SET provider_status = 'active', updated_at = datetime('now')
             WHERE owner_kind = 'plugin' AND owner_id = ? AND declaration_state = 'declared'",
        )
        .bind(plugin_id)
        .execute(&mut *conn)
        .await?;

        sqlx::query(
            "UPDATE content_type_taxonomy_eligibility
             SET provider_status = 'active', updated_at = datetime('now')
             WHERE owner_kind = 'plugin' AND owner_id = ? AND declaration_state = 'declared'",
        )
        .bind(plugin_id)
        .execute(&mut *conn)
        .await?;
    } else {
        sqlx::query(
            "UPDATE content_types
             SET provider_status = 'provider_inactive', updated_at = datetime('now')
             WHERE owner_kind = 'plugin' AND owner_id = ?",
        )
        .bind(plugin_id)
        .execute(&mut *conn)
        .await?;

        sqlx::query(
            "UPDATE content_type_taxonomy_eligibility
             SET provider_status = 'provider_inactive', updated_at = datetime('now')
             WHERE owner_kind = 'plugin' AND owner_id = ?",
        )
        .bind(plugin_id)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

async fn sync_owner_cnt_declarations_in_tx(
    conn: &mut SqliteConnection,
    owner_kind_str: &str,
    owner_id_str: &str,
    declarations: &opendash_core::content::CntDeclarations,
    incompat_as_conflict: bool,
) -> ApiResult<()> {
    let mut declared_ct_keys: HashSet<String> = HashSet::new();
    let mut declared_eligibility: HashSet<(String, String)> = HashSet::new();

    for ct in &declarations.content_types {
        if builtin_definition(&ct.key).is_some() {
            return Err(ApiError::conflict("post/page là khóa Core được bảo vệ."));
        }
        let incoming = ContentTypeDefinition {
            key: ct.key.clone(),
            label: ct.label.clone(),
            fields: ct.fields.clone(),
        };
        incoming.validate().map_err(ApiError::bad_request)?;
        let incoming_json = serde_json::to_string(&incoming)?;

        let existing: Option<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT definition_json, owner_kind, owner_id FROM content_types WHERE key = ?",
        )
        .bind(&ct.key)
        .fetch_optional(&mut *conn)
        .await?;

        match existing {
            Some((raw_def, okind, oid)) => {
                if okind != owner_kind_str || oid.as_deref() != Some(owner_id_str) {
                    return Err(ApiError::conflict(
                        "Khóa loại nội dung thuộc chủ sở hữu khác (409).",
                    ));
                }
                let existing_def: ContentTypeDefinition = serde_json::from_str(&raw_def)
                    .map_err(|_| ApiError::internal("Dữ liệu loại nội dung không hợp lệ."))?;
                let has_entries: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM content_entries WHERE content_type_key = ?)",
                )
                .bind(&ct.key)
                .fetch_one(&mut *conn)
                .await?;
                check_content_type_compatibility(&existing_def, &incoming, has_entries).map_err(
                    |msg| {
                        if incompat_as_conflict {
                            ApiError::conflict(msg)
                        } else {
                            ApiError(StatusCode::UNPROCESSABLE_ENTITY, msg.into())
                        }
                    },
                )?;
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
    let owned_keys: Vec<String> = sqlx::query_scalar(
        "SELECT key FROM content_types WHERE owner_kind = ? AND owner_id = ?",
    )
    .bind(owner_kind_str)
    .bind(owner_id_str)
    .fetch_all(&mut *conn)
    .await?;

    for key in owned_keys {
        if !declared_ct_keys.contains(&key) {
            sqlx::query(
                "UPDATE content_types
                 SET declaration_state = 'withdrawn', provider_status = 'active', updated_at = datetime('now')
                 WHERE key = ?",
            )
            .bind(&key)
            .execute(&mut *conn)
            .await?;
        }
    }

    // Merge explicit taxonomy_eligibility declarations and persist
    for elig in &declarations.taxonomy_eligibility {
        for tax in &elig.taxonomies {
            declared_eligibility.insert((elig.content_type.clone(), tax.clone()));
        }
    }

    for (ct_key, tax_kind) in &declared_eligibility {
        if tax_kind != "category" && tax_kind != "tag" {
            return Err(ApiError::bad_request(
                "Chỉ hỗ trợ taxonomy eligibility cho category hoặc tag lõi.",
            ));
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

        let existing_elig: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT owner_kind, owner_id FROM content_type_taxonomy_eligibility
             WHERE content_type_key = ? AND taxonomy_kind = ?",
        )
        .bind(ct_key)
        .bind(tax_kind)
        .fetch_optional(&mut *conn)
        .await?;

        match existing_elig {
            Some((okind, oid)) => {
                if okind != owner_kind_str || oid.as_deref() != Some(owner_id_str) {
                    return Err(ApiError::conflict(
                        "Taxonomy eligibility thuộc chủ sở hữu khác (409).",
                    ));
                }
                sqlx::query(
                    "UPDATE content_type_taxonomy_eligibility
                     SET provider_status = 'active', declaration_state = 'declared', updated_at = datetime('now')
                     WHERE content_type_key = ? AND taxonomy_kind = ?",
                )
                .bind(ct_key)
                .bind(tax_kind)
                .execute(&mut *conn)
                .await?;
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
            sqlx::query(
                "UPDATE content_type_taxonomy_eligibility
                 SET declaration_state = 'withdrawn', provider_status = 'active', updated_at = datetime('now')
                 WHERE content_type_key = ? AND taxonomy_kind = ?",
            )
            .bind(&ct_key)
            .bind(&tax_kind)
            .execute(&mut *conn)
            .await?;
        }
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
