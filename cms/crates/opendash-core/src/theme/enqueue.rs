use super::*;

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
    Search,
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

impl ThemeManifest {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.manifest_version != THEME_MANIFEST_VERSION {
            return Err("Theme manifest_version must be 1");
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
        if let Some(domain) = self.text_domain.as_deref() {
            crate::l10n::validate_extension_text_domain(domain)
                .map_err(|_| "Invalid theme text_domain")?;
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
                    !valid_theme_asset_path(path)
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
            if !valid_theme_asset_path(path) || !asset_paths.insert(path.as_str()) {
                return Err("Invalid or duplicate theme asset path");
            }
        }
        self.enqueue_graphs(&asset_paths)?;
        validate_theme_options_fields(&self.options)?;
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
            text_domain: self.text_domain.clone(),
            settings_schema: func.settings_schema.clone(),
        })
    }

    pub(super) fn enqueue_graphs(
        &self,
        asset_paths: &BTreeSet<&str>,
    ) -> Result<(EnqueueGraph, EnqueueGraph), &'static str> {
        if self.styles.len() > 64 || self.scripts.len() > 64 {
            return Err("Too many theme enqueue declarations");
        }
        for style in &self.styles {
            if !valid_theme_asset_path(&style.src)
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
            if !valid_theme_asset_path(&script.src)
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

pub(super) const TAILWIND_HANDLE: &str = "opendash-tailwind";

/// Edges point from each declaration to its dependencies. Indices preserve
/// the manifest's declaration order, independently of handle spelling.
pub(super) struct EnqueueGraph {
    pub(super) deps: Vec<Vec<usize>>,
}

impl EnqueueGraph {
    pub(super) fn new<'a>(
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

    pub(super) fn closure(&self, roots: impl Iterator<Item = usize>) -> Vec<bool> {
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

    pub(super) fn ordered(&self, selected: &[bool]) -> Vec<usize> {
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
    manifest: &ThemeManifest,
    revision_digest: &str,
    route_condition: EnqueueCondition,
) -> ResolvedEnqueuedAssets {
    let mut result = ResolvedEnqueuedAssets::default();
    let hex_part = revision_digest
        .strip_prefix("dev-")
        .unwrap_or(revision_digest);
    if hex_part.len() != 64
        || !hex_part
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
