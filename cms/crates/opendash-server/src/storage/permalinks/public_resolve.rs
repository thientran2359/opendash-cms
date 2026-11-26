use super::*;

impl From<ApiError> for PermalinkPutError {
    fn from(err: ApiError) -> Self {
        Self::Api(err)
    }
}

pub(in crate::storage) type Claim = (String, String, String); // (stable target kind, stable target ID, locale)

/// Build the public route path for `locale`, hiding prefix on `default_locale`
/// and prepending `/<locale>` for active secondary locales.
pub(crate) fn prefix_for_locale(locale: &str, default_locale: &str, relative_path: &str) -> String {
    if locale == default_locale {
        if relative_path.is_empty() {
            "/".to_owned()
        } else if relative_path.starts_with('/') {
            relative_path.to_owned()
        } else {
            format!("/{relative_path}")
        }
    } else if relative_path.is_empty() || relative_path == "/" {
        format!("/{locale}")
    } else if relative_path.starts_with('/') {
        format!("/{locale}{relative_path}")
    } else {
        format!("/{locale}/{relative_path}")
    }
}

/// Check whether root path segment `seg` collides with an active secondary locale code.
pub(crate) fn is_secondary_locale_root(seg: &str, secondary_roots: &HashSet<String>) -> bool {
    secondary_roots.contains(seg)
}

/// Load enabled non-default locale codes reserved at the URL root for `default_locale`.
pub(crate) async fn load_secondary_locale_roots_in_conn(
    conn: &mut SqliteConnection,
) -> ApiResult<HashSet<String>> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT code FROM site_locales WHERE enabled = 1 AND deleted_at IS NULL AND is_default = 0 AND (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1 ORDER BY code ASC",
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().collect())
}

pub(in crate::storage) async fn read_settings(
    conn: &mut SqliteConnection,
) -> ApiResult<PermalinkSettings> {
    let row: (i64, String, String, String) = sqlx::query_as(
        "SELECT version, post_structure, content_type_bases_json, taxonomy_bases_json
           FROM site_permalink_settings WHERE id=1",
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(PermalinkSettings {
        version: row.0,
        post_structure: row.1,
        content_type_bases: serde_json::from_str(&row.2)?,
        taxonomy_bases: serde_json::from_str(&row.3)?,
    })
}

pub async fn get_settings(db: &SqlitePool) -> ApiResult<PermalinkSettings> {
    let mut conn = db.acquire().await?;
    read_settings(&mut conn).await
}

/// The external C3 settings DTO contains only explicit site selections. Its
/// sparse maps intentionally distinguish an omitted key from an explicit null.
/// Overlay active provider defaults privately for canonical routing only.
pub(in crate::storage) async fn read_effective_settings(
    conn: &mut SqliteConnection,
) -> ApiResult<PermalinkSettings> {
    let mut settings = read_settings(conn).await?;
    let provider_rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT resource_kind, resource_key, rewrite_base
           FROM provider_rewrite_defaults
          WHERE rewrite_base IS NOT NULL AND provider_status='active'
            AND declaration_state='declared' ORDER BY resource_kind,resource_key",
    )
    .fetch_all(&mut *conn)
    .await?;
    for (kind, key, base) in provider_rows {
        match kind.as_str() {
            "content_type" if !settings.content_type_bases.contains_key(&key) => {
                settings.content_type_bases.insert(key, Some(base));
            }
            "taxonomy" if !settings.taxonomy_bases.contains_key(&key) => {
                settings.taxonomy_bases.insert(key, Some(base));
            }
            _ => {}
        }
    }
    Ok(settings)
}

pub(in crate::storage) async fn invalidate_if_effective_changed(
    conn: &mut SqliteConnection,
    before: &PermalinkSettings,
) -> ApiResult<()> {
    let after = read_effective_settings(conn).await?;
    if before.post_structure != after.post_structure
        || before.content_type_bases != after.content_type_bases
        || before.taxonomy_bases != after.taxonomy_bases
    {
        sqlx::query("UPDATE site_permalink_settings SET version=version+1 WHERE id=1")
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// The caller holds the same write transaction that installs/activates the
/// package and rebuilds all C3 claims. Owner identity is immutable even after
/// withdrawal, disable and provider reinstall.
pub async fn sync_provider_rewrite_defaults_in_tx(
    conn: &mut SqliteConnection,
    owner_kind: &str,
    owner_id: &str,
    declarations: &[ProviderRewriteDeclaration],
) -> ApiResult<()> {
    if !matches!(owner_kind, "plugin" | "theme") || owner_id.is_empty() {
        return Err(ApiError::bad_request(
            "Provider rewrite owner không hợp lệ.",
        ));
    }
    let before = read_effective_settings(conn).await?;
    let mut incoming = std::collections::BTreeSet::new();
    for item in declarations {
        if !matches!(item.resource_kind, "content_type" | "taxonomy")
            || !valid_content_type_key(&item.resource_key)
            || (item.resource_kind == "content_type"
                && matches!(item.resource_key.as_str(), "post" | "page" | "pattern"))
            || item
                .rewrite_base
                .as_deref()
                .is_some_and(|base| !valid_base(base))
        {
            return Err(ApiError::bad_request("Provider rewrite_base không hợp lệ."));
        }
        if !incoming.insert((item.resource_kind, item.resource_key.as_str())) {
            return Err(ApiError::bad_request(
                "Provider rewrite khai báo key trùng.",
            ));
        }
        let (existing_owner, existing_id): (String, Option<String>) = match item.resource_kind {
            "content_type" => {
                sqlx::query_as("SELECT owner_kind, owner_id FROM content_types WHERE key=?")
                    .bind(&item.resource_key)
                    .fetch_optional(&mut *conn)
                    .await?
            }
            _ => {
                sqlx::query_as("SELECT owner_kind, owner_id FROM taxonomy_definitions WHERE key=?")
                    .bind(&item.resource_key)
                    .fetch_optional(&mut *conn)
                    .await?
            }
        }
        .ok_or_else(|| ApiError::conflict("Provider rewrite target chưa được khai báo."))?;
        if existing_owner != owner_kind || existing_id.as_deref() != Some(owner_id) {
            return Err(ApiError::conflict("Provider rewrite key thuộc owner khác."));
        }
        let owner_of_default: Option<(String, String)> = sqlx::query_as(
            "SELECT owner_kind,owner_id FROM provider_rewrite_defaults
             WHERE resource_kind=? AND resource_key=?",
        )
        .bind(item.resource_kind)
        .bind(&item.resource_key)
        .fetch_optional(&mut *conn)
        .await?;
        if owner_of_default
            .as_ref()
            .is_some_and(|(kind, id)| kind != owner_kind || id != owner_id)
        {
            return Err(ApiError::conflict(
                "Provider rewrite key đã thuộc owner khác.",
            ));
        }
        sqlx::query(
            "INSERT INTO provider_rewrite_defaults
               (resource_kind,resource_key,owner_kind,owner_id,rewrite_base)
             VALUES (?,?,?,?,?)
             ON CONFLICT(resource_kind,resource_key) DO UPDATE SET
               rewrite_base=excluded.rewrite_base,provider_status='active',
               declaration_state='declared'",
        )
        .bind(item.resource_kind)
        .bind(&item.resource_key)
        .bind(owner_kind)
        .bind(owner_id)
        .bind(&item.rewrite_base)
        .execute(&mut *conn)
        .await?;
    }
    let old_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT resource_kind,resource_key FROM provider_rewrite_defaults
         WHERE owner_kind=? AND owner_id=?",
    )
    .bind(owner_kind)
    .bind(owner_id)
    .fetch_all(&mut *conn)
    .await?;
    for (kind, key) in old_rows {
        if !incoming.contains(&(kind.as_str(), key.as_str())) {
            sqlx::query(
                "UPDATE provider_rewrite_defaults
                 SET declaration_state='withdrawn' WHERE resource_kind=? AND resource_key=?",
            )
            .bind(kind)
            .bind(key)
            .execute(&mut *conn)
            .await?;
        }
    }
    invalidate_if_effective_changed(conn, &before).await
}

pub async fn set_provider_rewrite_status_in_tx(
    conn: &mut SqliteConnection,
    owner_kind: &str,
    owner_id: &str,
    active: bool,
) -> ApiResult<()> {
    let before = read_effective_settings(conn).await?;
    let status = if active {
        "active"
    } else {
        "provider_inactive"
    };
    sqlx::query(
        "UPDATE provider_rewrite_defaults SET provider_status=?
         WHERE owner_kind=? AND owner_id=?
           AND (?=0 OR declaration_state='declared')",
    )
    .bind(status)
    .bind(owner_kind)
    .bind(owner_id)
    .bind(i64::from(active))
    .execute(&mut *conn)
    .await?;
    invalidate_if_effective_changed(conn, &before).await
}

pub async fn deactivate_other_theme_rewrites_in_tx(
    conn: &mut SqliteConnection,
    active_theme_id: &str,
) -> ApiResult<()> {
    let before = read_effective_settings(conn).await?;
    sqlx::query(
        "UPDATE provider_rewrite_defaults SET provider_status='provider_inactive'
         WHERE owner_kind='theme' AND owner_id!=?",
    )
    .bind(active_theme_id)
    .execute(&mut *conn)
    .await?;
    invalidate_if_effective_changed(conn, &before).await
}

pub(in crate::storage) fn valid_base(base: &str) -> bool {
    let segs: Vec<_> = base.split('/').collect();
    !segs.is_empty()
        && segs.len() <= 3
        && segs
            .iter()
            .all(|seg| valid_kebab_segment_with_limit(seg, 64))
        && !crate::storage::seo::is_reserved_route_root(segs[0])
}

pub(in crate::storage) fn validate_post_structure(format: &str) -> ApiResult<()> {
    if format.len() > 512 || !format.starts_with('/') || format.ends_with('/') {
        return Err(ApiError::bad_request(
            "Post permalink phải bắt đầu bằng /, tối đa 512 bytes.",
        ));
    }
    let segs: Vec<_> = format[1..].split('/').collect();
    if segs.is_empty() || segs.len() > 8 {
        return Err(ApiError::bad_request(
            "Post permalink phải có từ 1 đến 8 segments.",
        ));
    }
    let token = *segs.last().unwrap();
    if token != "%postname%" && token != "%post_id%" {
        return Err(ApiError::bad_request(
            "Post permalink phải kết thúc bằng %postname% hoặc %post_id%.",
        ));
    }
    let mut category_count = 0usize;
    for &seg in &segs[..segs.len() - 1] {
        if seg == "%category%" {
            category_count += 1;
            if category_count > 1 {
                return Err(ApiError::bad_request(
                    "Post permalink chỉ hỗ trợ tối đa một thẻ %category%.",
                ));
            }
        } else if !valid_kebab_segment_with_limit(seg, 64) {
            return Err(ApiError::bad_request(
                "Post permalink chỉ hỗ trợ literal kebab-case, tối đa một thẻ %category% và thẻ cuối %postname% hoặc %post_id% (không dùng ngày/tháng/năm).",
            ));
        }
    }
    if segs.len() > 1
        && segs[0] != "%category%"
        && crate::storage::seo::is_reserved_route_root(segs[0])
    {
        return Err(ApiError::bad_request(
            "Post permalink dùng tiền tố hệ thống được bảo vệ.",
        ));
    }
    Ok(())
}

pub(in crate::storage) async fn validate_input_in_tx(
    conn: &mut SqliteConnection,
    input: &PermalinkSettingsInput,
) -> ApiResult<()> {
    validate_post_structure(&input.post_structure)?;
    let persisted = read_settings(conn).await?;
    for (key, base) in &input.content_type_bases {
        if !valid_content_type_key(key) || matches!(key.as_str(), "pattern" | "section") {
            return Err(ApiError::bad_request(format!(
                "CPT key không hợp lệ: {key}"
            )));
        }
        if matches!(key.as_str(), "post" | "page") && base.is_some() {
            return Err(ApiError::bad_request(format!(
                "Post/Page giữ route riêng theo chuẩn WordPress, không nhận rewrite_base: {key}",
            )));
        }
        let exists: Option<(String,)> = sqlx::query_as(
            "SELECT key FROM content_types WHERE key=? AND provider_status='active'
               AND COALESCE(json_extract(definition_json,'$.settings.public'),1)=1",
        )
        .bind(key)
        .fetch_optional(&mut *conn)
        .await?;
        if exists.is_none() && persisted.content_type_bases.get(key) != Some(base) {
            return Err(ApiError::bad_request(format!(
                "CPT không tồn tại/không public: {key}"
            )));
        }
        if base.as_ref().is_some_and(|b| !valid_base(b)) {
            return Err(ApiError::bad_request(format!(
                "CPT rewrite_base không hợp lệ: {key}"
            )));
        }
    }
    for (key, base) in &input.taxonomy_bases {
        if !valid_content_type_key(key) {
            return Err(ApiError::bad_request(format!(
                "Taxonomy key không hợp lệ: {key}"
            )));
        }
        let exists: Option<(String,)> = sqlx::query_as(
            "SELECT key FROM taxonomy_definitions WHERE key=? AND public=1
               AND provider_status='active' AND declaration_state='declared'",
        )
        .bind(key)
        .fetch_optional(&mut *conn)
        .await?;
        if exists.is_none() && persisted.taxonomy_bases.get(key) != Some(base) {
            return Err(ApiError::bad_request(format!(
                "Taxonomy không tồn tại/không public: {key}"
            )));
        }
        if base.as_ref().is_some_and(|b| !valid_base(b)) {
            return Err(ApiError::bad_request(format!(
                "Taxonomy rewrite_base không hợp lệ: {key}"
            )));
        }
    }
    Ok(())
}

impl DynamicPattern {
    pub(in crate::storage) fn matches(&self, path: &str) -> bool {
        let segments: Vec<_> = path.trim_start_matches('/').split('/').collect();
        segments.len() == self.seg_count
            && segments.starts_with(&self.prefix.iter().map(String::as_str).collect::<Vec<_>>())
            && !segments.last().is_some_and(|last| last.is_empty())
    }

    pub(in crate::storage) fn describe(&self) -> String {
        format!(
            "{}/%slug%",
            if self.prefix.is_empty() {
                String::new()
            } else {
                format!("/{}", self.prefix.join("/"))
            }
        )
    }
}

pub(in crate::storage) fn pattern_for_locale(
    relative_template: &str,
    locale: &str,
    default_locale: &str,
    kind: &str,
    family: &str,
) -> Option<DynamicPattern> {
    // Evaluate literal prefix on the unprefixed template first so wildcard
    // roots like /%postname% do not turn /<locale> into a catch-all pattern.
    let rel_prefix_has_literal = relative_template
        .trim_start_matches('/')
        .split('/')
        .next()
        .is_some_and(|seg| !seg.is_empty() && !seg.starts_with('%'));
    if !rel_prefix_has_literal {
        return None;
    }
    let template = prefix_for_locale(locale, default_locale, relative_template);
    let segments: Vec<&str> = template.trim_start_matches('/').split('/').collect();
    let seg_count = segments.len();
    let prefix: Vec<String> = segments
        .into_iter()
        .take_while(|seg| !seg.starts_with('%'))
        .map(str::to_owned)
        .collect();
    if prefix.is_empty() {
        return None;
    }
    Some(DynamicPattern {
        prefix,
        seg_count,
        kind: kind.to_owned(),
        family: family.to_owned(),
        locale: locale.to_owned(),
    })
}

pub(in crate::storage) fn patterns_collide(lhs: &DynamicPattern, rhs: &DynamicPattern) -> bool {
    lhs.seg_count == rhs.seg_count
        && (lhs.prefix.starts_with(&rhs.prefix) || rhs.prefix.starts_with(&lhs.prefix))
}

pub(in crate::storage) fn strip_locale_prefix<'a>(path: &'a str, locale: &str) -> &'a str {
    let prefix = format!("/{locale}");
    if path == prefix {
        "/"
    } else if path
        .strip_prefix(&prefix)
        .is_some_and(|rest| rest.starts_with('/'))
    {
        &path[prefix.len()..]
    } else {
        path
    }
}

pub(in crate::storage) fn rebase_path_for_swap(
    path: &str,
    locale: &str,
    old_default: Option<&str>,
    new_default: &str,
) -> String {
    if let Some(old_def) = old_default {
        if locale == old_def && locale != new_default {
            return prefix_for_locale(locale, new_default, path);
        }
        if locale == new_default && locale != old_def {
            return strip_locale_prefix(path, new_default).to_owned();
        }
    }
    path.to_owned()
}

/// This is the one intentional owner transfer in the permalink namespace:
/// the selected Page's entry URL becomes the post archive, and a former
/// Posts Page can reclaim its Page URL when changed/unset. All other cross-
/// target history remains collision protected.
pub(in crate::storage) fn reading_page_owner_transfer(
    old: &Claim,
    new: &Claim,
    reading_by_locale: &BTreeMap<String, (Option<i64>, Option<i64>)>,
    target_families: &BTreeMap<Claim, String>,
) -> bool {
    if old.2 != new.2 {
        return false;
    }
    let posts_page_id = reading_by_locale
        .get(&old.2)
        .and_then(|(_, posts_id)| *posts_id);
    let archive = ("archive".to_owned(), "post".to_owned(), old.2.clone());
    (old == &archive
        && new.0 == "entry"
        && target_families.get(new).is_some_and(|kind| kind == "page"))
        || (new == &archive
            && old.0 == "entry"
            && posts_page_id.is_some_and(|id| old.1 == id.to_string()))
}

pub(in crate::storage) fn first_literal_segment(path: &str) -> &str {
    path.trim_start_matches('/').split('/').next().unwrap_or("")
}

pub(in crate::storage) fn is_unpublished_entry(
    kind: &str,
    id: &str,
    unpublished: &HashSet<i64>,
) -> bool {
    kind == "entry"
        && id
            .parse::<i64>()
            .is_ok_and(|eid| unpublished.contains(&eid))
}

pub(in crate::storage) async fn category_is_eligible_for_post(
    conn: &mut SqliteConnection,
) -> ApiResult<bool> {
    let cat_public: i64 = sqlx::query_scalar(
        "SELECT EXISTS(
           SELECT 1 FROM taxonomy_definitions
            WHERE key = 'category' AND public = 1
              AND provider_status = 'active' AND declaration_state = 'declared'
         )",
    )
    .fetch_one(&mut *conn)
    .await?;
    if cat_public == 0 {
        return Ok(false);
    }
    let eligible = crate::storage::content::load_effective_taxonomies_in_tx(conn, "post").await?;
    Ok(eligible.iter().any(|k| k == "category"))
}

pub(in crate::storage) async fn load_post_category_slugs(
    conn: &mut SqliteConnection,
) -> ApiResult<BTreeMap<i64, String>> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT e.id,
                COALESCE(
                    (SELECT pc.slug
                       FROM entry_terms et
                       JOIN taxonomy_terms pc ON pc.id = et.term_id
                      WHERE et.entry_id = e.id
                        AND et.term_id = e.primary_category_term_id
                        AND pc.kind = 'category'
                        AND pc.locale = e.locale),
                    (SELECT t.slug
                       FROM entry_terms et
                       JOIN taxonomy_terms t ON t.id = et.term_id
                      WHERE et.entry_id = e.id
                        AND t.kind = 'category'
                        AND t.locale = e.locale
                      ORDER BY t.id ASC
                      LIMIT 1),
                    'uncategorized'
                ) AS category_slug
           FROM content_entries e
          WHERE e.content_type_key = 'post'",
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().collect())
}
