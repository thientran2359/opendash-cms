//! Public CPT HTML view model. Data is never evaluated as a MiniJinja template.
//! Rich-text stays untrusted text and is HTML-escaped, not marked |safe.

use super::*;

pub(super) use std::borrow::Cow;
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

#[derive(Debug, Clone, Serialize)]
pub struct CoreL10n {
    pub lang: Cow<'static, str>,
    pub yes: Cow<'static, str>,
    pub no: Cow<'static, str>,
    pub empty_content: Cow<'static, str>,
    pub previous_page: Cow<'static, str>,
    pub next_page: Cow<'static, str>,
    pub invalid_template_content: Cow<'static, str>,
    pub image_alt_prefix: Cow<'static, str>,
    pub list_unavailable: Cow<'static, str>,
    pub page_empty: Cow<'static, str>,
    pub back_to_first_page: Cow<'static, str>,
    pub list_prev: Cow<'static, str>,
    pub list_next: Cow<'static, str>,
    pub contact_unavailable: Cow<'static, str>,
    pub pattern_missing: Cow<'static, str>,
    pub select_placeholder: Cow<'static, str>,
    pub select_empty: Cow<'static, str>,
    pub home_unconfigured: Cow<'static, str>,
    pub not_found_title: Cow<'static, str>,
    pub contact_error_title: Cow<'static, str>,
    pub contact_error_heading: Cow<'static, str>,
    pub contact_received_title: Cow<'static, str>,
    pub contact_received_heading: Cow<'static, str>,
    pub contact_received_body: Cow<'static, str>,
}

fn embedded_vi_catalog() -> &'static opendash_core::l10n::Catalog {
    static CAT: std::sync::OnceLock<opendash_core::l10n::Catalog> = std::sync::OnceLock::new();
    CAT.get_or_init(|| {
        opendash_core::l10n::parse_mo(crate::l10n::EMBEDDED_VI_MO).unwrap_or_default()
    })
}

/// Returns Core fallback strings resolved from `CatalogRegistry` (or embedded `.mo` / English fallback).
pub fn core_l10n(locale_code: &str) -> CoreL10n {
    core_l10n_with_registry(None, locale_code)
}

pub fn core_l10n_with_registry(
    registry: Option<&crate::l10n::CatalogRegistry>,
    locale_code: &str,
) -> CoreL10n {
    let lower = locale_code.trim().to_ascii_lowercase();
    let cat: Option<std::sync::Arc<opendash_core::l10n::Catalog>> = registry
        .and_then(|r| r.resolve(crate::l10n::L10nScope::Core, "opendash", locale_code))
        .map(|l| l.catalog);

    let tr = |en: &'static str| -> Cow<'static, str> {
        if let Some(ref c) = cat
            && let Some(found) = c.get_with_context(None, en)
        {
            return Cow::Owned(found.to_owned());
        }
        if (lower == "vi" || lower.starts_with("vi-"))
            && let Some(found) = embedded_vi_catalog().get_with_context(None, en)
        {
            return Cow::Owned(found.to_owned());
        }
        Cow::Borrowed(en)
    };

    CoreL10n {
        lang: if lower.is_empty() {
            Cow::Borrowed("en")
        } else {
            Cow::Owned(lower.clone())
        },
        yes: tr("Yes"),
        no: tr("No"),
        empty_content: tr("No content yet."),
        previous_page: tr("← Previous"),
        next_page: tr("Next →"),
        invalid_template_content: tr("Invalid template content."),
        image_alt_prefix: tr("Illustration "),
        list_unavailable: tr("List is currently unavailable."),
        page_empty: tr("This page has no content."),
        back_to_first_page: tr("Back to first page"),
        list_prev: tr("Previous"),
        list_next: tr("Next"),
        contact_unavailable: tr("Contact form is currently unavailable."),
        pattern_missing: tr("Pattern no longer exists."),
        select_placeholder: tr("-- Select --"),
        select_empty: tr("Leave blank"),
        home_unconfigured: tr("Homepage is not configured."),
        not_found_title: tr("404 — Page not found"),
        contact_error_title: tr("Form submission error"),
        contact_error_heading: tr("Unable to submit form"),
        contact_received_title: tr("Message received"),
        contact_received_heading: tr("Thank you for contacting us"),
        contact_received_body: tr("Your message has been received."),
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
