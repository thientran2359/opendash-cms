use super::*;

use super::*;

#[test]
fn if_match_requires_exact_version() {
    let mut headers = HeaderMap::new();
    assert_eq!(
        expected_version(&headers).unwrap_err().0,
        StatusCode::PRECONDITION_REQUIRED
    );
    for invalid in ["*", "\"v0\"", "\"v01\"", "v1", "W/\"v1\"", "\"v-1\""] {
        headers.insert(header::IF_MATCH, invalid.parse().unwrap());
        assert_eq!(
            expected_version(&headers).unwrap_err().0,
            StatusCode::BAD_REQUEST
        );
    }
    headers.insert(header::IF_MATCH, "\"v17\"".parse().unwrap());
    assert_eq!(expected_version(&headers).unwrap(), 17);
}

#[test]
fn null_parent_is_distinct_from_omitted_parent() {
    let omitted: PatchTaxonomyTerm = serde_json::from_str(r#"{"name":"Changed"}"#).unwrap();
    assert_eq!(omitted.parent_id, None);
    let root: PatchTaxonomyTerm = serde_json::from_str(r#"{"parent_id":null}"#).unwrap();
    assert_eq!(root.parent_id, Some(None));
    let child: PatchTaxonomyTerm = serde_json::from_str(r#"{"parent_id":42}"#).unwrap();
    assert_eq!(child.parent_id, Some(Some(42)));
    assert!(serde_json::from_str::<PatchTaxonomyTerm>(r#"{"kind":"tag"}"#).is_err());
    assert!(serde_json::from_str::<PatchTaxonomyTerm>(r#"{"locale":"en"}"#).is_err());
}

#[test]
fn featured_image_patch_distinguishes_omitted_clear_and_set() {
    let omitted: PatchTaxonomyTerm = serde_json::from_str(r#"{"name":"Changed"}"#).unwrap();
    assert_eq!(omitted.featured_image_media_id, None);
    let cleared_null: PatchTaxonomyTerm =
        serde_json::from_str(r#"{"featured_image_media_id":null}"#).unwrap();
    assert_eq!(cleared_null.featured_image_media_id, Some(None));
    let cleared_empty: PatchTaxonomyTerm =
        serde_json::from_str(r#"{"featured_image_media_id":""}"#).unwrap();
    assert_eq!(cleared_empty.featured_image_media_id, Some(None));
    let media_id = "a".repeat(64);
    let set: PatchTaxonomyTerm =
        serde_json::from_str(&format!(r#"{{"featured_image_media_id":"{media_id}"}}"#)).unwrap();
    assert_eq!(set.featured_image_media_id, Some(Some(media_id)));
}

#[test]
fn bulk_term_resolve_has_hard_id_cap_and_strict_ids() {
    let hundred = (1..=100)
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(parse_term_resolve_ids(&hundred).unwrap().len(), 100);
    let hundred_one = format!("{hundred},101");
    assert_eq!(
        parse_term_resolve_ids(&hundred_one).unwrap_err().0,
        StatusCode::BAD_REQUEST
    );
    for invalid in [
        "",
        "0",
        "01",
        "1,",
        ",1",
        "1,,2",
        "1,1",
        "1,-2",
        "1,+2",
        "1, 2",
        "1,9223372036854775808",
        "1;DROP TABLE taxonomy_terms",
    ] {
        assert_eq!(
            parse_term_resolve_ids(invalid).unwrap_err().0,
            StatusCode::BAD_REQUEST,
            "must reject {invalid:?}"
        );
    }
}
