//! Persistent CPT definitions. All content SQL stays in the storage layer.

use opendash_core::{
    content::{ContentFieldDefinition, ContentFieldType, ContentTypeDefinition},
    registry::{OwnedDescriptor, ProviderStatus, ResourceOwner},
};
use sqlx::SqlitePool;

use crate::error::{ApiError, ApiResult};

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

/// Stores a user-created CPT definition. Theme/plugin registration is a later
/// slice; the schema already retains owner/status information for that work.
pub async fn insert_user_content_type(
    db: &SqlitePool,
    definition: &ContentTypeDefinition,
) -> ApiResult<OwnedDescriptor<ContentTypeDefinition>> {
    if builtin_definition(&definition.key).is_some() {
        return Err(ApiError::conflict("post/page là khóa Core được bảo vệ."));
    }
    let definition_json = serde_json::to_string(definition)?;
    let result = sqlx::query(
        "INSERT INTO content_types (key, definition_json, owner_kind, owner_id, provider_status)
         VALUES (?, ?, 'user', NULL, 'active')",
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
                descriptor: definition.clone(),
            })
        }
    }
}

pub async fn list_content_types(
    db: &SqlitePool,
) -> ApiResult<Vec<OwnedDescriptor<ContentTypeDefinition>>> {
    let rows: Vec<(String, String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT key, definition_json, owner_kind, owner_id, provider_status
         FROM content_types ORDER BY key ASC",
    )
    .fetch_all(db)
    .await?;

    rows.into_iter()
        .map(|(key, definition_json, owner_kind, owner_id, status)| {
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
                _ => return Err(ApiError::internal("Trạng thái loại nội dung không hợp lệ.")),
            };
            Ok(OwnedDescriptor {
                owner,
                status,
                descriptor,
            })
        })
        .collect()
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
