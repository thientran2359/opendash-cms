use super::*;

#[test]
fn od_image_enforces_csec_provenance_nonce_boundaries_and_attributes() {
    let media_id = "a".repeat(64);
    let schema: opendash_core::block_schema::BlockSchemaV1 =
        serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "id": "demo/img",
            "title": "Img Test",
            "category": "content",
            "render": "render.html",
            "fields": [
                { "type": "media", "key": "hero", "label": "Hero", "media_kind": "image" }
            ]
        }))
        .unwrap();

    let valid_tpl = r#"<figure>{{ od_image(hero, "(max-width: 768px) 100vw, 50vw", "w-full rounded", "Banner Alt", "lazy", "async", "high") }}</figure>"#;
    validate_block_template_csec("demo/img", valid_tpl, &schema).unwrap();

    let tagged_media = serde_json::json!({
        "id": media_id,
        "version": 1,
        "kind": "image",
        "url": format!("/media/{media_id}/file?v=1"),
        "thumbnail_url": format!("/media/{media_id}/thumbnail?v=1"),
        "alt": "Default Alt",
        "width": 1200,
        "height": 800,
        "webp_srcset": format!("/media/{media_id}/variant/medium.webp?v=1 768w, /media/{media_id}/variant/full.webp?v=1 1200w"),
        "fallback_srcset": format!("/media/{media_id}/variant/medium.jpg?v=1 768w, /media/{media_id}/file?v=1 1200w"),
        "variants": [],
        (MEDIA_PROVENANCE_KEY): media_provenance_token(),
    });
    let mut props = BTreeMap::from([("hero".to_owned(), tagged_media)]);
    let fmt_cfg = RenderFormattingConfig::default();
    let mut budget = RenderBudget::new();
    let shell = render_block_csec_with_inputs(
        "demo/img",
        valid_tpl,
        BlockRenderInputs {
            hydrated_props: &props,
            local_richtext_nonces: None,
            merged_options: None,
            site_options: None,
            menus: None,
            is_preview: false,
            fmt_cfg: &fmt_cfg,
        },
        &mut budget,
    )
    .unwrap();
    assert!(shell.contains(IMAGE_NONCE_PREFIX), "{shell}");
    assert!(!shell.contains("<picture"), "{shell}");
    let final_html = budget
        .image_nonces
        .substitute_into_document(shell.clone())
        .unwrap();
    assert!(final_html.contains("<picture><source type=\"image/webp\" srcset=\"/media/"));
    assert!(final_html.contains(&format!("src=\"/media/{media_id}/file?v=1\"")));
    assert!(final_html.contains("width=\"1200\" height=\"800\""));
    assert!(final_html.contains("alt=\"Banner Alt\""));
    assert!(final_html.contains("fetchpriority=\"high\""));

    // Duplicate nonce occurrence must be rejected
    let dup_html = format!("{shell}{shell}");
    assert!(
        budget
            .image_nonces
            .substitute_into_document(dup_html)
            .is_err()
    );

    // Stripped / forged media without provenance token fails runtime check even in valid template
    let untagged_media = serde_json::json!({
        "id": media_id,
        "version": 1,
        "kind": "image",
        "url": format!("/media/{media_id}/file?v=1"),
    });
    props.insert("hero".to_owned(), untagged_media);
    let mut forged_budget = RenderBudget::new();
    assert!(
        render_block_csec_with_inputs(
            "demo/img",
            valid_tpl,
            BlockRenderInputs {
                hydrated_props: &props,
                local_richtext_nonces: None,
                merged_options: None,
                site_options: None,
                menus: None,
                is_preview: false,
                fmt_cfg: &fmt_cfg,
            },
            &mut forged_budget,
        )
        .is_err()
    );

    // Ensure MEDIA_PROVENANCE_KEY cannot leak through JSON serialization helper
    let stripped = json_to_safe_value(&serde_json::json!({
        "foo": "bar",
        (MEDIA_PROVENANCE_KEY): media_provenance_token(),
    }));
    assert!(
        stripped
            .get_attr(MEDIA_PROVENANCE_KEY)
            .unwrap()
            .is_undefined()
    );
    let debug_repr = format!("{stripped:?}");
    assert!(!debug_repr.contains(MEDIA_PROVENANCE_KEY), "{debug_repr}");

    for invalid_block in [
        &format!(
            "{{% set fake = {{'id':'{media_id}','kind':'image','url':'/media/{media_id}/file?v=1'}} %}}{{{{ od_image(fake) }}}}"
        ),
        r#"<div>{{ od_image("not-a-media-field") }}</div>"#,
        r#"{% set cap %}{{ od_image(hero) }}{% endset %}<div>ok</div>"#,
        "<div>OD_IMAGE_0123456789abcdef0123456789abcdef</div>",
        r#"<div data-x="{{ od_image(hero) }}">bad</div>"#,
        r#"<div>{{ od_image(hero, "bad;size") }}</div>"#,
        r#"<div>{{ od_image(hero, "100vw", "ok", "alt", "invalid-loading") }}</div>"#,
        r#"<div>{{ od_image(hero, "100vw", "bad\"class") }}</div>"#,
        r#"<div>{{ od_image(hero, "100vw", "ok", "alt", "lazy", "bogus") }}</div>"#,
        r#"<div>{{ od_image(hero, "100vw", "ok", "alt", "lazy", "async", "urgent") }}</div>"#,
    ] {
        assert!(
            validate_block_template_csec("demo/img", invalid_block, &schema).is_err(),
            "{invalid_block}"
        );
    }

    // Theme C-SEC scope checks for starter entry.cover_media & field.media / gallery
    let mut theme_policy = ThemeUrlPolicy::default();
    theme_policy
        .options_tree
        .insert("site_logo".into(), FieldTemplateNode::Media);
    let theme_valid = r#"<main>{% for entry in entries %}{{ od_image(entry.cover_media, "100vw", "card-img", t.img_alt ~ entry.title) }}{% endfor %}{% for field in entry.field_list %}{% if field.media %}{{ od_image(field.media, "100vw", "hero", field.label) }}{% endif %}{% for img in field.gallery %}{{ od_image(img, "50vw", "gal", field.label) }}{% endfor %}{% endfor %}{{ od_image(options.site_logo, "200px", "logo") }}</main>"#;
    let empty_parts = BTreeMap::new();
    assert!(
        validate_template_csec_with_theme_urls("archive", theme_valid, &theme_policy).is_err(),
        "legacy non-composition validator must keep od_image() disabled"
    );
    assert!(
        validate_and_compile_csec_composition_with_theme_urls(
            "archive",
            theme_valid,
            &empty_parts,
            CsecContentKind::NonEntry,
            &theme_policy,
        )
        .is_ok()
    );

    let forged_theme = format!(
        r#"{{% set fake = {{'id':'{media_id}','kind':'image','url':'/media/{media_id}/file?v=1'}} %}}{{{{ od_image(fake) }}}}"#
    );
    assert!(
        validate_and_compile_csec_composition_with_theme_urls(
            "archive",
            &forged_theme,
            &empty_parts,
            CsecContentKind::NonEntry,
            &theme_policy,
        )
        .is_err()
    );
}
