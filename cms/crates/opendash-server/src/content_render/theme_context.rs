use super::*;

pub struct PublicThemeContextInput<'a> {
    pub site: &'a SiteContext<'a>,
    pub site_options: &'a BTreeMap<String, Value>,
    pub options: &'a BTreeMap<String, Value>,
    pub custom_options: &'a BTreeMap<String, Value>,
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
    pub term: Option<&'a PublicTermDetailView>,
    pub content_type: Option<&'a Value>,
    pub entry: Option<&'a EntryView>,
    pub search: &'a SearchView,
}

#[derive(Serialize)]
struct SerializedRouteContext<'a> {
    kind: &'a str,
    path: Option<&'a str>,
}

#[derive(Serialize)]
struct SerializedThemeContext<'a> {
    site: &'a SiteContext<'a>,
    site_options: &'a BTreeMap<String, Value>,
    options: &'a BTreeMap<String, Value>,
    theme_options: &'a BTreeMap<String, Value>,
    custom_options: &'a BTreeMap<String, Value>,
    theme_assets: &'a BTreeMap<String, String>,
    menus: &'a BTreeMap<String, crate::storage::menu::MenuViewModel>,
    route: SerializedRouteContext<'a>,
    breadcrumbs: &'a [BreadcrumbItem],
    pagination: &'a PaginationView,
    archive_url: Option<&'a str>,
    previous_url: Option<&'a str>,
    next_url: Option<&'a str>,
    entries: &'a [EntryView],
    related_entries: &'a [EntryView],
    recent_posts: &'a [EntryView],
    taxonomy: Option<&'a Value>,
    term: Option<&'a PublicTermDetailView>,
    content_type: Option<&'a Value>,
    entry: Option<&'a EntryView>,
    search: &'a SearchView,
}

pub fn build_public_theme_context(input: PublicThemeContextInput<'_>) -> ApiResult<Value> {
    let payload = SerializedThemeContext {
        site: input.site,
        site_options: input.site_options,
        options: input.options,
        theme_options: input.options,
        custom_options: input.custom_options,
        theme_assets: input.theme_assets,
        menus: input.menus,
        route: SerializedRouteContext {
            kind: input.route_kind,
            path: input.route_path,
        },
        breadcrumbs: input.breadcrumbs,
        pagination: input.pagination,
        archive_url: input.archive_url,
        previous_url: input.previous_url,
        next_url: input.next_url,
        entries: input.entries,
        related_entries: input.related_entries,
        recent_posts: input.recent_posts,
        taxonomy: input.taxonomy,
        term: input.term,
        content_type: input.content_type,
        entry: input.entry,
        search: input.search,
    };
    serde_json::to_value(payload).map_err(|err| ApiError::internal(err.to_string()))
}
