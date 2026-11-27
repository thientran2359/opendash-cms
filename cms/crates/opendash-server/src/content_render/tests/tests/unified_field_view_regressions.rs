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
