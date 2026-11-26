use super::*;

#[test]
fn pinned_schema_enforces_url_and_image_reference_separation() {
    let schema = settings::parse_theme_options_schema(
        &serde_json::json!({
            "options": [
                {"key":"cta_url", "label":"CTA", "type":"url", "default":"/contact", "public":true},
                {"key":"hero_image", "label":"Hero", "type":"media", "default":null, "public":true}
            ]
        })
        .to_string(),
    )
    .unwrap();
    let sample = |setting: serde_json::Value| -> ThemeSampleManifest {
        serde_json::from_value(serde_json::json!({
            "media":[{"object_key":"hero","path":"assets/hero.png"}],
            "settings":[setting]
        }))
        .unwrap()
    };
    assert!(
        validate_sample_option_values(
            &sample(serde_json::json!({
                "object_key":"image", "option_key":"hero_image", "media_ref":"hero", "value":null
            })),
            &schema
        )
        .is_ok()
    );
    assert!(
        validate_sample_option_values(
            &sample(serde_json::json!({
                "object_key":"url", "option_key":"cta_url", "value":"/contact?from=sample"
            })),
            &schema
        )
        .is_ok()
    );
    for invalid in [
        serde_json::json!({"object_key":"bad", "option_key":"cta_url", "value":"javascript:alert(1)"}),
        serde_json::json!({"object_key":"bad", "option_key":"cta_url", "value":null, "media_ref":"hero"}),
        serde_json::json!({"object_key":"bad", "option_key":"hero_image", "value":"/foo", "media_ref":"hero"}),
        serde_json::json!({"object_key":"bad", "option_key":"hero_image", "value":null, "media_ref":"missing"}),
        serde_json::json!({"object_key":"bad", "option_key":"undefined", "value":"/contact"}),
    ] {
        assert!(
            validate_sample_option_values(&sample(invalid.clone()), &schema).is_err(),
            "{invalid}"
        );
    }
}
