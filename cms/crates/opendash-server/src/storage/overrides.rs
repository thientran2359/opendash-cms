//! Site-owned typed templates. Candidate specificity wins before source priority:
//! for each candidate, site-owned Builder AST precedes active theme v3 HTML.
use std::collections::BTreeMap;

use opendash_core::{
    builder::{BuilderDocument, ElementContent, KnownElement},
    content::{ContentTypeDefinition, valid_content_type_key},
    theme::{TemplateCandidate, TemplateCandidateKind},
};
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

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
        theme::ActiveThemeV3,
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
    use TemplateCandidateKind as K;
    let mut candidates = vec![target.clone()];
    match target.kind {
        K::Home => {
            let layout: Option<String> = sqlx::query_scalar(
                "SELECT e.page_layout_key FROM site_reading_settings AS reading
                   JOIN content_entries AS e ON e.id = reading.homepage_page_id
                  WHERE reading.id=1 AND e.content_type_key='page'
                    AND e.publication_status='published'",
            )
            .fetch_optional(db)
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
                .fetch_optional(db)
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
                        .fetch_optional(db)
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

async fn resolve_template_2axis_on_connection(
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

async fn refresh_override_media_refs_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    override_id: i64,
    document: &BuilderDocument,
) -> ApiResult<()> {
    sqlx::query("DELETE FROM template_override_media_refs WHERE override_id = ?")
        .bind(override_id)
        .execute(&mut **tx)
        .await?;
    for (role, ids) in [
        ("image", builder::image_ids(document)),
        ("video", builder::video_ids(document)),
        ("poster", builder::poster_image_ids(document)),
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
    let scoped_fields = validate_typed_target_in_tx(&mut tx, target).await?;
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
    let state = builder::read_home_tx(&mut tx).await?;
    document
        .validate_for_save(
            &state.descriptors()?,
            existing.as_ref().map(|row| &row.document),
        )
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
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
    validate_part_graph_in_tx(&mut tx, target, document).await?;
    builder::validate_media_refs_in_tx(&mut tx, document).await?;

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
    let bindings =
        builder_binding::resolve_in_tx_with_route(&mut tx, document, true, route_ref).await?;
    let lists = resolve_lists_in_tx(&mut tx, document, &BTreeMap::new()).await?;
    let forms = resolve_contact_forms_in_tx(&mut tx, document).await?;
    tx.commit().await?;
    page::render_builder_with_lists_and_forms(&state, document, true, &bindings, &lists, &forms)
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
    let mut tx = db.begin().await?;
    let state: HomeState = builder::read_home_tx(&mut tx).await?;
    let bindings: BindingResults =
        builder_binding::resolve_in_tx_with_route(&mut tx, &record.document, false, route_entry)
            .await?;
    let lists: ListResults =
        resolve_lists_in_tx(&mut tx, &record.document, &BTreeMap::new()).await?;
    let forms = resolve_contact_forms_in_tx(&mut tx, &record.document).await?;
    tx.commit().await?;
    page::render_builder_with_lists_and_forms(
        &state,
        &record.document,
        false,
        &bindings,
        &lists,
        &forms,
    )
}

pub async fn resolve_and_render_header_footer(
    db: &SqlitePool,
    active_v3: Option<&ActiveThemeV3>,
    site_ctx: &SiteContext<'_>,
    theme_opts: &BTreeMap<String, serde_json::Value>,
    type_key: Option<&str>,
) -> ApiResult<(Option<String>, Option<String>)> {
    let menus = crate::storage::menu::resolve_active_slot_menus(db).await?;
    let header = match resolve_effective_override(db, OverrideRole::Header, type_key, None).await? {
        Some(record) => Some(render_override_for_public(db, &record, None).await?.html),
        None => active_v3
            .and_then(|v3| v3.parts.get("header"))
            .and_then(|t| {
                content_render::render_role_fragment_with_context(t, site_ctx, theme_opts, &menus)
            }),
    };
    let footer = match resolve_effective_override(db, OverrideRole::Footer, type_key, None).await? {
        Some(record) => Some(render_override_for_public(db, &record, None).await?.html),
        None => active_v3
            .and_then(|v3| v3.parts.get("footer"))
            .and_then(|t| {
                content_render::render_role_fragment_with_context(t, site_ctx, theme_opts, &menus)
            }),
    };
    Ok((header, footer))
}
