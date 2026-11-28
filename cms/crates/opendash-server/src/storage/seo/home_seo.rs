use super::*;

pub async fn get_home_seo_for_locale(
    db: &SqlitePool,
    requested_locale: Option<&str>,
) -> ApiResult<HomeSeoSnapshot> {
    let mut tx = db.begin().await?;
    let resolved_locale =
        crate::storage::settings::resolve_requested_locale_in_conn(&mut tx, requested_locale)
            .await?;
    let row: Option<(
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
    )> = sqlx::query_as(
        "SELECT version, title, description, og_image_id, canonical_url, noindex
               FROM site_home_seo WHERE locale = ?",
    )
    .bind(&resolved_locale)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    let (version, title, description, og_image_id, canonical_url, noindex) =
        row.unwrap_or((1, None, None, None, None, 0));
    Ok(HomeSeoSnapshot {
        version,
        title,
        description,
        og_image_id,
        canonical_url,
        noindex: noindex != 0,
        locale: requested_locale.map(|_| resolved_locale),
    })
}

pub async fn get_home_seo(db: &SqlitePool) -> ApiResult<HomeSeoSnapshot> {
    get_home_seo_for_locale(db, None).await
}

pub async fn put_home_seo(
    db: &SqlitePool,
    expected_version: i64,
    input: SeoMetadataInput,
) -> ApiResult<HomeSeoSnapshot> {
    put_home_seo_for_locale(db, None, expected_version, input).await
}

pub(in crate::storage) async fn project_og_image_url_versioned(
    db: &SqlitePool,
    pub_settings: &settings::PublicSiteSettings,
    media_id: Option<String>,
) -> ApiResult<Option<String>> {
    let Some(mid) = media_id else {
        return Ok(None);
    };
    let version: Option<i64> = sqlx::query_scalar(
        "SELECT version FROM media_assets WHERE id = ? AND lifecycle_state = 'active'",
    )
    .bind(&mid)
    .fetch_optional(db)
    .await?;
    let rel = match version {
        Some(v) => format!("/media/{mid}/file?v={}", v.max(1)),
        None => format!("/media/{mid}/file"),
    };
    Ok(Some(absolute_public_url(pub_settings, &rel).unwrap_or(rel)))
}

pub async fn build_entry_seo_view_for_locale(
    db: &SqlitePool,
    entry_id: i64,
    entry_title: &str,
    locale: Option<&str>,
    canonical_path: &str,
) -> ApiResult<SeoView> {
    let general = settings::get_general_for_locale(db, locale).await?;
    let pub_settings = settings::get_public(db).await?;
    let (resolved_locale, lang, dir) = settings::get_locale_bcp47_and_dir(db, locale).await?;
    let row: Option<(
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
    )> = sqlx::query_as(
        "SELECT title, description, og_image_id, canonical_url, noindex
               FROM entry_seo WHERE entry_id = ?",
    )
    .bind(entry_id)
    .fetch_optional(db)
    .await?;
    let (custom_title, description, og_image_id, custom_canonical, entry_noindex) =
        row.unwrap_or((None, None, None, None, 0));
    let mut proj =
        crate::storage::plugin_projections::batch_load_entry_projections(db, &[entry_id])
            .await?
            .remove(&entry_id)
            .unwrap_or_default();
    let (ov_title, ov_description, ov_og_image_id, ov_canonical, ov_noindex) =
        match proj.seo_override.take() {
            Some(ov) => (
                ov.title,
                ov.description,
                ov.og_image_id,
                ov.canonical_url,
                ov.noindex,
            ),
            None => (None, None, None, None, false),
        };
    let title = custom_title
        .or(ov_title)
        .unwrap_or_else(|| format!("{entry_title} — {}", general.site_title));
    let description = description.or(ov_description);
    let custom_canonical = custom_canonical.or(ov_canonical);
    let canonical_url = custom_canonical
        .clone()
        .or_else(|| absolute_public_url(&pub_settings, canonical_path));
    let og_image_url = if og_image_id.is_some() {
        let public_mid: Option<String> = sqlx::query_scalar(
            "SELECT media_id FROM seo_public_media_refs
              WHERE source_kind = 'entry_seo' AND source_id = ? LIMIT 1",
        )
        .bind(entry_id)
        .fetch_optional(db)
        .await?;
        project_og_image_url_versioned(db, &pub_settings, public_mid).await?
    } else if let Some(mid) = ov_og_image_id {
        if crate::storage::public_content::media_is_referenced(db, &mid).await? {
            project_og_image_url_versioned(db, &pub_settings, Some(mid)).await?
        } else {
            None
        }
    } else {
        None
    };
    let noindex = (entry_noindex != 0) || ov_noindex;
    let robots = if !pub_settings.indexing_enabled || noindex {
        "noindex,nofollow".to_string()
    } else {
        "index,follow".to_string()
    };

    let mut alternates = Vec::new();
    let mut tx = db.begin().await?;
    if let Some(origin) =
        has_multilingual_alternates_prereq(&mut tx, &pub_settings, noindex).await?
        && is_self_canonical_for_origin(custom_canonical.as_deref(), &origin, canonical_path)
    {
        let sibs: Vec<(String, i64, String, Option<String>)> = sqlx::query_as(
            "SELECT sib_sl.bcp47_tag, sib_sl.is_default, claims.path, s.canonical_url
               FROM content_entries cur
               JOIN site_locales cur_sl ON cur_sl.code = cur.locale AND cur_sl.enabled = 1 AND cur_sl.deleted_at IS NULL AND (cur_sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
               JOIN content_entries sib
                 ON sib.content_type_key = cur.content_type_key
                AND sib.translation_group_id = cur.translation_group_id
               JOIN site_locales sib_sl ON sib_sl.code = sib.locale AND sib_sl.enabled = 1 AND sib_sl.deleted_at IS NULL AND (sib_sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
               JOIN content_types t ON t.key = sib.content_type_key
               JOIN permalink_route_claims claims
                 ON claims.target_kind = 'entry'
                AND claims.target_id = CAST(sib.id AS TEXT)
                AND claims.locale = sib.locale
               LEFT JOIN entry_seo s ON s.entry_id = sib.id
              WHERE cur.id = ? AND cur.locale = ?
                AND cur.trashed_at IS NULL
                AND cur.publication_status = 'published'
                AND sib.trashed_at IS NULL
                AND sib.publication_status = 'published'
                AND sib.content_type_key NOT IN ('pattern', 'section')
                AND NOT EXISTS (
                    SELECT 1 FROM site_reading_settings reading
                     WHERE reading.locale = sib.locale
                       AND (reading.homepage_page_id = sib.id OR reading.posts_page_id = sib.id)
                )
                AND t.provider_status = 'active'
                AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
                AND COALESCE(s.noindex, 0) = 0
              ORDER BY sib_sl.is_default DESC, sib_sl.position ASC, sib_sl.code ASC",
        )
        .bind(entry_id)
        .bind(&resolved_locale)
        .fetch_all(&mut *tx)
        .await?;
        let members: Vec<(String, bool, String)> = sibs
            .into_iter()
            .filter(|(_, _, path, sib_canonical)| {
                is_self_canonical_for_origin(sib_canonical.as_deref(), &origin, path)
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
        json_ld: proj.json_ld,
    })
}

pub async fn build_simple_seo_view_for_locale(
    db: &SqlitePool,
    locale: Option<&str>,
    page_title: &str,
    canonical_path: Option<&str>,
    noindex_override: bool,
) -> ApiResult<SeoView> {
    let general = settings::get_general_for_locale(db, locale).await?;
    let pub_settings = settings::get_public(db).await?;
    let (_resolved_locale, lang, dir) = settings::get_locale_bcp47_and_dir(db, locale).await?;
    let title = format!("{page_title} — {}", general.site_title);
    let canonical_url = canonical_path.and_then(|p| absolute_public_url(&pub_settings, p));
    let robots = if noindex_override || !pub_settings.indexing_enabled {
        "noindex,nofollow".to_string()
    } else {
        "index,follow".to_string()
    };
    Ok(SeoView {
        lang,
        dir,
        title: title.clone(),
        description: None,
        canonical_url: canonical_url.clone(),
        alternates: Vec::new(),
        robots,
        og_title: title,
        og_description: None,
        og_url: canonical_url,
        og_image_url: None,
        json_ld: Vec::new(),
    })
}
