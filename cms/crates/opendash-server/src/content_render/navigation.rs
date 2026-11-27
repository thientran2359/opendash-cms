use serde::Serialize;

use super::PublicTermView;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BreadcrumbItem {
    pub name: String,
    pub url: Option<String>,
    pub is_current: bool,
}

pub fn home_breadcrumbs(home_label: &str, home_url: Option<&str>) -> Vec<BreadcrumbItem> {
    vec![BreadcrumbItem {
        name: home_label.to_owned(),
        url: home_url.map(str::to_owned),
        is_current: true,
    }]
}

pub fn archive_breadcrumbs(
    home_label: &str,
    home_url: Option<&str>,
    content_type_label: &str,
    archive_url: Option<String>,
) -> Vec<BreadcrumbItem> {
    vec![
        BreadcrumbItem {
            name: home_label.to_owned(),
            url: home_url.map(str::to_owned),
            is_current: false,
        },
        BreadcrumbItem {
            name: content_type_label.to_owned(),
            url: archive_url,
            is_current: true,
        },
    ]
}

pub fn taxonomy_breadcrumbs(
    home_label: &str,
    home_url: Option<&str>,
    mut ancestors: Vec<BreadcrumbItem>,
    term_name: &str,
    term_url: Option<String>,
) -> Vec<BreadcrumbItem> {
    let mut out = Vec::with_capacity(2 + ancestors.len());
    out.push(BreadcrumbItem {
        name: home_label.to_owned(),
        url: home_url.map(str::to_owned),
        is_current: false,
    });
    for item in &mut ancestors {
        item.is_current = false;
    }
    out.extend(ancestors);
    out.push(BreadcrumbItem {
        name: term_name.to_owned(),
        url: term_url,
        is_current: true,
    });
    out
}

#[allow(clippy::too_many_arguments)]
pub fn single_breadcrumbs(
    home_label: &str,
    home_url: Option<&str>,
    type_key: &str,
    content_type_label: &str,
    has_archive: bool,
    archive_url: Option<String>,
    primary_category: Option<&PublicTermView>,
    entry_title: &str,
    entry_url: Option<String>,
) -> Vec<BreadcrumbItem> {
    let mut out = vec![BreadcrumbItem {
        name: home_label.to_owned(),
        url: home_url.map(str::to_owned),
        is_current: false,
    }];
    if type_key != "page" && has_archive {
        out.push(BreadcrumbItem {
            name: content_type_label.to_owned(),
            url: archive_url,
            is_current: false,
        });
    }
    if let Some(cat) = primary_category {
        out.push(BreadcrumbItem {
            name: cat.name.clone(),
            url: cat.url.clone(),
            is_current: false,
        });
    }
    out.push(BreadcrumbItem {
        name: entry_title.to_owned(),
        url: entry_url,
        is_current: true,
    });
    out
}

pub fn not_found_breadcrumbs(
    home_label: &str,
    home_url: Option<&str>,
    not_found_label: &str,
) -> Vec<BreadcrumbItem> {
    vec![
        BreadcrumbItem {
            name: home_label.to_owned(),
            url: home_url.map(str::to_owned),
            is_current: false,
        },
        BreadcrumbItem {
            name: not_found_label.to_owned(),
            url: None,
            is_current: true,
        },
    ]
}

pub fn search_breadcrumbs(
    home_label: &str,
    home_url: Option<&str>,
    search_label: &str,
) -> Vec<BreadcrumbItem> {
    not_found_breadcrumbs(home_label, home_url, search_label)
}
