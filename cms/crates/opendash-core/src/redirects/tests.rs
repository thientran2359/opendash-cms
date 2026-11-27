use super::*;
use crate::id::is_reserved_system_root;

#[test]
fn reserved_system_roots_contains_expected_26_prefixes() {
    assert!(is_reserved_system_root("admin"));
    assert!(is_reserved_system_root("api"));
    assert!(is_reserved_system_root("assets"));
    assert!(is_reserved_system_root("favicon.ico"));
    assert!(is_reserved_system_root("uploads"));
    assert!(!is_reserved_system_root("Admin"));
    assert!(!is_reserved_system_root("san-pham"));
}

#[test]
fn decodes_safe_legacy_utf8_and_preserves_percent20() {
    assert_eq!(
        decode_safe_legacy_request_path("/tin-t%E1%BB%A9c").unwrap(),
        "/tin-tức"
    );
    assert_eq!(
        decode_safe_legacy_request_path("/old%20page.html").unwrap(),
        "/old%20page.html"
    );
    assert_eq!(
        normalize_redirect_source_path(" /Lien_He.html/ ").unwrap(),
        "/Lien_He.html"
    );
    assert_eq!(
        normalize_redirect_source_path("/tin-t%E1%BB%A9c/").unwrap(),
        "/tin-tức"
    );
}

#[test]
fn rejects_ascii_aliasing_and_reserved_roots() {
    for bad in [
        "/blog/%2Fprivate",
        "/blog/%252Fprivate",
        "/blog/%2e%2e/private",
        "/%61dmin/config",
        "/a%0Ab",
        "/a%3Fb",
        "/a%23b",
        "/a%FFb",
        "/a%2",
        "/a/../../b",
        "/a//b",
    ] {
        assert!(decode_safe_legacy_request_path(bad).is_err(), "{bad}");
    }

    assert_eq!(
        normalize_redirect_source_path("/Admin/panel"),
        Err(RedirectValidationError::SourcePathReservedRoot)
    );
    assert_eq!(
        normalize_redirect_source_path("/"),
        Err(RedirectValidationError::SourcePathInvalidFormat)
    );
    let too_long = format!("/{}", "a".repeat(512));
    assert_eq!(
        normalize_redirect_source_path(&too_long),
        Err(RedirectValidationError::SourcePathTooLong)
    );
}

#[test]
fn validates_targets_self_redirect_and_locations() {
    assert_eq!(
        normalize_redirect_target_url("//evil.com"),
        Err(RedirectValidationError::TargetUrlUnsafeProtocolRelative)
    );
    assert_eq!(
        normalize_redirect_target_url("javascript:alert(1)"),
        Err(RedirectValidationError::TargetUrlInvalid)
    );
    assert_eq!(
        normalize_redirect_target_url("https://user:pass@example.com/x"),
        Err(RedirectValidationError::TargetUrlInvalid)
    );
    assert_eq!(
        normalize_redirect_target_url("/%2Fbad"),
        Err(RedirectValidationError::TargetUrlInvalid)
    );
    assert_eq!(
        normalize_redirect_target_url("/dest/?#"),
        Ok("/dest".to_owned())
    );
    assert_eq!(
        normalize_redirect_target_url("/tin-t%E1%BB%A9c#sec"),
        Ok("/tin-tức#sec".to_owned())
    );

    let idn = normalize_redirect_target_url("https://ví-dụ.vn/bài?a=1#h").unwrap();
    assert!(idn.starts_with("https://xn--"));

    assert_eq!(
        check_not_self_redirect(
            "/tin-tức",
            "https://Example.COM/tin-t%E1%BB%A9c/",
            Some("https://example.com")
        ),
        Err(RedirectValidationError::SelfRedirect)
    );

    assert_eq!(
        build_redirect_location("/tin-tức#top", Some("utm=1\r\nx")),
        "/tin-t%E1%BB%A9c?utm=1x#top"
    );
    assert_eq!(
        build_redirect_location("/old%20a?x=1#s", Some("utm=2")),
        "/old%20a?x=1#s"
    );
}

#[test]
fn parses_and_formats_csv_safely() {
    let csv = "\u{FEFF}Target,From,Note,hit_count,last_hit_at\r\n/new,/old,\"a, \"\"b\"\"\",99,2026-01-01\r\n,/bad\r\n/next,/from-302,ok,0,\r\n";
    let parsed = parse_redirects_csv(csv).unwrap();
    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed[0].as_ref().unwrap().input.source_path, "/old");
    assert_eq!(parsed[0].as_ref().unwrap().input.target_url, "/new");
    assert_eq!(
        parsed[0].as_ref().unwrap().input.status_code,
        RedirectStatusCode::MovedPermanently
    );
    assert!(parsed[0].as_ref().unwrap().input.enabled);
    assert_eq!(parsed[0].as_ref().unwrap().input.note, "a, \"b\"");
    assert_eq!(
        parsed[1],
        Err(CsvRowError::Validation(
            RedirectValidationError::EmptyTargetUrl
        ))
    );

    let headerless = "/old,/new,302,no,\n/a,/b\n";
    let parsed_hl = parse_redirects_csv(headerless).unwrap();
    assert_eq!(
        parsed_hl[0].as_ref().unwrap().input.status_code,
        RedirectStatusCode::Found
    );
    assert!(!parsed_hl[0].as_ref().unwrap().input.enabled);
    assert_eq!(
        parsed_hl[1].as_ref().unwrap().input.status_code,
        RedirectStatusCode::MovedPermanently
    );
    assert!(parsed_hl[1].as_ref().unwrap().input.enabled);

    assert_eq!(
        parse_redirects_csv("source_path,note\n/a,b"),
        Err(RedirectValidationError::CsvMissingRequiredHeaders)
    );
    assert_eq!(
        parse_redirects_csv("source,from,target\n/a,/b,/c"),
        Err(RedirectValidationError::CsvDuplicateMappedHeaders)
    );
    assert_eq!(
        parse_redirects_csv("source,target\n"),
        Err(RedirectValidationError::CsvEmpty)
    );

    let exported = format_redirects_csv(&[ManualRedirectCsvRow {
        source_path: "/old".to_owned(),
        target_url: "/new".to_owned(),
        status_code: 301,
        enabled: true,
        note: " =1+1".to_owned(),
        hit_count: 7,
        last_hit_at: Some("\tcmd".to_owned()),
    }]);
    assert!(exported.starts_with('\u{FEFF}'));
    assert!(exported.contains("' =1+1"));
    assert!(exported.contains("'\tcmd"));
}
