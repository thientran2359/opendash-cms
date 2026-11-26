use super::*;

#[test]
fn preview_only_inserts_trusted_parts_in_executed_body_sites() {
    let empty = parts(&[]);
    let trusted = parts(&[("card", "<form><input disabled></form>")]);
    let source = "<main>{{ client_text }}{% if show %}{{ od_part('card') }}{% endif %}</main>";
    let context = minijinja::context!(
        client_text => "<form>OD_PREVIEW_PART_fake</form>",
        show => true
    );
    let html = render_preview_csec_fragment(
        "root",
        source,
        context,
        &empty,
        &trusted,
        &mut PreviewCompositionBudget::new(),
    )
    .unwrap();
    assert!(
        html.contains("&lt;form&gt;OD_PREVIEW_PART_fake&lt;&#x2f;form&gt;")
            || html.contains("&lt;form&gt;OD_PREVIEW_PART_fake&lt;/form&gt;")
    );
    assert!(html.contains("<form><input disabled></form>"));
    let hidden = render_preview_csec_fragment(
        "root",
        source,
        minijinja::context!(client_text => "", show => false),
        &empty,
        &trusted,
        &mut PreviewCompositionBudget::new(),
    )
    .unwrap();
    assert_eq!(hidden, "<main></main>");
    for bad in [
        "<div data-x=\"{{ od_part('card') }}\"></div>",
        "<style>.x{color:{{ od_part('card') }}}</style>",
        "<form>{{ od_part('card') }}</form>",
        "<script>{{ od_part('card') }}</script>",
        "OD_PREVIEW_PART_abcdef",
        "{{ od_content() }}",
    ] {
        assert!(preview_csec_part_keys("root", bad).is_err(), "{bad}");
        assert!(
            render_preview_csec_fragment(
                "root",
                bad,
                Value::UNDEFINED,
                &empty,
                &trusted,
                &mut PreviewCompositionBudget::new(),
            )
            .is_err(),
            "{bad}"
        );
    }
}

#[test]
fn preview_nonce_replacement_is_single_pass_and_checks_multiplicity_context_and_size() {
    let nonce_table = BTreeMap::from([
        ("marker-first".to_owned(), "<b>marker-second</b>"),
        ("marker-second".to_owned(), "<i>second</i>"),
    ]);
    assert_eq!(
        insert_preview_fragments("marker-first marker-second", &nonce_table).unwrap(),
        "<b>marker-second</b> <i>second</i>"
    );
    assert!(insert_preview_fragments("marker-first marker-first", &nonce_table).is_err());
    assert!(insert_preview_fragments("<div id=\"marker-first\"></div>", &nonce_table).is_err());
    let empty = parts(&[]);
    let large = "x".repeat(MAX_RENDERED_HTML_BYTES / 2 + 1);
    let trusted = parts(&[("card", &large)]);
    assert!(
        render_preview_csec_fragment(
            "root",
            "{{ od_part('card') }}{{ od_part('card') }}",
            Value::UNDEFINED,
            &empty,
            &trusted,
            &mut PreviewCompositionBudget::new(),
        )
        .is_err()
    );
}

#[test]
fn block_csec_blocks_html5_comment_smuggling_duplicate_attrs_entity_urls_and_richtext_aliasing() {
    use opendash_core::block_schema::BlockSchemaV1;
    use serde_json::json;

    let schema: BlockSchemaV1 = serde_json::from_value(json!({
        "schema_version": 1,
        "id": "core/test-card",
        "title": "Test Card",
        "category": "content",
        "render": "render.html",
        "fields": [
            { "type": "text", "key": "title", "label": "Title" },
            { "type": "richtext", "key": "body", "label": "Body" },
            { "type": "url", "key": "cta_url", "label": "CTA URL" },
            { "type": "media", "key": "hero", "label": "Hero", "media_kind": "image" },
            {
                "type": "repeater",
                "key": "items",
                "label": "Items",
                "fields": [
                    { "type": "text", "key": "label", "label": "Label" },
                    { "type": "richtext", "key": "content", "label": "Content" },
                    { "type": "url", "key": "link_url", "label": "Link" },
                    { "type": "media", "key": "thumb", "label": "Thumb", "media_kind": "image" }
                ]
            }
        ]
    }))
    .unwrap();

    let bad_templates = [
        "<!-- --!><iframe src='/x'></iframe><!-- -->",
        "<!--><iframe src='/x'></iframe><!-- -->",
        "<!---><iframe src='/x'></iframe><!-- -->",
        "<a href=\"javascript:alert(1)\" href=\"https://ok.example\">x</a>",
        "<a href=\"/&sol;evil.com\">x</a>",
        "<a href=\"java&#x09;script:alert(1)\">x</a>",
        "<img src=\"/media/ok.png\" srcset=\"/&sol;evil.com/a.png 1x\">",
        "<a href=\"https://ok.example\" ping=\"/&sol;evil.com/track\">x</a>",
        "<title>bad</title>",
        "<style>body{}</style>",
        "<!DOCTYPE html><div>bad</div>",
        "<xmp>bad</xmp>",
        "<noembed>bad</noembed>",
        "<noframes>bad</noframes>",
        "<plaintext>bad",
        "{% set rt = body %}<div>{{ rt }}</div>",
        "{% with rt = body %}<div>{{ rt }}</div>{% endwith %}",
        "<div data-x=\"{{ body }}\"></div>",
        "<div>{{ body | upper }}</div>",
        "{% for item in items %}<div title=\"{{ item.content }}\"></div>{% endfor %}",
        "{% for item in items %}{% set x = item.content %}{{ x }}{% endfor %}",
        "<div>{{ items[0].label }}</div>",
        "<div>{{ hero }}</div>",
        "{% for item in items %}<div>{{ item.thumb }}</div>{% endfor %}",
        "<a href=\"{{ title }}\">bad non-url field in href</a>",
        "<img src=\"{{ cta_url }}\">",
        "<div>OD_RICHTEXT_0123456789abcdef0123456789abcdef</div>",
    ];

    for bad in bad_templates {
        assert!(
            validate_block_template_csec("core/test-card", bad, &schema).is_err(),
            "expected C-SEC reject for: {bad}"
        );
    }

    // Valid template with V1 object paths (`hero.url`, `hero.alt`, `item.thumb.url`), richtext, and url
    let good_tpl = r#"<section class="card">
        <h2>{{ title }}</h2>
        {% if hero and hero.url %}<img src="{{ hero.url }}" alt="{{ hero.alt }}">{% endif %}
        {% if body %}<div class="body">{{ body }}</div>{% endif %}
        {% if cta_url %}<a href="{{ cta_url }}">Go</a>{% endif %}
        {% for item in items %}
            <div class="item">
                <span>{{ item.label }}</span>
                {% if item.thumb and item.thumb.url %}<img src="{{ item.thumb.url }}" alt="">{% endif %}
                {% if item.content %}<div>{{ item.content }}</div>{% endif %}
                {% if item.link_url %}<a href="{{ item.link_url }}">More</a>{% endif %}
            </div>
        {% endfor %}
    </section>"#;
    validate_block_template_csec("core/test-card", good_tpl, &schema).unwrap();

    let media_id = "a".repeat(64);
    let mut local_rt = RichTextNonceTable::default();
    let clean_body = opendash_core::richtext::sanitize_richtext(
        "<p>Rich <strong>text</strong> <script>evil()</script><a href=\"https://opendash.vn\">link</a></p>",
    )
    .unwrap();
    let body_nonce = local_rt.register_sanitized(clean_body).unwrap();
    let clean_item =
        opendash_core::richtext::sanitize_richtext("<p>Nội dung <em>mục 1</em></p>").unwrap();
    let item_nonce = local_rt.register_sanitized(clean_item).unwrap();

    let hydrated_props = BTreeMap::from([
        ("title".into(), json!("Chào <script>alert(1)</script>")),
        ("body".into(), json!(body_nonce)),
        ("cta_url".into(), json!("/lien-he")),
        (
            "hero".into(),
            json!({
                "id": media_id,
                "url": format!("/media/{media_id}/file"),
                "thumbnail_url": format!("/media/{media_id}/thumbnail"),
                "alt": "",
                "width": 800,
                "height": 600,
                "kind": "image"
            }),
        ),
        (
            "items".into(),
            json!([{
                "label": "Mục 1",
                "content": item_nonce,
                "link_url": "#top",
                "thumb": {
                    "id": media_id,
                    "url": format!("/media/{media_id}/file"),
                    "thumbnail_url": format!("/media/{media_id}/thumbnail"),
                    "alt": "",
                    "width": 800,
                    "height": 600,
                    "kind": "image"
                }
            }]),
        ),
    ]);

    let fmt_cfg = RenderFormattingConfig::default();
    let mut budget = RenderBudget::new();
    let rendered = render_block_csec_with_inputs(
        "core/test-card",
        good_tpl,
        BlockRenderInputs {
            hydrated_props: &hydrated_props,
            local_richtext_nonces: Some(local_rt),
            merged_options: None,
            site_options: None,
            menus: None,
            is_preview: false,
            fmt_cfg: &fmt_cfg,
        },
        &mut budget,
    )
    .unwrap();
    let decoded_rendered = opendash_core::richtext::decode_html5_entities(&rendered);
    assert!(rendered.contains("Chào &lt;script&gt;alert(1)&lt;"));
    assert!(rendered.contains("<p>Rich <strong>text</strong> <a href=\"https://opendash.vn\" rel=\"noopener noreferrer\">link</a></p>"));
    assert!(rendered.contains("<p>Nội dung <em>mục 1</em></p>"));
    assert!(decoded_rendered.contains(&format!("src=\"/media/{media_id}/file\"")));
    assert!(!rendered.contains("OD_RICHTEXT_"));
    assert_eq!(budget.richtext_nonces.total_count(), 2);
    assert!(budget.fuel_remaining < TEMPLATE_FUEL_LIMIT);
    assert!(budget.html_bytes_used > 0);

    // Preview mode uses /api/admin/media/{id}/file and validate_safe_scoped_block_css gates it on is_preview
    let preview_css = format!(
        "[data-od-node-scope=\"entry-b1\"]{{--od-bg:url(\"/api/admin/media/{media_id}/file\");}}"
    );
    validate_safe_scoped_block_css(&preview_css, true).unwrap();
    assert!(validate_safe_scoped_block_css(&preview_css, false).is_err());
}

#[test]
fn block_csec_supports_nested_group_icon_link_spacing_and_dynamic_menus() {
    use opendash_core::block_schema::BlockSchemaV1;
    use serde_json::json;

    let schema: BlockSchemaV1 = serde_json::from_value(json!({
        "schema_version": 1,
        "id": "core/showcase-test",
        "title": "Showcase",
        "category": "content",
        "render": "render.html",
        "fields": [
            {
                "type": "spacing",
                "key": "outer_margin",
                "label": "Outer margin",
                "property": "margin",
                "unit": "px",
                "responsive": true
            },
            {
                "type": "group",
                "key": "hero_card",
                "label": "Hero",
                "fields": [
                    { "type": "icon", "key": "badge_icon", "label": "Badge" },
                    { "type": "text", "key": "eyebrow", "label": "Eyebrow" },
                    { "type": "link", "key": "primary_link", "label": "CTA" }
                ]
            },
            {
                "type": "select",
                "key": "linked_menu",
                "label": "Menu",
                "options_source": "menus",
                "allow_null": true
            }
        ]
    }))
    .unwrap();

    let tpl = r#"<section class="sc {{ outer_margin.classes }}" data-od-spacing="outer_margin">
        {% if hero_card.badge_icon %}{{ od_icon(hero_card.badge_icon.name, 'h-4 w-4') }}{% endif %}
        <span>{{ hero_card.eyebrow }}</span>
        {% if hero_card.primary_link %}<a href="{{ hero_card.primary_link.url }}" target="{{ hero_card.primary_link.target }}" rel="{{ hero_card.primary_link.rel }}">{{ hero_card.primary_link.label }}</a>{% endif %}
        {% if linked_menu and menus[linked_menu] is defined and menus[linked_menu] %}
          <nav>{% for item in menus[linked_menu].items %}<a href="{{ item.url }}">{{ item.label }}</a>{% endfor %}</nav>
        {% endif %}
    </section>"#;
    validate_block_template_csec("core/showcase-test", tpl, &schema).unwrap();

    for bad in [
        r#"{{ hero_card }}"#,
        r#"{{ od_icon(hero_card.eyebrow) }}"#,
        r#"<a href="{{ hero_card.primary_link.label }}">bad</a>"#,
        r#"<div>{{ menus[linked_menu] }}</div>"#,
    ] {
        assert!(
            validate_block_template_csec("core/showcase-test", bad, &schema).is_err(),
            "{bad}"
        );
    }

    let hydrated_props = BTreeMap::from([
        (
            "outer_margin".into(),
            json!({ "top": -16, "bottom": 24, "css": "-16px 0px 24px 0px", "classes": "-mt-4 mb-6" }),
        ),
        (
            "hero_card".into(),
            json!({
                "badge_icon": { "name": "sparkles" },
                "eyebrow": "Mới",
                "primary_link": { "url": "/docs", "label": "Tài liệu", "target": "_blank", "rel": "noopener noreferrer", "new_tab": true, "nofollow": false }
            }),
        ),
        ("linked_menu".into(), json!("main-nav")),
    ]);
    let menus = BTreeMap::from([(
        "main-nav".into(),
        json!({
            "menu_id": 1,
            "menu_key": "main-nav",
            "title": "Main",
            "items": [{ "id": 1, "label": "Trang chủ", "url": "/", "is_active": true, "is_ancestor": false, "is_external": false, "depth": 0, "children": [] }],
            "diagnostics": []
        }),
    )]);
    let fmt_cfg = RenderFormattingConfig::default();
    let mut budget = RenderBudget::new();
    let shell = render_block_csec_with_inputs(
        "core/showcase-test",
        tpl,
        BlockRenderInputs {
            hydrated_props: &hydrated_props,
            local_richtext_nonces: None,
            merged_options: None,
            site_options: None,
            menus: Some(&menus),
            is_preview: false,
            fmt_cfg: &fmt_cfg,
        },
        &mut budget,
    )
    .unwrap();
    let html = budget.icon_nonces.substitute_into_document(shell).unwrap();
    assert!(html.contains("-mt-4 mb-6"));
    assert!(html.contains("<svg "));
    assert!(html.contains("Tài liệu"));
    assert!(html.contains("Trang chủ"));
}
