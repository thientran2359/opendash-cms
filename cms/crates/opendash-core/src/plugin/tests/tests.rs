use super::*;

#[test]
fn test_manifest_and_semver_validation() {
    let valid_json = r#"{
        "id": "my-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "name": "My Plugin",
        "description": "Test plugin",
        "entrypoint": "plugin.wasm",
        "permissions": {
            "sqlite": ["kv_read", "kv_write"],
            "http": ["http://localhost:8080/test"]
        }
    }"#;
    let manifest = PluginManifest::from_json_str(valid_json).expect("Manifest hợp lệ");
    assert_eq!(manifest.id, "my-plugin");
    assert!(manifest.has_sqlite_perm("kv_read"));
    assert!(manifest.is_http_allowed("http://localhost:8080/test/subpath"));
    assert!(!manifest.is_http_allowed("http://attacker.com/steal"));
    assert!(validate_semver("1.0.0-alpha.1+build.123"));
    assert!(!validate_semver("v1.0.0"));
}

#[test]
fn validates_scoped_field_grants_v2_and_rejects_v1_or_wildcards() {
    let v1_with_private = serde_json::json!({
        "id": "priv-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 1,
        "name": "Priv",
        "permissions": {
            "content_read": ["portfolio"],
            "content_read_private": [{ "content_type": "portfolio", "field_keys": ["secret"] }]
        }
    });
    assert!(PluginManifest::from_json_str(&v1_with_private.to_string()).is_err());

    let v2_wildcard = serde_json::json!({
        "id": "priv-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "min_host_version": "0.1.0",
        "name": "Priv",
        "permissions": {
            "content_read": ["*"],
            "content_read_private": [{ "content_type": "*", "field_keys": ["secret"] }]
        }
    });
    assert!(PluginManifest::from_json_str(&v2_wildcard.to_string()).is_err());

    let v2_valid = serde_json::json!({
        "id": "priv-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "min_host_version": "0.1.0",
        "name": "Priv",
        "permissions": {
            "content_read": ["portfolio"],
            "content_read_private": [{ "content_type": "portfolio", "field_keys": ["secret"] }]
        },
        "actions": [{
            "id": "sync",
            "title": "Sync",
            "required_role": "editor",
            "capabilities": ["content_read", "content_read_private"]
        }]
    });
    assert!(PluginManifest::from_json_str(&v2_valid.to_string()).is_ok());
}

#[test]
fn validates_editor_panels_in_v2_manifest() {
    let valid = serde_json::json!({
        "id": "seo-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "SEO Plugin",
        "actions": [{
            "id": "analyze_seo",
            "title": "Analyze SEO",
            "required_role": "admin",
            "capabilities": ["kv_read"]
        }],
        "editor_panels": [{
            "local_key": "seo_action",
            "title": "SEO Check",
            "action_id": "analyze_seo",
            "button_label": "Run SEO Check",
            "applies_to": ["project"],
            "min_edit_role": "admin"
        }]
    });
    PluginManifest::from_json_str(&valid.to_string()).expect("valid editor_panels");

    // Rejects min_edit_role < action.required_role
    let bad_acl = serde_json::json!({
        "id": "seo-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "SEO Plugin",
        "actions": [{
            "id": "analyze_seo",
            "title": "Analyze SEO",
            "required_role": "admin",
            "capabilities": ["kv_read"]
        }],
        "editor_panels": [{
            "local_key": "seo_action",
            "title": "SEO Check",
            "action_id": "analyze_seo",
            "button_label": "Run SEO Check",
            "applies_to": ["project"],
            "min_edit_role": "editor"
        }]
    });
    assert!(PluginManifest::from_json_str(&bad_acl.to_string()).is_err());

    // Rejects missing action_id
    let missing_action = serde_json::json!({
        "id": "seo-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "SEO Plugin",
        "editor_panels": [{
            "local_key": "seo_action",
            "title": "SEO Check",
            "action_id": "does_not_exist",
            "button_label": "Run",
            "applies_to": ["project"]
        }]
    });
    assert!(PluginManifest::from_json_str(&missing_action.to_string()).is_err());

    // Rejects v1 with editor_panels
    let v1_panels = serde_json::json!({
        "id": "seo-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 1,
        "name": "SEO Plugin",
        "actions": [{
            "id": "analyze_seo",
            "title": "Analyze SEO",
            "required_role": "editor",
            "capabilities": ["kv_read"]
        }],
        "editor_panels": [{
            "local_key": "seo_action",
            "title": "SEO Check",
            "action_id": "analyze_seo",
            "button_label": "Run",
            "applies_to": ["project"]
        }]
    });
    assert!(PluginManifest::from_json_str(&v1_panels.to_string()).is_err());
}
