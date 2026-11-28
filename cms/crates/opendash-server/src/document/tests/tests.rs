use super::*;

#[test]
fn trusted_head_and_body_scripts_preserve_raw_text_and_split_authorization() {
    let html = r#"<!doctype html><html><head>
        <script type="module">const x = 1 < 2; const s = "<div>"; const t = "</scripture>";</ScRiPt >
        <title>SEO owned by composer</title>
        </head><body><main><script>if (a < b) { console.log("<span>"); }</SCRIPT></main></body></html>"#;
    let parts = normalize_public_document_with_options(html, true).unwrap();
    assert_eq!(parts.authorized_head_scripts.len(), 1);
    assert!(parts.authorized_head_scripts[0].contains("1 < 2"));
    assert!(parts.authorized_head_scripts[0].contains("</scripture>"));
    assert!(parts.authorized_head_scripts[0].ends_with("</ScRiPt >"));
    assert!(parts.body_fragment.contains("if (a < b)"));
    assert!(parts.body_fragment.contains("console.log(\"<span>\")"));
    assert!(!parts.body_fragment.contains("SEO owned by composer"));
    assert!(!parts.body_fragment.contains("const x"));
    assert!(parts.authorized_styles.is_empty());
}

#[test]
fn trusted_fragment_preserves_svg_titles_static_handlers_and_scripts() {
    let html = r#"<svg xmlns="http://www.w3.org/2000/svg" onload="ready()"><title>Icon label</title>
        <svg><title>Nested label</title></svg>
        <foreignObject><div onclick="run()">content</div></foreignObject>
        <path d="M0 0 L1 1" /></svg>
        <script>const isLess = 1 < 2;</script><title>Discard this HTML title</title>"#;
    let parts = normalize_public_document_with_options(html, true).unwrap();
    assert_eq!(parts.authorized_head_scripts, Vec::<String>::new());
    assert!(parts.body_fragment.contains(r#"onload="ready()""#));
    assert!(parts.body_fragment.contains(r#"onclick="run()""#));
    assert!(parts.body_fragment.contains("<title>Icon label</title>"));
    assert!(parts.body_fragment.contains("<title>Nested label</title>"));
    assert!(parts.body_fragment.contains("const isLess = 1 < 2;"));
    assert!(
        !parts
            .body_fragment
            .contains("<title>Discard this HTML title</title>")
    );
}

#[test]
fn trusted_foreignobject_html_body_does_not_create_or_close_document_envelope() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><foreignObject><body class="nested"><p>Nested HTML</p></body></foreignObject></svg>"#;
    let fragment = normalize_public_document_with_options(svg, true).unwrap();
    assert!(fragment.body_fragment.contains(r#"<body class="nested">"#));
    assert!(fragment.body_fragment.contains("<p>Nested HTML</p>"));
    assert!(
        fragment
            .body_fragment
            .contains("</body></foreignobject></svg>")
    );
    assert!(fragment.body_attributes.is_empty());

    let document = format!(
        "<!doctype html><html><head></head><body class=\"outer\"><main>{svg}</main></body></html>"
    );
    let normalized = normalize_public_document_with_options(&document, true).unwrap();
    assert_eq!(
        normalized.body_attributes,
        vec![("class".to_owned(), "outer".to_owned())]
    );
    assert!(
        normalized
            .body_fragment
            .contains(r#"<body class="nested">"#)
    );
    assert!(normalized.body_fragment.contains("<p>Nested HTML</p>"));
    assert!(
        normalized
            .body_fragment
            .contains("</body></foreignobject></svg>")
    );
    assert!(normalize_public_document(svg).is_err());
}

#[test]
fn trusted_envelope_keeps_svg_title_but_filters_body_attrs() {
    let html = r#"<html lang="en" onload="bad()"><head></head><body class="ok" onload="bad()" data-live="1">
        <svg onpointerdown="onStatic()"><title>Accessible SVG</title></svg>
        <title>Document title</title></body></html>"#;
    let parts = normalize_public_document_with_options(html, true).unwrap();
    assert!(
        parts
            .body_fragment
            .contains("<title>Accessible SVG</title>")
    );
    assert!(
        !parts
            .body_fragment
            .contains("<title>Document title</title>")
    );
    assert!(
        parts
            .body_fragment
            .contains(r#"onpointerdown="onStatic()""#)
    );
    assert_eq!(
        parts.body_attributes,
        vec![
            ("class".to_string(), "ok".to_string()),
            ("data-live".to_string(), "1".to_string())
        ]
    );
}

#[test]
fn trusted_scripts_require_matching_end_tag_and_valid_envelope_position() {
    for (html, code) in [
        ("<script>const x = 1 < 2", "unclosed_script"),
        ("<script/>", "self_closing_script"),
        ("<script src=\"/app.js\" />", "self_closing_script"),
        ("</script>", "unexpected_close_script"),
        ("<script>x</script></SCRIPT>", "unexpected_close_script"),
        (
            "<html><script>x</script><body></body></html>",
            "script_outside_envelope",
        ),
        (
            "<html><head></head><script>x</script><body></body></html>",
            "script_outside_envelope",
        ),
        (
            "<html><head></head><body></body><script>x</script></html>",
            "script_outside_envelope",
        ),
        (
            "<html><head></head><body></body></html><script>x</script>",
            "script_outside_envelope",
        ),
    ] {
        let result = normalize_public_document_with_options(html, true);
        assert_eq!(result.unwrap_err().code, code, "input: {html}");
    }
    assert!(normalize_public_document_with_options("<iframe src='/a'></iframe>", true).is_err());
}

#[test]
fn trusted_script_rejects_html5_double_escaped_data_smuggling() {
    for script in [
        "<script>\n<!--\n/*<script>*/\n// </script>\nalert(1)\n<!-- </script> -->",
        "<svg><script><![CDATA[// </script>\nalert(1)\n//<!-- ]]> </script> --> </svg>",
    ] {
        let error = normalize_public_document_with_options(script, true).unwrap_err();
        assert_eq!(error.code, "script_escaped_state");
        let document =
            format!("<!doctype html><html><head></head><body><main>{script}</main></body></html>");
        let error = normalize_public_document_with_options(&document, true).unwrap_err();
        assert_eq!(error.code, "script_escaped_state");
        assert!(normalize_public_document(script).is_err());
    }
}

#[test]
fn strict_wrapper_rejects_scripts_svg_and_event_handlers() {
    for html in [
        "<script>const x = 1 < 2;</script>",
        "<svg><title>Icon</title></svg>",
        "<div onmouseover='run()'>text</div>",
        "<math><mi>x</mi></math>",
    ] {
        assert!(normalize_public_document(html).is_err(), "input: {html}");
        assert!(
            normalize_public_document_with_options(html, false).is_err(),
            "input: {html}"
        );
    }
    let parts = normalize_public_document("<p>safe</p>").unwrap();
    assert!(parts.authorized_head_scripts.is_empty());
    assert_eq!(parts.body_fragment, "<p>safe</p>");
}

#[test]
fn normalizes_mixed_case_body_attrs_comments_style_and_strips_forged_head_tags() {
    let html = r#"<!DOCTYPE html>
<HTML lang="en">
  <HEAD>
<TITLE>Forged Title</TITLE>
<link rel="canonical" href="https://evil.example/steal">
<meta name="robots" content="index,follow">
<STYLE>body::before { content: "</head>"; color: red; }</STYLE>
<link rel="stylesheet" href="/assets/theme.css">
  </HEAD>
  <!-- comment with </body> inside -->
  <BODY class="post-page" data-theme="v2" title="stripped">
<main data-attr="</BODY>">Hello</main>
  </BODY>
</HTML>"#;
    let parts = normalize_public_document(html).expect("valid mixed-case envelope");
    assert_eq!(
        parts.body_attributes,
        vec![
            ("class".to_string(), "post-page".to_string()),
            ("data-theme".to_string(), "v2".to_string()),
        ]
    );
    assert_eq!(parts.authorized_styles.len(), 2);
    assert!(parts.authorized_styles[0].contains("</head>"));
    assert!(parts.body_fragment.contains("Hello"));
    assert!(!parts.body_fragment.contains("Forged Title"));
    assert!(!parts.body_fragment.contains("evil.example"));
}

#[test]
fn rejects_duplicate_envelope_script_and_oversized() {
    let dup = "<html><body>a</body></html><html><body>b</body></html>";
    assert!(normalize_public_document(dup).is_err());

    let unclosed_style = "<html><head><style>body{}</head><body>a</body></html>";
    assert!(normalize_public_document(unclosed_style).is_err());

    let script = "<div><script>alert(1)</script></div>";
    assert!(normalize_public_document(script).is_err());

    for smuggled in [
        "<!-- --!><script>alert(1)</script><!-- -->",
        "<!--><script>alert(1)</script><!-- -->",
        "<!---><script>alert(1)</script><!-- -->",
        "<a href=\"https://ok.example\" href=\"javascript:alert(1)\">x</a>",
        "<xmp>raw</xmp>",
        "<plaintext>raw",
    ] {
        assert!(
            normalize_public_document(smuggled).is_err(),
            "expected reject on {smuggled}"
        );
    }

    let big = "a".repeat(MAX_PUBLIC_DOCUMENT_BYTES + 1);
    assert!(normalize_public_document(&big).is_err());
}

#[test]
fn stylesheet_link_decodes_entities_and_preserves_plain_text_lt() {
    let html = concat!(
        r#"<!DOCTYPE html><html><head>"#,
        r#"<link rel="stylesheet" href="&#x2f;theme&#x2f;rev&#x2f;assets&#x2f;style.css">"#,
        r#"</head><body><p>1 < 2</p><p>1 < 2 it's ok</p><p>a < b</p>< !doctype></body></html>"#
    );
    let parts = normalize_public_document(html).expect("valid entity stylesheet and lt text");
    assert_eq!(parts.authorized_styles.len(), 1);
    assert!(parts.authorized_styles[0].contains("style.css"));
    assert!(parts.body_fragment.contains("1 < 2"));
    assert!(parts.body_fragment.contains("1 < 2 it's ok"));
    assert!(parts.body_fragment.contains("a < b"));
    assert!(parts.body_fragment.contains("< !doctype>"));

    let tokens = tokenize("< !doctype><p>1 < 2</p>", false).expect("text lt tokenizes cleanly");
    assert!(matches!(tokens[0].0, TokenKind::Text("<")));
    assert!(
        !tokens
            .iter()
            .any(|(t, _)| matches!(t, TokenKind::Doctype(_)))
    );

    for bad in [
        r#"<link rel="stylesheet" href="&#x2f;&#x2f;evil.example&#x2f;a.css">"#,
        r#"<link rel="stylesheet" href="&#x2f;theme&#x2f;rev&#x2f;a.css&#x07;">"#,
    ] {
        let parts = normalize_public_document(bad).expect("tokenizes but strips unauthorized link");
        assert!(
            parts.authorized_styles.is_empty(),
            "expected unauthorized link stripped: {bad}"
        );
    }

    assert!(!is_authorized_stylesheet_link(&[
        ("rel".to_owned(), "stylesheet".to_owned()),
        ("rel".to_owned(), "icon".to_owned()),
        ("href".to_owned(), "/a.css".to_owned()),
    ]));
    assert!(!is_authorized_stylesheet_link(&[
        ("rel".to_owned(), "stylesheet".to_owned()),
        ("href".to_owned(), "/a.css".to_owned()),
        ("href".to_owned(), "/b.css".to_owned()),
    ]));

    for rejected in [
        r#"<link rel="stylesheet" rel="icon" href="/a.css">"#,
        r#"<link rel="stylesheet" href="/a.css" href="/b.css">"#,
        r#"<?x <div title="><meta charset='utf-8'>">"#,
        r#"</1 <div title=">hi">"#,
        r#"<!doctypex>"#,
        r#"<!CDATA[foo]>"#,
        r#"<style>a{}</style >"#,
        r#"<style>a{}</style/>"#,
    ] {
        assert!(
            normalize_public_document(rejected).is_err(),
            "expected reject on {rejected}"
        );
    }
}

#[test]
fn sanitize_render_head_fragment_strips_entity_encoded_seo_tags() {
    let raw = concat!(
        r#"<meta name="&#114;obots" content="index,follow">"#,
        r#"<meta name="&#x52;OBOTS" content="all">"#,
        r#"<link rel="&#99;anonical" href="https://evil.example/">"#,
        r#"<link rel="&#x61;lternate" hreflang="en" href="https://evil.example/en">"#,
        r#"<script type="&#97;pplication/ld+json">{"@type":"Fake"}</script>"#,
        r#"<script type="application&#x2f;ld&#43;json">{"@type":"Fake2"}</script>"#,
        r#"<script data-x=">" type="application/ld+json">{"@type":"GtInAttr"}</script>"#,
        r#"<meta name="author" content="OpenDash">"#,
    );
    let parts = sanitize_render_head_fragment(raw, true).expect("head fragment sanitizes");
    assert!(
        !parts.body_fragment.contains("evil.example"),
        "canonical/alternate escaped: {}",
        parts.body_fragment
    );
    assert!(
        !parts.body_fragment.contains("index,follow") && !parts.body_fragment.contains("OBOTS"),
        "robots meta escaped: {}",
        parts.body_fragment
    );
    assert!(
        parts.authorized_head_scripts.is_empty(),
        "json-ld script escaped: {:?}",
        parts.authorized_head_scripts
    );
    assert!(
        parts
            .body_fragment
            .contains(r#"<meta name="author" content="OpenDash">"#),
        "safe meta should be preserved: {}",
        parts.body_fragment
    );

    // Unquoted-attribute / attr-name quote smuggling must be rejected (F-01 & F-02)
    for smuggled in [
        r#"<div data-x=a'><script>alert(1)</script>'></div>"#,
        r#"<div data-x=a'><img/src="x"/onerror="alert(1)">'></div>"#,
        r#"<div a"><script>alert(1)</script>"></div>"#,
    ] {
        assert!(
            normalize_public_document_with_options(smuggled, false).is_err(),
            "expected document normalizer to reject quote differential payload: {smuggled}"
        );
    }
    let head_smuggle = r#"<meta name="author" content=a'><title>Hijacked</title><meta/name="robots"/content="index"><link/rel="canonical"/href="https://evil.example/"><script/type="application/ld+json">{"@type":"Fake"}</script>'>"#;
    assert!(
        sanitize_render_head_fragment(head_smuggle, true).is_err(),
        "expected sanitize_render_head_fragment to reject unquoted-attr SEO smuggling"
    );
}

#[test]
fn document_normalizer_gates_picture_source_and_admin_media_preview_urls() {
    let id = "a".repeat(64);
    let public_picture = format!(
        "<picture><source type=\"image/webp\" srcset=\"/media/{id}/variant/medium.webp?v=2 768w\"><img src=\"/media/{id}/file?v=2\" srcset=\"/media/{id}/variant/medium.jpg?v=2 768w\" sizes=\"100vw\" alt=\"OK\"></picture>"
    );
    let ok_pub = normalize_public_document(&public_picture).unwrap();
    assert!(ok_pub.body_fragment.contains("<source "));
    assert!(ok_pub.body_fragment.contains("</picture>"));

    let admin_picture = format!(
        "<picture><source type=\"image/webp\" srcset=\"/api/admin/media/{id}/variant/medium.webp?v=2 768w\"><img src=\"/api/admin/media/{id}/file?v=2\" alt=\"Preview\"></picture>"
    );
    assert_eq!(
        normalize_public_document(&admin_picture).unwrap_err().code,
        "invalid_media_srcset"
    );
    assert_eq!(
        normalize_public_document(&format!("<img src=\"/api/admin/media/{id}/file?v=2\">"))
            .unwrap_err()
            .code,
        "forbidden_admin_media_url"
    );
    let ok_prev = normalize_document_with_policy(
        &admin_picture,
        DocumentNormalizeOptions {
            allow_trusted_theme_markup: false,
            allow_admin_media_preview: true,
        },
    )
    .unwrap();
    assert!(ok_prev.body_fragment.contains("/api/admin/media/"));

    assert_eq!(
        normalize_document_with_policy(
            "<source srcset=\"x 1x\"></source>",
            DocumentNormalizeOptions::default(),
        )
        .unwrap_err()
        .code,
        "invalid_media_srcset"
    );
    assert_eq!(
        normalize_public_document("</source>").unwrap_err().code,
        "unexpected_void_close_tag"
    );
}
