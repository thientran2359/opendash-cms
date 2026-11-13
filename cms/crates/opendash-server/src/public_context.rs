//! Public route identity and canonical links. No session, preview token or
//! filesystem detail may enter a public context.
use std::collections::{BTreeMap, HashSet};

use opendash_core::{
    content::valid_content_type_key,
    id::{valid_kebab_segment_with_limit, valid_package_id},
};

pub const ARCHIVE_SIZE: i64 = 20;
pub const MAX_ARCHIVE_PAGE: i64 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaxonomyKind {
    Category,
    Tag,
}

impl TaxonomyKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Category => "category",
            Self::Tag => "tag",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PublicFilters {
    pub category_id: Option<i64>,
    pub tag_id: Option<i64>,
    pub descendants: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicQuery {
    pub page: i64,
    pub filters: PublicFilters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryScope {
    Single,
    ContentArchive,
    TaxonomyArchive(TaxonomyKind),
}

/// The only recognized query keys are validated strictly. Unknown tracking
/// keys are ignored, including repeated ones, and never emitted in links.
pub fn parse_query(raw: Option<&str>, scope: QueryScope) -> Result<PublicQuery, &'static str> {
    let raw = raw.unwrap_or("");
    let bytes = raw.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == b'%'
            && (index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit())
        {
            return Err("Query có mã hóa không hợp lệ.");
        }
    }
    let mut seen = HashSet::new();
    let mut query = PublicQuery {
        page: 1,
        filters: PublicFilters::default(),
    };
    for (key, value) in url::form_urlencoded::parse(raw.as_bytes()) {
        let recognized = matches!(
            key.as_ref(),
            "page" | "category_id" | "tag_id" | "include_descendants" | "descendants"
        );
        if !recognized {
            continue;
        }
        if !seen.insert(key.to_string()) {
            return Err("Không được lặp tham số công khai.");
        }
        let numeric = || {
            value
                .parse::<i64>()
                .ok()
                .filter(|n| *n > 0 && n.to_string() == value)
                .ok_or("Giá trị query không hợp lệ.")
        };
        let boolean = || match value.as_ref() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err("Giá trị boolean không hợp lệ."),
        };
        match key.as_ref() {
            "page" if !matches!(scope, QueryScope::Single) => {
                query.page = numeric()?;
                if query.page > MAX_ARCHIVE_PAGE {
                    return Err("Trang công khai vượt giới hạn.");
                }
            }
            "category_id" if !matches!(scope, QueryScope::Single) => {
                query.filters.category_id = Some(numeric()?);
            }
            "tag_id" if !matches!(scope, QueryScope::Single) => {
                query.filters.tag_id = Some(numeric()?);
            }
            "include_descendants" if matches!(scope, QueryScope::ContentArchive) => {
                query.filters.descendants = boolean()?;
            }
            "descendants" if matches!(scope, QueryScope::TaxonomyArchive(_)) => {
                query.filters.descendants = boolean()?;
            }
            _ => return Err("Tham số query không dùng được trên route này."),
        }
    }
    match scope {
        QueryScope::ContentArchive
            if query.filters.descendants && query.filters.category_id.is_none() =>
        {
            Err("include_descendants yêu cầu category_id.")
        }
        QueryScope::TaxonomyArchive(TaxonomyKind::Tag)
            if query.filters.descendants && query.filters.category_id.is_none() =>
        {
            Err("descendants yêu cầu category_id trên tag archive.")
        }
        _ => Ok(query),
    }
}

pub fn validate_single_query(raw: Option<&str>) -> Result<(), &'static str> {
    parse_query(raw, QueryScope::Single).map(|_| ())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublicRequestContext {
    Home {
        canonical_path: String,
    },
    Single {
        entry_id: i64,
        type_key: String,
        slug: String,
        canonical_path: String,
    },
    Archive {
        type_key: String,
        query: PublicQuery,
        canonical_path: String,
    },
    TaxonomyArchive {
        term_id: i64,
        kind: TaxonomyKind,
        slug: String,
        query: PublicQuery,
        canonical_path: String,
    },
    NotFound,
}

pub struct CanonicalUrlBuilder;

impl CanonicalUrlBuilder {
    pub fn home() -> &'static str {
        "/"
    }

    pub fn valid_type(type_key: &str) -> bool {
        valid_content_type_key(type_key)
            && !matches!(
                type_key,
                "api"
                    | "admin"
                    | "auth"
                    | "assets"
                    | "builder"
                    | "content"
                    | "media"
                    | "preview"
                    | "registry"
                    | "save"
                    | "taxonomy"
                    | "theme"
                    | "themes"
                    | "uploads"
            )
    }

    pub fn archive(type_key: &str) -> Option<String> {
        Self::valid_type(type_key).then(|| format!("/content/{type_key}"))
    }

    pub fn entry(type_key: &str, slug: &str) -> Option<String> {
        (Self::valid_type(type_key) && valid_kebab_segment_with_limit(slug, 64))
            .then(|| format!("/content/{type_key}/{slug}"))
    }

    pub fn entry_from_id(id: i64, type_key: &str, slug: &str) -> Option<String> {
        (id > 0).then(|| Self::entry(type_key, slug)).flatten()
    }

    pub fn taxonomy(kind: TaxonomyKind, slug: &str) -> Option<String> {
        valid_kebab_segment_with_limit(slug, 64)
            .then(|| format!("/taxonomy/{}/{slug}", kind.as_str()))
    }

    pub fn term_from_id(id: i64, kind: TaxonomyKind, slug: &str) -> Option<String> {
        (id > 0).then(|| Self::taxonomy(kind, slug)).flatten()
    }

    pub fn content_page(type_key: &str, query: PublicQuery) -> Option<String> {
        let base = Self::archive(type_key)?;
        Some(Self::with_query(base, query, QueryScope::ContentArchive))
    }

    pub fn taxonomy_page(kind: TaxonomyKind, slug: &str, query: PublicQuery) -> Option<String> {
        let base = Self::taxonomy(kind, slug)?;
        Some(Self::with_query(
            base,
            query,
            QueryScope::TaxonomyArchive(kind),
        ))
    }

    fn with_query(base: String, query: PublicQuery, scope: QueryScope) -> String {
        let mut pairs = url::form_urlencoded::Serializer::new(String::new());
        if query.page > 1 {
            pairs.append_pair("page", &query.page.to_string());
        }
        if let Some(id) = query.filters.category_id {
            pairs.append_pair("category_id", &id.to_string());
        }
        if let Some(id) = query.filters.tag_id {
            pairs.append_pair("tag_id", &id.to_string());
        }
        if query.filters.descendants {
            let key = if matches!(scope, QueryScope::ContentArchive) {
                "include_descendants"
            } else {
                "descendants"
            };
            pairs.append_pair(key, "true");
        }
        let query = pairs.finish();
        if query.is_empty() {
            base
        } else {
            format!("{base}?{query}")
        }
    }

    /// Builder list pagination has its own M4 contract (1..100, not the
    /// content archive's 1..10,000). Never copy unknown request query keys.
    pub fn home_list_pages(pages: &BTreeMap<String, i64>) -> String {
        let mut pairs = url::form_urlencoded::Serializer::new(String::new());
        for (id, page) in pages {
            if valid_package_id(id) && (2..=100).contains(page) {
                pairs.append_pair(&format!("list_page_{id}"), &page.to_string());
            }
        }
        let query = pairs.finish();
        if query.is_empty() {
            Self::home().into()
        } else {
            format!("/?{query}")
        }
    }

    /// A user-bound URL may point outside the CMS. Only known public route
    /// namespaces are rebuilt, with strict typed query parsing.
    pub fn normalize_internal_link(input: &str) -> Option<String> {
        if !input.starts_with("/content/") && !input.starts_with("/taxonomy/") {
            return Some(input.to_string());
        }
        let (path, raw) = input
            .split_once('?')
            .map_or((input, None), |(a, b)| (a, Some(b)));
        if path.contains('%') || path.contains('#') {
            return None;
        }
        let segments: Vec<_> = path.trim_start_matches('/').split('/').collect();
        match segments.as_slice() {
            ["content", type_key] => {
                Self::content_page(type_key, parse_query(raw, QueryScope::ContentArchive).ok()?)
            }
            ["content", type_key, slug] => {
                validate_single_query(raw).ok()?;
                Self::entry(type_key, slug)
            }
            ["taxonomy", "category", slug] => Self::taxonomy_page(
                TaxonomyKind::Category,
                slug,
                parse_query(raw, QueryScope::TaxonomyArchive(TaxonomyKind::Category)).ok()?,
            ),
            ["taxonomy", "tag", slug] => Self::taxonomy_page(
                TaxonomyKind::Tag,
                slug,
                parse_query(raw, QueryScope::TaxonomyArchive(TaxonomyKind::Tag)).ok()?,
            ),
            _ => None,
        }
    }
}
