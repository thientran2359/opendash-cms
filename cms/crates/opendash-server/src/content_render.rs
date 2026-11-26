//! Public CPT HTML view model. Data is never evaluated as a MiniJinja template.
//! Rich-text stays untrusted text and is HTML-escaped, not marked |safe.

use std::collections::{BTreeMap, BTreeSet};

use opendash_core::content::{
    ContentFieldDefinition, ContentFieldType, ContentTypeDefinition, EffectiveContentSchema,
    EffectiveFieldStatus, FieldRole, PublicExposure,
};
use opendash_core::theme::ResolvedEnqueuedAssets;
use serde::Serialize;
use serde_json::Value;

use crate::{
    error::{ApiError, ApiResult},
    public_context::CanonicalUrlBuilder,
    storage::{entries::ContentEntryRecord, media::valid_media_id},
    template_sec::RenderFormattingConfig,
    theme,
};

#[derive(Debug, Clone, Copy, Serialize)]
pub struct CoreL10n {
    pub lang: &'static str,
    pub yes: &'static str,
    pub no: &'static str,
    pub empty_content: &'static str,
    pub previous_page: &'static str,
    pub next_page: &'static str,
    pub invalid_template_content: &'static str,
    pub image_alt_prefix: &'static str,
    pub list_unavailable: &'static str,
    pub page_empty: &'static str,
    pub back_to_first_page: &'static str,
    pub list_prev: &'static str,
    pub list_next: &'static str,
    pub contact_unavailable: &'static str,
    pub pattern_missing: &'static str,
    pub select_placeholder: &'static str,
    pub select_empty: &'static str,
    pub home_unconfigured: &'static str,
    pub not_found_title: &'static str,
    pub contact_error_title: &'static str,
    pub contact_error_heading: &'static str,
    pub contact_received_title: &'static str,
    pub contact_received_heading: &'static str,
    pub contact_received_body: &'static str,
}

const CORE_L10N_VI: CoreL10n = CoreL10n {
    lang: "vi",
    yes: "Có",
    no: "Không",
    empty_content: "Chưa có nội dung.",
    previous_page: "← Trang trước",
    next_page: "Trang sau →",
    invalid_template_content: "Nội dung mẫu không hợp lệ.",
    image_alt_prefix: "Ảnh minh họa ",
    list_unavailable: "Danh sách chưa khả dụng.",
    page_empty: "Trang này không có nội dung.",
    back_to_first_page: "Về trang đầu",
    list_prev: "Trang trước",
    list_next: "Trang sau",
    contact_unavailable: "Biểu mẫu liên hệ hiện không khả dụng.",
    pattern_missing: "Pattern không còn tồn tại.",
    select_placeholder: "-- Chọn --",
    select_empty: "Để trống",
    home_unconfigured: "Chưa cấu hình trang chủ.",
    not_found_title: "404 — Không tìm thấy trang",
    contact_error_title: "Lỗi gửi biểu mẫu",
    contact_error_heading: "Không thể gửi biểu mẫu",
    contact_received_title: "Đã nhận liên hệ",
    contact_received_heading: "Cảm ơn bạn đã liên hệ",
    contact_received_body: "Thông tin của bạn đã được tiếp nhận.",
};

const CORE_L10N_EN: CoreL10n = CoreL10n {
    lang: "en",
    yes: "Yes",
    no: "No",
    empty_content: "No content yet.",
    previous_page: "← Previous",
    next_page: "Next →",
    invalid_template_content: "Invalid template content.",
    image_alt_prefix: "Illustration ",
    list_unavailable: "List is currently unavailable.",
    page_empty: "This page has no content.",
    back_to_first_page: "Back to first page",
    list_prev: "Previous",
    list_next: "Next",
    contact_unavailable: "Contact form is currently unavailable.",
    pattern_missing: "Pattern no longer exists.",
    select_placeholder: "-- Select --",
    select_empty: "Leave blank",
    home_unconfigured: "Homepage is not configured.",
    not_found_title: "404 — Page not found",
    contact_error_title: "Form submission error",
    contact_error_heading: "Unable to submit form",
    contact_received_title: "Message received",
    contact_received_heading: "Thank you for contacting us",
    contact_received_body: "Your message has been received.",
};

/// Returns built-in Core fallback strings (`vi` for Vietnamese, `en` fallback for all other locales).
pub fn core_l10n(locale_code: &str) -> &'static CoreL10n {
    let lower = locale_code.trim().to_ascii_lowercase();
    if lower == "vi" || lower.starts_with("vi-") {
        &CORE_L10N_VI
    } else {
        &CORE_L10N_EN
    }
}

pub const DEFAULT_ARCHIVE: &str = r##"<!doctype html>
<html lang="{{ l10n.lang }}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{{ content_type.label }} — {{ site.title }}</title>
</head><body><main class="mx-auto max-w-4xl px-4 py-8 text-od-text">
<header class="mb-8"><strong class="text-xl font-od-heading font-semibold">{{ site.title }}</strong>{% if site.tagline %}<p class="text-od-muted">{{ site.tagline }}</p>{% endif %}</header>
<h1 class="mb-6 text-3xl font-od-heading font-bold">{{ content_type.label }}</h1>
{% for entry in entries %}
<article class="od-prose border-b border-od-border py-5"><h2>{% if entry.url %}<a href="{{ entry.url }}">{{ entry.title }}</a>{% else %}{{ entry.title }}{% endif %}</h2>
{% if entry.cover_url %}{% if entry.url %}<a href="{{ entry.url }}">{% endif %}<img class="max-h-64 max-w-full rounded-lg object-contain" src="{{ entry.cover_url }}" alt="{{ l10n.image_alt_prefix }}{{ entry.title }}" loading="lazy">{% if entry.url %}</a>{% endif %}{% endif %}
</article>
{% else %}<p>{{ l10n.empty_content }}</p>{% endfor %}
<nav class="mt-8 flex gap-4">{% if previous_url %}<a href="{{ previous_url }}">{{ l10n.previous_page }}</a>{% endif %}
{% if next_url %}<a href="{{ next_url }}">{{ l10n.next_page }}</a>{% endif %}</nav>
</main></body></html>"##;

pub const DEFAULT_SINGLE: &str = r##"<!doctype html>
<html lang="{{ l10n.lang }}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
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

#[derive(Debug, Clone, Serialize)]
pub struct FieldView {
    pub key: String,
    pub label: String,
    pub kind: &'static str,
    pub text: String,
    pub image_url: Option<String>,
    pub gallery_urls: Vec<String>,
}

pub const DEFAULT_BREADCRUMB_HOME_LABEL: &str = "Trang chủ";
pub const DEFAULT_BREADCRUMB_HOME_URL: &str = "/";
pub const DEFAULT_BREADCRUMB_NOT_FOUND_LABEL: &str = "404";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PublicTermView {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub slug: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BreadcrumbItem {
    pub name: String,
    pub url: Option<String>,
    pub is_current: bool,
}

pub fn home_breadcrumbs(home_label: &str, home_url: Option<&str>) -> Vec<BreadcrumbItem> {
    vec![BreadcrumbItem {
        name: home_label.to_owned(),
        url: home_url.map(str::to_owned),
        is_current: true,
    }]
}

pub fn archive_breadcrumbs(
    home_label: &str,
    home_url: Option<&str>,
    content_type_label: &str,
    archive_url: Option<String>,
) -> Vec<BreadcrumbItem> {
    vec![
        BreadcrumbItem {
            name: home_label.to_owned(),
            url: home_url.map(str::to_owned),
            is_current: false,
        },
        BreadcrumbItem {
            name: content_type_label.to_owned(),
            url: archive_url,
            is_current: true,
        },
    ]
}

pub fn taxonomy_breadcrumbs(
    home_label: &str,
    home_url: Option<&str>,
    mut ancestors: Vec<BreadcrumbItem>,
    term_name: &str,
    term_url: Option<String>,
) -> Vec<BreadcrumbItem> {
    let mut out = Vec::with_capacity(2 + ancestors.len());
    out.push(BreadcrumbItem {
        name: home_label.to_owned(),
        url: home_url.map(str::to_owned),
        is_current: false,
    });
    for item in &mut ancestors {
        item.is_current = false;
    }
    out.extend(ancestors);
    out.push(BreadcrumbItem {
        name: term_name.to_owned(),
        url: term_url,
        is_current: true,
    });
    out
}

pub fn single_breadcrumbs(
    home_label: &str,
    home_url: Option<&str>,
    type_key: &str,
    content_type_label: &str,
    has_archive: bool,
    archive_url: Option<String>,
    primary_category: Option<&PublicTermView>,
    entry_title: &str,
    entry_url: Option<String>,
) -> Vec<BreadcrumbItem> {
    let mut out = vec![BreadcrumbItem {
        name: home_label.to_owned(),
        url: home_url.map(str::to_owned),
        is_current: false,
    }];
    if type_key != "page" && has_archive {
        out.push(BreadcrumbItem {
            name: content_type_label.to_owned(),
            url: archive_url,
            is_current: false,
        });
    }
    if let Some(cat) = primary_category {
        out.push(BreadcrumbItem {
            name: cat.name.clone(),
            url: cat.url.clone(),
            is_current: false,
        });
    }
    out.push(BreadcrumbItem {
        name: entry_title.to_owned(),
        url: entry_url,
        is_current: true,
    });
    out
}

pub fn not_found_breadcrumbs(
    home_label: &str,
    home_url: Option<&str>,
    not_found_label: &str,
) -> Vec<BreadcrumbItem> {
    vec![
        BreadcrumbItem {
            name: home_label.to_owned(),
            url: home_url.map(str::to_owned),
            is_current: false,
        },
        BreadcrumbItem {
            name: not_found_label.to_owned(),
            url: None,
            is_current: true,
        },
    ]
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PaginationPageLink {
    pub number: Option<i64>,
    pub url: Option<String>,
    pub is_current: bool,
    pub is_ellipsis: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PaginationView {
    pub page: i64,
    pub per_page: i64,
    pub total_items: i64,
    pub total_pages: i64,
    pub has_prev: bool,
    pub has_next: bool,
    pub previous_url: Option<String>,
    pub next_url: Option<String>,
    pub pages: Vec<PaginationPageLink>,
}

impl PaginationView {
    pub fn empty() -> Self {
        Self {
            page: 1,
            per_page: crate::public_context::ARCHIVE_SIZE,
            total_items: 0,
            total_pages: 0,
            has_prev: false,
            has_next: false,
            previous_url: None,
            next_url: None,
            pages: Vec::new(),
        }
    }

    pub fn compute<F>(
        page: i64,
        per_page: i64,
        total_items: i64,
        previous_url: Option<String>,
        next_url: Option<String>,
        mut page_url: F,
    ) -> Self
    where
        F: FnMut(i64) -> Option<String>,
    {
        let per_page = per_page.max(1);
        let total_items = total_items.max(0);
        let computed_pages = if total_items == 0 {
            0
        } else {
            ((total_items - 1) / per_page) + 1
        };
        let total_pages = computed_pages.min(crate::public_context::MAX_ARCHIVE_PAGE);

        let push_page = |num: i64, out: &mut Vec<PaginationPageLink>, page_url: &mut F| {
            out.push(PaginationPageLink {
                number: Some(num),
                url: page_url(num),
                is_current: num == page,
                is_ellipsis: false,
            });
        };
        let push_ellipsis = |out: &mut Vec<PaginationPageLink>| {
            out.push(PaginationPageLink {
                number: None,
                url: None,
                is_current: false,
                is_ellipsis: true,
            });
        };

        let mut pages = Vec::new();
        if total_pages > 0 && total_pages <= 9 {
            for n in 1..=total_pages {
                push_page(n, &mut pages, &mut page_url);
            }
        } else if total_pages > 9 {
            let p = page.clamp(1, total_pages);
            if p <= 5 {
                for n in 1..=7 {
                    push_page(n, &mut pages, &mut page_url);
                }
                push_ellipsis(&mut pages);
                push_page(total_pages, &mut pages, &mut page_url);
            } else if p >= total_pages - 4 {
                push_page(1, &mut pages, &mut page_url);
                push_ellipsis(&mut pages);
                for n in (total_pages - 6)..=total_pages {
                    push_page(n, &mut pages, &mut page_url);
                }
            } else {
                push_page(1, &mut pages, &mut page_url);
                push_ellipsis(&mut pages);
                for n in (p - 2)..=(p + 2) {
                    push_page(n, &mut pages, &mut page_url);
                }
                push_ellipsis(&mut pages);
                push_page(total_pages, &mut pages, &mut page_url);
            }
        }

        Self {
            page,
            per_page,
            total_items,
            total_pages,
            has_prev: previous_url.is_some(),
            has_next: next_url.is_some(),
            previous_url,
            next_url,
            pages,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EntryView {
    pub id: i64,
    pub type_key: String,
    pub title: String,
    pub slug: String,
    pub url: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub published_at: Option<String>,
    pub excerpt: String,
    pub reading_time_minutes: u32,
    pub values: BTreeMap<String, Value>,
    pub fields: Vec<FieldView>,
    pub cover_url: Option<String>,
    pub terms: BTreeMap<String, Vec<PublicTermView>>,
    pub all_terms: Vec<PublicTermView>,
    pub primary_category: Option<PublicTermView>,
    pub ext: BTreeMap<String, Value>,
    #[serde(default)]
    pub theme_ext: BTreeMap<String, Value>,
}

/// Only typed site strings enter the custom-template context; MiniJinja
/// performs HTML escaping just like every other content value.
#[derive(Debug, Clone, Serialize)]
pub struct SiteContext<'a> {
    pub title: &'a str,
    pub tagline: &'a str,
    pub timezone: &'a str,
    pub origin: &'a str,
    pub current_year: i32,
}

pub const DEFAULT_SITE: SiteContext<'static> = SiteContext {
    title: "OpenDash",
    tagline: "",
    timezone: "UTC",
    origin: "",
    current_year: 1970,
};

pub fn current_year_in_timezone(tz_name: &str) -> i32 {
    let tz = jiff::tz::TimeZone::get(tz_name).unwrap_or(jiff::tz::TimeZone::UTC);
    i32::from(jiff::Timestamp::now().to_zoned_datetime(tz).year())
}

fn media_url(value: &Value) -> Option<String> {
    let id = value.as_str()?;
    valid_media_id(id).then(|| format!("/media/{id}/file"))
}

pub fn strip_richtext_to_plain(html: &str) -> String {
    fn is_block_tag(name: &str) -> bool {
        matches!(
            name,
            "p" | "div"
                | "br"
                | "li"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "tr"
                | "td"
                | "th"
                | "blockquote"
                | "section"
                | "article"
                | "header"
                | "footer"
                | "pre"
                | "hr"
                | "ul"
                | "ol"
        )
    }

    let mut stripped = String::with_capacity(html.len());
    let mut i = 0;
    let bytes = html.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let next = bytes.get(i + 1).copied();
            let looks_like_tag =
                next.is_some_and(|b| b.is_ascii_alphabetic() || matches!(b, b'/' | b'!' | b'?'));
            if looks_like_tag {
                let mut quote: Option<u8> = None;
                let mut j = i + 1;
                let mut closed = false;
                while j < bytes.len() {
                    let b = bytes[j];
                    if let Some(q) = quote {
                        if b == q {
                            quote = None;
                        }
                    } else if b == b'"' || b == b'\'' {
                        quote = Some(b);
                    } else if b == b'>' {
                        closed = true;
                        break;
                    }
                    j += 1;
                }
                if closed {
                    let inner = html[i + 1..j].trim_start();
                    let inner = inner.strip_prefix('/').unwrap_or(inner);
                    let tag_name = inner
                        .split(|c: char| c.is_ascii_whitespace() || c == '/')
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if is_block_tag(&tag_name) && !stripped.ends_with(' ') {
                        stripped.push(' ');
                    }
                    i = j + 1;
                    continue;
                }
            }
        }
        let ch = html[i..].chars().next().unwrap_or('\0');
        stripped.push(ch);
        i += ch.len_utf8();
    }

    // Single-pass basic HTML entity decoding so "&amp;lt;" stays "&lt;".
    let mut decoded = String::with_capacity(stripped.len());
    let mut rest = stripped.as_str();
    while let Some(pos) = rest.find('&') {
        decoded.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        let mut matched = None;
        for (entity, replacement) in [
            ("&nbsp;", " "),
            ("&#160;", " "),
            ("&amp;", "&"),
            ("&lt;", "<"),
            ("&gt;", ">"),
            ("&quot;", "\""),
            ("&#39;", "'"),
            ("&#x27;", "'"),
            ("&apos;", "'"),
        ] {
            if tail.starts_with(entity) {
                decoded.push_str(replacement);
                rest = &tail[entity.len()..];
                matched = Some(());
                break;
            }
        }
        if matched.is_none() {
            decoded.push('&');
            rest = &tail[1..];
        }
    }
    decoded.push_str(rest);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn plain_text_from_field(definition: &ContentFieldDefinition, value: &Value) -> Option<String> {
    let raw = match (definition.field_type, value) {
        (
            ContentFieldType::ShortText | ContentFieldType::LongText | ContentFieldType::RichText,
            Value::String(s),
        ) => s.as_str(),
        _ => return None,
    };
    let clean = match definition.field_type {
        ContentFieldType::RichText => {
            let text = if opendash_core::builder_blocks::contains_blocks(raw) {
                opendash_core::builder_blocks::extract_visible_text(raw)
            } else {
                strip_richtext_to_plain(raw)
            };
            text.split_whitespace().collect::<Vec<_>>().join(" ")
        }
        ContentFieldType::ShortText | ContentFieldType::LongText => {
            raw.split_whitespace().collect::<Vec<_>>().join(" ")
        }
        _ => String::new(),
    };
    (!clean.is_empty()).then_some(clean)
}

fn truncate_excerpt(text: &str) -> String {
    if text.chars().count() <= 200 {
        return text.to_owned();
    }
    let mut byte_at_197 = text.len();
    let mut last_ws_char_idx = None;
    let mut last_ws_byte_idx = 0;
    for (char_idx, (byte_idx, ch)) in text.char_indices().enumerate() {
        if char_idx == 197 {
            byte_at_197 = byte_idx;
            break;
        }
        if ch.is_whitespace() {
            last_ws_char_idx = Some(char_idx);
            last_ws_byte_idx = byte_idx;
        }
    }
    let cut_byte = match last_ws_char_idx {
        Some(idx) if idx >= 98 => last_ws_byte_idx,
        _ => byte_at_197,
    };
    let prefix = text[..cut_byte].trim_end();
    format!("{prefix}...")
}

fn build_entry_view_with_defs(
    record: &ContentEntryRecord,
    type_key: &str,
    ordered_defs: &[&ContentFieldDefinition],
    url: Option<String>,
    terms: &[PublicTermView],
    eligible_public_taxonomies: &[String],
) -> EntryView {
    let l10n = core_l10n(&record.locale);
    let mut cover_url = None;
    let mut public_values = BTreeMap::new();
    let mut fields = Vec::with_capacity(ordered_defs.len());

    let mut summary_plain: Option<String> = None;
    let mut body_plain: Option<String> = None;
    let mut fallback_plain_parts: Vec<String> = Vec::new();
    let mut all_plain_parts: Vec<String> = Vec::new();

    for &definition in ordered_defs {
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
        } else {
            public_values.insert(definition.key.clone(), value.clone());
        }
        if let Some(plain) = plain_text_from_field(definition, value) {
            all_plain_parts.push(plain.clone());
            if definition.key == "summary" && summary_plain.is_none() {
                summary_plain = Some(plain);
            } else if definition.key == "body" && body_plain.is_none() {
                body_plain = Some(plain);
            } else {
                fallback_plain_parts.push(plain);
            }
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
                l10n.yes
            } else {
                l10n.no
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

    let excerpt_source = if let Some(ref summary) = summary_plain {
        summary.as_str().to_owned()
    } else if let Some(ref body) = body_plain {
        body.as_str().to_owned()
    } else {
        fallback_plain_parts.join(" ")
    };
    let excerpt = if excerpt_source.is_empty() {
        String::new()
    } else {
        truncate_excerpt(&excerpt_source)
    };

    let reading_source = if let Some(ref body) = body_plain {
        body.as_str().to_owned()
    } else {
        all_plain_parts.join(" ")
    };
    let words = reading_source.split_whitespace().count();
    let reading_time_minutes = if words == 0 {
        0
    } else {
        ((words + 199) / 200).max(1) as u32
    };

    let mut terms_map: BTreeMap<String, Vec<PublicTermView>> = BTreeMap::new();
    terms_map.insert("category".to_owned(), Vec::new());
    terms_map.insert("tag".to_owned(), Vec::new());
    for tax in eligible_public_taxonomies {
        terms_map.entry(tax.clone()).or_default();
    }
    let eligible_set: BTreeSet<&str> = eligible_public_taxonomies
        .iter()
        .map(String::as_str)
        .collect();
    let mut all_terms = Vec::with_capacity(terms.len());
    for term in terms {
        if !eligible_set.is_empty() && !eligible_set.contains(term.kind.as_str()) {
            continue;
        }
        terms_map
            .entry(term.kind.clone())
            .or_default()
            .push(term.clone());
        all_terms.push(term.clone());
    }

    let category_terms = terms_map.get("category").map(Vec::as_slice).unwrap_or(&[]);
    let primary_category = record
        .primary_category_term_id
        .and_then(|pid| category_terms.iter().find(|t| t.id == pid).cloned())
        .or_else(|| category_terms.iter().min_by_key(|t| t.id).cloned());

    EntryView {
        id: record.id,
        type_key: type_key.to_owned(),
        title: record.title.clone(),
        slug: record.slug.clone(),
        url,
        created_at: record.created_at.clone(),
        updated_at: record.updated_at.clone(),
        published_at: record.published_at.clone(),
        excerpt,
        reading_time_minutes,
        values: public_values,
        fields,
        cover_url,
        terms: terms_map,
        all_terms,
        primary_category,
        ext: BTreeMap::new(),
        theme_ext: BTreeMap::new(),
    }
}

pub fn entry_view(record: ContentEntryRecord, schema: &ContentTypeDefinition) -> EntryView {
    entry_view_with_url(record, schema, None)
}

pub fn entry_view_with_url(
    record: ContentEntryRecord,
    schema: &ContentTypeDefinition,
    url: Option<String>,
) -> EntryView {
    let ordered_defs: Vec<&ContentFieldDefinition> = schema
        .fields
        .iter()
        .filter(|d| d.public_exposure == PublicExposure::Public)
        .collect();
    build_entry_view_with_defs(&record, &schema.key, &ordered_defs, url, &[], &[])
}

pub fn entry_view_with_context(
    record: &ContentEntryRecord,
    effective_schema: &EffectiveContentSchema,
    url: Option<String>,
    terms: &[PublicTermView],
    eligible_public_taxonomies: &[String],
    max_view_role: FieldRole,
) -> EntryView {
    let mut seen = BTreeSet::new();
    let mut ordered_keys: Vec<&str> = Vec::with_capacity(effective_schema.effective_fields.len());
    for field in &effective_schema.base.descriptor.fields {
        if seen.insert(field.key.as_str()) {
            ordered_keys.push(field.key.as_str());
        }
    }
    for group in &effective_schema.attached_groups {
        for field in &group.fields {
            if seen.insert(field.key.as_str()) {
                ordered_keys.push(field.key.as_str());
            }
        }
    }
    for key in effective_schema.effective_fields.keys() {
        if seen.insert(key.as_str()) {
            ordered_keys.push(key.as_str());
        }
    }

    let ordered_defs: Vec<&ContentFieldDefinition> = ordered_keys
        .into_iter()
        .filter_map(|key| effective_schema.effective_fields.get(key))
        .filter(|fe| {
            fe.effective_status == EffectiveFieldStatus::Active
                && fe.definition.public_exposure == PublicExposure::Public
                && fe.definition.min_view_role <= max_view_role
        })
        .map(|fe| &fe.definition)
        .collect();

    build_entry_view_with_defs(
        record,
        &effective_schema.base.descriptor.key,
        &ordered_defs,
        url,
        terms,
        eligible_public_taxonomies,
    )
}

fn rewrite_preview_media_urls(mut view: EntryView, type_key: &str) -> EntryView {
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
    if view.url.is_none() {
        view.url = Some(format!(
            "/api/admin/content-types/{type_key}/entries/{}/preview",
            view.id
        ));
    }
    view
}

pub fn authenticated_preview_view_with_context(
    record: &ContentEntryRecord,
    effective_schema: &EffectiveContentSchema,
    url: Option<String>,
    terms: &[PublicTermView],
    eligible_public_taxonomies: &[String],
    actor_role: FieldRole,
) -> EntryView {
    let view = entry_view_with_context(
        record,
        effective_schema,
        url,
        terms,
        eligible_public_taxonomies,
        actor_role,
    );
    rewrite_preview_media_urls(view, &effective_schema.base.descriptor.key)
}

pub struct PublicThemeContextInput<'a> {
    pub site: &'a SiteContext<'a>,
    pub options: &'a BTreeMap<String, Value>,
    pub theme_assets: &'a BTreeMap<String, String>,
    pub menus: &'a BTreeMap<String, crate::storage::menu::MenuViewModel>,
    pub route_kind: &'a str,
    pub route_path: Option<&'a str>,
    pub breadcrumbs: &'a [BreadcrumbItem],
    pub pagination: &'a PaginationView,
    pub archive_url: Option<&'a str>,
    pub previous_url: Option<&'a str>,
    pub next_url: Option<&'a str>,
    pub entries: &'a [EntryView],
    pub related_entries: &'a [EntryView],
    pub recent_posts: &'a [EntryView],
    pub taxonomy: Option<&'a Value>,
    pub content_type: Option<&'a Value>,
    pub entry: Option<&'a EntryView>,
}

#[derive(Serialize)]
struct SerializedRouteContext<'a> {
    kind: &'a str,
    path: Option<&'a str>,
}

#[derive(Serialize)]
struct SerializedThemeContext<'a> {
    site: &'a SiteContext<'a>,
    options: &'a BTreeMap<String, Value>,
    theme_options: &'a BTreeMap<String, Value>,
    theme_assets: &'a BTreeMap<String, String>,
    menus: &'a BTreeMap<String, crate::storage::menu::MenuViewModel>,
    route: SerializedRouteContext<'a>,
    breadcrumbs: &'a [BreadcrumbItem],
    pagination: &'a PaginationView,
    archive_url: Option<&'a str>,
    previous_url: Option<&'a str>,
    next_url: Option<&'a str>,
    entries: &'a [EntryView],
    related_entries: &'a [EntryView],
    recent_posts: &'a [EntryView],
    taxonomy: Option<&'a Value>,
    content_type: Option<&'a Value>,
    entry: Option<&'a EntryView>,
}

pub fn build_public_theme_context(input: PublicThemeContextInput<'_>) -> ApiResult<Value> {
    let payload = SerializedThemeContext {
        site: input.site,
        options: input.options,
        theme_options: input.options,
        theme_assets: input.theme_assets,
        menus: input.menus,
        route: SerializedRouteContext {
            kind: input.route_kind,
            path: input.route_path,
        },
        breadcrumbs: input.breadcrumbs,
        pagination: input.pagination,
        archive_url: input.archive_url,
        previous_url: input.previous_url,
        next_url: input.next_url,
        entries: input.entries,
        related_entries: input.related_entries,
        recent_posts: input.recent_posts,
        taxonomy: input.taxonomy,
        content_type: input.content_type,
        entry: input.entry,
    };
    serde_json::to_value(payload).map_err(|err| ApiError::internal(err.to_string()))
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
    compose_document_with_seo_with_body_classes(header_html, rendered_layout, footer_html, seo, &[])
}

/// Server-owned SVG substitution happens only after document normalization.
pub fn finalize_document_icons(
    mut html: String,
    icon_nonces: &crate::template_sec::IconNonceTable,
) -> ApiResult<String> {
    if html.contains(HEAD_END_MARKER) {
        html = html.replace(HEAD_END_MARKER, "");
    }
    if icon_nonces.is_empty() {
        return Ok(html);
    }
    icon_nonces
        .substitute_into_document(html)
        .map_err(ApiError::internal)
}

pub const MAX_RENDER_HOOK_DOCUMENT_BYTES: usize = 2 * 1024 * 1024;
pub const HEAD_END_MARKER: &str = "<!-- opendash-head-end -->";

/// Applies already-sanitized per-subscriber render-hook outputs (`OD_HOOK_<32hex>` slots,
/// `filter:render/head` fragments/styles/scripts, and `filter:render/footer` fragments)
/// strictly AFTER `compose_document_with_seo_with_assets` and BEFORE `finalize_document_icons`.
pub fn apply_render_hooks_to_document(
    mut html: String,
    head_html: &str,
    head_styles: &[String],
    head_scripts: &[String],
    footer_html: &str,
    slot_replacements: &[(String, String)],
) -> ApiResult<String> {
    let apply = || -> Result<String, &'static str> {
        if !slot_replacements.is_empty() {
            let mut matches: Vec<(usize, &str, &str)> = Vec::new();
            for (nonce, replacement) in slot_replacements {
                let mut search_from = 0;
                while let Some(rel) = html[search_from..].find(nonce.as_str()) {
                    let offset = search_from + rel;
                    matches.push((offset, nonce.as_str(), replacement.as_str()));
                    search_from = offset + nonce.len();
                }
            }
            matches.sort_by_key(|(offset, _, _)| *offset);
            let mut out = String::with_capacity(html.len());
            let mut cursor = 0usize;
            for (offset, nonce, replacement) in matches {
                if offset < cursor {
                    continue;
                }
                out.push_str(&html[cursor..offset]);
                out.push_str(replacement);
                cursor = offset + nonce.len();
                if out.len() > MAX_RENDER_HOOK_DOCUMENT_BYTES {
                    return Err("Document chứa render hooks vượt giới hạn HTML.");
                }
            }
            out.push_str(&html[cursor..]);
            html = out;
        }

        let has_head_additions =
            !head_styles.is_empty() || !head_scripts.is_empty() || !head_html.is_empty();
        if let Some(marker_pos) = html.find(HEAD_END_MARKER) {
            let mut injected = String::new();
            if has_head_additions {
                for style in head_styles {
                    injected.push_str(style);
                }
                for script in head_scripts {
                    injected.push_str(script);
                }
                injected.push_str(head_html);
            }
            let new_len = html
                .len()
                .saturating_sub(HEAD_END_MARKER.len())
                .saturating_add(injected.len());
            if new_len > MAX_RENDER_HOOK_DOCUMENT_BYTES {
                return Err("Document chứa render hooks vượt giới hạn HTML.");
            }
            html.replace_range(marker_pos..marker_pos + HEAD_END_MARKER.len(), &injected);
        } else if has_head_additions && let Some(pos) = html.find("</head>") {
            let mut injected = String::new();
            for style in head_styles {
                injected.push_str(style);
            }
            for script in head_scripts {
                injected.push_str(script);
            }
            injected.push_str(head_html);
            if html.len().saturating_add(injected.len()) > MAX_RENDER_HOOK_DOCUMENT_BYTES {
                return Err("Document chứa render hooks vượt giới hạn HTML.");
            }
            html.insert_str(pos, &injected);
        }

        if !footer_html.is_empty()
            && let Some(pos) = html.rfind("</body>")
        {
            if html.len().saturating_add(footer_html.len()) > MAX_RENDER_HOOK_DOCUMENT_BYTES {
                return Err("Document chứa render hooks vượt giới hạn HTML.");
            }
            html.insert_str(pos, footer_html);
        }

        if html.len() > MAX_RENDER_HOOK_DOCUMENT_BYTES {
            return Err("Document chứa render hooks vượt giới hạn HTML.");
        }
        Ok(html)
    };
    apply().map_err(ApiError::internal)
}

pub fn compose_document_with_seo_with_body_classes(
    header_html: Option<&str>,
    rendered_layout: &str,
    footer_html: Option<&str>,
    seo: &crate::seo::SeoView,
    body_classes: &[&str],
) -> ApiResult<String> {
    compose_document_with_seo_with_assets(
        header_html,
        rendered_layout,
        footer_html,
        seo,
        body_classes,
        &ResolvedEnqueuedAssets::default(),
        false,
    )
}

/// Compose a public document with trusted theme assets, preserving the strict
/// normalizer for callers which do not explicitly opt into theme markup.
pub fn compose_document_with_seo_with_assets(
    header_html: Option<&str>,
    rendered_layout: &str,
    footer_html: Option<&str>,
    seo: &crate::seo::SeoView,
    body_classes: &[&str],
    enqueued: &ResolvedEnqueuedAssets,
    allow_trusted_theme_markup: bool,
) -> ApiResult<String> {
    let mut styles = Vec::new();
    let mut authorized_head_scripts = Vec::new();
    let (mut body_attributes, body_fragment) =
        match crate::document::normalize_public_document_with_options(
            rendered_layout,
            allow_trusted_theme_markup,
        ) {
            Ok(parts) => {
                styles.extend(parts.authorized_styles);
                authorized_head_scripts.extend(parts.authorized_head_scripts);
                (parts.body_attributes, parts.body_fragment)
            }
            Err(diag) => {
                let l10n = core_l10n(&seo.lang);
                (
                    Vec::new(),
                    format!(
                        "<main data-doc-diagnostic=\"{}\"><p>{}</p><pre>{}</pre></main>",
                        escape_html_attr(diag.code),
                        escape_html_attr(l10n.invalid_template_content),
                        escape_html_attr(rendered_layout),
                    ),
                )
            }
        };

    let extra_tokens: Vec<&str> = body_classes
        .iter()
        .flat_map(|c| c.split_whitespace())
        .filter(|t| !t.is_empty())
        .collect();
    if !extra_tokens.is_empty() {
        let mut seen = BTreeSet::new();
        let mut merged = Vec::new();
        if let Some(pos) = body_attributes
            .iter()
            .position(|(k, _)| k.eq_ignore_ascii_case("class"))
        {
            for tok in body_attributes[pos].1.split_whitespace() {
                if seen.insert(tok.to_owned()) {
                    merged.push(tok.to_owned());
                }
            }
            for tok in extra_tokens {
                if seen.insert(tok.to_owned()) {
                    merged.push(tok.to_owned());
                }
            }
            body_attributes[pos].1 = merged.join(" ");
        } else {
            for tok in extra_tokens {
                if seen.insert(tok.to_owned()) {
                    merged.push(tok.to_owned());
                }
            }
            if !merged.is_empty() {
                body_attributes.push(("class".to_owned(), merged.join(" ")));
            }
        }
    }

    let mut body_attrs_str = String::new();
    for (k, v) in body_attributes {
        body_attrs_str.push(' ');
        body_attrs_str.push_str(&k);
        body_attrs_str.push_str("=\"");
        body_attrs_str.push_str(&escape_html_attr(&v));
        body_attrs_str.push('"');
    }
    let normalize_slot =
        |slot: Option<&str>, styles: &mut Vec<String>, scripts: &mut Vec<String>| -> String {
            let Some(raw) = slot.filter(|s| !s.is_empty()) else {
                return String::new();
            };
            match crate::document::normalize_public_document_with_options(
                raw,
                allow_trusted_theme_markup,
            ) {
                Ok(parts) => {
                    styles.extend(parts.authorized_styles);
                    scripts.extend(parts.authorized_head_scripts);
                    parts.body_fragment
                }
                Err(_) => String::new(),
            }
        };
    let header = normalize_slot(header_html, &mut styles, &mut authorized_head_scripts);
    let footer = normalize_slot(footer_html, &mut styles, &mut authorized_head_scripts);

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
    for block in &seo.json_ld {
        head.push_str("<script type=\"application/ld+json\">");
        head.push_str(&serialize_json_ld_safe(block));
        head.push_str("</script>");
    }
    head.push_str("<link rel=\"stylesheet\" href=\"/assets/opendash-tailwind.css\">");
    for style in &enqueued.head_styles {
        head.push_str(style);
    }
    let mut seen_urls: BTreeSet<String> = enqueued.head_style_urls.iter().cloned().collect();
    seen_urls.insert("/assets/opendash-tailwind.css".to_owned());
    for style in styles {
        if let Some(href) = stylesheet_url_from_tag(&style) {
            if !seen_urls.insert(href) {
                continue;
            }
        }
        head.push_str(&style);
    }
    for script in &enqueued.head_scripts {
        head.push_str(script);
    }
    for script in authorized_head_scripts {
        head.push_str(&script);
    }
    let dir = if seo.dir == "rtl" { "rtl" } else { "ltr" };
    Ok(format!(
        "<!doctype html><html lang=\"{}\" dir=\"{}\"><head>{head}{HEAD_END_MARKER}</head><body{body_attrs_str}>{header}{body_fragment}{footer}{}</body></html>",
        escape_html_attr(&seo.lang),
        dir,
        enqueued.footer_scripts.concat(),
    ))
}

/// Stylesheet links are authorized by the normalizer. Extract their local URL
/// only for deduplication, including entity-encoded path characters.
fn stylesheet_url_from_tag(tag: &str) -> Option<String> {
    let tag = tag.trim_start();
    if !tag.get(..5)?.eq_ignore_ascii_case("<link") {
        return None;
    }
    let bytes = tag.as_bytes();
    let mut cursor = 5;
    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || matches!(bytes[cursor], b'>' | b'/') {
            break;
        }
        let start = cursor;
        while cursor < bytes.len()
            && !bytes[cursor].is_ascii_whitespace()
            && !matches!(bytes[cursor], b'=' | b'>' | b'/')
        {
            cursor += 1;
        }
        if start == cursor {
            cursor += 1;
            continue;
        }
        let name = &tag[start..cursor];
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] != b'=' {
            continue;
        }
        cursor += 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }
        let quote = bytes[cursor];
        let (from, to) = if quote == b'\'' || quote == b'"' {
            cursor += 1;
            let from = cursor;
            while cursor < bytes.len() && bytes[cursor] != quote {
                cursor += 1;
            }
            let to = cursor;
            cursor = (cursor + 1).min(bytes.len());
            (from, to)
        } else {
            let from = cursor;
            while cursor < bytes.len()
                && !bytes[cursor].is_ascii_whitespace()
                && bytes[cursor] != b'>'
            {
                cursor += 1;
            }
            (from, cursor)
        };
        if name.eq_ignore_ascii_case("href") {
            return Some(opendash_core::richtext::decode_html5_entities(
                &tag[from..to],
            ));
        }
    }
    None
}

pub fn serialize_json_ld_safe(val: &Value) -> String {
    let raw = serde_json::to_string(val).unwrap_or_else(|_| "{}".to_owned());
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            _ => out.push(ch),
        }
    }
    out
}

pub fn render_archive_with_site(
    schema: &ContentTypeDefinition,
    entries: &[EntryView],
    previous_url: Option<&str>,
    next_url: Option<&str>,
    selected_template: Option<&str>,
    site: &SiteContext<'_>,
) -> ApiResult<String> {
    render_archive_with_site_for_locale(
        schema,
        entries,
        previous_url,
        next_url,
        selected_template,
        site,
        "vi",
    )
}

pub fn render_archive_with_site_for_locale(
    schema: &ContentTypeDefinition,
    entries: &[EntryView],
    previous_url: Option<&str>,
    next_url: Option<&str>,
    selected_template: Option<&str>,
    site: &SiteContext<'_>,
    locale_code: &str,
) -> ApiResult<String> {
    render_archive_with_site_and_config_for_locale(
        schema,
        entries,
        previous_url,
        next_url,
        &PaginationView::empty(),
        &[],
        None,
        selected_template,
        site,
        &RenderFormattingConfig::default(),
        locale_code,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn render_archive_with_site_and_config_for_locale(
    schema: &ContentTypeDefinition,
    entries: &[EntryView],
    previous_url: Option<&str>,
    next_url: Option<&str>,
    pagination: &PaginationView,
    breadcrumbs: &[BreadcrumbItem],
    taxonomy: Option<&Value>,
    selected_template: Option<&str>,
    site: &SiteContext<'_>,
    fmt_cfg: &RenderFormattingConfig,
    locale_code: &str,
) -> ApiResult<String> {
    let data = serde_json::json!({
        "content_type": schema,
        "entries": entries,
        "previous_url": previous_url,
        "next_url": next_url,
        "pagination": pagination,
        "breadcrumbs": breadcrumbs,
        "taxonomy": taxonomy,
        "site": site,
        "l10n": core_l10n(locale_code),
    });
    if let Some(custom) = selected_template {
        if let Ok(html) = theme::render_content_with_config(custom, &data, fmt_cfg) {
            return Ok(html);
        }
    }
    theme::render_content_with_config(DEFAULT_ARCHIVE, &data, fmt_cfg).map_err(ApiError::internal)
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
    render_single_with_site_and_archive_for_locale(
        schema,
        entry,
        selected_template,
        site,
        archive_url,
        "vi",
    )
}

pub fn render_single_with_site_and_archive_for_locale(
    schema: &ContentTypeDefinition,
    entry: &EntryView,
    selected_template: Option<&str>,
    site: &SiteContext<'_>,
    archive_url: Option<&str>,
    locale_code: &str,
) -> ApiResult<String> {
    render_single_with_context_for_locale(
        schema,
        entry,
        selected_template,
        site,
        archive_url,
        &[],
        &RenderFormattingConfig::default(),
        locale_code,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn render_single_with_context_for_locale(
    schema: &ContentTypeDefinition,
    entry: &EntryView,
    selected_template: Option<&str>,
    site: &SiteContext<'_>,
    archive_url: Option<&str>,
    breadcrumbs: &[BreadcrumbItem],
    fmt_cfg: &RenderFormattingConfig,
    locale_code: &str,
) -> ApiResult<String> {
    let data = serde_json::json!({
        "content_type": schema,
        "entry": entry,
        "archive_url": archive_url,
        "breadcrumbs": breadcrumbs,
        "site": site,
        "l10n": core_l10n(locale_code),
    });
    if let Some(custom) = selected_template {
        if let Ok(html) = theme::render_content_with_config(custom, &data, fmt_cfg) {
            return Ok(html);
        }
    }
    theme::render_content_with_config(DEFAULT_SINGLE, &data, fmt_cfg).map_err(ApiError::internal)
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
    fn trusted_document_deduplicates_enqueued_styles_and_preserves_head_scripts() {
        let seo = crate::seo::SeoView::for_noindex_page("vi", "Trusted markup");
        let assets = ResolvedEnqueuedAssets {
            head_styles: vec![
                "<link rel=\"stylesheet\" href=\"/theme/digest/assets/base.css\">".to_owned(),
            ],
            head_style_urls: vec!["/theme/digest/assets/base.css".to_owned()],
            head_scripts: vec![
                "<script type=\"module\" src=\"/theme/digest/assets/init.mjs\"></script>"
                    .to_owned(),
            ],
            footer_scripts: vec![
                "<script src=\"/theme/digest/assets/app.js\" defer></script>".to_owned(),
            ],
        };
        let html = compose_document_with_seo_with_assets(
            Some("<header>Header</header>"),
            concat!(
                "<!doctype html><html><head>",
                "<link rel=\"stylesheet\" href=\"/assets/opendash-tailwind.css\">",
                "<link href=\"/theme/digest/assets/base.css\" rel=\"stylesheet\">",
                "<link rel=\"stylesheet\" href=\"/theme/digest/assets/&#x62;ase.css\">",
                "<style>.trusted{display:block}</style>",
                "<script>if (2 < 3) { window.headReady = true; }</script>",
                "</head><body><main>Body</main></body></html>"
            ),
            Some("<footer>Footer</footer>"),
            &seo,
            &["od-route-home"],
            &assets,
            true,
        )
        .unwrap();
        assert_eq!(html.matches("/assets/opendash-tailwind.css").count(), 1);
        assert_eq!(html.matches("/theme/digest/assets/base.css").count(), 1);
        assert!(!html.contains("&#x62;ase.css"));
        assert!(html.contains("<style>.trusted{display:block}</style>"));
        assert_ordered_markup(
            &html,
            &[
                "/assets/opendash-tailwind.css",
                "/theme/digest/assets/base.css",
                "/theme/digest/assets/init.mjs",
                "window.headReady = true",
                "</head>",
                "<main>Body</main>",
                "<footer>Footer</footer>",
                "/theme/digest/assets/app.js",
            ],
        );
    }

    fn assert_ordered_markup(html: &str, fragments: &[&str]) {
        let mut at = 0;
        for fragment in fragments {
            let pos = html[at..]
                .find(fragment)
                .unwrap_or_else(|| panic!("missing {fragment}: {html}"));
            at += pos + fragment.len();
        }
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
            locale: "vi".into(),
            translation_group_id: "tg_test_1".into(),
            translations: vec![],
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
            published_at: None,
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
            timezone: "UTC",
            origin: "",
            current_year: 2026,
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
            locale: "vi".into(),
            translation_group_id: "tg_test_17".into(),
            translations: vec![],
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
            published_at: None,
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

    #[test]
    fn strip_richtext_handles_block_breaks_entities_and_plain_comparisons() {
        assert_eq!(
            strip_richtext_to_plain(
                "<p>Chào <strong>bạn</strong> &amp; 1 &lt; 2</p><p>Dòng 2<br/>Dòng 3</p>"
            ),
            "Chào bạn & 1 < 2 Dòng 2 Dòng 3"
        );
        assert_eq!(
            strip_richtext_to_plain("<p>1 < 2 it's ok</p>"),
            "1 < 2 it's ok"
        );
        assert_eq!(
            strip_richtext_to_plain("&amp;lt;script&amp;gt;"),
            "&lt;script&gt;"
        );
    }

    #[test]
    fn entry_view_with_context_enforces_acl_declaration_order_nulls_and_excerpt() {
        use opendash_core::content::{
            AttachedFieldGroupSnapshot, ContentTypeSettings, EditorRegion, EffectiveFieldEntry,
            FieldGroupStatus,
        };
        use opendash_core::registry::{
            DeclarationState, OwnedDescriptor, ProviderStatus, ResourceOwner,
        };

        let base_price = ContentFieldDefinition {
            key: "price".into(),
            label: "Giá".into(),
            field_type: ContentFieldType::Number,
            min_view_role: FieldRole::Editor,
            public_exposure: PublicExposure::Public,
            ..Default::default()
        };
        let group_summary = ContentFieldDefinition {
            key: "summary".into(),
            label: "Tóm tắt".into(),
            field_type: ContentFieldType::ShortText,
            min_view_role: FieldRole::Editor,
            public_exposure: PublicExposure::Public,
            ..Default::default()
        };
        let admin_note = ContentFieldDefinition {
            key: "secret_note".into(),
            label: "Ghi chú Admin".into(),
            field_type: ContentFieldType::ShortText,
            min_view_role: FieldRole::Admin,
            public_exposure: PublicExposure::Public,
            ..Default::default()
        };
        let private_field = ContentFieldDefinition {
            key: "internal".into(),
            label: "Nội bộ".into(),
            field_type: ContentFieldType::ShortText,
            min_view_role: FieldRole::Editor,
            public_exposure: PublicExposure::Private,
            ..Default::default()
        };
        let missing_public = ContentFieldDefinition {
            key: "subtitle".into(),
            label: "Phụ đề".into(),
            field_type: ContentFieldType::ShortText,
            min_view_role: FieldRole::Editor,
            public_exposure: PublicExposure::Public,
            ..Default::default()
        };
        let extra_body = ContentFieldDefinition {
            key: "body".into(),
            label: "Nội dung".into(),
            field_type: ContentFieldType::RichText,
            min_view_role: FieldRole::Editor,
            public_exposure: PublicExposure::Public,
            ..Default::default()
        };

        let schema = EffectiveContentSchema {
            base: OwnedDescriptor {
                owner: ResourceOwner::Core,
                status: ProviderStatus::Active,
                declaration_state: DeclarationState::Declared,
                descriptor: ContentTypeDefinition {
                    key: "product".into(),
                    label: "Sản phẩm".into(),
                    fields: vec![base_price.clone(), missing_public.clone()],
                    settings: ContentTypeSettings::default(),
                },
            },
            base_version: 1,
            attached_groups: vec![AttachedFieldGroupSnapshot {
                group_id: "g1".into(),
                owner: ResourceOwner::Core,
                local_key: "g1".into(),
                title: "Group".into(),
                description: String::new(),
                fields: vec![
                    group_summary.clone(),
                    admin_note.clone(),
                    private_field.clone(),
                ],
                default_region: EditorRegion::MainPrimary,
                default_order: 0,
                collapsible: false,
                status: FieldGroupStatus::Active,
                association_state: DeclarationState::Declared,
                version: 1,
            }],
            effective_fields: [
                base_price,
                missing_public,
                group_summary,
                admin_note,
                private_field,
                extra_body,
            ]
            .into_iter()
            .map(|definition| {
                (
                    definition.key.clone(),
                    EffectiveFieldEntry {
                        definition,
                        owner: ResourceOwner::Core,
                        source_id: "core".into(),
                        effective_status: EffectiveFieldStatus::Active,
                    },
                )
            })
            .collect(),
            schema_token: "tok".into(),
        };

        let long_body = "<p>".to_owned() + &"tu ".repeat(210) + "</p>";
        let record = ContentEntryRecord {
            id: 9,
            content_type_key: "product".into(),
            locale: "vi".into(),
            translation_group_id: "tg_9".into(),
            translations: vec![],
            slug: "san-pham-1".into(),
            title: "SP 1".into(),
            values: [
                ("price".into(), Value::from(99000)),
                ("secret_note".into(), Value::String("Admin only".into())),
                ("internal".into(), Value::String("Private".into())),
                ("body".into(), Value::String(long_body)),
            ]
            .into(),
            term_ids: vec![12, 5],
            primary_category_term_id: Some(12),
            publication_status: "published".into(),
            page_layout_key: None,
            custom_template_key: None,
            content_mode: opendash_core::content::ContentMode::Editor,
            version: 2,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-02T00:00:00Z".into(),
            published_at: Some("2026-01-02T00:00:00Z".into()),
        };

        let terms = vec![
            PublicTermView {
                id: 5,
                kind: "category".into(),
                name: "Cat 5".into(),
                slug: "cat-5".into(),
                url: Some("/category/cat-5".into()),
            },
            PublicTermView {
                id: 12,
                kind: "category".into(),
                name: "Cat 12".into(),
                slug: "cat-12".into(),
                url: Some("/category/cat-12".into()),
            },
        ];
        let eligible = vec!["category".into(), "brand".into()];
        let public_view = entry_view_with_context(
            &record,
            &schema,
            Some("/product/san-pham-1".into()),
            &terms,
            &eligible,
            FieldRole::Editor,
        );
        let keys: Vec<&str> = public_view.fields.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, vec!["price", "subtitle", "summary", "body"]);
        assert_eq!(public_view.values.get("subtitle"), Some(&Value::Null));
        assert!(!public_view.values.contains_key("secret_note"));
        assert!(!public_view.values.contains_key("internal"));
        assert!(public_view.excerpt.chars().count() <= 200);
        assert!(public_view.excerpt.ends_with("..."));
        assert_eq!(public_view.reading_time_minutes, 2);
        assert_eq!(
            public_view.primary_category.as_ref().map(|c| c.id),
            Some(12)
        );
        assert!(public_view.terms.contains_key("tag"));
        assert!(public_view.terms.contains_key("brand"));

        // Number field alone (no text fields) must produce empty excerpt & 0 reading_time_minutes.
        let mut num_only_rec = record.clone();
        num_only_rec.values = [("price".into(), Value::from(42))].into();
        num_only_rec.primary_category_term_id = Some(999);
        let num_only_view = authenticated_preview_view_with_context(
            &num_only_rec,
            &schema,
            None,
            &terms,
            &eligible,
            FieldRole::Admin,
        );
        assert!(num_only_view.values.contains_key("secret_note"));
        assert_eq!(num_only_view.excerpt, "");
        assert_eq!(num_only_view.reading_time_minutes, 0);
        assert_eq!(
            num_only_view.primary_category.as_ref().map(|c| c.id),
            Some(5)
        );
        assert_eq!(
            num_only_view.url.as_deref(),
            Some("/api/admin/content-types/product/entries/9/preview")
        );
    }

    #[test]
    fn pagination_view_windows_and_clamps_max_archive_page() {
        let empty =
            PaginationView::compute(1, 20, 0, None, None, |p| Some(format!("/posts?page={p}")));
        assert_eq!(empty.total_pages, 0);
        assert!(empty.pages.is_empty());

        let small = PaginationView::compute(
            2,
            10,
            50,
            Some("/posts".into()),
            Some("/posts?page=3".into()),
            |p| Some(format!("/{p}")),
        );
        assert_eq!(small.total_pages, 5);
        assert_eq!(small.pages.len(), 5);
        assert!(small.has_prev);
        assert!(small.has_next);
        assert!(small.pages[1].is_current);

        let start = PaginationView::compute(3, 10, 200, None, None, |p| Some(format!("/{p}")));
        assert_eq!(start.pages.len(), 9);
        assert!(start.pages[7].is_ellipsis);
        assert_eq!(start.pages[8].number, Some(20));

        let mid = PaginationView::compute(10, 10, 200, None, None, |p| Some(format!("/{p}")));
        assert_eq!(mid.pages.len(), 9);
        assert_eq!(mid.pages[0].number, Some(1));
        assert!(mid.pages[1].is_ellipsis);
        assert_eq!(mid.pages[4].number, Some(10));
        assert!(mid.pages[4].is_current);
        assert!(mid.pages[7].is_ellipsis);
        assert_eq!(mid.pages[8].number, Some(20));

        let end = PaginationView::compute(18, 10, 200, None, None, |p| Some(format!("/{p}")));
        assert_eq!(end.pages.len(), 9);
        assert!(end.pages[1].is_ellipsis);
        assert_eq!(end.pages[8].number, Some(20));

        let capped = PaginationView::compute(1, 1, 500_000, None, None, |_| None);
        assert_eq!(capped.total_pages, crate::public_context::MAX_ARCHIVE_PAGE);
    }

    #[test]
    fn compose_document_merges_and_deduplicates_body_classes() {
        let seo = crate::seo::SeoView::for_noindex_page("vi", "Class Test");
        let html = compose_document_with_seo_with_body_classes(
            None,
            "<!doctype html><html><head></head><body class=\"alpha od-route-home alpha\"><main>Hi</main></body></html>",
            None,
            &seo,
            &["od-route-home", "od-extra", "alpha"],
        )
        .unwrap();
        assert_eq!(html.matches("class=\"").count(), 1);
        assert!(html.contains("class=\"alpha od-route-home od-extra\""));
    }

    #[test]
    fn build_public_theme_context_includes_all_strict_base_keys() {
        let opts = BTreeMap::new();
        let assets = BTreeMap::new();
        let menus = BTreeMap::new();
        let pagination = PaginationView::empty();
        let ctx = build_public_theme_context(PublicThemeContextInput {
            site: &DEFAULT_SITE,
            options: &opts,
            theme_assets: &assets,
            menus: &menus,
            route_kind: "home",
            route_path: Some("/"),
            breadcrumbs: &[],
            pagination: &pagination,
            archive_url: None,
            previous_url: None,
            next_url: None,
            entries: &[],
            related_entries: &[],
            recent_posts: &[],
            taxonomy: None,
            content_type: None,
            entry: None,
        })
        .unwrap();
        for key in [
            "site",
            "options",
            "theme_options",
            "theme_assets",
            "menus",
            "route",
            "breadcrumbs",
            "pagination",
            "archive_url",
            "previous_url",
            "next_url",
            "entries",
            "related_entries",
            "recent_posts",
            "taxonomy",
            "content_type",
            "entry",
        ] {
            assert!(ctx.get(key).is_some(), "missing key {key}");
        }
    }

    #[test]
    fn apply_render_hooks_injects_at_head_end_marker_despite_head_script_literal() {
        let seo = crate::seo::SeoView::for_noindex_page("vi", "Marker Test");
        let assets = ResolvedEnqueuedAssets::default();
        let composed = compose_document_with_seo_with_assets(
            None,
            concat!(
                "<!doctype html><html><head>",
                "<script>window.x = \"</head>\";</script>",
                "</head><body><main>Body</main></body></html>"
            ),
            None,
            &seo,
            &[],
            &assets,
            true,
        )
        .unwrap();
        assert!(composed.contains(HEAD_END_MARKER));

        let final_html = apply_render_hooks_to_document(
            composed,
            r#"<meta name="x-hook" content="ok">"#,
            &[],
            &[],
            "",
            &[],
        )
        .unwrap();
        assert!(!final_html.contains(HEAD_END_MARKER));
        let script_end = final_html.find("</script>").unwrap();
        let hook_pos = final_html
            .find(r#"<meta name="x-hook" content="ok">"#)
            .unwrap();
        let real_head_end = final_html.rfind("</head>").unwrap();
        assert!(
            script_end < hook_pos && hook_pos < real_head_end,
            "head hook must be injected AFTER inline head script and BEFORE real </head>: {final_html}"
        );
    }
}
