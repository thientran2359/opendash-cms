use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::id::{
    valid_kebab_segment_with_limit, valid_lowercase_locale_code, valid_namespaced_block_id,
    valid_namespaced_snake_key, valid_snake_key, valid_snake_key_with_limit, validate_package_id,
    validate_semver,
};

pub type Fields = BTreeMap<String, String>;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BlockDescriptor {
    pub id: String,
    pub fields: Fields,
}

impl BlockDescriptor {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_namespaced_block_id(&self.id) {
            return Err("ID block không hợp lệ.");
        }
        if self.fields.is_empty()
            || self.fields.len() > 12
            || self.fields.iter().any(|(key, label)| {
                !valid_snake_key_with_limit(key, 32)
                    || label.trim().is_empty()
                    || label.chars().count() > 80
            })
        {
            return Err("Khai báo field không hợp lệ.");
        }
        Ok(())
    }

    pub fn to_registered_schema(&self) -> crate::block_schema::RegisteredBlockSchema {
        crate::block_schema::upcast_legacy_descriptor(self)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Block {
    #[serde(rename = "type")]
    pub kind: String,
    pub values: Fields,
}

impl Block {
    pub fn validate_against_fields(&self, fields: &Fields) -> Result<(), &'static str> {
        if self.values.keys().ne(fields.keys())
            || self
                .values
                .values()
                .any(|value| value.chars().count() > 512)
        {
            return Err("Giá trị nhập không khớp khai báo block.");
        }
        Ok(())
    }
}

/// A block key is namespaced, e.g. `core/heading`.
pub fn valid_block_key(value: &str) -> bool {
    valid_namespaced_snake_key(value)
}

/// An ordered resolver target. IDs are site-local and must never appear in a
/// packaged theme declaration; they are reserved for site-owned overrides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateCandidateKind {
    Home,
    Page,
    PageLayout,
    Single,
    SingleCustom,
    Archive,
    Taxonomy,
    Singular,
    Index,
    NotFound,
    Part,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateCandidate {
    pub kind: TemplateCandidateKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taxonomy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector_slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub term_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_key: Option<String>,
}

impl TemplateCandidate {
    pub fn new(kind: TemplateCandidateKind) -> Self {
        Self {
            kind,
            content_type: None,
            taxonomy: None,
            selector_slug: None,
            entry_id: None,
            term_id: None,
            custom_key: None,
            layout_key: None,
            part_key: None,
        }
    }

    /// Canonical, collision-free identity shared by the DB and theme resolver.
    /// Rejects unused dimensions as well as invalid segments.
    pub fn canonical_key(&self) -> Result<String, &'static str> {
        if self
            .content_type
            .as_deref()
            .is_some_and(|key| !valid_snake_key(key))
            || self
                .taxonomy
                .as_deref()
                .is_some_and(|key| !valid_snake_key(key))
            || self
                .selector_slug
                .as_deref()
                .is_some_and(|slug| !valid_kebab_segment_with_limit(slug, 64))
            || self
                .custom_key
                .as_deref()
                .is_some_and(|key| !valid_snake_key(key))
            || self
                .layout_key
                .as_deref()
                .is_some_and(|key| !valid_snake_key(key))
            || self
                .part_key
                .as_deref()
                .is_some_and(|key| !valid_kebab_segment_with_limit(key, 64))
            || self.entry_id.is_some_and(|id| id <= 0)
            || self.term_id.is_some_and(|id| id <= 0)
            || (self.entry_id.is_some() && self.selector_slug.is_some())
            || (self.term_id.is_some() && self.selector_slug.is_some())
        {
            return Err("Invalid template candidate identifier");
        }

        let only_plain = self.content_type.is_none()
            && self.taxonomy.is_none()
            && self.selector_slug.is_none()
            && self.entry_id.is_none()
            && self.term_id.is_none()
            && self.custom_key.is_none()
            && self.layout_key.is_none()
            && self.part_key.is_none();
        let no_other =
            self.custom_key.is_none() && self.layout_key.is_none() && self.part_key.is_none();
        match self.kind {
            TemplateCandidateKind::Home if only_plain => Ok("home".into()),
            TemplateCandidateKind::Page if only_plain => Ok("page".into()),
            TemplateCandidateKind::Singular if only_plain => Ok("singular".into()),
            TemplateCandidateKind::Index if only_plain => Ok("index".into()),
            TemplateCandidateKind::NotFound if only_plain => Ok("not_found".into()),
            TemplateCandidateKind::PageLayout
                if self.layout_key.is_some()
                    && self.content_type.is_none()
                    && self.taxonomy.is_none()
                    && self.selector_slug.is_none()
                    && self.entry_id.is_none()
                    && self.term_id.is_none()
                    && self.custom_key.is_none()
                    && self.part_key.is_none() =>
            {
                Ok(format!(
                    "page_layout:{}",
                    self.layout_key.as_deref().unwrap()
                ))
            }
            TemplateCandidateKind::Part
                if self.part_key.is_some()
                    && self.content_type.is_none()
                    && self.taxonomy.is_none()
                    && self.selector_slug.is_none()
                    && self.entry_id.is_none()
                    && self.term_id.is_none()
                    && self.custom_key.is_none()
                    && self.layout_key.is_none() =>
            {
                Ok(format!("part:{}", self.part_key.as_deref().unwrap()))
            }
            TemplateCandidateKind::SingleCustom
                if self.custom_key.is_some()
                    && self.content_type.is_none()
                    && self.taxonomy.is_none()
                    && self.selector_slug.is_none()
                    && self.entry_id.is_none()
                    && self.term_id.is_none()
                    && self.layout_key.is_none()
                    && self.part_key.is_none() =>
            {
                Ok(format!(
                    "single_custom:{}",
                    self.custom_key.as_deref().unwrap()
                ))
            }
            TemplateCandidateKind::Single
                if no_other && self.taxonomy.is_none() && self.term_id.is_none() =>
            {
                match (&self.content_type, &self.selector_slug, self.entry_id) {
                    (None, None, None) => Ok("single".into()),
                    (Some(ty), None, None) => Ok(format!("single:type:{ty}")),
                    (Some(ty), Some(slug), None) => Ok(format!("single:type:{ty}:slug:{slug}")),
                    (Some(ty), None, Some(id)) => Ok(format!("single:type:{ty}:id:{id}")),
                    _ => Err("Invalid single candidate dimensions"),
                }
            }
            TemplateCandidateKind::Archive
                if no_other
                    && self.taxonomy.is_none()
                    && self.selector_slug.is_none()
                    && self.entry_id.is_none()
                    && self.term_id.is_none() =>
            {
                Ok(self
                    .content_type
                    .as_ref()
                    .map_or_else(|| "archive".to_string(), |ty| format!("archive:type:{ty}")))
            }
            TemplateCandidateKind::Taxonomy
                if no_other && self.content_type.is_none() && self.entry_id.is_none() =>
            {
                match (&self.taxonomy, &self.selector_slug, self.term_id) {
                    (None, None, None) => Ok("taxonomy".into()),
                    (Some(key), None, None) => Ok(format!("taxonomy:key:{key}")),
                    (Some(key), Some(slug), None) => Ok(format!("taxonomy:key:{key}:slug:{slug}")),
                    (Some(key), None, Some(id)) => Ok(format!("taxonomy:key:{key}:id:{id}")),
                    _ => Err("Invalid taxonomy candidate dimensions"),
                }
            }
            _ => Err("Invalid template candidate dimensions"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeTemplateDecl {
    pub kind: TemplateCandidateKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taxonomy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector_slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_types: Vec<String>,
    pub file: String,
}

impl ThemeTemplateDecl {
    pub fn candidate(&self) -> TemplateCandidate {
        TemplateCandidate {
            kind: self.kind,
            content_type: self.content_type.clone(),
            taxonomy: self.taxonomy.clone(),
            selector_slug: self.selector_slug.clone(),
            entry_id: None,
            term_id: None,
            custom_key: self.custom_key.clone(),
            layout_key: None,
            part_key: None,
        }
    }

    pub fn canonical_key(&self) -> Result<String, &'static str> {
        self.candidate().canonical_key()
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if matches!(
            self.kind,
            TemplateCandidateKind::PageLayout | TemplateCandidateKind::Part
        ) {
            return Err("Layout and part declarations use separate manifest fields");
        }
        self.canonical_key()?;
        if !valid_root_html(&self.file)
            || matches!(self.file.as_str(), "header.html" | "footer.html")
        {
            return Err("Template file must be a root-level HTML file");
        }
        if self.kind == TemplateCandidateKind::SingleCustom {
            if self
                .label
                .as_deref()
                .is_none_or(|label| !valid_label(label, 128))
                || self.allowed_types.is_empty()
                || self.allowed_types.len() > 64
                || self.allowed_types.iter().any(|key| !valid_snake_key(key))
                || self.allowed_types.iter().collect::<BTreeSet<_>>().len()
                    != self.allowed_types.len()
            {
                return Err("Invalid custom template label or allowed types");
            }
        } else if self.label.is_some() || !self.allowed_types.is_empty() {
            return Err("Custom template fields are only valid for single_custom");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageLayoutChrome {
    #[default]
    Default,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageLayoutDecl {
    pub key: String,
    pub label: String,
    pub file: String,
    #[serde(default)]
    pub chrome: PageLayoutChrome,
}

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
pub enum EnqueueCondition {
    #[default]
    All,
    Home,
    Page,
    Single,
    Archive,
    Taxonomy,
    NotFound,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeStyleEnqueueDecl {
    pub handle: String,
    pub src: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deps: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<EnqueueCondition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeScriptEnqueueDecl {
    pub handle: String,
    pub src: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deps: Vec<String>,
    #[serde(default)]
    pub location: ScriptLocation,
    #[serde(default)]
    pub strategy: ScriptStrategy,
    #[serde(default)]
    pub module: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<EnqueueCondition>,
}

fn default_theme_functions_api_version() -> u32 {
    crate::plugin::PLUGIN_ABI_VERSION
}

fn default_theme_hook_priority() -> i32 {
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
    pub settings_schema: Vec<crate::plugin::PluginSettingField>,
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
            // Validate declared entries (including invalid sqlite/http/network tokens)
            // before merging defaults, allowing content_read_private against effective reads.
            let mut merged = raw.clone();
            merged.content_read_all_public = true;
            crate::plugin::validate_permissions(&merged, &secret_keys)?;
        }
        let effective = self.effective_permissions();
        crate::plugin::validate_permissions(&effective, &secret_keys)?;
        crate::plugin::validate_actions(&self.actions, &effective)?;
        crate::plugin::validate_routes(&self.routes, &effective)?;
        Ok(())
    }
}

/// v3 is additive in S01: v2 manifests are still readable by existing callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeV3Manifest {
    pub manifest_version: u8,
    pub theme_id: String,
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_locale: Option<String>,
    #[serde(default)]
    pub templates: Vec<ThemeTemplateDecl>,
    #[serde(default)]
    pub parts: BTreeMap<String, String>,
    #[serde(default)]
    pub page_layouts: Vec<PageLayoutDecl>,
    #[serde(default)]
    pub patterns: Vec<ThemePatternDecl>,
    /// Explicit allowlist of static files within assets/. Omission means none.
    #[serde(default)]
    pub assets: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<ThemeStyleEnqueueDecl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<ThemeScriptEnqueueDecl>,
    #[serde(default)]
    pub options: Option<serde_json::Value>,
    #[serde(default)]
    pub menus: Option<serde_json::Value>,
    #[serde(default)]
    pub sample: Option<serde_json::Value>,
    #[serde(default)]
    pub declarations: Option<serde_json::Value>,
    #[serde(default)]
    pub field_groups: Option<serde_json::Value>,
    #[serde(default)]
    pub admin_extensions: Option<crate::admin_extensions::AdminExtensions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub functions: Option<ThemeFunctionsSpec>,
}

impl ThemeV3Manifest {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.manifest_version != 3 {
            return Err("Theme manifest_version must be 3");
        }
        validate_package_id(&self.theme_id)?;
        if !valid_label(&self.name, 128) || !validate_semver(&self.version) {
            return Err("Invalid theme name or SemVer version");
        }
        if let Some(locale) = self.default_locale.as_deref() {
            if !valid_lowercase_locale_code(locale) {
                return Err("Invalid theme default_locale");
            }
        }
        let mut targets = BTreeSet::new();
        for template in &self.templates {
            template.validate()?;
            if !targets.insert(template.canonical_key()?) {
                return Err("Duplicate canonical template target");
            }
        }
        for (key, file) in &self.parts {
            if !valid_kebab_segment_with_limit(key, 64)
                || match key.as_str() {
                    "header" => file != "header.html",
                    "footer" => file != "footer.html",
                    _ => !valid_subdir_file(file, "template-part/", ".html"),
                }
            {
                return Err("Invalid theme part key or file");
            }
        }
        let mut layout_keys = BTreeSet::new();
        for layout in &self.page_layouts {
            if !valid_snake_key(&layout.key)
                || !layout_keys.insert(layout.key.as_str())
                || !valid_label(&layout.label, 128)
                || !valid_subdir_file(&layout.file, "page-layouts/", ".html")
            {
                return Err("Invalid or duplicate page layout");
            }
        }
        let mut pattern_keys = BTreeSet::new();
        for pattern in &self.patterns {
            if !valid_snake_key(&pattern.key)
                || !pattern_keys.insert(pattern.key.as_str())
                || !valid_kebab_segment_with_limit(&pattern.category, 64)
                || !valid_label(&pattern.title, 128)
                || pattern
                    .description
                    .as_deref()
                    .is_some_and(|desc| desc.len() > 4096 || desc.chars().any(char::is_control))
                || !valid_subdir_file(&pattern.file, "patterns/", ".json")
                || pattern.preview_asset.as_deref().is_some_and(|path| {
                    !valid_v3_asset_path(path)
                        || !matches!(
                            path.rsplit_once('.').map(|(_, ext)| ext),
                            Some("png" | "jpg" | "jpeg" | "webp" | "gif")
                        )
                })
            {
                return Err("Invalid or duplicate theme pattern");
            }
        }
        let mut asset_paths = BTreeSet::new();
        for path in &self.assets {
            if !valid_v3_asset_path(path) || !asset_paths.insert(path.as_str()) {
                return Err("Invalid or duplicate theme asset path");
            }
        }
        self.enqueue_graphs(&asset_paths)?;
        if let Some(functions) = &self.functions {
            functions.validate()?;
        }
        Ok(())
    }

    /// Converts the optional `functions` block into a runtime `PluginManifest`
    /// with fixed entrypoint `"functions.js"` and active-theme default permissions.
    pub fn to_runtime_manifest(&self) -> Option<crate::plugin::PluginManifest> {
        let func = self.functions.as_ref()?;
        Some(crate::plugin::PluginManifest {
            id: self.theme_id.clone(),
            version: self.version.clone(),
            api_version: func.api_version,
            manifest_schema: 3,
            min_host_version: func.min_host_version.clone(),
            name: self.name.clone(),
            description: String::new(),
            entrypoint: "functions.js".into(),
            permissions: func.effective_permissions(),
            hooks: func.hooks.clone(),
            filters: func.filters.clone(),
            hook_priority: func.hook_priority,
            actions: func.actions.clone(),
            routes: func.routes.clone(),
            render_cache: func.render_cache,
            declarations: Default::default(),
            editor_panels: Vec::new(),
            settings_schema: func.settings_schema.clone(),
        })
    }

    fn enqueue_graphs(
        &self,
        asset_paths: &BTreeSet<&str>,
    ) -> Result<(EnqueueGraph, EnqueueGraph), &'static str> {
        if self.styles.len() > 64 || self.scripts.len() > 64 {
            return Err("Too many theme enqueue declarations");
        }
        for style in &self.styles {
            if !valid_v3_asset_path(&style.src)
                || !style.src.ends_with(".css")
                || !asset_paths.contains(style.src.as_str())
                || style.media.as_deref().is_some_and(|media| {
                    media.trim().is_empty()
                        || media.len() > 64
                        || !media
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b" (),:./-".contains(&c))
                })
            {
                return Err("Invalid theme style source or media");
            }
        }
        for script in &self.scripts {
            if !valid_v3_asset_path(&script.src)
                || !(script.src.ends_with(".js") || script.src.ends_with(".mjs"))
                || !asset_paths.contains(script.src.as_str())
            {
                return Err("Invalid theme script source");
            }
        }
        Ok((
            EnqueueGraph::new(
                self.styles
                    .iter()
                    .map(|item| (item.handle.as_str(), item.deps.as_slice())),
                true,
            )?,
            EnqueueGraph::new(
                self.scripts
                    .iter()
                    .map(|item| (item.handle.as_str(), item.deps.as_slice())),
                false,
            )?,
        ))
    }
}

const TAILWIND_HANDLE: &str = "opendash-tailwind";

/// Edges point from each declaration to its dependencies. Indices preserve
/// the manifest's declaration order, independently of handle spelling.
struct EnqueueGraph {
    deps: Vec<Vec<usize>>,
}

impl EnqueueGraph {
    fn new<'a>(
        items: impl Iterator<Item = (&'a str, &'a [String])>,
        allow_tailwind: bool,
    ) -> Result<Self, &'static str> {
        let items: Vec<_> = items.collect();
        let mut handles = BTreeMap::new();
        for (index, (handle, _)) in items.iter().enumerate() {
            if !valid_kebab_segment_with_limit(handle, 64)
                || *handle == TAILWIND_HANDLE
                || handles.insert(*handle, index).is_some()
            {
                return Err("Invalid or duplicate theme enqueue handle");
            }
        }
        let mut deps = vec![Vec::new(); items.len()];
        for (index, (_, names)) in items.iter().enumerate() {
            let mut seen = BTreeSet::new();
            for name in *names {
                if !seen.insert(name.as_str()) {
                    return Err("Duplicate theme enqueue dependency");
                }
                if allow_tailwind && name == TAILWIND_HANDLE {
                    continue;
                }
                let dep = handles
                    .get(name.as_str())
                    .ok_or("Unknown theme enqueue dependency")?;
                deps[index].push(*dep);
            }
        }
        // Three-color DFS checks every declaration, including routes which
        // would not select it. Depth is bounded by the manifest item cap.
        fn visit(index: usize, deps: &[Vec<usize>], colors: &mut [u8]) -> bool {
            match colors[index] {
                1 => return false,
                2 => return true,
                _ => {}
            }
            colors[index] = 1;
            if !deps[index].iter().all(|&dep| visit(dep, deps, colors)) {
                return false;
            }
            colors[index] = 2;
            true
        }
        let mut colors = vec![0; deps.len()];
        for index in 0..deps.len() {
            if !visit(index, &deps, &mut colors) {
                return Err("Cyclic theme enqueue dependencies");
            }
        }
        Ok(Self { deps })
    }

    fn closure(&self, roots: impl Iterator<Item = usize>) -> Vec<bool> {
        let mut selected = vec![false; self.deps.len()];
        let mut pending: Vec<_> = roots.collect();
        while let Some(index) = pending.pop() {
            if !selected[index] {
                selected[index] = true;
                pending.extend(self.deps[index].iter().copied());
            }
        }
        selected
    }

    fn ordered(&self, selected: &[bool]) -> Vec<usize> {
        let mut emitted = vec![false; self.deps.len()];
        let mut ordered = Vec::new();
        // Among eligible nodes choose the earliest declaration. All selected
        // dependencies are in the closure and the graph is already acyclic.
        while let Some(index) = (0..self.deps.len()).find(|&index| {
            selected[index] && !emitted[index] && self.deps[index].iter().all(|&dep| emitted[dep])
        }) {
            emitted[index] = true;
            ordered.push(index);
        }
        ordered
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedEnqueuedAssets {
    pub head_styles: Vec<String>,
    pub head_style_urls: Vec<String>,
    pub head_scripts: Vec<String>,
    pub footer_scripts: Vec<String>,
}

/// Resolve only declared, validated local assets into safe HTML tags.
pub fn resolve_enqueued_assets(
    manifest: &ThemeV3Manifest,
    revision_digest: &str,
    route_condition: EnqueueCondition,
) -> ResolvedEnqueuedAssets {
    let mut result = ResolvedEnqueuedAssets::default();
    if revision_digest.len() != 64
        || !revision_digest
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        || manifest.validate().is_err()
    {
        return result;
    }
    let asset_paths = manifest.assets.iter().map(String::as_str).collect();
    let Ok((styles, scripts)) = manifest.enqueue_graphs(&asset_paths) else {
        return result;
    };
    let matches_route = |conditions: &[EnqueueCondition]| {
        conditions.is_empty()
            || conditions.contains(&EnqueueCondition::All)
            || conditions.contains(&route_condition)
    };
    let selected_styles = styles.closure(
        manifest
            .styles
            .iter()
            .enumerate()
            .filter_map(|(index, item)| matches_route(&item.conditions).then_some(index)),
    );
    for index in styles.ordered(&selected_styles) {
        let style = &manifest.styles[index];
        let url = format!("/theme/{revision_digest}/{}", style.src);
        let media = style
            .media
            .as_ref()
            .map_or_else(String::new, |value| format!(" media=\"{value}\""));
        result
            .head_styles
            .push(format!("<link rel=\"stylesheet\" href=\"{url}\"{media}>"));
        result.head_style_urls.push(url);
    }
    let selected_scripts = scripts.closure(
        manifest
            .scripts
            .iter()
            .enumerate()
            .filter_map(|(index, item)| matches_route(&item.conditions).then_some(index)),
    );
    let head = scripts.closure(
        manifest
            .scripts
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                (selected_scripts[index] && item.location == ScriptLocation::Head).then_some(index)
            }),
    );
    let script_order = scripts.ordered(&selected_scripts);
    // Async execution may overtake its dependencies (or its dependents).
    // Keep async only for isolated handles; connected declarations execute
    // after parsing, in topological document order, like deferred scripts.
    let mut linked = vec![false; manifest.scripts.len()];
    for &index in &script_order {
        for &dep in &scripts.deps[index] {
            linked[index] = true;
            linked[dep] = true;
        }
    }
    let mut delayed = vec![false; manifest.scripts.len()];
    for index in script_order {
        let script = &manifest.scripts[index];
        // A parser-blocking dependent would run before a deferred/module
        // dependency, even if its tag appears later. Promote it to defer.
        delayed[index] = script.module
            || script.strategy == ScriptStrategy::Defer
            || (script.strategy == ScriptStrategy::Async && linked[index])
            || scripts.deps[index].iter().any(|&dep| delayed[dep]);
        let type_attr = if script.module {
            " type=\"module\""
        } else {
            ""
        };
        let strategy_attr = if script.strategy == ScriptStrategy::Async && !linked[index] {
            " async"
        } else if !script.module && delayed[index] {
            " defer"
        } else {
            ""
        };
        let tag = format!(
            "<script src=\"/theme/{revision_digest}/{}\"{type_attr}{strategy_attr}></script>",
            script.src
        );
        if head[index] {
            result.head_scripts.push(tag);
        } else {
            result.footer_scripts.push(tag);
        }
    }
    result
}

fn valid_label(value: &str, max_chars: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= max_chars
        && !value.chars().any(char::is_control)
}

fn valid_file_name(value: &str, suffix: &str) -> bool {
    value.strip_suffix(suffix).is_some_and(|stem| {
        (1..=96).contains(&stem.len())
            && stem
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_')
    })
}

fn valid_root_html(value: &str) -> bool {
    valid_file_name(value, ".html")
}

fn valid_subdir_file(value: &str, prefix: &str, suffix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|name| valid_file_name(name, suffix))
}

/// Validate a canonical theme v3 static-file path without accessing the ZIP.
///
/// The byte limit applies after "assets/"; directories use [a-z0-9_-]+,
/// filenames use [a-z0-9_.-] with a non-dot first byte, no ".." and only
/// the file extensions supported by the v3 archive reader.
pub fn valid_v3_asset_path(value: &str) -> bool {
    let Some(path) = value.strip_prefix("assets/") else {
        return false;
    };
    if path.is_empty() || path.len() > 240 || !path.is_ascii() {
        return false;
    }
    let mut components = path.split('/').peekable();
    while let Some(component) = components.next() {
        if components.peek().is_some() {
            if component.is_empty()
                || !component.bytes().all(|c| {
                    c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'-' | b'_')
                })
            {
                return false;
            }
        } else {
            if component.is_empty()
                || component.starts_with('.')
                || component.contains("..")
                || !component.bytes().all(|c| {
                    c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'-' | b'_' | b'.')
                })
            {
                return false;
            }
            let Some((stem, extension)) = component.rsplit_once('.') else {
                return false;
            };
            return !stem.is_empty()
                && matches!(
                    extension,
                    "css"
                        | "txt"
                        | "js"
                        | "mjs"
                        | "svg"
                        | "png"
                        | "jpg"
                        | "jpeg"
                        | "webp"
                        | "gif"
                        | "woff2"
                );
        }
    }
    false
}

/// Backwards-compatible alias: both server callers use identical D01 grammar.
pub fn valid_asset_path(value: &str) -> bool {
    valid_v3_asset_path(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn template(kind: TemplateCandidateKind, file: &str) -> ThemeTemplateDecl {
        ThemeTemplateDecl {
            kind,
            content_type: None,
            taxonomy: None,
            selector_slug: None,
            custom_key: None,
            label: None,
            allowed_types: Vec::new(),
            file: file.into(),
        }
    }

    fn v3_manifest() -> ThemeV3Manifest {
        ThemeV3Manifest {
            manifest_version: 3,
            theme_id: "ocean-theme".into(),
            name: "Ocean Theme".into(),
            version: "1.2.3-beta.1".into(),
            default_locale: None,
            templates: vec![
                template(TemplateCandidateKind::Index, "index.html"),
                template(TemplateCandidateKind::Page, "page.html"),
            ],
            parts: BTreeMap::from([
                ("header".into(), "header.html".into()),
                ("footer".into(), "footer.html".into()),
                (
                    "breadcrumbs".into(),
                    "template-part/breadcrumbs.html".into(),
                ),
            ]),
            page_layouts: vec![PageLayoutDecl {
                key: "canvas".into(),
                label: "Canvas".into(),
                file: "page-layouts/canvas.html".into(),
                chrome: PageLayoutChrome::Default,
            }],
            patterns: vec![ThemePatternDecl {
                key: "hero".into(),
                title: "Hero".into(),
                category: "landing".into(),
                description: Some("Welcome".into()),
                preview_asset: Some("assets/images/hero.webp".into()),
                file: "patterns/hero.json".into(),
            }],
            assets: vec!["assets/images/hero.webp".into(), "assets/styles.css".into()],
            styles: Vec::new(),
            scripts: Vec::new(),
            options: None,
            menus: None,
            sample: None,
            declarations: None,
            field_groups: None,
            admin_extensions: None,
            functions: None,
        }
    }

    #[test]
    fn v3_manifest_accepts_valid_resources_and_rejects_wrong_version_or_unknown_fields() {
        let manifest = v3_manifest();
        manifest.validate().unwrap();
        let decoded: ThemeV3Manifest =
            serde_json::from_value(serde_json::to_value(&manifest).unwrap()).unwrap();
        decoded.validate().unwrap();

        let mut wrong = manifest.clone();
        wrong.manifest_version = 2;
        assert!(wrong.validate().is_err());
        wrong = manifest.clone();
        wrong.theme_id = "../evil".into();
        assert!(wrong.validate().is_err());
        wrong = manifest.clone();
        wrong.version = "01.2.3".into();
        assert!(wrong.validate().is_err());
        let mut unknown = serde_json::to_value(manifest).unwrap();
        unknown["unexpected"] = json!("value");
        assert!(serde_json::from_value::<ThemeV3Manifest>(unknown).is_err());
    }

    #[test]
    fn page_layout_chrome_is_backward_compatible_and_explicit() {
        let mut old = serde_json::to_value(v3_manifest()).unwrap();
        old["page_layouts"][0]
            .as_object_mut()
            .unwrap()
            .remove("chrome");
        let parsed: ThemeV3Manifest = serde_json::from_value(old).unwrap();
        assert_eq!(parsed.page_layouts[0].chrome, PageLayoutChrome::Default);
        let mut declared = serde_json::to_value(parsed).unwrap();
        declared["page_layouts"][0]["chrome"] = json!("none");
        let parsed: ThemeV3Manifest = serde_json::from_value(declared.clone()).unwrap();
        assert_eq!(parsed.page_layouts[0].chrome, PageLayoutChrome::None);
        parsed.validate().unwrap();
        declared["page_layouts"][0]["chrome"] = json!("unknown");
        assert!(serde_json::from_value::<ThemeV3Manifest>(declared).is_err());
    }

    #[test]
    fn v3_assets_missing_field_defaults_to_empty_and_explicit_empty_is_valid() {
        let mut without_assets = serde_json::to_value(v3_manifest()).unwrap();
        without_assets.as_object_mut().unwrap().remove("assets");
        let omitted: ThemeV3Manifest = serde_json::from_value(without_assets).unwrap();
        assert!(omitted.assets.is_empty());
        omitted.validate().unwrap();

        let mut explicit_empty = v3_manifest();
        explicit_empty.assets.clear();
        explicit_empty.validate().unwrap();
        let roundtrip: ThemeV3Manifest =
            serde_json::from_value(serde_json::to_value(&explicit_empty).unwrap()).unwrap();
        assert_eq!(roundtrip.assets, Vec::<String>::new());

        let mut declared = v3_manifest();
        declared.assets = vec![
            "assets/styles.css".into(),
            "assets/images/logo.v2.png".into(),
            "assets/fonts/headline.woff2".into(),
            "assets/README.txt".to_ascii_lowercase(),
        ];
        declared.validate().unwrap();
        let decoded: ThemeV3Manifest =
            serde_json::from_value(serde_json::to_value(&declared).unwrap()).unwrap();
        assert_eq!(decoded.assets, declared.assets);
    }

    #[test]
    fn v3_asset_path_grammar_accepts_only_safe_canonical_files() {
        for path in [
            "assets/x.css",
            "assets/note.txt",
            "assets/other.svg",
            "assets/other.js",
            "assets/other.mjs",
            "assets/photo.png",
            "assets/photo.jpg",
            "assets/photo.jpeg",
            "assets/photo.webp",
            "assets/animation.gif",
            "assets/fonts/theme.woff2",
            "assets/images_2/ui-state.v2.png",
            "assets/_.png",
            "assets/_hidden-style.css",
        ] {
            assert!(valid_v3_asset_path(path), "should accept: {path}");
            assert_eq!(valid_asset_path(path), valid_v3_asset_path(path));
        }
        let exactly_240 = format!("assets/{}.png", "a".repeat(236));
        assert_eq!(exactly_240.len() - "assets/".len(), 240);
        assert!(valid_v3_asset_path(&exactly_240));
        let too_long = format!("assets/{}.png", "a".repeat(237));
        assert!(!valid_v3_asset_path(&too_long));

        for path in [
            "",
            "assets",
            "assets/",
            "assets/logo",
            "assets/.env",
            "assets/.private/logo.png",
            "assets/icons/.logo.png",
            "assets/icons.v2/logo.png",
            "assets/a..b.png",
            "assets/a../logo.png",
            "assets/a..b/logo.png",
            "assets/../logo.png",
            "assets/./logo.png",
            "assets//logo.png",
            "assets/logos//logo.png",
            "assets/logos/",
            "/assets/logo.png",
            "assets\\logo.png",
            "assets/logos\\logo.png",
            "assets/logo.PnG",
            "assets/Logo.png",
            "assets/other.wasm",
            "assets/page.html",
            "assets/archive.zip",
            "assets/img.png?foo=bar",
            "assets/img.png#fragment",
            "https://example.test/assets/logo.png",
            "assets/ü.png",
            "assets/test\n.png",
            "assets/img.png.",
        ] {
            assert!(!valid_v3_asset_path(path), "should reject: {path:?}");
            assert_eq!(valid_asset_path(path), valid_v3_asset_path(path));
        }
    }

    #[test]
    fn v3_manifest_rejects_duplicate_and_noncanonical_asset_declarations() {
        let baseline = v3_manifest();
        baseline.validate().unwrap();
        for bad in [
            "assets/.hidden/logo.png",
            "assets/icons.v2/logo.png",
            "assets/a..b.png",
            "assets/payload.wasm",
            "assets/icons/../../logo.png",
            "assets/icons//logo.png",
            "assets/img.PNG",
        ] {
            let mut manifest = baseline.clone();
            manifest.assets.push(bad.into());
            assert!(manifest.validate().is_err(), "{bad}");
        }
        let mut duplicated = baseline.clone();
        duplicated.assets.push("assets/styles.css".into());
        assert!(duplicated.validate().is_err());

        let mut invalid_preview = baseline.clone();
        invalid_preview.patterns[0].preview_asset = Some("assets/icons.v2/logo.png".into());
        assert!(invalid_preview.validate().is_err());
    }

    fn enqueue_manifest() -> ThemeV3Manifest {
        let mut value = serde_json::to_value(v3_manifest()).unwrap();
        value["assets"] = json!([
            "assets/base.css",
            "assets/layout.css",
            "assets/home.css",
            "assets/print.css",
            "assets/base.js",
            "assets/middle.mjs",
            "assets/head.js",
            "assets/footer.js",
            "assets/async.mjs",
            "assets/other.js",
            "assets/icon.svg"
        ]);
        value["styles"] = json!([
            {"handle":"home", "src":"assets/home.css", "deps":["layout"], "conditions":["home"]},
            {"handle":"print", "src":"assets/print.css", "media":"(min-aspect-ratio: 16/9)", "conditions":["all"]},
            {"handle":"layout", "src":"assets/layout.css", "deps":["base"], "conditions":["page"]},
            {"handle":"base", "src":"assets/base.css", "deps":["opendash-tailwind"], "conditions":["single"]}
        ]);
        value["scripts"] = json!([
            {"handle":"head", "src":"assets/head.js", "deps":["middle","base"], "location":"head", "strategy":"blocking", "conditions":["home"]},
            {"handle":"footer", "src":"assets/footer.js", "deps":["base"]},
            {"handle":"middle", "src":"assets/middle.mjs", "deps":["base"], "module":true, "conditions":["page"]},
            {"handle":"base", "src":"assets/base.js", "conditions":["single"]},
            {"handle":"async", "src":"assets/async.mjs", "module":true, "strategy":"async"},
            {"handle":"other", "src":"assets/other.js", "strategy":"async", "conditions":["archive"]}
        ]);
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn enqueue_defaults_and_strict_fields_are_backward_compatible() {
        let mut value = serde_json::to_value(v3_manifest()).unwrap();
        assert!(value.get("styles").is_none());
        assert!(value.get("scripts").is_none());
        let legacy: ThemeV3Manifest = serde_json::from_value(value.clone()).unwrap();
        assert!(legacy.styles.is_empty() && legacy.scripts.is_empty());
        legacy.validate().unwrap();
        value["styles"] = json!([{"handle":"main", "src":"assets/styles.css"}]);
        value["scripts"] = json!([{"handle":"main", "src":"assets/main.js"}]);
        let parsed: ThemeV3Manifest = serde_json::from_value(value.clone()).unwrap();
        assert!(parsed.styles[0].conditions.is_empty());
        assert!(parsed.styles[0].media.is_none());
        assert_eq!(parsed.scripts[0].location, ScriptLocation::Footer);
        assert_eq!(parsed.scripts[0].strategy, ScriptStrategy::Defer);
        assert!(!parsed.scripts[0].module);
        value["scripts"][0]["strategy"] = json!("unknown");
        assert!(serde_json::from_value::<ThemeV3Manifest>(value.clone()).is_err());
        value["scripts"][0]["strategy"] = json!("defer");
        value["styles"][0]["unknown"] = json!(true);
        assert!(serde_json::from_value::<ThemeV3Manifest>(value).is_err());
    }

    #[test]
    fn enqueue_resolves_route_transitive_dependencies_and_promotes_head_scripts() {
        let manifest = enqueue_manifest();
        manifest.validate().unwrap();
        let digest = "a".repeat(64);
        let url = |src: &str| format!("/theme/{digest}/assets/{src}");
        let css = |src: &str, attrs: &str| {
            format!("<link rel=\"stylesheet\" href=\"{}\"{attrs}>", url(src))
        };
        let js = |src: &str, attrs: &str| format!("<script src=\"{}\"{attrs}></script>", url(src));
        let home = resolve_enqueued_assets(&manifest, &digest, EnqueueCondition::Home);
        assert_eq!(
            home.head_style_urls,
            vec![
                url("print.css"),
                url("base.css"),
                url("layout.css"),
                url("home.css")
            ]
        );
        assert_eq!(
            home.head_styles,
            vec![
                css("print.css", " media=\"(min-aspect-ratio: 16/9)\""),
                css("base.css", ""),
                css("layout.css", ""),
                css("home.css", "")
            ]
        );
        assert_eq!(
            home.head_scripts,
            vec![
                js("base.js", " defer"),
                js("middle.mjs", " type=\"module\""),
                js("head.js", " defer")
            ]
        );
        assert_eq!(
            home.footer_scripts,
            vec![
                js("footer.js", " defer"),
                js("async.mjs", " type=\"module\" async")
            ]
        );
        let archive = resolve_enqueued_assets(&manifest, &digest, EnqueueCondition::Archive);
        assert_eq!(archive.head_style_urls, vec![url("print.css")]);
        assert!(archive.head_scripts.is_empty());
        assert_eq!(
            archive.footer_scripts,
            vec![
                js("base.js", " defer"),
                js("footer.js", " defer"),
                js("async.mjs", " type=\"module\" async"),
                js("other.js", " async")
            ]
        );
        let page = resolve_enqueued_assets(&manifest, &digest, EnqueueCondition::Page);
        assert_eq!(
            page.head_style_urls,
            vec![url("print.css"), url("base.css"), url("layout.css")]
        );
        assert!(page.head_scripts.is_empty());
        assert_eq!(
            page.footer_scripts,
            vec![
                js("base.js", " defer"),
                js("footer.js", " defer"),
                js("middle.mjs", " type=\"module\""),
                js("async.mjs", " type=\"module\" async")
            ]
        );
        for invalid in [
            "a".repeat(63),
            "A".repeat(64),
            "g".repeat(64),
            format!("{}\"", "a".repeat(63)),
        ] {
            assert_eq!(
                resolve_enqueued_assets(&manifest, &invalid, EnqueueCondition::Home),
                ResolvedEnqueuedAssets::default()
            );
        }
    }

    #[test]
    fn enqueue_preserves_execution_order_across_async_defer_and_blocking_dependencies() {
        let digest = "a".repeat(64);
        let mut manifest = enqueue_manifest();
        manifest.scripts[1].strategy = ScriptStrategy::Blocking;
        manifest.scripts[5].deps = vec!["async".into()];
        manifest.validate().unwrap();

        // A blocking footer script cannot execute ahead of its deferred
        // dependency. Connected async scripts must also execute in order.
        let archive = resolve_enqueued_assets(&manifest, &digest, EnqueueCondition::Archive);
        assert_eq!(
            archive.footer_scripts,
            vec![
                format!("<script src=\"/theme/{digest}/assets/base.js\" defer></script>"),
                format!("<script src=\"/theme/{digest}/assets/footer.js\" defer></script>"),
                format!(
                    "<script src=\"/theme/{digest}/assets/async.mjs\" type=\"module\"></script>"
                ),
                format!("<script src=\"/theme/{digest}/assets/other.js\" defer></script>"),
            ]
        );

        // The archive-only dependent must not alter the independent async
        // script's strategy on a different route.
        let home = resolve_enqueued_assets(&manifest, &digest, EnqueueCondition::Home);
        assert!(
            home.footer_scripts
                .iter()
                .any(|tag| tag.contains("async.mjs\" type=\"module\" async"))
        );
        assert!(
            home.head_scripts
                .last()
                .is_some_and(|tag| tag.contains("head.js\" defer"))
        );
    }

    #[test]
    fn enqueue_validates_sources_handles_media_and_entire_dependency_graph() {
        let baseline = enqueue_manifest();
        baseline.validate().unwrap();
        let mut cases = Vec::new();
        let mut m = baseline.clone();
        m.styles[0].handle = "opendash-tailwind".into();
        cases.push(m);
        let mut m = baseline.clone();
        m.scripts[0].handle = "opendash-tailwind".into();
        cases.push(m);
        let mut m = baseline.clone();
        m.styles[0].handle = "Invalid_handle".into();
        cases.push(m);
        let mut m = baseline.clone();
        m.scripts[0].handle = "a".repeat(65);
        cases.push(m);
        let mut m = baseline.clone();
        m.styles[0].handle = m.styles[1].handle.clone();
        cases.push(m);
        let mut m = baseline.clone();
        m.scripts[0].handle = m.scripts[1].handle.clone();
        cases.push(m);
        for src in [
            "assets/missing.css",
            "assets/base.js",
            "assets/../base.css",
            "https://example.com/base.css",
        ] {
            let mut m = baseline.clone();
            m.styles[0].src = src.into();
            cases.push(m);
        }
        for src in ["assets/missing.js", "assets/base.css", "assets/icon.svg"] {
            let mut m = baseline.clone();
            m.scripts[0].src = src.into();
            cases.push(m);
        }
        for media in [
            " ".into(),
            "a".repeat(65),
            "screen\"".into(),
            "screen\n".into(),
            "é".into(),
            "a&b".into(),
        ] {
            let mut m = baseline.clone();
            m.styles[0].media = Some(media);
            cases.push(m);
        }
        for deps in [
            vec!["missing"],
            vec!["layout", "layout"],
            vec!["home"],
            vec!["opendash-tailwind", "opendash-tailwind"],
            vec!["footer"],
        ] {
            let mut m = baseline.clone();
            m.styles[0].deps = deps.into_iter().map(String::from).collect();
            cases.push(m);
        }
        for deps in [
            vec!["missing"],
            vec!["base", "base"],
            vec!["head"],
            vec!["opendash-tailwind"],
            vec!["layout"],
        ] {
            let mut m = baseline.clone();
            m.scripts[0].deps = deps.into_iter().map(String::from).collect();
            cases.push(m);
        }
        let mut m = baseline.clone();
        m.styles[3].deps.push("home".into());
        cases.push(m);
        let mut m = baseline.clone();
        m.scripts[3].deps.push("head".into());
        cases.push(m);
        let mut m = baseline.clone();
        m.styles = (0..65)
            .map(|i| ThemeStyleEnqueueDecl {
                handle: format!("style-{i}"),
                src: "assets/base.css".into(),
                deps: vec![],
                media: None,
                conditions: vec![],
            })
            .collect();
        cases.push(m);
        let mut m = baseline.clone();
        m.scripts = (0..65)
            .map(|i| ThemeScriptEnqueueDecl {
                handle: format!("script-{i}"),
                src: "assets/base.js".into(),
                deps: vec![],
                location: ScriptLocation::Footer,
                strategy: ScriptStrategy::Defer,
                module: false,
                conditions: vec![],
            })
            .collect();
        cases.push(m);
        for (i, invalid) in cases.iter().enumerate() {
            assert!(invalid.validate().is_err(), "invalid case {i}");
            assert_eq!(
                resolve_enqueued_assets(invalid, &"0".repeat(64), EnqueueCondition::NotFound),
                ResolvedEnqueuedAssets::default(),
                "invalid case {i}"
            );
        }
        let mut valid = baseline;
        valid.styles[0].media = Some("(min-resolution: 1.5dppx)".into());
        valid.validate().unwrap();
    }

    #[test]
    fn pattern_preview_remains_raster_only() {
        for ext in [
            "png", "jpg", "jpeg", "webp", "gif", "js", "mjs", "svg", "css", "txt", "woff2",
        ] {
            let mut manifest = v3_manifest();
            let path = format!("assets/preview.{ext}");
            manifest.assets.push(path.clone());
            manifest.patterns[0].preview_asset = Some(path);
            assert_eq!(
                manifest.validate().is_ok(),
                matches!(ext, "png" | "jpg" | "jpeg" | "webp" | "gif"),
                "{ext}"
            );
        }
    }

    #[test]
    fn v3_manifest_enforces_unique_canonical_template_targets() {
        let mut manifest = v3_manifest();
        let mut typed = template(TemplateCandidateKind::Single, "single-post.html");
        typed.content_type = Some("post".into());
        manifest.templates.push(typed.clone());
        assert!(manifest.validate().is_ok());
        typed.file = "single-duplicate.html".into();
        manifest.templates.push(typed);
        assert!(manifest.validate().is_err());

        manifest.templates.pop();
        let mut slugged = template(TemplateCandidateKind::Single, "single-post-intro.html");
        slugged.content_type = Some("post".into());
        slugged.selector_slug = Some("intro".into());
        manifest.templates.push(slugged);
        assert!(manifest.validate().is_ok());
        manifest.templates.last_mut().unwrap().selector_slug = Some("../intro".into());
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn candidate_keys_cover_all_dimensions_and_reject_ambiguous_targets() {
        use TemplateCandidateKind as Kind;
        for (kind, key) in [
            (Kind::Home, "home"),
            (Kind::Page, "page"),
            (Kind::Single, "single"),
            (Kind::Archive, "archive"),
            (Kind::Taxonomy, "taxonomy"),
            (Kind::Singular, "singular"),
            (Kind::Index, "index"),
            (Kind::NotFound, "not_found"),
        ] {
            assert_eq!(TemplateCandidate::new(kind).canonical_key().unwrap(), key);
        }
        let mut candidate = TemplateCandidate::new(Kind::Single);
        candidate.content_type = Some("post".into());
        assert_eq!(candidate.canonical_key().unwrap(), "single:type:post");
        candidate.selector_slug = Some("hello-world".into());
        assert_eq!(
            candidate.canonical_key().unwrap(),
            "single:type:post:slug:hello-world"
        );
        candidate.selector_slug = None;
        candidate.entry_id = Some(42);
        assert_eq!(candidate.canonical_key().unwrap(), "single:type:post:id:42");
        candidate.selector_slug = Some("hello-world".into());
        assert!(candidate.canonical_key().is_err());
        candidate.entry_id = Some(0);
        candidate.selector_slug = None;
        assert!(candidate.canonical_key().is_err());

        let mut candidate = TemplateCandidate::new(Kind::Archive);
        candidate.content_type = Some("project".into());
        assert_eq!(candidate.canonical_key().unwrap(), "archive:type:project");
        candidate.kind = Kind::Page;
        assert!(candidate.canonical_key().is_err());

        let mut candidate = TemplateCandidate::new(Kind::Taxonomy);
        candidate.taxonomy = Some("category".into());
        assert_eq!(candidate.canonical_key().unwrap(), "taxonomy:key:category");
        candidate.selector_slug = Some("news".into());
        assert_eq!(
            candidate.canonical_key().unwrap(),
            "taxonomy:key:category:slug:news"
        );
        candidate.selector_slug = None;
        candidate.term_id = Some(7);
        assert_eq!(
            candidate.canonical_key().unwrap(),
            "taxonomy:key:category:id:7"
        );
        candidate.content_type = Some("post".into());
        assert!(candidate.canonical_key().is_err());

        let mut candidate = TemplateCandidate::new(Kind::SingleCustom);
        candidate.custom_key = Some("feature".into());
        assert_eq!(candidate.canonical_key().unwrap(), "single_custom:feature");
        let mut candidate = TemplateCandidate::new(Kind::PageLayout);
        candidate.layout_key = Some("full_width".into());
        assert_eq!(candidate.canonical_key().unwrap(), "page_layout:full_width");
        let mut candidate = TemplateCandidate::new(Kind::Part);
        candidate.part_key = Some("post-card".into());
        assert_eq!(candidate.canonical_key().unwrap(), "part:post-card");
        candidate.part_key = Some("header:injected".into());
        assert!(candidate.canonical_key().is_err());
    }

    #[test]
    fn v3_rejects_paths_outside_their_declared_directories() {
        let valid = v3_manifest();
        for path in [
            "templates/index.html",
            "../index.html",
            "/index.html",
            "index.htm",
            "page-layouts/canvas.html",
            "index.html/other",
        ] {
            let mut manifest = valid.clone();
            manifest.templates[0].file = path.into();
            assert!(manifest.validate().is_err(), "template: {path}");
        }
        for (key, path) in [
            ("header", "template-part/header.html"),
            ("footer", "footer-other.html"),
            ("breadcrumbs", "header.html"),
            ("breadcrumbs", "template-part/../secret.html"),
            ("breadcrumbs", "template-part/deep/a.html"),
        ] {
            let mut manifest = valid.clone();
            manifest.parts.insert(key.into(), path.into());
            assert!(manifest.validate().is_err(), "part: {key}/{path}");
        }
        for path in [
            "canvas.html",
            "../canvas.html",
            "page-layouts/sub/canvas.html",
        ] {
            let mut manifest = valid.clone();
            manifest.page_layouts[0].file = path.into();
            assert!(manifest.validate().is_err(), "layout: {path}");
        }
        for path in [
            "hero.json",
            "patterns/../hero.json",
            "patterns/inner/hero.json",
        ] {
            let mut manifest = valid.clone();
            manifest.patterns[0].file = path.into();
            assert!(manifest.validate().is_err(), "pattern: {path}");
        }
        for path in [
            "/assets/hero.webp",
            "assets/../secret",
            "assets/.secret",
            "assets/images//hero.webp",
            "https://evil.test/icon.webp",
        ] {
            let mut manifest = valid.clone();
            manifest.patterns[0].preview_asset = Some(path.into());
            assert!(manifest.validate().is_err(), "asset: {path}");
        }
    }

    #[test]
    fn v3_rejects_duplicate_keys_and_invalid_custom_templates() {
        let valid = v3_manifest();
        let mut manifest = valid.clone();
        manifest.page_layouts.push(manifest.page_layouts[0].clone());
        assert!(manifest.validate().is_err());
        let mut manifest = valid.clone();
        manifest.patterns.push(manifest.patterns[0].clone());
        assert!(manifest.validate().is_err());
        let mut manifest = valid.clone();
        manifest
            .parts
            .insert("bad:key".into(), "template-part/other.html".into());
        assert!(manifest.validate().is_err());

        let mut manifest = valid.clone();
        let mut custom = template(TemplateCandidateKind::SingleCustom, "single-custom.html");
        custom.custom_key = Some("landing".into());
        custom.label = Some("Landing".into());
        custom.allowed_types = vec!["post".into(), "project".into()];
        manifest.templates.push(custom);
        assert!(manifest.validate().is_ok());
        manifest
            .templates
            .last_mut()
            .unwrap()
            .allowed_types
            .push("post".into());
        assert!(manifest.validate().is_err());
        manifest.templates.last_mut().unwrap().allowed_types.pop();
        manifest.templates.last_mut().unwrap().label = None;
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_block_descriptor_and_values_validation() {
        let mut fields = Fields::new();
        fields.insert("title".into(), "Tiêu đề".into());
        fields.insert("note_1".into(), "Mô tả".into());

        let desc = BlockDescriptor {
            id: "sample-theme/hero-banner".into(),
            fields: fields.clone(),
        };
        assert!(desc.validate().is_ok());

        let bad_desc = BlockDescriptor {
            id: "Invalid/Block".into(),
            fields: fields.clone(),
        };
        assert!(bad_desc.validate().is_err());

        let mut values = Fields::new();
        values.insert("title".into(), "Xin chào".into());
        values.insert("note_1".into(), "Nội dung".into());
        let block = Block {
            kind: "sample-theme/hero-banner".into(),
            values,
        };
        assert!(block.validate_against_fields(&fields).is_ok());

        let empty_block = Block {
            kind: "sample-theme/hero-banner".into(),
            values: Fields::new(),
        };
        assert!(empty_block.validate_against_fields(&fields).is_err());
    }
}

#[cfg(test)]
mod scaffold_validation_tests {
    use super::valid_block_key;

    #[test]
    fn requires_valid_namespace_and_name() {
        assert!(valid_block_key("core/heading"));
        assert!(!valid_block_key("heading"));
        assert!(!valid_block_key("core/"));
        assert!(!valid_block_key("core/heading/extra"));
    }
}
