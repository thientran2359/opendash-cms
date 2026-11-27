use super::*;
use crate::field_hydrate::HydrationContext;
use crate::storage::fields::{HydratedEntryRefRow, HydratedMediaRow, HydratedTermRefRow};
use crate::template_sec::RichTextNonceTable;
use opendash_core::content::{
    ContentMode, ContentTypeDefinition, ContentTypeSettings, EffectiveContentSchema,
};
use opendash_core::field_schema::{FieldRole, FieldSpec, FieldType, MediaFieldKind};
use opendash_core::registry::{DeclarationState, OwnedDescriptor, ProviderStatus, ResourceOwner};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[test]
fn f03_hydrated_entry_view_renders_v1_shapes_cover_preference_and_richtext_nonces() {
    let media_hex = "a".repeat(64);
    let alt_hex = "b".repeat(64);
    let doc_hex = "c".repeat(64);

    let mut extra_img = FieldSpec::new(FieldType::Media, "banner_img", "Ảnh phụ");
    extra_img.media_kind = Some(MediaFieldKind::Image);

    let mut doc_file = FieldSpec::new(FieldType::Media, "spec_pdf", "Tài liệu");
    doc_file.media_kind = Some(MediaFieldKind::File);

    let mut featured = FieldSpec::new(FieldType::Media, "featured_image", "Ảnh đại diện");
    featured.media_kind = Some(MediaFieldKind::Image);

    let gallery = FieldSpec::new(FieldType::Gallery, "photos", "Bộ ảnh");
    let link = FieldSpec::new(FieldType::Link, "cta", "Liên kết");
    let body = FieldSpec::new(FieldType::Richtext, "body", "Nội dung");

    let schema_def = ContentTypeDefinition {
        key: "product".into(),
        label: "Sản phẩm".into(),
        fields: vec![doc_file, extra_img, featured, gallery, link, body],
        settings: ContentTypeSettings::default(),
    };
    let effective = EffectiveContentSchema::assemble(
        OwnedDescriptor {
            owner: ResourceOwner::User,
            status: ProviderStatus::Active,
            declaration_state: DeclarationState::Declared,
            descriptor: schema_def.clone(),
        },
        1,
        vec![],
    )
    .unwrap();

    let mut media_map = BTreeMap::new();
    media_map.insert(
        media_hex.clone(),
        HydratedMediaRow {
            id: media_hex.clone(),
            kind: "image".into(),
            mime_type: "image/png".into(),
            version: 1,
            width: Some(800),
            height: Some(600),
            variants: Vec::new(),
        },
    );
    media_map.insert(
        alt_hex.clone(),
        HydratedMediaRow {
            id: alt_hex.clone(),
            kind: "image".into(),
            mime_type: "image/png".into(),
            version: 1,
            width: Some(400),
            height: Some(300),
            variants: Vec::new(),
        },
    );
    media_map.insert(
        doc_hex.clone(),
        HydratedMediaRow {
            id: doc_hex.clone(),
            kind: "file".into(),
            mime_type: "application/pdf".into(),
            version: 1,
            width: None,
            height: None,
            variants: Vec::new(),
        },
    );
    let entries_map: BTreeMap<i64, HydratedEntryRefRow> = BTreeMap::new();
    let terms_map: BTreeMap<i64, HydratedTermRefRow> = BTreeMap::new();
    let hctx = HydrationContext {
        media: &media_map,
        entries: &entries_map,
        terms: &terms_map,
        is_preview: false,
        tag_media_provenance: true,
    };

    let mut record = ContentEntryRecord {
        id: 11,
        content_type_key: "product".into(),
        locale: "vi".into(),
        translation_group_id: "tg-product-11".into(),
        translations: vec![],
        slug: "ban-go".into(),
        title: "Bàn gỗ".into(),
        values: [
            ("spec_pdf".into(), json!(doc_hex)),
            ("banner_img".into(), json!(alt_hex)),
            ("featured_image".into(), json!(media_hex)),
            ("photos".into(), json!([alt_hex])),
            (
                "cta".into(),
                json!({"url": "https://example.org", "label": "Mua ngay", "new_tab": false, "nofollow": false}),
            ),
            (
                "body".into(),
                json!("<p>Mô tả <strong>an toàn</strong><script>alert(1)</script></p>"),
            ),
        ]
        .into(),
        term_ids: vec![],
        primary_category_term_id: None,
        publication_status: "published".into(),
        page_layout_key: None,
        custom_template_key: None,
        content_mode: ContentMode::Editor,
        version: 1,
        created_at: String::new(),
        updated_at: String::new(),
        published_at: None,
    };

    let mut nonces = RichTextNonceTable::default();
    let view = build_hydrated_entry_view_with_context(
        &record,
        &effective,
        Some("/product/ban-go".into()),
        &[],
        &[],
        FieldRole::Editor,
        Some(&hctx),
        Some(&mut nonces),
    );
    // Cover prefers featured_image over earlier banner_img/spec_pdf.
    assert_eq!(
        view.cover_url.as_deref(),
        Some(format!("/media/{media_hex}/file?v=1").as_str())
    );
    let banner = view
        .field_list
        .iter()
        .find(|f| f.key == "banner_img")
        .unwrap();
    assert_eq!(banner.field_type, "media");
    assert_eq!(
        banner.media.as_ref().unwrap()["url"],
        format!("/media/{alt_hex}/file?v=1")
    );
    let photos = view.field_list.iter().find(|f| f.key == "photos").unwrap();
    assert_eq!(photos.field_type, "gallery");
    assert_eq!(photos.gallery.len(), 1);
    let cta = view.field_list.iter().find(|f| f.key == "cta").unwrap();
    assert_eq!(cta.field_type, "link");
    assert_eq!(cta.link.as_ref().unwrap()["url"], "https://example.org");

    let html = render_single_with_context_and_nonces_for_locale(
        &schema_def,
        &view,
        None,
        &DEFAULT_SITE,
        Some("/product"),
        &[],
        &RenderFormattingConfig::default(),
        "vi",
        Some(&nonces),
    )
    .unwrap();
    let decoded = html.replace("&#x2f;", "/").replace("&#x2F;", "/");
    assert!(decoded.contains(&format!("/media/{media_hex}/file")));
    assert!(decoded.contains("https://example.org"));
    assert!(html.contains("<p>Mô tả <strong>an toàn</strong></p>"));
    assert!(!html.contains("<script>"));

    // Builder blocks in body extract plain text without registering a richtext nonce.
    record.values.insert(
        "body".into(),
        json!("<!-- od:text {\"id\":\"b1\"} -->\n<p>Builder Copy</p>\n<!-- /od:text -->"),
    );
    let mut builder_nonces = RichTextNonceTable::default();
    let builder_view = build_hydrated_entry_view_with_context(
        &record,
        &effective,
        None,
        &[],
        &[],
        FieldRole::Editor,
        Some(&hctx),
        Some(&mut builder_nonces),
    );
    let body_field = builder_view
        .field_list
        .iter()
        .find(|f| f.key == "body")
        .unwrap();
    assert_eq!(body_field.text, "<p>Builder Copy</p>");
    assert!(builder_nonces.is_empty());
}

#[test]
fn b02_entry_view_with_url_pure_media_fallback_renders_default_single_and_archive() {
    let media_hex = "d".repeat(64);
    let gallery_hex = "e".repeat(64);

    let mut featured = FieldSpec::new(FieldType::Media, "featured_image", "Ảnh đại diện");
    featured.media_kind = Some(MediaFieldKind::Image);
    let gallery = FieldSpec::new(FieldType::Gallery, "photos", "Bộ ảnh");

    let schema_def = ContentTypeDefinition {
        key: "article".into(),
        label: "Article".into(),
        fields: vec![featured, gallery],
        settings: ContentTypeSettings::default(),
    };
    let mut values = BTreeMap::new();
    values.insert("featured_image".into(), json!(media_hex));
    values.insert("photos".into(), json!([gallery_hex]));

    let record = ContentEntryRecord {
        id: 77,
        content_type_key: "article".into(),
        locale: "vi".into(),
        translation_group_id: "grp-77".into(),
        translations: vec![],
        slug: "pure-media-fallback".into(),
        title: "Pure Media Fallback".into(),
        values,
        term_ids: vec![],
        primary_category_term_id: None,
        publication_status: "published".into(),
        page_layout_key: None,
        custom_template_key: None,
        content_mode: ContentMode::Editor,
        version: 1,
        created_at: String::new(),
        updated_at: "2026-11-27T00:00:00Z".into(),
        published_at: Some("2026-11-27T00:00:00Z".into()),
    };

    let view = entry_view_with_url(record, &schema_def, Some("/pure-media-fallback".into()));
    let single_html = render_single_with_context_and_nonces_for_locale(
        &schema_def,
        &view,
        None,
        &DEFAULT_SITE,
        Some("/articles"),
        &[],
        &RenderFormattingConfig::default(),
        "vi",
        None,
    )
    .expect("DEFAULT_SINGLE renders pure_media_fallback without od_image provenance error");
    assert!(
        single_html.contains(&format!("/media/{media_hex}/file")),
        "{single_html}"
    );
    assert!(
        single_html.contains(&format!("/media/{gallery_hex}/file")),
        "{single_html}"
    );
    assert!(!single_html.contains("OD_IMAGE_"));

    let pagination = PaginationView {
        page: 1,
        per_page: 10,
        total_items: 1,
        total_pages: 1,
        has_prev: false,
        has_next: false,
        previous_url: None,
        next_url: None,
        pages: Vec::new(),
    };
    let archive_html = render_archive_with_site_and_config_for_locale(
        &schema_def,
        &[view],
        None,
        None,
        &pagination,
        &[],
        None,
        None,
        &DEFAULT_SITE,
        &RenderFormattingConfig::default(),
        "vi",
    )
    .expect("DEFAULT_ARCHIVE renders pure_media_fallback without od_image provenance error");
    assert!(
        archive_html.contains(&format!("/media/{media_hex}/file")),
        "{archive_html}"
    );
    assert!(!archive_html.contains("OD_IMAGE_"));
}

#[test]
fn r2_01_entry_view_strips_private_children_inside_public_group_and_repeater() {
    use opendash_core::field_schema::FieldAcl;

    let pub_hex = "a".repeat(64);
    let priv_hex = "b".repeat(64);
    let mut pub_media = FieldSpec::new(FieldType::Media, "pub_img", "Public Img");
    pub_media.media_kind = Some(MediaFieldKind::Image);
    let mut priv_media = FieldSpec::new(FieldType::Media, "priv_img", "Private Img");
    priv_media.acl = Some(FieldAcl {
        min_view_role: Some(FieldRole::Editor),
        min_edit_role: Some(FieldRole::Editor),
        public_exposure: Some(PublicExposure::Private),
    });
    let mut priv_rt = FieldSpec::new(FieldType::Richtext, "priv_rt", "Private RichText");
    priv_rt.acl = Some(FieldAcl {
        min_view_role: Some(FieldRole::Admin),
        min_edit_role: Some(FieldRole::Admin),
        public_exposure: Some(PublicExposure::Private),
    });
    let mut pub_group = FieldSpec::new(FieldType::Group, "pub_group", "Public Group");
    pub_group.fields = vec![pub_media, priv_media, priv_rt];

    let pub_text = FieldSpec::new(FieldType::Text, "label_txt", "Label");
    let mut priv_rep_rt = FieldSpec::new(FieldType::Richtext, "secret_rt", "Secret");
    priv_rep_rt.acl = Some(FieldAcl {
        min_view_role: Some(FieldRole::Editor),
        min_edit_role: Some(FieldRole::Editor),
        public_exposure: Some(PublicExposure::Private),
    });
    let mut pub_rep = FieldSpec::new(FieldType::Repeater, "pub_rep", "Public Rep");
    pub_rep.fields = vec![pub_text, priv_rep_rt];

    let mut schema_def = ContentTypeDefinition {
        key: "case_r2".into(),
        label: "Case R2".into(),
        fields: vec![pub_group, pub_rep],
        settings: ContentTypeSettings::default(),
    };
    for f in &mut schema_def.fields {
        f.acl = Some(FieldAcl {
            min_view_role: Some(FieldRole::Editor),
            min_edit_role: Some(FieldRole::Editor),
            public_exposure: Some(PublicExposure::Public),
        });
    }
    let effective = EffectiveContentSchema::assemble(
        OwnedDescriptor {
            owner: ResourceOwner::User,
            status: ProviderStatus::Active,
            declaration_state: DeclarationState::Declared,
            descriptor: schema_def,
        },
        1,
        vec![],
    )
    .unwrap();

    let mut media_map = BTreeMap::new();
    media_map.insert(
        pub_hex.clone(),
        HydratedMediaRow {
            id: pub_hex.clone(),
            kind: "image".into(),
            mime_type: "image/png".into(),
            version: 1,
            width: Some(100),
            height: Some(100),
            variants: Vec::new(),
        },
    );
    media_map.insert(
        priv_hex.clone(),
        HydratedMediaRow {
            id: priv_hex.clone(),
            kind: "image".into(),
            mime_type: "image/png".into(),
            version: 1,
            width: Some(100),
            height: Some(100),
            variants: Vec::new(),
        },
    );
    let hctx = HydrationContext {
        media: &media_map,
        entries: &BTreeMap::new(),
        terms: &BTreeMap::new(),
        is_preview: false,
        tag_media_provenance: true,
    };
    let record = ContentEntryRecord {
        id: 22,
        content_type_key: "case_r2".into(),
        locale: "vi".into(),
        translation_group_id: "tg-case-r2".into(),
        translations: vec![],
        slug: "r2".into(),
        title: "R2".into(),
        values: [
            (
                "pub_group".into(),
                json!({ "pub_img": pub_hex, "priv_img": priv_hex, "priv_rt": "<p>secret</p>" }),
            ),
            (
                "pub_rep".into(),
                json!([{ "label_txt": "Open", "secret_rt": "<p>leak</p>" }]),
            ),
        ]
        .into(),
        term_ids: vec![],
        primary_category_term_id: None,
        publication_status: "published".into(),
        page_layout_key: None,
        custom_template_key: None,
        content_mode: ContentMode::Editor,
        version: 1,
        created_at: String::new(),
        updated_at: String::new(),
        published_at: None,
    };
    let mut nonces = RichTextNonceTable::default();
    let view = build_hydrated_entry_view_with_context(
        &record,
        &effective,
        None,
        &[],
        &[],
        FieldRole::Editor,
        Some(&hctx),
        Some(&mut nonces),
    );
    let grp = view.fields["pub_group"].as_object().unwrap();
    assert!(grp.contains_key("pub_img"));
    assert!(!grp.contains_key("priv_img"));
    assert!(!grp.contains_key("priv_rt"));
    let rep_row = view.fields["pub_rep"][0].as_object().unwrap();
    assert_eq!(rep_row["label_txt"], json!("Open"));
    assert!(!rep_row.contains_key("secret_rt"));
    assert!(nonces.is_empty());
}
