use super::*;

#[test]
fn sanitizes_basic_allowlist_and_adds_rel() {
    let out = sanitize_richtext(
        r#"<p class="x" onclick="evil()">Hello <strong>World</strong> and <a href="https://example.com/path?a=1&b=2" target="_blank">Link</a></p>"#,
    )
    .unwrap();
    assert_eq!(
        out,
        r#"<p>Hello <strong>World</strong> and <a href="https://example.com/path?a=1&amp;b=2" rel="noopener noreferrer">Link</a></p>"#
    );
    // Idempotent under second sanitize pass
    assert_eq!(sanitize_richtext(&out).unwrap(), out);
}

#[test]
fn blocks_url_entity_smuggling_and_duplicate_href() {
    let cases = [
        r#"<a href="java&#x09;script:alert(1)">bad</a>"#,
        r#"<a href="java&#10;script:alert(1)">bad</a>"#,
        r#"<a href="&#106;&#97;vascript:alert(1)">bad</a>"#,
        r#"<a href="&#0000106avascript:alert(1)">bad</a>"#,
        r#"<a href="javascript&colon;alert(1)">bad</a>"#,
        r#"<a href="java&#0;script:alert(1)">bad</a>"#,
        r#"<a href="data:text/html,<script>alert(1)</script>">bad</a>"#,
        r#"<a href="vbscript:msgbox(1)">bad</a>"#,
        r#"<a href="//evil.com/phish">bad</a>"#,
        r#"<a href="/foo/../secret">bad</a>"#,
        r#"<a href="https://safe.com" href="javascript:alert(1)">bad</a>"#,
    ];
    for raw in cases {
        assert_eq!(sanitize_richtext(raw).unwrap(), "bad", "failed on {raw}");
    }
}

#[test]
fn forbids_nested_anchors_and_balances_misnested_tags() {
    let nested = sanitize_richtext(
        r#"<a href="https://a.example">outer <a href="https://b.example">inner</a></a>"#,
    )
    .unwrap();
    assert_eq!(
        nested,
        r#"<a href="https://a.example" rel="noopener noreferrer">outer inner</a>"#
    );

    let balanced = sanitize_richtext("<b><i>bolditalic</b> tail</i>").unwrap();
    assert_eq!(balanced, "<b><i>bolditalic</i></b> tail");
}

#[test]
fn strips_rawtext_rcdata_foreign_and_html5_comments() {
    assert_eq!(
        sanitize_richtext("<p>ok</p><script>alert(1)</script><style>body{}</style>").unwrap(),
        "<p>ok</p>"
    );
    assert_eq!(
        sanitize_richtext("<title><img src=x onerror=1></title><p>hi</p>").unwrap(),
        "<p>hi</p>"
    );
    assert_eq!(
        sanitize_richtext("<textarea>bad</textarea><xmp>bad</xmp><noembed>bad</noembed><noframes>bad</noframes><p>fine</p>").unwrap(),
        "<p>fine</p>"
    );
    assert_eq!(
        sanitize_richtext("<p>a</p><plaintext><b>ignored forever</b>").unwrap(),
        "<p>a</p>"
    );
    assert_eq!(
        sanitize_richtext(
            "<iframe src='x'>sub</iframe><object>obj</object><embed src='y'><span>kept</span>"
        )
        .unwrap(),
        "kept"
    );
    assert_eq!(
        sanitize_richtext(
            "<svg><svg></svg><script>alert(1)</script></svg><math><mtext>x</mtext></math><b>ok</b>"
        )
        .unwrap(),
        "<b>ok</b>"
    );
    assert_eq!(sanitize_richtext("<!--><b>ok</b>").unwrap(), "<b>ok</b>");
    assert_eq!(sanitize_richtext("<!---><p>x</p>").unwrap(), "<p>x</p>");
    assert_eq!(
        sanitize_richtext("<!-- hide --!><p>ok</p>").unwrap(),
        "<p>ok</p>"
    );
}

#[test]
fn enforces_depth_cap_null_and_byte_limits() {
    assert!(sanitize_richtext("abc\0def").is_err());

    let too_big = "a".repeat(MAX_RICHTEXT_INPUT_BYTES + 1);
    assert!(sanitize_richtext(&too_big).is_err());

    // Raw & expands to &amp; (5x), exceeding 32 KiB output limit even under 64 KiB input.
    let amplifies = "&".repeat(10_000);
    assert!(sanitize_richtext(&amplifies).is_err());

    let deep_open = "<b>".repeat(20);
    let deep_close = "</b>".repeat(20);
    let out = sanitize_richtext(&format!("{deep_open}hi{deep_close}")).unwrap();
    assert_eq!(
        out,
        format!(
            "{}hi{}",
            "<b>".repeat(MAX_RICHTEXT_DEPTH),
            "</b>".repeat(MAX_RICHTEXT_DEPTH)
        )
    );
}

#[test]
fn strips_richtext_to_plain_cleanly() {
    assert_eq!(
        strip_richtext_to_plain(
            "<p>Xin   chào</p><script>evil()</script><style>.x{display:none}</style><!-- bí mật --><p><b>OpenDash</b>&nbsp;Studio &amp; CMS</p>"
        ),
        "Xin chào OpenDash Studio & CMS"
    );
    assert_eq!(
        strip_richtext_to_plain("<p>1 < 2 it's ok</p><!-- unclosed secret"),
        "1 < 2 it's ok"
    );
    assert_eq!(
        strip_richtext_to_plain("<p>Giữ nguyên</p><script>unclosed"),
        "Giữ nguyên"
    );
    assert_eq!(
        strip_richtext_to_plain("&amp;lt;script&amp;gt;"),
        "&lt;script&gt;"
    );
}
