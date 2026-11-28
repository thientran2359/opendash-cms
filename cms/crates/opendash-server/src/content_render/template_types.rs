//! Strongly typed render context models shared across S02–S03 public routes and preview paths.

use serde::Deserialize;

use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydratedMedia {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub thumbnail_url: String,
    #[serde(default)]
    pub alt: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    #[serde(default)]
    pub kind: String,
    #[serde(default = "default_media_version")]
    pub version: i64,
    #[serde(default)]
    pub variants: Vec<opendash_core::media::MediaVariantView>,
    #[serde(default)]
    pub webp_srcset: Option<String>,
    #[serde(default)]
    pub fallback_srcset: Option<String>,
}

fn default_media_version() -> i64 {
    1
}

impl HydratedMedia {
    pub fn from_value(val: &Value) -> Option<Self> {
        serde_json::from_value(val.clone()).ok()
    }

    pub fn from_row(row: &crate::storage::fields::HydratedMediaRow, is_preview: bool) -> Self {
        let val = crate::field_hydrate::hydrate_media_row(row, is_preview);
        Self {
            id: row.id.clone(),
            url: val
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            thumbnail_url: val
                .get("thumbnail_url")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            alt: String::new(),
            width: row.width.and_then(|w| u32::try_from(w).ok()),
            height: row.height.and_then(|h| u32::try_from(h).ok()),
            kind: row.kind.clone(),
            version: row.version.max(1),
            variants: val
                .get("variants")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            webp_srcset: val
                .get("webp_srcset")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            fallback_srcset: val
                .get("fallback_srcset")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
        }
    }

    pub fn width_i64(&self) -> Option<i64> {
        self.width.map(i64::from)
    }

    pub fn height_i64(&self) -> Option<i64> {
        self.height.map(i64::from)
    }

    pub fn as_picture_input<'a>(
        &'a self,
        sizes: &'a str,
        class: &'a str,
        alt_override: Option<&'a str>,
        loading: &'a str,
        fetchpriority: &'a str,
    ) -> opendash_core::media::ResponsivePictureInput<'a> {
        let alt = alt_override
            .filter(|s| !s.is_empty())
            .unwrap_or(self.alt.as_str());
        opendash_core::media::ResponsivePictureInput {
            url: self.url.as_str(),
            width: self.width_i64(),
            height: self.height_i64(),
            webp_srcset: self.webp_srcset.as_deref(),
            fallback_srcset: self.fallback_srcset.as_deref(),
            sizes,
            class,
            alt,
            loading,
            decoding: "async",
            fetchpriority,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdjacentEntryItem {
    pub id: i64,
    pub title: String,
    pub slug: String,
    pub url: Option<String>,
    pub cover_url: Option<String>,
    pub cover: Option<HydratedMedia>,
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdjacentEntriesView {
    pub prev: Option<AdjacentEntryItem>,
    pub next: Option<AdjacentEntryItem>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArchiveTypeSummary {
    pub key: String,
    pub label: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TermRenderSummary {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub slug: String,
    pub taxonomy_label: String,
    pub description: String,
    pub featured_image: Option<HydratedMedia>,
    pub fields: BTreeMap<String, Value>,
}

impl TermRenderSummary {
    pub fn from_detail(detail: &PublicTermDetailView, taxonomy_label: String) -> Self {
        Self {
            id: detail.id,
            kind: detail.kind.clone(),
            name: detail.name.clone(),
            slug: detail.slug.clone(),
            taxonomy_label,
            description: detail.description.clone(),
            featured_image: detail
                .featured_image
                .as_ref()
                .and_then(HydratedMedia::from_value),
            fields: detail.fields.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocaleAlternateItem {
    pub code: String,
    pub bcp47_tag: String,
    pub label: String,
    pub url: String,
    pub is_current: bool,
    pub has_translation: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocaleRenderContext {
    pub code: String,
    pub bcp47_tag: String,
    pub label: String,
    pub direction: String,
    pub home_url: String,
    pub search_url: String,
    pub alternates: Vec<LocaleAlternateItem>,
}

pub struct TemplateRenderData<'a> {
    pub site: &'a SiteContext<'a>,
    pub site_logo: Option<&'a HydratedMedia>,
    pub locale: &'a LocaleRenderContext,
    pub menus_by_slot: &'a BTreeMap<String, crate::storage::menu::MenuViewModel>,
    pub menus_by_key: &'a BTreeMap<String, crate::storage::menu::MenuViewModel>,
    pub breadcrumbs: &'a [BreadcrumbItem],
    pub pagination: Option<&'a PaginationView>,
    pub entry: Option<&'a EntryView>,
    pub entry_schema: Option<&'a EffectiveContentSchema>,
    pub entry_body_html: Option<&'a str>,
    pub entry_has_builder_body: bool,
    pub adjacent_entries_global: Option<&'a AdjacentEntriesView>,
    pub adjacent_entries_same_taxonomy: Option<&'a AdjacentEntriesView>,
    pub related_entries: &'a [EntryView],
    pub archive_entries: &'a [EntryView],
    pub content_type: Option<&'a ArchiveTypeSummary>,
    pub term: Option<&'a TermRenderSummary>,
    pub is_preview: bool,
}
