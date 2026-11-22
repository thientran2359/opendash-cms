//! Public route identity and canonical links. No session, preview token or
//! filesystem detail may enter a public context.
use std::collections::{BTreeMap, HashSet};

use opendash_core::{
    content::valid_content_type_key,
    id::{valid_kebab_segment_with_limit, valid_package_id},
};

pub const ARCHIVE_SIZE: i64 = 20;
pub const MAX_ARCHIVE_PAGE: i64 = 10_000;

/// Single source of truth for the 25 reserved system path prefixes.
const RESERVED_SYSTEM_ROOTS: &[&str] = &[
    "admin",
    "api",
    "assets",
    "auth",
    "builder",
    "content",
    "favicon.ico",
    "forms",
    "health",
    "media",
    "page",
    "pattern",
    "preview",
    "registry",
    "robots.txt",
    "save",
    "section",
    "sitemap.xml",
    "sitemaps",
    "static",
    "studio",
    "taxonomy",
    "theme",
    "themes",
    "uploads",
];

/// Returns `true` when `root` matches any closed reserved system path prefix.
pub fn is_reserved_system_root(root: &str) -> bool {
    RESERVED_SYSTEM_ROOTS.contains(&root)
}

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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PublicFilters {
    pub category_id: Option<i64>,
    pub tag_id: Option<i64>,
    pub taxonomy_key: Option<String>,
    pub term_id: Option<i64>,
    pub descendants: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
            "page"
                | "category_id"
                | "tag_id"
                | "taxonomy_key"
                | "term_id"
                | "include_descendants"
                | "descendants"
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
            "taxonomy_key" if !matches!(scope, QueryScope::Single) => {
                if !valid_content_type_key(&value) {
                    return Err("taxonomy_key không hợp lệ.");
                }
                query.filters.taxonomy_key = Some(value.into_owned());
            }
            "term_id" if !matches!(scope, QueryScope::Single) => {
                query.filters.term_id = Some(numeric()?);
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
    if query.filters.taxonomy_key.is_some() != query.filters.term_id.is_some() {
        return Err("taxonomy_key và term_id phải dùng cùng nhau.");
    }
    if query.filters.taxonomy_key.is_some()
        && (query.filters.category_id.is_some() || query.filters.tag_id.is_some())
    {
        return Err("Không được trộn generic taxonomy filter với category/tag aliases.");
    }
    match scope {
        QueryScope::ContentArchive
            if query.filters.descendants
                && query.filters.category_id.is_none()
                && query.filters.taxonomy_key.is_none() =>
        {
            Err("include_descendants yêu cầu category_id hoặc taxonomy_key + term_id.")
        }
        QueryScope::TaxonomyArchive(TaxonomyKind::Tag)
            if query.filters.descendants
                && query.filters.category_id.is_none()
                && query.filters.taxonomy_key.is_none() =>
        {
            Err("descendants yêu cầu category_id hoặc hierarchical generic taxonomy.")
        }
        _ => Ok(query),
    }
}

pub fn validate_single_query(raw: Option<&str>) -> Result<(), &'static str> {
    parse_query(raw, QueryScope::Single).map(|_| ())
}

/// Validate the original HTTP request path *before* matching any history
/// claim. A decoded/normalized alternate spelling must never alias a route.
/// Canonical path segments in C3 are unescaped and slash-separated.
pub fn validate_public_request_path(path: &str) -> Result<(), &'static str> {
    if !path.starts_with('/')
        || path.contains('%')
        || path.contains('#')
        || path.contains('\\')
        || path.chars().any(char::is_control)
        || (path != "/"
            && path
                .split('/')
                .skip(1)
                .any(|s| s.is_empty() || s == "." || s == ".."))
    {
        return Err("Đường dẫn công khai không hợp lệ.");
    }
    Ok(())
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
    GenericTaxonomyArchive {
        term_id: i64,
        kind_key: String,
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
            && !is_reserved_system_root(type_key)
            && type_key != "posts"
    }

    pub fn archive(type_key: &str) -> Option<String> {
        if type_key == "post" {
            Some("/posts".to_owned())
        } else {
            Self::valid_type(type_key).then(|| format!("/{type_key}"))
        }
    }

    pub fn entry(type_key: &str, slug: &str) -> Option<String> {
        let allow_entry = type_key == "page" || Self::valid_type(type_key);
        (allow_entry && valid_kebab_segment_with_limit(slug, 64)).then(|| {
            if matches!(type_key, "post" | "page") {
                format!("/{slug}")
            } else {
                format!("/{type_key}/{slug}")
            }
        })
    }

    pub fn entry_from_id(id: i64, type_key: &str, slug: &str) -> Option<String> {
        (id > 0).then(|| Self::entry(type_key, slug)).flatten()
    }

    pub fn taxonomy(kind: TaxonomyKind, slug: &str) -> Option<String> {
        valid_kebab_segment_with_limit(slug, 64).then(|| format!("/{}/{slug}", kind.as_str()))
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

    /// Serialize only typed, validated query fields against a canonical base
    /// returned by storage. Public consumers must not reconstruct that base.
    pub fn with_query(base: String, query: PublicQuery, scope: QueryScope) -> String {
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
        if let (Some(key), Some(id)) = (&query.filters.taxonomy_key, query.filters.term_id) {
            pairs.append_pair("taxonomy_key", key);
            pairs.append_pair("term_id", &id.to_string());
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

    /// A user-bound URL may point outside the CMS. Preserve external URLs intact
    /// and check relative CMS paths for disallowed characters or ambiguous double slashes.
    pub fn normalize_internal_link(input: &str) -> Option<String> {
        if !input.starts_with('/') {
            return Some(input.to_string());
        }
        let (path, _raw) = input
            .split_once('?')
            .map_or((input, None), |(a, b)| (a, Some(b)));
        if path.contains("//")
            || path.contains('%')
            || path.contains('#')
            || path.contains('\\')
            || input.chars().any(char::is_control)
        {
            return None;
        }
        Some(input.to_string())
    }
}

#[cfg(test)]
mod request_path_tests {
    use super::*;

    #[test]
    fn reject_encoded_and_non_canonical_request_paths() {
        for path in [
            "/blog/%2fprivate",
            "/blog/%252fprivate",
            "/blog/%2e%2e/private",
            "/blog/../private",
            "/blog/./post",
            "/blog//post",
            "/blog/post/",
            "//blog/post",
            "/blog/post#fragment",
            "/blog\\post",
            "/blog/post\0",
        ] {
            assert!(validate_public_request_path(path).is_err(), "{path}");
        }
        for path in ["/", "/blog/post", "/hello", "/genres/fiction"] {
            assert!(validate_public_request_path(path).is_ok(), "{path}");
        }
    }

    #[test]
    fn wordpress_canonical_url_builder_defaults_and_links() {
        assert_eq!(
            CanonicalUrlBuilder::entry("page", "about").as_deref(),
            Some("/about")
        );
        assert_eq!(
            CanonicalUrlBuilder::entry("post", "hello").as_deref(),
            Some("/hello")
        );
        assert_eq!(
            CanonicalUrlBuilder::entry("project", "alpha").as_deref(),
            Some("/project/alpha")
        );
        assert_eq!(CanonicalUrlBuilder::archive("page"), None);
        assert_eq!(
            CanonicalUrlBuilder::archive("post").as_deref(),
            Some("/posts")
        );
        assert_eq!(
            CanonicalUrlBuilder::archive("project").as_deref(),
            Some("/project")
        );
        assert_eq!(
            CanonicalUrlBuilder::taxonomy(TaxonomyKind::Category, "news").as_deref(),
            Some("/category/news")
        );
        assert_eq!(
            CanonicalUrlBuilder::taxonomy(TaxonomyKind::Tag, "rust").as_deref(),
            Some("/tag/rust")
        );
        assert_eq!(
            CanonicalUrlBuilder::normalize_internal_link("https://example.com/#top%20").as_deref(),
            Some("https://example.com/#top%20")
        );
        assert_eq!(
            CanonicalUrlBuilder::normalize_internal_link("/about?utm=1").as_deref(),
            Some("/about?utm=1")
        );
        assert_eq!(CanonicalUrlBuilder::normalize_internal_link("/a\\b"), None);
    }

    #[test]
    fn canonical_pagination_uses_storage_base_not_legacy_prefix() {
        let query = parse_query(
            Some("page=2&category_id=5&utm_campaign=x"),
            QueryScope::ContentArchive,
        )
        .expect("valid bounded query");
        let href =
            CanonicalUrlBuilder::with_query("/blog".into(), query, QueryScope::ContentArchive);
        assert_eq!(href, "/blog?page=2&category_id=5");
    }
}
