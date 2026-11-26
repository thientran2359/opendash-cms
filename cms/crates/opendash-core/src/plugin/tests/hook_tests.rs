use super::*;
#[test]
fn subscriptions_are_optional_unique_and_limited_to_implemented_events() {
    let mut value =
        serde_json::json!({"id":"hook-plugin", "name":"Hook", "version":"1.0.0", "api_version":1});
    assert!(
        PluginManifest::from_json_str(&value.to_string())
            .unwrap()
            .hooks
            .is_empty()
    );
    value["hooks"] = serde_json::json!(["page/after_save"]);
    assert_eq!(
        PluginManifest::from_json_str(&value.to_string())
            .unwrap()
            .hooks
            .len(),
        1
    );
    value["hooks"] = serde_json::json!(["content/before_save", "content/after_save"]);
    assert!(PluginManifest::from_json_str(&value.to_string()).is_ok());

    for invalid in [
        serde_json::json!(["page/after_save", "page/after_save"]),
        serde_json::json!(["plugin/manual_run"]),
        serde_json::json!(["page_after_save"]),
    ] {
        value["hooks"] = invalid;
        assert!(PluginManifest::from_json_str(&value.to_string()).is_err());
    }
}

#[test]
fn validates_manifest_schema_v3_features_and_rejects_misuse() {
    let valid_v3 = serde_json::json!({
        "id": "crm-sync",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "min_host_version": "0.1.0",
        "name": "CRM Sync",
        "permissions": {
            "sqlite": ["kv_read", "kv_write"],
            "taxonomy_read": true,
            "media_read": true,
            "site_options_read": true,
            "plugin_settings_read": true,
            "plugin_secrets_read": ["api_key"],
            "content_delete": ["post"]
        },
        "hooks": ["content/after_publish", "settings/after_save"],
        "filters": ["filter:seo/head", "filter:entry/view"],
        "settings_schema": [
            { "key": "api_key", "label": "API Key", "type": "secret", "required": true },
            { "key": "mode", "label": "Mode", "type": "select", "default": "sync", "options": [
                { "value": "sync", "label": "Sync" },
                { "value": "dry", "label": "Dry" }
            ]}
        ],
        "actions": [
            { "id": "purge_post", "required_role": "admin", "capabilities": ["content_delete", "log"] },
            { "id": "list_kv", "required_role": "editor", "capabilities": ["kv_list", "kv_delete"] }
        ]
    });
    let parsed = PluginManifest::from_json_str(&valid_v3.to_string()).expect("valid v3");
    assert!(parsed.can_delete_content_type("post"));
    assert!(!parsed.can_delete_content_type("page"));
    assert_eq!(parsed.filters.len(), 2);

    // Rejects v3 features without manifest_schema >= 3
    let mut missing_schema = valid_v3.clone();
    missing_schema["manifest_schema"] = serde_json::json!(2);
    assert!(PluginManifest::from_json_str(&missing_schema.to_string()).is_err());

    // Rejects v3 without min_host_version
    let mut missing_host = valid_v3.clone();
    missing_host
        .as_object_mut()
        .unwrap()
        .remove("min_host_version");
    assert!(PluginManifest::from_json_str(&missing_host.to_string()).is_err());

    // Rejects plaintext default on Secret field
    let mut secret_default = valid_v3.clone();
    secret_default["settings_schema"][0]["default"] = serde_json::json!("sk_live_oops");
    assert!(PluginManifest::from_json_str(&secret_default.to_string()).is_err());

    // Rejects plugin_secrets_read referencing missing or wildcard key
    let mut bad_secret_perm = valid_v3.clone();
    bad_secret_perm["permissions"]["plugin_secrets_read"] = serde_json::json!(["*"]);
    assert!(PluginManifest::from_json_str(&bad_secret_perm.to_string()).is_err());
    bad_secret_perm["permissions"]["plugin_secrets_read"] = serde_json::json!(["mode"]);
    assert!(PluginManifest::from_json_str(&bad_secret_perm.to_string()).is_err());

    // Rejects filter in hooks or hook in filters
    let mut filter_in_hooks = valid_v3.clone();
    filter_in_hooks["hooks"] = serde_json::json!(["filter:seo/head"]);
    assert!(PluginManifest::from_json_str(&filter_in_hooks.to_string()).is_err());
    let mut hook_in_filters = valid_v3.clone();
    hook_in_filters["filters"] = serde_json::json!(["content/after_publish"]);
    assert!(PluginManifest::from_json_str(&hook_in_filters.to_string()).is_err());

    // Rejects combining content_delete and kv_delete in the same action
    let mut mixed_caps = valid_v3.clone();
    mixed_caps["actions"][0]["capabilities"] = serde_json::json!(["content_delete", "kv_delete"]);
    assert!(PluginManifest::from_json_str(&mixed_caps.to_string()).is_err());
}

#[test]
fn validates_s02_lifecycle_v011_manifest_gates_and_http_exclusion() {
    assert_eq!(CURRENT_HOST_VERSION, "0.1.1");
    assert!(!host_satisfies_min_version("0.1.0", "0.1.1"));
    assert!(host_satisfies_min_version("0.1.1", "0.1.1"));
    assert!(host_satisfies_min_version("0.1.1", "0.1.0"));

    let valid_s02 = serde_json::json!({
        "id": "lifecycle-tool",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "min_host_version": "0.1.1",
        "name": "Lifecycle Tool",
        "permissions": {
            "content_trash_read": ["post"],
            "content_restore": ["post"],
            "content_purge": ["post"],
            "http": ["https://example.com"]
        },
        "actions": [
            { "id": "list_trash", "required_role": "editor", "capabilities": ["content_trash_read", "http"] },
            { "id": "restore_post", "required_role": "editor", "capabilities": ["content_trash_read", "content_restore"] },
            { "id": "purge_post", "required_role": "admin", "capabilities": ["content_trash_read", "content_purge"] }
        ]
    });
    let parsed = PluginManifest::from_json_str(&valid_s02.to_string()).expect("valid s02");
    assert!(parsed.can_read_trash_content_type("post"));
    assert!(parsed.can_restore_content_type("post"));
    assert!(parsed.can_purge_content_type("post"));
    assert!(!parsed.can_purge_content_type("page"));

    // Rejects min_host_version 0.1.0 when new lifecycle permissions/capabilities are declared.
    let mut lower_host = valid_s02.clone();
    lower_host["min_host_version"] = serde_json::json!("0.1.0");
    assert!(PluginManifest::from_json_str(&lower_host.to_string()).is_err());

    // Rejects missing min_host_version.
    let mut missing_host = valid_s02.clone();
    missing_host
        .as_object_mut()
        .unwrap()
        .remove("min_host_version");
    assert!(PluginManifest::from_json_str(&missing_host.to_string()).is_err());

    // Rejects unsupported future min_host_version.
    let mut future_host = valid_s02.clone();
    future_host["min_host_version"] = serde_json::json!("0.2.0");
    assert!(PluginManifest::from_json_str(&future_host.to_string()).is_err());

    // Rejects schema 2 with new lifecycle grant.
    let mut bad_schema = valid_s02.clone();
    bad_schema["manifest_schema"] = serde_json::json!(2);
    assert!(PluginManifest::from_json_str(&bad_schema.to_string()).is_err());

    // Rejects http combined with content_restore or content_purge.
    let mut http_restore = valid_s02.clone();
    http_restore["actions"][1]["capabilities"] = serde_json::json!(["content_restore", "http"]);
    assert!(PluginManifest::from_json_str(&http_restore.to_string()).is_err());

    let mut http_purge = valid_s02.clone();
    http_purge["actions"][2]["capabilities"] = serde_json::json!(["content_purge", "http"]);
    assert!(PluginManifest::from_json_str(&http_purge.to_string()).is_err());

    // Rejects mixing write scopes (content_restore + content_purge or content_delete + content_restore).
    let mut mixed_write = valid_s02.clone();
    mixed_write["actions"][1]["capabilities"] =
        serde_json::json!(["content_restore", "content_purge"]);
    assert!(PluginManifest::from_json_str(&mixed_write.to_string()).is_err());
}
