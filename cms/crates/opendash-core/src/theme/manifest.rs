use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemePatternDecl {
    pub key: String,
    pub title: String,
    pub category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_asset: Option<String>,
    pub file: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptLocation {
    Head,
    #[default]
    Footer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptStrategy {
    Blocking,
    #[default]
    Defer,
    Async,
}

pub(super) fn default_theme_functions_api_version() -> u32 {
    crate::plugin::PLUGIN_ABI_VERSION
}

pub(super) fn default_theme_hook_priority() -> i32 {
    100
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeFunctionsSpec {
    #[serde(default = "default_theme_functions_api_version")]
    pub api_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_host_version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<crate::hooks::HookEvent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<crate::hooks::HookEvent>,
    #[serde(default = "default_theme_hook_priority")]
    pub hook_priority: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<crate::plugin::PluginPermissions>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<crate::plugin::PluginActionDeclaration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<crate::plugin::ExtensionRouteDecl>,
    #[serde(
        default,
        skip_serializing_if = "crate::plugin::RenderCachePolicy::is_default"
    )]
    pub render_cache: crate::plugin::RenderCachePolicy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings_schema: Vec<crate::field_schema::FieldSpec>,
}

impl ThemeFunctionsSpec {
    /// Active Theme `functions.js` automatically receives basic public read and
    /// theme-scoped KV permissions while sensitive permissions remain explicit.
    pub fn effective_permissions(&self) -> crate::plugin::PluginPermissions {
        let mut perms = self.permissions.clone().unwrap_or_default();
        if !perms.sqlite.iter().any(|p| p == "kv_read") {
            perms.sqlite.push("kv_read".into());
        }
        if !perms.sqlite.iter().any(|p| p == "kv_write") {
            perms.sqlite.push("kv_write".into());
        }
        perms.content_read_all_public = true;
        perms.taxonomy_read = true;
        perms.media_read = true;
        perms.site_options_read = true;
        perms.plugin_settings_read = true;
        perms
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.api_version != crate::plugin::PLUGIN_ABI_VERSION {
            return Err("Chỉ hỗ trợ functions.api_version 1.");
        }
        crate::plugin::validate_min_host_version(self.min_host_version.as_deref())?;
        crate::plugin::validate_hooks_and_filters(&self.hooks, &self.filters, true)?;
        let secret_keys = crate::plugin::validate_settings_schema(&self.settings_schema)?;
        if let Some(raw) = &self.permissions {
            if !raw.content_trash_read.is_empty()
                || !raw.content_restore.is_empty()
                || !raw.content_purge.is_empty()
            {
                return Err(
                    "Theme functions.js không hỗ trợ quyền content_trash_read/content_restore/content_purge; chỉ plugin khai báo min_host_version >= 0.1.1 mới được cấp.",
                );
            }
            // Validate declared entries (including invalid sqlite/http/network tokens)
            // before merging defaults, allowing content_read_private against effective reads.
            let mut merged = raw.clone();
            merged.content_read_all_public = true;
            crate::plugin::validate_permissions(&merged, &secret_keys)?;
        }
        for action in &self.actions {
            if action.capabilities.iter().any(|c| {
                matches!(
                    c.as_str(),
                    "content_trash_read" | "content_restore" | "content_purge"
                )
            }) {
                return Err(
                    "Theme functions.js actions không hỗ trợ capability content_trash_read/content_restore/content_purge.",
                );
            }
        }
        let effective = self.effective_permissions();
        crate::plugin::validate_permissions(&effective, &secret_keys)?;
        crate::plugin::validate_actions(&self.actions, &effective)?;
        crate::plugin::validate_routes(&self.routes, &effective)?;
        Ok(())
    }
}

pub(super) fn deserialize_null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    T: Default + serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

pub fn validate_theme_options_fields(
    specs: &[crate::field_schema::FieldSpec],
) -> Result<(), &'static str> {
    crate::field_schema::validate_field_specs(specs, crate::field_schema::FieldContext::Options)?;
    for spec in specs {
        if spec.is_ui_only() {
            continue;
        }
        if crate::field_schema::GLOBAL_PUBLIC_URL_OPTION_KEYS.contains(&spec.key.as_str())
            && spec.field_type != crate::field_schema::FieldType::Url
        {
            return Err(
                "Theme option cannot override a global public URL option with a non-url type",
            );
        }
        if crate::field_schema::GLOBAL_PUBLIC_MEDIA_OPTION_KEYS.contains(&spec.key.as_str())
            && !(spec.field_type == crate::field_schema::FieldType::Media
                && spec.effective_media_kind() == crate::field_schema::MediaFieldKind::Image)
        {
            return Err(
                "Theme option cannot override a global public media option with an incompatible type",
            );
        }
    }
    Ok(())
}

pub(super) fn valid_label(value: &str, max_chars: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= max_chars
        && !value.chars().any(char::is_control)
}

pub(super) fn valid_file_name(value: &str, suffix: &str) -> bool {
    value.strip_suffix(suffix).is_some_and(|stem| {
        (1..=96).contains(&stem.len())
            && stem
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_')
    })
}

pub(super) fn valid_root_html(value: &str) -> bool {
    valid_file_name(value, ".html")
}

pub(super) fn valid_subdir_file(value: &str, prefix: &str, suffix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|name| valid_file_name(name, suffix))
}
