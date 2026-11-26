use super::*;

#[test]
fn build_public_theme_context_includes_all_strict_base_keys() {
    let site_opts = BTreeMap::new();
    let opts = BTreeMap::new();
    let assets = BTreeMap::new();
    let menus = BTreeMap::new();
    let pagination = PaginationView::empty();
    let ctx = build_public_theme_context(PublicThemeContextInput {
        site: &DEFAULT_SITE,
        site_options: &site_opts,
        options: &opts,
        theme_assets: &assets,
        menus: &menus,
        route_kind: "home",
        route_path: Some("/"),
        breadcrumbs: &[],
        pagination: &pagination,
        archive_url: None,
        previous_url: None,
        next_url: None,
        entries: &[],
        related_entries: &[],
        recent_posts: &[],
        taxonomy: None,
        content_type: None,
        entry: None,
    })
    .unwrap();
    for key in [
        "site",
        "site_options",
        "options",
        "theme_options",
        "theme_assets",
        "menus",
        "route",
        "breadcrumbs",
        "pagination",
        "archive_url",
        "previous_url",
        "next_url",
        "entries",
        "related_entries",
        "recent_posts",
        "taxonomy",
        "content_type",
        "entry",
    ] {
        assert!(ctx.get(key).is_some(), "missing key {key}");
    }
}

#[test]
fn apply_render_hooks_injects_at_head_end_marker_despite_head_script_literal() {
    let seo = crate::seo::SeoView::for_noindex_page("vi", "Marker Test");
    let assets = ResolvedEnqueuedAssets::default();
    let composed = compose_document_with_seo_with_assets(
        None,
        concat!(
            "<!doctype html><html><head>",
            "<script>window.x = \"</head>\";</script>",
            "</head><body><main>Body</main></body></html>"
        ),
        None,
        &seo,
        &[],
        &assets,
        true,
    )
    .unwrap();
    assert!(composed.contains(HEAD_END_MARKER));

    let final_html = apply_render_hooks_to_document(
        composed,
        r#"<meta name="x-hook" content="ok">"#,
        &[],
        &[],
        "",
        &[],
    )
    .unwrap();
    assert!(!final_html.contains(HEAD_END_MARKER));
    let script_end = final_html.find("</script>").unwrap();
    let hook_pos = final_html
        .find(r#"<meta name="x-hook" content="ok">"#)
        .unwrap();
    let real_head_end = final_html.rfind("</head>").unwrap();
    assert!(
        script_end < hook_pos && hook_pos < real_head_end,
        "head hook must be injected AFTER inline head script and BEFORE real </head>: {final_html}"
    );
}
