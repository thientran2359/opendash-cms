use super::*;

pub(super) use std::collections::HashSet;

pub(super) use serde::{Deserialize, Serialize};

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

pub(super) fn default_https_scheme() -> String {
    "https".into()
}

pub(super) fn default_network_methods() -> Vec<String> {
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

pub(super) fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    pub content_read_all_public: bool,
    #[serde(default)]
    pub content_write: Vec<String>,
    #[serde(default)]
    pub content_read_private: Vec<ScopedFieldGrant>,
    #[serde(default)]
    pub content_write_private: Vec<ScopedFieldGrant>,
    #[serde(default)]
    pub taxonomy_read: bool,
    #[serde(default)]
    pub media_read: bool,
    #[serde(default)]
    pub site_options_read: bool,
    #[serde(default)]
    pub plugin_settings_read: bool,
    #[serde(default)]
    pub plugin_secrets_read: Vec<String>,
    #[serde(default)]
    pub content_delete: Vec<String>,
    #[serde(default)]
    pub content_trash_read: Vec<String>,
    #[serde(default)]
    pub content_restore: Vec<String>,
    #[serde(default)]
    pub content_purge: Vec<String>,
    #[serde(default = "default_true")]
    pub render_trusted_html: bool,
}

impl Default for PluginPermissions {
    fn default() -> Self {
        Self {
            sqlite: Vec::new(),
            http: Vec::new(),
            network: Vec::new(),
            content_read: Vec::new(),
            content_read_all_public: false,
            content_write: Vec::new(),
            content_read_private: Vec::new(),
            content_write_private: Vec::new(),
            taxonomy_read: false,
            media_read: false,
            site_options_read: false,
            plugin_settings_read: false,
            plugin_secrets_read: Vec::new(),
            content_delete: Vec::new(),
            content_trash_read: Vec::new(),
            content_restore: Vec::new(),
            content_purge: Vec::new(),
            render_trusted_html: true,
        }
    }
}

pub(super) fn default_action_role() -> String {
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<crate::hooks::HookEvent>,
    #[serde(default)]
    pub hook_priority: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<PluginActionDeclaration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<ExtensionRouteDecl>,
    #[serde(default, skip_serializing_if = "RenderCachePolicy::is_default")]
    pub render_cache: RenderCachePolicy,
    #[serde(
        default,
        skip_serializing_if = "crate::content::CntDeclarations::is_empty"
    )]
    pub declarations: crate::content::CntDeclarations,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub editor_panels: Vec<crate::content::ManifestActionPanelInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings_schema: Vec<crate::field_schema::FieldSpec>,
}

pub(super) fn default_manifest_schema() -> u32 {
    1
}

pub(super) fn default_entrypoint() -> String {
    "plugin.wasm".into()
}

pub(super) fn validate_scoped_field_grants<F>(
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
                "read" => {
                    "content_read_private không cho phép wildcard '*' và phải dùng content_type hợp lệ."
                }
                _ => {
                    "content_write_private không cho phép wildcard '*' và phải dùng content_type hợp lệ."
                }
            });
        }
        if !has_base_type_perm(&grant.content_type) {
            return Err(match label {
                "read" => {
                    "content_read_private yêu cầu plugin đã có quyền content_read cho loại nội dung tương ứng."
                }
                _ => {
                    "content_write_private yêu cầu plugin đã có quyền content_write cho loại nội dung tương ứng."
                }
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

pub(crate) fn validate_permissions(
    permissions: &PluginPermissions,
    secret_keys: &HashSet<&str>,
) -> Result<(), &'static str> {
    for ct_perm in &permissions.content_read {
        if ct_perm != "*" && !crate::id::valid_snake_key(ct_perm) {
            return Err("Quyền content_read không hợp lệ.");
        }
    }
    for ct_perm in &permissions.content_write {
        if ct_perm != "*" && !crate::id::valid_snake_key(ct_perm) {
            return Err("Quyền content_write không hợp lệ.");
        }
    }
    for ct_perm in &permissions.content_delete {
        if ct_perm != "*" && !crate::id::valid_snake_key(ct_perm) {
            return Err("Quyền content_delete không hợp lệ.");
        }
    }
    for ct_perm in &permissions.content_trash_read {
        if ct_perm != "*" && !crate::id::valid_snake_key(ct_perm) {
            return Err("Quyền content_trash_read không hợp lệ.");
        }
    }
    for ct_perm in &permissions.content_restore {
        if ct_perm != "*" && !crate::id::valid_snake_key(ct_perm) {
            return Err("Quyền content_restore không hợp lệ.");
        }
    }
    for ct_perm in &permissions.content_purge {
        if ct_perm != "*" && !crate::id::valid_snake_key(ct_perm) {
            return Err("Quyền content_purge không hợp lệ.");
        }
    }
    let mut seen_secrets_read = HashSet::new();
    for secret_key in &permissions.plugin_secrets_read {
        if secret_key == "*"
            || !crate::id::valid_snake_key(secret_key)
            || !seen_secrets_read.insert(secret_key.as_str())
            || !secret_keys.contains(secret_key.as_str())
        {
            return Err(
                "plugin_secrets_read phải liệt kê các khóa secret hợp lệ tồn tại trong settings_schema (cấm wildcard '*').",
            );
        }
    }
    validate_scoped_field_grants(
        &permissions.content_read_private,
        |ct| {
            permissions.content_read_all_public
                || permissions.content_read.iter().any(|k| k == "*" || k == ct)
        },
        "read",
    )?;
    validate_scoped_field_grants(
        &permissions.content_write_private,
        |ct| {
            permissions
                .content_write
                .iter()
                .any(|k| k == "*" || k == ct)
        },
        "write",
    )?;

    for perm in &permissions.sqlite {
        if perm != "kv_read" && perm != "kv_write" {
            return Err("Quyền SQLite không hợp lệ (chỉ hỗ trợ 'kv_read' và 'kv_write').");
        }
    }
    for grant in &permissions.network {
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
        if let Some(prefix) = &grant.path_prefix
            && (!prefix.starts_with('/')
                || prefix.contains('?')
                || prefix.contains('#')
                || prefix.contains('%')
                || prefix.contains('\\')
                || prefix.contains(".."))
        {
            return Err("Network scope path_prefix không hợp lệ.");
        }
    }
    for url_str in &permissions.http {
        let parsed = url::Url::parse(url_str).map_err(|_| "URL trong quyền HTTP không hợp lệ.")?;
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err("Chỉ hỗ trợ scheme http hoặc https.");
        }
    }
    Ok(())
}

pub(crate) fn validate_actions(
    actions: &[PluginActionDeclaration],
    permissions: &PluginPermissions,
) -> Result<(), &'static str> {
    let mut seen_actions = HashSet::new();
    for action in actions {
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
        let mut has_content_delete = false;
        let mut has_content_restore = false;
        let mut has_content_purge = false;
        let mut has_http = false;
        let mut has_content_read = false;
        let mut has_content_read_private = false;
        let mut has_content_write_private = false;
        for cap in &action.capabilities {
            match cap.as_str() {
                "kv_read"
                | "kv_list"
                | "taxonomy_read"
                | "media_read"
                | "site_options_read"
                | "plugin_settings_read"
                | "plugin_secrets_read"
                | "content_trash_read"
                | "log" => {}
                "http" => has_http = true,
                "content_read" => has_content_read = true,
                "content_read_private" => has_content_read_private = true,
                "kv_write" | "kv_delete" => has_kv_write = true,
                "content_write" => has_content_write = true,
                "content_write_private" => has_content_write_private = true,
                "content_delete" => has_content_delete = true,
                "content_restore" => has_content_restore = true,
                "content_purge" => has_content_purge = true,
                _ => return Err("Action capability không nằm trong allowlist."),
            }
        }
        if (has_kv_write as u8)
            + (has_content_write as u8)
            + (has_content_delete as u8)
            + (has_content_restore as u8)
            + (has_content_purge as u8)
            > 1
        {
            return Err(
                "Action không được kết hợp kv_write/kv_delete, content_write, content_delete, content_restore hoặc content_purge trong cùng một action.",
            );
        }
        if (has_content_restore || has_content_purge) && has_http {
            return Err(
                "Action khai báo content_restore hoặc content_purge không được kết hợp capability http.",
            );
        }
        if has_content_read_private
            && (!has_content_read || permissions.content_read_private.is_empty())
        {
            return Err(
                "Action khai báo content_read_private phải kèm content_read và permissions.content_read_private.",
            );
        }
        if has_content_write_private
            && (!has_content_write || permissions.content_write_private.is_empty())
        {
            return Err(
                "Action khai báo content_write_private phải kèm content_write và permissions.content_write_private.",
            );
        }
    }
    Ok(())
}

pub(crate) fn validate_routes(
    routes: &[ExtensionRouteDecl],
    permissions: &PluginPermissions,
) -> Result<(), &'static str> {
    if routes.len() > 32 {
        return Err("Manifest không được khai báo quá 32 routes.");
    }
    let mut seen_ids = HashSet::new();
    let mut seen_method_paths = HashSet::new();
    for route in routes {
        if !crate::id::valid_kebab_segment_with_limit(&route.id, 64) {
            return Err("Route id trong manifest phải là kebab-case (1..=64 ký tự).");
        }
        if !seen_ids.insert(route.id.as_str()) {
            return Err("Route id trong manifest bị trùng lặp.");
        }
        if route.method == RouteMethod::Get && route.access == RouteAccess::Webhook {
            return Err("Route GET không hỗ trợ access='webhook' (chỉ POST).");
        }
        let path = route.path.as_str();
        if path.is_empty()
            || path.len() > 128
            || !path.starts_with('/')
            || !path
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'_' | b':' | b'-'))
            || path.contains("..")
            || (path.len() > 1 && path.ends_with('/'))
        {
            return Err("Route path trong manifest không hợp lệ.");
        }
        if path != "/" {
            let segments: Vec<&str> = path[1..].split('/').collect();
            if segments.len() > 8 {
                return Err("Route path không được vượt quá 8 segments.");
            }
            let mut param_names = HashSet::new();
            for seg in segments {
                if seg.is_empty() {
                    return Err("Route path không được chứa segment rỗng.");
                }
                if let Some(param) = seg.strip_prefix(':') {
                    if param.is_empty()
                        || param.len() > 32
                        || !param
                            .bytes()
                            .next()
                            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
                        || !param
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                    {
                        return Err("Tên tham số route (:param) không hợp lệ.");
                    }
                    if !param_names.insert(param) {
                        return Err("Tên tham số route (:param) bị trùng lặp trong cùng path.");
                    }
                } else if seg.contains(':') {
                    return Err("Ký tự ':' chỉ được dùng ở đầu segment tham số (:param).");
                }
            }
        }
        if !seen_method_paths.insert((route.method, path)) {
            return Err("Cặp (method, path) trong routes bị trùng lặp.");
        }
        let mut has_kv_write = false;
        let mut has_content_write = false;
        let mut has_content_delete = false;
        let mut has_content_read = false;
        let mut has_content_read_private = false;
        let mut has_content_write_private = false;
        for cap in &route.capabilities {
            match cap.as_str() {
                "kv_read"
                | "kv_list"
                | "http"
                | "taxonomy_read"
                | "media_read"
                | "site_options_read"
                | "plugin_settings_read"
                | "plugin_secrets_read"
                | "log" => {}
                "content_read" => has_content_read = true,
                "content_read_private" => has_content_read_private = true,
                "kv_write" | "kv_delete" => has_kv_write = true,
                "content_write" => has_content_write = true,
                "content_write_private" => has_content_write_private = true,
                "content_delete" => has_content_delete = true,
                _ => return Err("Route capability không nằm trong allowlist."),
            }
        }
        if (has_kv_write as u8) + (has_content_write as u8) + (has_content_delete as u8) > 1 {
            return Err(
                "Route không được kết hợp kv_write/kv_delete, content_write hoặc content_delete trong cùng một route.",
            );
        }
        if has_content_read_private
            && (!has_content_read || permissions.content_read_private.is_empty())
        {
            return Err(
                "Route khai báo content_read_private phải kèm content_read và permissions.content_read_private.",
            );
        }
        if has_content_write_private
            && (!has_content_write || permissions.content_write_private.is_empty())
        {
            return Err(
                "Route khai báo content_write_private phải kèm content_write và permissions.content_write_private.",
            );
        }
    }
    Ok(())
}
