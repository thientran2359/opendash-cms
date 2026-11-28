use super::*;

pub(in crate::storage) fn candidate(kind: TemplateCandidateKind) -> TemplateCandidate {
    TemplateCandidate::new(kind)
}

pub(in crate::storage) async fn build_reset_candidate_chain_in_conn(
    conn: &mut SqliteConnection,
    target: &TemplateCandidate,
) -> ApiResult<Vec<TemplateCandidate>> {
    build_reset_candidate_chain_for_locale_in_conn(conn, target, None).await
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

pub(in crate::storage) type OverrideRow = (
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

pub(in crate::storage) fn row_to_record(row: OverrideRow) -> ApiResult<TemplateOverrideRecord> {
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
        TemplateCandidateKind::Index
        | TemplateCandidateKind::NotFound
        | TemplateCandidateKind::Search => "index",
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

pub(in crate::storage) async fn get_typed_override_on_connection(
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

pub(in crate::storage) fn theme_source_for_candidate<'a>(
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

pub(in crate::storage) async fn refresh_override_media_refs_in_tx(
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
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(saved)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplatePreviewEntry {
    pub type_key: String,
    pub entry_id: i64,
}

pub(in crate::storage) fn part_scope_prefix(key: &str, expansion_idx: usize) -> String {
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

pub(in crate::storage) fn actor_field_role(actor: &SessionContext) -> FieldRole {
    match actor.user.role {
        Role::Admin => FieldRole::Admin,
        Role::Editor => FieldRole::Editor,
    }
}

pub(in crate::storage) fn normalized_origin(public_origin: Option<&str>) -> &str {
    public_origin
        .map(|s| s.trim().trim_end_matches('/'))
        .unwrap_or("")
}

pub(in crate::storage) fn route_kind_for_candidate(kind: TemplateCandidateKind) -> &'static str {
    match kind {
        TemplateCandidateKind::Home => "home",
        TemplateCandidateKind::Page | TemplateCandidateKind::PageLayout => "page",
        TemplateCandidateKind::Single
        | TemplateCandidateKind::SingleCustom
        | TemplateCandidateKind::Singular => "single",
        TemplateCandidateKind::Archive => "archive",
        TemplateCandidateKind::Taxonomy => "taxonomy",
        TemplateCandidateKind::Index => "index",
        TemplateCandidateKind::NotFound => "not_found",
        TemplateCandidateKind::Search => "search",
        TemplateCandidateKind::Part => "part",
    }
}

/// Previews need the theme's styles, while scripts are reserved for the public
/// page. The source manifest and digest are already verified by theme storage.
pub(in crate::storage) fn preview_enqueued_styles(
    active: Option<&ActiveThemeV3>,
    route: EnqueueCondition,
) -> ResolvedEnqueuedAssets {
    let Some(active) = active else {
        return ResolvedEnqueuedAssets::default();
    };
    let mut resolved = resolve_enqueued_assets(&active.manifest, &active.revision_digest, route);
    resolved.head_scripts.clear();
    resolved.footer_scripts.clear();
    resolved
}

pub(in crate::storage) fn preview_condition(kind: TemplateCandidateKind) -> EnqueueCondition {
    match kind {
        TemplateCandidateKind::Home => EnqueueCondition::Home,
        TemplateCandidateKind::Page | TemplateCandidateKind::PageLayout => EnqueueCondition::Page,
        TemplateCandidateKind::Single
        | TemplateCandidateKind::SingleCustom
        | TemplateCandidateKind::Singular => EnqueueCondition::Single,
        TemplateCandidateKind::Archive => EnqueueCondition::Archive,
        TemplateCandidateKind::Taxonomy => EnqueueCondition::Taxonomy,
        TemplateCandidateKind::NotFound => EnqueueCondition::NotFound,
        TemplateCandidateKind::Search => EnqueueCondition::Search,
        TemplateCandidateKind::Index | TemplateCandidateKind::Part => EnqueueCondition::All,
    }
}

pub(in crate::storage) struct BuiltEntryPreviewContext {
    pub(in crate::storage) view: EntryView,
    pub(in crate::storage) archive_url: Option<String>,
    pub(in crate::storage) breadcrumbs: Vec<content_render::BreadcrumbItem>,
    pub(in crate::storage) theme_context: serde_json::Value,
}

#[derive(Debug, Default)]
pub struct PrecompiledPublicShell {
    pub header: Option<Arc<template_sec::CompiledCsecComposition>>,
    pub footer: Option<Arc<template_sec::CompiledCsecComposition>>,
}

pub(in crate::storage) fn csec_content_kind_for_target_key(
    target_key: &str,
) -> template_sec::CsecContentKind {
    if target_key == "index" {
        template_sec::CsecContentKind::Index
    } else if matches!(target_key, "home" | "page" | "singular" | "single")
        || target_key.starts_with("single:")
        || target_key.starts_with("single_custom:")
        || target_key.starts_with("page_layout:")
    {
        template_sec::CsecContentKind::Entry
    } else {
        template_sec::CsecContentKind::NonEntry
    }
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

pub(in crate::storage) fn push_studio_target_item(
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
