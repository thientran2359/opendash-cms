//! Durable, owner-aware, bounded Admin contributions. All SQL remains here.
//! Config is data only; provider-controlled HTML/JS is never evaluated.
use std::collections::{BTreeMap, BTreeSet};

use opendash_core::admin_extensions::{
    AdminColumn, AdminExtensions, AdminTermPanel, ColumnFormat, MAX_COLUMNS_EFFECTIVE,
};
use opendash_core::content::FieldRole;
use opendash_core::field_schema::{FieldContext, FieldSpec, validate_field_value};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::error::{ApiError, ApiResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "config", rename_all = "snake_case")]
enum Declaration {
    Column(AdminColumn),
    TermPanel(AdminTermPanel),
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

#[derive(Debug, Clone, Serialize)]
pub struct EffectiveTermPanel {
    pub key: String,
    pub label: String,
    pub order: i32,
    pub fields: Vec<FieldSpec>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EffectiveTermConfig {
    pub schema_version: u8,
    pub schema_token: String,
    pub panels: Vec<EffectiveTermPanel>,
    pub extensions: BTreeMap<String, BTreeMap<String, Value>>,
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
             OR (a.owner_kind='theme' AND r.theme_id=a.owner_id AND r.source_format='v3'))
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
    for panel in &ext.term_panels {
        let mut panel = panel.clone();
        opendash_core::field_schema::normalize_field_specs_for_scope(
            &mut panel.fields,
            FieldContext::Term,
            opendash_core::field_schema::FieldScopeKind::TermPanelField,
        )
        .map_err(ApiError::bad_request)?;
        let raw = serde_json::to_string(&Declaration::TermPanel(panel.clone()))?;
        sqlx::query("INSERT INTO admin_extensions (owner_kind,owner_id,local_key,declaration_json,provider_status) VALUES (?,?,?,?,?)")
            .bind(owner_kind).bind(owner_id).bind(&panel.local_key).bind(raw).bind(status)
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

fn allowed_field_role(role: &str, min: FieldRole) -> bool {
    role == "admin" || (role == "editor" && min == FieldRole::Editor)
}

fn token_for(kind: &str, role: &str, panels: &[EffectiveTermPanel], gens: &[String]) -> String {
    let canonical =
        serde_json::to_vec(&(kind, role, panels, gens)).expect("bounded serializable declarations");
    Sha256::digest(canonical)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub async fn effective_term_in_conn(
    conn: &mut SqliteConnection,
    term_id: i64,
    kind: &str,
    actor_role: &str,
) -> ApiResult<EffectiveTermConfig> {
    let mut panels = Vec::new();
    let mut gens = Vec::new();
    for reg in active_in_conn(conn).await? {
        gens.push(format!(
            "{}:{}:{}:{}",
            reg.owner_kind, reg.owner_id, reg.local_key, reg.generation,
        ));
        if let Declaration::TermPanel(panel) = reg.declaration {
            if panel.kind != kind {
                continue;
            }
            let mut fields = panel
                .fields
                .into_iter()
                .filter(|field| allowed_field_role(actor_role, field.effective_acl().0))
                .collect::<Vec<_>>();
            if fields.is_empty() {
                continue;
            }
            fields.sort_by(|a, b| a.key.cmp(&b.key));
            let key = qualified(&reg.owner_kind, &reg.owner_id, &reg.local_key);
            panels.push(EffectiveTermPanel {
                key,
                label: panel.label,
                order: panel.order,
                fields,
            });
        }
    }
    panels.sort_by(|a, b| (a.order, &a.key).cmp(&(b.order, &b.key)));
    gens.sort();
    let schema_token = token_for(kind, actor_role, &panels, &gens);
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT owner_kind,owner_id,local_key,values_json FROM term_extension_values WHERE term_id=?")
        .bind(term_id).fetch_all(&mut *conn).await?;
    let stored: BTreeMap<String, BTreeMap<String, Value>> = rows
        .into_iter()
        .filter_map(|(owner_kind, owner_id, local_key, raw)| {
            serde_json::from_str(&raw)
                .ok()
                .map(|value| (qualified(&owner_kind, &owner_id, &local_key), value))
        })
        .collect();
    let mut extensions = BTreeMap::new();
    for panel in &panels {
        let allowed: BTreeSet<_> = panel.fields.iter().map(|f| f.key.as_str()).collect();
        let values = stored
            .get(&panel.key)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|(k, _)| allowed.contains(k.as_str()))
            .collect();
        extensions.insert(panel.key.clone(), values);
    }
    Ok(EffectiveTermConfig {
        schema_version: 1,
        schema_token,
        panels,
        extensions,
    })
}

pub async fn effective_term(
    db: &SqlitePool,
    term_id: i64,
    kind: &str,
    role: &str,
) -> ApiResult<EffectiveTermConfig> {
    let mut conn = db.acquire().await?;
    effective_term_in_conn(&mut conn, term_id, kind, role).await
}

/// Validate full patch before any extension write; caller holds the same term
/// versioned core update transaction. Stored non-mentioned values survive.
pub async fn update_term_values_in_conn(
    conn: &mut SqliteConnection,
    term_id: i64,
    kind: &str,
    actor_role: &str,
    schema_token: &str,
    patches: &BTreeMap<String, Value>,
) -> ApiResult<()> {
    let current = effective_term_in_conn(conn, term_id, kind, actor_role).await?;
    // Check explicit unauthorized fields before token comparison, so a role
    // downgrade never discloses their schema via a conflict response.
    for (panel_key, raw) in patches {
        let panel = current
            .panels
            .iter()
            .find(|p| &p.key == panel_key)
            .ok_or_else(|| ApiError::conflict("Panel đã rút khỏi registry hoặc không có quyền."))?;
        let fields = raw
            .as_object()
            .ok_or_else(|| ApiError::bad_request("extensions panel phải là JSON object."))?;
        for key in fields.keys() {
            if !panel.fields.iter().any(|field| &field.key == key) {
                return Err(ApiError::forbidden(
                    "Không có quyền cập nhật extension field.",
                ));
            }
        }
    }
    if current.schema_token != schema_token {
        return Err(ApiError::conflict(
            "schema_token đã thay đổi, giữ bản nhập và tải lại cấu hình.",
        ));
    }
    for (panel_key, fields) in patches {
        let panel = current
            .panels
            .iter()
            .find(|p| &p.key == panel_key)
            .ok_or_else(|| ApiError::conflict("Panel đã rút khỏi registry hoặc không có quyền."))?;
        let raw = fields
            .as_object()
            .ok_or_else(|| ApiError::bad_request("extensions panel phải là JSON object."))?;
        let segments = panel_key.split('/').collect::<Vec<_>>();
        if segments.len() != 3 {
            return Err(ApiError::bad_request("Extension key không hợp lệ."));
        }
        // IMPORTANT: the role-filtered GET projection is not a valid base for
        // a write. Editor must preserve admin-only keys the actor cannot see.
        let previous: Option<String> = sqlx::query_scalar(
            "SELECT values_json FROM term_extension_values
             WHERE term_id=? AND owner_kind=? AND owner_id=? AND local_key=?",
        )
        .bind(term_id)
        .bind(segments[0])
        .bind(segments[1])
        .bind(segments[2])
        .fetch_optional(&mut *conn)
        .await?;
        let mut saved: BTreeMap<String, Value> = previous
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|_| ApiError::internal("Dữ liệu extension đã lưu không hợp lệ."))?
            .unwrap_or_default();
        for (field_key, value) in raw {
            let def = panel
                .fields
                .iter()
                .find(|f| &f.key == field_key)
                .ok_or_else(|| ApiError::forbidden("Không có quyền cập nhật extension field."))?;
            if value.is_null() {
                if def.required {
                    return Err(ApiError::bad_request(
                        "Trường bắt buộc không được xóa null.",
                    ));
                }
                saved.remove(field_key);
                continue;
            }
            let empty = BTreeMap::new();
            validate_field_value(
                def,
                FieldContext::Term,
                value,
                true,
                true,
                false,
                None,
                &panel.fields,
                &empty,
                None,
                true,
            )
            .map_err(ApiError::bad_request)?;
            saved.insert(field_key.clone(), value.clone());
        }
        sqlx::query(
            "INSERT INTO term_extension_values (term_id,owner_kind,owner_id,local_key,values_json)
             VALUES (?,?,?,?,?) ON CONFLICT(term_id,owner_kind,owner_id,local_key)
             DO UPDATE SET values_json=excluded.values_json",
        )
        .bind(term_id)
        .bind(segments[0])
        .bind(segments[1])
        .bind(segments[2])
        .bind(serde_json::to_string(&saved)?)
        .execute(&mut *conn)
        .await?;
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
                || role < field.definition.effective_acl().0
                || field.definition.effective_acl().2 != PublicExposure::Public
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
                        serde_json::from_str::<opendash_core::theme::ThemeV3Manifest>(&raw)
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
