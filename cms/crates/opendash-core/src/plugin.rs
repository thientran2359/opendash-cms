use std::collections::HashSet;

use serde::{Deserialize, Serialize};

pub use crate::id::{
    valid_package_id as valid_plugin_id, validate_package_id as validate_plugin_id, validate_semver,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NetworkScopeGrant {
    #[serde(default = "default_https_scheme")]
    pub scheme: String,
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default = "default_network_methods")]
    pub methods: Vec<String>,
    #[serde(default)]
    pub path_prefix: Option<String>,
}

impl NetworkScopeGrant {
    pub fn effective_port(&self) -> Option<u16> {
        if let Some(p) = self.port {
            return Some(p);
        }
        match self.scheme.to_ascii_lowercase().as_str() {
            "https" => Some(443),
            "http" => Some(80),
            _ => None,
        }
    }
}

fn default_https_scheme() -> String {
    "https".into()
}

fn default_network_methods() -> Vec<String> {
    vec!["GET".into()]
}

/// Narrowly scoped permission grant for reading or writing `public_exposure = Private` fields
/// owned by another entity (`270-R3`). Wildcard `"*"` is forbidden for both `content_type` and `field_keys`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScopedFieldGrant {
    pub content_type: String,
    pub field_keys: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PluginPermissions {
    #[serde(default)]
    pub sqlite: Vec<String>,
    #[serde(default)]
    pub http: Vec<String>,
    #[serde(default)]
    pub network: Vec<NetworkScopeGrant>,
    #[serde(default)]
    pub content_read: Vec<String>,
    #[serde(default)]
    pub content_write: Vec<String>,
    #[serde(default)]
    pub content_read_private: Vec<ScopedFieldGrant>,
    #[serde(default)]
    pub content_write_private: Vec<ScopedFieldGrant>,
}

fn default_action_role() -> String {
    "admin".into()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PluginActionDeclaration {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default = "default_action_role")]
    pub required_role: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub id: String,
    pub version: String,
    pub api_version: u32,
    #[serde(default = "default_manifest_schema")]
    pub manifest_schema: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_host_version: Option<String>,
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
    #[serde(default)]
    pub hook_priority: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<PluginActionDeclaration>,
    #[serde(default, skip_serializing_if = "crate::content::CntDeclarations::is_empty")]
    pub declarations: crate::content::CntDeclarations,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub editor_panels: Vec<crate::content::ManifestActionPanelInput>,
}

pub const CURRENT_HOST_VERSION: &str = env!("CARGO_PKG_VERSION");

fn default_manifest_schema() -> u32 {
    1
}

fn parse_core_semver(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

pub fn host_satisfies_min_version(host: &str, min: &str) -> bool {
    match (parse_core_semver(host), parse_core_semver(min)) {
        (Some(h), Some(m)) => h >= m,
        _ => false,
    }
}

fn default_entrypoint() -> String {
    "plugin.wasm".into()
}

fn validate_scoped_field_grants<F>(
    grants: &[ScopedFieldGrant],
    has_base_type_perm: F,
    label: &str,
) -> Result<(), &'static str>
where
    F: Fn(&str) -> bool,
{
    for grant in grants {
        if grant.content_type == "*" || !crate::content::valid_content_type_key(&grant.content_type)
        {
            return Err(match label {
                "read" => "content_read_private không cho phép wildcard '*' và phải dùng content_type hợp lệ.",
                _ => "content_write_private không cho phép wildcard '*' và phải dùng content_type hợp lệ.",
            });
        }
        if !has_base_type_perm(&grant.content_type) {
            return Err(match label {
                "read" => "content_read_private yêu cầu plugin đã có quyền content_read cho loại nội dung tương ứng.",
                _ => "content_write_private yêu cầu plugin đã có quyền content_write cho loại nội dung tương ứng.",
            });
        }
        if grant.field_keys.is_empty() || grant.field_keys.len() > 64 {
            return Err("ScopedFieldGrant.field_keys phải có từ 1 đến 64 khóa trường.");
        }
        let mut seen = HashSet::new();
        for fk in &grant.field_keys {
            if fk == "*" || !crate::id::valid_snake_key(fk) || !seen.insert(fk.as_str()) {
                return Err(
                    "ScopedFieldGrant.field_keys không cho phép wildcard '*', phải là snake_case hợp lệ và không trùng lặp.",
                );
            }
        }
    }
    Ok(())
}

impl PluginManifest {
    pub fn from_json_str(json_str: &str) -> Result<Self, &'static str> {
        let mut raw_val: serde_json::Value =
            serde_json::from_str(json_str).map_err(|_| "JSON manifest không hợp lệ.")?;
        if let Some(obj) = raw_val.as_object_mut() {
            if let Some(top_fg) = obj.remove("field_groups") {
                let decl = obj
                    .entry("declarations")
                    .or_insert_with(|| serde_json::json!({}));
                let Some(decl_obj) = decl.as_object_mut() else {
                    return Err("Trường declarations trong plugin.json không hợp lệ.");
                };
                if decl_obj.contains_key("field_groups") {
                    return Err(
                        "Không được khai báo trùng field_groups ở cả cấp gốc và trong declarations.",
                    );
                }
                decl_obj.insert("field_groups".into(), top_fg);
            }
        }
        let manifest: Self =
            serde_json::from_value(raw_val).map_err(|_| "JSON manifest không hợp lệ.")?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.api_version != PLUGIN_ABI_VERSION {
            return Err("Chỉ hỗ trợ api_version 1.");
        }
        if self.manifest_schema != 1 && self.manifest_schema != 2 {
            return Err("manifest_schema không được hỗ trợ (chỉ hỗ trợ 1 hoặc 2).");
        }
        if !self.declarations.field_groups.is_empty() && self.manifest_schema != 2 {
            return Err("field_groups trong plugin.json chỉ hợp lệ khi manifest_schema = 2.");
        }
        if !self.editor_panels.is_empty() && self.manifest_schema != 2 {
            return Err("editor_panels trong plugin.json chỉ hợp lệ khi manifest_schema = 2.");
        }
        if self.editor_panels.len() > 16 {
            return Err("plugin.json không được khai báo quá 16 editor_panels.");
        }
        if (!self.permissions.content_read_private.is_empty()
            || !self.permissions.content_write_private.is_empty())
            && (self.manifest_schema != 2 || self.min_host_version.is_none())
        {
            return Err(
                "content_read_private và content_write_private chỉ hợp lệ khi manifest_schema = 2 và có khai báo min_host_version.",
            );
        }
        if let Some(min_ver) = &self.min_host_version {
            if !validate_semver(min_ver) {
                return Err("min_host_version không đúng định dạng SemVer 2.0.0.");
            }
            if !host_satisfies_min_version(CURRENT_HOST_VERSION, min_ver) {
                return Err("Phiên bản host hiện tại thấp hơn min_host_version yêu cầu.");
            }
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
        let mut seen_hooks = HashSet::new();
        for event in &self.hooks {
            if !matches!(
                event,
                crate::hooks::HookEvent::PageAfterSave
                    | crate::hooks::HookEvent::ContentBeforeSave
                    | crate::hooks::HookEvent::ContentAfterSave
            ) {
                return Err("Sự kiện hook không được hỗ trợ để đăng ký.");
            }
            if !seen_hooks.insert(event.as_str()) {
                return Err("Danh sách hooks không được chứa sự kiện trùng lặp.");
            }
        }
        for ct_perm in &self.permissions.content_read {
            if ct_perm != "*" && !crate::id::valid_snake_key(ct_perm) {
                return Err("Quyền content_read không hợp lệ.");
            }
        }
        for ct_perm in &self.permissions.content_write {
            if ct_perm != "*" && !crate::id::valid_snake_key(ct_perm) {
                return Err("Quyền content_write không hợp lệ.");
            }
        }
        validate_scoped_field_grants(
            &self.permissions.content_read_private,
            |ct| self.can_read_content_type(ct),
            "read",
        )?;
        validate_scoped_field_grants(
            &self.permissions.content_write_private,
            |ct| self.can_write_content_type(ct),
            "write",
        )?;

        for perm in &self.permissions.sqlite {
            if perm != "kv_read" && perm != "kv_write" {
                return Err("Quyền SQLite không hợp lệ (chỉ hỗ trợ 'kv_read' và 'kv_write').");
            }
        }
        for grant in &self.permissions.network {
            let scheme = grant.scheme.to_ascii_lowercase();
            if scheme != "http" && scheme != "https" {
                return Err("Network scope scheme chỉ hỗ trợ http hoặc https.");
            }
            let host = grant.host.trim();
            if host.is_empty()
                || host.contains('/')
                || host.contains('?')
                || host.contains('#')
                || host.contains('@')
                || host.chars().any(|c| c.is_whitespace())
            {
                return Err("Network scope host không hợp lệ.");
            }
            if grant.port == Some(0) {
                return Err("Network scope port phải lớn hơn 0.");
            }
            if grant.methods.is_empty() {
                return Err("Network scope methods không được rỗng.");
            }
            for m in &grant.methods {
                let upper = m.to_ascii_uppercase();
                if !matches!(
                    upper.as_str(),
                    "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD"
                ) {
                    return Err("Network scope method không được hỗ trợ.");
                }
            }
            if let Some(prefix) = &grant.path_prefix {
                if !prefix.starts_with('/')
                    || prefix.contains('?')
                    || prefix.contains('#')
                    || prefix.contains('%')
                    || prefix.contains('\\')
                    || prefix.contains("..")
                {
                    return Err("Network scope path_prefix không hợp lệ.");
                }
            }
        }
        for url_str in &self.permissions.http {
            let parsed =
                url::Url::parse(url_str).map_err(|_| "URL trong quyền HTTP không hợp lệ.")?;
            if parsed.scheme() != "http" && parsed.scheme() != "https" {
                return Err("Chỉ hỗ trợ scheme http hoặc https.");
            }
        }
        let mut seen_actions = HashSet::new();
        for action in &self.actions {
            if !crate::id::valid_package_id(&action.id) && !crate::id::valid_snake_key(&action.id) {
                return Err("Action id trong manifest không hợp lệ.");
            }

            if !seen_actions.insert(action.id.as_str()) {
                return Err("Action id trong manifest bị trùng lặp.");
            }
            if action.required_role != "admin" && action.required_role != "editor" {
                return Err("Action required_role chỉ hỗ trợ 'admin' hoặc 'editor'.");
            }
            let mut has_kv_write = false;
            let mut has_content_write = false;
            let mut has_content_read = false;
            let mut has_content_read_private = false;
            let mut has_content_write_private = false;
            for cap in &action.capabilities {
                match cap.as_str() {
                    "kv_read" | "http" => {}
                    "content_read" => has_content_read = true,
                    "content_read_private" => has_content_read_private = true,
                    "kv_write" => has_kv_write = true,
                    "content_write" => has_content_write = true,
                    "content_write_private" => has_content_write_private = true,
                    _ => return Err("Action capability không nằm trong allowlist V1."),
                }
            }
            if has_kv_write && has_content_write {
                return Err("Action V1 không được kết hợp cả kv_write và content_write.");
            }
            if has_content_read_private
                && (!has_content_read || self.permissions.content_read_private.is_empty())
            {
                return Err(
                    "Action khai báo content_read_private phải kèm content_read và permissions.content_read_private.",
                );
            }
            if has_content_write_private
                && (!has_content_write || self.permissions.content_write_private.is_empty())
            {
                return Err(
                    "Action khai báo content_write_private phải kèm content_write và permissions.content_write_private.",
                );
            }
        }
        let mut seen_panel_keys = HashSet::new();
        for fg in &self.declarations.field_groups {
            seen_panel_keys.insert(fg.local_key.as_str());
        }
        for ep in &self.editor_panels {
            ep.validate()?;
            if !seen_panel_keys.insert(ep.local_key.as_str()) {
                return Err("local_key trong editor_panels bị trùng lặp hoặc trùng với field_groups.");
            }
            let Some(act) = self.action(&ep.action_id) else {
                return Err("editor_panels tham chiếu action_id không tồn tại trong actions.");
            };
            let min_edit = ep.min_edit_role.unwrap_or(crate::content::FieldRole::Editor);
            let act_role = if act.required_role == "admin" {
                crate::content::FieldRole::Admin
            } else {
                crate::content::FieldRole::Editor
            };
            if min_edit < act_role {
                return Err("editor_panels có min_edit_role thấp hơn required_role của action.");
            }
        }
        self.declarations.validate()?;
        Ok(())
    }

    pub fn can_read_content_type(&self, type_key: &str) -> bool {
        self.permissions
            .content_read
            .iter()
            .any(|k| k == "*" || k == type_key)
    }

    pub fn has_sqlite_perm(&self, perm: &str) -> bool {
        self.permissions.sqlite.iter().any(|p| p == perm)
    }

    pub fn can_write_content_type(&self, type_key: &str) -> bool {
        self.permissions
            .content_write
            .iter()
            .any(|k| k == "*" || k == type_key)
    }

    pub fn has_private_read_grant(&self, type_key: &str, field_key: &str) -> bool {
        self.permissions
            .content_read_private
            .iter()
            .any(|g| g.content_type == type_key && g.field_keys.iter().any(|k| k == field_key))
    }

    pub fn has_private_write_grant(&self, type_key: &str, field_key: &str) -> bool {
        self.permissions
            .content_write_private
            .iter()
            .any(|g| g.content_type == type_key && g.field_keys.iter().any(|k| k == field_key))
    }

    pub fn action(&self, id: &str) -> Option<&PluginActionDeclaration> {
        self.actions.iter().find(|a| a.id == id)
    }

    pub fn is_http_allowed_with_method(&self, method: &str, target_url: &str) -> bool {
        let Ok(target) = url::Url::parse(target_url) else {
            return false;
        };
        if !target.username().is_empty()
            || target.password().is_some()
            || target.fragment().is_some()
        {
            return false;
        }
        let lower_url = target_url.to_ascii_lowercase();
        let path_and_query = match lower_url.split_once("://") {
            Some((_, rest)) => rest.split_once('/').map(|(_, p)| p).unwrap_or(""),
            None => return false,
        };
        if path_and_query.contains("%2e")
            || path_and_query.contains("%2f")
            || path_and_query.contains("%5c")
            || path_and_query.contains('\\')
            || target.path().split('/').any(|seg| seg == "." || seg == "..")
        {
            return false;
        }
        let method_upper = method.to_ascii_uppercase();
        let target_scheme = target.scheme().to_ascii_lowercase();
        let Some(target_host_raw) = target.host_str() else {
            return false;
        };
        let target_host = target_host_raw
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_ascii_lowercase();
        let target_port = target.port_or_known_default();

        for grant in &self.permissions.network {
            let grant_scheme = grant.scheme.to_ascii_lowercase();
            let grant_host = grant
                .host
                .trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_ascii_lowercase();
            if target_scheme != grant_scheme || target_host != grant_host {
                continue;
            }
            if target_port != grant.effective_port() {
                continue;
            }
            if !grant
                .methods
                .iter()
                .any(|m| m.eq_ignore_ascii_case(&method_upper))
            {
                continue;
            }
            if let Some(prefix) = &grant.path_prefix {
                if prefix != "/" {
                    let trimmed = prefix.trim_end_matches('/');
                    let path = target.path();
                    let matches_prefix = if prefix.ends_with('/') {
                        path.starts_with(prefix)
                    } else {
                        path == trimmed || path.starts_with(&format!("{trimmed}/"))
                    };
                    if !matches_prefix {
                        continue;
                    }
                }
            }
            return true;
        }

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

/// Checks whether a plugin (in hook mode `action_caps == None` or action mode `Some(caps)`)
/// is permitted to read `field_entry` for `actor_role` (`270-R3`).
pub fn plugin_can_read_field(
    manifest: &PluginManifest,
    action_caps: Option<&[String]>,
    actor_role: crate::content::FieldRole,
    type_key: &str,
    field_entry: &crate::content::EffectiveFieldEntry,
) -> bool {
    if !manifest.can_read_content_type(type_key) {
        return false;
    }
    if let Some(caps) = action_caps {
        if !caps.iter().any(|c| c == "content_read") {
            return false;
        }
    }
    if actor_role < field_entry.definition.min_view_role {
        return false;
    }
    if field_entry.definition.public_exposure == crate::content::PublicExposure::Public {
        return true;
    }
    if field_entry.owner == crate::registry::ResourceOwner::Plugin(manifest.id.clone()) {
        return true;
    }
    manifest.has_private_read_grant(type_key, &field_entry.definition.key)
        && action_caps.is_none_or(|caps| caps.iter().any(|c| c == "content_read_private"))
}

/// Checks whether a plugin executing an action with `action_caps` is permitted to write
/// `field_entry` for `actor_role` (`270-R3`).
pub fn plugin_can_write_field(
    manifest: &PluginManifest,
    action_caps: &[String],
    actor_role: crate::content::FieldRole,
    type_key: &str,
    field_entry: &crate::content::EffectiveFieldEntry,
) -> bool {
    if !manifest.can_write_content_type(type_key)
        || !action_caps.iter().any(|c| c == "content_write")
    {
        return false;
    }
    if actor_role < field_entry.definition.min_edit_role {
        return false;
    }
    if field_entry.definition.public_exposure == crate::content::PublicExposure::Public {
        return true;
    }
    if field_entry.owner == crate::registry::ResourceOwner::Plugin(manifest.id.clone()) {
        return true;
    }
    manifest.has_private_write_grant(type_key, &field_entry.definition.key)
        && action_caps.iter().any(|c| c == "content_write_private")
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
}
