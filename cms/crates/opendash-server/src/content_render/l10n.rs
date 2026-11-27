//! Public CPT HTML view model. Data is never evaluated as a MiniJinja template.
//! Rich-text stays untrusted text and is HTML-escaped, not marked |safe.

use super::*;

pub(super) use std::collections::{BTreeMap, BTreeSet};

pub(super) use opendash_core::content::{
    ContentTypeDefinition, EffectiveContentSchema, EffectiveFieldStatus, FieldRole, PublicExposure,
};

pub(super) use opendash_core::theme::ResolvedEnqueuedAssets;

pub(super) use serde::Serialize;

pub(super) use serde_json::Value;

pub(super) use crate::{
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

pub(super) const CORE_L10N_VI: CoreL10n = CoreL10n {
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

pub(super) const CORE_L10N_EN: CoreL10n = CoreL10n {
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
<dl class="mt-8">{% for field in entry.field_list %}
{% if field.text or field.image_url or field.gallery_urls %}
<dt class="mt-6 font-semibold">{{ field.label }}</dt>
{% if field.image_url %}<dd class="my-2"><img class="h-auto max-w-full rounded-lg" src="{{ field.image_url }}" alt="{{ field.label }}" loading="lazy"></dd>
{% elif field.gallery_urls %}<dd class="my-2 flex flex-wrap gap-3">{% for url in field.gallery_urls %}<img class="h-36 w-48 rounded-lg object-contain" src="{{ url }}" alt="{{ field.label }}" loading="lazy">{% endfor %}</dd>
{% else %}<dd class="my-2 whitespace-pre-wrap break-words">{{ field.text }}</dd>{% endif %}
{% endif %}
{% endfor %}</dl>
</article></main></body></html>"##;

pub const DEFAULT_BREADCRUMB_HOME_LABEL: &str = "Trang chủ";

pub const DEFAULT_BREADCRUMB_HOME_URL: &str = "/";

pub const DEFAULT_BREADCRUMB_NOT_FOUND_LABEL: &str = "404";

pub const DEFAULT_SITE: SiteContext<'static> = SiteContext {
    title: "OpenDash",
    tagline: "",
    timezone: "UTC",
    origin: "",
    current_year: 1970,
};

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
