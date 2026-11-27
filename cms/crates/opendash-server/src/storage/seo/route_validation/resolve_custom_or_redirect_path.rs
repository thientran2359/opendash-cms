use super::*;

/// Resolves an arbitrary public fallback path against `entry_public_paths` and `entry_redirects`
/// (scoped by active locale prefix). Never returns draft/unpublished/disabled-locale entries.
pub async fn resolve_custom_or_redirect_path(
    db: &SqlitePool,
    path: &str,
) -> ApiResult<PublicRouteResolution> {
    let mut tx = db.begin().await?;
    let default_locale = crate::storage::settings::get_default_locale_code_in_conn(&mut tx).await?;
    let secondary_roots =
        crate::storage::permalinks::load_secondary_locale_roots_in_conn(&mut tx).await?;
    let first_seg = path.trim_start_matches('/').split('/').next().unwrap_or("");
    let (lookup_locale, relative_path) = if secondary_roots.contains(first_seg) {
        let prefix = format!("/{first_seg}");
        if path == prefix {
            (first_seg.to_owned(), "/")
        } else if let Some(rest) = path.strip_prefix(&prefix)
            && rest.starts_with('/')
        {
            (first_seg.to_owned(), rest)
        } else {
            (default_locale.clone(), path)
        }
    } else {
        (default_locale.clone(), path)
    };

    if validate_custom_entry_path(relative_path).is_ok() {
        let direct_id: Option<i64> = sqlx::query_scalar(
            "SELECT e.id
               FROM entry_public_paths p
               JOIN content_entries e ON e.id = p.entry_id
               JOIN site_locales sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
               JOIN content_types t ON t.key = e.content_type_key
              WHERE p.locale = ?
                AND p.path = ?
                AND e.trashed_at IS NULL
                AND e.publication_status = 'published'
                AND e.content_type_key != 'pattern'
                AND t.provider_status = 'active'
                AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1",
        )
        .bind(&lookup_locale)
        .bind(relative_path)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(eid) = direct_id {
            tx.commit().await?;
            if let Some(record) =
                crate::storage::public_content::get_published_by_id(db, eid).await?
            {
                let canonical = crate::storage::permalinks::prefix_for_locale(
                    &lookup_locale,
                    &default_locale,
                    relative_path,
                );
                return Ok(PublicRouteResolution::Entry {
                    record,
                    canonical_path: canonical,
                });
            }
            return Ok(PublicRouteResolution::NotFound);
        }
    }

    let redirect_eid: Option<i64> = sqlx::query_scalar(
        "SELECT r.entry_id
           FROM entry_redirects r
           JOIN content_entries e ON e.id = r.entry_id
           JOIN site_locales sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
           JOIN content_types t ON t.key = e.content_type_key
          WHERE r.locale = ?
            AND r.source_path = ?
            AND e.trashed_at IS NULL
            AND e.publication_status = 'published'
            AND e.content_type_key != 'pattern'
            AND t.provider_status = 'active'
            AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1",
    )
    .bind(&lookup_locale)
    .bind(relative_path)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(eid) = redirect_eid
        && let Some(target) = resolve_public_entry_path_in_tx(&mut tx, eid).await?
        && target != path
    {
        tx.commit().await?;
        return Ok(PublicRouteResolution::Redirect { location: target });
    }
    tx.commit().await?;
    Ok(PublicRouteResolution::NotFound)
}

pub(in crate::storage) use crate::public_context::{PublicFilters, PublicQuery};

pub(in crate::storage) use crate::seo::{
    EntrySeoSnapshot, HomeSeoSnapshot, HreflangAlternate, SeoMetadataInput, SeoView,
    validate_seo_input,
};

pub(in crate::storage) use crate::storage::settings::{self, absolute_public_url};

pub async fn build_home_seo_view_for_locale(
    db: &SqlitePool,
    locale: Option<&str>,
    canonical_path: &str,
    allow_alternates: bool,
) -> ApiResult<SeoView> {
    let general = settings::get_general_for_locale(db, locale).await?;
    let pub_settings = settings::get_public(db).await?;
    let (_resolved_locale, lang, dir) = settings::get_locale_bcp47_and_dir(db, locale).await?;
    let home = get_home_seo_for_locale(db, locale).await?;
    let mut has_explicit_title = home.title.is_some();
    let mut title = home.title.unwrap_or_else(|| {
        if general.tagline.trim().is_empty() {
            general.site_title.clone()
        } else {
            format!("{} — {}", general.site_title, general.tagline)
        }
    });
    let mut description = home.description;
    let mut custom_canonical = home.canonical_url;
    let mut og_image_url = project_og_image_url(&pub_settings, home.og_image_id);
    let mut noindex = home.noindex;
    let mut home_json_ld = Vec::new();
    let page_state = reading_page_state_for_locale(db, locale, true).await?;
    if let ReadingPageState::Published(id) = page_state {
        let entry_meta: Option<(
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            i64,
        )> = sqlx::query_as(
            "SELECT title, description, og_image_id, canonical_url, noindex
                   FROM entry_seo WHERE entry_id = ?",
        )
        .bind(id)
        .fetch_optional(db)
        .await?;
        if let Some((entry_title, entry_description, entry_image, entry_canonical, entry_noindex)) =
            entry_meta
        {
            if let Some(entry_title) = entry_title {
                title = entry_title;
                has_explicit_title = true;
            }
            description = entry_description.or(description);
            custom_canonical = entry_canonical.or(custom_canonical);
            if entry_image.is_some() {
                let media_id: Option<String> = sqlx::query_scalar(
                    "SELECT media_id FROM seo_public_media_refs
                      WHERE source_kind = 'entry_seo' AND source_id = ? LIMIT 1",
                )
                .bind(id)
                .fetch_optional(db)
                .await?;
                og_image_url = project_og_image_url(&pub_settings, media_id);
            }
            noindex |= entry_noindex != 0;
        }
        let mut home_proj =
            crate::storage::plugin_projections::batch_load_entry_projections(db, &[id])
                .await?
                .remove(&id)
                .unwrap_or_default();
        if let Some(ov) = home_proj.seo_override.take() {
            if !has_explicit_title && let Some(ov_title) = ov.title {
                title = ov_title;
            }
            description = description.or(ov.description);
            custom_canonical = custom_canonical.or(ov.canonical_url);
            if og_image_url.is_none()
                && let Some(mid) = ov.og_image_id
                && crate::storage::public_content::media_is_referenced(db, &mid).await?
            {
                og_image_url = project_og_image_url(&pub_settings, Some(mid));
            }
            noindex |= ov.noindex;
        }
        home_json_ld = home_proj.json_ld;
    }
    let canonical_url = custom_canonical
        .clone()
        .or_else(|| absolute_public_url(&pub_settings, canonical_path));
    let robots = if !pub_settings.indexing_enabled || noindex {
        "noindex,nofollow".to_string()
    } else {
        "index,follow".to_string()
    };

    let mut alternates = Vec::new();
    let mut tx = db.begin().await?;
    if allow_alternates
        && matches!(page_state, ReadingPageState::Published(_))
        && let Some(origin) =
            has_multilingual_alternates_prereq(&mut tx, &pub_settings, noindex).await?
        && is_self_canonical_for_origin(custom_canonical.as_deref(), &origin, canonical_path)
    {
        let homes: Vec<(String, i64, String, Option<String>)> = sqlx::query_as(
            "SELECT sl.bcp47_tag, sl.is_default, claims.path, COALESCE(s.canonical_url, h.canonical_url)
               FROM site_reading_settings reading
               JOIN site_locales sl ON sl.code = reading.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
               JOIN content_entries e ON e.id = reading.homepage_page_id
               JOIN content_types t ON t.key = e.content_type_key
               JOIN permalink_route_claims claims
                 ON claims.target_kind = 'entry'
                AND claims.target_id = CAST(e.id AS TEXT)
                AND claims.locale = reading.locale
               LEFT JOIN entry_seo s ON s.entry_id = e.id
               LEFT JOIN site_home_seo h ON h.locale = reading.locale
              WHERE e.content_type_key = 'page'
                AND e.locale = reading.locale
                AND e.trashed_at IS NULL
                AND e.publication_status = 'published'
                AND t.provider_status = 'active'
                AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
                AND COALESCE(s.noindex, h.noindex, 0) = 0
              ORDER BY sl.is_default DESC, sl.position ASC, sl.code ASC",
        )
        .fetch_all(&mut *tx)
        .await?;
        let members: Vec<(String, bool, String)> = homes
            .into_iter()
            .filter(|(_, _, path, home_canonical)| {
                is_self_canonical_for_origin(home_canonical.as_deref(), &origin, path)
            })
            .map(|(bcp47, is_def, path, _)| (bcp47, is_def != 0, path))
            .collect();
        alternates = finalize_alternates_for_origin(&origin, &members);
    }
    tx.commit().await?;

    Ok(SeoView {
        lang,
        dir,
        title: title.clone(),
        description: description.clone(),
        canonical_url: canonical_url.clone(),
        alternates,
        robots,
        og_title: title,
        og_description: description,
        og_url: canonical_url,
        og_image_url,
        json_ld: home_json_ld,
    })
}
