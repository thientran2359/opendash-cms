//! Public CPT HTML view model. Data is never evaluated as a MiniJinja template.
//! Rich-text stays untrusted text and is HTML-escaped, not marked |safe.

use std::collections::BTreeMap;

use opendash_core::content::{ContentFieldType, ContentTypeDefinition};
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
<style>
body{font:16px/1.6 system-ui,sans-serif;margin:0 auto;padding:2rem 1rem;max-width:850px;color:#172033;overflow-wrap:anywhere;box-sizing:border-box}
a{color:#155e9b}article{padding:1rem 0;border-bottom:1px solid #d8dfe9}
img{max-width:100%;height:auto;max-height:260px;object-fit:contain;border-radius:8px}
nav{display:flex;gap:1rem;margin-top:2rem}
</style></head><body><main>
<header><strong>{{ site.title }}</strong>{% if site.tagline %}<p>{{ site.tagline }}</p>{% endif %}</header>
<h1>{{ content_type.label }}</h1>
{% for entry in entries %}
<article><h2>{% if entry.url %}<a href="{{ entry.url }}">{{ entry.title }}</a>{% else %}{{ entry.title }}{% endif %}</h2>
{% if entry.cover_url %}{% if entry.url %}<a href="{{ entry.url }}">{% endif %}<img src="{{ entry.cover_url }}" alt="Ảnh minh họa {{ entry.title }}" loading="lazy">{% if entry.url %}</a>{% endif %}{% endif %}
</article>
{% else %}<p>Chưa có nội dung.</p>{% endfor %}
<nav>{% if previous_url %}<a href="{{ previous_url }}">← Trang trước</a>{% endif %}
{% if next_url %}<a href="{{ next_url }}">Trang sau →</a>{% endif %}</nav>
</main></body></html>"##;

pub const DEFAULT_SINGLE: &str = r##"<!doctype html>
<html lang="vi"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{{ entry.title }} — {{ site.title }}</title>
<style>
body{font:16px/1.6 system-ui,sans-serif;margin:0 auto;padding:2rem 1rem;max-width:850px;color:#172033;overflow-wrap:anywhere;box-sizing:border-box}
a{color:#155e9b}dl{margin-top:2rem}dt{font-weight:700;margin-top:1.5rem}
dd{margin:0.3rem 0 1rem;white-space:pre-wrap;overflow-wrap:anywhere}
img{max-width:100%;height:auto;border-radius:8px}
.gallery{display:flex;flex-wrap:wrap;gap:0.75rem}.gallery img{width:190px;height:150px;object-fit:contain}
</style></head><body><main>
<header><strong>{{ site.title }}</strong>{% if site.tagline %}<p>{{ site.tagline }}</p>{% endif %}</header>
<nav><a href="{{ archive_url }}">← {{ content_type.label }}</a></nav>
<article><h1>{{ entry.title }}</h1>
<dl>{% for field in entry.fields %}
{% if field.text or field.image_url or field.gallery_urls %}
<dt>{{ field.label }}</dt>
{% if field.image_url %}<dd><img src="{{ field.image_url }}" alt="{{ field.label }}" loading="lazy"></dd>
{% elif field.gallery_urls %}<dd class="gallery">{% for url in field.gallery_urls %}<img src="{{ url }}" alt="{{ field.label }}" loading="lazy">{% endfor %}</dd>
{% else %}<dd>{{ field.text }}</dd>{% endif %}
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
    let mut fields = Vec::with_capacity(schema.fields.len());
    for definition in &schema.fields {
        let value = record.values.get(&definition.key).unwrap_or(&Value::Null);
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
            _ => match value {
                Value::Null => String::new(),
                Value::String(text) => text.clone(),
                other => other.to_string(),
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
    let url = url.or_else(|| CanonicalUrlBuilder::entry_from_id(record.id, &schema.key, &record.slug));
    EntryView {
        id: record.id,
        title: record.title,
        slug: record.slug,
        url,
        updated_at: record.updated_at,
        values: record.values,
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

/// Composes optional header and footer fragments with the body HTML once in
/// core, never via raw HTML interpolation inside MiniJinja templates.
pub fn compose_public_document(
    header_html: Option<&str>,
    body_html: &str,
    footer_html: Option<&str>,
) -> String {
    let header = header_html.unwrap_or("");
    let footer = footer_html.unwrap_or("");
    if header.is_empty() && footer.is_empty() {
        return body_html.to_string();
    }
    if let Some(body_open) = body_html.find("<body>") {
        let insert_start = body_open + "<body>".len();
        if let Some(body_close) = body_html.rfind("</body>") {
            let mut out = String::with_capacity(body_html.len() + header.len() + footer.len());
            out.push_str(&body_html[..insert_start]);
            out.push_str(header);
            out.push_str(&body_html[insert_start..body_close]);
            out.push_str(footer);
            out.push_str(&body_html[body_close..]);
            return out;
        }
    }
    format!(
        "<!doctype html><html lang=\"vi\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"></head><body>{header}{body_html}{footer}</body></html>"
    )
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
    let data = serde_json::json!({
        "content_type": schema,
        "entry": entry,
        "archive_url": CanonicalUrlBuilder::archive(&schema.key).unwrap_or_default(),
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
    fn escapes_untrusted_title_rich_text_and_template_tokens() {
        let schema = ContentTypeDefinition {
            key: "post".into(),
            label: "Bài viết".into(),
            fields: vec![ContentFieldDefinition {
                key: "body".into(),
                label: "Nội dung".into(),
                field_type: ContentFieldType::RichText,
                required: false,
                options: vec![],
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
            publication_status: "draft".into(),
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
}
