use super::*;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TranslationLinkSummary {
    pub id: i64,
    pub locale: String,
    pub slug: String,
    pub title: String,
    pub publication_status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContentEntryRecord {
    pub id: i64,
    pub content_type_key: String,
    pub locale: String,
    pub translation_group_id: String,
    pub slug: String,
    pub title: String,
    pub values: BTreeMap<String, Value>,
    /// Site-owned term IDs. Stable across content slug edits, publication and provider disable.
    pub term_ids: Vec<i64>,
    pub primary_category_term_id: Option<i64>,
    /// Site-owned theme selection; retained even if its provider is withdrawn.
    pub page_layout_key: Option<String>,
    pub custom_template_key: Option<String>,
    pub content_mode: ContentMode,
    /// Server-owned status; legacy CPT entries remain published on Save.
    pub publication_status: String,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
    pub published_at: Option<String>,
    pub translations: Vec<TranslationLinkSummary>,
}

pub(in crate::storage) fn decode_row(row: EntryRow) -> ApiResult<ContentEntryRecord> {
    let (
        id,
        content_type_key,
        slug,
        title,
        json,
        publication_status,
        version,
        created_at,
        updated_at,
        published_at,
    ) = row;
    let values: BTreeMap<String, Value> = serde_json::from_str(&json)
        .map_err(|_| ApiError::internal("Dữ liệu custom fields không hợp lệ."))?;
    Ok(ContentEntryRecord {
        id,
        content_type_key,
        locale: String::new(),
        translation_group_id: String::new(),
        slug,
        title,
        values,
        term_ids: Vec::new(),
        primary_category_term_id: None,
        page_layout_key: None,
        custom_template_key: None,
        content_mode: ContentMode::Editor,
        publication_status,
        version,
        created_at,
        updated_at,
        published_at,
        translations: Vec::new(),
    })
}

pub(in crate::storage) async fn decoded_with_terms(
    tx: &mut Transaction<'_, Sqlite>,
    row: EntryRow,
) -> ApiResult<ContentEntryRecord> {
    let mut record = decode_row(row)?;
    record.term_ids = read_term_ids(tx, record.id).await?;
    let (locale, translation_group_id, layout, custom, mode, primary_category_term_id): (
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        Option<i64>,
    ) = sqlx::query_as(
        "SELECT locale, translation_group_id, page_layout_key, custom_template_key,
                content_mode, primary_category_term_id
           FROM content_entries WHERE id = ?",
    )
    .bind(record.id)
    .fetch_one(&mut **tx)
    .await?;
    let sibling_rows: Vec<(i64, String, String, String, String)> = sqlx::query_as(
        "SELECT e.id, e.locale, e.slug, e.title, e.publication_status
           FROM content_entries e
           JOIN site_locales sl ON sl.code = e.locale AND sl.deleted_at IS NULL
          WHERE e.content_type_key = ?
            AND e.translation_group_id = ?
            AND e.trashed_at IS NULL
          ORDER BY sl.is_default DESC, sl.position ASC, sl.code ASC",
    )
    .bind(&record.content_type_key)
    .bind(&translation_group_id)
    .fetch_all(&mut **tx)
    .await?;
    record.locale = locale;
    record.translation_group_id = translation_group_id;
    record.page_layout_key = layout;
    record.custom_template_key = custom;
    record.primary_category_term_id = primary_category_term_id;
    record.content_mode = match mode.as_str() {
        "editor" => ContentMode::Editor,
        "builder" => ContentMode::Builder,
        _ => return Err(ApiError::internal("Chế độ nội dung không hợp lệ.")),
    };
    record.translations = sibling_rows
        .into_iter()
        .map(
            |(id, locale, slug, title, publication_status)| TranslationLinkSummary {
                id,
                locale,
                slug,
                title,
                publication_status,
            },
        )
        .collect();
    Ok(record)
}

pub(in crate::storage) async fn get_entry_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    id: i64,
) -> ApiResult<Option<ContentEntryRecord>> {
    let row: Option<EntryRow> = sqlx::query_as(
        "SELECT id, content_type_key, slug, title, values_json, publication_status, version, created_at, updated_at, published_at
         FROM content_entries WHERE content_type_key = ? AND id = ? AND trashed_at IS NULL",
    )
    .bind(type_key)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;
    let entry = match row {
        Some(row) => Some(decoded_with_terms(tx, row).await?),
        None => None,
    };
    Ok(entry)
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositeEntrySaveInput {
    #[serde(default)]
    pub expected_version: Option<i64>,
    #[serde(default)]
    pub expected_schema_token: Option<String>,
    #[serde(default)]
    pub locale: Option<String>,
    pub title: String,
    pub slug: String,
    #[serde(default)]
    pub publication_status: Option<String>,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
    #[serde(default)]
    pub term_ids: Option<Vec<i64>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub primary_category_term_id: PatchField<i64>,
    #[serde(default)]
    pub auto_unique_slug: bool,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub custom_path: PatchField<String>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub page_layout_key: PatchField<String>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub custom_template_key: PatchField<String>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub content_mode: PatchField<ContentMode>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub seo: PatchField<crate::seo::SeoMetadataInput>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

pub(in crate::storage) async fn resolve_auto_unique_slug_in_tx(
    conn: &mut sqlx::SqliteConnection,
    type_key: &str,
    base_slug: &str,
    term_ids: Option<&[i64]>,
    primary_category_term_id: Option<i64>,
    has_custom_path: bool,
) -> ApiResult<String> {
    resolve_auto_unique_slug_for_locale_in_tx(
        conn,
        type_key,
        base_slug,
        term_ids,
        primary_category_term_id,
        has_custom_path,
        None,
    )
    .await
}

pub(crate) async fn resolve_auto_unique_slug_for_locale_in_tx(
    conn: &mut sqlx::SqliteConnection,
    type_key: &str,
    base_slug: &str,
    term_ids: Option<&[i64]>,
    primary_category_term_id: Option<i64>,
    has_custom_path: bool,
    locale: Option<&str>,
) -> ApiResult<String> {
    if !opendash_core::id::valid_kebab_segment_with_limit(base_slug, 64) {
        return Err(ApiError::unprocessable("Slug không hợp lệ."));
    }
    let default_locale = crate::storage::settings::get_default_locale_code_in_conn(conn).await?;
    let target_locale = match locale {
        Some(loc) => loc.to_owned(),
        None => default_locale.clone(),
    };
    let secondary_roots =
        crate::storage::permalinks::load_secondary_locale_roots_in_conn(conn).await?;
    let (post_structure, cpt_bases_json, tax_bases_json): (String, String, String) =
        sqlx::query_as(
            "SELECT post_structure, content_type_bases_json, taxonomy_bases_json
               FROM site_permalink_settings WHERE id = 1",
        )
        .fetch_one(&mut *conn)
        .await?;
    let posts_page_id: Option<i64> = sqlx::query_scalar(
        "SELECT r.posts_page_id
           FROM site_reading_settings r
          WHERE r.locale = ?",
    )
    .bind(&target_locale)
    .fetch_optional(&mut *conn)
    .await?
    .flatten();
    let cpt_bases: BTreeMap<String, Option<String>> =
        serde_json::from_str(&cpt_bases_json).unwrap_or_default();
    let tax_bases: BTreeMap<String, Option<String>> =
        serde_json::from_str(&tax_bases_json).unwrap_or_default();

    let is_public_type = if matches!(type_key, "pattern" | "section") {
        false
    } else {
        sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(json_extract(definition_json, '$.settings.public'), 1)
               FROM content_types WHERE key = ? AND provider_status = 'active'",
        )
        .bind(type_key)
        .fetch_optional(&mut *conn)
        .await?
            == Some(1)
    };

    let cat_slug = if type_key == "post" && post_structure.contains("%category%") {
        let cat_public: i64 = sqlx::query_scalar(
            "SELECT EXISTS(
               SELECT 1 FROM taxonomy_definitions
                WHERE key = 'category' AND public = 1
                  AND provider_status = 'active' AND declaration_state = 'declared'
             )",
        )
        .fetch_one(&mut *conn)
        .await?;
        let cat_eligible = if cat_public == 1 {
            crate::storage::content::load_effective_taxonomies_in_tx(&mut *conn, "post")
                .await?
                .iter()
                .any(|k| k == "category")
        } else {
            false
        };
        let resolved_cat = if !cat_eligible {
            "uncategorized".to_owned()
        } else {
            let primary_slug: Option<String> = match primary_category_term_id {
                Some(pid) => {
                    sqlx::query_scalar(
                        "SELECT slug FROM taxonomy_terms WHERE id = ? AND kind = 'category' AND locale = ?",
                    )
                    .bind(pid)
                    .bind(&target_locale)
                    .fetch_optional(&mut *conn)
                    .await?
                }
                None => None,
            };
            match primary_slug {
                Some(slug) => slug,
                None => match term_ids {
                    Some(ids) if !ids.is_empty() => {
                        let ids_json = serde_json::to_string(ids)?;
                        let min_slug: Option<String> = sqlx::query_scalar(
                            "SELECT slug FROM taxonomy_terms
                              WHERE kind = 'category'
                                AND locale = ?
                                AND id IN (SELECT value FROM json_each(?))
                              ORDER BY id ASC LIMIT 1",
                        )
                        .bind(&target_locale)
                        .bind(ids_json)
                        .fetch_optional(&mut *conn)
                        .await?;
                        min_slug.unwrap_or_else(|| "uncategorized".to_owned())
                    }
                    _ => "uncategorized".to_owned(),
                },
            }
        };
        if post_structure.starts_with("/%category%/") {
            if crate::seo::is_reserved_system_root(&resolved_cat)
                || (target_locale == default_locale
                    && crate::storage::permalinks::is_secondary_locale_root(
                        &resolved_cat,
                        &secondary_roots,
                    ))
            {
                return Err(ApiError::conflict(format!(
                    "permalink_category_root_collision: slug '{resolved_cat}' trùng gốc hệ thống."
                )));
            }
            if posts_page_id.is_none() && resolved_cat == "posts" {
                return Err(ApiError::conflict(
                    "permalink_category_root_collision: slug 'posts' trùng trang lưu trữ mặc định.",
                ));
            }
            let public_cpts: Vec<String> = sqlx::query_scalar(
                "SELECT key FROM content_types
                  WHERE key NOT IN ('pattern', 'section', 'page', 'post')
                    AND provider_status = 'active'
                    AND COALESCE(json_extract(definition_json, '$.settings.public'), 1) = 1",
            )
            .fetch_all(&mut *conn)
            .await?;
            for key in public_cpts {
                let base = resolve_effective_rewrite_base_in_tx(
                    &mut *conn,
                    "content_type",
                    &key,
                    &cpt_bases,
                )
                .await?;
                if base.trim_start_matches('/').split('/').next().unwrap_or("") == resolved_cat {
                    return Err(ApiError::conflict(format!(
                        "permalink_category_root_collision: slug '{resolved_cat}' trùng CPT '{key}'."
                    )));
                }
            }
            let public_taxes: Vec<String> = sqlx::query_scalar(
                "SELECT key FROM taxonomy_definitions
                  WHERE public = 1 AND provider_status = 'active' AND declaration_state = 'declared'",
            )
            .fetch_all(&mut *conn)
            .await?;
            for key in public_taxes {
                let base =
                    resolve_effective_rewrite_base_in_tx(&mut *conn, "taxonomy", &key, &tax_bases)
                        .await?;
                if base.trim_start_matches('/').split('/').next().unwrap_or("") == resolved_cat {
                    return Err(ApiError::conflict(format!(
                        "permalink_category_root_collision: slug '{resolved_cat}' trùng taxonomy '{key}'."
                    )));
                }
            }
        }
        resolved_cat
    } else {
        "uncategorized".to_owned()
    };

    let cpt_base = if !matches!(type_key, "post" | "page") {
        Some(
            resolve_effective_rewrite_base_in_tx(&mut *conn, "content_type", type_key, &cpt_bases)
                .await?,
        )
    } else {
        None
    };
    let uses_post_id = type_key == "post" && post_structure.contains("%post_id%");
    let check_public_path = !has_custom_path && is_public_type && !uses_post_id;
    let check_root_reserved =
        type_key == "page" || (type_key == "post" && post_structure == "/%postname%");

    for n in 1..=100usize {
        let candidate = if n == 1 {
            base_slug.to_owned()
        } else {
            let suffix = format!("-{n}");
            let max_prefix = 64usize.saturating_sub(suffix.len());
            let prefix = base_slug[..base_slug.len().min(max_prefix)].trim_end_matches('-');
            if prefix.is_empty() {
                continue;
            }
            format!("{prefix}{suffix}")
        };
        let slug_taken: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM content_entries WHERE content_type_key = ? AND locale = ? AND slug = ?)",
        )
        .bind(type_key)
        .bind(&target_locale)
        .bind(&candidate)
        .fetch_one(&mut *conn)
        .await?;
        if slug_taken != 0 {
            continue;
        }
        if check_public_path {
            if check_root_reserved
                && (crate::seo::is_reserved_system_root(&candidate)
                    || (target_locale == default_locale
                        && crate::storage::permalinks::is_secondary_locale_root(
                            &candidate,
                            &secondary_roots,
                        )))
            {
                continue;
            }
            let candidate_path = match type_key {
                "page" => format!("/{candidate}"),
                "post" => post_structure
                    .replace("%category%", &cat_slug)
                    .replace("%postname%", &candidate),
                _ => format!("/{}/{candidate}", cpt_base.as_deref().unwrap_or(type_key)),
            };
            let effective_path = crate::storage::permalinks::prefix_for_locale(
                &target_locale,
                &default_locale,
                &candidate_path,
            );
            let path_taken: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM permalink_route_claims WHERE path = ?)
                     OR EXISTS(SELECT 1 FROM permalink_route_redirects WHERE source_path = ? AND origin != 'locale_swap')
                     OR EXISTS(SELECT 1 FROM entry_redirects WHERE locale = ? AND source_path = ?)",
            )
            .bind(&effective_path)
            .bind(&effective_path)
            .bind(&target_locale)
            .bind(&candidate_path)
            .fetch_one(&mut *conn)
            .await?;
            if path_taken != 0 {
                continue;
            }
        }
        return Ok(candidate);
    }

    Err(ApiError::conflict(
        "Không thể tự động tạo slug duy nhất trong giới hạn 100 lần thử.",
    ))
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateEntryTranslationInput {
    pub target_locale: String,
    pub slug: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub auto_unique_slug: bool,
}

pub struct EntryTranslationPlan {
    pub target_locale: String,
    pub translation_group_id: String,
    pub slug: String,
    pub title: String,
    pub values: BTreeMap<String, Value>,
    pub term_ids: Vec<i64>,
    pub primary_category_term_id: Option<i64>,
    pub page_layout_key: Option<String>,
    pub custom_template_key: Option<String>,
    pub content_mode: ContentMode,
}
