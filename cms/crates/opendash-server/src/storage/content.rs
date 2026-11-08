//! Persistent CPT definitions. All content SQL stays in the storage layer.

use opendash_core::{
    content::ContentTypeDefinition,
    registry::{OwnedDescriptor, ProviderStatus, ResourceOwner},
};
use sqlx::SqlitePool;

use crate::error::{ApiError, ApiResult};

/// Stores a user-created CPT definition. Theme/plugin registration is a later
/// slice; the schema already retains owner/status information for that work.
pub async fn insert_user_content_type(
    db: &SqlitePool,
    definition: &ContentTypeDefinition,
) -> ApiResult<OwnedDescriptor<ContentTypeDefinition>> {
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
