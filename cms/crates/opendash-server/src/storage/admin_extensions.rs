//! Durable, owner-aware, bounded Admin contributions. All SQL remains here.
//! Config is data only; provider-controlled HTML/JS is never evaluated.
use std::collections::BTreeMap;

use opendash_core::admin_extensions::{
    AdminColumn, AdminExtensions, ColumnFormat, MAX_COLUMNS_EFFECTIVE,
};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::error::{ApiError, ApiResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "config", rename_all = "snake_case")]
enum Declaration {
    Column(AdminColumn),
}

#[derive(Debug, Clone)]
struct Registered {
    owner_kind: String,
    owner_id: String,
    local_key: String,
    declaration: Declaration,
    generation: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EffectiveColumn {
    pub key: String,
    pub owner_kind: String,
    pub owner_id: String,
    pub label: String,
    pub field_key: String,
    pub format: ColumnFormat,
    pub order: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EffectiveListConfig {
    pub schema_version: u8,
    pub columns: Vec<EffectiveColumn>,
}

fn qualified(kind: &str, id: &str, key: &str) -> String {
    format!("{kind}/{id}/{key}")
}

/// Active/healthy providers only. A provider switch/disable never purges values.
async fn active_in_conn(conn: &mut SqliteConnection) -> ApiResult<Vec<Registered>> {
    let rows = sqlx::query(
        "SELECT a.owner_kind,a.owner_id,a.local_key,a.declaration_json,
                CASE WHEN a.owner_kind='plugin' THEN p.revision_generation ELSE s.version END generation
         FROM admin_extensions a
         LEFT JOIN plugins p ON a.owner_kind='plugin' AND p.id=a.owner_id
         LEFT JOIN theme_active_selection s ON s.id=1
         LEFT JOIN theme_revisions r ON r.revision_id=s.revision_id
         WHERE a.provider_status='active'
           AND ((a.owner_kind='plugin' AND p.enabled=1 AND p.last_error IS NULL)
             OR (a.owner_kind='theme' AND r.theme_id=a.owner_id AND r.source_format='v1'))
         ORDER BY a.owner_kind,a.owner_id,a.local_key",
    )
    .fetch_all(&mut *conn)
    .await?;
    rows.into_iter()
        .map(|row| {
            let raw: String = row.try_get("declaration_json")?;
            let declaration: Declaration = serde_json::from_str(&raw)
                .map_err(|_| ApiError::internal("admin_extensions registry không hợp lệ."))?;
            Ok(Registered {
                owner_kind: row.try_get("owner_kind")?,
                owner_id: row.try_get("owner_id")?,
                local_key: row.try_get("local_key")?,
                declaration,
                generation: row.try_get("generation")?,
            })
        })
        .collect()
}

/// Validate combined cap BEFORE staging a ZIP and again under the install/
/// activation writer. Replacing an owner excludes its previous declarations.
pub async fn require_capacity_in_conn(
    conn: &mut SqliteConnection,
    owner_kind: &str,
    owner_id: &str,
    candidate: Option<&AdminExtensions>,
) -> ApiResult<()> {
    let mut counts = BTreeMap::<String, usize>::new();
    for reg in active_in_conn(conn).await? {
        if (reg.owner_kind == owner_kind && reg.owner_id == owner_id)
            || (owner_kind == "theme" && reg.owner_kind == "theme")
        {
            continue;
        }
        if let Declaration::Column(column) = reg.declaration {
            for key in column.type_keys {
                *counts.entry(key).or_default() += 1;
            }
        }
    }
    if let Some(ext) = candidate {
        for column in &ext.columns {
            for key in &column.type_keys {
                *counts.entry(key.clone()).or_default() += 1;
            }
        }
    } else {
        // A manifest omitting admin_extensions retains registrations; no
        // replacement should be assumed for an upgrade.
        for reg in active_in_conn(conn).await? {
            if reg.owner_kind == owner_kind && reg.owner_id == owner_id {
                if let Declaration::Column(column) = reg.declaration {
                    for key in column.type_keys {
                        *counts.entry(key).or_default() += 1;
                    }
                }
            }
        }
    }
    if counts.values().any(|count| *count > MAX_COLUMNS_EFFECTIVE) {
        return Err(ApiError::conflict(
            "admin_extensions vượt giới hạn 32 cột hiệu lực cho cùng loại nội dung.",
        ));
    }
    Ok(())
}

pub async fn require_capacity(
    db: &SqlitePool,
    owner_kind: &str,
    owner_id: &str,
    candidate: Option<&AdminExtensions>,
) -> ApiResult<()> {
    let mut conn = db.acquire().await?;
    require_capacity_in_conn(&mut conn, owner_kind, owner_id, candidate).await
}

/// Atomic with installer revision and content declarations transaction.
pub async fn sync_owner_in_conn(
    conn: &mut SqliteConnection,
    owner_kind: &str,
    owner_id: &str,
    extensions: Option<&AdminExtensions>,
    status: &str,
    plugin_actions: &[String],
) -> ApiResult<()> {
    if !matches!(owner_kind, "plugin" | "theme") || !matches!(status, "active" | "inactive") {
        return Err(ApiError::bad_request(
            "Owner/status admin_extensions không hợp lệ.",
        ));
    }
    let Some(ext) = extensions else {
        // Absence is additive and does not delete prior registrations.
        return Ok(());
    };
    ext.validate(owner_kind, plugin_actions)
        .map_err(ApiError::bad_request)?;
    sqlx::query("DELETE FROM admin_extensions WHERE owner_kind=? AND owner_id=?")
        .bind(owner_kind)
        .bind(owner_id)
        .execute(&mut *conn)
        .await?;
    for column in &ext.columns {
        let raw = serde_json::to_string(&Declaration::Column(column.clone()))?;
        sqlx::query("INSERT INTO admin_extensions (owner_kind,owner_id,local_key,declaration_json,provider_status) VALUES (?,?,?,?,?)")
            .bind(owner_kind).bind(owner_id).bind(&column.local_key).bind(raw).bind(status)
            .execute(&mut *conn).await?;
    }
    Ok(())
}

pub async fn set_owner_status_in_conn(
    conn: &mut SqliteConnection,
    kind: &str,
    id: &str,
    enabled: bool,
) -> ApiResult<()> {
    sqlx::query("UPDATE admin_extensions SET provider_status=? WHERE owner_kind=? AND owner_id=?")
        .bind(if enabled { "active" } else { "inactive" })
        .bind(kind)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn set_theme_active_in_conn(
    conn: &mut SqliteConnection,
    theme_id: Option<&str>,
) -> ApiResult<()> {
    sqlx::query("UPDATE admin_extensions SET provider_status='inactive' WHERE owner_kind='theme'")
        .execute(&mut *conn)
        .await?;
    if let Some(id) = theme_id {
        set_owner_status_in_conn(conn, "theme", id, true).await?;
    }
    Ok(())
}

pub async fn effective_list(
    db: &SqlitePool,
    type_key: &str,
    role: opendash_core::content::FieldRole,
) -> ApiResult<EffectiveListConfig> {
    use opendash_core::content::{EffectiveFieldStatus, PublicExposure};
    let mut tx = db.begin().await?;
    let Some(effective) =
        super::content::load_effective_content_schema_in_tx(&mut tx, type_key).await?
    else {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung."));
    };
    if effective.base.status != opendash_core::registry::ProviderStatus::Active {
        return Err(ApiError::not_found("Loại nội dung không khả dụng."));
    }
    let mut columns = Vec::new();
    for reg in active_in_conn(&mut tx).await? {
        if let Declaration::Column(column) = reg.declaration {
            if !column.type_keys.iter().any(|key| key == type_key) {
                continue;
            }
            let Some(field) = effective.effective_fields.get(&column.field_key) else {
                continue;
            };
            if field.effective_status != EffectiveFieldStatus::Active
                || field.definition.effective_acl().2 != PublicExposure::Public
                || role < field.definition.effective_acl().0
                || (role < opendash_core::content::FieldRole::Admin
                    && effective.is_group_admin_only(&field.source_id))
            {
                continue;
            }
            let mut action_id = column.action_id;
            if action_id.is_some() {
                let manifest_opt = if reg.owner_kind == "theme" {
                    let raw: Option<String> = sqlx::query_scalar(
                        "SELECT r.manifest_json
                           FROM theme_active_selection a
                           JOIN theme_revisions r ON r.revision_id = a.revision_id
                          WHERE a.id = 1 AND r.theme_id = ?",
                    )
                    .bind(&reg.owner_id)
                    .fetch_optional(&mut *tx)
                    .await?;
                    raw.and_then(|raw| {
                        serde_json::from_str::<opendash_core::theme::ThemeManifest>(&raw)
                            .ok()
                            .and_then(|m| m.to_runtime_manifest())
                    })
                } else {
                    let raw: Option<String> = sqlx::query_scalar(
                        "SELECT manifest_json FROM plugins WHERE id=? AND enabled=1 AND last_error IS NULL",
                    )
                    .bind(&reg.owner_id)
                    .fetch_optional(&mut *tx)
                    .await?;
                    raw.and_then(|raw| {
                        opendash_core::plugin::PluginManifest::from_json_str(&raw).ok()
                    })
                };
                let Some(manifest) = manifest_opt else {
                    continue;
                };
                if !manifest.actions.iter().any(|action| {
                    action_id.as_deref() == Some(&action.id)
                        && (role == opendash_core::content::FieldRole::Admin
                            || action.required_role == "editor")
                }) {
                    action_id = None;
                }
            }
            columns.push(EffectiveColumn {
                key: qualified(&reg.owner_kind, &reg.owner_id, &reg.local_key),
                owner_kind: reg.owner_kind,
                owner_id: reg.owner_id,
                label: column.label,
                field_key: column.field_key,
                format: column.format,
                order: column.order,
                action_id,
            });
        }
    }
    tx.commit().await?;
    columns.sort_by(|a, b| (a.order, &a.key).cmp(&(b.order, &b.key)));
    if columns.len() > MAX_COLUMNS_EFFECTIVE {
        return Err(ApiError::conflict("Vượt giới hạn cột admin hiệu lực."));
    }
    Ok(EffectiveListConfig {
        schema_version: 1,
        columns,
    })
}

/// Returns the set of `field_key`s targeted by active `AdminColumn` declarations for `type_key`.
pub(in crate::storage) async fn active_column_field_keys_in_conn(
    conn: &mut SqliteConnection,
    type_key: &str,
) -> ApiResult<std::collections::BTreeSet<String>> {
    let mut keys = std::collections::BTreeSet::new();
    for reg in active_in_conn(conn).await? {
        let Declaration::Column(column) = reg.declaration;
        if column.type_keys.iter().any(|k| k == type_key) {
            keys.insert(column.field_key);
        }
    }
    Ok(keys)
}
