//! C3: transactional authority for canonical routes and their stable-ID history.
//! All writers share sync_claims_in_tx before committing their source changes.
use std::collections::BTreeMap;

use opendash_core::content::valid_content_type_key;
use opendash_core::id::valid_kebab_segment_with_limit;
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};

use crate::error::{ApiError, ApiResult};

const LEGACY_POST: &str = "/content/post/%postname%";
const MAX_CLAIMS: usize = 50_000;

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
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum PublicRouteTarget {
    Home,
    Entry(i64),
    Term(i64),
    Archive(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedPublicRoute {
    pub target: PublicRouteTarget,
    pub canonical_path: String,
    pub redirect: bool,
}

type Claim = (String, String); // (stable target kind, stable target ID)

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
        return Err(ApiError::bad_request("Provider rewrite owner không hợp lệ."));
    }
    let before = read_effective_settings(conn).await?;
    let mut incoming = std::collections::BTreeSet::new();
    for item in declarations {
        if !matches!(item.resource_kind, "content_type" | "taxonomy")
            || !valid_content_type_key(&item.resource_key)
            || (item.resource_kind == "content_type"
                && matches!(item.resource_key.as_str(), "post" | "page" | "section"))
            || item.rewrite_base.as_deref().is_some_and(|base| !valid_base(base))
        {
            return Err(ApiError::bad_request("Provider rewrite_base không hợp lệ."));
        }
        if !incoming.insert((item.resource_kind, item.resource_key.as_str())) {
            return Err(ApiError::bad_request("Provider rewrite khai báo key trùng."));
        }
        let (existing_owner, existing_id): (String, Option<String>) = match item.resource_kind {
            "content_type" => sqlx::query_as(
                "SELECT owner_kind, owner_id FROM content_types WHERE key=?",
            )
            .bind(&item.resource_key)
            .fetch_optional(&mut *conn)
            .await?,
            _ => sqlx::query_as(
                "SELECT owner_kind, owner_id FROM taxonomy_definitions WHERE key=?",
            )
            .bind(&item.resource_key)
            .fetch_optional(&mut *conn)
            .await?,
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
            return Err(ApiError::conflict("Provider rewrite key đã thuộc owner khác."));
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
    let status = if active { "active" } else { "provider_inactive" };
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
    if segs[..segs.len() - 1]
        .iter()
        .any(|seg| !valid_kebab_segment_with_limit(seg, 64))
    {
        return Err(ApiError::bad_request(
            "Literal permalink phải là kebab-case, không được chứa token khác.",
        ));
    }
    if format != LEGACY_POST
        && segs.len() > 1
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
        if !valid_content_type_key(key) || key == "section" {
            return Err(ApiError::bad_request(format!(
                "CPT key không hợp lệ: {key}"
            )));
        }
        if matches!(key.as_str(), "post" | "page") && base.is_some() {
            return Err(ApiError::bad_request(format!(
                "Post/Page giữ route riêng theo C3, không nhận rewrite_base: {key}",
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
) -> ApiResult<PermalinkSettings> {
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE bootstrap SET initialized=initialized WHERE id=1")
        .execute(&mut *tx)
        .await?;
    let current = read_settings(&mut tx).await?;
    if current.version != expected_version {
        return Err(ApiError::precondition_failed(
            "Permalink settings đã thay đổi; hãy tải lại.",
        ));
    }
    validate_input_in_tx(&mut tx, input).await?;
    let cpt_json = serde_json::to_string(&input.content_type_bases)?;
    let taxonomy_json = serde_json::to_string(&input.taxonomy_bases)?;
    let updated = sqlx::query(
        "UPDATE site_permalink_settings SET version=version+1, post_structure=?,
           content_type_bases_json=?, taxonomy_bases_json=? WHERE id=1 AND version=?",
    )
    .bind(&input.post_structure)
    .bind(cpt_json)
    .bind(taxonomy_json)
    .bind(expected_version)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::precondition_failed(
            "Permalink settings đã thay đổi; hãy tải lại.",
        ));
    }
    sync_claims_in_tx(&mut tx).await?;
    let output = read_settings(&mut tx).await?;
    tx.commit().await?;
    Ok(output)
}

fn entry_path(
    settings: &PermalinkSettings,
    id: i64,
    key: &str,
    slug: &str,
    custom: Option<&str>,
) -> String {
    if let Some(path) = custom {
        return path.to_owned();
    }
    if key == "post" {
        return settings
            .post_structure
            .replace("%postname%", slug)
            .replace("%post_id%", &id.to_string());
    }
    if key != "page"
        && let Some(Some(base)) = settings.content_type_bases.get(key)
    {
        return format!("/{base}/{slug}");
    }
    format!("/content/{key}/{slug}")
}

fn term_path(settings: &PermalinkSettings, id: i64, kind: &str, slug: &str) -> String {
    let _ = id;
    match settings.taxonomy_bases.get(kind) {
        Some(Some(base)) => format!("/{base}/{slug}"),
        _ => format!("/taxonomy/{kind}/{slug}"),
    }
}

fn archive_path(settings: &PermalinkSettings, key: &str) -> String {
    match settings.content_type_bases.get(key) {
        Some(Some(base)) => format!("/{base}"),
        _ => format!("/content/{key}"),
    }
}

pub async fn canonical_entry_path_in_tx(
    conn: &mut SqliteConnection,
    id: i64,
) -> ApiResult<Option<String>> {
    sqlx::query_scalar(
        "SELECT c.path FROM permalink_route_claims c
           JOIN content_entries e ON c.target_kind='entry' AND c.target_id=CAST(e.id AS TEXT)
           JOIN content_types t ON t.key=e.content_type_key
          WHERE e.id=? AND e.content_type_key!='section' AND t.provider_status='active'
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
           JOIN taxonomy_definitions d ON d.key=t.kind
          WHERE t.id=? AND d.public=1 AND d.provider_status='active'
            AND d.declaration_state='declared'",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Into::into)
}

pub async fn canonical_archive_path_in_tx(
    conn: &mut SqliteConnection,
    key: &str,
) -> ApiResult<Option<String>> {
    sqlx::query_scalar(
        "SELECT c.path FROM permalink_route_claims c
           JOIN content_types t ON c.target_kind='archive' AND c.target_id=t.key
          WHERE t.key=? AND t.key!='section' AND t.provider_status='active'
            AND COALESCE(json_extract(t.definition_json,'$.settings.public'),1)=1
            AND COALESCE(json_extract(t.definition_json,'$.settings.has_archive'),1)=1",
    )
    .bind(key)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Into::into)
}

#[derive(Debug, Clone)]
struct ProposedClaim {
    kind: String,
    id: String,
    family: Option<String>,
    generated: bool,
}

#[derive(Debug)]
struct DynamicPattern {
    prefix: Vec<String>,
    kind: String,
    family: String,
}

impl DynamicPattern {
    fn matches(&self, path: &str) -> bool {
        let segments: Vec<_> = path.trim_start_matches('/').split('/').collect();
        segments.len() == self.prefix.len() + 1
            && self.prefix.iter().zip(segments.iter()).all(|(a, b)| a == b)
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

fn pattern_for(path: &str, kind: &str, family: &str) -> DynamicPattern {
    let mut segments: Vec<_> = path[1..].split('/').map(str::to_owned).collect();
    segments.pop();
    DynamicPattern {
        prefix: segments,
        kind: kind.to_owned(),
        family: family.to_owned(),
    }
}

/// Deleted stable targets no longer have an entry/term row from which to read
/// their family. The protected fixed legacy routes encode that family in the
/// URL, and therefore remain safe to retain as dead (404) redirects. Never
/// infer ownership from arbitrary custom URLs or rewrite bases.
fn legacy_family_of_deleted_target<'a>(path: &'a str, target_kind: &str) -> Option<&'a str> {
    let parts: Vec<_> = path.split('/').collect();
    if parts.len() != 4 || parts[0] != "" || parts[2].is_empty() || parts[3].is_empty() {
        return None;
    }
    match (target_kind, parts[1]) {
        ("entry", "content") | ("term", "taxonomy") => Some(parts[2]),
        _ => None,
    }
}

fn add_claim(
    all: &mut BTreeMap<String, ProposedClaim>,
    path: String,
    kind: &str,
    id: String,
    family: Option<String>,
    generated: bool,
) -> ApiResult<()> {
    if path.len() > 512
        || !path.starts_with('/')
        || path.contains('?')
        || path.contains('#')
        || path.contains('\\')
        || path.contains("//")
    {
        return Err(ApiError::conflict(format!(
            "permalink_invalid_claim:path={path} target={kind}:{id}"
        )));
    }
    if let Some(previous) = all.get(&path) {
        return Err(ApiError::conflict(format!(
            "permalink_collision:path={path} new={kind}:{id} existing={}:{}",
            previous.kind, previous.id,
        )));
    }
    all.insert(
        path,
        ProposedClaim {
            kind: kind.to_owned(),
            id,
            family,
            generated,
        },
    );
    if all.len() > MAX_CLAIMS {
        return Err(ApiError::conflict(
            "permalink_claim_limit: tối đa 50000 canonical claims.",
        ));
    }
    Ok(())
}

/// Always called while the caller already holds SQLite's single-writer
/// reservation. Any failure (including a namespace conflict) rolls back the
/// caller's data mutation as well as the proposed claims.
pub async fn sync_claims_in_tx(conn: &mut SqliteConnection) -> ApiResult<()> {
    let settings = read_effective_settings(conn).await?;
    let mut proposed = BTreeMap::<String, ProposedClaim>::new();
    // Historical routes carry only stable target IDs. Resolve their family
    // from the current rows when checking dynamic routes, not from the path.
    let mut target_families = BTreeMap::<Claim, String>::new();
    let mut legacy_aliases = Vec::<(String, Claim)>::new();
    add_claim(&mut proposed, "/".into(), "home", "site".into(), None, true)?;

    let cpts: Vec<(String, i64)> = sqlx::query_as(
        "SELECT key, COALESCE(json_extract(definition_json,'$.settings.has_archive'),1)
           FROM content_types WHERE key!='section' ORDER BY key",
    )
    .fetch_all(&mut *conn)
    .await?;
    let taxonomies: Vec<(String,)> =
        sqlx::query_as("SELECT key FROM taxonomy_definitions ORDER BY key")
            .fetch_all(&mut *conn)
            .await?;

    // Distinct dynamic patterns need distinct route authority even when the
    // current database does not yet contain a colliding entry or term.
    let mut patterns = vec![pattern_for(&settings.post_structure, "entry", "post")];
    for (key, _) in &cpts {
        if key != "post" {
            let generated = format!("{}/%postname%", archive_path(&settings, key));
            patterns.push(pattern_for(&generated, "entry", key));
        }
    }
    for (key,) in &taxonomies {
        let template = match settings.taxonomy_bases.get(key) {
            Some(Some(base)) => format!("/{base}/%term%"),
            _ => format!("/taxonomy/{key}/%term%"),
        };
        patterns.push(pattern_for(&template, "term", key));
    }
    for (i, lhs) in patterns.iter().enumerate() {
        for rhs in patterns.iter().skip(i + 1) {
            if lhs.prefix == rhs.prefix {
                return Err(ApiError::conflict(format!(
                    "permalink_pattern_collision:{}:{} and {}:{} at {}",
                    lhs.kind,
                    lhs.family,
                    rhs.kind,
                    rhs.family,
                    lhs.describe(),
                )));
            }
        }
    }

    let entries: Vec<(i64, String, String, Option<String>)> = sqlx::query_as(
        "SELECT e.id,e.content_type_key,e.slug,p.path
           FROM content_entries e LEFT JOIN entry_public_paths p ON p.entry_id=e.id
          WHERE e.content_type_key!='section' ORDER BY e.id",
    )
    .fetch_all(&mut *conn)
    .await?;
    for (id, key, slug, custom) in entries {
        target_families.insert(("entry".to_owned(), id.to_string()), key.clone());
        if let Some(custom) = &custom {
            crate::storage::seo::validate_c3_custom_entry_path(custom)
                .map_err(|e| ApiError::conflict(format!("invalid_custom_path:entry:{id}: {e}",)))?;
        }
        let generated_path = entry_path(&settings, id, &key, &slug, None);
        let generated = custom.as_ref().is_none_or(|p| p == &generated_path);
        let current_path = entry_path(&settings, id, &key, &slug, custom.as_deref());
        let old_path = format!("/content/{key}/{slug}");
        if current_path != old_path {
            legacy_aliases.push((old_path, ("entry".to_owned(), id.to_string())));
        }
        add_claim(
            &mut proposed,
            current_path,
            "entry",
            id.to_string(),
            Some(key),
            generated,
        )?;
    }
    for (key, archive_enabled) in cpts {
        if archive_enabled != 0 {
            let current_path = archive_path(&settings, &key);
            let old_path = format!("/content/{key}");
            if current_path != old_path {
                legacy_aliases.push((old_path, ("archive".to_owned(), key.clone())));
            }
            add_claim(&mut proposed, current_path, "archive", key, None, true)?;
        }
    }
    let terms: Vec<(i64, String, String)> =
        sqlx::query_as("SELECT id, kind, slug FROM taxonomy_terms ORDER BY id")
            .fetch_all(&mut *conn)
            .await?;
    for (id, kind, slug) in terms {
        target_families.insert(("term".to_owned(), id.to_string()), kind.clone());
        let current_path = term_path(&settings, id, &kind, &slug);
        let old_path = format!("/taxonomy/{kind}/{slug}");
        if current_path != old_path {
            legacy_aliases.push((old_path, ("term".to_owned(), id.to_string())));
        }
        add_claim(
            &mut proposed,
            current_path,
            "term",
            id.to_string(),
            Some(kind),
            true,
        )?;
    }

    for (path, proposed_claim) in &proposed {
        if proposed_claim.kind == "home" {
            continue;
        }
        let root = path.trim_start_matches('/').split('/').next().unwrap_or("");
        let legacy_content = root == "content"
            && proposed_claim.generated
            && matches!(proposed_claim.kind.as_str(), "entry" | "archive");
        let legacy_taxonomy =
            root == "taxonomy" && proposed_claim.generated && proposed_claim.kind == "term";
        if !(legacy_content || legacy_taxonomy) && crate::storage::seo::is_reserved_route_root(root)
        {
            return Err(ApiError::conflict(format!(
                "permalink_reserved_collision:path={path} target={}:{}",
                proposed_claim.kind, proposed_claim.id,
            )));
        }
        // Static routes, custom aliases, and archives may not shadow a
        // generated dynamic namespace belonging to a different family.
        for pattern in &patterns {
            if pattern.matches(path)
                && !(proposed_claim.generated
                    && proposed_claim.kind == pattern.kind
                    && proposed_claim.family.as_deref() == Some(&pattern.family))
            {
                return Err(ApiError::conflict(format!(
                    "permalink_pattern_collision:path={path} target={}:{} pattern={}:{}",
                    proposed_claim.kind, proposed_claim.id, pattern.kind, pattern.family,
                )));
            }
        }
    }

    let previous: Vec<(String, String, String)> =
        sqlx::query_as("SELECT path,target_kind,target_id FROM permalink_route_claims")
            .fetch_all(&mut *conn)
            .await?;
    let old_redirects: Vec<(String, String, String)> =
        sqlx::query_as("SELECT source_path,target_kind,target_id FROM permalink_route_redirects")
            .fetch_all(&mut *conn)
            .await?;
    let mut historical = BTreeMap::<String, Claim>::new();
    for (path, kind, id) in old_redirects {
        historical.insert(path, (kind, id));
    }
    let previous_history = historical.clone();
    // Compatibility: until all legacy entry mutation paths are replaced,
    // migration0027 redirects remain an input to the new global authority.
    let old_entry_redirects: Vec<(String, i64)> =
        sqlx::query_as("SELECT source_path,entry_id FROM entry_redirects")
            .fetch_all(&mut *conn)
            .await?;
    for (path, id) in old_entry_redirects {
        let owner = ("entry".to_owned(), id.to_string());
        if let Some(other) = historical.insert(path.clone(), owner.clone()) {
            if other != owner {
                return Err(ApiError::conflict(format!(
                    "permalink_redirect_collision:path={path} legacy=entry:{id} owner={}:{}",
                    other.0, other.1,
                )));
            }
        }
    }
    for (path, owner) in legacy_aliases {
        if let Some(other) = historical.insert(path.clone(), owner.clone()) {
            if other != owner {
                return Err(ApiError::conflict(format!(
                    "permalink_redirect_collision:path={path} new={}:{} owner={}:{}",
                    owner.0, owner.1, other.0, other.1,
                )));
            }
        }
    }
    for (path, kind, id) in &previous {
        let owner = (kind.clone(), id.clone());
        if kind != "home"
            && !proposed
                .get(path)
                .is_some_and(|p| p.kind == *kind && p.id == *id)
        {
            if let Some(other) = historical.insert(path.clone(), owner.clone()) {
                if other != owner {
                    return Err(ApiError::conflict(format!(
                        "permalink_redirect_collision:path={path} old={kind}:{id} owner={}:{}",
                        other.0, other.1,
                    )));
                }
            }
        }
    }
    for (path, candidate) in &proposed {
        if let Some(owner) = historical.get(path) {
            if owner.0 == candidate.kind && owner.1 == candidate.id {
                historical.remove(path); // Reclaim own old canonical URL.
            } else {
                return Err(ApiError::conflict(format!(
                    "permalink_collision:path={path} new={}:{} redirect_owner={}:{}",
                    candidate.kind, candidate.id, owner.0, owner.1,
                )));
            }
        }
    }
    // Redirects and legacy fallbacks also occupy the complete route
    // namespace. A URL owned by post cannot shadow another CPT's generated
    // /base/{slug} pattern even if that CPT has no entries yet. Keep aliases
    // inside their own dynamic family; an owner's canonical reclaim was
    // already removed from historical above.
    //
    // A deleted stable ID has no row in target_families. Its C3-generated
    // protected legacy alias (/content/{type}/{slug} or
    // /taxonomy/{kind}/{slug}) is nevertheless retained for that exact ID,
    // including when its former canonical URL used a custom rewrite base.
    // Recover the family only from such owner-matched protected aliases, not
    // from the custom/rewrite URL itself. Conflicting proofs fail closed.
    let mut orphan_families = BTreeMap::<Claim, Option<String>>::new();
    for (path, owner) in &historical {
        if target_families.contains_key(owner) {
            continue;
        }
        if let Some(family) = legacy_family_of_deleted_target(path, &owner.0) {
            orphan_families
                .entry(owner.clone())
                .and_modify(|known| {
                    if known.as_deref() != Some(family) {
                        *known = None;
                    }
                })
                .or_insert_with(|| Some(family.to_owned()));
        }
    }
    for (path, owner) in &historical {
        for pattern in &patterns {
            if pattern.matches(path)
                && !(owner.0 == pattern.kind
                    && match target_families.get(owner) {
                        Some(family) => family == &pattern.family,
                        None => orphan_families.get(owner).and_then(Option::as_deref)
                            == Some(pattern.family.as_str()),
                    })
            {
                return Err(ApiError::conflict(format!(
                    "permalink_pattern_collision:redirect={path} owner={}:{} pattern={}:{}",
                    owner.0, owner.1, pattern.kind, pattern.family,
                )));
            }
        }
    }

    // Read-only validation above is complete. Only changed claims/redirects
    // are written, avoiding a full database rewrite for every content save.
    let previous_map: BTreeMap<_, _> = previous
        .into_iter()
        .map(|(path, kind, id)| (path, (kind, id)))
        .collect();
    for (path, owner) in &previous_map {
        if !proposed
            .get(path)
            .is_some_and(|now| now.kind == owner.0 && now.id == owner.1)
        {
            sqlx::query("DELETE FROM permalink_route_claims WHERE path=?")
                .bind(path)
                .execute(&mut *conn)
                .await?;
        }
    }
    for (path, now) in &proposed {
        if !previous_map
            .get(path)
            .is_some_and(|old| old.0 == now.kind && old.1 == now.id)
        {
            sqlx::query(
                "INSERT INTO permalink_route_claims(path,target_kind,target_id) VALUES (?,?,?)",
            )
            .bind(path)
            .bind(&now.kind)
            .bind(&now.id)
            .execute(&mut *conn)
            .await?;
        }
    }
    for (path, owner) in &previous_history {
        if historical.get(path) != Some(owner) {
            sqlx::query("DELETE FROM permalink_route_redirects WHERE source_path=?")
                .bind(path)
                .execute(&mut *conn)
                .await?;
        }
    }
    for (path, (kind, id)) in historical {
        if previous_history.get(&path) != Some(&(kind.clone(), id.clone())) {
            sqlx::query(
                "INSERT INTO permalink_route_redirects(source_path,target_kind,target_id) VALUES (?,?,?)",
            ).bind(path).bind(kind).bind(id).execute(&mut *conn).await?;
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
    let current: Option<(String, String)> =
        sqlx::query_as("SELECT target_kind,target_id FROM permalink_route_claims WHERE path=?")
            .bind(path)
            .fetch_optional(&mut *tx)
            .await?;
    let (kind, id, redirect) =
        match current {
            Some((kind, id)) => (kind, id, false),
            None => {
                let old: Option<(String, String)> = sqlx::query_as(
                "SELECT target_kind,target_id FROM permalink_route_redirects WHERE source_path=?",
            ).bind(path).fetch_optional(&mut *tx).await?;
                match old {
                    Some((kind, id)) => (kind, id, true),
                    None => return Ok(None),
                }
            }
        };
    let (target, canonical_path) = match kind.as_str() {
        "home" if !redirect && path == "/" => (PublicRouteTarget::Home, "/".to_owned()),
        "entry" => {
            let Ok(entry_id) = id.parse::<i64>() else {
                return Ok(None);
            };
            let visible: i64 = sqlx::query_scalar(
                "SELECT EXISTS(
                   SELECT 1 FROM content_entries e JOIN content_types t ON t.key=e.content_type_key
                   WHERE e.id=? AND e.publication_status='published'
                     AND e.content_type_key!='section' AND t.provider_status='active'
                     AND COALESCE(json_extract(t.definition_json,'$.settings.public'),1)=1
                 )",
            )
            .bind(entry_id)
            .fetch_one(&mut *tx)
            .await?;
            if visible == 0 {
                return Ok(None);
            }
            let Some(canonical) = canonical_entry_path_in_tx(&mut tx, entry_id).await? else {
                return Ok(None);
            };
            (PublicRouteTarget::Entry(entry_id), canonical)
        }
        "term" => {
            let Ok(term_id) = id.parse::<i64>() else {
                return Ok(None);
            };
            let Some(canonical) = canonical_term_path_in_tx(&mut tx, term_id).await? else {
                return Ok(None);
            };
            (PublicRouteTarget::Term(term_id), canonical)
        }
        "archive" => {
            let Some(canonical) = canonical_archive_path_in_tx(&mut tx, &id).await? else {
                return Ok(None);
            };
            (PublicRouteTarget::Archive(id), canonical)
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

    async fn add_post(db: &SqlitePool, slug: &str, published: bool) -> i64 {
        let id = sqlx::query(
            "INSERT INTO content_entries
             (content_type_key,slug,title,values_json,publication_status)
             VALUES('post',?,'Example','{}','draft')",
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
        assert_eq!(after, before, "migration must not infer provider ownership");
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
        sync_provider_rewrite_defaults_in_tx(&mut tx, "plugin", "books_plugin", &[declaration.clone()])
            .await
            .unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        let enabled = get_settings(&db).await.unwrap();
        assert_eq!(enabled.version, before.version + 1);
        assert!(!enabled.content_type_bases.contains_key("books"));
        let mut conn = db.acquire().await.unwrap();
        let effective = read_effective_settings(&mut conn).await.unwrap();
        assert_eq!(effective.content_type_bases["books"], Some("library".into()));
        drop(conn);

        let mut changed = declaration.clone();
        changed.rewrite_base = Some("catalog".into());
        let mut tx = db.begin().await.unwrap();
        sync_provider_rewrite_defaults_in_tx(&mut tx, "plugin", "books_plugin", &[changed.clone()])
            .await.unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        let modified = get_settings(&db).await.unwrap();
        assert_eq!(modified.version, enabled.version + 1,
            "updating an effective provider default invalidates an older site editor CAS");
        assert!(!modified.content_type_bases.contains_key("books"));
        let mut conn = db.acquire().await.unwrap();
        assert_eq!(read_effective_settings(&mut conn).await.unwrap().content_type_bases["books"],
            Some("catalog".to_owned()));
        drop(conn);

        let mut explicit_legacy = get_settings(&db).await.unwrap().input();
        explicit_legacy.content_type_bases.insert("books".into(), None);
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
        assert_eq!(final_site.version, saved.version, "provider defaults hidden by site choice");
        assert_eq!(final_site.content_type_bases.get("books"), Some(&None));
        let mut conn = db.acquire().await.unwrap();
        assert_eq!(read_effective_settings(&mut conn).await.unwrap().content_type_bases["books"], None);
        drop(conn);
        let mut tx = db.begin().await.unwrap();
        set_provider_rewrite_status_in_tx(&mut tx, "plugin", "books_plugin", false).await.unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(get_settings(&db).await.unwrap().version, saved.version);
        let mut tx = db.begin().await.unwrap();
        set_provider_rewrite_status_in_tx(&mut tx, "plugin", "books_plugin", true).await.unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(get_settings(&db).await.unwrap().version, saved.version,
            "explicit null must survive disable and re-enable");
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

        migrator.run(&db).await.unwrap();
        assert_eq!(
            resolve_public_path(&db, "/content/page/upgrade-page")
                .await
                .unwrap(),
            Some(ResolvedPublicRoute {
                target: PublicRouteTarget::Entry(page_id),
                canonical_path: "/old-page-custom".to_owned(),
                redirect: true,
            }),
        );
        assert_eq!(
            resolve_public_path(&db, "/old-page-custom").await.unwrap(),
            Some(ResolvedPublicRoute {
                target: PublicRouteTarget::Entry(page_id),
                canonical_path: "/old-page-custom".to_owned(),
                redirect: false,
            }),
        );
        assert_eq!(
            resolve_public_path(&db, "/content/post/untouched-post")
                .await
                .unwrap(),
            Some(ResolvedPublicRoute {
                target: PublicRouteTarget::Entry(untouched),
                canonical_path: "/content/post/untouched-post".to_owned(),
                redirect: false,
            }),
        );
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
        migrator.run(&db).await.unwrap();
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
        let legacy = "/content/post/first";
        assert_eq!(resolve_public_path(&db, legacy).await.unwrap(), None);
        let config = get_settings(&db).await.unwrap();
        assert_eq!(config.version, 1);
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
        assert_eq!(resolve_public_path(&db, legacy).await.unwrap(), None);

        let mut tx = db.begin().await.unwrap();
        sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            resolve_public_path(&db, legacy).await.unwrap(),
            Some(ResolvedPublicRoute {
                target: PublicRouteTarget::Entry(id),
                canonical_path: "/blog/first".into(),
                redirect: true,
            }),
        );
        assert_eq!(
            resolve_public_path(&db, "/blog/first").await.unwrap(),
            Some(ResolvedPublicRoute {
                target: PublicRouteTarget::Entry(id),
                canonical_path: "/blog/first".into(),
                redirect: false,
            }),
        );
        // Reclaimed custom URL that equals the generated route is unambiguous.
        let mut tx = db.begin().await.unwrap();
        sqlx::query("INSERT INTO entry_public_paths(entry_id,path) VALUES(?,?)")
            .bind(id)
            .bind("/blog/first")
            .execute(&mut *tx)
            .await
            .unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        let prior = get_settings(&db).await.unwrap();
        assert_eq!(
            put_settings(&db, 1, &prior.input()).await.unwrap_err().0,
            axum::http::StatusCode::PRECONDITION_FAILED
        );
    }

    #[tokio::test]
    async fn conflicting_permalinks_abort_configuration_and_preserve_history() {
        let db = site().await;
        let id = add_post(&db, "admin", true).await;
        let original = get_settings(&db).await.unwrap();
        let bad = PermalinkSettingsInput {
            post_structure: "/%postname%".into(),
            content_type_bases: BTreeMap::new(),
            taxonomy_bases: BTreeMap::new(),
        };
        let err = put_settings(&db, original.version, &bad).await.unwrap_err();
        assert_eq!(err.0, axum::http::StatusCode::CONFLICT);
        assert_eq!(get_settings(&db).await.unwrap(), original);
        assert_eq!(
            canonical_entry_path_in_tx(&mut db.acquire().await.unwrap(), id)
                .await
                .unwrap(),
            Some("/content/post/admin".into())
        );

        let mut other = original.input();
        other.post_structure = "/blog/%postname%".into();
        other
            .taxonomy_bases
            .insert("category".into(), Some("blog".into()));
        let err = put_settings(&db, original.version, &other)
            .await
            .unwrap_err();
        assert_eq!(err.0, axum::http::StatusCode::CONFLICT);
        assert_eq!(get_settings(&db).await.unwrap(), original);
        assert!(
            resolve_public_path(&db, "/content/post/admin")
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn term_history_follows_rewrite_and_slug_updates_and_hides_private_terms() {
        let db = site().await;
        let id = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug) VALUES('category','Sea','sea')",
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
            resolve_public_path(&db, "/taxonomy/category/sea")
                .await
                .unwrap()
                .unwrap(),
            ResolvedPublicRoute {
                target: PublicRouteTarget::Term(id),
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
            resolve_public_path(&db, "/taxonomy/category/sea")
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
        put_settings(
            &db,
            config.version,
            &PermalinkSettingsInput {
                post_structure: "/%postname%".into(),
                content_type_bases: BTreeMap::new(),
                taxonomy_bases: BTreeMap::new(),
            },
        )
        .await
        .unwrap();
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
            PublicRouteTarget::Entry(id),
        );
        assert_eq!(resolve_public_path(&db, "/admin").await.unwrap(), None);
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
                target: PublicRouteTarget::Entry(post_id),
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
        assert_eq!(err.0, axum::http::StatusCode::CONFLICT, "{err:?}");
        assert_eq!(get_settings(&db).await.unwrap(), latest);
        assert_eq!(
            resolve_public_path(&db, "/blog/alpha").await.unwrap(),
            Some(ResolvedPublicRoute {
                target: PublicRouteTarget::Entry(post_id),
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
                target: PublicRouteTarget::Entry(post_id),
                canonical_path: "/blog/alpha".into(),
                redirect: false,
            })
        );
        assert_eq!(
            resolve_public_path(&db, "/content/post/alpha")
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

        // Migration 0027 redirects are still a source of truth when
        // permalink claims are rebuilt by later settings/CPT mutations.
        sqlx::query("INSERT INTO entry_redirects(source_path,entry_id) VALUES('/books/alpha', ?)")
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
        assert_eq!(error.0, axum::http::StatusCode::CONFLICT, "{error:?}");
        assert_eq!(get_settings(&db).await.unwrap(), config);
        assert_eq!(
            resolve_public_path(&db, "/news/alpha")
                .await
                .unwrap()
                .unwrap()
                .target,
            PublicRouteTarget::Entry(post_id)
        );
        assert_eq!(resolve_public_path(&db, "/books").await.unwrap(), None);
    }

    #[tokio::test]
    async fn deleted_term_legacy_alias_remains_404_without_blocking_atomic_delete() {
        let db = site().await;
        let term_id = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug) VALUES('category','Parent','parent')",
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
        // Before the fix, the former canonical URL is turned into a redirect
        // whose target ID no longer has a row: missing target_families wrongly
        // triggers a conflict with the category family's own legacy pattern.
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            resolve_public_path(&db, "/taxonomy/category/parent")
                .await
                .unwrap(),
            None,
        );
        let historical: Option<(String, String)> = sqlx::query_as(
            "SELECT target_kind,target_id FROM permalink_route_redirects
              WHERE source_path='/taxonomy/category/parent'",
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
        assert_eq!(
            resolve_public_path(&db, "/content/post/old-post")
                .await
                .unwrap(),
            None,
        );
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
            PublicRouteTarget::Entry(post_id),
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
        assert_eq!(
            resolve_public_path(&db, "/content/post/gone").await.unwrap(),
            None,
        );
    }

    #[tokio::test]
    async fn deleted_rewritten_term_retains_dead_aliases_without_blocking_delete() {
        let db = site().await;
        let original = get_settings(&db).await.unwrap();
        let mut genres = original.input();
        genres
            .taxonomy_bases
            .insert("category".to_owned(), Some("genres".to_owned()));
        put_settings(&db, original.version, &genres)
            .await
            .unwrap();
        let term_id = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug) VALUES('category','Gone','gone')",
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
            PublicRouteTarget::Term(term_id),
        );

        let mut tx = db.begin().await.unwrap();
        sqlx::query("DELETE FROM taxonomy_terms WHERE id=?")
            .bind(term_id)
            .execute(&mut *tx)
            .await
            .unwrap();
        sync_claims_in_tx(&mut tx).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(resolve_public_path(&db, "/genres/gone").await.unwrap(), None);
        assert_eq!(
            resolve_public_path(&db, "/taxonomy/category/gone")
                .await
                .unwrap(),
            None,
        );
    }

    #[tokio::test]
    async fn dead_rewritten_post_alias_cannot_shadow_foreign_cpt_family() {
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
        let mut conflicting = news.clone();
        conflicting
            .content_type_bases
            .insert("books".into(), Some("blog".into()));
        let err = put_settings(&db, news_saved.version, &conflicting)
            .await
            .unwrap_err();
        assert_eq!(err.0, axum::http::StatusCode::CONFLICT, "{err:?}");
        assert_eq!(get_settings(&db).await.unwrap(), news_saved);
        assert_eq!(resolve_public_path(&db, "/blog/gone").await.unwrap(), None);
    }

    #[tokio::test]
    async fn missing_target_with_unproven_family_must_not_shadow_foreign_rewrite() {
        let db = site().await;
        sqlx::query(
            "INSERT INTO permalink_route_redirects(source_path,target_kind,target_id)
             VALUES('/books/orphan','entry','999999')",
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
        let error = put_settings(&db, initial.version, &attempted)
            .await
            .unwrap_err();
        assert_eq!(error.0, axum::http::StatusCode::CONFLICT, "{error:?}");
        assert_eq!(get_settings(&db).await.unwrap(), initial);
        assert_eq!(
            resolve_public_path(&db, "/books/orphan").await.unwrap(),
            None
        );
    }

    #[test]
    fn claims_beyond_fifty_thousand_are_rejected_before_writing() {
        let mut claims = BTreeMap::new();
        for id in 1..=MAX_CLAIMS {
            add_claim(
                &mut claims,
                format!("/claim-{id}"),
                "entry",
                id.to_string(),
                Some("post".to_owned()),
                true,
            )
            .unwrap();
        }
        let err = add_claim(
            &mut claims,
            "/claim-overflow".into(),
            "entry",
            "50001".into(),
            Some("post".to_owned()),
            true,
        )
        .unwrap_err();
        assert_eq!(err.0, axum::http::StatusCode::CONFLICT);
        assert_eq!(claims.len(), MAX_CLAIMS + 1);
    }
}
