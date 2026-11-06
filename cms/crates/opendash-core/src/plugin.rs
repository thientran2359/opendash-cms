use serde::{Deserialize, Serialize};

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

pub fn validate_semver(version: &str) -> bool {
    if version.is_empty() || version.len() > 64 {
        return false;
    }
    let mut parts = version.splitn(2, '+');
    let main_and_pre = parts.next().unwrap_or("");
    if let Some(build) = parts.next() {
        if build.is_empty()
            || !build
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        {
            return false;
        }
    }
    let mut pre_parts = main_and_pre.splitn(2, '-');
    let numbers_part = pre_parts.next().unwrap_or("");
    if let Some(pre) = pre_parts.next() {
        if pre.is_empty()
            || !pre
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        {
            return false;
        }
    }
    let nums: Vec<&str> = numbers_part.split('.').collect();
    if nums.len() != 3 {
        return false;
    }
    for num in nums {
        if num.is_empty() || (num.len() > 1 && num.starts_with('0')) {
            return false;
        }
        if !num.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
        if num.parse::<u64>().is_err() {
            return false;
        }
    }
    true
}

pub const PLUGIN_ABI_VERSION: u32 = 1;

fn validate_plugin_id(id: &str) -> Result<(), &'static str> {
    if id.len() < 3 || id.len() > 64 {
        return Err("ID plugin phải từ 3 đến 64 ký tự.");
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("ID plugin chỉ được chứa chữ thường, số và dấu gạch ngang.");
    }
    if id.starts_with('-') || id.ends_with('-') {
        return Err("ID plugin không được bắt đầu hoặc kết thúc bằng dấu gạch ngang.");
    }
    Ok(())
}

pub fn valid_plugin_id(value: &str) -> bool {
    validate_plugin_id(value).is_ok()
}

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
