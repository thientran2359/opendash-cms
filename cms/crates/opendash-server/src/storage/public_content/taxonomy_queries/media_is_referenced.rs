use super::*;

use opendash_core::{
    content::{
        FieldGroupDomain, FieldRole, LocationEvalContext, evaluate_location_rules,
        is_restricted_to_admin_only,
    },
    field_schema::HydrationVisibility,
    registry::{DeclarationState, ProviderStatus},
};

/// Images become public only while referenced by an image/gallery custom field
/// of an existing CPT entry, published term, or public options page.
/// A private unreferenced upload remains 404 publicly.
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
           FROM seo_public_media_refs AS sm
           JOIN taxonomy_terms AS term ON sm.source_kind = 'term_seo' AND term.id = sm.source_id
           JOIN taxonomy_definitions AS td ON td.key = term.kind
           JOIN site_locales AS sl ON sl.code = term.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
           WHERE sm.media_id = ?
             AND td.public = 1
             AND td.provider_status = 'active'
             AND td.declaration_state = 'declared'
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
               ov.target_kind IN ('index', 'not_found', 'singular', 'part')
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
           LEFT JOIN site_locales AS sl ON sl.code = tom.locale AND tom.locale != ''
          WHERE tom.owner_kind = 'theme'
            AND tom.media_id = ?
            AND (
              tom.locale = ''
              OR (
                sl.code IS NOT NULL AND sl.enabled = 1 AND sl.deleted_at IS NULL
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
            .filter(|s| HydrationVisibility::OptionsPublicOnly.allows(s))
            .map(|s| s.key.as_str())
            .collect();
        if theme_option_candidates
            .iter()
            .any(|(opt_key, _)| public_keys.contains(opt_key.as_str()))
        {
            return Ok(true);
        }
    }

    let options_page_candidates: Vec<(String, String)> = sqlx::query_as(
        "SELECT tom.owner_id, tom.option_key
           FROM theme_option_media_refs AS tom
           LEFT JOIN site_locales AS sl ON sl.code = tom.locale AND tom.locale != ''
          WHERE tom.owner_kind = 'options_page'
            AND tom.media_id = ?
            AND (
              tom.locale = ''
              OR (
                sl.code IS NOT NULL AND sl.enabled = 1 AND sl.deleted_at IS NULL
                AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
              )
            )",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    if !options_page_candidates.is_empty() {
        let mut conn = db.acquire().await?;
        let active_theme_id: Option<String> = sqlx::query_scalar(
            "SELECT r.theme_id FROM theme_active_selection a
               JOIN theme_revisions r ON r.revision_id = a.revision_id
              WHERE a.id = 1 AND r.source_format = 'v3'",
        )
        .fetch_optional(&mut *conn)
        .await?;
        let mut checked_refs: std::collections::BTreeMap<
            String,
            std::collections::BTreeSet<String>,
        > = std::collections::BTreeMap::new();
        for (owner_id, opt_key) in options_page_candidates {
            let allowed_keys = if let Some(existing) = checked_refs.get(&owner_id) {
                existing
            } else {
                let is_owner_active = if owner_id == "site_global" {
                    true
                } else if let Some(tid) = owner_id.strip_prefix("theme:") {
                    active_theme_id.as_deref() == Some(tid)
                } else if let Some(page_key) = owner_id.strip_prefix("custom:") {
                    crate::storage::options_pages::get_options_page_in_conn(&mut conn, page_key)
                        .await?
                        .is_some_and(|p| {
                            p.provider_status == ProviderStatus::Active
                                && p.declaration_state == DeclarationState::Declared
                        })
                } else {
                    false
                };
                let mut set = std::collections::BTreeSet::new();
                if is_owner_active {
                    let groups = crate::storage::content::load_attached_groups_in_tx(
                        &mut conn,
                        "options_page",
                        &owner_id,
                        FieldGroupDomain::OptionsPage,
                    )
                    .await?;
                    let eval_ctx = LocationEvalContext {
                        options_page: Some(owner_id.clone()),
                        user_role: FieldRole::Editor,
                        ..Default::default()
                    };
                    for grp in groups {
                        if grp.is_runtime_active()
                            && !is_restricted_to_admin_only(&grp.location)
                            && evaluate_location_rules(&grp.location, &eval_ctx)
                        {
                            for spec in grp.fields {
                                if HydrationVisibility::OptionsPublicOnly.allows(&spec) {
                                    set.insert(spec.key);
                                }
                            }
                        }
                    }
                }
                checked_refs.insert(owner_id.clone(), set);
                &checked_refs[&owner_id]
            };
            if allowed_keys.contains(&opt_key) {
                return Ok(true);
            }
        }
    }

    let term_candidates: Vec<(String, String)> = sqlx::query_as(
        "SELECT DISTINCT tm.kind, tm.values_json
           FROM term_field_media_refs AS r
           JOIN taxonomy_terms AS tm ON tm.id = r.term_id
           JOIN taxonomy_definitions AS td ON td.key = tm.kind
           JOIN site_locales AS sl ON sl.code = tm.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL
            AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
          WHERE r.media_id = ?
            AND td.public = 1
            AND td.provider_status = 'active'
            AND td.declaration_state = 'declared'",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    if !term_candidates.is_empty() {
        let mut conn = db.acquire().await?;
        for (kind, values_json) in term_candidates {
            let Some(eff) =
                crate::storage::taxonomy::load_effective_term_schema_in_tx(&mut conn, &kind)
                    .await?
            else {
                continue;
            };
            if !eff.taxonomy.public
                || eff.taxonomy.provider_status != ProviderStatus::Active
                || eff.taxonomy.declaration_state != DeclarationState::Declared
            {
                continue;
            }
            let Ok(values) = serde_json::from_str::<
                std::collections::BTreeMap<String, serde_json::Value>,
            >(&values_json) else {
                continue;
            };
            let eval_ctx = LocationEvalContext {
                taxonomy_term: Some(kind),
                user_role: FieldRole::Editor,
                ..Default::default()
            };
            let specs: Vec<opendash_core::field_schema::FieldSpec> = eff
                .attached_groups
                .iter()
                .filter(|grp| {
                    grp.is_runtime_active()
                        && !is_restricted_to_admin_only(&grp.location)
                        && evaluate_location_rules(&grp.location, &eval_ctx)
                })
                .flat_map(|grp| grp.fields.iter().cloned())
                .filter(|spec| {
                    !spec.is_ui_only()
                        && eff.effective_fields.get(&spec.key).is_some_and(|fe| {
                            fe.effective_status
                                == opendash_core::content::EffectiveFieldStatus::Active
                        })
                })
                .collect();
            let collected =
                opendash_core::field_schema::collect_public_media_ids(&specs, &values, false);
            if collected.all_ids().contains(id) {
                return Ok(true);
            }
        }
    }

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

    let field_candidates: Vec<(i64, String, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT DISTINCT e.id, e.content_type_key, e.page_layout_key, e.custom_template_key, e.values_json
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
        let meta: Vec<(i64, String, Option<String>, Option<String>)> = field_candidates
            .iter()
            .map(|(eid, tk, pl, ct, _)| (*eid, tk.clone(), pl.clone(), ct.clone()))
            .collect();
        let eval_map = super::super::batch_load_entry_eval_contexts_in_conn(
            &mut conn,
            &meta,
            FieldRole::Editor,
        )
        .await?;
        for (eid, type_key, _, _, values_json) in field_candidates {
            let Some(schema) =
                crate::storage::content::load_effective_content_schema_in_tx(&mut conn, &type_key)
                    .await?
            else {
                continue;
            };
            if schema.base.status != ProviderStatus::Active
                || schema.base.declaration_state != DeclarationState::Declared
                || !schema.base.descriptor.settings.public
            {
                continue;
            }
            let Ok(values) = serde_json::from_str::<
                std::collections::BTreeMap<String, serde_json::Value>,
            >(&values_json) else {
                continue;
            };
            let eval_ctx = eval_map.get(&eid).cloned().unwrap_or_default();
            let specs: Vec<opendash_core::field_schema::FieldSpec> =
                crate::content_render::ordered_public_field_defs_for_entry(
                    &schema,
                    FieldRole::Editor,
                    &eval_ctx,
                )
                .into_iter()
                .filter(|spec| spec.key != "body")
                .cloned()
                .collect();
            let collected =
                opendash_core::field_schema::collect_public_media_ids(&specs, &values, false);
            if collected.all_ids().contains(id) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
