use super::*;

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
        let uses_lifecycle_v011 = !self.permissions.content_trash_read.is_empty()
            || !self.permissions.content_restore.is_empty()
            || !self.permissions.content_purge.is_empty()
            || self.actions.iter().any(|action| {
                action.capabilities.iter().any(|cap| {
                    matches!(
                        cap.as_str(),
                        "content_trash_read" | "content_restore" | "content_purge"
                    )
                })
            });
        let uses_v3_features = uses_lifecycle_v011
            || self.entrypoint == "plugin.js"
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
        if uses_lifecycle_v011
            && !self
                .min_host_version
                .as_deref()
                .is_some_and(|min_ver| host_satisfies_min_version(min_ver, "0.1.1"))
        {
            return Err(
                "Quyền vòng đời nâng cao (content_trash_read, content_restore, content_purge) yêu cầu min_host_version >= 0.1.1.",
            );
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

    pub fn can_read_trash_content_type(&self, type_key: &str) -> bool {
        self.permissions
            .content_trash_read
            .iter()
            .any(|k| k == "*" || k == type_key)
    }

    pub fn can_restore_content_type(&self, type_key: &str) -> bool {
        self.permissions
            .content_restore
            .iter()
            .any(|k| k == "*" || k == type_key)
    }

    pub fn can_purge_content_type(&self, type_key: &str) -> bool {
        self.permissions
            .content_purge
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
