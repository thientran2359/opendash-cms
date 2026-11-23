//! Site-owned typed templates. Candidate specificity wins before source priority:
//! for each candidate, site-owned Builder AST precedes active theme v3 HTML.
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use opendash_core::{
    builder::{BuilderDocument, ElementContent, KnownElement},
    content::{
        ContentSupport, ContentTypeDefinition, EffectiveContentSchema, FieldRole,
        valid_content_type_key,
    },
    registry::{DeclarationState, ProviderStatus},
    theme::{TemplateCandidate, TemplateCandidateKind},
};
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

use crate::{
    auth::SessionContext,
    content_render::{self, SiteContext},
    error::{ApiError, ApiResult},
    page::{self, RenderAnnotationScope, RenderedLayout},
    storage::{
        builder::{
            self, HomeState, ListResults, check_preview_images, resolve_contact_forms_in_tx,
            resolve_lists_in_tx,
        },
        builder_binding::{self, BindingResults},
        content, entries, menu, settings, theme,
        theme::ActiveThemeV3,
    },
    template_sec::{self, PreviewCompositionBudget},
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
            _ => Err(ApiError::bad_request(
                "Role template override không hợp lệ.",
            )),
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
            // Site-owned Page body overrides have exactly one identity. A
            // post/CPT must never be addressable through the Page role.
            if role == OverrideRole::Page && scope_type_key.as_deref() != Some("page") {
                return Err(ApiError::bad_request("Page Builder cần type_key=page."));
            }
        }
        Ok(Self {
            role,
            scope_type_key,
            scope_entry_id,
        })
    }
}

/// The route must authenticate publication, provider visibility, archive
/// eligibility and assigned template eligibility before constructing context.
#[derive(Debug, Clone)]
pub enum PublicTemplateContext {
    Home {
        page_layout_key: Option<String>,
    },
    Page {
        page_layout_key: Option<String>,
    },
    Single {
        content_type: String,
        entry_id: i64,
        slug: String,
        custom_template_key: Option<String>,
    },
    Archive {
        content_type: String,
    },
    Taxonomy {
        taxonomy: String,
        term_id: i64,
        slug: String,
    },
    NotFound,
    Part {
        part_key: String,
    },
}

fn candidate(kind: TemplateCandidateKind) -> TemplateCandidate {
    TemplateCandidate::new(kind)
}

pub fn build_candidates(context: &PublicTemplateContext) -> ApiResult<Vec<TemplateCandidate>> {
    use TemplateCandidateKind as K;
    let mut result = Vec::new();
    match context {
        PublicTemplateContext::Home { page_layout_key } => {
            result.push(candidate(K::Home));
            if let Some(layout_key) = page_layout_key {
                let mut layout = candidate(K::PageLayout);
                layout.layout_key = Some(layout_key.clone());
                result.push(layout);
            }
            result.push(candidate(K::Page));
            result.push(candidate(K::Index));
        }
        PublicTemplateContext::Page { page_layout_key } => {
            if let Some(layout_key) = page_layout_key {
                let mut layout = candidate(K::PageLayout);
                layout.layout_key = Some(layout_key.clone());
                result.push(layout);
            }
            result.push(candidate(K::Page));
            result.push(candidate(K::Index));
        }
        PublicTemplateContext::Single {
            content_type,
            entry_id,
            slug,
            custom_template_key,
        } => {
            if let Some(custom_key) = custom_template_key {
                let mut custom = candidate(K::SingleCustom);
                custom.custom_key = Some(custom_key.clone());
                result.push(custom);
            }
            let mut specific = candidate(K::Single);
            specific.content_type = Some(content_type.clone());
            specific.entry_id = Some(*entry_id);
            result.push(specific);
            let mut slugged = candidate(K::Single);
            slugged.content_type = Some(content_type.clone());
            slugged.selector_slug = Some(slug.clone());
            result.push(slugged);
            let mut typed = candidate(K::Single);
            typed.content_type = Some(content_type.clone());
            result.push(typed);
            result.push(candidate(K::Single));
            result.push(candidate(K::Singular));
            result.push(candidate(K::Index));
        }
        PublicTemplateContext::Archive { content_type } => {
            let mut typed = candidate(K::Archive);
            typed.content_type = Some(content_type.clone());
            result.push(typed);
            result.push(candidate(K::Archive));
            result.push(candidate(K::Index));
        }
        PublicTemplateContext::Taxonomy {
            taxonomy,
            term_id,
            slug,
        } => {
            let mut specific = candidate(K::Taxonomy);
            specific.taxonomy = Some(taxonomy.clone());
            specific.term_id = Some(*term_id);
            result.push(specific);
            let mut slugged = candidate(K::Taxonomy);
            slugged.taxonomy = Some(taxonomy.clone());
            slugged.selector_slug = Some(slug.clone());
            result.push(slugged);
            let mut typed = candidate(K::Taxonomy);
            typed.taxonomy = Some(taxonomy.clone());
            result.push(typed);
            result.push(candidate(K::Taxonomy));
            result.push(candidate(K::Archive));
            result.push(candidate(K::Index));
        }
        PublicTemplateContext::NotFound => {
            result.push(candidate(K::NotFound));
            result.push(candidate(K::Index));
        }
        PublicTemplateContext::Part { part_key } => {
            let mut part = candidate(K::Part);
            part.part_key = Some(part_key.clone());
            result.push(part);
        }
    }
    for item in &result {
        item.canonical_key().map_err(ApiError::bad_request)?;
    }
    Ok(result)
}

/// For the Admin reset API, reconstruct the fallback chain from a stored
/// target. ID-scoped targets recover the saved slug from SQLite so resetting
/// an ID override can correctly reveal a more specific slug theme template.
/// Callers with a fully authorized public route context may instead pass
/// `build_candidates` for that context to `reset_typed_override`.
pub async fn build_reset_candidate_chain(
    db: &SqlitePool,
    target: &TemplateCandidate,
) -> ApiResult<Vec<TemplateCandidate>> {
    let mut conn = db.acquire().await?;
    build_reset_candidate_chain_in_conn(&mut conn, target).await
}

pub(super) async fn build_reset_candidate_chain_in_conn(
    conn: &mut SqliteConnection,
    target: &TemplateCandidate,
) -> ApiResult<Vec<TemplateCandidate>> {
    use TemplateCandidateKind as K;
    let mut candidates = vec![target.clone()];
    match target.kind {
        K::Home => {
            let layout: Option<String> = sqlx::query_scalar(
                "SELECT e.page_layout_key FROM site_reading_settings AS reading
                   JOIN site_locales AS sl ON sl.code = reading.locale AND sl.is_default = 1
                   JOIN content_entries AS e ON e.id = reading.homepage_page_id
                  WHERE e.content_type_key='page'
                    AND e.publication_status='published'",
            )
            .fetch_optional(&mut *conn)
            .await?
            .flatten();
            if let Some(layout_key) = layout {
                let mut layout_candidate = candidate(K::PageLayout);
                layout_candidate.layout_key = Some(layout_key);
                candidates.push(layout_candidate);
            }
            candidates.push(candidate(K::Page));
            candidates.push(candidate(K::Index));
        }
        K::PageLayout => {
            candidates.push(candidate(K::Page));
            candidates.push(candidate(K::Index));
        }
        K::Page => candidates.push(candidate(K::Index)),
        K::Single => {
            if let (Some(entry_id), Some(content_type)) =
                (target.entry_id, target.content_type.as_deref())
            {
                let slug: Option<String> = sqlx::query_scalar(
                    "SELECT slug FROM content_entries WHERE id=? AND content_type_key=?",
                )
                .bind(entry_id)
                .bind(content_type)
                .fetch_optional(&mut *conn)
                .await?;
                if let Some(selector_slug) = slug {
                    let mut slugged = candidate(K::Single);
                    slugged.content_type = Some(content_type.to_owned());
                    slugged.selector_slug = Some(selector_slug);
                    candidates.push(slugged);
                }
            }
            if let Some(content_type) = &target.content_type {
                let mut typed = candidate(K::Single);
                typed.content_type = Some(content_type.clone());
                candidates.push(typed);
            }
            candidates.push(candidate(K::Single));
            candidates.push(candidate(K::Singular));
            candidates.push(candidate(K::Index));
        }
        K::SingleCustom => {
            candidates.push(candidate(K::Single));
            candidates.push(candidate(K::Singular));
            candidates.push(candidate(K::Index));
        }
        K::Singular => candidates.push(candidate(K::Index)),
        K::Archive => {
            candidates.push(candidate(K::Archive));
            candidates.push(candidate(K::Index));
        }
        K::Taxonomy => {
            if let (Some(term_id), Some(taxonomy)) = (target.term_id, target.taxonomy.as_deref()) {
                let slug: Option<String> =
                    sqlx::query_scalar("SELECT slug FROM taxonomy_terms WHERE id=? AND kind=?")
                        .bind(term_id)
                        .bind(taxonomy)
                        .fetch_optional(&mut *conn)
                        .await?;
                if let Some(selector_slug) = slug {
                    let mut slugged = candidate(K::Taxonomy);
                    slugged.taxonomy = Some(taxonomy.to_owned());
                    slugged.selector_slug = Some(selector_slug);
                    candidates.push(slugged);
                }
            }
            if let Some(taxonomy) = &target.taxonomy {
                let mut typed = candidate(K::Taxonomy);
                typed.taxonomy = Some(taxonomy.clone());
                candidates.push(typed);
            }
            candidates.push(candidate(K::Taxonomy));
            candidates.push(candidate(K::Archive));
            candidates.push(candidate(K::Index));
        }
        K::NotFound => candidates.push(candidate(K::Index)),
        K::Index | K::Part => {}
    }
    let mut seen = std::collections::BTreeSet::new();
    candidates.retain(|candidate| match candidate.canonical_key() {
        Ok(key) => seen.insert(key),
        // Keep malformed candidates for the explicit error pass below.
        Err(_) => true,
    });
    for candidate in &candidates {
        candidate.canonical_key().map_err(ApiError::bad_request)?;
    }
    Ok(candidates)
}

fn candidate_for_legacy_scope(scope: &OverrideScope) -> ApiResult<TemplateCandidate> {
    use TemplateCandidateKind as K;
    let mut result = match scope.role {
        OverrideRole::Page => candidate(K::Page),
        OverrideRole::Header | OverrideRole::Footer => {
            let mut part = candidate(K::Part);
            part.part_key = Some(scope.role.as_str().to_string());
            part
        }
        OverrideRole::Archive => candidate(K::Archive),
        OverrideRole::Single => candidate(K::Single),
        OverrideRole::TaxonomyArchive => candidate(K::Taxonomy),
    };
    if scope.role == OverrideRole::Page
        && (scope.scope_entry_id.is_some() || scope.scope_type_key.is_some())
    {
        return Err(ApiError::bad_request(
            "Legacy Page scope has no typed template identity.",
        ));
    }
    result.content_type = scope.scope_type_key.clone();
    result.entry_id = scope.scope_entry_id;
    result.canonical_key().map_err(ApiError::bad_request)?;
    Ok(result)
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateOverrideRecord {
    pub id: i64,
    pub target_kind: TemplateCandidateKind,
    pub canonical_target_key: String,
    pub target: TemplateCandidate,
    #[serde(skip_serializing)]
    pub role: String,
    #[serde(skip_serializing)]
    pub scope_type_key: Option<String>,
    #[serde(skip_serializing)]
    pub scope_entry_id: Option<i64>,
    #[serde(rename = "ast_json")]
    pub document: BuilderDocument,
    pub version: i64,
    pub base_theme_id: Option<String>,
    pub base_revision_digest: Option<String>,
    #[serde(skip_serializing)]
    pub base_revision_id: Option<String>,
    pub updated_at: String,
}

pub type TemplateTargetInput = TemplateCandidate;

#[derive(Debug, Clone)]
pub enum TemplateResolution {
    DbOverride(TemplateOverrideRecord),
    ActiveTheme { target_key: String, html: String },
    CoreFallback,
}

#[derive(Debug, Clone, Serialize)]
pub struct EffectiveSourceDescriptor {
    pub kind: &'static str,
    pub label: String,
    pub target_key: Option<String>,
}

impl TemplateResolution {
    pub fn effective_source(&self) -> EffectiveSourceDescriptor {
        match self {
            Self::DbOverride(record) => EffectiveSourceDescriptor {
                kind: "db_override",
                label: format!("Site template: {}", record.canonical_target_key),
                target_key: Some(record.canonical_target_key.clone()),
            },
            Self::ActiveTheme { target_key, .. } => EffectiveSourceDescriptor {
                kind: "active_theme",
                label: format!("Active theme: {target_key}"),
                target_key: Some(target_key.clone()),
            },
            Self::CoreFallback => EffectiveSourceDescriptor {
                kind: "core_fallback",
                label: "Core fallback".into(),
                target_key: None,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ResetTemplateOutcome {
    pub reset_id: i64,
    pub canonical_target_key: String,
    pub next_effective_source: EffectiveSourceDescriptor,
}

type OverrideRow = (
    i64,
    String,
    String,
    String,
    String,
    i64,
    Option<String>,
    Option<String>,
    String,
);

fn row_to_record(row: OverrideRow) -> ApiResult<TemplateOverrideRecord> {
    let target: TemplateCandidate = serde_json::from_str(&row.3)
        .map_err(|_| ApiError::internal("Template override target trong DB không hợp lệ."))?;
    let key = target.canonical_key().map_err(ApiError::internal)?;
    if key != row.2
        || serde_json::to_value(target.kind)? != serde_json::Value::String(row.1.clone())
    {
        return Err(ApiError::internal(
            "Template override target và canonical key không khớp.",
        ));
    }
    let document = BuilderDocument::from_json(row.4.as_bytes())
        .map_err(|_| ApiError::internal("Template override AST trong DB không hợp lệ."))?;
    let role = match target.kind {
        TemplateCandidateKind::Home
        | TemplateCandidateKind::Page
        | TemplateCandidateKind::PageLayout => "page",
        TemplateCandidateKind::Single
        | TemplateCandidateKind::SingleCustom
        | TemplateCandidateKind::Singular => "single",
        TemplateCandidateKind::Archive => "archive",
        TemplateCandidateKind::Taxonomy => "taxonomy_archive",
        TemplateCandidateKind::Part => target.part_key.as_deref().unwrap_or("part"),
        TemplateCandidateKind::Index | TemplateCandidateKind::NotFound => "index",
    };
    Ok(TemplateOverrideRecord {
        id: row.0,
        target_kind: target.kind,
        canonical_target_key: key,
        role: role.to_string(),
        scope_type_key: target.content_type.clone(),
        scope_entry_id: target.entry_id,
        target,
        document,
        version: row.5,
        base_theme_id: row.6,
        base_revision_digest: row.7.clone(),
        base_revision_id: row.7,
        updated_at: row.8,
    })
}

async fn get_typed_override_on_connection(
    conn: &mut SqliteConnection,
    canonical_key: &str,
) -> ApiResult<Option<TemplateOverrideRecord>> {
    let row: Option<OverrideRow> = sqlx::query_as(
        "SELECT id, target_kind, canonical_target_key, target_json, ast_json, version,
                base_theme_id, base_revision_digest, updated_at
           FROM template_overrides WHERE canonical_target_key = ?",
    )
    .bind(canonical_key)
    .fetch_optional(&mut *conn)
    .await?;
    row.map(row_to_record).transpose()
}

pub async fn get_typed_override(
    db: &SqlitePool,
    target: &TemplateCandidate,
) -> ApiResult<Option<TemplateOverrideRecord>> {
    let key = target.canonical_key().map_err(ApiError::bad_request)?;
    let mut conn = db.acquire().await?;
    get_typed_override_on_connection(&mut conn, &key).await
}

pub async fn get_typed_override_by_id(
    db: &SqlitePool,
    override_id: i64,
) -> ApiResult<Option<TemplateOverrideRecord>> {
    let row: Option<OverrideRow> = sqlx::query_as(
        "SELECT id, target_kind, canonical_target_key, target_json, ast_json, version,
                base_theme_id, base_revision_digest, updated_at
           FROM template_overrides WHERE id = ?",
    )
    .bind(override_id)
    .fetch_optional(db)
    .await?;
    row.map(row_to_record).transpose()
}

fn theme_source_for_candidate<'a>(
    candidate: &TemplateCandidate,
    target_key: &str,
    templates: Option<&'a BTreeMap<String, String>>,
    parts: Option<&'a BTreeMap<String, String>>,
    layouts: Option<&'a BTreeMap<String, String>>,
) -> Option<&'a str> {
    match candidate.kind {
        TemplateCandidateKind::Part => candidate
            .part_key
            .as_deref()
            .and_then(|key| parts.and_then(|map| map.get(key)))
            .map(String::as_str),
        TemplateCandidateKind::PageLayout => candidate
            .layout_key
            .as_deref()
            .and_then(|key| layouts.and_then(|map| map.get(key)))
            .map(String::as_str),
        _ => templates
            .and_then(|map| map.get(target_key))
            .map(String::as_str),
    }
}

pub(super) async fn resolve_template_2axis_on_connection(
    conn: &mut SqliteConnection,
    candidates: &[TemplateCandidate],
    templates: Option<&BTreeMap<String, String>>,
    parts: Option<&BTreeMap<String, String>>,
    layouts: Option<&BTreeMap<String, String>>,
) -> ApiResult<TemplateResolution> {
    for candidate in candidates {
        let key = candidate.canonical_key().map_err(ApiError::bad_request)?;
        if let Some(record) = get_typed_override_on_connection(conn, &key).await? {
            return Ok(TemplateResolution::DbOverride(record));
        }
        if let Some(html) = theme_source_for_candidate(candidate, &key, templates, parts, layouts) {
            return Ok(TemplateResolution::ActiveTheme {
                target_key: key,
                html: html.to_owned(),
            });
        }
    }
    Ok(TemplateResolution::CoreFallback)
}

/// The `templates`, `parts`, `layouts` maps MUST come from the verified active
/// durable revision (storage::theme::load_active_theme_v3); raw untrusted theme
/// paths, without verification, must never be passed into this resolver.
pub async fn resolve_template_2axis(
    db: &SqlitePool,
    candidates: &[TemplateCandidate],
    templates: Option<&BTreeMap<String, String>>,
    parts: Option<&BTreeMap<String, String>>,
    layouts: Option<&BTreeMap<String, String>>,
) -> ApiResult<TemplateResolution> {
    let mut conn = db.acquire().await?;
    resolve_template_2axis_on_connection(&mut conn, candidates, templates, parts, layouts).await
}

pub async fn list_overrides(db: &SqlitePool) -> ApiResult<Vec<TemplateOverrideRecord>> {
    let rows: Vec<OverrideRow> = sqlx::query_as(
        "SELECT id, target_kind, canonical_target_key, target_json, ast_json, version,
                base_theme_id, base_revision_digest, updated_at
           FROM template_overrides
          ORDER BY canonical_target_key ASC",
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
    validate_scope_and_load_cpt_fields(&mut tx, scope).await?;
    let record = get_override_in_tx(&mut tx, scope).await?;
    tx.commit().await?;
    Ok(record)
}

async fn get_override_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    scope: &OverrideScope,
) -> ApiResult<Option<TemplateOverrideRecord>> {
    let candidate = candidate_for_legacy_scope(scope)?;
    let key = candidate.canonical_key().map_err(ApiError::bad_request)?;
    let row: Option<OverrideRow> = sqlx::query_as(
        "SELECT id, target_kind, canonical_target_key, target_json, ast_json, version,
                base_theme_id, base_revision_digest, updated_at
           FROM template_overrides WHERE canonical_target_key = ?",
    )
    .bind(key)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(row_to_record).transpose()
}

/// Compatibility reader for callers being retired with the v2 public routes.
pub async fn resolve_effective_override(
    db: &SqlitePool,
    role: OverrideRole,
    type_key: Option<&str>,
    entry_id: Option<i64>,
) -> ApiResult<Option<TemplateOverrideRecord>> {
    let scope = OverrideScope::new(
        role,
        type_key
            .filter(|_| role.allows_type_scope())
            .map(str::to_string),
        entry_id.filter(|_| role.allows_entry_scope()),
    )?;
    let mut candidates = vec![candidate_for_legacy_scope(&scope)?];
    if scope.scope_entry_id.is_some() {
        candidates.push(candidate_for_legacy_scope(&OverrideScope::new(
            role,
            scope.scope_type_key.clone(),
            None,
        )?)?);
    }
    if scope.scope_type_key.is_some() {
        candidates.push(candidate_for_legacy_scope(&OverrideScope::new(
            role, None, None,
        )?)?);
    }
    for candidate in candidates {
        if let Some(record) = get_typed_override(db, &candidate).await? {
            return Ok(Some(record));
        }
    }
    Ok(None)
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
        return Err(ApiError::not_found(
            "Không tìm thấy loại nội dung cho scope.",
        ));
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

fn template_kind_str(kind: TemplateCandidateKind) -> &'static str {
    use TemplateCandidateKind as K;
    match kind {
        K::Home => "home",
        K::Page => "page",
        K::PageLayout => "page_layout",
        K::Single => "single",
        K::SingleCustom => "single_custom",
        K::Archive => "archive",
        K::Taxonomy => "taxonomy",
        K::Singular => "singular",
        K::Index => "index",
        K::NotFound => "not_found",
        K::Part => "part",
    }
}

fn document_parts_and_slot_count(document: &BuilderDocument) -> ApiResult<(Vec<String>, usize)> {
    let mut parts = Vec::new();
    let mut slots = 0;
    for section in &document.sections {
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    match &element.content {
                        ElementContent::Known(KnownElement::ContentSlot(_)) => slots += 1,
                        ElementContent::Known(KnownElement::TemplatePart(props)) => {
                            parts.push(props.part_key.clone());
                        }
                        ElementContent::Unresolved(value)
                            if matches!(
                                value.get("type").and_then(serde_json::Value::as_str),
                                Some("core/content_slot" | "core/template_part")
                            ) =>
                        {
                            return Err(ApiError::bad_request(
                                "Malformed content slot or template part block.",
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    Ok((parts, slots))
}

fn validate_content_slots(target: &TemplateCandidate, slots: usize) -> ApiResult<()> {
    use TemplateCandidateKind as K;
    let valid = match target.kind {
        K::Home | K::Page | K::PageLayout | K::Single | K::SingleCustom | K::Singular => slots == 1,
        K::Index => slots <= 1,
        K::Archive | K::Taxonomy | K::NotFound | K::Part => slots == 0,
    };
    if valid {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "Invalid content slot count for template target.",
        ))
    }
}

/// Check site-owned part edges in the final post-save graph, not only the
/// submitted node. A missing part may be provided by the active theme; the
/// public renderer must enforce the same depth/fan-out budget for theme parts.
async fn validate_part_graph_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    target: &TemplateCandidate,
    submitted: &BuilderDocument,
) -> ApiResult<()> {
    let target_key = target.canonical_key().map_err(ApiError::bad_request)?;
    let (initial, _) = document_parts_and_slot_count(submitted)?;
    let mut stack = vec![(initial, 0_usize, vec![target_key.clone()])];
    let mut expansions = 0_usize;
    while let Some((parts, depth, path)) = stack.pop() {
        for part_key in parts {
            let part_candidate = TemplateCandidate {
                part_key: Some(part_key),
                ..candidate(TemplateCandidateKind::Part)
            };
            let key = part_candidate
                .canonical_key()
                .map_err(ApiError::bad_request)?;
            if path.iter().any(|ancestor| ancestor == &key) {
                return Err(ApiError::bad_request("Template part cycle detected."));
            }
            if depth >= 3 {
                return Err(ApiError::bad_request("Template part depth exceeds 3."));
            }
            expansions += 1;
            if expansions > 32 {
                return Err(ApiError::bad_request(
                    "Template parts exceed 32 expansions.",
                ));
            }
            let nested_doc = if key == target_key {
                Some(submitted.clone())
            } else {
                get_typed_override_on_connection(&mut *tx, &key)
                    .await?
                    .map(|record| record.document)
            };
            if let Some(nested_doc) = nested_doc {
                let (children, count) = document_parts_and_slot_count(&nested_doc)?;
                validate_content_slots(&part_candidate, count)?;
                let mut next_path = path.clone();
                next_path.push(key);
                stack.push((children, depth + 1, next_path));
            }
        }
    }
    Ok(())
}

async fn validate_typed_target_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    target: &TemplateCandidate,
) -> ApiResult<Option<Vec<String>>> {
    target.canonical_key().map_err(ApiError::bad_request)?;
    if let Some(kind) = target.content_type.as_deref() {
        let role = if target.kind == TemplateCandidateKind::Archive {
            OverrideRole::Archive
        } else {
            OverrideRole::Single
        };
        let scope = OverrideScope::new(role, Some(kind.to_owned()), target.entry_id)?;
        return validate_scope_and_load_cpt_fields(tx, &scope).await;
    }
    if let Some(term_id) = target.term_id {
        let exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM taxonomy_terms WHERE id = ? AND kind = ?)",
        )
        .bind(term_id)
        .bind(target.taxonomy.as_deref())
        .fetch_one(&mut **tx)
        .await?;
        if exists == 0 {
            return Err(ApiError::not_found(
                "Taxonomy term for template target not found.",
            ));
        }
    }
    Ok(None)
}

/// The same AST and route-binding validation is used by Save and read-only Preview.
async fn validate_typed_document_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    target: &TemplateCandidate,
    document: &BuilderDocument,
    prior: Option<&BuilderDocument>,
) -> ApiResult<HomeState> {
    let scoped_fields = validate_typed_target_in_tx(tx, target).await?;
    let state = builder::read_home_tx(tx).await?;
    document
        .validate_for_save_with_schemas(&state.unified_block_schemas()?, prior)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    // S02 stores synchronized Pattern references per content entry through
    // entry_pattern_refs. Shared template overrides have no such ownership or
    // public-media projection, so accepting a PatternRef here would silently
    // hide its markup on public pages. Keep these blocks entry-scoped.
    if !builder::pattern_ids(document).is_empty() {
        return Err(ApiError::bad_request(
            "Reusable Pattern chỉ hỗ trợ Builder của từng entry/Page.",
        ));
    }
    let route_role = match target.kind {
        TemplateCandidateKind::Single
        | TemplateCandidateKind::SingleCustom
        | TemplateCandidateKind::Singular => Some("single"),
        TemplateCandidateKind::Home
        | TemplateCandidateKind::Page
        | TemplateCandidateKind::PageLayout => Some("page"),
        _ => None,
    };
    document
        .validate_route_context_for_role(route_role, scoped_fields.as_deref())
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let (_, slots) = document_parts_and_slot_count(document)?;
    validate_content_slots(target, slots)?;
    builder::validate_media_refs_in_tx(&mut **tx, document).await?;
    Ok(state)
}

async fn refresh_override_media_refs_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    override_id: i64,
    document: &BuilderDocument,
) -> ApiResult<()> {
    let refs = builder::resolve_document_media_refs_in_conn(&mut **tx, document).await?;
    sqlx::query("DELETE FROM template_override_media_refs WHERE override_id = ?")
        .bind(override_id)
        .execute(&mut **tx)
        .await?;
    for (role, ids) in [
        ("image", refs.image_ids),
        ("video", refs.video_ids),
        ("poster", refs.poster_image_ids),
    ] {
        for media_id in ids {
            sqlx::query(
                "INSERT INTO template_override_media_refs (override_id, media_id, ref_role)
                 VALUES (?, ?, ?)",
            )
            .bind(override_id)
            .bind(media_id)
            .bind(role)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

/// Creates without an ETag, or updates only the exact typed target at If-Match
/// version. The AST, provenance and media projections commit atomically.
pub async fn save_typed_override(
    db: &SqlitePool,
    target: &TemplateCandidate,
    document: &BuilderDocument,
    expected_version: Option<i64>,
) -> ApiResult<TemplateOverrideRecord> {
    let key = target.canonical_key().map_err(ApiError::bad_request)?;
    check_preview_images(db, document).await?;
    let mut tx = db.begin().await?;
    let existing = get_typed_override_on_connection(&mut tx, &key).await?;
    match (existing.as_ref(), expected_version) {
        (Some(_), None) => {
            return Err(ApiError::precondition_required(
                "If-Match required for existing override.",
            ));
        }
        (Some(row), Some(version)) if row.version != version => {
            return Err(ApiError::precondition_failed("Template override changed."));
        }
        (None, Some(_)) => {
            return Err(ApiError::precondition_failed(
                "Template override no longer exists.",
            ));
        }
        _ => {}
    }
    validate_typed_document_in_tx(
        &mut tx,
        target,
        document,
        existing.as_ref().map(|row| &row.document),
    )
    .await?;
    validate_part_graph_in_tx(&mut tx, target, document).await?;

    let ast_json = String::from_utf8(
        document
            .to_json()
            .map_err(|e| ApiError::bad_request(e.to_string()))?,
    )
    .map_err(|_| ApiError::bad_request("Invalid Builder UTF-8 document."))?;
    let target_json = serde_json::to_string(target)?;
    let active: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT r.theme_id, r.revision_digest
           FROM theme_active_selection a
           LEFT JOIN theme_revisions r ON r.revision_id = a.revision_id
          WHERE a.id = 1",
    )
    .fetch_optional(&mut *tx)
    .await?;
    let (base_theme_id, base_revision_digest) = active.unwrap_or((None, None));

    if let Some(ref row) = existing {
        let affected = sqlx::query(
            "UPDATE template_overrides SET ast_json = ?, target_json = ?, version = version + 1,
                   base_theme_id = COALESCE(base_theme_id, ?),
                   base_revision_digest = COALESCE(base_revision_digest, ?),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
              WHERE id = ? AND version = ?",
        )
        .bind(&ast_json)
        .bind(&target_json)
        .bind(base_theme_id.as_deref())
        .bind(base_revision_digest.as_deref())
        .bind(row.id)
        .bind(row.version)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if affected != 1 {
            return Err(ApiError::precondition_failed(
                "Template override changed concurrently.",
            ));
        }
    } else {
        sqlx::query(
            "INSERT INTO template_overrides (
                target_kind, canonical_target_key, target_json, ast_json,
                base_theme_id, base_revision_digest, version
             ) VALUES (?, ?, ?, ?, ?, ?, 1)",
        )
        .bind(template_kind_str(target.kind))
        .bind(&key)
        .bind(&target_json)
        .bind(&ast_json)
        .bind(base_theme_id.as_deref())
        .bind(base_revision_digest.as_deref())
        .execute(&mut *tx)
        .await?;
    }
    let saved = get_typed_override_on_connection(&mut tx, &key)
        .await?
        .ok_or_else(|| ApiError::internal("Template override not readable after save."))?;
    refresh_override_media_refs_in_tx(&mut tx, saved.id, document).await?;
    tx.commit().await?;
    Ok(saved)
}

pub async fn reset_typed_override(
    db: &SqlitePool,
    override_id: i64,
    expected_version: Option<i64>,
    candidates: &[TemplateCandidate],
    templates: Option<&BTreeMap<String, String>>,
    parts: Option<&BTreeMap<String, String>>,
    layouts: Option<&BTreeMap<String, String>>,
) -> ApiResult<ResetTemplateOutcome> {
    let expected_version = expected_version
        .ok_or_else(|| ApiError::precondition_required("If-Match required for reset."))?;
    let mut tx = db.begin().await?;
    let row: Option<OverrideRow> = sqlx::query_as(
        "SELECT id, target_kind, canonical_target_key, target_json, ast_json, version,
                base_theme_id, base_revision_digest, updated_at
           FROM template_overrides WHERE id = ?",
    )
    .bind(override_id)
    .fetch_optional(&mut *tx)
    .await?;
    let existing = row
        .map(row_to_record)
        .transpose()?
        .ok_or_else(|| ApiError::not_found("Template override not found."))?;
    if existing.version != expected_version {
        return Err(ApiError::precondition_failed(
            "Template override changed before reset.",
        ));
    }
    if candidates
        .first()
        .and_then(|candidate| candidate.canonical_key().ok())
        .as_deref()
        != Some(existing.canonical_target_key.as_str())
    {
        return Err(ApiError::bad_request(
            "Reset candidate chain must begin with target.",
        ));
    }
    let affected = sqlx::query("DELETE FROM template_overrides WHERE id = ? AND version = ?")
        .bind(override_id)
        .bind(expected_version)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if affected != 1 {
        return Err(ApiError::precondition_failed(
            "Template override changed concurrently.",
        ));
    }
    let next = resolve_template_2axis_on_connection(&mut tx, candidates, templates, parts, layouts)
        .await?
        .effective_source();
    tx.commit().await?;
    Ok(ResetTemplateOutcome {
        reset_id: existing.id,
        canonical_target_key: existing.canonical_target_key,
        next_effective_source: next,
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplatePreviewEntry {
    pub type_key: String,
    pub entry_id: i64,
}

enum PreviewPartSource {
    Builder(BuilderDocument),
    Theme(String),
    Missing,
}

/// Only the manifest captured for this request can authorize theme anchor
/// expressions; no caller-provided preview values contribute permissions.
fn active_theme_url_policy(
    active: Option<&ActiveThemeV3>,
) -> ApiResult<template_sec::ThemeUrlPolicy> {
    match active {
        Some(active) => settings::theme_url_policy(&serde_json::to_string(&active.manifest)?)
            .map_err(ApiError::internal),
        None => Ok(settings::global_theme_url_policy()),
    }
}

async fn preview_part_graph_from_edges(
    tx: &mut Transaction<'_, Sqlite>,
    root_key: &str,
    edges: Vec<String>,
    active: Option<&ActiveThemeV3>,
    budget: &mut PreviewCompositionBudget,
) -> ApiResult<BTreeMap<String, PreviewPartSource>> {
    let url_policy = active_theme_url_policy(active)?;
    let mut pending = vec![(edges, 0_usize, vec![root_key.to_owned()])];
    let mut graph = BTreeMap::new();
    while let Some((edges, depth, path)) = pending.pop() {
        budget.stack = path;
        for part_key in edges {
            let target = TemplateCandidate {
                part_key: Some(part_key.clone()),
                ..candidate(TemplateCandidateKind::Part)
            };
            let key = target.canonical_key().map_err(ApiError::bad_request)?;
            if budget.stack.contains(&key) {
                return Err(ApiError::bad_request("Template part cycle detected."));
            }
            budget.expansions += 1;
            if depth >= 3 || budget.expansions > 32 {
                return Err(ApiError::bad_request(
                    "Template part expansion limit exceeded.",
                ));
            }
            if !graph.contains_key(&part_key) {
                let source = if let Some(row) = get_typed_override_on_connection(tx, &key).await? {
                    validate_typed_document_in_tx(tx, &target, &row.document, Some(&row.document))
                        .await?;
                    PreviewPartSource::Builder(row.document)
                } else if let Some(source) = active.and_then(|a| a.parts.get(&part_key)).cloned() {
                    PreviewPartSource::Theme(source)
                } else {
                    PreviewPartSource::Missing
                };
                graph.insert(part_key.clone(), source);
            }
            let children = match &graph[&part_key] {
                PreviewPartSource::Builder(doc) => document_parts_and_slot_count(doc)?.0,
                PreviewPartSource::Theme(source) => {
                    template_sec::preview_csec_part_keys_with_theme_urls(
                        &part_key,
                        source,
                        &url_policy,
                    )
                    .map_err(ApiError::bad_request)?
                }
                PreviewPartSource::Missing => Vec::new(),
            };
            let mut path = budget.stack.clone();
            path.push(key);
            pending.push((children, depth + 1, path));
        }
    }
    budget.stack.clear();
    Ok(graph)
}

async fn preview_part_graph(
    tx: &mut Transaction<'_, Sqlite>,
    target: &TemplateCandidate,
    document: &BuilderDocument,
    active: Option<&ActiveThemeV3>,
    budget: &mut PreviewCompositionBudget,
) -> ApiResult<BTreeMap<String, PreviewPartSource>> {
    let (edges, _) = document_parts_and_slot_count(document)?;
    let root_key = target.canonical_key().map_err(ApiError::bad_request)?;
    preview_part_graph_from_edges(tx, &root_key, edges, active, budget).await
}

fn part_scope_prefix(key: &str, expansion_idx: usize) -> String {
    match key {
        "header" => "hdr".to_owned(),
        "footer" => "ftr".to_owned(),
        _ => {
            let clean: String = key
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                        c
                    } else {
                        '_'
                    }
                })
                .take(24)
                .collect();
            if clean.is_empty() {
                format!("tp_{expansion_idx}")
            } else {
                format!("tp_{expansion_idx}_{clean}")
            }
        }
    }
}

struct PreviewRenderContext<'a> {
    state: &'a HomeState,
    graph: &'a BTreeMap<String, PreviewPartSource>,
    theme_context: &'a serde_json::Value,
    route: Option<(&'a str, &'a str)>,
    cache: &'a page::BlockTemplateCache,
    url_policy: &'a template_sec::ThemeUrlPolicy,
}

async fn render_preview_builder(
    tx: &mut Transaction<'_, Sqlite>,
    document: &BuilderDocument,
    slot: Option<&str>,
    ctx: &PreviewRenderContext<'_>,
    budget: &mut PreviewCompositionBudget,
    annotation_scope: RenderAnnotationScope,
    scope: &str,
) -> ApiResult<RenderedLayout> {
    let mut parts = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for key in document_parts_and_slot_count(document)?.0 {
        if parts.contains_key(&key) {
            continue;
        }
        if let Some(mut part) = Box::pin(render_preview_part(tx, &key, ctx, budget)).await? {
            diagnostics.append(&mut part.diagnostics);
            parts.insert(key, part.html);
        }
    }
    let bindings = builder_binding::resolve_in_tx_with_route(tx, document, true, ctx.route).await?;
    let lists = resolve_lists_in_tx(tx, document, &BTreeMap::new()).await?;
    let forms = resolve_contact_forms_in_tx(tx, document).await?;
    let empty_patterns = page::PatternResults::new();
    let mut rb = budget.as_render_budget();
    let mut rendered = page::render_builder_with_budget(
        ctx.state,
        document,
        true,
        &bindings,
        &lists,
        &forms,
        Some(&page::BuilderTemplateContext {
            content_html: slot,
            parts: &parts,
        }),
        &empty_patterns,
        false,
        annotation_scope,
        scope,
        &mut rb,
        ctx.cache,
    )?;
    budget.sync_from_render_budget(&rb);
    rendered.diagnostics.append(&mut diagnostics);
    Ok(rendered)
}

async fn render_preview_part(
    tx: &mut Transaction<'_, Sqlite>,
    key: &str,
    ctx: &PreviewRenderContext<'_>,
    budget: &mut PreviewCompositionBudget,
) -> ApiResult<Option<RenderedLayout>> {
    match ctx.graph.get(key) {
        Some(PreviewPartSource::Builder(doc)) => {
            let scope = part_scope_prefix(key, budget.expansions);
            Ok(Some(
                Box::pin(render_preview_builder(
                    tx,
                    doc,
                    None,
                    ctx,
                    budget,
                    RenderAnnotationScope::None,
                    &scope,
                ))
                .await?,
            ))
        }
        Some(PreviewPartSource::Theme(source)) => {
            // Flatten only contiguous Theme-to-Theme edges. Builder boundaries
            // use the server snapshot rather than the surrounding lexical scope.
            let mut theme_sources = BTreeMap::new();
            let mut boundaries = BTreeSet::new();
            let mut todo =
                template_sec::preview_csec_part_keys_with_theme_urls(key, source, ctx.url_policy)
                    .map_err(ApiError::bad_request)?;
            while let Some(child) = todo.pop() {
                match ctx.graph.get(&child) {
                    Some(PreviewPartSource::Theme(child_source)) => {
                        if !theme_sources.contains_key(&child) {
                            todo.extend(
                                template_sec::preview_csec_part_keys_with_theme_urls(
                                    &child,
                                    child_source,
                                    ctx.url_policy,
                                )
                                .map_err(ApiError::bad_request)?,
                            );
                            theme_sources.insert(child, child_source.clone());
                        }
                    }
                    _ => {
                        boundaries.insert(child);
                    }
                }
            }
            let mut trusted = BTreeMap::new();
            let mut diagnostics = Vec::new();
            for child in boundaries {
                if let Some(mut rendered) =
                    Box::pin(render_preview_part(tx, &child, ctx, budget)).await?
                {
                    trusted.insert(child, rendered.html);
                    diagnostics.append(&mut rendered.diagnostics);
                } else {
                    diagnostics.push(page::BuilderDiagnostic {
                        node_id: child.clone(),
                        code: "template_part_unavailable",
                    });
                    trusted.insert(child, String::new());
                }
            }
            let html = template_sec::render_preview_csec_fragment_with_theme_urls(
                key,
                source,
                template_sec::json_to_safe_value(ctx.theme_context),
                &theme_sources,
                &trusted,
                budget,
                ctx.url_policy,
            )
            .map_err(ApiError::bad_request)?;
            Ok(Some(RenderedLayout {
                html,
                diagnostics,
                has_active_contact_form: false,
            }))
        }
        _ => Ok(None),
    }
}

async fn resolve_preview_entry(
    tx: &mut Transaction<'_, Sqlite>,
    target: &TemplateCandidate,
    sample: Option<&TemplatePreviewEntry>,
    active: Option<&ActiveThemeV3>,
) -> ApiResult<Option<(entries::ContentEntryRecord, ContentTypeDefinition)>> {
    use TemplateCandidateKind as K;
    let selected = if let Some(sample) = sample {
        if sample.entry_id <= 0 || !valid_content_type_key(&sample.type_key) {
            return Err(ApiError::bad_request("Invalid preview entry."));
        }
        Some((sample.type_key.clone(), sample.entry_id))
    } else if target.kind == K::Single {
        match (&target.content_type, target.entry_id, &target.selector_slug) {
            (Some(t), Some(id), _) => Some((t.clone(), id)),
            (Some(t), None, Some(slug)) => {
                let id: i64 = sqlx::query_scalar(
                    "SELECT id FROM content_entries WHERE content_type_key = ? AND slug = ?",
                )
                .bind(t)
                .bind(slug)
                .fetch_optional(&mut **tx)
                .await?
                .ok_or_else(|| ApiError::not_found("Preview entry not found."))?;
                Some((t.clone(), id))
            }
            _ => None,
        }
    } else {
        None
    };
    let Some((type_key, id)) = selected else {
        return Ok(None);
    };
    if type_key == "pattern" {
        return Err(ApiError::not_found("Preview entry not found."));
    }
    let valid = match target.kind {
        K::Home | K::Page | K::PageLayout => type_key == "page",
        K::Single => {
            type_key != "page"
                && target.content_type.as_ref().is_none_or(|t| t == &type_key)
                && target.entry_id.is_none_or(|target_id| target_id == id)
        }
        K::SingleCustom => {
            type_key != "page"
                && active
                    .and_then(|a| {
                        a.manifest.templates.iter().find(|d| {
                            d.kind == K::SingleCustom && d.custom_key == target.custom_key
                        })
                    })
                    .is_none_or(|d| d.allowed_types.contains(&type_key))
        }
        K::Singular => type_key != "page",
        K::Index => true,
        _ => false,
    };
    if !valid {
        return Err(ApiError::bad_request(
            "Preview entry does not match template target.",
        ));
    }
    let mut entry = entries::get_entry_in_tx(tx, &type_key, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Preview entry not found."))?;
    if target.kind == K::Single
        && target
            .selector_slug
            .as_ref()
            .is_some_and(|s| s != &entry.slug)
    {
        return Err(ApiError::bad_request(
            "Preview entry slug does not match template target.",
        ));
    }
    let schema = content::load_effective_content_schema_in_tx(tx, &type_key)
        .await?
        .filter(|s| {
            s.base.status == ProviderStatus::Active
                && s.base.declaration_state == DeclarationState::Declared
        })
        .ok_or_else(|| ApiError::not_found("Preview entry type not available."))?;
    entries::filter_entry_values_for_role(&mut entry, &schema, FieldRole::Admin);
    Ok(Some((entry, schema.base.descriptor)))
}

pub async fn preview_typed_override(
    db: &SqlitePool,
    themes_root: &Path,
    actor: &SessionContext,
    target: &TemplateCandidate,
    document: &BuilderDocument,
    preview_entry: Option<&TemplatePreviewEntry>,
    expected_version: Option<i64>,
) -> ApiResult<RenderedLayout> {
    preview_typed_override_with_cache(
        db,
        themes_root,
        actor,
        target,
        document,
        preview_entry,
        expected_version,
        page::BlockTemplateCache::global(),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn preview_typed_override_with_cache(
    db: &SqlitePool,
    themes_root: &Path,
    actor: &SessionContext,
    target: &TemplateCandidate,
    document: &BuilderDocument,
    preview_entry: Option<&TemplatePreviewEntry>,
    expected_version: Option<i64>,
    cache: &page::BlockTemplateCache,
) -> ApiResult<RenderedLayout> {
    let key = target.canonical_key().map_err(ApiError::bad_request)?;
    let mut tx = db.begin().await?;
    let existing = get_typed_override_on_connection(&mut tx, &key).await?;
    if expected_version.is_some_and(|v| existing.as_ref().is_none_or(|r| r.version != v)) {
        return Err(ApiError::precondition_failed("Template override changed."));
    }
    let mut state = validate_typed_document_in_tx(
        &mut tx,
        target,
        document,
        existing.as_ref().map(|r| &r.document),
    )
    .await?;
    let active = theme::load_active_theme_v3_in_conn(&mut tx, themes_root).await?;
    let url_policy = active_theme_url_policy(active.as_ref())?;
    let mut budget = PreviewCompositionBudget::new();
    let graph = preview_part_graph(&mut tx, target, document, active.as_ref(), &mut budget).await?;
    let entry = resolve_preview_entry(&mut tx, target, preview_entry, active.as_ref()).await?;
    let general = settings::get_general_in_conn(&mut tx).await?;
    let site = SiteContext {
        title: &general.site_title,
        tagline: &general.tagline,
    };
    let manifest_json = active
        .as_ref()
        .map(|a| serde_json::to_string(&a.manifest))
        .transpose()?;
    let active_ref = active
        .as_ref()
        .zip(manifest_json.as_deref())
        .map(|(a, m)| (a.manifest.theme_id.as_str(), m));
    let options = settings::resolve_merged_public_options_with_overrides_in_conn(
        &mut tx, active_ref, None, None, true, None,
    )
    .await?;
    state.site_options = options.clone();
    let menus = if let Some(active) = &active {
        menu::resolve_slot_menus_in_tx(&mut tx, &active.manifest.theme_id).await?
    } else {
        BTreeMap::new()
    };
    let mut theme_context = serde_json::json!({ "site": site, "options": options, "theme_options": options, "menus": menus });
    let mut body_diagnostics = Vec::new();
    let mut route = None;
    let body = if let Some((record, definition)) = &entry {
        route = Some((record.content_type_key.as_str(), record.slug.as_str()));
        let view = content_render::authenticated_preview_view(record.clone(), definition);
        theme_context["entry"] = serde_json::to_value(&view)?;
        theme_context["content_type"] = serde_json::to_value(definition)?;
        let raw_body_has_blocks = record
            .values
            .get("body")
            .and_then(serde_json::Value::as_str)
            .is_some_and(opendash_core::builder_blocks::contains_blocks);
        if raw_body_has_blocks
            && definition.settings.supports(ContentSupport::Builder)
            && record.content_type_key != "pattern"
        {
            let mut rb = budget.as_render_budget();
            let res = builder::preview_saved_page_body_in_tx_with_options_and_budget(
                &mut tx,
                actor,
                &record.content_type_key,
                record.id,
                Some(&options),
                &mut rb,
                cache,
            )
            .await?;
            budget.sync_from_render_budget(&rb);
            if let Some(mut rendered) = res {
                body_diagnostics.append(&mut rendered.diagnostics);
                Some(rendered.html)
            } else {
                None
            }
        } else {
            None
        }
        .map(Ok)
        .unwrap_or_else(|| {
            let html = content_render::render_single_with_site_and_archive(
                definition, &view, None, &site, None,
            )?;
            crate::document::normalize_public_document(&html)
                .map(|doc| doc.body_fragment)
                .map_err(|_| ApiError::bad_request("Invalid preview body."))
        })?
    } else {
        "<div data-template-preview-placeholder>Preview content</div>".to_owned()
    };
    if body.len() > template_sec::MAX_RENDERED_HTML_BYTES {
        return Err(ApiError::bad_request("Preview output exceeds 512 KiB."));
    }
    let ctx = PreviewRenderContext {
        state: &state,
        graph: &graph,
        theme_context: &theme_context,
        route,
        cache,
        url_policy: &url_policy,
    };
    let root_scope = if target.kind == TemplateCandidateKind::Part {
        part_scope_prefix(target.part_key.as_deref().unwrap_or("part"), 0)
    } else {
        "ov".to_owned()
    };
    let mut rendered = render_preview_builder(
        &mut tx,
        document,
        Some(&body),
        &ctx,
        &mut budget,
        RenderAnnotationScope::None,
        &root_scope,
    )
    .await?;
    rendered.diagnostics.append(&mut body_diagnostics);
    tx.rollback().await?;
    Ok(rendered)
}

/// Kept temporarily for the old Builder endpoint callers until routes migrate.
pub async fn save_override_and_render(
    db: &SqlitePool,
    scope: &OverrideScope,
    document: &BuilderDocument,
    expected_version: Option<i64>,
) -> ApiResult<(TemplateOverrideRecord, RenderedLayout)> {
    let target = candidate_for_legacy_scope(scope)?;
    let saved = save_typed_override(db, &target, document, expected_version).await?;
    let rendered = render_override_for_public(db, &saved, None).await?;
    Ok((saved, rendered))
}

/// Kept temporarily for old Builder endpoints; v3 reset API uses the typed ID.
pub async fn reset_override(
    db: &SqlitePool,
    scope: &OverrideScope,
    expected_version: i64,
    confirmed: bool,
) -> ApiResult<()> {
    if !confirmed {
        return Err(ApiError::bad_request("Reset requires confirmation."));
    }
    let target = candidate_for_legacy_scope(scope)?;
    let existing = get_typed_override(db, &target)
        .await?
        .ok_or_else(|| ApiError::not_found("Template override not found."))?;
    reset_typed_override(
        db,
        existing.id,
        Some(expected_version),
        &[target],
        None,
        None,
        None,
    )
    .await?;
    Ok(())
}

pub async fn render_override_for_public(
    db: &SqlitePool,
    record: &TemplateOverrideRecord,
    route_entry: Option<(&str, &str)>,
) -> ApiResult<RenderedLayout> {
    render_override_for_public_with_context(db, record, route_entry, None, None).await
}

pub async fn render_override_for_public_with_context(
    db: &SqlitePool,
    record: &TemplateOverrideRecord,
    route_entry: Option<(&str, &str)>,
    content_html: Option<&str>,
    parts: Option<&BTreeMap<String, String>>,
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
    let empty_parts = BTreeMap::new();
    let context = page::BuilderTemplateContext {
        content_html,
        parts: parts.unwrap_or(&empty_parts),
    };
    page::render_builder_with_template_context(
        &state,
        &record.document,
        false,
        &bindings,
        &lists,
        &forms,
        Some(&context),
    )
}

/// The public S02 graph has one budget across Builder and C-SEC segments.
pub struct PublicCompositionBudget {
    pub render_budget: page::RenderBudget,
    expansions: usize,
}

impl Default for PublicCompositionBudget {
    fn default() -> Self {
        Self {
            render_budget: page::RenderBudget::new(),
            expansions: 0,
        }
    }
}

impl PublicCompositionBudget {
    pub fn with_render_budget(render_budget: page::RenderBudget) -> Self {
        Self {
            render_budget,
            expansions: 0,
        }
    }

    fn charge(&mut self, count: usize) -> ApiResult<()> {
        self.expansions = self
            .expansions
            .checked_add(count)
            .filter(|n| *n <= 32)
            .ok_or_else(|| ApiError::internal("Template parts exceed 32 expansions."))?;
        Ok(())
    }

    pub fn render_theme(
        &mut self,
        compiled: &template_sec::CompiledCsecComposition,
        context: &serde_json::Value,
        slot: Option<&str>,
    ) -> ApiResult<String> {
        self.charge(compiled.part_expansions())?;
        let html = compiled
            .render_with_remaining_fuel(
                template_sec::json_to_safe_value(context),
                slot,
                &mut self.render_budget.fuel_remaining,
            )
            .map_err(ApiError::internal)?;
        let own_bytes = html.len().saturating_sub(slot.map_or(0, str::len));
        self.render_budget
            .charge_html_bytes(own_bytes)
            .map_err(ApiError::internal)?;
        Ok(html)
    }
}

struct PublicBuilderContext<'a> {
    state: &'a HomeState,
    active: Option<&'a ActiveThemeV3>,
    theme_context: &'a serde_json::Value,
    route_entry: Option<(&'a str, &'a str)>,
    cache: &'a page::BlockTemplateCache,
    url_policy: &'a template_sec::ThemeUrlPolicy,
}

/// Check the full depth of a contiguous theme graph at its actual Builder
/// boundary; the compiler checks the same graph from its own lexical root.
fn check_theme_part_depth(
    key: &str,
    sources: &BTreeMap<String, String>,
    depth: usize,
    url_policy: &template_sec::ThemeUrlPolicy,
) -> ApiResult<()> {
    if depth > 3 {
        return Err(ApiError::internal("Template part depth exceeds 3."));
    }
    let source = sources
        .get(key)
        .ok_or_else(|| ApiError::internal("Theme part missing."))?;
    for child in template_sec::csec_part_keys_with_theme_urls(key, source, url_policy)
        .map_err(ApiError::internal)?
    {
        check_theme_part_depth(&child, sources, depth + 1, url_policy)?;
    }
    Ok(())
}

async fn render_public_part(
    tx: &mut Transaction<'_, Sqlite>,
    key: &str,
    depth: usize,
    stack: &[String],
    ctx: &PublicBuilderContext<'_>,
    budget: &mut PublicCompositionBudget,
) -> ApiResult<Option<RenderedLayout>> {
    let target = TemplateCandidate {
        part_key: Some(key.to_owned()),
        ..candidate(TemplateCandidateKind::Part)
    };
    let canonical = target.canonical_key().map_err(ApiError::internal)?;
    if stack.contains(&canonical) || depth > 3 {
        return Err(ApiError::internal("Template part cycle or depth limit."));
    }
    if let Some(row) = get_typed_override_on_connection(tx, &canonical).await? {
        let (_, slots) = document_parts_and_slot_count(&row.document)?;
        validate_content_slots(&target, slots)?;
        let mut next_stack = stack.to_vec();
        next_stack.push(canonical);
        let scope = part_scope_prefix(key, budget.expansions);
        return Ok(Some(
            Box::pin(render_public_builder(
                tx,
                &row.document,
                None,
                depth,
                &next_stack,
                ctx,
                budget,
                &scope,
            ))
            .await?,
        ));
    }
    let Some(active) = ctx.active else {
        return Ok(None);
    };
    let Some(source) = active.parts.get(key) else {
        return Ok(None);
    };
    let compiled = template_sec::validate_and_compile_csec_composition_with_theme_urls(
        &canonical,
        source,
        &active.parts,
        template_sec::CsecContentKind::NonEntry,
        ctx.url_policy,
    )
    .map_err(ApiError::internal)?;
    check_theme_part_depth(key, &active.parts, depth, ctx.url_policy)?;
    let html = budget.render_theme(&compiled, ctx.theme_context, None)?;
    Ok(Some(RenderedLayout {
        html,
        diagnostics: Vec::new(),
        has_active_contact_form: false,
    }))
}

#[allow(clippy::too_many_arguments)]
async fn render_public_builder(
    tx: &mut Transaction<'_, Sqlite>,
    document: &BuilderDocument,
    slot: Option<&str>,
    depth: usize,
    stack: &[String],
    ctx: &PublicBuilderContext<'_>,
    budget: &mut PublicCompositionBudget,
    scope: &str,
) -> ApiResult<RenderedLayout> {
    let mut parts = BTreeMap::new();
    let mut child_diagnostics = Vec::new();
    let mut child_forms = false;
    // Count every occurrence, including repeated references. A stored Builder
    // part wins before consulting the immutable active-theme source.
    for key in document_parts_and_slot_count(document)?.0 {
        budget.charge(1)?;
        if let Some(mut rendered) =
            Box::pin(render_public_part(tx, &key, depth + 1, stack, ctx, budget)).await?
        {
            child_forms |= rendered.has_active_contact_form;
            child_diagnostics.append(&mut rendered.diagnostics);
            parts.insert(key, rendered.html);
        }
    }
    let bindings =
        builder_binding::resolve_in_tx_with_route(tx, document, false, ctx.route_entry).await?;
    let lists = resolve_lists_in_tx(tx, document, &BTreeMap::new()).await?;
    let forms = resolve_contact_forms_in_tx(tx, document).await?;
    let empty_patterns = page::PatternResults::new();
    let mut rendered = page::render_builder_with_budget(
        ctx.state,
        document,
        false,
        &bindings,
        &lists,
        &forms,
        Some(&page::BuilderTemplateContext {
            content_html: slot,
            parts: &parts,
        }),
        &empty_patterns,
        false,
        RenderAnnotationScope::None,
        scope,
        &mut budget.render_budget,
        ctx.cache,
    )?;
    rendered.has_active_contact_form |= child_forms;
    rendered.diagnostics.append(&mut child_diagnostics);
    Ok(rendered)
}

#[allow(clippy::too_many_arguments)]
pub async fn render_resolved_public_builder(
    db: &SqlitePool,
    record: &TemplateOverrideRecord,
    active: Option<&ActiveThemeV3>,
    theme_context: &serde_json::Value,
    route_entry: Option<(&str, &str)>,
    slot: Option<&str>,
    budget: &mut PublicCompositionBudget,
) -> ApiResult<RenderedLayout> {
    render_resolved_public_builder_with_cache(
        db,
        record,
        active,
        theme_context,
        route_entry,
        slot,
        budget,
        page::BlockTemplateCache::global(),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn render_resolved_public_builder_with_cache(
    db: &SqlitePool,
    record: &TemplateOverrideRecord,
    active: Option<&ActiveThemeV3>,
    theme_context: &serde_json::Value,
    route_entry: Option<(&str, &str)>,
    slot: Option<&str>,
    budget: &mut PublicCompositionBudget,
    cache: &page::BlockTemplateCache,
) -> ApiResult<RenderedLayout> {
    let mut tx = db.begin().await?;
    let state = builder::read_home_tx(&mut tx).await?;
    let url_policy = active_theme_url_policy(active)?;
    let context = PublicBuilderContext {
        state: &state,
        active,
        theme_context,
        route_entry,
        cache,
        url_policy: &url_policy,
    };
    let stack = vec![record.canonical_target_key.clone()];
    let rendered = render_public_builder(
        &mut tx,
        &record.document,
        slot,
        0,
        &stack,
        &context,
        budget,
        "ov",
    )
    .await?;
    tx.commit().await?;
    Ok(rendered)
}

pub async fn resolve_and_render_header_footer(
    db: &SqlitePool,
    active_v3: Option<&ActiveThemeV3>,
    site_ctx: &SiteContext<'_>,
    theme_opts: &BTreeMap<String, serde_json::Value>,
    budget: &mut PublicCompositionBudget,
) -> ApiResult<(Option<String>, Option<String>, bool)> {
    resolve_and_render_header_footer_with_cache(
        db,
        active_v3,
        site_ctx,
        theme_opts,
        budget,
        page::BlockTemplateCache::global(),
    )
    .await
}

pub async fn resolve_and_render_header_footer_with_cache(
    db: &SqlitePool,
    active_v3: Option<&ActiveThemeV3>,
    site_ctx: &SiteContext<'_>,
    theme_opts: &BTreeMap<String, serde_json::Value>,
    budget: &mut PublicCompositionBudget,
    cache: &page::BlockTemplateCache,
) -> ApiResult<(Option<String>, Option<String>, bool)> {
    let menus = crate::storage::menu::resolve_active_slot_menus(db).await?;
    let part_context = serde_json::json!({
        "site": site_ctx, "options": theme_opts, "theme_options": theme_opts, "menus": menus,
    });
    let mut tx = db.begin().await?;
    let state = builder::read_home_tx(&mut tx).await?;
    let url_policy = active_theme_url_policy(active_v3)?;
    let context = PublicBuilderContext {
        state: &state,
        active: active_v3,
        theme_context: &part_context,
        route_entry: None,
        cache,
        url_policy: &url_policy,
    };
    let header = render_public_part(&mut tx, "header", 0, &[], &context, budget).await?;
    let footer = render_public_part(&mut tx, "footer", 0, &[], &context, budget).await?;
    let has_form = header.as_ref().is_some_and(|r| r.has_active_contact_form)
        || footer.as_ref().is_some_and(|r| r.has_active_contact_form);
    tx.commit().await?;
    Ok((header.map(|r| r.html), footer.map(|r| r.html), has_form))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransientStudioOverrides {
    #[serde(default)]
    pub general: Option<settings::GeneralSettingsInput>,
    #[serde(default)]
    pub global_options: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(default)]
    pub theme_options: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(default)]
    pub menu_slots: Option<BTreeMap<String, Option<i64>>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StudioTargetItem {
    pub canonical_target_key: String,
    pub label: String,
    pub group: &'static str,
    pub target: TemplateCandidate,
    pub override_id: Option<i64>,
    pub version: Option<i64>,
    pub effective_source: EffectiveSourceDescriptor,
    pub requires_content_slot: bool,
    pub forbids_content_slot: bool,
    pub has_theme_source: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StudioTargetsResponse {
    pub active_theme_id: Option<String>,
    pub active_theme_name: Option<String>,
    pub items: Vec<StudioTargetItem>,
}

fn target_requires_and_forbids_slot(kind: TemplateCandidateKind) -> (bool, bool) {
    use TemplateCandidateKind as K;
    let requires = matches!(
        kind,
        K::Home | K::Page | K::PageLayout | K::Single | K::SingleCustom | K::Singular
    );
    let forbids = matches!(kind, K::Archive | K::Taxonomy | K::NotFound | K::Part);
    (requires, forbids)
}

fn push_studio_target_item(
    items: &mut Vec<StudioTargetItem>,
    seen: &mut BTreeSet<String>,
    overrides_map: &BTreeMap<String, &TemplateOverrideRecord>,
    active: Option<&ActiveThemeV3>,
    target: TemplateCandidate,
    label: String,
    group: &'static str,
) -> ApiResult<()> {
    let key = target.canonical_key().map_err(ApiError::bad_request)?;
    if !seen.insert(key.clone()) {
        return Ok(());
    }
    let has_theme_source = theme_source_for_candidate(
        &target,
        &key,
        active.map(|a| &a.templates),
        active.map(|a| &a.parts),
        active.map(|a| &a.page_layouts),
    )
    .is_some();
    let existing = overrides_map.get(&key).copied();
    let effective_source = if let Some(record) = existing {
        TemplateResolution::DbOverride(record.clone()).effective_source()
    } else if has_theme_source {
        TemplateResolution::ActiveTheme {
            target_key: key.clone(),
            html: String::new(),
        }
        .effective_source()
    } else {
        TemplateResolution::CoreFallback.effective_source()
    };
    let (requires_content_slot, forbids_content_slot) =
        target_requires_and_forbids_slot(target.kind);
    items.push(StudioTargetItem {
        canonical_target_key: key,
        label,
        group,
        target,
        override_id: existing.map(|r| r.id),
        version: existing.map(|r| r.version),
        effective_source,
        requires_content_slot,
        forbids_content_slot,
        has_theme_source,
    });
    Ok(())
}

pub async fn list_studio_targets(
    db: &SqlitePool,
    themes_root: &Path,
) -> ApiResult<StudioTargetsResponse> {
    use TemplateCandidateKind as K;
    let active = theme::load_active_theme_v3(db, themes_root).await?;
    let overrides_list = list_overrides(db).await?;
    let overrides_map: BTreeMap<String, &TemplateOverrideRecord> = overrides_list
        .iter()
        .map(|row| (row.canonical_target_key.clone(), row))
        .collect();
    let content_types = content::list_content_types_with_meta(db).await?;

    let mut items = Vec::new();
    let mut seen = BTreeSet::new();

    for (part_key, label) in [("header", "Header"), ("footer", "Footer")] {
        let mut target = candidate(K::Part);
        target.part_key = Some(part_key.to_owned());
        push_studio_target_item(
            &mut items,
            &mut seen,
            &overrides_map,
            active.as_ref(),
            target,
            label.to_owned(),
            "parts",
        )?;
    }
    if let Some(ref theme) = active {
        for part_key in theme.manifest.parts.keys() {
            if part_key == "header" || part_key == "footer" {
                continue;
            }
            let mut target = candidate(K::Part);
            target.part_key = Some(part_key.clone());
            push_studio_target_item(
                &mut items,
                &mut seen,
                &overrides_map,
                active.as_ref(),
                target,
                format!("Part: {part_key}"),
                "parts",
            )?;
        }
    }

    for (kind, label) in [
        (K::Home, "Trang chủ (Home)"),
        (K::Page, "Trang mặc định (Page)"),
        (K::Single, "Chi tiết bài viết (Single)"),
        (K::Archive, "Danh sách lưu trữ (Archive)"),
        (K::Index, "Index dự phòng (Index)"),
    ] {
        push_studio_target_item(
            &mut items,
            &mut seen,
            &overrides_map,
            active.as_ref(),
            candidate(kind),
            label.to_owned(),
            "core",
        )?;
    }

    for ct in content_types {
        if ct.base.status != ProviderStatus::Active
            || ct.base.declaration_state != DeclarationState::Declared
        {
            continue;
        }
        let d = &ct.base.descriptor;
        if d.key == "pattern" || d.key == "section" || d.key == "page" || !d.settings.public {
            continue;
        }
        let mut single_target = candidate(K::Single);
        single_target.content_type = Some(d.key.clone());
        push_studio_target_item(
            &mut items,
            &mut seen,
            &overrides_map,
            active.as_ref(),
            single_target,
            format!("Chi tiết: {}", d.label),
            "content_types",
        )?;
        if d.settings.has_archive {
            let mut archive_target = candidate(K::Archive);
            archive_target.content_type = Some(d.key.clone());
            push_studio_target_item(
                &mut items,
                &mut seen,
                &overrides_map,
                active.as_ref(),
                archive_target,
                format!("Lưu trữ: {}", d.label),
                "content_types",
            )?;
        }
    }

    if let Some(ref theme) = active {
        for layout in &theme.manifest.page_layouts {
            let mut target = candidate(K::PageLayout);
            target.layout_key = Some(layout.key.clone());
            push_studio_target_item(
                &mut items,
                &mut seen,
                &overrides_map,
                active.as_ref(),
                target,
                format!("Bố cục trang: {}", layout.label),
                "page_layouts",
            )?;
        }
        for tpl in &theme.manifest.templates {
            if tpl.kind == K::SingleCustom
                && let Some(ck) = &tpl.custom_key
            {
                let mut target = candidate(K::SingleCustom);
                target.custom_key = Some(ck.clone());
                let label = tpl
                    .label
                    .clone()
                    .unwrap_or_else(|| format!("Mẫu tùy chỉnh: {ck}"));
                push_studio_target_item(
                    &mut items,
                    &mut seen,
                    &overrides_map,
                    active.as_ref(),
                    target,
                    label,
                    "custom_templates",
                )?;
            }
        }
    }

    for row in &overrides_list {
        push_studio_target_item(
            &mut items,
            &mut seen,
            &overrides_map,
            active.as_ref(),
            row.target.clone(),
            row.canonical_target_key.clone(),
            "overrides",
        )?;
    }

    Ok(StudioTargetsResponse {
        active_theme_id: active.as_ref().map(|a| a.manifest.theme_id.clone()),
        active_theme_name: active.as_ref().map(|a| a.manifest.name.clone()),
        items,
    })
}

async fn build_preview_context_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    active: Option<&ActiveThemeV3>,
    transient: Option<&TransientStudioOverrides>,
) -> ApiResult<(
    settings::GeneralSettings,
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, menu::MenuViewModel>,
)> {
    let mut general = settings::get_general_in_conn(&mut **tx).await?;
    if let Some(g) = transient.and_then(|t| t.general.as_ref()) {
        let valid = settings::GeneralSettingsInput {
            site_title: g.site_title.clone(),
            tagline: g.tagline.clone(),
            timezone: g.timezone.clone(),
        }
        .validate()
        .map_err(ApiError::unprocessable)?;
        if valid.timezone != general.timezone && !settings::is_current_timezone(&valid.timezone) {
            return Err(ApiError::unprocessable(
                "Múi giờ không có trong danh mục IANA tzdb 2026f.",
            ));
        }
        general.site_title = valid.site_title;
        general.tagline = valid.tagline;
        general.timezone = valid.timezone;
    }

    let manifest_json = active
        .as_ref()
        .map(|a| serde_json::to_string(&a.manifest))
        .transpose()?;
    let active_ref = active
        .as_ref()
        .zip(manifest_json.as_deref())
        .map(|(a, m)| (a.manifest.theme_id.as_str(), m));
    let options = settings::resolve_merged_public_options_with_overrides_in_conn(
        &mut **tx,
        active_ref,
        transient.and_then(|t| t.global_options.as_ref()),
        transient.and_then(|t| t.theme_options.as_ref()),
        true,
        None,
    )
    .await?;

    let mut menus = BTreeMap::new();
    if let Some(active) = active {
        let manifest_json = manifest_json.as_deref().unwrap_or("{}");
        let mut assignments =
            menu::list_slot_assignments_in_tx(tx, &active.manifest.theme_id).await?;
        if let Some(slot_ov) = transient.and_then(|t| t.menu_slots.as_ref()) {
            let declared = menu::extract_declared_menu_slots(manifest_json);
            for (slot_key, maybe_id) in slot_ov {
                if !declared.iter().any(|s| s == slot_key) {
                    return Err(ApiError::unprocessable(
                        "Vị trí menu không được khai báo trong theme.",
                    ));
                }
                match maybe_id {
                    None => {
                        assignments.remove(slot_key);
                    }
                    Some(id) => {
                        if *id <= 0 || menu::get_menu_in_tx(tx, *id).await?.is_none() {
                            return Err(ApiError::unprocessable("Menu không tồn tại."));
                        }
                        assignments.insert(slot_key.clone(), *id);
                    }
                }
            }
        }
        for (slot_key, menu_id) in assignments {
            if let Some(view) = menu::resolve_menu_view_in_tx(tx, menu_id).await? {
                menus.insert(slot_key, view);
            }
        }
    } else if transient
        .and_then(|t| t.menu_slots.as_ref())
        .is_some_and(|m| !m.is_empty())
    {
        return Err(ApiError::unprocessable(
            "Không có theme v3 đang kích hoạt để xem trước menu_slots.",
        ));
    }
    Ok((general, options, menus))
}

#[allow(clippy::too_many_arguments)]
async fn render_preview_named_part_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    key: &str,
    state: &HomeState,
    active: Option<&ActiveThemeV3>,
    theme_context: &serde_json::Value,
    route: Option<(&str, &str)>,
    budget: &mut PreviewCompositionBudget,
    cache: &page::BlockTemplateCache,
) -> ApiResult<Option<RenderedLayout>> {
    let url_policy = active_theme_url_policy(active)?;
    let part_target = TemplateCandidate {
        part_key: Some(key.to_owned()),
        ..candidate(TemplateCandidateKind::Part)
    };
    let canonical = part_target.canonical_key().map_err(ApiError::bad_request)?;
    if let Some(row) = get_typed_override_on_connection(&mut **tx, &canonical).await? {
        validate_typed_document_in_tx(tx, &part_target, &row.document, Some(&row.document)).await?;
        let graph = preview_part_graph(tx, &part_target, &row.document, active, budget).await?;
        let ctx = PreviewRenderContext {
            state,
            graph: &graph,
            theme_context,
            route,
            cache,
            url_policy: &url_policy,
        };
        let scope = part_scope_prefix(key, budget.expansions);
        let rendered = render_preview_builder(
            tx,
            &row.document,
            None,
            &ctx,
            budget,
            RenderAnnotationScope::None,
            &scope,
        )
        .await?;
        return Ok(Some(rendered));
    }
    if let Some(source) = active.and_then(|a| a.parts.get(key)) {
        let edges = template_sec::preview_csec_part_keys_with_theme_urls(key, source, &url_policy)
            .map_err(ApiError::bad_request)?;
        let mut graph =
            preview_part_graph_from_edges(tx, &canonical, edges, active, budget).await?;
        graph.insert(key.to_owned(), PreviewPartSource::Theme(source.clone()));
        let ctx = PreviewRenderContext {
            state,
            graph: &graph,
            theme_context,
            route,
            cache,
            url_policy: &url_policy,
        };
        return render_preview_part(tx, key, &ctx, budget).await;
    }
    Ok(None)
}

fn render_csec_template_for_preview(
    target_key: &str,
    html: &str,
    active: Option<&ActiveThemeV3>,
    theme_context: &serde_json::Value,
    slot_html: Option<&str>,
    budget: &mut PublicCompositionBudget,
) -> ApiResult<String> {
    use crate::template_sec::{
        CsecContentKind, validate_and_compile_csec_composition_with_theme_urls,
    };
    let kind = if target_key == "index" {
        CsecContentKind::Index
    } else if matches!(target_key, "home" | "page" | "singular" | "single")
        || target_key.starts_with("single:")
        || target_key.starts_with("single_custom:")
        || target_key.starts_with("page_layout:")
    {
        CsecContentKind::Entry
    } else {
        CsecContentKind::NonEntry
    };
    let empty_parts = BTreeMap::new();
    let parts = active.map(|v| &v.parts).unwrap_or(&empty_parts);
    let url_policy = active_theme_url_policy(active)?;
    let composition = validate_and_compile_csec_composition_with_theme_urls(
        target_key,
        html,
        parts,
        kind,
        &url_policy,
    )
    .map_err(ApiError::bad_request)?;
    budget.render_theme(&composition, theme_context, slot_html)
}

fn escape_preview_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub async fn preview_studio(
    db: &SqlitePool,
    themes_root: &Path,
    actor: &SessionContext,
    target: &TemplateCandidate,
    document: Option<&BuilderDocument>,
    preview_entry: Option<&TemplatePreviewEntry>,
    preview_annotations: bool,
    transient: Option<&TransientStudioOverrides>,
) -> ApiResult<RenderedLayout> {
    preview_studio_with_cache(
        db,
        themes_root,
        actor,
        target,
        document,
        preview_entry,
        preview_annotations,
        transient,
        page::BlockTemplateCache::global(),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn preview_studio_with_cache(
    db: &SqlitePool,
    themes_root: &Path,
    actor: &SessionContext,
    target: &TemplateCandidate,
    document: Option<&BuilderDocument>,
    preview_entry: Option<&TemplatePreviewEntry>,
    preview_annotations: bool,
    transient: Option<&TransientStudioOverrides>,
    cache: &page::BlockTemplateCache,
) -> ApiResult<RenderedLayout> {
    use TemplateCandidateKind as K;
    let key = target.canonical_key().map_err(ApiError::bad_request)?;
    let mut tx = db.begin().await?;
    validate_typed_target_in_tx(&mut tx, target).await?;
    let active = theme::load_active_theme_v3_in_conn(&mut tx, themes_root).await?;
    let url_policy = active_theme_url_policy(active.as_ref())?;
    let mut state = builder::read_home_tx(&mut tx).await?;
    let (general, options, menus) =
        build_preview_context_in_tx(&mut tx, active.as_ref(), transient).await?;
    state.site_options = options.clone();
    let site = SiteContext {
        title: &general.site_title,
        tagline: &general.tagline,
    };

    let default_home_entry: Option<TemplatePreviewEntry> =
        if preview_entry.is_none() && target.kind == K::Home {
            let reading = settings::get_reading_in_conn(&mut *tx).await?;
            match reading.homepage_page_id {
                Some(id) => {
                    let exists: Option<i64> = sqlx::query_scalar(
                        "SELECT id FROM content_entries WHERE id = ? AND content_type_key = 'page'",
                    )
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await?;
                    exists.map(|entry_id| TemplatePreviewEntry {
                        type_key: "page".to_owned(),
                        entry_id,
                    })
                }
                None => None,
            }
        } else {
            None
        };
    let effective_sample = preview_entry.or(default_home_entry.as_ref());
    let entry = resolve_preview_entry(&mut tx, target, effective_sample, active.as_ref()).await?;

    let mut theme_context = serde_json::json!({
        "site": site,
        "options": options,
        "theme_options": options,
        "menus": menus,
    });
    let mut diagnostics = Vec::new();
    let mut has_active_contact_form = false;
    let mut route = None;
    let mut shared_rb = page::RenderBudget::new();
    let body = if let Some((record, definition)) = &entry {
        route = Some((record.content_type_key.as_str(), record.slug.as_str()));
        let view = content_render::authenticated_preview_view(record.clone(), definition);
        theme_context["entry"] = serde_json::to_value(&view)?;
        theme_context["content_type"] = serde_json::to_value(definition)?;
        let raw_body_has_blocks = record
            .values
            .get("body")
            .and_then(serde_json::Value::as_str)
            .is_some_and(opendash_core::builder_blocks::contains_blocks);
        if raw_body_has_blocks
            && definition.settings.supports(ContentSupport::Builder)
            && record.content_type_key != "pattern"
        {
            if let Some(mut rendered) =
                builder::preview_saved_page_body_in_tx_with_options_and_budget(
                    &mut tx,
                    actor,
                    &record.content_type_key,
                    record.id,
                    Some(&options),
                    &mut shared_rb,
                    cache,
                )
                .await?
            {
                diagnostics.append(&mut rendered.diagnostics);
                Some(rendered.html)
            } else {
                None
            }
        } else {
            None
        }
        .map(Ok)
        .unwrap_or_else(|| {
            let html = content_render::render_single_with_site_and_archive(
                definition, &view, None, &site, None,
            )?;
            crate::document::normalize_public_document(&html)
                .map(|doc| doc.body_fragment)
                .map_err(|_| ApiError::bad_request("Invalid preview body."))
        })?
    } else {
        if matches!(
            target.kind,
            K::Home
                | K::Page
                | K::PageLayout
                | K::Single
                | K::SingleCustom
                | K::Singular
                | K::Index
        ) {
            theme_context["entry"] = serde_json::json!({
                "title": "Nội dung xem trước",
                "slug": "preview",
                "url": "/",
                "excerpt": "",
                "values": {},
                "fields": {},
            });
            theme_context["content_type"] = serde_json::json!({
                "key": target.content_type.as_deref().unwrap_or("page"),
                "label": "Trang",
            });
        } else if target.kind == K::Archive {
            let ct_key = target.content_type.as_deref().unwrap_or("post");
            theme_context["content_type"] = serde_json::json!({
                "key": ct_key,
                "label": ct_key,
            });
            theme_context["entries"] = serde_json::json!([]);
            theme_context["previous_url"] = serde_json::Value::Null;
            theme_context["next_url"] = serde_json::Value::Null;
        }
        "<div data-template-preview-placeholder>Preview content</div>".to_owned()
    };
    if body.len() > template_sec::MAX_RENDERED_HTML_BYTES {
        return Err(ApiError::bad_request("Preview output exceeds 512 KiB."));
    }

    let slot_fragment = crate::document::normalize_public_document(&body)
        .map(|parts| {
            let mut f = parts.authorized_styles.concat();
            f.push_str(&parts.body_fragment);
            f
        })
        .unwrap_or_else(|_| body.clone());

    let scope = if preview_annotations {
        RenderAnnotationScope::TemplateAll
    } else {
        RenderAnnotationScope::None
    };
    let mut preview_budget = PreviewCompositionBudget::with_render_budget(shared_rb.clone());
    let mut public_budget = PublicCompositionBudget::with_render_budget(shared_rb);
    let mut canvas_layout = target.kind == K::PageLayout;
    let main_scope = if target.kind == K::Part {
        part_scope_prefix(target.part_key.as_deref().unwrap_or("part"), 0)
    } else {
        "ov".to_owned()
    };

    let main_html = if let Some(doc) = document {
        let existing = get_typed_override_on_connection(&mut tx, &key).await?;
        validate_typed_document_in_tx(&mut tx, target, doc, existing.as_ref().map(|r| &r.document))
            .await?;
        let graph =
            preview_part_graph(&mut tx, target, doc, active.as_ref(), &mut preview_budget).await?;
        let ctx = PreviewRenderContext {
            state: &state,
            graph: &graph,
            theme_context: &theme_context,
            route,
            cache,
            url_policy: &url_policy,
        };
        let mut rendered = render_preview_builder(
            &mut tx,
            doc,
            Some(&slot_fragment),
            &ctx,
            &mut preview_budget,
            scope,
            &main_scope,
        )
        .await?;
        diagnostics.append(&mut rendered.diagnostics);
        has_active_contact_form |= rendered.has_active_contact_form;
        rendered.html
    } else {
        let chain = build_reset_candidate_chain_in_conn(&mut tx, target).await?;
        let resolved = resolve_template_2axis_on_connection(
            &mut tx,
            &chain,
            active.as_ref().map(|a| &a.templates),
            active.as_ref().map(|a| &a.parts),
            active.as_ref().map(|a| &a.page_layouts),
        )
        .await?;
        match resolved {
            TemplateResolution::DbOverride(record) => {
                canvas_layout = record.target.kind == K::PageLayout;
                let graph = preview_part_graph(
                    &mut tx,
                    &record.target,
                    &record.document,
                    active.as_ref(),
                    &mut preview_budget,
                )
                .await?;
                let ctx = PreviewRenderContext {
                    state: &state,
                    graph: &graph,
                    theme_context: &theme_context,
                    route,
                    cache,
                    url_policy: &url_policy,
                };
                let slot = if target.kind == K::Part {
                    None
                } else {
                    Some(slot_fragment.as_str())
                };
                let mut rendered = render_preview_builder(
                    &mut tx,
                    &record.document,
                    slot,
                    &ctx,
                    &mut preview_budget,
                    scope,
                    &main_scope,
                )
                .await?;
                diagnostics.append(&mut rendered.diagnostics);
                has_active_contact_form |= rendered.has_active_contact_form;
                rendered.html
            }
            TemplateResolution::ActiveTheme { target_key, html } => {
                if target_key.starts_with("page_layout:") {
                    canvas_layout = true;
                }
                let slot = if target.kind == K::Part {
                    None
                } else {
                    Some(slot_fragment.as_str())
                };
                let out = render_csec_template_for_preview(
                    &target_key,
                    &html,
                    active.as_ref(),
                    &theme_context,
                    slot,
                    &mut public_budget,
                )?;
                preview_budget.sync_from_render_budget(&public_budget.render_budget);
                out
            }
            TemplateResolution::CoreFallback => {
                if target.kind == K::Part {
                    String::new()
                } else {
                    format!(
                        "<main><h1>{}</h1><p>{}</p>{}</main>",
                        escape_preview_text(&general.site_title),
                        escape_preview_text(&general.tagline),
                        slot_fragment
                    )
                }
            }
        }
    };

    let (header_html, footer_html) = if target.kind != K::Part && !canvas_layout {
        let hdr = render_preview_named_part_in_tx(
            &mut tx,
            "header",
            &state,
            active.as_ref(),
            &theme_context,
            route,
            &mut preview_budget,
            cache,
        )
        .await?;
        let ftr = render_preview_named_part_in_tx(
            &mut tx,
            "footer",
            &state,
            active.as_ref(),
            &theme_context,
            route,
            &mut preview_budget,
            cache,
        )
        .await?;
        if let Some(ref h) = hdr {
            has_active_contact_form |= h.has_active_contact_form;
        }
        if let Some(ref f) = ftr {
            has_active_contact_form |= f.has_active_contact_form;
        }
        (hdr.map(|r| r.html), ftr.map(|r| r.html))
    } else {
        (None, None)
    };

    let seo = crate::seo::SeoView::for_noindex_page("vi", &general.site_title);
    let full_html = content_render::compose_document_with_seo(
        header_html.as_deref(),
        &main_html,
        footer_html.as_deref(),
        &seo,
    )?;
    tx.rollback().await?;
    Ok(RenderedLayout {
        has_active_contact_form,
        html: full_html,
        diagnostics,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn wrap_entry_body_in_preview_shell_in_tx_with_budget(
    tx: &mut Transaction<'_, Sqlite>,
    themes_root: &Path,
    actor_role: FieldRole,
    schema: &EffectiveContentSchema,
    entry_id: i64,
    page_layout_key: Option<&str>,
    custom_template_key: Option<&str>,
    entry_rendered: RenderedLayout,
    entry_render_budget: page::RenderBudget,
    cache: &page::BlockTemplateCache,
) -> ApiResult<RenderedLayout> {
    let mut record = entries::get_entry_in_tx(tx, &schema.base.descriptor.key, entry_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Content entry not found."))?;
    entries::filter_entry_values_for_role(&mut record, schema, actor_role);
    let definition = &schema.base.descriptor;
    let active = theme::load_active_theme_v3_in_conn(&mut **tx, themes_root).await?;
    let url_policy = active_theme_url_policy(active.as_ref())?;
    let mut state = builder::read_home_tx(tx).await?;
    let (general, options, menus) = build_preview_context_in_tx(tx, active.as_ref(), None).await?;
    state.site_options = options.clone();
    let site = SiteContext {
        title: &general.site_title,
        tagline: &general.tagline,
    };
    let view = content_render::authenticated_preview_view(record.clone(), definition);
    let theme_context = serde_json::json!({
        "site": site,
        "options": options,
        "theme_options": options,
        "menus": menus,
        "entry": view,
        "content_type": definition,
    });
    let ctx_target = if schema.base.descriptor.key == "page" {
        PublicTemplateContext::Page {
            page_layout_key: page_layout_key.map(ToOwned::to_owned),
        }
    } else {
        PublicTemplateContext::Single {
            content_type: schema.base.descriptor.key.clone(),
            entry_id: record.id,
            slug: record.slug.clone(),
            custom_template_key: custom_template_key.map(ToOwned::to_owned),
        }
    };
    let candidates = build_candidates(&ctx_target)?;
    let resolution = resolve_template_2axis_on_connection(
        &mut **tx,
        &candidates,
        active.as_ref().map(|a| &a.templates),
        active.as_ref().map(|a| &a.parts),
        active.as_ref().map(|a| &a.page_layouts),
    )
    .await?;
    let slot_fragment = crate::document::normalize_public_document(&entry_rendered.html)
        .map(|parts| {
            let mut f = parts.authorized_styles.concat();
            f.push_str(&parts.body_fragment);
            f
        })
        .unwrap_or_else(|_| entry_rendered.html.clone());

    let mut preview_budget =
        PreviewCompositionBudget::with_render_budget(entry_render_budget.clone());
    let mut public_budget = PublicCompositionBudget::with_render_budget(entry_render_budget);
    let route = Some((record.content_type_key.as_str(), record.slug.as_str()));
    let mut diagnostics = entry_rendered.diagnostics;
    let mut has_active_contact_form = entry_rendered.has_active_contact_form;
    let mut canvas_layout = false;

    let template_html = match resolution {
        TemplateResolution::DbOverride(override_rec) => {
            canvas_layout = override_rec.target.kind == TemplateCandidateKind::PageLayout;
            let graph = preview_part_graph(
                tx,
                &override_rec.target,
                &override_rec.document,
                active.as_ref(),
                &mut preview_budget,
            )
            .await?;
            let ctx = PreviewRenderContext {
                state: &state,
                graph: &graph,
                theme_context: &theme_context,
                route,
                cache,
                url_policy: &url_policy,
            };
            let mut tpl_rendered = render_preview_builder(
                tx,
                &override_rec.document,
                Some(&slot_fragment),
                &ctx,
                &mut preview_budget,
                RenderAnnotationScope::None,
                "ov",
            )
            .await?;
            diagnostics.append(&mut tpl_rendered.diagnostics);
            has_active_contact_form |= tpl_rendered.has_active_contact_form;
            tpl_rendered.html
        }
        TemplateResolution::ActiveTheme { target_key, html } => {
            if target_key.starts_with("page_layout:") {
                canvas_layout = true;
            }
            let out = render_csec_template_for_preview(
                &target_key,
                &html,
                active.as_ref(),
                &theme_context,
                Some(&slot_fragment),
                &mut public_budget,
            )?;
            preview_budget.sync_from_render_budget(&public_budget.render_budget);
            out
        }
        TemplateResolution::CoreFallback => slot_fragment,
    };

    let (header_html, footer_html) = if !canvas_layout {
        let hdr = render_preview_named_part_in_tx(
            tx,
            "header",
            &state,
            active.as_ref(),
            &theme_context,
            route,
            &mut preview_budget,
            cache,
        )
        .await?;
        let ftr = render_preview_named_part_in_tx(
            tx,
            "footer",
            &state,
            active.as_ref(),
            &theme_context,
            route,
            &mut preview_budget,
            cache,
        )
        .await?;
        (hdr.map(|r| r.html), ftr.map(|r| r.html))
    } else {
        (None, None)
    };

    let seo = crate::seo::SeoView::for_noindex_page("vi", &record.title);
    let full_html = content_render::compose_document_with_seo(
        header_html.as_deref(),
        &template_html,
        footer_html.as_deref(),
        &seo,
    )?;
    Ok(RenderedLayout {
        has_active_contact_form,
        html: full_html,
        diagnostics,
    })
}
