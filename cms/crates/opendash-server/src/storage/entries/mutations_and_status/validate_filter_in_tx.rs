use super::*;

/// Validate all three supported filter forms in the same read snapshot as
/// the query. Inactive, private or unassociated terms cannot be used to probe
/// published entry existence; Admin may still manage private taxonomies.
pub(crate) async fn validate_filter_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    filters: &EntryFilters,
    published_only: bool,
) -> ApiResult<()> {
    if filters.taxonomy_key.is_some() != filters.term_id.is_some()
        || (filters.taxonomy_key.is_some()
            && (filters.category_id.is_some() || filters.tag_id.is_some()))
    {
        return Err(ApiError::bad_request(
            "Trộn hoặc thiếu cặp taxonomy_key + term_id.",
        ));
    }
    if filters.include_descendants
        && filters.category_id.is_none()
        && filters.taxonomy_key.is_none()
    {
        return Err(ApiError::bad_request(
            "include_descendants cần taxonomy filter.",
        ));
    }
    let allowed =
        crate::storage::content::load_effective_taxonomies_in_tx(&mut **tx, type_key).await?;
    for (id, kind) in [
        (filters.category_id, "category"),
        (filters.tag_id, "tag"),
        (
            filters.term_id,
            filters.taxonomy_key.as_deref().unwrap_or(""),
        ),
    ] {
        if let Some(id) = id {
            if id <= 0 {
                return Err(ApiError::bad_request("Bộ lọc taxonomy không hợp lệ."));
            }
            let actual: Option<(String, i64, i64)> = sqlx::query_as(
                "SELECT t.kind,d.public,d.hierarchical FROM taxonomy_terms t
                  JOIN taxonomy_definitions d ON d.key=t.kind
                  JOIN site_locales sl ON sl.code=t.locale AND sl.deleted_at IS NULL
                 WHERE t.id=? AND d.provider_status='active'
                   AND d.declaration_state='declared'
                   AND (?=0 OR (sl.enabled=1 AND (sl.is_default=1 OR
                        (SELECT multilingual_enabled FROM site_locales_state WHERE id=1)=1)))",
            )
            .bind(id)
            .bind(i64::from(published_only))
            .fetch_optional(&mut **tx)
            .await?;
            if !actual.as_ref().is_some_and(|(actual, _, _)| actual == kind) {
                return Err(ApiError::bad_request(
                    "Term trong bộ lọc không tồn tại hoặc sai taxonomy.",
                ));
            }
            let (_, is_public, hierarchical) = actual.expect("checked term");
            if !allowed.iter().any(|a| a == kind) || (published_only && is_public == 0) {
                return Err(ApiError::bad_request(
                    "Taxonomy không được dùng cho CPT này.",
                ));
            }
            if filters.include_descendants
                && filters.taxonomy_key.as_deref() == Some(kind)
                && hierarchical == 0
            {
                return Err(ApiError::bad_request(
                    "Taxonomy không phân cấp không nhận descendants.",
                ));
            }
        }
    }
    Ok(())
}

pub fn filter_entry_values_for_role(
    record: &mut ContentEntryRecord,
    effective: &EffectiveContentSchema,
    caller_role: FieldRole,
) {
    record.values.retain(|key, _| {
        effective
            .effective_fields
            .get(key)
            .map(|fe| caller_role >= fe.definition.effective_acl().0)
            .unwrap_or(true)
    });
}

pub(in crate::storage) async fn verify_schema_token_in_tx(
    tx: &mut sqlx::SqliteConnection,
    type_key: &str,
    expected_schema_token: &str,
) -> ApiResult<EffectiveContentSchema> {
    let Some(eff) =
        crate::storage::content::load_effective_content_schema_in_tx(tx, type_key).await?
    else {
        return Err(ApiError::not_found(format!(
            "Không tìm thấy loại nội dung '{type_key}'."
        )));
    };
    if eff.schema_token != expected_schema_token {
        return Err(ApiError::conflict(format!(
            "schema_version_conflict: schema_token hiện tại là '{}', không khớp '{}'.",
            eff.schema_token, expected_schema_token
        )));
    }
    Ok(eff)
}

pub(in crate::storage) fn normalize_entry_timestamp(
    raw: &str,
    field_name: &str,
) -> ApiResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 32 || !trimmed.is_ascii() {
        return Err(ApiError::unprocessable(format!(
            "Định dạng thời gian {field_name} không hợp lệ (dùng YYYY-MM-DD HH:MM:SS hoặc ISO-8601)."
        )));
    }
    let normalized = trimmed.trim_end_matches('Z').trim_end_matches('z');
    let normalized = normalized.split('.').next().unwrap_or(normalized);
    let (date_part, time_part) = normalized
        .split_once('T')
        .or_else(|| normalized.split_once('t'))
        .or_else(|| normalized.split_once(' '))
        .ok_or_else(|| {
            ApiError::unprocessable(format!(
                "Định dạng thời gian {field_name} không hợp lệ (thiếu ngày hoặc giờ)."
            ))
        })?;
    let mut date_it = date_part.split('-');
    let year: u32 = date_it
        .next()
        .and_then(|s| (s.len() == 4).then(|| s.parse().ok()).flatten())
        .ok_or_else(|| ApiError::unprocessable(format!("Năm trong {field_name} không hợp lệ.")))?;
    let month: u32 = date_it
        .next()
        .and_then(|s| (s.len() == 2).then(|| s.parse().ok()).flatten())
        .ok_or_else(|| {
            ApiError::unprocessable(format!("Tháng trong {field_name} không hợp lệ."))
        })?;
    let day: u32 = date_it
        .next()
        .and_then(|s| (s.len() == 2).then(|| s.parse().ok()).flatten())
        .ok_or_else(|| ApiError::unprocessable(format!("Ngày trong {field_name} không hợp lệ.")))?;
    if date_it.next().is_some()
        || !(1970..=2100).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
    {
        return Err(ApiError::unprocessable(format!(
            "Ngày trong {field_name} nằm ngoài phạm vi hợp lệ."
        )));
    }
    let mut time_it = time_part.split(':');
    let hour: u32 = time_it
        .next()
        .and_then(|s| (s.len() == 2).then(|| s.parse().ok()).flatten())
        .ok_or_else(|| ApiError::unprocessable(format!("Giờ trong {field_name} không hợp lệ.")))?;
    let minute: u32 = time_it
        .next()
        .and_then(|s| (s.len() == 2).then(|| s.parse().ok()).flatten())
        .ok_or_else(|| ApiError::unprocessable(format!("Phút trong {field_name} không hợp lệ.")))?;
    let second: u32 = match time_it.next() {
        Some(s) => (s.len() == 2)
            .then(|| s.parse().ok())
            .flatten()
            .ok_or_else(|| {
                ApiError::unprocessable(format!("Giây trong {field_name} không hợp lệ."))
            })?,
        None => 0,
    };
    if time_it.next().is_some() || hour > 23 || minute > 59 || second > 59 {
        return Err(ApiError::unprocessable(format!(
            "Giờ phút giây trong {field_name} không hợp lệ."
        )));
    }
    Ok(format!(
        "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}"
    ))
}

#[derive(Debug, Clone, Serialize)]
pub struct CompositeEntrySaveResult {
    pub entry: ContentEntryRecord,
    pub custom_path: Option<String>,
    pub seo: Option<crate::seo::EntrySeoSnapshot>,
    pub schema_token: String,
}

pub struct CompositePreflightPlan {
    pub preflight_schema_token: String,
    pub locale: String,
    pub translation_group_id: Option<String>,
    pub effective: EffectiveContentSchema,
    pub effective_ct_def: ContentTypeDefinition,
    pub merged_input: ContentEntryInput,
    pub publication_status: Option<String>,
}

/// Check a selection under the same SQLite writer as the entry mutation. A
/// theme switch and entry Save cannot silently race and validate against two
/// different active revisions. Existing values are retained on omission even
/// if their original provider is no longer active.
pub(in crate::storage) async fn resolve_entry_template_patches_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    entry_id: Option<i64>,
    input: &CompositeEntrySaveInput,
) -> ApiResult<(Option<String>, Option<String>)> {
    let existing: (Option<String>, Option<String>) = match entry_id {
        Some(id) => sqlx::query_as(
            "SELECT page_layout_key, custom_template_key FROM content_entries WHERE id = ? AND content_type_key = ?",
        )
        .bind(id)
        .bind(type_key)
        .fetch_one(&mut **tx)
        .await?,
        None => (None, None),
    };

    let mut next = existing.clone();
    let mut needs_layout_validation = false;
    let mut needs_custom_validation = false;
    match &input.page_layout_key {
        PatchField::Omitted => {}
        PatchField::Clear => next.0 = None,
        PatchField::Set(key) => {
            needs_layout_validation = existing.0.as_deref() != Some(key.as_str());
            next.0 = Some(key.clone());
        }
    }
    match &input.custom_template_key {
        PatchField::Omitted => {}
        PatchField::Clear => next.1 = None,
        PatchField::Set(key) => {
            needs_custom_validation = existing.1.as_deref() != Some(key.as_str());
            next.1 = Some(key.clone());
        }
    }
    if !needs_layout_validation && !needs_custom_validation {
        return Ok(next);
    }
    let manifest_json: Option<String> = sqlx::query_scalar(
        "SELECT r.manifest_json FROM theme_active_selection s
           JOIN theme_revisions r ON r.revision_id = s.revision_id
          WHERE s.id = 1 AND r.source_format = 'v3'",
    )
    .fetch_optional(&mut **tx)
    .await?;
    let Some(manifest_json) = manifest_json else {
        return Err(ApiError::unprocessable(
            "Chưa có theme v3 hoạt động để chọn layout/template.",
        ));
    };
    let manifest: opendash_core::theme::ThemeV3Manifest = serde_json::from_str(&manifest_json)
        .map_err(|_| ApiError::internal("Manifest theme hoạt động không hợp lệ."))?;
    manifest.validate().map_err(ApiError::internal)?;
    if needs_layout_validation
        && !manifest
            .page_layouts
            .iter()
            .any(|layout| Some(layout.key.as_str()) == next.0.as_deref())
    {
        return Err(ApiError::unprocessable(
            "Page Layout không có trong theme đang hoạt động.",
        ));
    }
    if needs_custom_validation
        && !manifest.templates.iter().any(|tpl| {
            tpl.kind == opendash_core::theme::TemplateCandidateKind::SingleCustom
                && tpl.custom_key.as_deref() == next.1.as_deref()
                && tpl.allowed_types.iter().any(|allowed| allowed == type_key)
        })
    {
        return Err(ApiError::unprocessable(
            "Custom Single Template không dành cho loại nội dung này.",
        ));
    }
    Ok(next)
}

pub(in crate::storage) fn actor_field_role(role: auth::Role) -> FieldRole {
    match role {
        auth::Role::Admin => FieldRole::Admin,
        auth::Role::Editor => FieldRole::Editor,
    }
}

pub(in crate::storage) fn effective_as_ct_def(
    effective: &EffectiveContentSchema,
) -> ContentTypeDefinition {
    ContentTypeDefinition {
        key: effective.base.descriptor.key.clone(),
        label: effective.base.descriptor.label.clone(),
        settings: effective.base.descriptor.settings.clone(),
        fields: effective
            .effective_fields
            .values()
            .map(|fe| fe.definition.clone())
            .collect(),
    }
}

pub(in crate::storage) async fn resolve_primary_category_patch_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    entry_id: Option<i64>,
    patch: &PatchField<i64>,
    term_ids_provided: bool,
    effective_term_ids: &[i64],
) -> ApiResult<Option<i64>> {
    match *patch {
        PatchField::Clear => Ok(None),
        PatchField::Set(tid) => {
            if type_key != "post" {
                return Err(ApiError::bad_request(
                    "primary_category_term_id chỉ áp dụng cho bài viết (post).",
                ));
            }
            if tid <= 0 || !effective_term_ids.contains(&tid) {
                return Err(ApiError::bad_request(
                    "primary_category_term_id phải là một term đã được gán cho bài viết.",
                ));
            }
            let is_category: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM taxonomy_terms WHERE id = ? AND kind = 'category')",
            )
            .bind(tid)
            .fetch_one(&mut **tx)
            .await?;
            if is_category == 0 {
                return Err(ApiError::bad_request(
                    "primary_category_term_id phải thuộc taxonomy 'category'.",
                ));
            }
            Ok(Some(tid))
        }
        PatchField::Omitted => match entry_id {
            None => Ok(None),
            Some(id) => {
                if type_key != "post" {
                    return Ok(None);
                }
                let current_primary: Option<i64> = sqlx::query_scalar(
                    "SELECT primary_category_term_id FROM content_entries WHERE id = ?",
                )
                .bind(id)
                .fetch_one(&mut **tx)
                .await?;
                Ok(current_primary
                    .filter(|pid| !term_ids_provided || effective_term_ids.contains(pid)))
            }
        },
    }
}

pub(in crate::storage) async fn resolve_effective_rewrite_base_in_tx(
    conn: &mut sqlx::SqliteConnection,
    resource_kind: &str,
    resource_key: &str,
    site_bases: &BTreeMap<String, Option<String>>,
) -> ApiResult<String> {
    match site_bases.get(resource_key) {
        Some(Some(base)) if !base.trim().is_empty() => return Ok(base.trim().to_owned()),
        Some(None) => return Ok(resource_key.to_owned()),
        _ => {}
    }
    let provider_default: Option<String> = sqlx::query_scalar(
        "SELECT rewrite_base FROM provider_rewrite_defaults
          WHERE resource_kind = ? AND resource_key = ?
            AND provider_status = 'active' AND declaration_state = 'declared'",
    )
    .bind(resource_kind)
    .bind(resource_key)
    .fetch_optional(&mut *conn)
    .await?
    .flatten();
    Ok(provider_default
        .filter(|b| !b.trim().is_empty())
        .unwrap_or_else(|| resource_key.to_owned()))
}

pub(in crate::storage) async fn remap_term_ids_to_locale_in_conn(
    conn: &mut sqlx::SqliteConnection,
    type_key: &str,
    ids: &[i64],
    target_locale: &str,
) -> ApiResult<BTreeMap<i64, i64>> {
    let mut map = BTreeMap::new();
    if ids.is_empty() {
        return Ok(map);
    }
    let eligible = crate::storage::content::load_effective_taxonomies_in_tx(conn, type_key).await?;
    let ids_json = serde_json::to_string(ids)?;
    let rows: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT src.id, dst.id, dst.kind
           FROM taxonomy_terms src
           JOIN taxonomy_terms dst
             ON dst.kind = src.kind
            AND dst.translation_group_id = src.translation_group_id
            AND dst.locale = ?2
           JOIN taxonomy_definitions d ON d.key = dst.kind
          WHERE src.id IN (SELECT value FROM json_each(?1))
            AND d.provider_status = 'active'
            AND d.declaration_state = 'declared'",
    )
    .bind(ids_json)
    .bind(target_locale)
    .fetch_all(conn)
    .await?;
    for (src_id, dst_id, kind) in rows {
        if eligible.contains(&kind) {
            map.insert(src_id, dst_id);
        }
    }
    Ok(map)
}

pub(in crate::storage) async fn remap_list_term_ids_in_conn(
    conn: &mut sqlx::SqliteConnection,
    ids: &[i64],
    target_locale: &str,
) -> ApiResult<BTreeMap<i64, i64>> {
    let mut map = BTreeMap::new();
    if ids.is_empty() {
        return Ok(map);
    }
    let ids_json = serde_json::to_string(ids)?;
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT src.id, dst.id
           FROM taxonomy_terms src
           JOIN taxonomy_terms dst
             ON dst.kind = src.kind
            AND dst.translation_group_id = src.translation_group_id
            AND dst.locale = ?2
           JOIN taxonomy_definitions d ON d.key = dst.kind
          WHERE src.id IN (SELECT value FROM json_each(?1))
            AND d.public = 1
            AND d.provider_status = 'active'
            AND d.declaration_state = 'declared'",
    )
    .bind(ids_json)
    .bind(target_locale)
    .fetch_all(conn)
    .await?;
    for (src_id, dst_id) in rows {
        map.insert(src_id, dst_id);
    }
    Ok(map)
}

pub(in crate::storage) fn collect_builder_list_term_ids(
    doc: &opendash_core::builder::BuilderDocument,
) -> Vec<i64> {
    let mut out = Vec::new();
    for section in &doc.sections {
        for row in &section.children {
            for col in &row.children {
                for el in &col.children {
                    if let opendash_core::builder::ElementContent::Known(
                        opendash_core::builder::KnownElement::List(props),
                    ) = &el.content
                    {
                        if let Some(id) = props.category_id {
                            out.push(id);
                        }
                        if let Some(id) = props.tag_id {
                            out.push(id);
                        }
                        if let Some(id) = props.term_id {
                            out.push(id);
                        }
                    }
                }
            }
        }
    }
    out
}
