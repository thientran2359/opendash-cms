//! Site-owned template overrides (`page`, `header`, `footer`, `archive`, `single`,
//! `taxonomy_archive`) with CAS versioning, scoped precedence, and RouteContext support.
use std::collections::BTreeMap;

use opendash_core::{
    builder::BuilderDocument,
    content::{ContentTypeDefinition, valid_content_type_key},
};
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::{
    content_render::{self, SiteContext},
    error::{ApiError, ApiResult},
    page::{self, RenderedLayout},
    storage::{
        builder::{
            self, HomeState, ListResults, check_preview_images, resolve_contact_forms_in_tx,
            resolve_lists_in_tx,
        },
        builder_binding::{self, BindingResults},
        theme::ActiveV2RoleTemplates,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverrideRole {
    Page,
    Header,
    Footer,
    Archive,
    Single,
    TaxonomyArchive,
}

impl OverrideRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Page => "page",
            Self::Header => "header",
            Self::Footer => "footer",
            Self::Archive => "archive",
            Self::Single => "single",
            Self::TaxonomyArchive => "taxonomy_archive",
        }
    }

    pub fn parse(raw: &str) -> ApiResult<Self> {
        match raw {
            "page" => Ok(Self::Page),
            "header" => Ok(Self::Header),
            "footer" => Ok(Self::Footer),
            "archive" => Ok(Self::Archive),
            "single" => Ok(Self::Single),
            "taxonomy_archive" => Ok(Self::TaxonomyArchive),
            "home" => Err(ApiError::bad_request(
                "Trang chủ dùng /api/admin/builder/homepage, không tạo override riêng.",
            )),
            _ => Err(ApiError::bad_request("Role template override không hợp lệ.")),
        }
    }

    pub fn allows_entry_scope(self) -> bool {
        matches!(self, Self::Page | Self::Single)
    }

    pub fn allows_type_scope(self) -> bool {
        matches!(self, Self::Page | Self::Archive | Self::Single)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverrideScope {
    pub role: OverrideRole,
    pub scope_type_key: Option<String>,
    pub scope_entry_id: Option<i64>,
}

impl OverrideScope {
    pub fn new(
        role: OverrideRole,
        scope_type_key: Option<String>,
        scope_entry_id: Option<i64>,
    ) -> ApiResult<Self> {
        let scope_type_key = scope_type_key.filter(|s| !s.trim().is_empty());
        if let Some(ref tk) = scope_type_key {
            if !role.allows_type_scope() {
                return Err(ApiError::bad_request(
                    "Role này không hỗ trợ scope theo loại nội dung.",
                ));
            }
            if !valid_content_type_key(tk) {
                return Err(ApiError::bad_request("scope_type_key không hợp lệ."));
            }
        }
        if let Some(eid) = scope_entry_id {
            if !role.allows_entry_scope() {
                return Err(ApiError::bad_request(
                    "Chỉ role page hoặc single mới hỗ trợ scope theo entry.",
                ));
            }
            if scope_type_key.is_none() {
                return Err(ApiError::bad_request(
                    "Cần scope_type_key khi chỉ định scope_entry_id.",
                ));
            }
            if eid <= 0 {
                return Err(ApiError::bad_request("scope_entry_id không hợp lệ."));
            }
        }
        Ok(Self {
            role,
            scope_type_key,
            scope_entry_id,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateOverrideRecord {
    pub id: i64,
    pub role: String,
    pub scope_type_key: Option<String>,
    pub scope_entry_id: Option<i64>,
    pub document: BuilderDocument,
    pub version: i64,
    pub base_theme_id: Option<String>,
    pub base_revision_id: Option<String>,
    pub updated_at: String,
}

type OverrideRow = (
    i64,
    String,
    Option<String>,
    Option<i64>,
    String,
    i64,
    Option<String>,
    Option<String>,
    String,
);

fn row_to_record(row: OverrideRow) -> ApiResult<TemplateOverrideRecord> {
    let document = BuilderDocument::from_json(row.4.as_bytes())
        .map_err(|_| ApiError::internal("Template override AST trong DB không hợp lệ."))?;
    Ok(TemplateOverrideRecord {
        id: row.0,
        role: row.1,
        scope_type_key: row.2,
        scope_entry_id: row.3,
        document,
        version: row.5,
        base_theme_id: row.6,
        base_revision_id: row.7,
        updated_at: row.8,
    })
}

pub async fn list_overrides(db: &SqlitePool) -> ApiResult<Vec<TemplateOverrideRecord>> {
    let rows: Vec<OverrideRow> = sqlx::query_as(
        "SELECT id, role, scope_type_key, scope_entry_id, ast_json, version,
                base_theme_id, base_revision_id, updated_at
           FROM template_overrides
          ORDER BY role ASC, COALESCE(scope_type_key, '') ASC, COALESCE(scope_entry_id, -1) ASC",
    )
    .fetch_all(db)
    .await?;
    rows.into_iter().map(row_to_record).collect()
}

pub async fn get_override(
    db: &SqlitePool,
    scope: &OverrideScope,
) -> ApiResult<Option<TemplateOverrideRecord>> {
    let mut tx = db.begin().await?;
    let record = get_override_in_tx(&mut tx, scope).await?;
    tx.commit().await?;
    Ok(record)
}

async fn get_override_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    scope: &OverrideScope,
) -> ApiResult<Option<TemplateOverrideRecord>> {
    let row: Option<OverrideRow> = sqlx::query_as(
        "SELECT id, role, scope_type_key, scope_entry_id, ast_json, version,
                base_theme_id, base_revision_id, updated_at
           FROM template_overrides
          WHERE role = ?
            AND COALESCE(scope_type_key, '') = COALESCE(?, '')
            AND COALESCE(scope_entry_id, -1) = COALESCE(?, -1)",
    )
    .bind(scope.role.as_str())
    .bind(scope.scope_type_key.as_deref())
    .bind(scope.scope_entry_id)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(row_to_record).transpose()
}

/// Resolves the most specific saved override for a public route context:
/// `(role, type_key, entry_id)` -> `(role, type_key, None)` -> `(role, None, None)`.
pub async fn resolve_effective_override(
    db: &SqlitePool,
    role: OverrideRole,
    type_key: Option<&str>,
    entry_id: Option<i64>,
) -> ApiResult<Option<TemplateOverrideRecord>> {
    let row: Option<OverrideRow> = sqlx::query_as(
        "SELECT id, role, scope_type_key, scope_entry_id, ast_json, version,
                base_theme_id, base_revision_id, updated_at
           FROM template_overrides
          WHERE role = ?
            AND (scope_type_key IS NULL OR scope_type_key = ?)
            AND (scope_entry_id IS NULL OR scope_entry_id = ?)
          ORDER BY (scope_entry_id IS NOT NULL) DESC,
                   (scope_type_key IS NOT NULL) DESC,
                   id DESC
          LIMIT 1",
    )
    .bind(role.as_str())
    .bind(type_key)
    .bind(entry_id)
    .fetch_optional(db)
    .await?;
    row.map(row_to_record).transpose()
}

async fn validate_scope_and_load_cpt_fields(
    tx: &mut Transaction<'_, Sqlite>,
    scope: &OverrideScope,
) -> ApiResult<Option<Vec<String>>> {
    let Some(ref type_key) = scope.scope_type_key else {
        return Ok(None);
    };
    let def_json: Option<String> =
        sqlx::query_scalar("SELECT definition_json FROM content_types WHERE key = ?")
            .bind(type_key)
            .fetch_optional(&mut **tx)
            .await?;
    let Some(raw) = def_json else {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung cho scope."));
    };
    let def: ContentTypeDefinition = serde_json::from_str(&raw)
        .map_err(|_| ApiError::internal("Định nghĩa loại nội dung không hợp lệ."))?;
    let field_keys: Vec<String> = def.fields.into_iter().map(|f| f.key).collect();

    if let Some(entry_id) = scope.scope_entry_id {
        let exists: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM content_entries WHERE id = ? AND content_type_key = ?",
        )
        .bind(entry_id)
        .bind(type_key)
        .fetch_optional(&mut **tx)
        .await?;
        if exists.is_none() {
            return Err(ApiError::not_found("Không tìm thấy entry cho scope."));
        }
    }

    Ok(Some(field_keys))
}

pub async fn preview_override(
    db: &SqlitePool,
    scope: &OverrideScope,
    document: &BuilderDocument,
    preview_entry: Option<(&str, &str)>,
) -> ApiResult<RenderedLayout> {
    check_preview_images(db, document).await?;
    let mut tx = db.begin().await?;
    let scoped_fields = validate_scope_and_load_cpt_fields(&mut tx, scope).await?;
    let state = builder::read_home_tx(&mut tx).await?;
    let existing = get_override_in_tx(&mut tx, scope).await?;
    let prior_doc = existing.as_ref().map(|r| &r.document);
    document
        .validate_for_save(&state.descriptors()?, prior_doc)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    document
        .validate_route_context_for_role(Some(scope.role.as_str()), scoped_fields.as_deref())
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    let effective_entry: Option<(String, String)> = match preview_entry {
        Some((tk, slug)) => Some((tk.to_string(), slug.to_string())),
        None => {
            if let (Some(tk), Some(eid)) = (&scope.scope_type_key, scope.scope_entry_id) {
                let slug: Option<String> = sqlx::query_scalar(
                    "SELECT slug FROM content_entries WHERE id = ? AND content_type_key = ?",
                )
                .bind(eid)
                .bind(tk)
                .fetch_optional(&mut *tx)
                .await?;
                slug.map(|s| (tk.clone(), s))
            } else {
                None
            }
        }
    };
    let route_ref = effective_entry
        .as_ref()
        .map(|(t, s)| (t.as_str(), s.as_str()));
    let bindings = builder_binding::resolve_in_tx_with_route(&mut tx, document, true, route_ref).await?;
    let lists = resolve_lists_in_tx(&mut tx, document, &BTreeMap::new()).await?;
    let forms = resolve_contact_forms_in_tx(&mut tx, document).await?;
    tx.commit().await?;
    page::render_builder_with_lists_and_forms(&state, document, true, &bindings, &lists, &forms)
}

pub async fn save_override_and_render(
    db: &SqlitePool,
    scope: &OverrideScope,
    document: &BuilderDocument,
    expected_version: Option<i64>,
) -> ApiResult<(TemplateOverrideRecord, RenderedLayout)> {
    check_preview_images(db, document).await?;
    let mut tx = db.begin().await?;
    let scoped_fields = validate_scope_and_load_cpt_fields(&mut tx, scope).await?;
    let state = builder::read_home_tx(&mut tx).await?;
    let existing = get_override_in_tx(&mut tx, scope).await?;

    match (&existing, expected_version) {
        (Some(_row), None) => {
            return Err(ApiError::precondition_required(
                "Cần If-Match khi cập nhật template override đã tồn tại.",
            ));
        }
        (Some(row), Some(v)) if v != row.version => {
            return Err(ApiError::precondition_failed(
                "Template override đã thay đổi bởi phiên bản khác.",
            ));
        }
        (None, Some(v)) if v != 0 => {
            return Err(ApiError::precondition_failed(
                "Template override chưa tồn tại với phiên bản yêu cầu.",
            ));
        }
        _ => {}
    }

    let prior_doc = existing.as_ref().map(|r| &r.document);
    document
        .validate_for_save(&state.descriptors()?, prior_doc)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    document
        .validate_route_context_for_role(Some(scope.role.as_str()), scoped_fields.as_deref())
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    let ast_json = serde_json::to_string(document)?;
    let active_theme: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT r.theme_id, a.revision_id
           FROM theme_active_selection a
           LEFT JOIN theme_revisions r ON r.revision_id = a.revision_id
          WHERE a.id = 1",
    )
    .fetch_optional(&mut *tx)
    .await?;
    let (base_theme_id, base_revision_id) = active_theme.unwrap_or((None, None));

    let next_version = existing.as_ref().map_or(1, |r| r.version + 1);
    if let Some(ref row) = existing {
        let res = sqlx::query(
            "UPDATE template_overrides
                SET ast_json = ?,
                    version = ?,
                    base_theme_id = COALESCE(base_theme_id, ?),
                    base_revision_id = COALESCE(base_revision_id, ?),
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
              WHERE id = ? AND version = ?",
        )
        .bind(&ast_json)
        .bind(next_version)
        .bind(base_theme_id.as_deref())
        .bind(base_revision_id.as_deref())
        .bind(row.id)
        .bind(row.version)
        .execute(&mut *tx)
        .await?;
        if res.rows_affected() == 0 {
            return Err(ApiError::precondition_failed(
                "Template override đã thay đổi đồng thời.",
            ));
        }
    } else {
        sqlx::query(
            "INSERT INTO template_overrides (
                role, scope_type_key, scope_entry_id, ast_json, version, base_theme_id, base_revision_id
             ) VALUES (?, ?, ?, ?, 1, ?, ?)",
        )
        .bind(scope.role.as_str())
        .bind(scope.scope_type_key.as_deref())
        .bind(scope.scope_entry_id)
        .bind(&ast_json)
        .bind(base_theme_id.as_deref())
        .bind(base_revision_id.as_deref())
        .execute(&mut *tx)
        .await?;
    }

    let saved = get_override_in_tx(&mut tx, scope)
        .await?
        .ok_or_else(|| ApiError::internal("Không đọc lại được template override vừa lưu."))?;
    let bindings = builder_binding::resolve_in_tx_with_route(&mut tx, document, false, None).await?;
    let lists = resolve_lists_in_tx(&mut tx, document, &BTreeMap::new()).await?;
    let forms = resolve_contact_forms_in_tx(&mut tx, document).await?;
    let rendered = page::render_builder_with_lists_and_forms(&state, document, false, &bindings, &lists, &forms)?;
    tx.commit().await?;
    Ok((saved, rendered))
}

pub async fn reset_override(
    db: &SqlitePool,
    scope: &OverrideScope,
    expected_version: i64,
    confirmed: bool,
) -> ApiResult<()> {
    if !confirmed {
        return Err(ApiError::bad_request(
            "Reset template override cần xác nhận rõ ràng (confirm=true).",
        ));
    }
    let mut tx = db.begin().await?;
    let existing = get_override_in_tx(&mut tx, scope)
        .await?
        .ok_or_else(|| ApiError::not_found("Không có template override để reset."))?;
    if existing.version != expected_version {
        return Err(ApiError::precondition_failed(
            "Template override đã thay đổi, không thể reset.",
        ));
    }
    let res = sqlx::query("DELETE FROM template_overrides WHERE id = ? AND version = ?")
        .bind(existing.id)
        .bind(expected_version)
        .execute(&mut *tx)
        .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::precondition_failed(
            "Template override đã thay đổi đồng thời.",
        ));
    }
    tx.commit().await?;
    Ok(())
}

pub async fn render_override_for_public(
    db: &SqlitePool,
    record: &TemplateOverrideRecord,
    route_entry: Option<(&str, &str)>,
) -> ApiResult<RenderedLayout> {
    let mut tx = db.begin().await?;
    let state: HomeState = builder::read_home_tx(&mut tx).await?;
    let bindings: BindingResults =
        builder_binding::resolve_in_tx_with_route(&mut tx, &record.document, false, route_entry)
            .await?;
    let lists: ListResults =
        resolve_lists_in_tx(&mut tx, &record.document, &BTreeMap::new()).await?;
    let forms = resolve_contact_forms_in_tx(&mut tx, &record.document).await?;
    tx.commit().await?;
    page::render_builder_with_lists_and_forms(&state, &record.document, false, &bindings, &lists, &forms)
}

pub async fn resolve_and_render_header_footer(
    db: &SqlitePool,
    active_v2: Option<&ActiveV2RoleTemplates>,
    site_ctx: &SiteContext<'_>,
    theme_opts: &BTreeMap<String, serde_json::Value>,
    type_key: Option<&str>,
) -> ApiResult<(Option<String>, Option<String>)> {
    let menus = crate::storage::menu::resolve_active_slot_menus(db).await?;
    let header = match resolve_effective_override(db, OverrideRole::Header, type_key, None).await?
    {
        Some(record) => Some(render_override_for_public(db, &record, None).await?.html),
        None => active_v2
            .and_then(|v2| v2.roles.get("header"))
            .and_then(|t| content_render::render_role_fragment_with_context(t, site_ctx, theme_opts, &menus)),
    };
    let footer = match resolve_effective_override(db, OverrideRole::Footer, type_key, None).await?
    {
        Some(record) => Some(render_override_for_public(db, &record, None).await?.html),
        None => active_v2
            .and_then(|v2| v2.roles.get("footer"))
            .and_then(|t| content_render::render_role_fragment_with_context(t, site_ctx, theme_opts, &menus)),
    };
    Ok((header, footer))
}
