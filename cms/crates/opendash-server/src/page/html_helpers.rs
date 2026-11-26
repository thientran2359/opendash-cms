use super::*;

/// Canonical link: only currently resolved node IDs with valid nondefault pages
/// survive. Unknown query parameters, duplicates and invalid values are dropped.
pub(super) fn list_page_href(
    base_path: &str,
    lists: &ListResults,
    node_id: &str,
    target: i64,
) -> String {
    let mut pages = std::collections::BTreeMap::new();
    for (id, current) in lists {
        let ResolvedList::Page {
            page, out_of_range, ..
        } = current
        else {
            continue;
        };
        let effective = if id == node_id {
            target
        } else if *out_of_range {
            1
        } else {
            *page
        };
        if (2..=100).contains(&effective) {
            pages.insert(id.clone(), effective);
        }
    }
    CanonicalUrlBuilder::list_pages(base_path, &pages)
}
