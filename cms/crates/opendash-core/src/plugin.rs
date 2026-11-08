use serde::{Deserialize, Serialize};

pub use crate::id::{
    valid_package_id as valid_plugin_id, validate_package_id as validate_plugin_id, validate_semver,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PluginPermissions {
    #[serde(default)]
    pub sqlite: Vec<String>,
    #[serde(default)]
    pub http: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub id: String,
    pub version: String,
    pub api_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_entrypoint")]
    pub entrypoint: String,
    #[serde(default)]
    pub permissions: PluginPermissions,
    /// Optional lifecycle subscriptions; manual invocation input remains unchanged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<crate::hooks::HookEvent>,
}

fn default_entrypoint() -> String {
    "plugin.wasm".into()
}

impl PluginManifest {
    pub fn from_json_str(json_str: &str) -> Result<Self, &'static str> {
        let manifest: Self =
            serde_json::from_str(json_str).map_err(|_| "JSON manifest không hợp lệ.")?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.api_version != PLUGIN_ABI_VERSION {
            return Err("Chỉ hỗ trợ api_version 1.");
        }
        if !validate_semver(&self.version) {
            return Err("Phiên bản plugin không đúng định dạng SemVer 2.0.0.");
        }
        validate_plugin_id(&self.id)?;
        if self.name.trim().is_empty() || self.name.chars().count() > 100 {
            return Err("Tên plugin không hợp lệ.");
        }
        if self.description.chars().count() > 500 {
            return Err("Mô tả plugin quá dài.");
        }
        if self.entrypoint != "plugin.wasm" {
            return Err("Entrypoint phải là 'plugin.wasm'.");
        }
        if self.hooks.len() > 1
            || self
                .hooks
                .iter()
                .any(|event| *event != crate::hooks::HookEvent::PageAfterSave)
        {
            return Err("Only one page/after_save subscription is supported.");
        }
        for perm in &self.permissions.sqlite {
            if perm != "kv_read" && perm != "kv_write" {
                return Err("Quyền SQLite không hợp lệ (chỉ hỗ trợ 'kv_read' và 'kv_write').");
            }
        }
        for url_str in &self.permissions.http {
            let parsed =
                url::Url::parse(url_str).map_err(|_| "URL trong quyền HTTP không hợp lệ.")?;
            if parsed.scheme() != "http" && parsed.scheme() != "https" {
                return Err("Chỉ hỗ trợ scheme http hoặc https.");
            }
        }
        Ok(())
    }

    pub fn has_sqlite_perm(&self, perm: &str) -> bool {
        self.permissions.sqlite.iter().any(|p| p == perm)
    }

    pub fn is_http_allowed(&self, target_url: &str) -> bool {
        let Ok(target) = url::Url::parse(target_url) else {
            return false;
        };
        for allowed in &self.permissions.http {
            if let Ok(allowed_parsed) = url::Url::parse(allowed) {
                if target.scheme() == allowed_parsed.scheme()
                    && target.host_str() == allowed_parsed.host_str()
                    && target.port_or_known_default() == allowed_parsed.port_or_known_default()
                {
                    return true;
                }
            }
        }
        false
    }
}

pub const PLUGIN_ABI_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
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
}

#[cfg(test)]
mod scaffold_validation_tests {
    use super::{PLUGIN_ABI_VERSION, valid_plugin_id};

    #[test]
    fn ids_follow_abi_rules() {
        assert_eq!(PLUGIN_ABI_VERSION, 1);
        assert!(valid_plugin_id("sample-plugin"));
        assert!(!valid_plugin_id("a"));
        assert!(!valid_plugin_id("-bad"));
        assert!(!valid_plugin_id("bad-"));
        assert!(!valid_plugin_id("Invalid_Plugin"));
    }
}

#[cfg(test)]
mod hook_tests {
    use super::*;
    #[test]
    fn subscriptions_are_optional_unique_and_limited_to_implemented_events() {
        let mut value = serde_json::json!({"id":"hook-plugin", "name":"Hook", "version":"1.0.0", "api_version":1});
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
        for invalid in [
            serde_json::json!(["page/after_save", "page/after_save"]),
            serde_json::json!(["content/before_save"]),
            serde_json::json!(["page_after_save"]),
        ] {
            value["hooks"] = invalid;
            assert!(PluginManifest::from_json_str(&value.to_string()).is_err());
        }
    }
}
