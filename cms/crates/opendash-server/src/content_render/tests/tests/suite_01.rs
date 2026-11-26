use super::*;

#[test]
fn public_document_injects_one_tailwind_stylesheet_without_legacy_body_width() {
    let seo = crate::seo::SeoView::for_noindex_page("vi", "Trang thử nghiệm");
    let html = compose_document_with_seo(
        Some("<header>Header</header>"),
        "<main class=\"od-prose\"><article>Classic Editor</article></main>",
        Some("<footer>Footer</footer>"),
        &seo,
    )
    .unwrap();
    let stylesheet = "<link rel=\"stylesheet\" href=\"/assets/opendash-tailwind.css\">";
    assert_eq!(html.matches(stylesheet).count(), 1);
    assert!(html.contains("<head>"));
    assert!(html.contains("<main class=\"od-prose\">"));
    assert!(!DEFAULT_SINGLE.contains("max-width:850px"));
    assert!(!DEFAULT_ARCHIVE.contains("max-width:850px"));
    assert!(!html.contains("max-width:850px"));
}

#[test]
fn trusted_document_deduplicates_enqueued_styles_and_preserves_head_scripts() {
    let seo = crate::seo::SeoView::for_noindex_page("vi", "Trusted markup");
    let assets = ResolvedEnqueuedAssets {
        head_styles: vec![
            "<link rel=\"stylesheet\" href=\"/theme/digest/assets/base.css\">".to_owned(),
        ],
        head_style_urls: vec!["/theme/digest/assets/base.css".to_owned()],
        head_scripts: vec![
            "<script type=\"module\" src=\"/theme/digest/assets/init.mjs\"></script>".to_owned(),
        ],
        footer_scripts: vec![
            "<script src=\"/theme/digest/assets/app.js\" defer></script>".to_owned(),
        ],
    };
    let html = compose_document_with_seo_with_assets(
        Some("<header>Header</header>"),
        concat!(
            "<!doctype html><html><head>",
            "<link rel=\"stylesheet\" href=\"/assets/opendash-tailwind.css\">",
            "<link href=\"/theme/digest/assets/base.css\" rel=\"stylesheet\">",
            "<link rel=\"stylesheet\" href=\"/theme/digest/assets/&#x62;ase.css\">",
            "<style>.trusted{display:block}</style>",
            "<script>if (2 < 3) { window.headReady = true; }</script>",
            "</head><body><main>Body</main></body></html>"
        ),
        Some("<footer>Footer</footer>"),
        &seo,
        &["od-route-home"],
        &assets,
        true,
    )
    .unwrap();
    assert_eq!(html.matches("/assets/opendash-tailwind.css").count(), 1);
    assert_eq!(html.matches("/theme/digest/assets/base.css").count(), 1);
    assert!(!html.contains("&#x62;ase.css"));
    assert!(html.contains("<style>.trusted{display:block}</style>"));
    assert_ordered_markup(
        &html,
        &[
            "/assets/opendash-tailwind.css",
            "/theme/digest/assets/base.css",
            "/theme/digest/assets/init.mjs",
            "window.headReady = true",
            "</head>",
            "<main>Body</main>",
            "<footer>Footer</footer>",
            "/theme/digest/assets/app.js",
        ],
    );
}

#[test]
fn escapes_untrusted_title_rich_text_and_template_tokens() {
    let schema = ContentTypeDefinition {
        key: "post".into(),
        label: "Bài viết".into(),
        settings: opendash_core::content::ContentTypeSettings::builtin_publication(),
        fields: vec![ContentFieldDefinition {
            key: "body".into(),
            label: "Nội dung".into(),
            field_type: ContentFieldType::RichText,
            required: false,
            options: vec![],
            ..Default::default()
        }],
    };
    let record = ContentEntryRecord {
        id: 1,
        content_type_key: "post".into(),
        locale: "vi".into(),
        translation_group_id: "tg_test_1".into(),
        translations: vec![],
        slug: "hello".into(),
        title: "<script>alert(1)</script>".into(),
        values: [(
            "body".into(),
            Value::String("<img src=x onerror=alert(2)> {{ 7*7 }}".into()),
        )]
        .into(),
        term_ids: vec![],
        primary_category_term_id: None,
        publication_status: "draft".into(),
        page_layout_key: None,
        custom_template_key: None,
        content_mode: opendash_core::content::ContentMode::Editor,
        version: 1,
        created_at: "".into(),
        updated_at: "".into(),
        published_at: None,
    };
    let html = render_single(&schema, &entry_view(record, &schema), None).unwrap();
    assert!(html.contains("&lt;script&gt;alert(1)&lt;&#x2f;script&gt;"));
    assert!(html.contains("&lt;img src=x onerror=alert(2)&gt;"));
    assert!(html.contains("{{ 7*7 }}"));
    assert!(!html.contains("<script>alert(1)</script>"));
}

#[test]
fn site_context_is_html_escaped_in_default_and_custom_templates() {
    let schema = ContentTypeDefinition {
        key: "news".into(),
        label: "Tin tức".into(),
        fields: vec![],
        settings: Default::default(),
    };
    let site = SiteContext {
        title: "<script>alert(1)</script>",
        tagline: "<img src=x onerror=alert(2)>",
        timezone: "UTC",
        origin: "",
        current_year: 2026,
    };
    let default_html = render_archive_with_site(&schema, &[], None, None, None, &site).unwrap();
    assert!(default_html.contains("&lt;script&gt;alert(1)&lt;"));
    assert!(default_html.contains("&lt;img src=x onerror=alert(2)&gt;"));
    assert!(!default_html.contains("<script>"));
    let custom_html = render_archive_with_site(
        &schema,
        &[],
        None,
        None,
        Some("<h1>{{ site.title }}</h1><p>{{ site.tagline }}</p>"),
        &site,
    )
    .unwrap();
    assert!(custom_html.contains("&lt;script&gt;"));
    assert!(!custom_html.contains("<img"));
}

#[test]
fn public_entry_without_resolved_href_never_forges_a_legacy_url() {
    let schema = ContentTypeDefinition {
        key: "post".into(),
        label: "Posts".into(),
        fields: vec![],
        settings: Default::default(),
    };
    let entry = ContentEntryRecord {
        id: 17,
        content_type_key: "post".into(),
        locale: "vi".into(),
        translation_group_id: "tg_test_17".into(),
        translations: vec![],
        slug: "old-slug".into(),
        title: "Safe title".into(),
        values: BTreeMap::new(),
        term_ids: vec![],
        primary_category_term_id: None,
        publication_status: "published".into(),
        page_layout_key: None,
        custom_template_key: None,
        content_mode: opendash_core::content::ContentMode::Editor,
        version: 1,
        created_at: String::new(),
        updated_at: String::new(),
        published_at: None,
    };
    let unresolved = entry_view_with_url(entry.clone(), &schema, None);
    assert_eq!(unresolved.url, None);
    let html = render_single_with_site_and_archive(&schema, &unresolved, None, &DEFAULT_SITE, None)
        .unwrap();
    assert!(!html.contains("content/post"));
    let resolved = entry_view_with_url(entry, &schema, Some("/blog/new-slug".into()));
    assert_eq!(resolved.url.as_deref(), Some("/blog/new-slug"));
    let html =
        render_single_with_site_and_archive(&schema, &resolved, None, &DEFAULT_SITE, Some("/blog"))
            .unwrap();
    assert!(html.contains("href=\"/blog\"") || html.contains("href=\"&#x2f;blog\""));
    assert!(!html.contains("content/post"));
}

#[test]
fn strip_richtext_handles_block_breaks_entities_and_plain_comparisons() {
    assert_eq!(
        strip_richtext_to_plain(
            "<p>Chào <strong>bạn</strong> &amp; 1 &lt; 2</p><p>Dòng 2<br/>Dòng 3</p>"
        ),
        "Chào bạn & 1 < 2 Dòng 2 Dòng 3"
    );
    assert_eq!(
        strip_richtext_to_plain("<p>1 < 2 it's ok</p>"),
        "1 < 2 it's ok"
    );
    assert_eq!(
        strip_richtext_to_plain("&amp;lt;script&amp;gt;"),
        "&lt;script&gt;"
    );
}

#[test]
fn entry_view_with_context_enforces_acl_declaration_order_nulls_and_excerpt() {
    use opendash_core::content::{
        AttachedFieldGroupSnapshot, ContentTypeSettings, EditorRegion, EffectiveFieldEntry,
        FieldGroupStatus,
    };
    use opendash_core::registry::{
        DeclarationState, OwnedDescriptor, ProviderStatus, ResourceOwner,
    };

    let base_price = ContentFieldDefinition {
        key: "price".into(),
        label: "Giá".into(),
        field_type: ContentFieldType::Number,
        min_view_role: FieldRole::Editor,
        public_exposure: PublicExposure::Public,
        ..Default::default()
    };
    let group_summary = ContentFieldDefinition {
        key: "summary".into(),
        label: "Tóm tắt".into(),
        field_type: ContentFieldType::ShortText,
        min_view_role: FieldRole::Editor,
        public_exposure: PublicExposure::Public,
        ..Default::default()
    };
    let admin_note = ContentFieldDefinition {
        key: "secret_note".into(),
        label: "Ghi chú Admin".into(),
        field_type: ContentFieldType::ShortText,
        min_view_role: FieldRole::Admin,
        public_exposure: PublicExposure::Public,
        ..Default::default()
    };
    let private_field = ContentFieldDefinition {
        key: "internal".into(),
        label: "Nội bộ".into(),
        field_type: ContentFieldType::ShortText,
        min_view_role: FieldRole::Editor,
        public_exposure: PublicExposure::Private,
        ..Default::default()
    };
    let missing_public = ContentFieldDefinition {
        key: "subtitle".into(),
        label: "Phụ đề".into(),
        field_type: ContentFieldType::ShortText,
        min_view_role: FieldRole::Editor,
        public_exposure: PublicExposure::Public,
        ..Default::default()
    };
    let extra_body = ContentFieldDefinition {
        key: "body".into(),
        label: "Nội dung".into(),
        field_type: ContentFieldType::RichText,
        min_view_role: FieldRole::Editor,
        public_exposure: PublicExposure::Public,
        ..Default::default()
    };

    let schema = EffectiveContentSchema {
        base: OwnedDescriptor {
            owner: ResourceOwner::Core,
            status: ProviderStatus::Active,
            declaration_state: DeclarationState::Declared,
            descriptor: ContentTypeDefinition {
                key: "product".into(),
                label: "Sản phẩm".into(),
                fields: vec![base_price.clone(), missing_public.clone()],
                settings: ContentTypeSettings::default(),
            },
        },
        base_version: 1,
        attached_groups: vec![AttachedFieldGroupSnapshot {
            group_id: "g1".into(),
            owner: ResourceOwner::Core,
            local_key: "g1".into(),
            title: "Group".into(),
            description: String::new(),
            fields: vec![
                group_summary.clone(),
                admin_note.clone(),
                private_field.clone(),
            ],
            default_region: EditorRegion::MainPrimary,
            default_order: 0,
            collapsible: false,
            status: FieldGroupStatus::Active,
            association_state: DeclarationState::Declared,
            version: 1,
        }],
        effective_fields: [
            base_price,
            missing_public,
            group_summary,
            admin_note,
            private_field,
            extra_body,
        ]
        .into_iter()
        .map(|definition| {
            (
                definition.key.clone(),
                EffectiveFieldEntry {
                    definition,
                    owner: ResourceOwner::Core,
                    source_id: "core".into(),
                    effective_status: EffectiveFieldStatus::Active,
                },
            )
        })
        .collect(),
        schema_token: "tok".into(),
    };

    let long_body = "<p>".to_owned() + &"tu ".repeat(210) + "</p>";
    let record = ContentEntryRecord {
        id: 9,
        content_type_key: "product".into(),
        locale: "vi".into(),
        translation_group_id: "tg_9".into(),
        translations: vec![],
        slug: "san-pham-1".into(),
        title: "SP 1".into(),
        values: [
            ("price".into(), Value::from(99000)),
            ("secret_note".into(), Value::String("Admin only".into())),
            ("internal".into(), Value::String("Private".into())),
            ("body".into(), Value::String(long_body)),
        ]
        .into(),
        term_ids: vec![12, 5],
        primary_category_term_id: Some(12),
        publication_status: "published".into(),
        page_layout_key: None,
        custom_template_key: None,
        content_mode: opendash_core::content::ContentMode::Editor,
        version: 2,
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-02T00:00:00Z".into(),
        published_at: Some("2026-01-02T00:00:00Z".into()),
    };

    let terms = vec![
        PublicTermView {
            id: 5,
            kind: "category".into(),
            name: "Cat 5".into(),
            slug: "cat-5".into(),
            url: Some("/category/cat-5".into()),
        },
        PublicTermView {
            id: 12,
            kind: "category".into(),
            name: "Cat 12".into(),
            slug: "cat-12".into(),
            url: Some("/category/cat-12".into()),
        },
    ];
    let eligible = vec!["category".into(), "brand".into()];
    let public_view = entry_view_with_context(
        &record,
        &schema,
        Some("/product/san-pham-1".into()),
        &terms,
        &eligible,
        FieldRole::Editor,
    );
    let keys: Vec<&str> = public_view.fields.iter().map(|f| f.key.as_str()).collect();
    assert_eq!(keys, vec!["price", "subtitle", "summary", "body"]);
    assert_eq!(public_view.values.get("subtitle"), Some(&Value::Null));
    assert!(!public_view.values.contains_key("secret_note"));
    assert!(!public_view.values.contains_key("internal"));
    assert!(public_view.excerpt.chars().count() <= 200);
    assert!(public_view.excerpt.ends_with("..."));
    assert_eq!(public_view.reading_time_minutes, 2);
    assert_eq!(
        public_view.primary_category.as_ref().map(|c| c.id),
        Some(12)
    );
    assert!(public_view.terms.contains_key("tag"));
    assert!(public_view.terms.contains_key("brand"));

    // Number field alone (no text fields) must produce empty excerpt & 0 reading_time_minutes.
    let mut num_only_rec = record.clone();
    num_only_rec.values = [("price".into(), Value::from(42))].into();
    num_only_rec.primary_category_term_id = Some(999);
    let num_only_view = authenticated_preview_view_with_context(
        &num_only_rec,
        &schema,
        None,
        &terms,
        &eligible,
        FieldRole::Admin,
    );
    assert!(num_only_view.values.contains_key("secret_note"));
    assert_eq!(num_only_view.excerpt, "");
    assert_eq!(num_only_view.reading_time_minutes, 0);
    assert_eq!(
        num_only_view.primary_category.as_ref().map(|c| c.id),
        Some(5)
    );
    assert_eq!(
        num_only_view.url.as_deref(),
        Some("/api/admin/content-types/product/entries/9/preview")
    );
}

#[test]
fn pagination_view_windows_and_clamps_max_archive_page() {
    let empty = PaginationView::compute(1, 20, 0, None, None, |p| Some(format!("/posts?page={p}")));
    assert_eq!(empty.total_pages, 0);
    assert!(empty.pages.is_empty());

    let small = PaginationView::compute(
        2,
        10,
        50,
        Some("/posts".into()),
        Some("/posts?page=3".into()),
        |p| Some(format!("/{p}")),
    );
    assert_eq!(small.total_pages, 5);
    assert_eq!(small.pages.len(), 5);
    assert!(small.has_prev);
    assert!(small.has_next);
    assert!(small.pages[1].is_current);

    let start = PaginationView::compute(3, 10, 200, None, None, |p| Some(format!("/{p}")));
    assert_eq!(start.pages.len(), 9);
    assert!(start.pages[7].is_ellipsis);
    assert_eq!(start.pages[8].number, Some(20));

    let mid = PaginationView::compute(10, 10, 200, None, None, |p| Some(format!("/{p}")));
    assert_eq!(mid.pages.len(), 9);
    assert_eq!(mid.pages[0].number, Some(1));
    assert!(mid.pages[1].is_ellipsis);
    assert_eq!(mid.pages[4].number, Some(10));
    assert!(mid.pages[4].is_current);
    assert!(mid.pages[7].is_ellipsis);
    assert_eq!(mid.pages[8].number, Some(20));

    let end = PaginationView::compute(18, 10, 200, None, None, |p| Some(format!("/{p}")));
    assert_eq!(end.pages.len(), 9);
    assert!(end.pages[1].is_ellipsis);
    assert_eq!(end.pages[8].number, Some(20));

    let capped = PaginationView::compute(1, 1, 500_000, None, None, |_| None);
    assert_eq!(capped.total_pages, crate::public_context::MAX_ARCHIVE_PAGE);
}

#[test]
fn compose_document_merges_and_deduplicates_body_classes() {
    let seo = crate::seo::SeoView::for_noindex_page("vi", "Class Test");
    let html = compose_document_with_seo_with_body_classes(
        None,
        "<!doctype html><html><head></head><body class=\"alpha od-route-home alpha\"><main>Hi</main></body></html>",
        None,
        &seo,
        &["od-route-home", "od-extra", "alpha"],
    )
    .unwrap();
    assert_eq!(html.matches("class=\"").count(), 1);
    assert!(html.contains("class=\"alpha od-route-home od-extra\""));
}
