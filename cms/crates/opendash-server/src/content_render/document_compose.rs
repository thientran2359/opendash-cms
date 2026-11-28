use super::*;

pub fn build_public_theme_context(input: PublicThemeContextInput<'_>) -> ApiResult<Value> {
    let payload = SerializedThemeContext {
        site: input.site,
        site_options: input.site_options,
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
        search: input.search,
    };
    serde_json::to_value(payload).map_err(|err| ApiError::internal(err.to_string()))
}

pub(super) fn escape_html_attr(raw: &str) -> String {
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

/// Server-owned responsive `<picture>` / `<img>` substitution happens only after document normalization.
pub fn finalize_document_images(
    mut html: String,
    image_nonces: &crate::template_sec::ImageNonceTable,
) -> ApiResult<String> {
    if html.contains(HEAD_END_MARKER) {
        html = html.replace(HEAD_END_MARKER, "");
    }
    if image_nonces.is_empty() {
        return Ok(html);
    }
    image_nonces
        .substitute_into_document(html)
        .map_err(ApiError::internal)
}

/// Server-owned sanitized RichText nonce substitution happens strictly after
/// document normalization inside safe `HtmlScanState::Body` regions.
pub fn finalize_document_richtext(
    mut html: String,
    richtext_nonces: &crate::template_sec::RichTextNonceTable,
) -> ApiResult<String> {
    if html.contains(HEAD_END_MARKER) {
        html = html.replace(HEAD_END_MARKER, "");
    }
    if richtext_nonces.is_empty() {
        return Ok(html);
    }
    richtext_nonces
        .substitute_into_document(html)
        .map_err(ApiError::internal)
}

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
    compose_document_with_seo_with_policy(
        header_html,
        rendered_layout,
        footer_html,
        seo,
        body_classes,
        enqueued,
        allow_trusted_theme_markup,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn compose_document_with_seo_with_policy(
    header_html: Option<&str>,
    rendered_layout: &str,
    footer_html: Option<&str>,
    seo: &crate::seo::SeoView,
    body_classes: &[&str],
    enqueued: &ResolvedEnqueuedAssets,
    allow_trusted_theme_markup: bool,
    allow_admin_media_preview: bool,
) -> ApiResult<String> {
    let normalize_opts = crate::document::DocumentNormalizeOptions {
        allow_trusted_theme_markup,
        allow_admin_media_preview,
    };
    let mut styles = Vec::new();
    let mut authorized_head_scripts = Vec::new();
    let (mut body_attributes, body_fragment) =
        match crate::document::normalize_document_with_policy(rendered_layout, normalize_opts) {
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
                        escape_html_attr(&l10n.invalid_template_content),
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
            match crate::document::normalize_document_with_policy(raw, normalize_opts) {
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
