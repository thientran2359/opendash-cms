use std::collections::{BTreeMap, HashSet};

use axum::{
    extract::{RawQuery, State},
    http::header,
    response::{Html, IntoResponse, Response},
};
use sqlx::SqlitePool;

use opendash_core::id::valid_package_id;

use crate::{
    error::ApiResult,
    page::{render_builder_with_lists, render_legacy_state},
    storage::builder,
};

/// Malformed, repeated or unknown list-page parameters cannot become SQL or
/// survive in canonical pagination links. Only a single valid integer 1..100
/// for an eligible node ID counts; otherwise that list starts at page 1.
fn parsed_list_pages(raw: Option<&str>) -> BTreeMap<String, i64> {
    let mut pages = BTreeMap::new();
    let mut seen = HashSet::new();
    let mut repeated = HashSet::new();
    for (key, value) in url::form_urlencoded::parse(raw.unwrap_or("").as_bytes()) {
        let Some(id) = key.strip_prefix("list_page_") else {
            continue;
        };
        if !valid_package_id(id) {
            continue;
        }
        if !seen.insert(id.to_string()) {
            repeated.insert(id.to_string());
            pages.remove(id);
            continue;
        }
        let parsed = value.parse::<i64>().ok().filter(|p| (1..=100).contains(p));
        if let Some(page) = parsed.filter(|p| value.as_ref() == p.to_string()) {
            pages.insert(id.to_string(), page);
        }
    }
    pages.retain(|id, _| !repeated.contains(id));
    pages
}

pub async fn public_page(
    State(db): State<SqlitePool>,
    RawQuery(raw): RawQuery,
) -> ApiResult<Response> {
    // One SQLite read snapshot for source, document, active theme and block
    // templates. Do not split them across old / and new Builder reads.
    let (state, bindings, lists) =
        builder::read_public_resolved(&db, &parsed_list_pages(raw.as_deref())).await?;
    if let Some(document) = &state.saved {
        return Ok(rendered_layout(
            render_builder_with_lists(&state, document, false, &bindings, &lists)?.html,
        ));
    }
    let html = match render_legacy_state(&state)? {
        Some(snapshot) => snapshot.html,
        None => "<p>Chưa lưu block.</p>".into(),
    };
    Ok(rendered(html))
}

#[cfg(test)]
mod list_query_tests {
    use super::*;

    #[test]
    fn duplicate_invalid_and_noncanonical_pages_default_to_one() {
        let parsed = parsed_list_pages(Some(
            "list_page_list-a=2&list_page_list-b=4&list_page_list-a=3&list_page_list-c=02&list_page_list-d=101&list_page_list-e=-1&list_page_invalid%2Fnode=8&x=anything",
        ));
        assert_eq!(parsed.get("list-a"), None);
        assert_eq!(parsed.get("list-b"), Some(&4));
        assert!(!parsed.contains_key("list-c"));
        assert!(!parsed.contains_key("list-d"));
        assert!(!parsed.contains_key("list-e"));
        assert_eq!(parsed.len(), 1);
    }
}

pub fn rendered_layout(html: String) -> Response {
    (
        [
            (header::CONTENT_SECURITY_POLICY, "default-src 'none'; img-src 'self'; style-src 'unsafe-inline'; script-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Html(html),
    )
        .into_response()
}

pub fn rendered(html: String) -> Response {
    (
        [
            (
                header::CONTENT_SECURITY_POLICY,
                "sandbox; default-src 'none'",
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        Html(html),
    )
        .into_response()
}
