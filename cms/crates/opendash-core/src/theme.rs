use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::id::{
    valid_kebab_segment_with_limit, valid_namespaced_block_id, valid_namespaced_snake_key,
    valid_snake_key, valid_snake_key_with_limit, validate_package_id, validate_semver,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageLayoutDecl {
    pub key: String,
    pub label: String,
    pub file: String,
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

/// v3 is additive in S01: v2 manifests are still readable by existing callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeV3Manifest {
    pub manifest_version: u8,
    pub theme_id: String,
    pub name: String,
    pub version: String,
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
                || pattern
                    .preview_asset
                    .as_deref()
                    .is_some_and(|path| !valid_v3_asset_path(path))
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
        Ok(())
    }
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
/// the inert file extensions supported by the v3 archive reader.
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
                    "css" | "txt" | "png" | "jpg" | "jpeg" | "webp" | "gif" | "woff2"
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
            options: None,
            menus: None,
            sample: None,
            declarations: None,
            field_groups: None,
            admin_extensions: None,
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
            "assets/other.svg",
            "assets/other.js",
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
            "assets/payload.js",
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
