//! C3: transactional authority for canonical routes and their stable-ID history.
//! All writers share sync_claims_in_tx before committing their source changes.

use super::*;

pub(super) use std::collections::{BTreeMap, HashSet};

pub(super) use opendash_core::content::valid_content_type_key;

pub(super) use opendash_core::id::valid_kebab_segment_with_limit;

pub(super) use serde::{Deserialize, Serialize};

pub(super) use sqlx::{SqliteConnection, SqlitePool};

pub(super) use crate::error::{ApiError, ApiResult};

pub(in crate::storage) const MAX_CLAIMS: usize = 200_000;

pub const MAX_REDIRECTS: usize = 200_000;

pub(in crate::storage) const MAX_CONFLICT_ITEMS: usize = 20;

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

/// An immutable-owner package declaration. `None` withdraws the default base,
/// but never creates or overwrites a site-level selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderRewriteDeclaration {
    pub resource_kind: &'static str,
    pub resource_key: String,
    pub rewrite_base: Option<String>,
}

#[derive(Debug, Clone)]
pub(in crate::storage) struct ProposedClaim {
    pub(in crate::storage) kind: String,
    pub(in crate::storage) id: String,
    pub(in crate::storage) locale: String,
    pub(in crate::storage) family: Option<String>,
    pub(in crate::storage) generated: bool,
}

#[derive(Debug)]
pub(in crate::storage) struct DynamicPattern {
    pub(in crate::storage) prefix: Vec<String>,
    pub(in crate::storage) seg_count: usize,
    pub(in crate::storage) kind: String,
    pub(in crate::storage) family: String,
    pub(in crate::storage) locale: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::storage) struct HistoricalOwner {
    pub(in crate::storage) claim: Claim,
    pub(in crate::storage) origin: String,
}

pub(in crate::storage) struct ConflictCollector {
    pub(in crate::storage) collect_all: bool,
    pub(in crate::storage) total_conflicts: usize,
    pub(in crate::storage) items: Vec<PermalinkConflictItem>,
    pub(in crate::storage) first_error: Option<String>,
}

impl ConflictCollector {
    pub(in crate::storage) fn new(collect_all: bool) -> Self {
        Self {
            collect_all,
            total_conflicts: 0,
            items: Vec::new(),
            first_error: None,
        }
    }

    pub(in crate::storage) fn record(
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

    pub(in crate::storage) fn finish(self) -> Result<(), PermalinkPutError> {
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
pub(in crate::storage) fn add_claim(
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

pub(crate) async fn sync_claims_with_options_and_swap_and_limit(
    conn: &mut SqliteConnection,
    collect_conflicts: bool,
    swapped_from: Option<&str>,
    max_redirects: usize,
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

    // Prune oldest deleted-target orphan redirects directly against DB tables when exceeding max_redirects.
    if historical.len() > max_redirects {
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
            if historical.len() <= max_redirects {
                break;
            }
            if historical.get(&path).is_some_and(is_orphan) {
                historical.remove(&path);
            }
        }
        if historical.len() > max_redirects {
            let extra_orphans: Vec<String> = historical
                .iter()
                .filter(|(_, entry)| is_orphan(entry))
                .map(|(path, _)| path.clone())
                .collect();
            for path in extra_orphans {
                if historical.len() <= max_redirects {
                    break;
                }
                historical.remove(&path);
            }
        }
        if historical.len() > max_redirects {
            record_redirect_limit(&mut collector, max_redirects)?;
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

impl PermalinkPutError {
    pub(crate) fn into_api_error(self) -> ApiError {
        match self {
            Self::Api(err) => err,
            Self::Conflicts(c) => ApiError::conflict(c.error),
        }
    }

    #[cfg(test)]
    pub fn status(&self) -> axum::http::StatusCode {
        match self {
            Self::Api(err) => err.0,
            Self::Conflicts(_) => axum::http::StatusCode::CONFLICT,
        }
    }
}

#[cfg(test)]
impl PermalinkSettings {
    pub(in crate::storage::permalinks) fn input(&self) -> PermalinkSettingsInput {
        PermalinkSettingsInput {
            post_structure: self.post_structure.clone(),
            content_type_bases: self.content_type_bases.clone(),
            taxonomy_bases: self.taxonomy_bases.clone(),
        }
    }
}
