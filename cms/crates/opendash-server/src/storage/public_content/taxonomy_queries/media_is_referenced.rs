use super::*;

/// Images become public only while referenced by an image/gallery custom field
/// of an existing CPT entry. A private unreferenced upload remains 404 publicly.
/// JSON paths are derived from validated field keys persisted in CPT schemas.
pub async fn media_is_referenced(db: &SqlitePool, id: &str) -> ApiResult<bool> {
    let result: i64 = sqlx::query_scalar(
        r#"SELECT EXISTS (
           SELECT 1
           FROM site_global_option_media_refs AS gom
           WHERE gom.media_id = ?
         ) OR EXISTS (
           SELECT 1
           FROM seo_public_media_refs AS sm
           JOIN content_entries AS e ON sm.source_kind = 'entry_seo' AND e.id = sm.source_id
           JOIN site_locales AS sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
           JOIN content_types AS t ON t.key = e.content_type_key
           WHERE sm.media_id = ?
             AND e.trashed_at IS NULL
             AND e.publication_status = 'published'
             AND t.provider_status = 'active'
             AND e.content_type_key != 'pattern'
             AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
         ) OR EXISTS (
           SELECT 1
           FROM site_home_seo AS h
           JOIN site_locales AS sl ON sl.code = h.locale
           WHERE h.og_image_id = ? AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
         ) OR EXISTS (
           SELECT 1
           FROM template_override_media_refs AS tom
           JOIN template_overrides AS ov ON ov.id = tom.override_id
           WHERE tom.media_id = ?
             AND (
               ov.target_kind IN ('index', 'not_found', 'search', 'singular', 'part')
               OR (ov.target_kind = 'home' AND EXISTS (
                 SELECT 1 FROM site_reading_settings AS reading
                 JOIN site_locales AS sl ON sl.code = reading.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                 JOIN content_entries AS e ON e.id = reading.homepage_page_id
                 WHERE e.content_type_key = 'page'
                   AND e.locale = reading.locale
                   AND e.trashed_at IS NULL
                   AND e.publication_status = 'published'
               ))
               OR (ov.target_kind = 'page' AND EXISTS (
                 SELECT 1 FROM content_entries AS e
                 JOIN site_locales AS sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                 WHERE e.content_type_key = 'page' AND e.trashed_at IS NULL AND e.publication_status = 'published'
               ))
               OR (ov.target_kind = 'page_layout' AND EXISTS (
                 SELECT 1 FROM content_entries AS e
                 JOIN site_locales AS sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                 WHERE e.content_type_key = 'page' AND e.trashed_at IS NULL AND e.publication_status = 'published'
                   AND e.page_layout_key = json_extract(ov.target_json, '$.layout_key')
               ))
               OR (ov.target_kind = 'single_custom' AND EXISTS (
                 SELECT 1 FROM content_entries AS e
                 JOIN site_locales AS sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                 JOIN content_types AS t ON t.key = e.content_type_key
                 WHERE e.custom_template_key = json_extract(ov.target_json, '$.custom_key')
                   AND e.trashed_at IS NULL
                   AND e.publication_status = 'published'
                   AND t.provider_status = 'active' AND t.key != 'pattern' AND t.key != 'page'
                   AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
               ))
               OR (ov.target_kind = 'single' AND EXISTS (
                 SELECT 1 FROM content_entries AS e
                 JOIN site_locales AS sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                 JOIN content_types AS t ON t.key = e.content_type_key
                 WHERE e.trashed_at IS NULL
                   AND e.publication_status = 'published'
                   AND t.provider_status = 'active' AND t.key NOT IN ('pattern', 'page')
                   AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
                   AND (json_extract(ov.target_json, '$.content_type') IS NULL
                     OR e.content_type_key = json_extract(ov.target_json, '$.content_type'))
                   AND (json_extract(ov.target_json, '$.entry_id') IS NULL
                     OR e.id = json_extract(ov.target_json, '$.entry_id'))
                   AND (json_extract(ov.target_json, '$.selector_slug') IS NULL
                     OR e.slug = json_extract(ov.target_json, '$.selector_slug'))
               ))
               OR (ov.target_kind = 'archive' AND EXISTS (
                 SELECT 1 FROM content_types AS t
                 WHERE t.provider_status = 'active' AND t.key != 'pattern'
                   AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
                   AND COALESCE(json_extract(t.definition_json, '$.settings.has_archive'), 0) = 1
                   AND (json_extract(ov.target_json, '$.content_type') IS NULL
                     OR t.key = json_extract(ov.target_json, '$.content_type'))
               ))
               OR (ov.target_kind = 'taxonomy' AND EXISTS (
                 SELECT 1 FROM taxonomy_definitions AS d
                 LEFT JOIN taxonomy_terms AS term ON term.kind = d.key
                 LEFT JOIN site_locales AS sl ON sl.code = term.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                 WHERE d.public = 1 AND d.provider_status = 'active'
                   AND d.declaration_state = 'declared'
                   AND (json_extract(ov.target_json, '$.taxonomy') IS NULL
                     OR d.key = json_extract(ov.target_json, '$.taxonomy'))
                   AND (json_extract(ov.target_json, '$.term_id') IS NULL
                     OR (term.id = json_extract(ov.target_json, '$.term_id') AND sl.code IS NOT NULL))
                   AND (json_extract(ov.target_json, '$.selector_slug') IS NULL
                     OR (term.slug = json_extract(ov.target_json, '$.selector_slug') AND sl.code IS NOT NULL))
               ))
             )
         ) OR EXISTS (
           SELECT 1
           FROM taxonomy_terms AS term
           JOIN taxonomy_definitions AS d ON d.key = term.kind
           JOIN site_locales AS sl ON sl.code = term.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
           WHERE term.featured_image_media_id = ?
             AND d.public = 1
             AND d.provider_status = 'active'
             AND d.declaration_state = 'declared'
         )"#,
    )
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .fetch_one(db)
    .await?;
    if result != 0 {
        return Ok(true);
    }

    let theme_option_candidates: Vec<(String, String)> = sqlx::query_as(
        "SELECT tom.option_key, r.manifest_json
           FROM theme_option_media_refs AS tom
           JOIN theme_revisions AS r ON r.theme_id = tom.owner_id
           JOIN theme_active_selection AS a ON a.revision_id = r.revision_id AND a.id = 1
           LEFT JOIN site_locales AS sl
             ON sl.code = tom.locale
            AND tom.locale != ''
          WHERE tom.owner_kind = 'theme'
            AND tom.media_id = ?
            AND (
              tom.locale = ''
              OR (
                sl.code IS NOT NULL
                AND sl.enabled = 1
                AND sl.deleted_at IS NULL
                AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
              )
            )",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    if !theme_option_candidates.is_empty()
        && let Some((_, first_manifest)) = theme_option_candidates.first()
        && let Ok(specs) = crate::storage::settings::parse_theme_options_schema(first_manifest)
    {
        let public_keys: std::collections::BTreeSet<&str> = specs
            .iter()
            .filter(|s| !s.is_ui_only() && s.public)
            .map(|s| s.key.as_str())
            .collect();
        if theme_option_candidates
            .iter()
            .any(|(opt_key, _)| public_keys.contains(opt_key.as_str()))
        {
            return Ok(true);
        }
    }

    // Builder + one-hop Pattern ACL projections are only candidates: a change
    // in Content Type / Field Group visibility can revoke body access without
    // rewriting old entry_pattern_refs (which intentionally preserve content).
    // Re-evaluate the effective schema at the public read boundary so stale
    // projections cannot serve a private body or an inactive provider.
    let builder_type_keys: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT e.content_type_key FROM content_entries AS e
         JOIN site_locales AS sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
         WHERE e.trashed_at IS NULL
           AND e.publication_status = 'published'
           AND e.content_type_key NOT IN ('pattern', 'section')
           AND (
             EXISTS (SELECT 1 FROM entry_builder_media_refs AS r
                     WHERE r.entry_id = e.id AND r.media_id = ? AND r.source_kind = 'builder')
             OR EXISTS (
               SELECT 1 FROM entry_pattern_refs AS pref
               JOIN entry_builder_media_refs AS r ON r.entry_id = pref.target_pattern_id
               WHERE pref.source_entry_id = e.id AND r.media_id = ? AND r.source_kind = 'builder'
             )
           )",
    )
    .bind(id)
    .bind(id)
    .fetch_all(db)
    .await?;
    if !builder_type_keys.is_empty() {
        let mut conn = db.acquire().await?;
        for type_key in builder_type_keys {
            if let Some(schema) =
                crate::storage::content::load_effective_content_schema_in_tx(&mut conn, &type_key)
                    .await?
                && crate::storage::page::body_can_grant_media(&schema)
            {
                return Ok(true);
            }
        }
    }

    let field_candidates: Vec<(String, String)> = sqlx::query_as(
        "SELECT DISTINCT e.content_type_key, e.values_json
         FROM entry_builder_media_refs AS r
         JOIN content_entries AS e ON e.id = r.entry_id
         JOIN content_types AS t ON t.key = e.content_type_key
         JOIN site_locales AS sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL
          AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
         WHERE r.media_id = ?
           AND r.source_kind = 'field'
           AND e.trashed_at IS NULL
           AND e.publication_status = 'published'
           AND e.content_type_key NOT IN ('pattern', 'section')
           AND t.provider_status = 'active'
           AND t.declaration_state = 'declared'
           AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    if !field_candidates.is_empty() {
        let mut conn = db.acquire().await?;
        for (type_key, values_json) in field_candidates {
            let Some(schema) =
                crate::storage::content::load_effective_content_schema_in_tx(&mut conn, &type_key)
                    .await?
            else {
                continue;
            };
            if schema.base.status != opendash_core::registry::ProviderStatus::Active
                || schema.base.declaration_state
                    != opendash_core::registry::DeclarationState::Declared
                || !schema.base.descriptor.settings.public
            {
                continue;
            }
            let Ok(values) = serde_json::from_str::<
                std::collections::BTreeMap<String, serde_json::Value>,
            >(&values_json) else {
                continue;
            };
            let specs: Vec<opendash_core::field_schema::FieldSpec> = schema
                .effective_fields
                .iter()
                .filter(|(k, entry)| {
                    k.as_str() != "body"
                        && entry.effective_status
                            == opendash_core::content::EffectiveFieldStatus::Active
                })
                .map(|(_, entry)| entry.definition.clone())
                .collect();
            let collected =
                opendash_core::field_schema::collect_public_media_ids(&specs, &values, true);
            if collected.all_ids().contains(id) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
