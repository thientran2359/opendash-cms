//! Studio Mode 1 taxonomy term sample resolver & archive/related/adjacent preview stubs.

use super::*;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(in crate::storage) struct PreviewArchiveSample {
    pub entries: Vec<EntryView>,
    pub total: i64,
}

async fn load_taxonomy_label_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    kind: &str,
) -> ApiResult<Option<String>> {
    sqlx::query_scalar(
        "SELECT label FROM taxonomy_definitions
          WHERE key = ? AND public = 1 AND provider_status = 'active'
            AND declaration_state = 'declared'",
    )
    .bind(kind)
    .fetch_optional(&mut **tx)
    .await
    .map_err(Into::into)
}

pub(in crate::storage) async fn resolve_preview_term_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    target: &TemplateCandidate,
    sample: Option<&TemplatePreviewTerm>,
    preview_locale: &str,
    mut richtext_nonces: Option<&mut template_sec::RichTextNonceTable>,
) -> ApiResult<Option<(content_render::PublicTermDetailView, String)>> {
    use TemplateCandidateKind as K;

    if let Some(sample) = sample {
        if target.kind != K::Taxonomy {
            return Err(ApiError::bad_request(
                "Term xem trước không khớp target template.",
            ));
        }
        let tax_key = sample.taxonomy_key.trim();
        if sample.term_id <= 0 || !opendash_core::content::valid_content_type_key(tax_key) {
            return Err(ApiError::bad_request(
                "Term xem trước không khớp target template.",
            ));
        }
        if target
            .taxonomy
            .as_deref()
            .is_some_and(|expected| expected != tax_key)
        {
            return Err(ApiError::bad_request(
                "Term xem trước không khớp target template.",
            ));
        }
        if target
            .term_id
            .is_some_and(|expected_id| expected_id != sample.term_id)
        {
            return Err(ApiError::bad_request(
                "Term xem trước không khớp target template.",
            ));
        }
        let row: Option<(i64, String, String, String, String)> = sqlx::query_as(
            "SELECT t.id, t.kind, t.slug, t.locale, d.label
               FROM taxonomy_terms t
               JOIN taxonomy_definitions d ON d.key = t.kind
              WHERE t.id = ?
                AND d.public = 1
                AND d.provider_status = 'active'
                AND d.declaration_state = 'declared'",
        )
        .bind(sample.term_id)
        .fetch_optional(&mut **tx)
        .await?;
        let Some((term_id, kind, slug, locale, taxonomy_label)) = row else {
            return Err(ApiError::not_found(
                "Không tìm thấy term phân loại để xem trước.",
            ));
        };
        if kind != tax_key
            || target
                .selector_slug
                .as_deref()
                .is_some_and(|expected_slug| expected_slug != slug)
        {
            return Err(ApiError::bad_request(
                "Term xem trước không khớp target template.",
            ));
        }
        if locale != preview_locale {
            return Err(ApiError::bad_request(
                "Term xem trước không khớp ngôn ngữ đang chọn.",
            ));
        }
        let detail = public_content::load_public_term_detail_in_tx(
            tx,
            term_id,
            true,
            richtext_nonces.as_deref_mut(),
        )
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy term phân loại để xem trước."))?;
        return Ok(Some((detail, taxonomy_label)));
    }

    if target.kind != K::Taxonomy {
        return Ok(None);
    }

    let term_kind = target.taxonomy.as_deref().unwrap_or("category");
    let Some(taxonomy_label) = load_taxonomy_label_in_tx(tx, term_kind).await? else {
        return Ok(None);
    };

    let resolved_id = if let Some(explicit_id) = target.term_id {
        let row: Option<(i64, String, String)> = sqlx::query_as(
            "SELECT t.id, t.kind, t.locale FROM taxonomy_terms t WHERE t.id = ?",
        )
        .bind(explicit_id)
        .fetch_optional(&mut **tx)
        .await?;
        let Some((tid, kind, locale)) = row else {
            return Err(ApiError::not_found(
                "Không tìm thấy term phân loại để xem trước.",
            ));
        };
        if kind != term_kind {
            return Err(ApiError::bad_request(
                "Term xem trước không khớp target template.",
            ));
        }
        if locale != preview_locale {
            return Err(ApiError::bad_request(
                "Term xem trước không khớp ngôn ngữ đang chọn.",
            ));
        }
        Some(tid)
    } else if let Some(selector_slug) = target.selector_slug.as_deref() {
        let tid: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM taxonomy_terms WHERE kind = ? AND slug = ? AND locale = ?",
        )
        .bind(term_kind)
        .bind(selector_slug)
        .bind(preview_locale)
        .fetch_optional(&mut **tx)
        .await?;
        if tid.is_none() {
            return Err(ApiError::not_found(
                "Không tìm thấy term phân loại để xem trước.",
            ));
        }
        tid
    } else {
        sqlx::query_scalar(
            "SELECT id FROM taxonomy_terms
              WHERE kind = ? AND locale = ?
              ORDER BY id DESC
              LIMIT 1",
        )
        .bind(term_kind)
        .bind(preview_locale)
        .fetch_optional(&mut **tx)
        .await?
    };

    let Some(tid) = resolved_id else {
        return Ok(None);
    };
    let detail = public_content::load_public_term_detail_in_tx(tx, tid, true, richtext_nonces)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy term phân loại để xem trước."))?;
    Ok(Some((detail, taxonomy_label)))
}

pub(in crate::storage) fn build_placeholder_preview_entry(type_key: &str) -> EntryView {
    let mut terms = BTreeMap::new();
    terms.insert("category".to_owned(), Vec::new());
    terms.insert("tag".to_owned(), Vec::new());
    EntryView {
        id: 0,
        type_key: type_key.to_owned(),
        title: "Nội dung xem trước".to_owned(),
        slug: "preview".to_owned(),
        url: Some("/".to_owned()),
        parent: None,
        menu_order: 0,
        created_at: String::new(),
        updated_at: String::new(),
        published_at: None,
        excerpt: String::new(),
        reading_time_minutes: 0,
        fields: BTreeMap::new(),
        field_list: Vec::new(),
        cover_url: None,
        cover_media: None,
        cover: None,
        richtext_html: BTreeMap::new(),
        terms,
        all_terms: Vec::new(),
        primary_category: None,
        ext: BTreeMap::new(),
        theme_ext: BTreeMap::new(),
        snippet_segments: Vec::new(),
        type_label: String::new(),
    }
}

#[allow(dead_code)]
pub(in crate::storage) fn build_preview_entry_stub(
    type_key: &str,
    index: usize,
    locale: &str,
) -> EntryView {
    let mut entry = build_placeholder_preview_entry(type_key);
    let is_vi = locale.trim().to_ascii_lowercase().starts_with("vi") || locale.trim().is_empty();
    entry.id = -(index as i64 + 1);
    entry.slug = format!("sample-item-{}", index + 1);
    entry.title = if is_vi {
        format!("Bài viết mẫu #{}", index + 1)
    } else {
        format!("Sample post #{}", index + 1)
    };
    entry.excerpt = if is_vi {
        "Đoạn mô tả xem trước cho thẻ bài viết trong lưới Template Studio.".to_owned()
    } else {
        "Preview summary text for template loop cards in Studio.".to_owned()
    };
    entry.reading_time_minutes = 3;
    entry.published_at = Some("2026-11-26T09:00:00Z".to_owned());
    entry
}

#[allow(dead_code)]
pub(in crate::storage) fn build_preview_related_stubs(
    limit: usize,
    locale: &str,
) -> Vec<EntryView> {
    (0..limit.min(3))
        .map(|i| build_preview_entry_stub("post", i, locale))
        .collect()
}

#[allow(dead_code)]
pub(in crate::storage) fn build_preview_adjacent_stub(
    locale: &str,
) -> content_render::AdjacentEntriesView {
    let is_vi = locale.trim().to_ascii_lowercase().starts_with("vi") || locale.trim().is_empty();
    content_render::AdjacentEntriesView {
        prev: Some(content_render::AdjacentEntryItem {
            id: -1,
            title: if is_vi {
                "Bài viết trước (Mẫu)".to_owned()
            } else {
                "Previous post (Sample)".to_owned()
            },
            slug: "sample-prev".to_owned(),
            url: Some("/sample-prev".to_owned()),
            cover_url: None,
            cover: None,
            published_at: Some("2026-11-25T09:00:00Z".to_owned()),
        }),
        next: Some(content_render::AdjacentEntryItem {
            id: -2,
            title: if is_vi {
                "Bài viết kế tiếp (Mẫu)".to_owned()
            } else {
                "Next post (Sample)".to_owned()
            },
            slug: "sample-next".to_owned(),
            url: Some("/sample-next".to_owned()),
            cover_url: None,
            cover: None,
            published_at: Some("2026-11-27T09:00:00Z".to_owned()),
        }),
    }
}

#[allow(dead_code)]
pub(in crate::storage) async fn load_preview_archive_samples_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    target: &TemplateCandidate,
    resolved_term_id: Option<i64>,
    effective_locale: &str,
    actor_role: FieldRole,
    projection: &opendash_core::builder::LoopProjection,
    richtext_nonces: Option<&mut template_sec::RichTextNonceTable>,
) -> ApiResult<PreviewArchiveSample> {
    use TemplateCandidateKind as K;

    let (records, total): (Vec<entries::ContentEntryRecord>, i64) = match target.kind {
        K::Taxonomy => {
            if let Some(tid) = resolved_term_id {
                let page: Vec<public_content::HydratedEntrySqlRow> = sqlx::query_as(
                    "SELECT e.id, e.content_type_key, e.slug, e.title, e.values_json,
                            e.publication_status, e.version, e.created_at, e.updated_at,
                            e.published_at, e.primary_category_term_id, e.locale,
                            e.parent_id, e.menu_order
                       FROM content_entries e
                       JOIN content_types t ON t.key = e.content_type_key
                      WHERE e.locale = ?
                        AND e.trashed_at IS NULL
                        AND e.publication_status = 'published'
                        AND e.content_type_key NOT IN ('pattern', 'section')
                        AND t.provider_status = 'active'
                        AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
                        AND EXISTS (
                            SELECT 1 FROM entry_terms et
                             WHERE et.entry_id = e.id AND et.term_id = ?
                        )
                      ORDER BY COALESCE(e.published_at, e.updated_at) DESC, e.id DESC
                      LIMIT 8",
                )
                .bind(effective_locale)
                .bind(tid)
                .fetch_all(&mut **tx)
                .await?;
                let decoded = public_content::decode_hydrated_entry_rows(page)?;
                let cnt = decoded.len() as i64;
                (decoded, cnt)
            } else {
                (Vec::new(), 0)
            }
        }
        K::Archive | K::Index => {
            let ct_key = target.content_type.as_deref().unwrap_or("post");
            let page: Vec<public_content::HydratedEntrySqlRow> = sqlx::query_as(
                "SELECT e.id, e.content_type_key, e.slug, e.title, e.values_json,
                        e.publication_status, e.version, e.created_at, e.updated_at,
                        e.published_at, e.primary_category_term_id, e.locale,
                        e.parent_id, e.menu_order
                   FROM content_entries e
                   JOIN content_types t ON t.key = e.content_type_key
                  WHERE e.content_type_key = ?
                    AND e.locale = ?
                    AND e.trashed_at IS NULL
                    AND e.publication_status = 'published'
                    AND t.provider_status = 'active'
                    AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
                  ORDER BY COALESCE(e.published_at, e.updated_at) DESC, e.id DESC
                  LIMIT 8",
            )
            .bind(ct_key)
            .bind(effective_locale)
            .fetch_all(&mut **tx)
            .await?;
            let decoded = public_content::decode_hydrated_entry_rows(page)?;
            let cnt = decoded.len() as i64;
            (decoded, cnt)
        }
        _ => (Vec::new(), 0),
    };

    if records.is_empty() {
        let ct_key = target.content_type.as_deref().unwrap_or("post");
        let stubs = (0..3)
            .map(|i| build_preview_entry_stub(ct_key, i, effective_locale))
            .collect();
        return Ok(PreviewArchiveSample {
            entries: stubs,
            total: 3,
        });
    }

    let entries = public_content::hydrate_entry_views_projected_in_conn(
        &mut **tx,
        records,
        Some(effective_locale),
        true,
        actor_role,
        richtext_nonces,
        projection,
    )
    .await?;
    Ok(PreviewArchiveSample { entries, total })
}
