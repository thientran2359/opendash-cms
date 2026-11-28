use super::*;

pub(crate) fn build_media_urls_and_srcsets(
    row: &HydratedMediaRow,
    is_preview: bool,
) -> (
    String,
    String,
    Vec<opendash_core::media::MediaVariantView>,
    Option<String>,
    Option<String>,
) {
    let prefix = if is_preview {
        "/api/admin/media"
    } else {
        "/media"
    };
    let version = row.version.max(1);
    let file_url = format!("{prefix}/{}/file?v={version}", row.id);
    let thumbnail_url = if row.kind == "image" {
        format!("{prefix}/{}/thumbnail?v={version}", row.id)
    } else {
        String::new()
    };
    let variants: Vec<opendash_core::media::MediaVariantView> = row
        .variants
        .iter()
        .map(|v| opendash_core::media::MediaVariantView {
            spec: v.spec,
            format: v.format,
            width: v.width,
            height: v.height,
            byte_size: v.byte_size,
            url: format!(
                "{prefix}/{}/variant/{}.{}?v={version}",
                row.id,
                v.spec.as_str(),
                v.format.extension()
            ),
        })
        .collect();
    let (webp_srcset, fallback_srcset) = if row.kind == "image" {
        let orig_w = row.width.and_then(|w| u32::try_from(w).ok()).unwrap_or(0);
        if orig_w > 0 {
            opendash_core::media::build_responsive_srcsets(
                &row.mime_type,
                orig_w,
                &file_url,
                &variants,
            )
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };
    (
        file_url,
        thumbnail_url,
        variants,
        webp_srcset,
        fallback_srcset,
    )
}

pub fn hydrate_media_row(row: &HydratedMediaRow, is_preview: bool) -> Value {
    let version = row.version.max(1);
    let (file_url, thumbnail_url, variants, webp_srcset, fallback_srcset) =
        build_media_urls_and_srcsets(row, is_preview);
    json!({
        "id": row.id,
        "version": version,
        "url": file_url,
        "thumbnail_url": thumbnail_url,
        "alt": "",
        "width": row.width,
        "height": row.height,
        "kind": row.kind,
        "variants": variants,
        "webp_srcset": webp_srcset,
        "fallback_srcset": fallback_srcset,
    })
}

pub fn hydrate_media_row_for_template(row: &HydratedMediaRow, is_preview: bool) -> Value {
    let mut val = hydrate_media_row(row, is_preview);
    if let Value::Object(ref mut map) = val {
        map.insert(
            crate::template_sec::MEDIA_PROVENANCE_KEY.to_owned(),
            Value::String(crate::template_sec::media_provenance_token().to_owned()),
        );
    }
    val
}

pub(super) fn hydrate_media_for_ctx(row: &HydratedMediaRow, ctx: &HydrationContext<'_>) -> Value {
    if ctx.tag_media_provenance {
        hydrate_media_row_for_template(row, ctx.is_preview)
    } else {
        hydrate_media_row(row, ctx.is_preview)
    }
}

pub(super) fn hydrate_entry_row(row: &HydratedEntryRefRow) -> Value {
    json!({
        "id": row.id,
        "type_key": row.type_key,
        "title": row.title,
        "slug": row.slug,
        "url": row.url,
        "cover_url": row.cover_url,
        "excerpt": row.excerpt,
    })
}

pub(super) fn hydrate_term_row(row: &HydratedTermRefRow, ctx: &HydrationContext<'_>) -> Value {
    let featured_image = row
        .featured_image_media_id
        .as_deref()
        .and_then(|id| ctx.media.get(id))
        .map(|m| hydrate_media_for_ctx(m, ctx))
        .unwrap_or(Value::Null);
    json!({
        "id": row.id,
        "kind": row.taxonomy_key,
        "name": row.name,
        "slug": row.slug,
        "url": row.url,
        "featured_image": featured_image,
    })
}

fn extract_raw_spacing(val: &Value) -> Option<SpacingValue> {
    if let Ok(mut s) = serde_json::from_value::<SpacingValue>(val.clone()) {
        s.canonicalize();
        return Some(s);
    }
    None
}

pub(super) fn hydrate_spacing_value(spec: &FieldSpec, raw_val: Option<&Value>) -> Value {
    let effective = raw_val.or(spec.default.as_ref());
    let prop = spec.effective_spacing_property();
    let unit = spec.effective_unit();

    if let Some(obj) = effective.and_then(Value::as_object)
        && (obj.contains_key("desktop") || obj.contains_key("tablet") || obj.contains_key("mobile"))
    {
        let desktop = obj.get("desktop").and_then(extract_raw_spacing);
        let tablet = obj.get("tablet").and_then(extract_raw_spacing);
        let mobile = obj.get("mobile").and_then(extract_raw_spacing);
        let classes =
            SpacingValue::format_responsive_classes(desktop.clone(), tablet, mobile, prop, unit);
        let base = desktop.unwrap_or_default();
        let css = base.format_css(unit).unwrap_or_default();
        return json!({
            "top": base.top,
            "right": base.right,
            "bottom": base.bottom,
            "left": base.left,
            "css": css,
            "classes": classes,
        });
    }

    let mut base = effective.and_then(extract_raw_spacing).unwrap_or_default();
    base.canonicalize();
    let css = base.format_css(unit).unwrap_or_default();
    let prefix = match prop {
        SpacingProperty::Padding => SpacingProperty::Padding,
        SpacingProperty::Margin => SpacingProperty::Margin,
    };
    let classes = base.format_classes(prefix, unit);
    json!({
        "top": base.top,
        "right": base.right,
        "bottom": base.bottom,
        "left": base.left,
        "css": css,
        "classes": classes,
    })
}

pub(super) fn hydrate_link_value(spec: &FieldSpec, raw_val: Option<&Value>) -> Value {
    let candidate = match raw_val {
        Some(v) if !v.is_null() => Some(v),
        _ => spec.default.as_ref(),
    };
    let Some(val) = candidate else {
        return Value::Null;
    };
    let Ok(link) = serde_json::from_value::<LinkValue>(val.clone()) else {
        return Value::Null;
    };
    let trimmed_url = link.url.trim();
    if trimmed_url.is_empty() || !valid_safe_url(trimmed_url) {
        return Value::Null;
    }
    let target = if link.new_tab { "_blank" } else { "" };
    let rel = match (link.new_tab, link.nofollow) {
        (true, true) => "noopener noreferrer nofollow",
        (true, false) => "noopener noreferrer",
        (false, true) => "nofollow",
        (false, false) => "",
    };
    json!({
        "url": trimmed_url,
        "label": link.label,
        "target": target,
        "rel": rel,
        "new_tab": link.new_tab,
        "nofollow": link.nofollow,
    })
}

pub(super) fn parse_i64_id(val: &Value) -> Option<i64> {
    val.as_i64().filter(|id| *id > 0)
}

pub(super) fn unwrap_responsive_desktop<'a>(
    spec: &FieldSpec,
    raw_val: Option<&'a Value>,
) -> Option<&'a Value> {
    let val = raw_val?;
    if spec.responsive
        && let Some(obj) = val.as_object()
        && (obj.contains_key("desktop") || obj.contains_key("tablet") || obj.contains_key("mobile"))
    {
        return obj.get("desktop");
    }
    Some(val)
}
