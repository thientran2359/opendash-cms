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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginSettingFieldType {
    Text,
    Textarea,
    Number,
    Boolean,
    Select,
    Url,
    Media,
    Secret,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PluginSettingChoice {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PluginSettingField {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: PluginSettingFieldType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: serde_json::Value,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub choices: Vec<PluginSettingChoice>,
}

fn default_true() -> bool {
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
            render_trusted_html: true,
        }
    }
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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RenderCachePolicy {
    #[default]
    ByRoute,
    Global,
    None,
}

impl RenderCachePolicy {
    pub fn is_default(&self) -> bool {
        *self == Self::ByRoute
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RouteMethod {
    #[serde(alias = "get")]
    Get,
    #[serde(alias = "post")]
    Post,
}

impl RouteMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteAccess {
    #[default]
    Public,
    Session,
    Admin,
    Webhook,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionRouteDecl {
    pub id: String,
    pub method: RouteMethod,
    pub path: String,
    #[serde(default)]
    pub access: RouteAccess,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "RenderCachePolicy::is_default")]
    pub cache: RenderCachePolicy,
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
    pub settings_schema: Vec<PluginSettingField>,
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

pub(crate) fn validate_min_host_version(
    min_host_version: Option<&str>,
) -> Result<(), &'static str> {
    if let Some(min_ver) = min_host_version {
        if !validate_semver(min_ver) {
            return Err("min_host_version không đúng định dạng SemVer 2.0.0.");
        }
        if !host_satisfies_min_version(CURRENT_HOST_VERSION, min_ver) {
            return Err("Phiên bản host hiện tại thấp hơn min_host_version yêu cầu.");
        }
    }
    Ok(())
}

pub(crate) fn validate_hooks_and_filters(
    hooks: &[crate::hooks::HookEvent],
    filters: &[crate::hooks::HookEvent],
    allow_theme_after_activate: bool,
) -> Result<(), &'static str> {
    let mut seen_hooks = HashSet::new();
    for event in hooks {
        if *event == crate::hooks::HookEvent::PluginManualRun
            || event.is_filter()
            || (!allow_theme_after_activate
                && *event == crate::hooks::HookEvent::ThemeAfterActivate)
        {
            return Err("Sự kiện hook không được hỗ trợ trong danh sách hooks.");
        }
        if !seen_hooks.insert(event.as_str()) {
            return Err("Danh sách hooks không được chứa sự kiện trùng lặp.");
        }
    }
    let mut seen_filters = HashSet::new();
    for filter in filters {
        if !filter.is_filter() {
            return Err("Chỉ các filter:* mới được khai báo trong danh sách filters.");
        }
        if !seen_filters.insert(filter.as_str()) {
            return Err("Danh sách filters không được chứa sự kiện trùng lặp.");
        }
    }
    Ok(())
}

pub(crate) fn validate_settings_schema(
    settings_schema: &[PluginSettingField],
) -> Result<HashSet<&str>, &'static str> {
    if settings_schema.len() > 64 {
        return Err("settings_schema không được vượt quá 64 trường.");
    }
    let mut seen_settings = HashSet::new();
    let mut secret_keys = HashSet::new();
    for field in settings_schema {
        if !crate::id::valid_snake_key(&field.key) || !seen_settings.insert(field.key.as_str()) {
            return Err("Khóa trường trong settings_schema phải là snake_case và không trùng lặp.");
        }
        if field.label.trim().is_empty() || field.label.chars().count() > 100 {
            return Err("Nhãn trường trong settings_schema không hợp lệ.");
        }
        if field.field_type == PluginSettingFieldType::Secret {
            if !field.default.is_null() {
                return Err(
                    "Trường settings kiểu secret không được đặt giá trị mặc định khác null.",
                );
            }
            secret_keys.insert(field.key.as_str());
        }
        if let Some(min) = field.min
            && !min.is_finite()
        {
            return Err("settings_schema min không hợp lệ.");
        }
        if let Some(max) = field.max
            && !max.is_finite()
        {
            return Err("settings_schema max không hợp lệ.");
        }
        if let (Some(min), Some(max)) = (field.min, field.max)
            && min > max
        {
            return Err("settings_schema yêu cầu min <= max.");
        }
        if field.field_type == PluginSettingFieldType::Select {
            if field.choices.is_empty() || field.choices.len() > 64 {
                return Err("Trường select trong settings_schema phải có 1..=64 lựa chọn.");
            }
            let mut seen_choices = HashSet::new();
            for ch in &field.choices {
                if ch.value.trim().is_empty()
                    || ch.value.chars().count() > 100
                    || ch.label.trim().is_empty()
                    || ch.label.chars().count() > 100
                    || !seen_choices.insert(ch.value.as_str())
                {
                    return Err("Lựa chọn của trường select trong settings_schema không hợp lệ.");
                }
            }
        }
    }
    Ok(secret_keys)
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
        let mut has_content_read = false;
        let mut has_content_read_private = false;
        let mut has_content_write_private = false;
        for cap in &action.capabilities {
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
                _ => return Err("Action capability không nằm trong allowlist."),
            }
        }
        if (has_kv_write as u8) + (has_content_write as u8) + (has_content_delete as u8) > 1 {
            return Err(
                "Action không được kết hợp kv_write/kv_delete, content_write hoặc content_delete trong cùng một action.",
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

/// Match a normalized request subpath (with or without leading `/`) against a declared route path.
pub fn match_route_path(
    pattern: &str,
    subpath: &str,
) -> Option<std::collections::BTreeMap<String, String>> {
    let trimmed = subpath.trim_matches('/');
    let req_norm = if trimmed.is_empty() { "/" } else { trimmed };
    if pattern == "/" {
        return (req_norm == "/").then(std::collections::BTreeMap::new);
    }
    if req_norm == "/" {
        return None;
    }
    let pat_segs: Vec<&str> = pattern.trim_matches('/').split('/').collect();
    let req_segs: Vec<&str> = req_norm.split('/').collect();
    if pat_segs.len() != req_segs.len() {
        return None;
    }
    let mut params = std::collections::BTreeMap::new();
    for (p, r) in pat_segs.into_iter().zip(req_segs.into_iter()) {
        if let Some(name) = p.strip_prefix(':') {
            if r.is_empty() || r == "." || r == ".." {
                return None;
            }
            params.insert(name.to_owned(), r.to_owned());
        } else if p != r {
            return None;
        }
    }
    Some(params)
}

impl PluginManifest {
    pub fn from_json_str(json_str: &str) -> Result<Self, &'static str> {
        let mut raw_val: serde_json::Value =
            serde_json::from_str(json_str).map_err(|_| "JSON manifest không hợp lệ.")?;
        if let Some(obj) = raw_val.as_object_mut() {
            // Admin extensions are persisted in the owner-aware registry, not in
            // the Wasm ABI manifest. Keep Rust PluginManifest literals and ABI
            // clients backwards compatible.
            if let Some(raw) = obj.remove("admin_extensions") {
                let value: crate::admin_extensions::AdminExtensions =
                    serde_json::from_value(raw)
                        .map_err(|_| "admin_extensions không đúng cấu trúc.")?;
                let schema = obj
                    .get("manifest_schema")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(1);
                if schema < 2 {
                    return Err("admin_extensions yêu cầu manifest_schema >= 2.");
                }
                let actions = obj
                    .get("actions")
                    .and_then(serde_json::Value::as_array)
                    .map(|list| {
                        list.iter()
                            .filter_map(|action| {
                                action
                                    .get("id")
                                    .and_then(serde_json::Value::as_str)
                                    .map(str::to_string)
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                value.validate("plugin", &actions)?;
            }
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
        if !matches!(self.manifest_schema, 1..=3) {
            return Err("manifest_schema không được hỗ trợ (chỉ hỗ trợ 1, 2 hoặc 3).");
        }
        if !self.declarations.field_groups.is_empty() && self.manifest_schema < 2 {
            return Err("field_groups trong plugin.json chỉ hợp lệ khi manifest_schema >= 2.");
        }
        if !self.editor_panels.is_empty() && self.manifest_schema < 2 {
            return Err("editor_panels trong plugin.json chỉ hợp lệ khi manifest_schema >= 2.");
        }
        if self.editor_panels.len() > 16 {
            return Err("plugin.json không được khai báo quá 16 editor_panels.");
        }
        if (!self.permissions.content_read_private.is_empty()
            || !self.permissions.content_write_private.is_empty())
            && (self.manifest_schema < 2 || self.min_host_version.is_none())
        {
            return Err(
                "content_read_private và content_write_private chỉ hợp lệ khi manifest_schema >= 2 và có khai báo min_host_version.",
            );
        }
        let uses_v3_features = self.entrypoint == "plugin.js"
            || self.render_cache != RenderCachePolicy::ByRoute
            || !self.routes.is_empty()
            || !self.filters.is_empty()
            || !self.settings_schema.is_empty()
            || self.permissions.content_read_all_public
            || self.permissions.taxonomy_read
            || self.permissions.media_read
            || self.permissions.site_options_read
            || self.permissions.plugin_settings_read
            || !self.permissions.plugin_secrets_read.is_empty()
            || !self.permissions.content_delete.is_empty()
            || self.hooks.iter().any(|h| {
                !matches!(
                    h,
                    crate::hooks::HookEvent::PageAfterSave
                        | crate::hooks::HookEvent::ContentBeforeSave
                        | crate::hooks::HookEvent::ContentAfterSave
                )
            });
        if uses_v3_features && (self.manifest_schema < 3 || self.min_host_version.is_none()) {
            return Err(
                "Tính năng mở rộng v3 (plugin.js, filters, settings_schema, quyền v3, lifecycle hooks v3) yêu cầu manifest_schema >= 3 và khai báo min_host_version.",
            );
        }
        validate_min_host_version(self.min_host_version.as_deref())?;
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
        if self.entrypoint != "plugin.wasm" && self.entrypoint != "plugin.js" {
            return Err("Entrypoint phải là 'plugin.wasm' hoặc 'plugin.js'.");
        }
        validate_hooks_and_filters(&self.hooks, &self.filters, false)?;
        let secret_keys = validate_settings_schema(&self.settings_schema)?;
        validate_permissions(&self.permissions, &secret_keys)?;
        validate_actions(&self.actions, &self.permissions)?;
        validate_routes(&self.routes, &self.permissions)?;
        let mut seen_panel_keys = HashSet::new();
        for fg in &self.declarations.field_groups {
            seen_panel_keys.insert(fg.local_key.as_str());
        }
        for ep in &self.editor_panels {
            ep.validate()?;
            if !seen_panel_keys.insert(ep.local_key.as_str()) {
                return Err(
                    "local_key trong editor_panels bị trùng lặp hoặc trùng với field_groups.",
                );
            }
            let Some(act) = self.action(&ep.action_id) else {
                return Err("editor_panels tham chiếu action_id không tồn tại trong actions.");
            };
            let min_edit = ep
                .min_edit_role
                .unwrap_or(crate::content::FieldRole::Editor);
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

    pub fn runtime_kind(&self) -> crate::hooks::ExtensionRuntimeKind {
        crate::hooks::ExtensionRuntimeKind::from_entrypoint(&self.entrypoint)
    }

    pub fn can_read_content_type(&self, type_key: &str) -> bool {
        self.permissions.content_read_all_public
            || self
                .permissions
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

    pub fn can_delete_content_type(&self, type_key: &str) -> bool {
        self.permissions
            .content_delete
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
            || target
                .path()
                .split('/')
                .any(|seg| seg == "." || seg == "..")
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

/// Checks whether an extension (`owner_kind` Plugin or Theme) is permitted to read
/// `field_entry` for `actor_role`.
pub fn plugin_can_read_field_for_owner(
    owner_kind: crate::hooks::ExtensionOwnerKind,
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
    let expected_owner = match owner_kind {
        crate::hooks::ExtensionOwnerKind::Plugin => {
            crate::registry::ResourceOwner::Plugin(manifest.id.clone())
        }
        crate::hooks::ExtensionOwnerKind::Theme => {
            crate::registry::ResourceOwner::Theme(manifest.id.clone())
        }
    };
    if field_entry.owner == expected_owner {
        return true;
    }
    manifest.has_private_read_grant(type_key, &field_entry.definition.key)
        && action_caps.is_none_or(|caps| caps.iter().any(|c| c == "content_read_private"))
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
    plugin_can_read_field_for_owner(
        crate::hooks::ExtensionOwnerKind::Plugin,
        manifest,
        action_caps,
        actor_role,
        type_key,
        field_entry,
    )
}

/// Checks whether an extension (`owner_kind` Plugin or Theme) is permitted to write
/// `field_entry` for `actor_role`.
pub fn plugin_can_write_field_for_owner(
    owner_kind: crate::hooks::ExtensionOwnerKind,
    manifest: &PluginManifest,
    action_caps: Option<&[String]>,
    actor_role: crate::content::FieldRole,
    type_key: &str,
    field_entry: &crate::content::EffectiveFieldEntry,
) -> bool {
    if !manifest.can_write_content_type(type_key) {
        return false;
    }
    if let Some(caps) = action_caps {
        if !caps.iter().any(|c| c == "content_write") {
            return false;
        }
    }
    if actor_role < field_entry.definition.min_edit_role {
        return false;
    }
    if field_entry.definition.public_exposure == crate::content::PublicExposure::Public {
        return true;
    }
    let expected_owner = match owner_kind {
        crate::hooks::ExtensionOwnerKind::Plugin => {
            crate::registry::ResourceOwner::Plugin(manifest.id.clone())
        }
        crate::hooks::ExtensionOwnerKind::Theme => {
            crate::registry::ResourceOwner::Theme(manifest.id.clone())
        }
    };
    if field_entry.owner == expected_owner {
        return true;
    }
    manifest.has_private_write_grant(type_key, &field_entry.definition.key)
        && action_caps.is_none_or(|caps| caps.iter().any(|c| c == "content_write_private"))
}

/// Checks whether a plugin (in hook/filter mode `action_caps == None` or action mode `Some(caps)`)
/// is permitted to write `field_entry` for `actor_role` (`270-R3`).
pub fn plugin_can_write_field(
    manifest: &PluginManifest,
    action_caps: Option<&[String]>,
    actor_role: crate::content::FieldRole,
    type_key: &str,
    field_entry: &crate::content::EffectiveFieldEntry,
) -> bool {
    plugin_can_write_field_for_owner(
        crate::hooks::ExtensionOwnerKind::Plugin,
        manifest,
        action_caps,
        actor_role,
        type_key,
        field_entry,
    )
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
                { "key": "mode", "label": "Mode", "type": "select", "default": "sync", "choices": [
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
        mixed_caps["actions"][0]["capabilities"] =
            serde_json::json!(["content_delete", "kv_delete"]);
        assert!(PluginManifest::from_json_str(&mixed_caps.to_string()).is_err());
    }
}
