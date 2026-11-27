use opendash_core::content::{CntDeclarations, ContentTypeDefinitionInput};
use serde_json::json;

#[test]
fn custom_type_accepts_explicit_publication_and_visibility_settings() {
    let input: ContentTypeDefinitionInput = serde_json::from_value(json!({
        "key": "press_release",
        "label": "Press release",
        "fields": [],
        "settings": {
            "public": false,
            "has_archive": false,
            "publication": "draft_publish",
            "supports": ["editor"]
        }
    }))
    .expect("new CPT settings must be accepted by the canonical DTO");
    let canonical = input.into_canonical().expect("settings are valid");
    let value = serde_json::to_value(canonical).unwrap();
    assert_eq!(value["settings"]["publication"], "draft_publish");
    assert_eq!(value["settings"]["public"], false);
}

#[test]
fn old_cpt_without_settings_defaults_to_immediate_public_archive() {
    let legacy: ContentTypeDefinitionInput = serde_json::from_value(json!({
        "key": "old_cpt", "label": "Legacy", "fields": []
    }))
    .unwrap();
    let value = serde_json::to_value(legacy.into_canonical().unwrap()).unwrap();
    assert_eq!(value["settings"]["publication"], "immediate");
    assert_eq!(value["settings"]["public"], true);
    assert_eq!(value["settings"]["has_archive"], true);
}

#[test]
fn settings_deny_unknown_keys_and_duplicate_supports() {
    let input = json!({
        "key": "news_c1", "label": "News", "fields": [],
        "settings": {
            "public": true, "has_archive": true, "publication": "draft_publish",
            "supports": ["editor", "featured_image"], "unexpected": "x"
        }
    });
    assert!(serde_json::from_value::<ContentTypeDefinitionInput>(input).is_err());
    let duplicate = serde_json::from_value::<ContentTypeDefinitionInput>(json!({
        "key": "news_c1", "label": "News", "fields": [],
        "settings": {
            "public": true, "has_archive": true, "publication": "draft_publish",
            "supports": ["editor", "editor"]
        }
    }))
    .unwrap();
    assert!(duplicate.into_canonical().is_err());
}

#[test]
fn builtin_status_mode_is_immutable_even_if_requested_on_create() {
    let legacy: ContentTypeDefinitionInput = serde_json::from_value(json!({
        "key": "post", "label": "Bài viết", "fields": []
    }))
    .unwrap();
    let default = legacy.into_canonical().unwrap();
    assert_eq!(
        serde_json::to_value(default).unwrap()["settings"]["publication"],
        "draft_publish"
    );

    let attempt: ContentTypeDefinitionInput = serde_json::from_value(json!({
        "key": "page", "label": "Trang", "fields": [],
        "settings": {
            "public": true, "has_archive": true,
            "publication": "immediate", "supports": []
        }
    }))
    .unwrap();
    assert!(attempt.into_canonical().is_err());
}

#[test]
fn provider_declarations_accept_policy_but_legacy_omission_remains_optional() {
    let manifest: CntDeclarations = serde_json::from_value(json!({
        "content_types": [{
            "key": "news_c1", "label": "News", "fields": [], "taxonomies": [],
            "settings": {"public": false, "has_archive": false,
                         "publication": "draft_publish", "supports": ["featured_image", "editor"]}
        }]
    }))
    .unwrap();
    manifest.validate().unwrap();
    assert_eq!(
        manifest.content_types[0].settings.as_ref().unwrap().public,
        false
    );
    let legacy: CntDeclarations = serde_json::from_value(json!({
        "content_types": [{"key": "legacy_c1", "label": "Legacy"}]
    }))
    .unwrap();
    legacy.validate().unwrap();
    assert!(legacy.content_types[0].settings.is_none());
}
