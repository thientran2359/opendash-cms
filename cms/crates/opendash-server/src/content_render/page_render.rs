use super::*;

#[derive(Debug, Clone, Serialize)]
pub struct FieldView {
    pub key: String,
    pub label: String,
    pub field_type: &'static str,
    pub text: String,
    pub media: Option<Value>,
    pub gallery: Vec<Value>,
    pub link: Option<Value>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PublicTermView {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub slug: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PaginationPageLink {
    pub number: Option<i64>,
    pub url: Option<String>,
    pub is_current: bool,
    pub is_ellipsis: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PaginationView {
    pub page: i64,
    pub per_page: i64,
    pub total_items: i64,
    pub total_pages: i64,
    pub has_prev: bool,
    pub has_next: bool,
    pub previous_url: Option<String>,
    pub next_url: Option<String>,
    pub pages: Vec<PaginationPageLink>,
}

impl PaginationView {
    pub fn empty() -> Self {
        Self {
            page: 1,
            per_page: crate::public_context::ARCHIVE_SIZE,
            total_items: 0,
            total_pages: 0,
            has_prev: false,
            has_next: false,
            previous_url: None,
            next_url: None,
            pages: Vec::new(),
        }
    }

    pub fn compute<F>(
        page: i64,
        per_page: i64,
        total_items: i64,
        previous_url: Option<String>,
        next_url: Option<String>,
        page_url: F,
    ) -> Self
    where
        F: FnMut(i64) -> Option<String>,
    {
        Self::compute_with_max_pages(
            page,
            per_page,
            total_items,
            crate::public_context::MAX_ARCHIVE_PAGE,
            previous_url,
            next_url,
            page_url,
        )
    }

    pub fn compute_with_max_pages<F>(
        page: i64,
        per_page: i64,
        total_items: i64,
        max_pages: i64,
        previous_url: Option<String>,
        next_url: Option<String>,
        mut page_url: F,
    ) -> Self
    where
        F: FnMut(i64) -> Option<String>,
    {
        let per_page = per_page.max(1);
        let total_items = total_items.max(0);
        let computed_pages = if total_items == 0 {
            0
        } else {
            ((total_items - 1) / per_page) + 1
        };
        let total_pages = computed_pages.min(max_pages.max(1));

        let push_page = |num: i64, out: &mut Vec<PaginationPageLink>, page_url: &mut F| {
            out.push(PaginationPageLink {
                number: Some(num),
                url: page_url(num),
                is_current: num == page,
                is_ellipsis: false,
            });
        };
        let push_ellipsis = |out: &mut Vec<PaginationPageLink>| {
            out.push(PaginationPageLink {
                number: None,
                url: None,
                is_current: false,
                is_ellipsis: true,
            });
        };

        let mut pages = Vec::new();
        if total_pages > 0 && total_pages <= 9 {
            for n in 1..=total_pages {
                push_page(n, &mut pages, &mut page_url);
            }
        } else if total_pages > 9 {
            let p = page.clamp(1, total_pages);
            if p <= 5 {
                for n in 1..=7 {
                    push_page(n, &mut pages, &mut page_url);
                }
                push_ellipsis(&mut pages);
                push_page(total_pages, &mut pages, &mut page_url);
            } else if p >= total_pages - 4 {
                push_page(1, &mut pages, &mut page_url);
                push_ellipsis(&mut pages);
                for n in (total_pages - 6)..=total_pages {
                    push_page(n, &mut pages, &mut page_url);
                }
            } else {
                push_page(1, &mut pages, &mut page_url);
                push_ellipsis(&mut pages);
                for n in (p - 2)..=(p + 2) {
                    push_page(n, &mut pages, &mut page_url);
                }
                push_ellipsis(&mut pages);
                push_page(total_pages, &mut pages, &mut page_url);
            }
        }

        Self {
            page,
            per_page,
            total_items,
            total_pages,
            has_prev: previous_url.is_some(),
            has_next: next_url.is_some(),
            previous_url,
            next_url,
            pages,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EntryView {
    pub id: i64,
    pub type_key: String,
    pub title: String,
    pub slug: String,
    pub url: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub published_at: Option<String>,
    pub excerpt: String,
    pub reading_time_minutes: u32,
    pub fields: BTreeMap<String, Value>,
    pub field_list: Vec<FieldView>,
    pub cover_url: Option<String>,
    pub terms: BTreeMap<String, Vec<PublicTermView>>,
    pub all_terms: Vec<PublicTermView>,
    pub primary_category: Option<PublicTermView>,
    pub ext: BTreeMap<String, Value>,
    #[serde(default)]
    pub theme_ext: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snippet_segments: Vec<opendash_core::search::SnippetSegment>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub type_label: String,
}

/// Only typed site strings enter the custom-template context; MiniJinja
/// performs HTML escaping just like every other content value.
#[derive(Debug, Clone, Serialize)]
pub struct SiteContext<'a> {
    pub title: &'a str,
    pub tagline: &'a str,
    pub timezone: &'a str,
    pub origin: &'a str,
    pub current_year: i32,
}

pub fn current_year_in_timezone(tz_name: &str) -> i32 {
    let tz = jiff::tz::TimeZone::get(tz_name).unwrap_or(jiff::tz::TimeZone::UTC);
    i32::from(jiff::Timestamp::now().to_zoned_datetime(tz).year())
}

pub(super) fn media_url(value: &Value) -> Option<String> {
    let id = value.as_str()?;
    valid_media_id(id).then(|| format!("/media/{id}/file"))
}

fn rewrite_media_urls_in_value(val: &mut Value) {
    match val {
        Value::Object(obj) => {
            for k in ["url", "thumbnail_url", "cover_url"] {
                if let Some(Value::String(s)) = obj.get_mut(k)
                    && s.starts_with("/media/")
                {
                    *s = s.replacen("/media/", "/api/admin/media/", 1);
                }
            }
            for child in obj.values_mut() {
                if child.is_object() || child.is_array() {
                    rewrite_media_urls_in_value(child);
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                rewrite_media_urls_in_value(item);
            }
        }
        _ => {}
    }
}

pub(crate) fn rewrite_preview_media_urls(mut view: EntryView, type_key: &str) -> EntryView {
    let admin_media = |url: &str| url.replacen("/media/", "/api/admin/media/", 1);
    view.cover_url = view.cover_url.as_deref().map(admin_media);
    for val in view.fields.values_mut() {
        rewrite_media_urls_in_value(val);
    }
    for field in &mut view.field_list {
        if let Some(ref mut media) = field.media {
            rewrite_media_urls_in_value(media);
        }
        for item in &mut field.gallery {
            rewrite_media_urls_in_value(item);
        }
    }
    if view.url.is_none() {
        view.url = Some(format!(
            "/api/admin/content-types/{type_key}/entries/{}/preview",
            view.id
        ));
    }
    view
}

pub struct PublicThemeContextInput<'a> {
    pub site: &'a SiteContext<'a>,
    pub site_options: &'a BTreeMap<String, Value>,
    pub options: &'a BTreeMap<String, Value>,
    pub theme_assets: &'a BTreeMap<String, String>,
    pub menus: &'a BTreeMap<String, crate::storage::menu::MenuViewModel>,
    pub route_kind: &'a str,
    pub route_path: Option<&'a str>,
    pub breadcrumbs: &'a [BreadcrumbItem],
    pub pagination: &'a PaginationView,
    pub archive_url: Option<&'a str>,
    pub previous_url: Option<&'a str>,
    pub next_url: Option<&'a str>,
    pub entries: &'a [EntryView],
    pub related_entries: &'a [EntryView],
    pub recent_posts: &'a [EntryView],
    pub taxonomy: Option<&'a Value>,
    pub content_type: Option<&'a Value>,
    pub entry: Option<&'a EntryView>,
    pub search: &'a SearchView,
}

#[derive(Serialize)]
pub(super) struct SerializedRouteContext<'a> {
    pub(super) kind: &'a str,
    pub(super) path: Option<&'a str>,
}

#[derive(Serialize)]
pub(super) struct SerializedThemeContext<'a> {
    pub(super) site: &'a SiteContext<'a>,
    pub(super) site_options: &'a BTreeMap<String, Value>,
    pub(super) options: &'a BTreeMap<String, Value>,
    pub(super) theme_options: &'a BTreeMap<String, Value>,
    pub(super) theme_assets: &'a BTreeMap<String, String>,
    pub(super) menus: &'a BTreeMap<String, crate::storage::menu::MenuViewModel>,
    pub(super) route: SerializedRouteContext<'a>,
    pub(super) breadcrumbs: &'a [BreadcrumbItem],
    pub(super) pagination: &'a PaginationView,
    pub(super) archive_url: Option<&'a str>,
    pub(super) previous_url: Option<&'a str>,
    pub(super) next_url: Option<&'a str>,
    pub(super) entries: &'a [EntryView],
    pub(super) related_entries: &'a [EntryView],
    pub(super) recent_posts: &'a [EntryView],
    pub(super) taxonomy: Option<&'a Value>,
    pub(super) content_type: Option<&'a Value>,
    pub(super) entry: Option<&'a EntryView>,
    pub(super) search: &'a SearchView,
}

pub const MAX_RENDER_HOOK_DOCUMENT_BYTES: usize = 2 * 1024 * 1024;

pub const HEAD_END_MARKER: &str = "<!-- opendash-head-end -->";

/// Stylesheet links are authorized by the normalizer. Extract their local URL
/// only for deduplication, including entity-encoded path characters.
pub(super) fn stylesheet_url_from_tag(tag: &str) -> Option<String> {
    let tag = tag.trim_start();
    if !tag.get(..5)?.eq_ignore_ascii_case("<link") {
        return None;
    }
    let bytes = tag.as_bytes();
    let mut cursor = 5;
    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || matches!(bytes[cursor], b'>' | b'/') {
            break;
        }
        let start = cursor;
        while cursor < bytes.len()
            && !bytes[cursor].is_ascii_whitespace()
            && !matches!(bytes[cursor], b'=' | b'>' | b'/')
        {
            cursor += 1;
        }
        if start == cursor {
            cursor += 1;
            continue;
        }
        let name = &tag[start..cursor];
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] != b'=' {
            continue;
        }
        cursor += 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }
        let quote = bytes[cursor];
        let (from, to) = if quote == b'\'' || quote == b'"' {
            cursor += 1;
            let from = cursor;
            while cursor < bytes.len() && bytes[cursor] != quote {
                cursor += 1;
            }
            let to = cursor;
            cursor = (cursor + 1).min(bytes.len());
            (from, to)
        } else {
            let from = cursor;
            while cursor < bytes.len()
                && !bytes[cursor].is_ascii_whitespace()
                && bytes[cursor] != b'>'
            {
                cursor += 1;
            }
            (from, cursor)
        };
        if name.eq_ignore_ascii_case("href") {
            return Some(opendash_core::richtext::decode_html5_entities(
                &tag[from..to],
            ));
        }
    }
    None
}

pub fn serialize_json_ld_safe(val: &Value) -> String {
    let raw = serde_json::to_string(val).unwrap_or_else(|_| "{}".to_owned());
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            _ => out.push(ch),
        }
    }
    out
}

pub fn render_archive_with_site(
    schema: &ContentTypeDefinition,
    entries: &[EntryView],
    previous_url: Option<&str>,
    next_url: Option<&str>,
    selected_template: Option<&str>,
    site: &SiteContext<'_>,
) -> ApiResult<String> {
    render_archive_with_site_for_locale(
        schema,
        entries,
        previous_url,
        next_url,
        selected_template,
        site,
        "vi",
    )
}

pub fn render_archive_with_site_for_locale(
    schema: &ContentTypeDefinition,
    entries: &[EntryView],
    previous_url: Option<&str>,
    next_url: Option<&str>,
    selected_template: Option<&str>,
    site: &SiteContext<'_>,
    locale_code: &str,
) -> ApiResult<String> {
    render_archive_with_site_and_config_for_locale(
        schema,
        entries,
        previous_url,
        next_url,
        &PaginationView::empty(),
        &[],
        None,
        selected_template,
        site,
        &RenderFormattingConfig::default(),
        locale_code,
    )
}

pub fn render_single_with_site(
    schema: &ContentTypeDefinition,
    entry: &EntryView,
    selected_template: Option<&str>,
    site: &SiteContext<'_>,
) -> ApiResult<String> {
    // Admin preview/legacy callers have no public route context. Public routes
    // must supply the storage-resolved, visibility-checked archive explicitly.
    let fallback = CanonicalUrlBuilder::archive(&schema.key);
    render_single_with_site_and_archive(schema, entry, selected_template, site, fallback.as_deref())
}

/// Public rendering must never synthesize a legacy archive link when the
/// archive is private, disabled or has a configured permalink rewrite.
pub fn render_single_with_site_and_archive(
    schema: &ContentTypeDefinition,
    entry: &EntryView,
    selected_template: Option<&str>,
    site: &SiteContext<'_>,
    archive_url: Option<&str>,
) -> ApiResult<String> {
    render_single_with_site_and_archive_for_locale(
        schema,
        entry,
        selected_template,
        site,
        archive_url,
        "vi",
    )
}

pub fn render_single_with_site_and_archive_for_locale(
    schema: &ContentTypeDefinition,
    entry: &EntryView,
    selected_template: Option<&str>,
    site: &SiteContext<'_>,
    archive_url: Option<&str>,
    locale_code: &str,
) -> ApiResult<String> {
    render_single_with_context_for_locale(
        schema,
        entry,
        selected_template,
        site,
        archive_url,
        &[],
        &RenderFormattingConfig::default(),
        locale_code,
    )
}
