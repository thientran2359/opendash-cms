use super::*;
use opendash_core::field_schema::FieldContext;

fn sample_hash(fill: char) -> String {
    std::iter::repeat_n(fill, 64).collect()
}

#[test]
fn hydrates_all_option_field_shapes_with_nonces_and_public_filter() {
    let img_id = sample_hash('a');
    let vid_id = sample_hash('b');
    let term_img_id = sample_hash('c');

    let specs: Vec<FieldSpec> = serde_json::from_value(json!([
        { "type": "tab", "key": "tab_main", "label": "Main" },
        { "type": "text", "key": "headline", "label": "Headline", "default": "Default", "public": true },
        { "type": "text", "key": "internal_note", "label": "Secret Note", "public": false },
        { "type": "media", "key": "hero_img", "label": "Hero", "public": true },
        { "type": "gallery", "key": "slides", "label": "Slides", "media_kind": "any", "public": true },
        { "type": "link", "key": "cta", "label": "CTA", "public": true },
        { "type": "icon", "key": "badge_icon", "label": "Icon", "public": true },
        { "type": "spacing", "key": "outer_margin", "label": "Margin", "property": "margin", "min": -128, "public": true },
        { "type": "richtext", "key": "bio", "label": "Bio", "public": true },
        { "type": "entry_ref", "key": "featured", "label": "Featured", "target_types": ["post"], "public": true },
        { "type": "term_ref", "key": "topic", "label": "Topic", "taxonomy": "category", "public": true },
        {
            "type": "group",
            "key": "banner",
            "label": "Banner",
            "public": true,
            "fields": [
                { "type": "media", "key": "bg", "label": "BG" },
                { "type": "link", "key": "btn", "label": "Btn" }
            ]
        },
        {
            "type": "repeater",
            "key": "cards",
            "label": "Cards",
            "public": true,
            "fields": [
                { "type": "text", "key": "title", "label": "Title" },
                { "type": "richtext", "key": "body", "label": "Body" }
            ]
        }
    ]))
    .unwrap();
    opendash_core::field_schema::validate_field_specs(&specs, FieldContext::Options).unwrap();

    let raw = BTreeMap::from([
        ("headline".into(), json!("OpenDash V1")),
        ("internal_note".into(), json!("do not leak")),
        ("hero_img".into(), json!(img_id)),
        ("slides".into(), json!([img_id, vid_id, sample_hash('f')])),
        (
            "cta".into(),
            json!({ "url": "/contact", "label": "Contact", "new_tab": true, "nofollow": true }),
        ),
        ("badge_icon".into(), json!("star")),
        ("outer_margin".into(), json!({ "top": -16, "bottom": 24 })),
        (
            "bio".into(),
            json!("<p>Hello <script>alert(1)</script><strong>world</strong></p>"),
        ),
        ("featured".into(), json!(10)),
        ("topic".into(), json!(5)),
        (
            "banner".into(),
            json!({ "bg": img_id, "btn": { "url": "", "label": "Empty" } }),
        ),
        (
            "cards".into(),
            json!([{ "title": "One", "body": "<p>Safe card</p>" }]),
        ),
    ]);

    let media = BTreeMap::from([
        (
            img_id.clone(),
            HydratedMediaRow {
                id: img_id.clone(),
                kind: "image".into(),
                mime_type: "image/jpeg".into(),
                version: 1,
                width: Some(1200),
                height: Some(630),
                variants: vec![crate::storage::fields::HydratedMediaVariantRow {
                    spec: opendash_core::media::MediaVariantSpec::Medium,
                    format: opendash_core::media::MediaVariantFormat::Webp,
                    width: 768,
                    height: 403,
                    byte_size: 12_345,
                }],
            },
        ),
        (
            vid_id.clone(),
            HydratedMediaRow {
                id: vid_id.clone(),
                kind: "video".into(),
                mime_type: "video/mp4".into(),
                version: 1,
                width: Some(1920),
                height: Some(1080),
                variants: Vec::new(),
            },
        ),
        (
            term_img_id.clone(),
            HydratedMediaRow {
                id: term_img_id.clone(),
                kind: "image".into(),
                mime_type: "image/png".into(),
                version: 1,
                width: Some(400),
                height: Some(400),
                variants: Vec::new(),
            },
        ),
    ]);
    let entries = BTreeMap::from([(
        10,
        HydratedEntryRefRow {
            id: 10,
            type_key: "post".into(),
            title: "Post Ten".into(),
            slug: "post-ten".into(),
            url: "/post-ten".into(),
            cover_url: Some(format!("/media/{img_id}/file?v=1")),
            excerpt: "Summary".into(),
            published_at: None,
        },
    )]);
    let terms = BTreeMap::from([(
        5,
        HydratedTermRefRow {
            id: 5,
            taxonomy_key: "category".into(),
            name: "News".into(),
            slug: "news".into(),
            url: "/category/news".into(),
            featured_image_media_id: Some(term_img_id.clone()),
        },
    )]);
    let ctx = HydrationContext {
        media: &media,
        entries: &entries,
        terms: &terms,
        is_preview: false,
        tag_media_provenance: false,
    };

    let mut nonces = RichTextNonceTable::default();
    let hydrated = hydrate_field_values(
        &specs,
        &raw,
        &ctx,
        Some(&mut nonces),
        HydrationVisibility::OptionsPublicOnly,
    )
    .unwrap();

    assert!(!hydrated.contains_key("tab_main"));
    assert!(!hydrated.contains_key("internal_note"));
    assert_eq!(hydrated["headline"], json!("OpenDash V1"));
    assert_eq!(
        hydrated["hero_img"]["url"],
        json!(format!("/media/{img_id}/file?v=1"))
    );
    assert_eq!(
        hydrated["hero_img"]["thumbnail_url"],
        json!(format!("/media/{img_id}/thumbnail?v=1"))
    );
    assert_eq!(hydrated["hero_img"]["version"], json!(1));
    assert_eq!(
        hydrated["hero_img"]["webp_srcset"],
        json!(format!("/media/{img_id}/variant/medium.webp?v=1 768w"))
    );
    assert!(
        hydrated["hero_img"]
            .get(crate::template_sec::MEDIA_PROVENANCE_KEY)
            .is_none()
    );
    assert_eq!(hydrated["hero_img"]["width"], json!(1200));
    assert_eq!(hydrated["slides"].as_array().unwrap().len(), 2);
    assert_eq!(hydrated["slides"][1]["thumbnail_url"], json!(""));
    assert_eq!(hydrated["cta"]["target"], json!("_blank"));
    assert_eq!(
        hydrated["cta"]["rel"],
        json!("noopener noreferrer nofollow")
    );
    assert_eq!(hydrated["badge_icon"], json!({ "name": "star" }));
    assert!(
        hydrated["outer_margin"]["classes"]
            .as_str()
            .unwrap()
            .contains("-mt-4")
    );
    assert!(
        hydrated["outer_margin"]["classes"]
            .as_str()
            .unwrap()
            .contains("mb-6")
    );
    assert!(
        hydrated["bio"]
            .as_str()
            .unwrap()
            .starts_with("OD_RICHTEXT_")
    );
    assert_eq!(hydrated["featured"]["title"], json!("Post Ten"));
    assert_eq!(hydrated["topic"]["kind"], json!("category"));
    assert_eq!(
        hydrated["topic"]["featured_image"]["url"],
        json!(format!("/media/{term_img_id}/file?v=1"))
    );
    assert_eq!(hydrated["banner"]["bg"]["id"], json!(img_id));
    assert_eq!(hydrated["banner"]["btn"], Value::Null);
    let card_nonce = hydrated["cards"][0]["body"].as_str().unwrap();
    assert!(card_nonce.starts_with("OD_RICHTEXT_"));

    // Nonce substitution works inside <body> and fails closed inside attributes
    let html = format!(
        "<!doctype html><html><body><div>{}</div><article>{}</article></body></html>",
        hydrated["bio"].as_str().unwrap(),
        card_nonce
    );
    let substituted = nonces.substitute_into_document(html).unwrap();
    assert!(substituted.contains("<strong>world</strong>"));
    assert!(!substituted.contains("<script>"));
    assert!(substituted.contains("<p>Safe card</p>"));

    let bad_attr = format!("<div title=\"{}\"></div>", card_nonce);
    assert!(nonces.substitute_into_document(bad_attr).is_err());

    // Plain sanitized mode (for plugin host SiteOptionsGet) returns HTML without nonces
    let plain = hydrate_field_values(
        &specs,
        &raw,
        &ctx,
        None,
        HydrationVisibility::OptionsPublicOnly,
    )
    .unwrap();
    assert!(
        plain["bio"]
            .as_str()
            .unwrap()
            .contains("<strong>world</strong>")
    );
    assert!(!plain["bio"].as_str().unwrap().starts_with("OD_RICHTEXT_"));
}

#[test]
fn hydrates_responsive_studio_block_scalars_and_nested_repeater_groups() {
    let specs: Vec<FieldSpec> = serde_json::from_value(json!([
        {
            "type": "radio_buttons",
            "key": "align",
            "label": "Align",
            "responsive": true,
            "default": "center",
            "options": [
                { "value": "left", "label": "Left" },
                { "value": "center", "label": "Center" }
            ]
        },
        {
            "type": "range",
            "key": "radius",
            "label": "Radius",
            "min": 0,
            "max": 32,
            "responsive": true,
            "default": 8
        },
        {
            "type": "select",
            "key": "layout",
            "label": "Layout",
            "responsive": true,
            "default": "grid",
            "options": [
                { "value": "grid", "label": "Grid" },
                { "value": "stack", "label": "Stack" }
            ]
        },
        {
            "type": "repeater",
            "key": "items",
            "label": "Items",
            "max_items": 1,
            "fields": [
                { "type": "text", "key": "title", "label": "Title" },
                {
                    "type": "group",
                    "key": "meta",
                    "label": "Meta",
                    "fields": [
                        { "type": "icon", "key": "badge", "label": "Badge" }
                    ]
                }
            ]
        }
    ]))
    .unwrap();
    opendash_core::field_schema::validate_field_specs(&specs, FieldContext::StudioBlock).unwrap();

    let raw = BTreeMap::from([
        (
            "align".into(),
            json!({ "desktop": "left", "mobile": "center" }),
        ),
        ("radius".into(), json!({ "tablet": 12 })),
        ("layout".into(), json!({ "desktop": "stack" })),
        (
            "items".into(),
            json!([
                { "title": "First", "meta": { "badge": "star" } },
                { "title": "Truncated", "meta": { "badge": "check" } }
            ]),
        ),
    ]);
    let media = BTreeMap::new();
    let entries = BTreeMap::new();
    let terms = BTreeMap::new();
    let ctx = HydrationContext {
        media: &media,
        entries: &entries,
        terms: &terms,
        is_preview: false,
        tag_media_provenance: false,
    };
    let hydrated =
        hydrate_field_values(&specs, &raw, &ctx, None, HydrationVisibility::All).unwrap();
    assert_eq!(hydrated["align"], json!("left"));
    assert_eq!(hydrated["radius"], json!(8));
    assert_eq!(hydrated["layout"], json!("stack"));
    assert_eq!(hydrated["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        hydrated["items"][0]["meta"]["badge"],
        json!({ "name": "star" })
    );
}

#[test]
fn content_public_visibility_filters_nested_private_and_admin_group_repeater_children() {
    use opendash_core::content::FieldRole;

    let pub_img = sample_hash('a');
    let priv_img = sample_hash('b');
    let specs: Vec<FieldSpec> = serde_json::from_value(json!([
        {
            "type": "group",
            "key": "pub_group",
            "label": "Public Group",
            "acl": { "min_view_role": "editor", "min_edit_role": "editor", "public_exposure": "public" },
            "fields": [
                { "type": "media", "key": "pub_media", "label": "Pub Media", "acl": { "min_view_role": "editor", "min_edit_role": "editor", "public_exposure": "public" } },
                { "type": "media", "key": "priv_media", "label": "Priv Media", "acl": { "min_view_role": "editor", "min_edit_role": "editor", "public_exposure": "private" } },
                { "type": "richtext", "key": "priv_rt", "label": "Priv RT", "acl": { "min_view_role": "admin", "min_edit_role": "admin", "public_exposure": "public" } }
            ]
        },
        {
            "type": "repeater",
            "key": "pub_rep",
            "label": "Public Repeater",
            "acl": { "min_view_role": "editor", "min_edit_role": "editor", "public_exposure": "public" },
            "fields": [
                { "type": "text", "key": "pub_title", "label": "Title", "acl": { "min_view_role": "editor", "min_edit_role": "editor", "public_exposure": "public" } },
                { "type": "text", "key": "priv_secret", "label": "Secret", "acl": { "min_view_role": "editor", "min_edit_role": "editor", "public_exposure": "private" } },
                { "type": "richtext", "key": "priv_rep_rt", "label": "Priv Rep RT", "acl": { "min_view_role": "editor", "min_edit_role": "editor", "public_exposure": "private" } }
            ]
        }
    ]))
    .unwrap();
    let raw = BTreeMap::from([
        (
            "pub_group".into(),
            json!({ "pub_media": pub_img, "priv_media": priv_img, "priv_rt": "<p>secret</p>" }),
        ),
        (
            "pub_rep".into(),
            json!([{ "pub_title": "Visible", "priv_secret": "Hidden", "priv_rep_rt": "<p>hidden</p>" }]),
        ),
    ]);
    let vis = HydrationVisibility::ContentPublic {
        max_view_role: FieldRole::Editor,
    };
    let mut media_ids = BTreeSet::new();
    let mut entry_ids = BTreeSet::new();
    let mut term_ids = BTreeSet::new();
    collect_hydration_ids(
        &specs,
        &raw,
        vis,
        &mut media_ids,
        &mut entry_ids,
        &mut term_ids,
    );
    assert!(media_ids.contains(&pub_img));
    assert!(!media_ids.contains(&priv_img));

    let media = BTreeMap::from([(
        pub_img.clone(),
        HydratedMediaRow {
            id: pub_img.clone(),
            kind: "image".into(),
            mime_type: "image/png".into(),
            version: 1,
            width: Some(100),
            height: Some(100),
            variants: Vec::new(),
        },
    )]);
    let entries = BTreeMap::new();
    let terms = BTreeMap::new();
    let ctx = HydrationContext {
        media: &media,
        entries: &entries,
        terms: &terms,
        is_preview: false,
        tag_media_provenance: false,
    };
    let mut nonces = RichTextNonceTable::default();
    let hydrated = hydrate_field_values(&specs, &raw, &ctx, Some(&mut nonces), vis).unwrap();
    let grp = hydrated["pub_group"].as_object().unwrap();
    assert!(grp.contains_key("pub_media"));
    assert!(!grp.contains_key("priv_media"));
    assert!(!grp.contains_key("priv_rt"));
    let row = hydrated["pub_rep"][0].as_object().unwrap();
    assert_eq!(row["pub_title"], json!("Visible"));
    assert!(!row.contains_key("priv_secret"));
    assert!(!row.contains_key("priv_rep_rt"));
    assert!(nonces.is_empty());

    let mut rt_nonces = RichTextNonceTable::default();
    let mut rt_html = BTreeMap::new();
    let rt_specs: Vec<FieldSpec> = serde_json::from_value(json!([
        { "type": "richtext", "key": "bio", "label": "Bio", "public": true },
        { "type": "group", "key": "meta", "label": "Meta", "public": true, "fields": [
            { "type": "richtext", "key": "note", "label": "Note" }
        ]}
    ]))
    .unwrap();
    let rt_raw = BTreeMap::from([
        ("bio".into(), json!("<p>Top<script>x</script></p>")),
        ("meta".into(), json!({ "note": "<p>Nested</p>" })),
    ]);
    let hydrated_rt = hydrate_field_values_with_richtext(
        &rt_specs,
        &rt_raw,
        &ctx,
        Some(&mut rt_nonces),
        vis,
        Some(&mut rt_html),
        None,
    )
    .unwrap();
    assert!(
        hydrated_rt["bio"]
            .as_str()
            .unwrap()
            .starts_with("OD_RICHTEXT_")
    );
    assert_eq!(rt_html.get("bio").map(String::as_str), Some("<p>Top</p>"));
    assert_eq!(
        rt_html.get("meta.note").map(String::as_str),
        Some("<p>Nested</p>")
    );
}
