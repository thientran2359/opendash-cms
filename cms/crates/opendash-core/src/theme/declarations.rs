use super::*;

pub(super) use std::collections::{BTreeMap, BTreeSet};

pub(super) use serde::{Deserialize, Serialize};

pub(super) use crate::id::{
    valid_kebab_segment_with_limit, valid_lowercase_locale_code, valid_namespaced_snake_key,
    valid_snake_key, validate_package_id, validate_semver,
};

/// A block key is namespaced, e.g. `core/heading`.
pub fn valid_block_key(value: &str) -> bool {
    valid_namespaced_snake_key(value)
}

/// An ordered resolver target. IDs are site-local and must never appear in a
/// packaged theme declaration; they are reserved for site-owned overrides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateCandidateKind {
    Page,
    PageLayout,
    Single,
    SingleCustom,
    Archive,
    Taxonomy,
    Singular,
    Index,
    NotFound,
    Search,
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
            TemplateCandidateKind::Page if only_plain => Ok("page".into()),
            TemplateCandidateKind::Singular if only_plain => Ok("singular".into()),
            TemplateCandidateKind::Index if only_plain => Ok("index".into()),
            TemplateCandidateKind::NotFound if only_plain => Ok("not_found".into()),
            TemplateCandidateKind::Search if only_plain => Ok("search".into()),
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

pub const THEME_MANIFEST_VERSION: u8 = 1;

/// Clean-slate Theme Format V1 manifest (`theme.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeManifest {
    pub manifest_version: u8,
    pub theme_id: String,
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_domain: Option<String>,
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
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub options: Vec<crate::field_schema::FieldSpec>,
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

/// Validate a canonical Theme Format V1 static-file path without accessing the ZIP.
///
/// The byte limit applies after "assets/"; directories use [a-z0-9_-]+,
/// filenames use [a-z0-9_.-] with a non-dot first byte, no ".." and only
/// the file extensions supported by the V1 archive reader.
pub fn valid_theme_asset_path(value: &str) -> bool {
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

/// Backwards-compatible alias: callers use identical D01 grammar.
pub fn valid_asset_path(value: &str) -> bool {
    valid_theme_asset_path(value)
}
