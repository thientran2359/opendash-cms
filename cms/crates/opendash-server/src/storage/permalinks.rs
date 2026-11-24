//! C3: transactional authority for canonical routes and their stable-ID history.
//! All writers share sync_claims_in_tx before committing their source changes.
use std::collections::{BTreeMap, HashSet};

use opendash_core::content::valid_content_type_key;
use opendash_core::id::valid_kebab_segment_with_limit;
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};

use crate::error::{ApiError, ApiResult};

const MAX_CLAIMS: usize = 200_000;
pub const MAX_REDIRECTS: usize = 200_000;
const MAX_CONFLICT_ITEMS: usize = 20;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PermalinkConflictItem {
    pub path: String,
    pub new_target: String,
    pub existing_target: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PermalinkConflictError {
    pub error: String,
    pub total_conflicts: usize,
    pub truncated: bool,
    pub conflicts: Vec<PermalinkConflictItem>,
}

#[derive(Debug)]
pub enum PermalinkPutError {
    Api(ApiError),
    Conflicts(PermalinkConflictError),
}

impl From<ApiError> for PermalinkPutError {
    fn from(err: ApiError) -> Self {
        Self::Api(err)
    }
}

impl PermalinkPutError {
    #[allow(dead_code)]
    #[cfg(test)]
    pub fn status(&self) -> axum::http::StatusCode {
        match self {
            Self::Api(err) => err.0,
            Self::Conflicts(_) => axum::http::StatusCode::CONFLICT,
        }
    }

    pub(crate) fn into_api_error(self) -> ApiError {
        match self {
            Self::Api(err) => err,
            Self::Conflicts(c) => ApiError::conflict(c.error),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermalinkSettingsInput {
    pub post_structure: String,
    pub content_type_bases: BTreeMap<String, Option<String>>,
    pub taxonomy_bases: BTreeMap<String, Option<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermalinkSettings {
    pub version: i64,
    pub post_structure: String,
    pub content_type_bases: BTreeMap<String, Option<String>>,
    pub taxonomy_bases: BTreeMap<String, Option<String>>,
}

impl PermalinkSettings {
    #[cfg(test)]
    fn input(&self) -> PermalinkSettingsInput {
        PermalinkSettingsInput {
            post_structure: self.post_structure.clone(),
            content_type_bases: self.content_type_bases.clone(),
            taxonomy_bases: self.taxonomy_bases.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PublicRouteTarget {
    Home {
        locale: String,
    },
    Entry {
        id: i64,
        locale: String,
        is_home: bool,
    },
    Term {
        id: i64,
        locale: String,
    },
    Archive {
        type_key: String,
        locale: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedPublicRoute {
    pub target: PublicRouteTarget,
    pub canonical_path: String,
    pub redirect: bool,
}

type Claim = (String, String, String); // (stable target kind, stable target ID, locale)

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

async fn read_settings(conn: &mut SqliteConnection) -> ApiResult<PermalinkSettings> {
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

/// An immutable-owner package declaration. `None` withdraws the default base,
/// but never creates or overwrites a site-level selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderRewriteDeclaration {
    pub resource_kind: &'static str,
    pub resource_key: String,
    pub rewrite_base: Option<String>,
}

/// The external C3 settings DTO contains only explicit site selections. Its
/// sparse maps intentionally distinguish an omitted key from an explicit null.
/// Overlay active provider defaults privately for canonical routing only.
async fn read_effective_settings(conn: &mut SqliteConnection) -> ApiResult<PermalinkSettings> {
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

async fn invalidate_if_effective_changed(
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

fn valid_base(base: &str) -> bool {
    let segs: Vec<_> = base.split('/').collect();
    !segs.is_empty()
        && segs.len() <= 3
        && segs
            .iter()
            .all(|seg| valid_kebab_segment_with_limit(seg, 64))
        && !crate::storage::seo::is_reserved_route_root(segs[0])
}

fn validate_post_structure(format: &str) -> ApiResult<()> {
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

async fn validate_input_in_tx(
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

/// Serialized writer, optimistic configuration CAS, and whole-namespace validation.
pub async fn put_settings(
    db: &SqlitePool,
    expected_version: i64,
    input: &PermalinkSettingsInput,
) -> Result<PermalinkSettings, PermalinkPutError> {
    let mut tx = db.begin().await.map_err(ApiError::from)?;
    sqlx::query("UPDATE bootstrap SET initialized=initialized WHERE id=1")
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;
    let current = read_settings(&mut tx).await?;
    if current.version != expected_version {
        return Err(
            ApiError::precondition_failed("Permalink settings đã thay đổi; hãy tải lại.").into(),
        );
    }
    validate_input_in_tx(&mut tx, input).await?;
    let cpt_json = serde_json::to_string(&input.content_type_bases).map_err(ApiError::from)?;
    let taxonomy_json = serde_json::to_string(&input.taxonomy_bases).map_err(ApiError::from)?;
    let updated = sqlx::query(
        "UPDATE site_permalink_settings SET version=version+1, post_structure=?,
           content_type_bases_json=?, taxonomy_bases_json=? WHERE id=1 AND version=?",
    )
    .bind(&input.post_structure)
    .bind(cpt_json)
    .bind(taxonomy_json)
    .bind(expected_version)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::from)?;
    if updated.rows_affected() != 1 {
        return Err(
            ApiError::precondition_failed("Permalink settings đã thay đổi; hãy tải lại.").into(),
        );
    }
    sync_claims_with_options(&mut tx, true).await?;
    let output = read_settings(&mut tx).await?;
    tx.commit().await.map_err(ApiError::from)?;
    Ok(output)
}

fn entry_path(
    settings: &PermalinkSettings,
    id: i64,
    key: &str,
    slug: &str,
    custom: Option<&str>,
    primary_category_slug: Option<&str>,
) -> String {
    if let Some(path) = custom {
        return path.to_owned();
    }
    if key == "page" {
        return format!("/{slug}");
    }
    if key == "post" {
        let cat = primary_category_slug.unwrap_or("uncategorized");
        return settings
            .post_structure
            .replace("%category%", cat)
            .replace("%postname%", slug)
            .replace("%post_id%", &id.to_string());
    }
    if let Some(Some(base)) = settings.content_type_bases.get(key) {
        return format!("/{base}/{slug}");
    }
    format!("/{key}/{slug}")
}

fn term_path(settings: &PermalinkSettings, id: i64, kind: &str, slug: &str) -> String {
    let _ = id;
    match settings.taxonomy_bases.get(kind) {
        Some(Some(base)) => format!("/{base}/{slug}"),
        _ => format!("/{kind}/{slug}"),
    }
}

fn archive_path(settings: &PermalinkSettings, key: &str) -> String {
    if key == "post" {
        return "/posts".to_owned();
    }
    match settings.content_type_bases.get(key) {
        Some(Some(base)) => format!("/{base}"),
        _ => format!("/{key}"),
    }
}

pub async fn canonical_entry_path_in_tx(
    conn: &mut SqliteConnection,
    id: i64,
) -> ApiResult<Option<String>> {
    sqlx::query_scalar(
        "SELECT c.path FROM permalink_route_claims c
           JOIN content_entries e ON c.target_kind='entry' AND c.target_id=CAST(e.id AS TEXT)
            AND c.locale = e.locale
           JOIN site_locales sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
           JOIN content_types t ON t.key=e.content_type_key
          WHERE e.id=? AND e.content_type_key NOT IN ('pattern', 'section')
            AND t.provider_status='active'
            AND COALESCE(json_extract(t.definition_json,'$.settings.public'),1)=1",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Into::into)
}

pub async fn canonical_term_path_in_tx(
    conn: &mut SqliteConnection,
    id: i64,
) -> ApiResult<Option<String>> {
    sqlx::query_scalar(
        "SELECT c.path FROM permalink_route_claims c
           JOIN taxonomy_terms t ON c.target_kind='term' AND c.target_id=CAST(t.id AS TEXT)
            AND c.locale = t.locale
           JOIN site_locales sl ON sl.code = t.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
           JOIN taxonomy_definitions d ON d.key=t.kind
          WHERE t.id=? AND d.public=1 AND d.provider_status='active'
            AND d.declaration_state='declared'",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Into::into)
}

pub async fn batch_canonical_term_paths_in_tx(
    conn: &mut SqliteConnection,
    term_ids: &[i64],
) -> ApiResult<BTreeMap<i64, String>> {
    let mut out = BTreeMap::new();
    if term_ids.is_empty() {
        return Ok(out);
    }
    for chunk in term_ids.chunks(500) {
        let ids_json = serde_json::to_string(chunk)?;
        let rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT t.id, c.path
               FROM permalink_route_claims c
               JOIN taxonomy_terms t
                 ON c.target_kind = 'term'
                AND c.target_id = CAST(t.id AS TEXT)
                AND c.locale = t.locale
               JOIN site_locales sl ON sl.code = t.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
               JOIN taxonomy_definitions d ON d.key = t.kind
              WHERE t.id IN (SELECT value FROM json_each(?))
                AND d.public = 1
                AND d.provider_status = 'active'
                AND d.declaration_state = 'declared'",
        )
        .bind(ids_json)
        .fetch_all(&mut *conn)
        .await?;
        for (tid, path) in rows {
            out.insert(tid, path);
        }
    }
    Ok(out)
}

pub async fn canonical_archive_path_in_tx(
    conn: &mut SqliteConnection,
    key: &str,
    locale: &str,
) -> ApiResult<Option<String>> {
    sqlx::query_scalar(
        "SELECT c.path FROM permalink_route_claims c
           JOIN content_types t ON c.target_kind='archive' AND c.target_id=t.key
           JOIN site_locales sl ON sl.code = c.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
          WHERE t.key=? AND c.locale=? AND t.key NOT IN ('pattern', 'section', 'page')
            AND t.provider_status='active'
            AND COALESCE(json_extract(t.definition_json,'$.settings.public'),1)=1
            AND COALESCE(json_extract(t.definition_json,'$.settings.has_archive'),1)=1",
    )
    .bind(key)
    .bind(locale)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Into::into)
}

#[derive(Debug, Clone)]
struct ProposedClaim {
    kind: String,
    id: String,
    locale: String,
    family: Option<String>,
    generated: bool,
}

#[derive(Debug)]
struct DynamicPattern {
    prefix: Vec<String>,
    seg_count: usize,
    kind: String,
    family: String,
    locale: String,
}

impl DynamicPattern {
    fn matches(&self, path: &str) -> bool {
        let segments: Vec<_> = path.trim_start_matches('/').split('/').collect();
        segments.len() == self.seg_count
            && segments.starts_with(&self.prefix.iter().map(String::as_str).collect::<Vec<_>>())
            && !segments.last().is_some_and(|last| last.is_empty())
    }

    fn describe(&self) -> String {
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

fn pattern_for_locale(
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

fn patterns_collide(lhs: &DynamicPattern, rhs: &DynamicPattern) -> bool {
    lhs.seg_count == rhs.seg_count
        && (lhs.prefix.starts_with(&rhs.prefix) || rhs.prefix.starts_with(&lhs.prefix))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HistoricalOwner {
    claim: Claim,
    origin: String,
}

fn strip_locale_prefix<'a>(path: &'a str, locale: &str) -> &'a str {
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

fn rebase_path_for_swap(
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

struct ConflictCollector {
    collect_all: bool,
    total_conflicts: usize,
    items: Vec<PermalinkConflictItem>,
    first_error: Option<String>,
}

impl ConflictCollector {
    fn new(collect_all: bool) -> Self {
        Self {
            collect_all,
            total_conflicts: 0,
            items: Vec::new(),
            first_error: None,
        }
    }

    fn record(
        &mut self,
        path: impl Into<String>,
        new_target: impl Into<String>,
        existing_target: impl Into<String>,
        message: String,
    ) -> Result<(), PermalinkPutError> {
        if !self.collect_all {
            return Err(PermalinkPutError::Api(ApiError::conflict(message)));
        }
        self.total_conflicts += 1;
        if self.first_error.is_none() {
            self.first_error = Some(message);
        }
        if self.items.len() < MAX_CONFLICT_ITEMS {
            self.items.push(PermalinkConflictItem {
                path: path.into(),
                new_target: new_target.into(),
                existing_target: existing_target.into(),
            });
        }
        Ok(())
    }

    fn finish(self) -> Result<(), PermalinkPutError> {
        if self.total_conflicts == 0 {
            return Ok(());
        }
        Err(PermalinkPutError::Conflicts(PermalinkConflictError {
            error: self.first_error.unwrap_or_else(|| {
                format!("Phát hiện {} xung đột đường dẫn.", self.total_conflicts)
            }),
            total_conflicts: self.total_conflicts,
            truncated: self.total_conflicts > self.items.len(),
            conflicts: self.items,
        }))
    }
}

#[allow(clippy::too_many_arguments)]
fn add_claim(
    all: &mut BTreeMap<String, ProposedClaim>,
    path: String,
    kind: &str,
    id: String,
    locale: &str,
    family: Option<String>,
    generated: bool,
    collector: &mut ConflictCollector,
) -> Result<(), PermalinkPutError> {
    if path.len() > 512
        || !path.starts_with('/')
        || path.contains('?')
        || path.contains('#')
        || path.contains('\\')
        || path.contains("//")
    {
        return Err(PermalinkPutError::Api(ApiError::conflict(format!(
            "permalink_invalid_claim:path={path} target={kind}:{id}:{locale}"
        ))));
    }
    if let Some(previous) = all.get(&path) {
        collector.record(
            &path,
            format!("{kind}:{id}:{locale}"),
            format!("{}:{}:{}", previous.kind, previous.id, previous.locale),
            format!(
                "permalink_collision:path={path} new={kind}:{id}({locale}) existing={}:{}({})",
                previous.kind, previous.id, previous.locale,
            ),
        )?;
        return Ok(());
    }
    all.insert(
        path,
        ProposedClaim {
            kind: kind.to_owned(),
            id,
            locale: locale.to_owned(),
            family,
            generated,
        },
    );
    if all.len() > MAX_CLAIMS {
        return Err(PermalinkPutError::Api(ApiError::conflict(
            "permalink_claim_limit: tối đa 200000 canonical claims.",
        )));
    }
    Ok(())
}

/// This is the one intentional owner transfer in the permalink namespace:
/// the selected Page's entry URL becomes the post archive, and a former
/// Posts Page can reclaim its Page URL when changed/unset. All other cross-
/// target history remains collision protected.
fn reading_page_owner_transfer(
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

fn first_literal_segment(path: &str) -> &str {
    path.trim_start_matches('/').split('/').next().unwrap_or("")
}

fn is_unpublished_entry(kind: &str, id: &str, unpublished: &HashSet<i64>) -> bool {
    kind == "entry"
        && id
            .parse::<i64>()
            .is_ok_and(|eid| unpublished.contains(&eid))
}

async fn category_is_eligible_for_post(conn: &mut SqliteConnection) -> ApiResult<bool> {
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

async fn load_post_category_slugs(conn: &mut SqliteConnection) -> ApiResult<BTreeMap<i64, String>> {
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

/// Always called while the caller already holds SQLite's single-writer
/// reservation. Any failure (including a namespace conflict) rolls back the
/// caller's data mutation as well as the proposed claims.
pub async fn sync_claims_in_tx(conn: &mut SqliteConnection) -> ApiResult<()> {
    sync_claims_with_options(conn, false)
        .await
        .map_err(PermalinkPutError::into_api_error)
}

pub(crate) async fn sync_claims_with_options(
    conn: &mut SqliteConnection,
    collect_conflicts: bool,
) -> Result<(), PermalinkPutError> {
    sync_claims_with_options_and_swap(conn, collect_conflicts, None).await
}

pub(crate) async fn sync_claims_with_options_and_swap(
    conn: &mut SqliteConnection,
    collect_conflicts: bool,
    swapped_from: Option<&str>,
) -> Result<(), PermalinkPutError> {
    let settings = read_effective_settings(conn).await?;
    let enabled_locales: Vec<(String, i64)> = sqlx::query_as(
        "SELECT code, is_default
           FROM site_locales
          WHERE enabled = 1 AND deleted_at IS NULL
            AND (is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
          ORDER BY is_default DESC, position ASC, code ASC",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(ApiError::from)?;
    let default_locale = crate::storage::settings::get_default_locale_code_in_conn(conn).await?;
    let enabled_set: HashSet<String> = enabled_locales.iter().map(|(c, _)| c.clone()).collect();
    let secondary_roots: HashSet<String> = enabled_locales
        .iter()
        .filter(|(c, is_def)| *is_def == 0 && c != &default_locale)
        .map(|(c, _)| c.clone())
        .collect();

    let reading_rows: Vec<(String, Option<i64>, Option<i64>)> =
        sqlx::query_as("SELECT locale, homepage_page_id, posts_page_id FROM site_reading_settings")
            .fetch_all(&mut *conn)
            .await
            .map_err(ApiError::from)?;
    let reading_by_locale: BTreeMap<String, (Option<i64>, Option<i64>)> = reading_rows
        .into_iter()
        .map(|(loc, home, posts)| (loc, (home, posts)))
        .collect();

    let previous_root_locale: Option<String> =
        sqlx::query_scalar("SELECT locale FROM permalink_route_claims WHERE path = '/'")
            .fetch_optional(&mut *conn)
            .await
            .map_err(ApiError::from)?;
    let swap_old_default: Option<String> = swapped_from
        .filter(|old| *old != default_locale)
        .map(str::to_owned)
        .or_else(|| previous_root_locale.filter(|prev| prev != &default_locale));

    let mut collector = ConflictCollector::new(collect_conflicts);
    let mut proposed = BTreeMap::<String, ProposedClaim>::new();
    let mut target_families = BTreeMap::<Claim, String>::new();

    // A deleted/draft assigned Homepage keeps its locale root reserved as a tombstone ('entry', id, locale).
    for (loc, _) in &enabled_locales {
        let homepage_id = reading_by_locale.get(loc).and_then(|(home, _)| *home);
        let home_path = prefix_for_locale(loc, &default_locale, "/");
        match homepage_id {
            Some(id) => add_claim(
                &mut proposed,
                home_path,
                "entry",
                id.to_string(),
                loc,
                Some("page".into()),
                true,
                &mut collector,
            )?,
            None => add_claim(
                &mut proposed,
                home_path,
                "home",
                "site".into(),
                loc,
                None,
                true,
                &mut collector,
            )?,
        }
    }

    let cpts: Vec<(String, i64)> = sqlx::query_as(
        "SELECT key, COALESCE(json_extract(definition_json,'$.settings.has_archive'),1)
           FROM content_types
          WHERE key NOT IN ('pattern', 'section', 'page')
            AND provider_status = 'active'
            AND COALESCE(json_extract(definition_json,'$.settings.public'),1)=1
          ORDER BY key",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(ApiError::from)?;
    let taxonomies: Vec<(String,)> = sqlx::query_as(
        "SELECT key FROM taxonomy_definitions
          WHERE public = 1 AND provider_status = 'active' AND declaration_state = 'declared'
          ORDER BY key",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(ApiError::from)?;

    // Build dynamic patterns per enabled locale for templates with a relative literal prefix.
    let mut patterns = Vec::<DynamicPattern>::new();
    for (loc, _) in &enabled_locales {
        if let Some(p) = pattern_for_locale(
            &settings.post_structure,
            loc,
            &default_locale,
            "entry",
            "post",
        ) {
            patterns.push(p);
        }
        for (key, _) in &cpts {
            if key != "post" {
                let generated = format!("{}/%postname%", archive_path(&settings, key));
                if let Some(p) = pattern_for_locale(&generated, loc, &default_locale, "entry", key)
                {
                    patterns.push(p);
                }
            }
        }
        for (key,) in &taxonomies {
            let template = match settings.taxonomy_bases.get(key) {
                Some(Some(base)) => format!("/{base}/%term%"),
                _ => format!("/{key}/%term%"),
            };
            if let Some(p) = pattern_for_locale(&template, loc, &default_locale, "term", key) {
                patterns.push(p);
            }
        }
    }
    for pattern in &patterns {
        if pattern.locale == default_locale
            && let Some(first) = pattern.prefix.first()
            && is_secondary_locale_root(first, &secondary_roots)
        {
            collector.record(
                pattern.describe(),
                format!("{}:{}", pattern.kind, pattern.family),
                format!("locale:{first}"),
                format!(
                    "permalink_reserved_collision:path={} target={}:{}",
                    pattern.describe(),
                    pattern.kind,
                    pattern.family,
                ),
            )?;
        }
    }
    for (i, lhs) in patterns.iter().enumerate() {
        for rhs in patterns.iter().skip(i + 1) {
            if patterns_collide(lhs, rhs) {
                collector.record(
                    lhs.describe(),
                    format!("{}:{}", lhs.kind, lhs.family),
                    format!("{}:{}", rhs.kind, rhs.family),
                    format!(
                        "permalink_pattern_collision:{}:{} and {}:{} at {}",
                        lhs.kind,
                        lhs.family,
                        rhs.kind,
                        rhs.family,
                        lhs.describe(),
                    ),
                )?;
            }
        }
    }

    // When %category% leads post_structure (segment 0), guard against reserved roots,
    // /posts (default archive), active secondary locale roots (on default_locale),
    // and active public CPT / Taxonomy leading bases.
    if settings.post_structure.starts_with("/%category%/") {
        let mut forbidden_by_locale = BTreeMap::<String, BTreeMap<String, String>>::new();
        for (loc, _) in &enabled_locales {
            let posts_page_id = reading_by_locale.get(loc).and_then(|(_, posts)| *posts);
            let mut forbidden_cat_first_segs = BTreeMap::<String, String>::new();
            if posts_page_id.is_none() {
                forbidden_cat_first_segs.insert("posts".into(), "archive:post".into());
            }
            for (key, _) in &cpts {
                if key != "post" {
                    let first = first_literal_segment(&archive_path(&settings, key)).to_owned();
                    if !first.is_empty() {
                        forbidden_cat_first_segs
                            .entry(first)
                            .or_insert_with(|| format!("cpt:{key}"));
                    }
                }
            }
            for (key,) in &taxonomies {
                let base_path = match settings.taxonomy_bases.get(key) {
                    Some(Some(base)) => format!("/{base}"),
                    _ => format!("/{key}"),
                };
                let first = first_literal_segment(&base_path).to_owned();
                if !first.is_empty() {
                    forbidden_cat_first_segs
                        .entry(first)
                        .or_insert_with(|| format!("taxonomy:{key}"));
                }
            }
            forbidden_by_locale.insert(loc.clone(), forbidden_cat_first_segs);
        }
        let category_rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT t.slug, t.locale
               FROM taxonomy_terms t
               JOIN site_locales sl ON sl.code = t.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
              WHERE t.kind = 'category'
              ORDER BY t.id",
        )
        .fetch_all(&mut *conn)
        .await
        .map_err(ApiError::from)?;
        let mut category_slugs = category_rows;
        for (loc, _) in &enabled_locales {
            category_slugs.push(("uncategorized".to_owned(), loc.clone()));
        }
        for (slug, loc) in category_slugs {
            let cat_path = prefix_for_locale(&loc, &default_locale, &format!("/{slug}"));
            if crate::storage::seo::is_reserved_route_root(&slug) {
                collector.record(
                    cat_path.clone(),
                    "post_structure:%category%",
                    format!("reserved:{slug}"),
                    format!(
                        "permalink_category_root_collision:path={cat_path} existing=reserved:{slug}"
                    ),
                )?;
            } else if loc == default_locale && is_secondary_locale_root(&slug, &secondary_roots) {
                collector.record(
                    cat_path.clone(),
                    "post_structure:%category%",
                    format!("locale:{slug}"),
                    format!(
                        "permalink_category_root_collision:path={cat_path} existing=locale:{slug}"
                    ),
                )?;
            } else if let Some(existing) =
                forbidden_by_locale.get(&loc).and_then(|map| map.get(&slug))
            {
                collector.record(
                    cat_path.clone(),
                    "post_structure:%category%",
                    existing,
                    format!(
                        "permalink_category_root_collision:path={cat_path} existing={existing}"
                    ),
                )?;
            }
        }
    }

    // Resolve Posts Page custom/slug path per enabled locale before inserting Archives ahead of Entries.
    let prior_post_archives: Vec<(String, String)> = sqlx::query_as(
        "SELECT locale, path FROM permalink_route_claims
          WHERE target_kind='archive' AND target_id='post'",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(ApiError::from)?;
    let prior_post_archive_map: BTreeMap<String, String> =
        prior_post_archives.into_iter().collect();

    for (loc, _) in &enabled_locales {
        let posts_page_id = reading_by_locale.get(loc).and_then(|(_, posts)| *posts);
        let mut posts_page_path = None::<String>;
        if let Some(page_id) = posts_page_id {
            let row: Option<(String, Option<String>)> = sqlx::query_as(
                "SELECT e.slug, p.path
                   FROM content_entries e
                   LEFT JOIN entry_public_paths p ON p.entry_id = e.id
                  WHERE e.id = ? AND e.content_type_key = 'page' AND e.locale = ?",
            )
            .bind(page_id)
            .bind(loc)
            .fetch_optional(&mut *conn)
            .await
            .map_err(ApiError::from)?;
            if let Some((slug, custom)) = row {
                if let Some(c) = &custom {
                    crate::storage::seo::validate_c3_custom_entry_path(c).map_err(|e| {
                        ApiError::conflict(format!("invalid_custom_path:entry:{page_id}: {e}"))
                    })?;
                }
                let rel = entry_path(&settings, page_id, "page", &slug, custom.as_deref(), None);
                posts_page_path = Some(prefix_for_locale(loc, &default_locale, &rel));
            }
        }
        let prior_post_archive_path = prior_post_archive_map
            .get(loc)
            .map(|p| rebase_path_for_swap(p, loc, swap_old_default.as_deref(), &default_locale));
        let posts_archive_path = if posts_page_id.is_some() {
            posts_page_path.or(prior_post_archive_path)
        } else {
            None
        };

        // Archives & Terms are claimed before Entries so drafts cannot squat /posts or /category/{slug}.
        for (key, archive_enabled) in &cpts {
            if *archive_enabled != 0 {
                let current_path = if key == "post" {
                    posts_archive_path.clone().unwrap_or_else(|| {
                        prefix_for_locale(loc, &default_locale, &archive_path(&settings, key))
                    })
                } else {
                    prefix_for_locale(loc, &default_locale, &archive_path(&settings, key))
                };
                add_claim(
                    &mut proposed,
                    current_path,
                    "archive",
                    key.clone(),
                    loc,
                    None,
                    true,
                    &mut collector,
                )?;
            }
        }
    }

    let terms: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT t.id, t.kind, t.slug, t.locale
           FROM taxonomy_terms t
           JOIN site_locales sl ON sl.code = t.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
           JOIN taxonomy_definitions d ON d.key = t.kind
          WHERE d.public = 1 AND d.provider_status = 'active' AND d.declaration_state = 'declared'
          ORDER BY t.id",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(ApiError::from)?;
    for (id, kind, slug, loc) in terms {
        target_families.insert(
            ("term".to_owned(), id.to_string(), loc.clone()),
            kind.clone(),
        );
        let rel = term_path(&settings, id, &kind, &slug);
        let current_path = prefix_for_locale(&loc, &default_locale, &rel);
        add_claim(
            &mut proposed,
            current_path,
            "term",
            id.to_string(),
            &loc,
            Some(kind),
            true,
            &mut collector,
        )?;
    }

    let post_cat_slugs = if settings.post_structure.contains("%category%")
        && category_is_eligible_for_post(conn).await?
    {
        load_post_category_slugs(conn).await?
    } else {
        BTreeMap::new()
    };

    let entries: Vec<(i64, String, String, Option<String>, i64, String)> = sqlx::query_as(
        "SELECT e.id, e.content_type_key, e.slug, p.path, e.has_been_published, e.locale
           FROM content_entries e
           JOIN site_locales sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
           JOIN content_types t ON t.key = e.content_type_key
           LEFT JOIN entry_public_paths p ON p.entry_id = e.id
          WHERE e.content_type_key NOT IN ('pattern', 'section')
            AND t.provider_status = 'active'
            AND COALESCE(json_extract(t.definition_json,'$.settings.public'),1)=1
          ORDER BY e.id",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(ApiError::from)?;

    let mut homepage_redirects = Vec::<(String, Claim)>::new();
    let mut reclaimable_page_aliases = Vec::<(String, Claim)>::new();
    for (id, key, slug, custom, has_been_published, loc) in entries {
        target_families.insert(
            ("entry".to_owned(), id.to_string(), loc.clone()),
            key.clone(),
        );
        if let Some(custom) = &custom {
            crate::storage::seo::validate_c3_custom_entry_path(custom)
                .map_err(|e| ApiError::conflict(format!("invalid_custom_path:entry:{id}: {e}")))?;
        }
        let (homepage_id, posts_page_id) =
            reading_by_locale.get(&loc).copied().unwrap_or((None, None));
        if key == "page" && Some(id) == posts_page_id {
            continue;
        }
        let cat_slug = if key == "post" {
            Some(
                post_cat_slugs
                    .get(&id)
                    .map(String::as_str)
                    .unwrap_or("uncategorized"),
            )
        } else {
            None
        };
        let rel_current = entry_path(&settings, id, &key, &slug, custom.as_deref(), cat_slug);
        let current_path = prefix_for_locale(&loc, &default_locale, &rel_current);
        let home_path = prefix_for_locale(&loc, &default_locale, "/");
        if key == "page" && Some(id) == homepage_id {
            if has_been_published != 0 && current_path != home_path {
                homepage_redirects.push((current_path, ("entry".into(), id.to_string(), loc)));
            }
            continue;
        }
        let rel_generated = entry_path(&settings, id, &key, &slug, None, cat_slug);
        let generated_path = prefix_for_locale(&loc, &default_locale, &rel_generated);
        let generated = custom.as_ref().is_none_or(|p| p == &rel_generated);
        if key == "page"
            && Some(id) != posts_page_id
            && Some(id) != homepage_id
            && has_been_published != 0
            && !generated
        {
            reclaimable_page_aliases.push((
                generated_path,
                ("entry".to_owned(), id.to_string(), loc.clone()),
            ));
        }
        add_claim(
            &mut proposed,
            current_path,
            "entry",
            id.to_string(),
            &loc,
            Some(key),
            generated,
            &mut collector,
        )?;
    }

    for (path, proposed_claim) in &proposed {
        let (homepage_id, posts_page_id) = reading_by_locale
            .get(&proposed_claim.locale)
            .copied()
            .unwrap_or((None, None));
        let locale_home_path = prefix_for_locale(&proposed_claim.locale, &default_locale, "/");
        if path == &locale_home_path
            && (proposed_claim.kind == "home"
                || (proposed_claim.kind == "entry"
                    && homepage_id.is_some_and(|id| proposed_claim.id == id.to_string())))
        {
            continue;
        }
        if proposed_claim.locale == default_locale {
            let root = first_literal_segment(path);
            if crate::storage::seo::is_reserved_route_root(root) {
                collector.record(
                    path,
                    format!("{}:{}", proposed_claim.kind, proposed_claim.id),
                    format!("reserved:{root}"),
                    format!(
                        "permalink_reserved_collision:path={path} target={}:{}",
                        proposed_claim.kind, proposed_claim.id,
                    ),
                )?;
            } else if is_secondary_locale_root(root, &secondary_roots) {
                collector.record(
                    path,
                    format!("{}:{}", proposed_claim.kind, proposed_claim.id),
                    format!("locale:{root}"),
                    format!(
                        "permalink_reserved_collision:path={path} target={}:{} locale={root}",
                        proposed_claim.kind, proposed_claim.id,
                    ),
                )?;
            }
        } else {
            let rel = strip_locale_prefix(path, &proposed_claim.locale);
            let rel_root = first_literal_segment(rel);
            if rel != "/" && crate::storage::seo::is_reserved_route_root(rel_root) {
                collector.record(
                    path,
                    format!("{}:{}", proposed_claim.kind, proposed_claim.id),
                    format!("reserved:{rel_root}"),
                    format!(
                        "permalink_reserved_collision:path={path} target={}:{}",
                        proposed_claim.kind, proposed_claim.id,
                    ),
                )?;
            }
        }
        let default_posts_path =
            prefix_for_locale(&proposed_claim.locale, &default_locale, "/posts");
        if proposed_claim.kind != "archive"
            && path == &default_posts_path
            && posts_page_id.is_none()
        {
            collector.record(
                path,
                format!("{}:{}", proposed_claim.kind, proposed_claim.id),
                "archive:post",
                format!(
                    "permalink_collision:path={path} new={}:{} existing=archive:post",
                    proposed_claim.kind, proposed_claim.id,
                ),
            )?;
        }
        for pattern in &patterns {
            if pattern.matches(path)
                && !(proposed_claim.generated
                    && proposed_claim.kind == pattern.kind
                    && proposed_claim.locale == pattern.locale
                    && proposed_claim.family.as_deref() == Some(&pattern.family))
            {
                collector.record(
                    path,
                    format!("{}:{}", proposed_claim.kind, proposed_claim.id),
                    format!("{}:{}", pattern.kind, pattern.family),
                    format!(
                        "permalink_pattern_collision:path={path} target={}:{} pattern={}:{}",
                        proposed_claim.kind, proposed_claim.id, pattern.kind, pattern.family,
                    ),
                )?;
            }
        }
    }

    let unpublished_rows: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM content_entries WHERE has_been_published = 0")
            .fetch_all(&mut *conn)
            .await
            .map_err(ApiError::from)?;
    let unpublished_ids: HashSet<i64> = unpublished_rows.into_iter().collect();

    let previous: Vec<(String, String, String, String)> =
        sqlx::query_as("SELECT path,target_kind,target_id,locale FROM permalink_route_claims")
            .fetch_all(&mut *conn)
            .await
            .map_err(ApiError::from)?;
    let old_redirects: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT source_path,target_kind,target_id,locale,origin
           FROM permalink_route_redirects
          ORDER BY created_at ASC, source_path ASC",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(ApiError::from)?;
    // Soft-deleted locale history is parked until revive/purge. It must
    // neither participate in collision detection nor be deleted as stale.
    let soft_deleted_codes: HashSet<String> =
        sqlx::query_scalar("SELECT code FROM site_locales WHERE deleted_at IS NOT NULL")
            .fetch_all(&mut *conn)
            .await
            .map_err(ApiError::from)?
            .into_iter()
            .collect();
    let old_redirects: Vec<_> = old_redirects
        .into_iter()
        .filter(|(_, _, _, locale, _)| !soft_deleted_codes.contains(locale))
        .collect();
    let old_redirect_order: Vec<String> = old_redirects
        .iter()
        .map(|(p, _, _, _, _)| p.clone())
        .collect();
    let previous_history: BTreeMap<String, HistoricalOwner> = old_redirects
        .iter()
        .map(|(path, kind, id, loc, origin)| {
            (
                path.clone(),
                HistoricalOwner {
                    claim: (kind.clone(), id.clone(), loc.clone()),
                    origin: origin.clone(),
                },
            )
        })
        .collect();
    let mut historical = BTreeMap::<String, HistoricalOwner>::new();
    for (path, kind, id, loc, origin) in old_redirects {
        if path == "/" || is_unpublished_entry(&kind, &id, &unpublished_ids) {
            continue;
        }
        let posts_page_id = reading_by_locale.get(&loc).and_then(|(_, posts)| *posts);
        let claim = if kind == "entry" && posts_page_id.is_some_and(|page| id == page.to_string()) {
            ("archive".into(), "post".into(), loc)
        } else {
            (kind, id, loc)
        };
        historical.insert(path, HistoricalOwner { claim, origin });
    }

    let old_entry_redirects: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT locale,source_path,entry_id FROM entry_redirects ORDER BY created_at ASC, source_path ASC",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(ApiError::from)?;
    for (loc, rel_path, id) in old_entry_redirects {
        if !enabled_set.contains(&loc) || unpublished_ids.contains(&id) {
            continue;
        }
        let path = prefix_for_locale(&loc, &default_locale, &rel_path);
        if path == "/" || path == prefix_for_locale(&loc, &default_locale, "/") {
            continue;
        }
        let posts_page_id = reading_by_locale.get(&loc).and_then(|(_, posts)| *posts);
        let owner: Claim = if Some(id) == posts_page_id {
            ("archive".into(), "post".into(), loc.clone())
        } else {
            ("entry".to_owned(), id.to_string(), loc.clone())
        };
        let prefixed_variant = format!("/{loc}{rel_path}");
        let already_recorded_for_owner = [&rel_path, &prefixed_variant].iter().any(|candidate| {
            historical
                .get(*candidate)
                .is_some_and(|existing| existing.claim == owner && existing.origin == "canonical")
        });
        if already_recorded_for_owner {
            continue;
        }
        if let Some(other) = historical.get(&path).cloned() {
            if other.claim != owner {
                if other.origin == "locale_swap"
                    || reading_page_owner_transfer(
                        &other.claim,
                        &owner,
                        &reading_by_locale,
                        &target_families,
                    )
                {
                    historical.insert(
                        path,
                        HistoricalOwner {
                            claim: owner,
                            origin: "canonical".into(),
                        },
                    );
                } else {
                    collector.record(
                        &path,
                        format!("entry:{id}"),
                        format!("{}:{}", other.claim.0, other.claim.1),
                        format!(
                            "permalink_redirect_collision:path={path} legacy=entry:{id} owner={}:{}",
                            other.claim.0, other.claim.1,
                        ),
                    )?;
                }
            } else if other.origin != "canonical" {
                historical.insert(
                    path,
                    HistoricalOwner {
                        claim: owner,
                        origin: "canonical".into(),
                    },
                );
            }
        } else {
            historical.insert(
                path,
                HistoricalOwner {
                    claim: owner,
                    origin: "canonical".into(),
                },
            );
        }
    }
    for (path, owner) in homepage_redirects {
        if proposed
            .get(&path)
            .is_some_and(|cand| cand.locale != owner.2)
        {
            continue;
        }
        if let Some(other) = historical.get(&path).cloned() {
            if other.claim != owner {
                if other.claim.2 != owner.2 && other.origin == "canonical" {
                    // Preserve pre-swap canonical history from another locale rather than
                    // letting a synthetic homepage slug alias collide across swaps.
                    continue;
                }
                if other.origin == "locale_swap"
                    || reading_page_owner_transfer(
                        &other.claim,
                        &owner,
                        &reading_by_locale,
                        &target_families,
                    )
                {
                    historical.insert(
                        path,
                        HistoricalOwner {
                            claim: owner,
                            origin: "canonical".into(),
                        },
                    );
                } else {
                    collector.record(
                        &path,
                        format!("{}:{}", owner.0, owner.1),
                        format!("{}:{}", other.claim.0, other.claim.1),
                        format!(
                            "permalink_redirect_collision:path={path} new={}:{} owner={}:{}",
                            owner.0, owner.1, other.claim.0, other.claim.1,
                        ),
                    )?;
                }
            }
        } else {
            historical.insert(
                path,
                HistoricalOwner {
                    claim: owner,
                    origin: "canonical".into(),
                },
            );
        }
    }
    for (path, kind, id, loc) in &previous {
        // Disabled locales are hidden without turning their claims into redirects.
        if !enabled_set.contains(loc) {
            continue;
        }
        if path == "/" || is_unpublished_entry(kind, id, &unpublished_ids) {
            continue;
        }
        let posts_page_id = reading_by_locale.get(loc).and_then(|(_, posts)| *posts);
        let owner: Claim =
            if kind == "entry" && posts_page_id.is_some_and(|page| id == &page.to_string()) {
                ("archive".into(), "post".into(), loc.clone())
            } else {
                (kind.clone(), id.clone(), loc.clone())
            };
        let is_unchanged_claim = proposed
            .get(path)
            .is_some_and(|p| p.kind == *kind && p.id == *id && p.locale == *loc);
        if is_unchanged_claim {
            continue;
        }
        let is_swap_move = swap_old_default
            .as_deref()
            .is_some_and(|old_def| loc == old_def || loc == &default_locale);
        if is_swap_move {
            if proposed.contains_key(path) {
                continue;
            }
            let owner_family = target_families.get(&owner);
            let hits_foreign_pattern = patterns.iter().any(|pattern| {
                pattern.matches(path)
                    && !(owner.0 == pattern.kind
                        && owner.2 == pattern.locale
                        && owner_family.is_some_and(|fam| fam == &pattern.family))
            });
            if hits_foreign_pattern {
                continue;
            }
            if let Some(existing) = historical.get(path)
                && existing.origin == "canonical"
                && existing.claim != owner
            {
                continue;
            }
            historical.insert(
                path.clone(),
                HistoricalOwner {
                    claim: owner,
                    origin: "locale_swap".into(),
                },
            );
            continue;
        }
        if kind != "home" {
            if let Some(other) = historical.get(path).cloned() {
                if other.claim != owner
                    && other.origin != "locale_swap"
                    && !reading_page_owner_transfer(
                        &other.claim,
                        &owner,
                        &reading_by_locale,
                        &target_families,
                    )
                    && !reading_page_owner_transfer(
                        &owner,
                        &other.claim,
                        &reading_by_locale,
                        &target_families,
                    )
                {
                    collector.record(
                        path,
                        format!("{kind}:{id}"),
                        format!("{}:{}", other.claim.0, other.claim.1),
                        format!(
                            "permalink_redirect_collision:path={path} old={kind}:{id} owner={}:{}",
                            other.claim.0, other.claim.1,
                        ),
                    )?;
                }
                if reading_page_owner_transfer(
                    &owner,
                    &other.claim,
                    &reading_by_locale,
                    &target_families,
                ) {
                    historical.insert(path.clone(), other);
                } else if other.origin == "locale_swap" {
                    historical.insert(
                        path.clone(),
                        HistoricalOwner {
                            claim: owner,
                            origin: "canonical".into(),
                        },
                    );
                }
            } else {
                historical.insert(
                    path.clone(),
                    HistoricalOwner {
                        claim: owner,
                        origin: "canonical".into(),
                    },
                );
            }
        }
    }
    for (path, owner) in reclaimable_page_aliases {
        if let Some(other) = historical.get(&path).cloned()
            && other.claim != owner
            && (other.origin == "locale_swap"
                || reading_page_owner_transfer(
                    &other.claim,
                    &owner,
                    &reading_by_locale,
                    &target_families,
                ))
        {
            historical.insert(
                path,
                HistoricalOwner {
                    claim: owner,
                    origin: "canonical".into(),
                },
            );
        }
    }
    for (path, candidate) in &proposed {
        let cand_claim: Claim = (
            candidate.kind.clone(),
            candidate.id.clone(),
            candidate.locale.clone(),
        );
        if let Some(existing) = historical.get(path).cloned() {
            if existing.claim == cand_claim
                || existing.origin == "locale_swap"
                || reading_page_owner_transfer(
                    &existing.claim,
                    &cand_claim,
                    &reading_by_locale,
                    &target_families,
                )
            {
                historical.remove(path);
            } else {
                collector.record(
                    path,
                    format!("{}:{}", candidate.kind, candidate.id),
                    format!("{}:{}", existing.claim.0, existing.claim.1),
                    format!(
                        "permalink_collision:path={path} new={}:{} redirect_owner={}:{}",
                        candidate.kind, candidate.id, existing.claim.0, existing.claim.1,
                    ),
                )?;
            }
        }
    }

    // Active historical routes must stay inside their own dynamic family; orphan redirects whose
    // target was deleted remain exact-path redirects and never deadlock new CPT/taxonomy prefixes.
    // Reclaimable locale_swap redirects that overlap a new foreign DynamicPattern are dropped safely.
    let mut dropped_swap_patterns = Vec::<String>::new();
    for (path, entry) in &historical {
        let owner = &entry.claim;
        let Some(owner_family) = target_families.get(owner) else {
            continue;
        };
        for pattern in &patterns {
            if pattern.matches(path)
                && !(owner.0 == pattern.kind
                    && owner.2 == pattern.locale
                    && owner_family == &pattern.family)
            {
                if entry.origin == "locale_swap" {
                    dropped_swap_patterns.push(path.clone());
                    break;
                }
                collector.record(
                    path,
                    format!("{}:{}", pattern.kind, pattern.family),
                    format!("{}:{}", owner.0, owner.1),
                    format!(
                        "permalink_pattern_collision:redirect={path} owner={}:{} pattern={}:{}",
                        owner.0, owner.1, pattern.kind, pattern.family,
                    ),
                )?;
            }
        }
    }
    for path in dropped_swap_patterns {
        historical.remove(&path);
    }

    // Prune oldest deleted-target orphan redirects directly against DB tables when exceeding MAX_REDIRECTS.
    if historical.len() > MAX_REDIRECTS {
        let live_entries: HashSet<String> =
            sqlx::query_scalar::<_, String>("SELECT CAST(id AS TEXT) FROM content_entries")
                .fetch_all(&mut *conn)
                .await
                .map_err(ApiError::from)?
                .into_iter()
                .collect();
        let live_terms: HashSet<String> =
            sqlx::query_scalar::<_, String>("SELECT CAST(id AS TEXT) FROM taxonomy_terms")
                .fetch_all(&mut *conn)
                .await
                .map_err(ApiError::from)?
                .into_iter()
                .collect();
        let is_orphan = |entry: &HistoricalOwner| match entry.claim.0.as_str() {
            "entry" => !live_entries.contains(&entry.claim.1),
            "term" => !live_terms.contains(&entry.claim.1),
            _ => false,
        };
        for path in old_redirect_order {
            if historical.len() <= MAX_REDIRECTS {
                break;
            }
            if historical.get(&path).is_some_and(is_orphan) {
                historical.remove(&path);
            }
        }
        if historical.len() > MAX_REDIRECTS {
            let extra_orphans: Vec<String> = historical
                .iter()
                .filter(|(_, entry)| is_orphan(entry))
                .map(|(path, _)| path.clone())
                .collect();
            for path in extra_orphans {
                if historical.len() <= MAX_REDIRECTS {
                    break;
                }
                historical.remove(&path);
            }
        }
        if historical.len() > MAX_REDIRECTS {
            collector.record(
                "/redirects",
                "permalink_redirect_limit",
                "max_redirects",
                "permalink_redirect_limit: tối đa 200000 redirects đang hoạt động.".to_owned(),
            )?;
        }
    }

    collector.finish()?;

    // Read-only validation above is complete. Only changed claims/redirects
    // are written, avoiding a full database rewrite for every content save.
    let previous_map: BTreeMap<_, _> = previous
        .into_iter()
        .map(|(path, kind, id, loc)| (path, (kind, id, loc)))
        .collect();
    for (path, owner) in &previous_map {
        if !proposed
            .get(path)
            .is_some_and(|now| now.kind == owner.0 && now.id == owner.1 && now.locale == owner.2)
        {
            sqlx::query("DELETE FROM permalink_route_claims WHERE path=?")
                .bind(path)
                .execute(&mut *conn)
                .await
                .map_err(ApiError::from)?;
        }
    }
    for (path, now) in &proposed {
        if !previous_map
            .get(path)
            .is_some_and(|old| old.0 == now.kind && old.1 == now.id && old.2 == now.locale)
        {
            sqlx::query(
                "INSERT INTO permalink_route_claims(path,target_kind,target_id,locale) VALUES (?,?,?,?)",
            )
            .bind(path)
            .bind(&now.kind)
            .bind(&now.id)
            .bind(&now.locale)
            .execute(&mut *conn)
            .await
            .map_err(ApiError::from)?;
        }
    }
    for (path, entry) in &previous_history {
        if historical.get(path) != Some(entry) {
            sqlx::query("DELETE FROM permalink_route_redirects WHERE source_path=?")
                .bind(path)
                .execute(&mut *conn)
                .await
                .map_err(ApiError::from)?;
        }
    }
    for (path, entry) in historical {
        if previous_history.get(&path) != Some(&entry) {
            sqlx::query(
                "INSERT INTO permalink_route_redirects(source_path,target_kind,target_id,locale,origin) VALUES (?,?,?,?,?)
                 ON CONFLICT(source_path) DO UPDATE SET
                    target_kind=excluded.target_kind, target_id=excluded.target_id,
                    locale=excluded.locale, origin=excluded.origin,
                    created_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
                 WHERE permalink_route_redirects.locale IN
                    (SELECT code FROM site_locales WHERE deleted_at IS NOT NULL)",
            )
            .bind(path)
            .bind(entry.claim.0)
            .bind(entry.claim.1)
            .bind(entry.claim.2)
            .bind(entry.origin)
            .execute(&mut *conn)
            .await
            .map_err(ApiError::from)?;
        }
    }
    Ok(())
}

/// Resolve only publicly accessible destinations. Old aliases always go
/// directly to the current canonical path and never form redirect chains.
pub async fn resolve_public_path(
    db: &SqlitePool,
    path: &str,
) -> ApiResult<Option<ResolvedPublicRoute>> {
    if !path.starts_with('/') || path.contains('?') || path.contains('#') {
        return Ok(None);
    }
    let mut tx = db.begin().await?;
    let current: Option<(String, String, String)> = sqlx::query_as(
        "SELECT c.target_kind, c.target_id, c.locale
           FROM permalink_route_claims c
           JOIN site_locales sl ON sl.code = c.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
          WHERE c.path = ?",
    )
    .bind(path)
    .fetch_optional(&mut *tx)
    .await?;
    let (kind, id, locale, redirect) = match current {
        Some((kind, id, loc)) => (kind, id, loc, false),
        None => {
            let old: Option<(String, String, String)> = sqlx::query_as(
                "SELECT r.target_kind, r.target_id, r.locale
                   FROM permalink_route_redirects r
                   JOIN site_locales sl ON sl.code = r.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                  WHERE r.source_path = ?",
            )
            .bind(path)
            .fetch_optional(&mut *tx)
            .await?;
            match old {
                Some((kind, id, loc)) => (kind, id, loc, true),
                None => return Ok(None),
            }
        }
    };
    let default_locale = crate::storage::settings::get_default_locale_code_in_conn(&mut tx).await?;
    let (target, canonical_path) = match kind.as_str() {
        "home" => {
            let canonical = prefix_for_locale(&locale, &default_locale, "/");
            if !redirect && path != canonical {
                return Ok(None);
            }
            (PublicRouteTarget::Home { locale }, canonical)
        }
        "entry" => {
            let Ok(entry_id) = id.parse::<i64>() else {
                return Ok(None);
            };
            let visible: i64 = sqlx::query_scalar(
                "SELECT EXISTS(
                   SELECT 1 FROM content_entries e
                   JOIN site_locales sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                   JOIN content_types t ON t.key=e.content_type_key
                   WHERE e.id=? AND e.locale=? AND e.publication_status='published'
                     AND e.content_type_key NOT IN ('pattern', 'section')
                     AND t.provider_status='active'
                     AND COALESCE(json_extract(t.definition_json,'$.settings.public'),1)=1
                 )",
            )
            .bind(entry_id)
            .bind(&locale)
            .fetch_one(&mut *tx)
            .await?;
            if visible == 0 {
                return Ok(None);
            }
            let Some(canonical) = canonical_entry_path_in_tx(&mut tx, entry_id).await? else {
                return Ok(None);
            };
            let is_home: i64 = sqlx::query_scalar(
                "SELECT EXISTS(
                   SELECT 1 FROM site_reading_settings
                    WHERE locale = ? AND homepage_page_id = ?
                 )",
            )
            .bind(&locale)
            .bind(entry_id)
            .fetch_one(&mut *tx)
            .await?;
            (
                PublicRouteTarget::Entry {
                    id: entry_id,
                    locale,
                    is_home: is_home != 0,
                },
                canonical,
            )
        }
        "term" => {
            let Ok(term_id) = id.parse::<i64>() else {
                return Ok(None);
            };
            let Some(canonical) = canonical_term_path_in_tx(&mut tx, term_id).await? else {
                return Ok(None);
            };
            (
                PublicRouteTarget::Term {
                    id: term_id,
                    locale,
                },
                canonical,
            )
        }
        "archive" => {
            if id == "post" {
                // An assigned Blog Page which later becomes draft/deleted is a
                // dead route, not an implicit fallback to the default archive.
                let posts_page_id: Option<i64> = sqlx::query_scalar(
                    "SELECT r.posts_page_id
                       FROM site_reading_settings r
                       JOIN site_locales sl ON sl.code = r.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                      WHERE r.locale = ?",
                )
                .bind(&locale)
                .fetch_optional(&mut *tx)
                .await?
                .flatten();
                if let Some(page_id) = posts_page_id {
                    let published: i64 = sqlx::query_scalar(
                        "SELECT EXISTS(
                           SELECT 1 FROM content_entries e
                           JOIN content_types t ON t.key = e.content_type_key
                          WHERE e.id = ? AND e.locale = ? AND e.content_type_key = 'page'
                            AND e.publication_status = 'published'
                            AND t.provider_status = 'active'
                            AND COALESCE(json_extract(t.definition_json,'$.settings.public'),1)=1
                         )",
                    )
                    .bind(page_id)
                    .bind(&locale)
                    .fetch_one(&mut *tx)
                    .await?;
                    if published == 0 {
                        return Ok(None);
                    }
                }
            }
            let Some(canonical) = canonical_archive_path_in_tx(&mut tx, &id, &locale).await? else {
                return Ok(None);
            };
            (
                PublicRouteTarget::Archive {
                    type_key: id,
                    locale,
                },
                canonical,
            )
        }
        _ => return Ok(None),
    };
    if redirect && canonical_path == path {
        return Ok(None);
    }
    tx.commit().await?;
    Ok(Some(ResolvedPublicRoute {
        target,
        canonical_path,
        redirect,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn site() -> SqlitePool {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        db
    }

    fn vi_home() -> PublicRouteTarget {
        PublicRouteTarget::Home {
            locale: "vi".to_owned(),
        }
    }

    fn vi_entry(id: i64) -> PublicRouteTarget {
        PublicRouteTarget::Entry {
            id,
            locale: "vi".to_owned(),
            is_home: false,
        }
    }

    fn vi_home_entry(id: i64) -> PublicRouteTarget {
        PublicRouteTarget::Entry {
            id,
            locale: "vi".to_owned(),
            is_home: true,
        }
    }

    fn vi_term(id: i64) -> PublicRouteTarget {
        PublicRouteTarget::Term {
            id,
            locale: "vi".to_owned(),
        }
    }

    fn vi_archive(type_key: &str) -> PublicRouteTarget {
        PublicRouteTarget::Archive {
            type_key: type_key.to_owned(),
            locale: "vi".to_owned(),
        }
    }

    async fn reading_page(db: &SqlitePool, slug: &str, published: bool) -> i64 {
        let id = sqlx::query(
            "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
             VALUES('page',?,'Page','{}','draft','vi','eg-' || lower(hex(randomblob(8))))",
        )
        .bind(slug)
        .execute(db)
        .await
        .unwrap()
        .last_insert_rowid();
        if published {
            sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
                .bind(id)
                .execute(db)
                .await
                .unwrap();
        }
        id
    }

    #[tokio::test]
    async fn reading_settings_reclaims_home_and_posts_page_paths_without_collision() {
        use crate::storage::settings::{ReadingSettingsInput, save_reading};
        let db = site().await;
        let front = reading_page(&db, "front", true).await;
        let blog = reading_page(&db, "blog", true).await;
        let updates = reading_page(&db, "updates", true).await;

        let first = save_reading(
            &db,
            ReadingSettingsInput {
                homepage_page_id: Some(front),
                posts_page_id: Some(blog),
            },
            1,
        )
        .await
        .unwrap();
        assert_eq!(first.version, 2);
        assert_eq!(
            resolve_public_path(&db, "/").await.unwrap().unwrap().target,
            vi_home_entry(front)
        );
        let home_alias = resolve_public_path(&db, "/front").await.unwrap().unwrap();
        assert!(home_alias.redirect);
        assert_eq!(home_alias.canonical_path, "/");
        let blog_route = resolve_public_path(&db, "/blog").await.unwrap().unwrap();
        assert_eq!(blog_route.target, vi_archive("post"));
        assert!(!blog_route.redirect);
        let old_archive = resolve_public_path(&db, "/posts").await.unwrap().unwrap();
        assert_eq!(old_archive.canonical_path, "/blog");
        assert!(old_archive.redirect);
        assert_eq!(
            canonical_entry_path_in_tx(&mut db.acquire().await.unwrap(), blog)
                .await
                .unwrap(),
            None,
            "Posts Page must have no competing entry claim"
        );

        let moved = save_reading(
            &db,
            ReadingSettingsInput {
                homepage_page_id: Some(front),
                posts_page_id: Some(updates),
            },
            first.version,
        )
        .await
        .unwrap();
        assert_eq!(moved.version, 3);
        assert_eq!(
            resolve_public_path(&db, "/blog")
                .await
                .unwrap()
                .unwrap()
                .target,
            vi_entry(blog),
            "Former Posts Page regains its Page claim"
        );
        assert_eq!(
            resolve_public_path(&db, "/updates")
                .await
                .unwrap()
                .unwrap()
                .target,
            vi_archive("post")
        );
        let unset = save_reading(
            &db,
            ReadingSettingsInput {
                homepage_page_id: None,
                posts_page_id: None,
            },
            moved.version,
        )
        .await
        .unwrap();
        assert_eq!(unset.version, 4);
        assert_eq!(
            resolve_public_path(&db, "/").await.unwrap().unwrap().target,
            vi_home()
        );
        assert_eq!(
            resolve_public_path(&db, "/front")
                .await
                .unwrap()
                .unwrap()
                .target,
            vi_entry(front)
        );
        assert_eq!(
            resolve_public_path(&db, "/updates")
                .await
                .unwrap()
                .unwrap()
                .target,
            vi_entry(updates)
        );
        assert_eq!(
            resolve_public_path(&db, "/posts")
                .await
                .unwrap()
                .unwrap()
                .target,
            vi_archive("post")
        );
        save_reading(
            &db,
            ReadingSettingsInput {
                homepage_page_id: Some(front),
                posts_page_id: Some(blog),
            },
            unset.version,
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn reading_settings_reject_invalid_ids_and_stale_cas_without_writes() {
        use crate::storage::settings::{ReadingSettingsInput, get_reading, save_reading};
        let db = site().await;
        let draft = reading_page(&db, "draft-page", false).await;
        let published = reading_page(&db, "published-page", true).await;
        let post = sqlx::query(
            "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
             VALUES('post','article','Article','{}','draft','vi','eg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
            .bind(post)
            .execute(&db)
            .await
            .unwrap();
        for (home, posts) in [
            (Some(published), Some(published)),
            (Some(draft), None),
            (Some(post), None),
            (Some(999999), None),
            (Some(-1), None),
        ] {
            let error = save_reading(
                &db,
                ReadingSettingsInput {
                    homepage_page_id: home,
                    posts_page_id: posts,
                },
                1,
            )
            .await
            .unwrap_err();
            assert_eq!(error.0, axum::http::StatusCode::BAD_REQUEST);
            assert_eq!(get_reading(&db).await.unwrap().version, 1);
        }
        let saved = save_reading(
            &db,
            ReadingSettingsInput {
                homepage_page_id: Some(published),
                posts_page_id: None,
            },
            1,
        )
        .await
        .unwrap();
        assert_eq!(saved.version, 2);
        let error = save_reading(
            &db,
            ReadingSettingsInput {
                homepage_page_id: None,
                posts_page_id: None,
            },
            1,
        )
        .await
        .unwrap_err();
        assert_eq!(error.0, axum::http::StatusCode::PRECONDITION_FAILED);
        assert_eq!(
            get_reading(&db).await.unwrap().homepage_page_id,
            Some(published)
        );
    }

    #[tokio::test]
    async fn reading_settings_custom_blog_path_reclaims_page_alias_on_unset() {
        use crate::storage::settings::{ReadingSettingsInput, save_reading};
        let db = site().await;
        let blog = reading_page(&db, "blog", true).await;
        let mut tx = db.begin().await.unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        sqlx::query(
            "INSERT INTO entry_public_paths(entry_id,locale,path) VALUES(?, 'vi', '/news')",
        )
        .bind(blog)
        .execute(&db)
        .await
        .unwrap();

        let selected = save_reading(
            &db,
            ReadingSettingsInput {
                homepage_page_id: None,
                posts_page_id: Some(blog),
            },
            1,
        )
        .await
        .unwrap();
        assert_eq!(
            resolve_public_path(&db, "/news")
                .await
                .unwrap()
                .unwrap()
                .target,
            vi_archive("post")
        );
        let alias = resolve_public_path(&db, "/blog").await.unwrap().unwrap();
        assert_eq!(alias.canonical_path, "/news");
        assert!(alias.redirect);

        let unselected = save_reading(
            &db,
            ReadingSettingsInput {
                homepage_page_id: None,
                posts_page_id: None,
            },
            selected.version,
        )
        .await
        .unwrap();
        assert_eq!(
            resolve_public_path(&db, "/news")
                .await
                .unwrap()
                .unwrap()
                .target,
            vi_entry(blog)
        );
        let page_alias = resolve_public_path(&db, "/blog").await.unwrap().unwrap();
        assert_eq!(page_alias.target, vi_entry(blog));
        assert_eq!(page_alias.canonical_path, "/news");

        save_reading(
            &db,
            ReadingSettingsInput {
                homepage_page_id: None,
                posts_page_id: Some(blog),
            },
            unselected.version,
        )
        .await
        .unwrap();
        assert_eq!(
            resolve_public_path(&db, "/news")
                .await
                .unwrap()
                .unwrap()
                .target,
            vi_archive("post")
        );
    }

    #[tokio::test]
    async fn assigned_draft_and_deleted_pages_stay_dead_and_advance_delete_etag() {
        use crate::storage::settings::{
            ReadingPageStatus, ReadingSettingsInput, bump_reading_version_on_deleted_entry_in_tx,
            get_reading, save_reading,
        };
        let db = site().await;
        let front = reading_page(&db, "front", true).await;
        let blog = reading_page(&db, "blog", true).await;
        let saved = save_reading(
            &db,
            ReadingSettingsInput {
                homepage_page_id: Some(front),
                posts_page_id: Some(blog),
            },
            1,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE content_entries SET publication_status='draft' WHERE id IN (?,?)")
            .bind(front)
            .bind(blog)
            .execute(&db)
            .await
            .unwrap();
        let mut tx = db.begin().await.unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        let draft = get_reading(&db).await.unwrap();
        assert_eq!(draft.homepage_status, ReadingPageStatus::Draft);
        assert_eq!(draft.posts_page_status, ReadingPageStatus::Draft);
        assert_eq!(draft.version, saved.version);
        assert!(resolve_public_path(&db, "/").await.unwrap().is_none());
        assert!(resolve_public_path(&db, "/blog").await.unwrap().is_none());

        let mut tx = db.begin().await.unwrap();
        sqlx::query("DELETE FROM content_entries WHERE id IN (?,?)")
            .bind(front)
            .bind(blog)
            .execute(&mut *tx)
            .await
            .unwrap();
        bump_reading_version_on_deleted_entry_in_tx(&mut tx, front)
            .await
            .unwrap();
        bump_reading_version_on_deleted_entry_in_tx(&mut tx, blog)
            .await
            .unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        let deleted = get_reading(&db).await.unwrap();
        assert_eq!(deleted.homepage_page_id, Some(front));
        assert_eq!(deleted.posts_page_id, Some(blog));
        assert_eq!(deleted.homepage_status, ReadingPageStatus::Deleted);
        assert_eq!(deleted.posts_page_status, ReadingPageStatus::Deleted);
        assert_eq!(deleted.version, saved.version + 2);
        assert!(resolve_public_path(&db, "/").await.unwrap().is_none());
        assert!(resolve_public_path(&db, "/blog").await.unwrap().is_none());
        let claimed: (String, String) = sqlx::query_as(
            "SELECT target_kind,target_id FROM permalink_route_claims WHERE path='/blog'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(claimed, ("archive".into(), "post".into()));
    }

    async fn add_post(db: &SqlitePool, slug: &str, published: bool) -> i64 {
        let id = sqlx::query(
            "INSERT INTO content_entries
             (content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
             VALUES('post',?,'Example','{}','draft','vi','eg-' || lower(hex(randomblob(8))))",
        )
        .bind(slug)
        .execute(db)
        .await
        .unwrap()
        .last_insert_rowid();
        if published {
            sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
                .bind(id)
                .execute(db)
                .await
                .unwrap();
        }
        let mut tx = db.begin().await.unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        id
    }

    #[tokio::test]
    async fn upgrade_0040_preserves_explicit_site_null_and_absent_choices() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let migrator = sqlx::migrate!("../../migrations");
        migrator.run_to(40, &db).await.unwrap();
        sqlx::query(
            r#"UPDATE site_permalink_settings
             SET content_type_bases_json='{"books":null,"magazines":"issues"}',
                 taxonomy_bases_json='{"genre":null}' WHERE id=1"#,
        )
        .execute(&db)
        .await
        .unwrap();
        let before = get_settings(&db).await.unwrap();
        migrator.run(&db).await.unwrap();
        let after = get_settings(&db).await.unwrap();
        assert_eq!(after.post_structure, "/%postname%");
        assert_eq!(
            after.content_type_bases, before.content_type_bases,
            "migration must preserve explicit content_type_bases"
        );
        assert_eq!(
            after.taxonomy_bases, before.taxonomy_bases,
            "migration must preserve explicit taxonomy_bases"
        );
        assert_eq!(after.content_type_bases.get("books"), Some(&None));
        assert!(!after.content_type_bases.contains_key("not_selected"));
        assert_eq!(after.taxonomy_bases.get("genre"), Some(&None));
        let defaults: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM provider_rewrite_defaults")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(defaults, 0);
    }

    #[tokio::test]
    async fn provider_default_respects_site_choice_and_c3_version() {
        let db = site().await;
        let schema = serde_json::to_string(&opendash_core::content::ContentTypeDefinition {
            key: "books".to_owned(),
            label: "Books".to_owned(),
            fields: vec![],
            settings: Default::default(),
        })
        .unwrap();
        sqlx::query(
            "INSERT INTO content_types(key,definition_json,owner_kind,owner_id)
             VALUES('books',?,'plugin','books_plugin')",
        )
        .bind(schema)
        .execute(&db)
        .await
        .unwrap();
        let before = get_settings(&db).await.unwrap();
        let declaration = ProviderRewriteDeclaration {
            resource_kind: "content_type",
            resource_key: "books".to_owned(),
            rewrite_base: Some("library".to_owned()),
        };
        let mut tx = db.begin().await.unwrap();
        sync_provider_rewrite_defaults_in_tx(
            &mut tx,
            "plugin",
            "books_plugin",
            &[declaration.clone()],
        )
        .await
        .unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        let enabled = get_settings(&db).await.unwrap();
        assert_eq!(enabled.version, before.version + 1);
        assert!(!enabled.content_type_bases.contains_key("books"));
        let mut conn = db.acquire().await.unwrap();
        let effective = read_effective_settings(&mut conn).await.unwrap();
        assert_eq!(
            effective.content_type_bases["books"],
            Some("library".into())
        );
        drop(conn);

        let mut changed = declaration.clone();
        changed.rewrite_base = Some("catalog".into());
        let mut tx = db.begin().await.unwrap();
        sync_provider_rewrite_defaults_in_tx(&mut tx, "plugin", "books_plugin", &[changed.clone()])
            .await
            .unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        let modified = get_settings(&db).await.unwrap();
        assert_eq!(
            modified.version,
            enabled.version + 1,
            "updating an effective provider default invalidates an older site editor CAS"
        );
        assert!(!modified.content_type_bases.contains_key("books"));
        let mut conn = db.acquire().await.unwrap();
        assert_eq!(
            read_effective_settings(&mut conn)
                .await
                .unwrap()
                .content_type_bases["books"],
            Some("catalog".to_owned())
        );
        drop(conn);

        let mut explicit_legacy = get_settings(&db).await.unwrap().input();
        explicit_legacy
            .content_type_bases
            .insert("books".into(), None);
        let saved = put_settings(&db, modified.version, &explicit_legacy)
            .await
            .unwrap();
        let mut tx = db.begin().await.unwrap();
        changed.rewrite_base = Some("reading".into());
        sync_provider_rewrite_defaults_in_tx(&mut tx, "plugin", "books_plugin", &[changed])
            .await
            .unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        let final_site = get_settings(&db).await.unwrap();
        assert_eq!(
            final_site.version, saved.version,
            "provider defaults hidden by site choice"
        );
        assert_eq!(final_site.content_type_bases.get("books"), Some(&None));
        let mut conn = db.acquire().await.unwrap();
        assert_eq!(
            read_effective_settings(&mut conn)
                .await
                .unwrap()
                .content_type_bases["books"],
            None
        );
        drop(conn);
        let mut tx = db.begin().await.unwrap();
        set_provider_rewrite_status_in_tx(&mut tx, "plugin", "books_plugin", false)
            .await
            .unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(get_settings(&db).await.unwrap().version, saved.version);
        let mut tx = db.begin().await.unwrap();
        set_provider_rewrite_status_in_tx(&mut tx, "plugin", "books_plugin", true)
            .await
            .unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            get_settings(&db).await.unwrap().version,
            saved.version,
            "explicit null must survive disable and re-enable"
        );
    }

    #[tokio::test]
    async fn upgrade_0039_custom_page_keeps_default_slug_redirect_without_blanket_aliases() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let migrator = sqlx::migrate!("../../migrations");
        migrator.run_to(39, &db).await.unwrap();

        let page_id = sqlx::query(
            "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
             VALUES ('page','upgrade-page','Upgrade','{}','draft')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
            .bind(page_id)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO entry_public_paths(entry_id,path) VALUES(?, '/old-page-custom')")
            .bind(page_id)
            .execute(&db)
            .await
            .unwrap();
        let untouched = sqlx::query(
            "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
             VALUES ('post','untouched-post','Untouched','{}','draft')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
            .bind(untouched)
            .execute(&db)
            .await
            .unwrap();

        migrator.run_to(40, &db).await.unwrap();
        let redirect_row: (String, String, String) = sqlx::query_as(
            "SELECT target_kind, target_id, source_path FROM permalink_route_redirects WHERE source_path='/content/page/upgrade-page'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(
            redirect_row,
            (
                "entry".to_owned(),
                page_id.to_string(),
                "/content/page/upgrade-page".to_owned(),
            ),
        );
        let claim_page: (String, String) = sqlx::query_as(
            "SELECT target_kind, target_id FROM permalink_route_claims WHERE path='/old-page-custom'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(claim_page, ("entry".to_owned(), page_id.to_string()));
        let claim_post: (String, String) = sqlx::query_as(
            "SELECT target_kind, target_id FROM permalink_route_claims WHERE path='/content/post/untouched-post'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(claim_post, ("entry".to_owned(), untouched.to_string()));
        let redirect_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM permalink_route_redirects WHERE target_kind='entry'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(
            redirect_count, 1,
            "no blanket aliases for untouched entries"
        );
        migrator.run(&db).await.unwrap();
        assert_eq!(
            resolve_public_path(&db, "/content/page/upgrade-page")
                .await
                .unwrap(),
            Some(ResolvedPublicRoute {
                target: vi_entry(page_id),
                canonical_path: "/old-page-custom".to_owned(),
                redirect: true,
            }),
        );
        assert_eq!(
            resolve_public_path(&db, "/old-page-custom").await.unwrap(),
            Some(ResolvedPublicRoute {
                target: vi_entry(page_id),
                canonical_path: "/old-page-custom".to_owned(),
                redirect: false,
            }),
        );
        assert_eq!(
            resolve_public_path(&db, "/untouched-post").await.unwrap(),
            Some(ResolvedPublicRoute {
                target: vi_entry(untouched),
                canonical_path: "/untouched-post".to_owned(),
                redirect: false,
            }),
        );
    }

    #[tokio::test]
    async fn upgrade_0039_custom_page_alias_collision_fails_closed_without_partial_migration() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let migrator = sqlx::migrate!("../../migrations");
        migrator.run_to(39, &db).await.unwrap();

        let page_id = sqlx::query(
            "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
             VALUES('page','upgrade-page','Upgrade','{}','draft')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query("INSERT INTO entry_public_paths(entry_id,path) VALUES(?,'/old-page-custom')")
            .bind(page_id)
            .execute(&db)
            .await
            .unwrap();
        // Different Page claims exactly the old implicit URL of page_id.
        let other_id = sqlx::query(
            "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
             VALUES('page','other-page','Other','{}','draft')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query(
            "INSERT INTO entry_public_paths(entry_id,path) VALUES(?,'/content/page/upgrade-page')",
        )
        .bind(other_id)
        .execute(&db)
        .await
        .unwrap();

        let err = migrator.run(&db).await.unwrap_err();
        assert!(
            err.to_string().contains("CHECK constraint failed"),
            "expected fail-closed canonical/alias collision: {err:?}"
        );
        let migrated_0040: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version=40 AND success=1",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(migrated_0040, 0);
        let claims_table: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='permalink_route_claims'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(
            claims_table, 0,
            "failed migration must not leave partial 0040 tables"
        );
        let existing_path: String =
            sqlx::query_scalar("SELECT path FROM entry_public_paths WHERE entry_id=?")
                .bind(other_id)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(existing_path, "/content/page/upgrade-page");
    }

    #[tokio::test]
    async fn upgrade_0039_reuses_same_owner_redirect_preserving_original_timestamp() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let migrator = sqlx::migrate!("../../migrations");
        migrator.run_to(39, &db).await.unwrap();
        let page_id = sqlx::query(
            "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
             VALUES('page','upgrade-page','Upgrade','{}','draft')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query("INSERT INTO entry_public_paths(entry_id,path) VALUES(?,'/old-page-custom')")
            .bind(page_id)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO entry_redirects(source_path,entry_id,created_at)
             VALUES('/content/page/upgrade-page',?,'2026-11-18T01:23:45Z')",
        )
        .bind(page_id)
        .execute(&db)
        .await
        .unwrap();
        migrator.run_to(40, &db).await.unwrap();
        let (owner, created_at): (String, String) = sqlx::query_as(
            "SELECT target_id,created_at FROM permalink_route_redirects
              WHERE source_path='/content/page/upgrade-page'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(owner, page_id.to_string());
        assert_eq!(created_at, "2026-11-18T01:23:45Z");
    }

    #[tokio::test]
    async fn legacy_draft_is_private_and_redirect_tracks_stable_entry_id() {
        let db = site().await;
        let id = add_post(&db, "first", false).await;
        assert_eq!(resolve_public_path(&db, "/first").await.unwrap(), None);
        let config = get_settings(&db).await.unwrap();
        assert_eq!(config.version, 1);
        assert_eq!(config.post_structure, "/%postname%");
        let changed = put_settings(
            &db,
            config.version,
            &PermalinkSettingsInput {
                post_structure: "/blog/%postname%".into(),
                content_type_bases: BTreeMap::new(),
                taxonomy_bases: BTreeMap::new(),
            },
        )
        .await
        .unwrap();
        assert_eq!(changed.version, 2);
        // Draft moving from /first to /blog/first never records a redirect at /first.
        assert_eq!(resolve_public_path(&db, "/first").await.unwrap(), None);

        let mut tx = db.begin().await.unwrap();
        sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        // Pre-publish draft URL /first stays 404 even after publishing on /blog/first.
        assert_eq!(resolve_public_path(&db, "/first").await.unwrap(), None);
        assert_eq!(
            resolve_public_path(&db, "/blog/first").await.unwrap(),
            Some(ResolvedPublicRoute {
                target: vi_entry(id),
                canonical_path: "/blog/first".into(),
                redirect: false,
            }),
        );

        let moved = put_settings(
            &db,
            changed.version,
            &PermalinkSettingsInput {
                post_structure: "/news/%postname%".into(),
                content_type_bases: BTreeMap::new(),
                taxonomy_bases: BTreeMap::new(),
            },
        )
        .await
        .unwrap();
        // Published entry moving /blog/first -> /news/first records 301 at /blog/first.
        assert_eq!(
            resolve_public_path(&db, "/blog/first").await.unwrap(),
            Some(ResolvedPublicRoute {
                target: vi_entry(id),
                canonical_path: "/news/first".into(),
                redirect: true,
            }),
        );
        // Reclaimed custom URL that equals the generated route is unambiguous.
        let mut tx = db.begin().await.unwrap();
        sqlx::query("INSERT INTO entry_public_paths(entry_id,locale,path) VALUES(?,'vi',?)")
            .bind(id)
            .bind("/news/first")
            .execute(&mut *tx)
            .await
            .unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        let prior = get_settings(&db).await.unwrap();
        assert_eq!(prior.version, moved.version);
        assert_eq!(
            put_settings(&db, 1, &prior.input())
                .await
                .unwrap_err()
                .status(),
            axum::http::StatusCode::PRECONDITION_FAILED
        );
    }

    #[tokio::test]
    async fn conflicting_permalinks_abort_configuration_and_preserve_history() {
        let db = site().await;
        let v1 = get_settings(&db).await.unwrap();
        let blog = put_settings(
            &db,
            v1.version,
            &PermalinkSettingsInput {
                post_structure: "/blog/%postname%".into(),
                content_type_bases: BTreeMap::new(),
                taxonomy_bases: BTreeMap::new(),
            },
        )
        .await
        .unwrap();
        let id = add_post(&db, "admin", true).await;
        let bad = PermalinkSettingsInput {
            post_structure: "/%postname%".into(),
            content_type_bases: BTreeMap::new(),
            taxonomy_bases: BTreeMap::new(),
        };
        let err = put_settings(&db, blog.version, &bad).await.unwrap_err();
        assert_eq!(err.status(), axum::http::StatusCode::CONFLICT);
        assert_eq!(get_settings(&db).await.unwrap(), blog);
        assert_eq!(
            canonical_entry_path_in_tx(&mut db.acquire().await.unwrap(), id)
                .await
                .unwrap(),
            Some("/blog/admin".into())
        );

        let mut other = blog.input();
        other.post_structure = "/blog/%postname%".into();
        other
            .taxonomy_bases
            .insert("category".into(), Some("blog".into()));
        let err = put_settings(&db, blog.version, &other).await.unwrap_err();
        assert_eq!(err.status(), axum::http::StatusCode::CONFLICT);
        assert_eq!(get_settings(&db).await.unwrap(), blog);
        assert!(
            resolve_public_path(&db, "/blog/admin")
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn term_history_follows_rewrite_and_slug_updates_and_hides_private_terms() {
        let db = site().await;
        let id = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Sea','sea','vi','tg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let mut tx = db.begin().await.unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        let previous = get_settings(&db).await.unwrap();
        let mut changed = previous.input();
        changed
            .taxonomy_bases
            .insert("category".into(), Some("genres".into()));
        put_settings(&db, previous.version, &changed).await.unwrap();
        assert_eq!(
            resolve_public_path(&db, "/category/sea")
                .await
                .unwrap()
                .unwrap(),
            ResolvedPublicRoute {
                target: vi_term(id),
                canonical_path: "/genres/sea".into(),
                redirect: true,
            },
        );
        let mut tx = db.begin().await.unwrap();
        sqlx::query("UPDATE taxonomy_terms SET slug='ocean' WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            resolve_public_path(&db, "/category/sea")
                .await
                .unwrap()
                .unwrap()
                .canonical_path,
            "/genres/ocean",
        );
        assert_eq!(
            resolve_public_path(&db, "/genres/sea")
                .await
                .unwrap()
                .unwrap()
                .canonical_path,
            "/genres/ocean",
        );
        sqlx::query("UPDATE taxonomy_definitions SET public=0 WHERE key='category'")
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(
            resolve_public_path(&db, "/genres/ocean").await.unwrap(),
            None
        );
        assert_eq!(resolve_public_path(&db, "/genres/sea").await.unwrap(), None);
    }

    #[tokio::test]
    async fn failed_writer_claim_rebuild_rolls_back_original_entry() {
        let db = site().await;
        let id = add_post(&db, "safe", true).await;
        let config = get_settings(&db).await.unwrap();
        assert_eq!(config.post_structure, "/%postname%");
        let mut tx = db.begin().await.unwrap();
        sqlx::query("UPDATE content_entries SET slug='admin' WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .unwrap();
        let collision = sync_claims_in_tx(&mut tx).await.unwrap_err();
        assert_eq!(collision.0, axum::http::StatusCode::CONFLICT);
        tx.rollback().await.unwrap();
        let slug: String = sqlx::query_scalar("SELECT slug FROM content_entries WHERE id=?")
            .bind(id)
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(slug, "safe");
        assert_eq!(
            resolve_public_path(&db, "/safe")
                .await
                .unwrap()
                .unwrap()
                .target,
            vi_entry(id),
        );
        assert_eq!(resolve_public_path(&db, "/admin").await.unwrap(), None);
    }

    #[tokio::test]
    async fn soft_deleted_redirect_is_preserved_until_active_history_reuses_path() {
        let db = site().await;
        sqlx::query(
            "INSERT INTO site_locales(code,bcp47_tag,label,enabled,deleted_at)
             VALUES('en','en','English',0,'2026-01-01')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO permalink_route_redirects
             (source_path,target_kind,target_id,locale,origin,created_at)
             VALUES('/alpha','entry','999','en','canonical','2026-01-01')",
        )
        .execute(&db)
        .await
        .unwrap();

        assert!(resolve_public_path(&db, "/alpha").await.unwrap().is_none());
        // An active canonical claim may coexist with parked history.
        let id = add_post(&db, "alpha", true).await;
        let parked: (String, String, String) = sqlx::query_as(
            "SELECT target_id,locale,created_at FROM permalink_route_redirects WHERE source_path='/alpha'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(parked, ("999".into(), "en".into(), "2026-01-01".into()));
        assert_eq!(
            resolve_public_path(&db, "/alpha")
                .await
                .unwrap()
                .unwrap()
                .target,
            vi_entry(id)
        );

        // Moving the live canonical path creates history at the same PK.
        let mut tx = db.begin().await.unwrap();
        sqlx::query("UPDATE content_entries SET slug='beta' WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        let redirect: (String, String, String) = sqlx::query_as(
            "SELECT target_id,locale,origin FROM permalink_route_redirects WHERE source_path='/alpha'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(redirect, (id.to_string(), "vi".into(), "canonical".into()));
        assert_eq!(
            resolve_public_path(&db, "/alpha").await.unwrap(),
            Some(ResolvedPublicRoute {
                target: vi_entry(id),
                canonical_path: "/beta".into(),
                redirect: true,
            })
        );
    }

    #[tokio::test]
    async fn historical_post_redirect_cannot_shadow_new_cpt_dynamic_family() {
        let db = site().await;
        let post_id = add_post(&db, "alpha", true).await;
        let original = get_settings(&db).await.unwrap();
        let mut blog = original.input();
        blog.post_structure = "/blog/%postname%".to_owned();
        let updated = put_settings(&db, original.version, &blog).await.unwrap();
        let mut news = blog.clone();
        news.post_structure = "/news/%postname%".to_owned();
        let latest = put_settings(&db, updated.version, &news).await.unwrap();

        // /blog/alpha remains a 301 alias after moving the post to /news.
        assert_eq!(
            resolve_public_path(&db, "/blog/alpha").await.unwrap(),
            Some(ResolvedPublicRoute {
                target: vi_entry(post_id),
                canonical_path: "/news/alpha".into(),
                redirect: true,
            })
        );
        sqlx::query(
            "INSERT INTO content_types(key,definition_json,owner_kind,owner_id,provider_status,declaration_state,version)
             SELECT 'books',definition_json,'user',NULL,'active','declared',1
               FROM content_types WHERE key='post'",
        ).execute(&db).await.unwrap();
        let mut conflicting = news.clone();
        conflicting
            .content_type_bases
            .insert("books".into(), Some("blog".into()));

        // /blog itself can host an archive, but /blog/{slug} is already
        // occupied by a historical post redirect belonging to another family.
        let err = put_settings(&db, latest.version, &conflicting)
            .await
            .unwrap_err();
        assert_eq!(err.status(), axum::http::StatusCode::CONFLICT, "{err:?}");
        assert_eq!(get_settings(&db).await.unwrap(), latest);
        assert_eq!(
            resolve_public_path(&db, "/blog/alpha").await.unwrap(),
            Some(ResolvedPublicRoute {
                target: vi_entry(post_id),
                canonical_path: "/news/alpha".into(),
                redirect: true,
            })
        );
        assert_eq!(resolve_public_path(&db, "/blog").await.unwrap(), None);

        // Reclaiming the same old path for its original post restores a
        // direct canonical route; the pre-upgrade URL remains a one-hop 301.
        let reclaimed = put_settings(&db, latest.version, &blog).await.unwrap();
        assert_eq!(reclaimed.post_structure, "/blog/%postname%");
        assert_eq!(
            resolve_public_path(&db, "/blog/alpha").await.unwrap(),
            Some(ResolvedPublicRoute {
                target: vi_entry(post_id),
                canonical_path: "/blog/alpha".into(),
                redirect: false,
            })
        );
        assert_eq!(
            resolve_public_path(&db, "/alpha")
                .await
                .unwrap()
                .unwrap()
                .canonical_path,
            "/blog/alpha"
        );
    }

    #[tokio::test]
    async fn legacy_migration_redirect_cannot_shadow_foreign_dynamic_family() {
        let db = site().await;
        let post_id = add_post(&db, "alpha", true).await;
        let mut config = get_settings(&db).await.unwrap();
        let mut news = config.input();
        news.post_structure = "/news/%postname%".into();
        config = put_settings(&db, config.version, &news).await.unwrap();

        sqlx::query(
            "INSERT INTO entry_redirects(locale,source_path,entry_id) VALUES('vi','/books/alpha', ?)",
        )
            .bind(post_id)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO content_types(key,definition_json,owner_kind,owner_id,provider_status,declaration_state,version)
             SELECT 'novels',definition_json,'user',NULL,'active','declared',1
               FROM content_types WHERE key='post'",
        ).execute(&db).await.unwrap();

        let mut bad = config.input();
        bad.content_type_bases
            .insert("novels".into(), Some("books".into()));
        let error = put_settings(&db, config.version, &bad).await.unwrap_err();
        assert_eq!(
            error.status(),
            axum::http::StatusCode::CONFLICT,
            "{error:?}"
        );
        assert_eq!(get_settings(&db).await.unwrap(), config);
        assert_eq!(
            resolve_public_path(&db, "/news/alpha")
                .await
                .unwrap()
                .unwrap()
                .target,
            vi_entry(post_id)
        );
        assert_eq!(resolve_public_path(&db, "/books").await.unwrap(), None);
    }

    #[tokio::test]
    async fn deleted_term_legacy_alias_remains_404_without_blocking_atomic_delete() {
        let db = site().await;
        let term_id = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Parent','parent','vi','tg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let mut tx = db.begin().await.unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();

        let mut tx = db.begin().await.unwrap();
        sqlx::query("DELETE FROM taxonomy_terms WHERE id=?")
            .bind(term_id)
            .execute(&mut *tx)
            .await
            .unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            resolve_public_path(&db, "/category/parent").await.unwrap(),
            None,
        );
        let historical: Option<(String, String)> = sqlx::query_as(
            "SELECT target_kind,target_id FROM permalink_route_redirects
              WHERE source_path='/category/parent'",
        )
        .fetch_optional(&db)
        .await
        .unwrap();
        assert_eq!(historical, Some(("term".to_owned(), term_id.to_string())));
    }

    #[tokio::test]
    async fn deleted_post_legacy_alias_remains_404_without_blocking_atomic_delete() {
        let db = site().await;
        let post_id = add_post(&db, "old-post", true).await;
        let mut tx = db.begin().await.unwrap();
        sqlx::query("DELETE FROM content_entries WHERE id=?")
            .bind(post_id)
            .execute(&mut *tx)
            .await
            .unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(resolve_public_path(&db, "/old-post").await.unwrap(), None,);
    }

    #[tokio::test]
    async fn deleted_rewritten_post_retains_dead_aliases_without_blocking_delete() {
        let db = site().await;
        let original = get_settings(&db).await.unwrap();
        let mut blog = original.input();
        blog.post_structure = "/blog/%postname%".to_owned();
        put_settings(&db, original.version, &blog).await.unwrap();
        let post_id = add_post(&db, "gone", true).await;
        assert_eq!(
            resolve_public_path(&db, "/blog/gone")
                .await
                .unwrap()
                .unwrap()
                .target,
            vi_entry(post_id),
        );

        let mut tx = db.begin().await.unwrap();
        sqlx::query("DELETE FROM content_entries WHERE id=?")
            .bind(post_id)
            .execute(&mut *tx)
            .await
            .unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(resolve_public_path(&db, "/blog/gone").await.unwrap(), None);
        assert_eq!(resolve_public_path(&db, "/gone").await.unwrap(), None);
    }

    #[tokio::test]
    async fn deleted_rewritten_term_retains_dead_aliases_without_blocking_delete() {
        let db = site().await;
        let original = get_settings(&db).await.unwrap();
        let mut genres = original.input();
        genres
            .taxonomy_bases
            .insert("category".to_owned(), Some("genres".to_owned()));
        put_settings(&db, original.version, &genres).await.unwrap();
        let term_id = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Gone','gone','vi','tg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let mut tx = db.begin().await.unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            resolve_public_path(&db, "/genres/gone")
                .await
                .unwrap()
                .unwrap()
                .target,
            vi_term(term_id),
        );

        let mut tx = db.begin().await.unwrap();
        sqlx::query("DELETE FROM taxonomy_terms WHERE id=?")
            .bind(term_id)
            .execute(&mut *tx)
            .await
            .unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            resolve_public_path(&db, "/genres/gone").await.unwrap(),
            None
        );
        assert_eq!(
            resolve_public_path(&db, "/category/gone").await.unwrap(),
            None,
        );
    }

    #[tokio::test]
    async fn dead_rewritten_post_alias_does_not_deadlock_new_cpt_family() {
        let db = site().await;
        let original = get_settings(&db).await.unwrap();
        let mut blog = original.input();
        blog.post_structure = "/blog/%postname%".to_owned();
        let blog_saved = put_settings(&db, original.version, &blog).await.unwrap();
        let post_id = add_post(&db, "gone", true).await;
        let mut tx = db.begin().await.unwrap();
        sqlx::query("DELETE FROM content_entries WHERE id=?")
            .bind(post_id)
            .execute(&mut *tx)
            .await
            .unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();

        let mut news = blog.clone();
        news.post_structure = "/news/%postname%".to_owned();
        let news_saved = put_settings(&db, blog_saved.version, &news).await.unwrap();
        sqlx::query(
            "INSERT INTO content_types(key,definition_json,owner_kind,owner_id,provider_status,declaration_state,version)
             SELECT 'books',definition_json,'user',NULL,'active','declared',1
               FROM content_types WHERE key='post'",
        )
        .execute(&db)
        .await
        .unwrap();
        let mut allowed = news.clone();
        allowed
            .content_type_bases
            .insert("books".into(), Some("blog".into()));
        let updated = put_settings(&db, news_saved.version, &allowed)
            .await
            .unwrap();
        assert_eq!(
            updated.content_type_bases.get("books"),
            Some(&Some("blog".into()))
        );
        assert_eq!(resolve_public_path(&db, "/blog/gone").await.unwrap(), None);
    }

    #[tokio::test]
    async fn orphan_redirect_never_deadlocks_new_cpt_rewrite_while_exact_slug_stays_protected() {
        let db = site().await;
        sqlx::query(
            "INSERT INTO permalink_route_redirects(source_path,target_kind,target_id,locale,origin)
             VALUES('/books/orphan','entry','999999','vi','canonical')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO content_types(key,definition_json,owner_kind,owner_id,provider_status,declaration_state,version)
             SELECT 'books',definition_json,'user',NULL,'active','declared',1
               FROM content_types WHERE key='post'",
        )
        .execute(&db)
        .await
        .unwrap();

        let initial = get_settings(&db).await.unwrap();
        let mut attempted = initial.input();
        attempted
            .content_type_bases
            .insert("books".to_owned(), Some("books".to_owned()));
        let updated = put_settings(&db, initial.version, &attempted)
            .await
            .unwrap();
        assert_eq!(
            updated.content_type_bases.get("books"),
            Some(&Some("books".to_owned()))
        );
        assert_eq!(
            resolve_public_path(&db, "/books/orphan").await.unwrap(),
            None
        );
    }

    #[test]
    fn claims_beyond_fifty_thousand_are_rejected_before_writing() {
        let mut claims = BTreeMap::new();
        let mut collector = ConflictCollector::new(false);
        for id in 1..=MAX_CLAIMS {
            add_claim(
                &mut claims,
                format!("/claim-{id}"),
                "entry",
                id.to_string(),
                "vi",
                Some("post".to_owned()),
                true,
                &mut collector,
            )
            .unwrap();
        }
        let err = add_claim(
            &mut claims,
            "/claim-overflow".into(),
            "entry",
            "200001".into(),
            "vi",
            Some("post".to_owned()),
            true,
            &mut collector,
        )
        .unwrap_err();
        assert_eq!(err.status(), axum::http::StatusCode::CONFLICT);
        assert_eq!(claims.len(), MAX_CLAIMS + 1);
    }

    #[tokio::test]
    async fn upgrade_0045_sets_wordpress_defaults_triggers_and_cleans_draft_redirects() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let migrator = sqlx::migrate!("../../migrations");
        migrator.run_to(44, &db).await.unwrap();

        sqlx::query(
            "INSERT INTO content_types(key,definition_json,owner_kind,owner_id,provider_status,declaration_state,version)
             VALUES('books', json_object('key','books','label','Books','fields',json('[]'),'settings',json_object('public',json('true'),'has_archive',json('true'))), 'plugin','bk','active','declared',1)",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO provider_rewrite_defaults(resource_kind,resource_key,owner_kind,owner_id,rewrite_base)
             VALUES('content_type','books','plugin','bk','library')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE site_permalink_settings SET content_type_bases_json='{\"books\":null}' WHERE id=1",
        )
        .execute(&db)
        .await
        .unwrap();

        let page_about = sqlx::query(
            "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
             VALUES('page','about','About','{}','draft')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let post_about = sqlx::query(
            "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
             VALUES('post','about','About Post','{}','draft')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id IN (?,?)")
            .bind(page_about)
            .bind(post_about)
            .execute(&db)
            .await
            .unwrap();
        let post_draft = sqlx::query(
            "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
             VALUES('post','draft-item','Draft','{}','draft')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query("INSERT INTO entry_public_paths(entry_id,path) VALUES(?,'/studio/custom')")
            .bind(post_draft)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO entry_redirects(source_path,entry_id) VALUES('/draft-old',?)")
            .bind(post_draft)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO permalink_route_redirects(source_path,target_kind,target_id) VALUES('/draft-route','entry',?)",
        )
        .bind(post_draft.to_string())
        .execute(&db)
        .await
        .unwrap();

        migrator.run(&db).await.unwrap();

        let settings = get_settings(&db).await.unwrap();
        assert_eq!(settings.post_structure, "/%postname%");
        assert_eq!(
            resolve_public_path(&db, "/about")
                .await
                .unwrap()
                .unwrap()
                .target,
            vi_entry(page_about)
        );
        let disambiguated_slug: String =
            sqlx::query_scalar("SELECT slug FROM content_entries WHERE id=?")
                .bind(post_about)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(disambiguated_slug, format!("about-{post_about}"));
        assert_eq!(
            resolve_public_path(&db, &format!("/{disambiguated_slug}"))
                .await
                .unwrap()
                .unwrap()
                .target,
            vi_entry(post_about)
        );
        // Explicit null keeps system default /books instead of provider 'library'.
        assert_eq!(
            resolve_public_path(&db, "/books")
                .await
                .unwrap()
                .unwrap()
                .target,
            vi_archive("books")
        );
        // Reserved /studio custom path & draft redirects cleaned up.
        let custom_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM entry_public_paths WHERE path='/studio/custom'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(custom_count, 0);
        assert_eq!(resolve_public_path(&db, "/draft-old").await.unwrap(), None);
        let redir_left: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM permalink_route_redirects WHERE source_path='/draft-route'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(redir_left, 0);

        // Monotonic triggers keep has_been_published = 1 on publish and revert to draft.
        let inserted_pub = sqlx::query(
            "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
             VALUES('books','trigger-ins','Trigger','{}','published','vi','eg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let hbp_ins: i64 =
            sqlx::query_scalar("SELECT has_been_published FROM content_entries WHERE id=?")
                .bind(inserted_pub)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(hbp_ins, 1);

        sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
            .bind(post_draft)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("UPDATE content_entries SET publication_status='draft' WHERE id=?")
            .bind(post_draft)
            .execute(&db)
            .await
            .unwrap();
        let hbp_draft_reverted: i64 =
            sqlx::query_scalar("SELECT has_been_published FROM content_entries WHERE id=?")
                .bind(post_draft)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(hbp_draft_reverted, 1);
    }

    #[tokio::test]
    async fn category_permalink_uses_primary_min_or_uncategorized_and_guards_roots() {
        let db = site().await;
        let root_page = reading_page(&db, "about", true).await;
        let cat_a = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Alpha','alpha','vi','tg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let cat_b = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Beta','beta','vi','tg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let post_primary = add_post(&db, "story", true).await;
        let post_none = add_post(&db, "plain", true).await;
        sqlx::query("INSERT INTO entry_terms(entry_id,term_id) VALUES(?,?),(?,?)")
            .bind(post_primary)
            .bind(cat_a)
            .bind(post_primary)
            .bind(cat_b)
            .execute(&db)
            .await
            .unwrap();

        let s1 = get_settings(&db).await.unwrap();
        let s2 = put_settings(
            &db,
            s1.version,
            &PermalinkSettingsInput {
                post_structure: "/%category%/%postname%".into(),
                content_type_bases: BTreeMap::new(),
                taxonomy_bases: BTreeMap::new(),
            },
        )
        .await
        .unwrap();
        // Page stays at root, post_primary picks min(term_id)=alpha, post_none falls back to uncategorized.
        assert_eq!(
            resolve_public_path(&db, "/about")
                .await
                .unwrap()
                .unwrap()
                .target,
            vi_entry(root_page)
        );
        assert_eq!(
            resolve_public_path(&db, "/alpha/story")
                .await
                .unwrap()
                .unwrap()
                .target,
            vi_entry(post_primary)
        );
        assert_eq!(
            resolve_public_path(&db, "/uncategorized/plain")
                .await
                .unwrap()
                .unwrap()
                .target,
            vi_entry(post_none)
        );

        // Selecting cat_b as primary switches route to /beta/story with 301 at /alpha/story.
        let mut tx = db.begin().await.unwrap();
        sqlx::query("UPDATE content_entries SET primary_category_term_id=? WHERE id=?")
            .bind(cat_b)
            .bind(post_primary)
            .execute(&mut *tx)
            .await
            .unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            resolve_public_path(&db, "/beta/story")
                .await
                .unwrap()
                .unwrap()
                .target,
            vi_entry(post_primary)
        );
        assert!(
            resolve_public_path(&db, "/alpha/story")
                .await
                .unwrap()
                .unwrap()
                .redirect
        );

        // Switching to a reserved category slug is rejected under /%category%/%postname%.
        sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Media','media','vi','tg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap();
        let mut tx = db.begin().await.unwrap();
        assert_eq!(
            sync_claims_in_tx(&mut tx).await.unwrap_err().0,
            axum::http::StatusCode::CONFLICT
        );
        tx.rollback().await.unwrap();

        // /blog/%category%/%postname% (3 segs) allows CPT with /blog/{slug} (2 segs) but rejects /blog/docs/{slug} (3 segs).
        sqlx::query("DELETE FROM taxonomy_terms WHERE slug='media'")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO content_types(key,definition_json,owner_kind,owner_id,provider_status,declaration_state,version)
             SELECT 'project',definition_json,'user',NULL,'active','declared',1 FROM content_types WHERE key='post'",
        ).execute(&db).await.unwrap();
        let mut ok_mixed = s2.input();
        ok_mixed.post_structure = "/blog/%category%/%postname%".into();
        ok_mixed
            .content_type_bases
            .insert("project".into(), Some("blog".into()));
        let s3 = put_settings(&db, s2.version, &ok_mixed).await.unwrap();
        let mut conflict_3seg = s3.input();
        conflict_3seg
            .content_type_bases
            .insert("project".into(), Some("blog/docs".into()));
        assert_eq!(
            put_settings(&db, s3.version, &conflict_3seg)
                .await
                .unwrap_err()
                .status(),
            axum::http::StatusCode::CONFLICT
        );
    }

    #[tokio::test]
    async fn put_settings_collects_structured_409_conflicts_capped_at_20() {
        let db = site().await;
        let v1 = get_settings(&db).await.unwrap();
        let blog = put_settings(
            &db,
            v1.version,
            &PermalinkSettingsInput {
                post_structure: "/blog/%postname%".into(),
                content_type_bases: BTreeMap::new(),
                taxonomy_bases: BTreeMap::new(),
            },
        )
        .await
        .unwrap();
        for i in 1..=25 {
            reading_page(&db, &format!("dup-{i}"), true).await;
            add_post(&db, &format!("dup-{i}"), true).await;
        }
        let err = put_settings(
            &db,
            blog.version,
            &PermalinkSettingsInput {
                post_structure: "/%postname%".into(),
                content_type_bases: BTreeMap::new(),
                taxonomy_bases: BTreeMap::new(),
            },
        )
        .await
        .unwrap_err();
        match err {
            PermalinkPutError::Conflicts(payload) => {
                assert_eq!(payload.total_conflicts, 25);
                assert!(payload.truncated);
                assert_eq!(payload.conflicts.len(), 20);
            }
            other => panic!("expected structured PermalinkPutError::Conflicts, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn max_redirects_prunes_oldest_orphans_fifo_before_rejecting() {
        let db = site().await;
        sqlx::query(
            "WITH RECURSIVE cnt(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM cnt WHERE x <= 200001)
             INSERT INTO permalink_route_redirects(source_path, target_kind, target_id, locale, origin, created_at)
             SELECT '/old-orphan-' || x, 'entry', CAST(1000000 + x AS TEXT), 'vi', 'canonical',
                    strftime('%Y-%m-%dT%H:%M:%fZ', 1700000000 + x, 'unixepoch')
               FROM cnt",
        )
        .execute(&db)
        .await
        .unwrap();

        let mut tx = db.begin().await.unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM permalink_route_redirects")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(remaining as usize, MAX_REDIRECTS);
        // Oldest orphan (/old-orphan-1) pruned FIFO while newest remains.
        let oldest: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM permalink_route_redirects WHERE source_path='/old-orphan-1'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(oldest, 0);
    }
}
