//! Public CPT HTML view model. Data is never evaluated as a MiniJinja template.
//! Rich-text stays untrusted text and is HTML-escaped, not marked |safe.

use std::collections::BTreeMap;

use opendash_core::content::{ContentFieldType, ContentTypeDefinition, PublicExposure};
use serde::Serialize;
use serde_json::Value;

use crate::{
    error::{ApiError, ApiResult},
    public_context::CanonicalUrlBuilder,
    storage::{entries::ContentEntryRecord, media::valid_media_id},
    theme,
};

pub const DEFAULT_ARCHIVE: &str = r##"<!doctype html>
<html lang="vi"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{{ content_type.label }} — {{ site.title }}</title>
</head><body><main class="mx-auto max-w-4xl px-4 py-8 text-od-text">
<header class="mb-8"><strong class="text-xl font-od-heading font-semibold">{{ site.title }}</strong>{% if site.tagline %}<p class="text-od-muted">{{ site.tagline }}</p>{% endif %}</header>
<h1 class="mb-6 text-3xl font-od-heading font-bold">{{ content_type.label }}</h1>
{% for entry in entries %}
<article class="od-prose border-b border-od-border py-5"><h2>{% if entry.url %}<a href="{{ entry.url }}">{{ entry.title }}</a>{% else %}{{ entry.title }}{% endif %}</h2>
{% if entry.cover_url %}{% if entry.url %}<a href="{{ entry.url }}">{% endif %}<img class="max-h-64 max-w-full rounded-lg object-contain" src="{{ entry.cover_url }}" alt="Ảnh minh họa {{ entry.title }}" loading="lazy">{% if entry.url %}</a>{% endif %}{% endif %}
</article>
{% else %}<p>Chưa có nội dung.</p>{% endfor %}
<nav class="mt-8 flex gap-4">{% if previous_url %}<a href="{{ previous_url }}">← Trang trước</a>{% endif %}
{% if next_url %}<a href="{{ next_url }}">Trang sau →</a>{% endif %}</nav>
</main></body></html>"##;

pub const DEFAULT_SINGLE: &str = r##"<!doctype html>
<html lang="vi"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{{ entry.title }} — {{ site.title }}</title>
</head><body><main class="mx-auto max-w-4xl px-4 py-8 text-od-text">
<header class="mb-8"><strong class="text-xl font-od-heading font-semibold">{{ site.title }}</strong>{% if site.tagline %}<p class="text-od-muted">{{ site.tagline }}</p>{% endif %}</header>
{% if archive_url %}<nav class="mb-6"><a class="text-od-primary hover:underline" href="{{ archive_url }}">← {{ content_type.label }}</a></nav>{% endif %}
<article class="od-prose"><h1>{{ entry.title }}</h1>
<dl class="mt-8">{% for field in entry.fields %}
{% if field.text or field.image_url or field.gallery_urls %}
<dt class="mt-6 font-semibold">{{ field.label }}</dt>
{% if field.image_url %}<dd class="my-2"><img class="h-auto max-w-full rounded-lg" src="{{ field.image_url }}" alt="{{ field.label }}" loading="lazy"></dd>
{% elif field.gallery_urls %}<dd class="my-2 flex flex-wrap gap-3">{% for url in field.gallery_urls %}<img class="h-36 w-48 rounded-lg object-contain" src="{{ url }}" alt="{{ field.label }}" loading="lazy">{% endfor %}</dd>
{% else %}<dd class="my-2 whitespace-pre-wrap break-words">{{ field.text }}</dd>{% endif %}
{% endif %}
{% endfor %}</dl>
</article></main></body></html>"##;

#[derive(Debug, Serialize)]
pub struct FieldView {
    pub key: String,
    pub label: String,
    pub kind: &'static str,
    pub text: String,
    pub image_url: Option<String>,
    pub gallery_urls: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct EntryView {
    pub id: i64,
    pub title: String,
    pub slug: String,
    pub url: Option<String>,
    pub updated_at: String,
    pub values: BTreeMap<String, Value>,
    pub fields: Vec<FieldView>,
    pub cover_url: Option<String>,
}

/// Only typed site strings enter the custom-template context; MiniJinja
/// performs HTML escaping just like every other content value.
#[derive(Debug, Serialize)]
pub struct SiteContext<'a> {
    pub title: &'a str,
    pub tagline: &'a str,
}

const DEFAULT_SITE: SiteContext<'static> = SiteContext {
    title: "OpenDash",
    tagline: "",
};

fn media_url(value: &Value) -> Option<String> {
    let id = value.as_str()?;
    valid_media_id(id).then(|| format!("/media/{id}/file"))
}

pub fn entry_view(record: ContentEntryRecord, schema: &ContentTypeDefinition) -> EntryView {
    entry_view_with_url(record, schema, None)
}

pub fn entry_view_with_url(
    record: ContentEntryRecord,
    schema: &ContentTypeDefinition,
    url: Option<String>,
) -> EntryView {
    let mut cover_url = None;
    let mut public_values = BTreeMap::new();
    let mut fields = Vec::with_capacity(schema.fields.len());
    for definition in &schema.fields {
        if definition.public_exposure != PublicExposure::Public {
            continue;
        }
        let value = record.values.get(&definition.key).unwrap_or(&Value::Null);
        let extracted_rich = if definition.field_type == ContentFieldType::RichText {
            value.as_str().map(|s| {
                if opendash_core::builder_blocks::contains_blocks(s) {
                    opendash_core::builder_blocks::extract_visible_text(s)
                } else {
                    s.to_owned()
                }
            })
        } else {
            None
        };
        if let Some(ref clean) = extracted_rich {
            public_values.insert(definition.key.clone(), Value::String(clean.clone()));
        } else if !value.is_null() {
            public_values.insert(definition.key.clone(), value.clone());
        }
        let kind = match definition.field_type {
            ContentFieldType::ShortText => "short_text",
            ContentFieldType::LongText => "long_text",
            ContentFieldType::RichText => "rich_text",
            ContentFieldType::Number => "number",
            ContentFieldType::Boolean => "boolean",
            ContentFieldType::Date => "date",
            ContentFieldType::Select => "select",
            ContentFieldType::Image => "image",
            ContentFieldType::Gallery => "gallery",
        };
        let image_url = if definition.field_type == ContentFieldType::Image {
            media_url(value)
        } else {
            None
        };
        if cover_url.is_none() {
            cover_url = image_url.clone();
        }
        let gallery_urls = if definition.field_type == ContentFieldType::Gallery {
            value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(media_url)
                .collect()
        } else {
            Vec::new()
        };
        let text = match definition.field_type {
            ContentFieldType::Image | ContentFieldType::Gallery => String::new(),
            ContentFieldType::Boolean if value.is_boolean() => if value == &Value::Bool(true) {
                "Có"
            } else {
                "Không"
            }
            .to_string(),
            _ => match (extracted_rich, value) {
                (Some(clean), _) => clean,
                (None, Value::Null) => String::new(),
                (None, Value::String(text)) => text.clone(),
                (None, other) => other.to_string(),
            },
        };
        fields.push(FieldView {
            key: definition.key.clone(),
            label: definition.label.clone(),
            kind,
            text,
            image_url,
            gallery_urls,
        });
    }
    EntryView {
        id: record.id,
        title: record.title,
        slug: record.slug,
        url,
        updated_at: record.updated_at,
        values: public_values,
        fields,
        cover_url,
    }
}

/// Draft preview runs behind an authenticated Admin route. In contrast to
/// public entry rendering, media must resolve through authenticated URLs.
pub fn authenticated_preview_view(
    record: ContentEntryRecord,
    schema: &ContentTypeDefinition,
) -> EntryView {
    let mut view = entry_view(record, schema);
    let admin_media = |url: &str| url.replacen("/media/", "/api/admin/media/", 1);
    view.cover_url = view.cover_url.as_deref().map(admin_media);
    for field in &mut view.fields {
        field.image_url = field.image_url.as_deref().map(admin_media);
        field.gallery_urls = field
            .gallery_urls
            .iter()
            .map(|url| admin_media(url))
            .collect();
    }
    view.url = Some(format!(
        "/api/admin/content-types/{}/entries/{}/preview",
        schema.key, view.id
    ));
    view
}

pub fn render_role_fragment_with_options(
    template: &str,
    site: &SiteContext<'_>,
    options: &BTreeMap<String, Value>,
) -> Option<String> {
    render_role_fragment_with_context(template, site, options, &BTreeMap::new())
}

pub fn render_role_fragment_with_context(
    template: &str,
    site: &SiteContext<'_>,
    options: &BTreeMap<String, Value>,
    menus: &BTreeMap<String, crate::storage::menu::MenuViewModel>,
) -> Option<String> {
    theme::render_content(
        template,
        &serde_json::json!({
            "site": site,
            "options": options,
            "menus": menus,
        }),
    )
    .ok()
}

fn escape_html_attr(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Normalizes header/body/footer through the bounded document normalizer and emits
/// a single trusted `<head>` from `SeoView`. Replaces legacy literal composition.
pub fn compose_document_with_seo(
    header_html: Option<&str>,
    rendered_layout: &str,
    footer_html: Option<&str>,
    seo: &crate::seo::SeoView,
) -> ApiResult<String> {
    let mut styles = Vec::new();
    let mut body_attrs_str = String::new();
    let body_fragment = match crate::document::normalize_public_document(rendered_layout) {
        Ok(parts) => {
            for (k, v) in parts.body_attributes {
                body_attrs_str.push(' ');
                body_attrs_str.push_str(&k);
                body_attrs_str.push_str("=\"");
                body_attrs_str.push_str(&escape_html_attr(&v));
                body_attrs_str.push('"');
            }
            styles.extend(parts.authorized_styles);
            parts.body_fragment
        }
        Err(diag) => {
            format!(
                "<main data-doc-diagnostic=\"{}\"><p>Nội dung mẫu không hợp lệ.</p><pre>{}</pre></main>",
                escape_html_attr(diag.code),
                escape_html_attr(rendered_layout),
            )
        }
    };
    let normalize_slot = |slot: Option<&str>, styles: &mut Vec<String>| -> String {
        let Some(raw) = slot.filter(|s| !s.is_empty()) else {
            return String::new();
        };
        match crate::document::normalize_public_document(raw) {
            Ok(parts) => {
                styles.extend(parts.authorized_styles);
                parts.body_fragment
            }
            Err(_) => String::new(),
        }
    };
    let header = normalize_slot(header_html, &mut styles);
    let footer = normalize_slot(footer_html, &mut styles);

    let mut head = String::from(
        "<meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">",
    );
    head.push_str(&format!("<title>{}</title>", escape_html_attr(&seo.title)));
    head.push_str(&format!(
        "<meta name=\"robots\" content=\"{}\">",
        escape_html_attr(&seo.robots)
    ));
    if let Some(desc) = &seo.description {
        head.push_str(&format!(
            "<meta name=\"description\" content=\"{}\">",
            escape_html_attr(desc)
        ));
    }
    if let Some(canonical) = &seo.canonical_url {
        head.push_str(&format!(
            "<link rel=\"canonical\" href=\"{}\">",
            escape_html_attr(canonical)
        ));
    }
    for alt in &seo.alternates {
        head.push_str(&format!(
            "<link rel=\"alternate\" hreflang=\"{}\" href=\"{}\">",
            escape_html_attr(&alt.hreflang),
            escape_html_attr(&alt.href)
        ));
    }
    head.push_str(&format!(
        "<meta property=\"og:title\" content=\"{}\">",
        escape_html_attr(&seo.og_title)
    ));
    if let Some(og_desc) = &seo.og_description {
        head.push_str(&format!(
            "<meta property=\"og:description\" content=\"{}\">",
            escape_html_attr(og_desc)
        ));
    }
    if let Some(og_url) = &seo.og_url {
        head.push_str(&format!(
            "<meta property=\"og:url\" content=\"{}\">",
            escape_html_attr(og_url)
        ));
    }
    if let Some(og_img) = &seo.og_image_url {
        head.push_str(&format!(
            "<meta property=\"og:image\" content=\"{}\">",
            escape_html_attr(og_img)
        ));
    }
    if !styles
        .iter()
        .any(|style| style.contains("/assets/opendash-tailwind.css"))
    {
        head.push_str("<link rel=\"stylesheet\" href=\"/assets/opendash-tailwind.css\">");
    }
    for style in styles {
        head.push_str(&style);
    }
    let dir = if seo.dir == "rtl" { "rtl" } else { "ltr" };
    Ok(format!(
        "<!doctype html><html lang=\"{}\" dir=\"{}\"><head>{head}</head><body{body_attrs_str}>{header}{body_fragment}{footer}</body></html>",
        escape_html_attr(&seo.lang),
        dir
    ))
}

pub fn render_archive_with_site(
    schema: &ContentTypeDefinition,
    entries: &[EntryView],
    previous_url: Option<&str>,
    next_url: Option<&str>,
    selected_template: Option<&str>,
    site: &SiteContext<'_>,
) -> ApiResult<String> {
    let data = serde_json::json!({
        "content_type": schema,
        "entries": entries,
        "previous_url": previous_url,
        "next_url": next_url,
        "site": site,
    });
    if let Some(custom) = selected_template {
        if let Ok(html) = theme::render_content(custom, &data) {
            return Ok(html);
        }
    }
    theme::render_content(DEFAULT_ARCHIVE, &data).map_err(ApiError::internal)
}

pub fn render_single(
    schema: &ContentTypeDefinition,
    entry: &EntryView,
    selected_template: Option<&str>,
) -> ApiResult<String> {
    render_single_with_site(schema, entry, selected_template, &DEFAULT_SITE)
}

pub fn render_single_with_site(
    schema: &ContentTypeDefinition,
    entry: &EntryView,
    selected_template: Option<&str>,
    site: &SiteContext<'_>,
) -> ApiResult<String> {
    // Admin preview/legacy callers have no public route context. Public routes
    // must supply the storage-resolved, visibility-checked archive explicitly.
    let fallback = CanonicalUrlBuilder::archive(&schema.key);
    render_single_with_site_and_archive(schema, entry, selected_template, site, fallback.as_deref())
}

/// Public rendering must never synthesize a legacy archive link when the
/// archive is private, disabled or has a configured permalink rewrite.
pub fn render_single_with_site_and_archive(
    schema: &ContentTypeDefinition,
    entry: &EntryView,
    selected_template: Option<&str>,
    site: &SiteContext<'_>,
    archive_url: Option<&str>,
) -> ApiResult<String> {
    let data = serde_json::json!({
        "content_type": schema,
        "entry": entry,
        "archive_url": archive_url,
        "site": site,
    });
    if let Some(custom) = selected_template {
        if let Ok(html) = theme::render_content(custom, &data) {
            return Ok(html);
        }
    }
    theme::render_content(DEFAULT_SINGLE, &data).map_err(ApiError::internal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendash_core::content::{ContentFieldDefinition, ContentFieldType};

    #[test]
    fn public_document_injects_one_tailwind_stylesheet_without_legacy_body_width() {
        let seo = crate::seo::SeoView::for_noindex_page("vi", "Trang thử nghiệm");
        let html = compose_document_with_seo(
            Some("<header>Header</header>"),
            "<main class=\"od-prose\"><article>Classic Editor</article></main>",
            Some("<footer>Footer</footer>"),
            &seo,
        )
        .unwrap();
        let stylesheet = "<link rel=\"stylesheet\" href=\"/assets/opendash-tailwind.css\">";
        assert_eq!(html.matches(stylesheet).count(), 1);
        assert!(html.contains("<head>"));
        assert!(html.contains("<main class=\"od-prose\">"));
        assert!(!DEFAULT_SINGLE.contains("max-width:850px"));
        assert!(!DEFAULT_ARCHIVE.contains("max-width:850px"));
        assert!(!html.contains("max-width:850px"));
    }

    #[test]
    fn escapes_untrusted_title_rich_text_and_template_tokens() {
        let schema = ContentTypeDefinition {
            key: "post".into(),
            label: "Bài viết".into(),
            settings: opendash_core::content::ContentTypeSettings::builtin_publication(),
            fields: vec![ContentFieldDefinition {
                key: "body".into(),
                label: "Nội dung".into(),
                field_type: ContentFieldType::RichText,
                required: false,
                options: vec![],
                ..Default::default()
            }],
        };
        let record = ContentEntryRecord {
            id: 1,
            content_type_key: "post".into(),
            slug: "hello".into(),
            title: "<script>alert(1)</script>".into(),
            values: [(
                "body".into(),
                Value::String("<img src=x onerror=alert(2)> {{ 7*7 }}".into()),
            )]
            .into(),
            term_ids: vec![],
            primary_category_term_id: None,
            publication_status: "draft".into(),
            page_layout_key: None,
            custom_template_key: None,
            content_mode: opendash_core::content::ContentMode::Editor,
            version: 1,
            created_at: "".into(),
            updated_at: "".into(),
        };
        let html = render_single(&schema, &entry_view(record, &schema), None).unwrap();
        assert!(html.contains("&lt;script&gt;alert(1)&lt;&#x2f;script&gt;"));
        assert!(html.contains("&lt;img src=x onerror=alert(2)&gt;"));
        assert!(html.contains("{{ 7*7 }}"));
        assert!(!html.contains("<script>alert(1)</script>"));
    }

    #[test]
    fn site_context_is_html_escaped_in_default_and_custom_templates() {
        let schema = ContentTypeDefinition {
            key: "news".into(),
            label: "Tin tức".into(),
            fields: vec![],
            settings: Default::default(),
        };
        let site = SiteContext {
            title: "<script>alert(1)</script>",
            tagline: "<img src=x onerror=alert(2)>",
        };
        let default_html = render_archive_with_site(&schema, &[], None, None, None, &site).unwrap();
        assert!(default_html.contains("&lt;script&gt;alert(1)&lt;"));
        assert!(default_html.contains("&lt;img src=x onerror=alert(2)&gt;"));
        assert!(!default_html.contains("<script>"));
        let custom_html = render_archive_with_site(
            &schema,
            &[],
            None,
            None,
            Some("<h1>{{ site.title }}</h1><p>{{ site.tagline }}</p>"),
            &site,
        )
        .unwrap();
        assert!(custom_html.contains("&lt;script&gt;"));
        assert!(!custom_html.contains("<img"));
    }

    #[test]
    fn public_entry_without_resolved_href_never_forges_a_legacy_url() {
        let schema = ContentTypeDefinition {
            key: "post".into(),
            label: "Posts".into(),
            fields: vec![],
            settings: Default::default(),
        };
        let entry = ContentEntryRecord {
            id: 17,
            content_type_key: "post".into(),
            slug: "old-slug".into(),
            title: "Safe title".into(),
            values: BTreeMap::new(),
            term_ids: vec![],
            primary_category_term_id: None,
            publication_status: "published".into(),
            page_layout_key: None,
            custom_template_key: None,
            content_mode: opendash_core::content::ContentMode::Editor,
            version: 1,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let unresolved = entry_view_with_url(entry.clone(), &schema, None);
        assert_eq!(unresolved.url, None);
        let html =
            render_single_with_site_and_archive(&schema, &unresolved, None, &DEFAULT_SITE, None)
                .unwrap();
        assert!(!html.contains("content/post"));
        let resolved = entry_view_with_url(entry, &schema, Some("/blog/new-slug".into()));
        assert_eq!(resolved.url.as_deref(), Some("/blog/new-slug"));
        let html = render_single_with_site_and_archive(
            &schema,
            &resolved,
            None,
            &DEFAULT_SITE,
            Some("/blog"),
        )
        .unwrap();
        assert!(html.contains("href=\"/blog\"") || html.contains("href=\"&#x2f;blog\""));
        assert!(!html.contains("content/post"));
    }
}
