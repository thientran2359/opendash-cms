use super::*;

#[test]
fn if_match_requires_canonical_positive_version() {
    let mut headers = HeaderMap::new();
    assert_eq!(
        expected_version(&headers).unwrap_err().0,
        StatusCode::PRECONDITION_REQUIRED
    );
    for invalid in ["*", "\"v0\"", "\"v01\"", "v1", "\"v-1\"", "W/\"v1\""] {
        headers.insert(header::IF_MATCH, invalid.parse().unwrap());
        assert_eq!(
            expected_version(&headers).unwrap_err().0,
            StatusCode::BAD_REQUEST
        );
    }
    headers.insert(header::IF_MATCH, "\"v42\"".parse().unwrap());
    assert_eq!(expected_version(&headers).unwrap(), 42);
}
