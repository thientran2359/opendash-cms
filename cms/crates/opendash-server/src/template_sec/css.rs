use super::*;

/// Validates a CSS stylesheet (both `<style>` blocks and `/theme/*.css` files).
pub fn validate_safe_css(css: &str) -> Result<(), &'static str> {
    let lower = css.to_ascii_lowercase();
    if lower.contains("</style")
        || lower.contains("@import")
        || lower.contains("expression(")
        || lower.contains("-moz-binding")
        || lower.contains("behavior:")
    {
        return Err("CSS vi phạm chính sách C-SEC (chứa @import/expression/binding/breakout).");
    }

    let mut idx = 0;
    while let Some(pos) = lower[idx..].find("url(") {
        let start = idx + pos + 4;
        let Some(end_rel) = lower[start..].find(')') else {
            return Err("CSS url(...) không hợp lệ.");
        };
        let raw_inner = css[start..start + end_rel].trim();
        let inner = raw_inner
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| {
                raw_inner
                    .strip_prefix('\'')
                    .and_then(|s| s.strip_suffix('\''))
            })
            .unwrap_or(raw_inner)
            .trim();
        if !(inner.starts_with("/theme/") || inner.starts_with("/media/")) || inner.contains("..") {
            return Err("CSS url(...) chỉ được phép trỏ tới /theme/... hoặc /media/....");
        }
        idx = start + end_rel + 1;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn finalize_theme_attr(
    tag_name: &str,
    attr_name: &mut String,
    quote: &mut Option<u8>,
    attr_has_static_bytes: &mut bool,
    attr_expr_count: &mut u8,
    seen_attrs: &mut BTreeSet<String>,
    link_rel_stylesheet: &mut bool,
    link_has_valid_href: &mut bool,
    meta_ok: &mut bool,
    attr_static_buf: &mut String,
    allow_trusted_theme_markup: bool,
) -> Result<(), &'static str> {
    if attr_name.is_empty() {
        return Ok(());
    }
    if !seen_attrs.insert(attr_name.clone()) {
        return Err("Thẻ HTML chứa thuộc tính bị trùng lặp.");
    }
    if is_dangerous_attr_name(attr_name, allow_trusted_theme_markup) {
        return Err("Template chứa thuộc tính HTML bị C-SEC cấm.");
    }
    if forbids_dynamic_interpolation(attr_name)
        && (*attr_expr_count != 0
            || attr_static_buf.contains("{{")
            || attr_static_buf.contains("{%"))
    {
        return Err("C-SEC cấm nội suy trong thuộc tính sự kiện/namespace HTML.");
    }
    if tag_name == "meta" {
        if !matches!(attr_name.as_str(), "charset" | "name" | "content") {
            return Err("C-SEC chỉ cho phép <meta charset> hoặc <meta name=\"viewport\">.");
        }
        if attr_name == "charset" {
            *meta_ok = true;
        } else if attr_name == "name" {
            if *attr_expr_count != 0 || !attr_static_buf.trim().eq_ignore_ascii_case("viewport") {
                return Err("C-SEC chỉ cho phép <meta charset> hoặc <meta name=\"viewport\">.");
            }
            *meta_ok = true;
        }
    }
    if tag_name == "link" {
        if attr_name == "rel"
            && *attr_expr_count == 0
            && attr_static_buf.trim().eq_ignore_ascii_case("stylesheet")
        {
            *link_rel_stylesheet = true;
        } else if attr_name == "href" && *attr_expr_count == 0 {
            *link_has_valid_href = is_valid_theme_stylesheet_href(attr_static_buf);
        }
    }
    if attr_name == "style" && *attr_expr_count == 0 {
        validate_safe_css(attr_static_buf)?;
    }
    if matches!(attr_name.as_str(), "srcset" | "ping") && *attr_expr_count == 0 {
        let decoded = decode_html_entities_for_url(attr_static_buf);
        if decoded.trim().is_empty()
            || decoded.contains('\0')
            || decoded.bytes().any(|b| b <= 0x1f || b == 0x7f)
        {
            return Err("Thuộc tính URL danh sách không hợp lệ.");
        }
        if attr_name == "srcset" {
            for candidate in decoded.split(',') {
                let url_part = candidate
                    .split_whitespace()
                    .next()
                    .ok_or("Thuộc tính srcset chứa mục rỗng.")?;
                validate_safe_url(url_part)?;
            }
        } else {
            for target in decoded.split_whitespace() {
                validate_safe_url(target)?;
            }
        }
    }
    if matches!(
        attr_name.as_str(),
        "href" | "src" | "poster" | "cite" | "xlink:href"
    ) {
        if *attr_expr_count > 1 || (*attr_expr_count > 0 && *attr_has_static_bytes) {
            return Err("URL động trong thuộc tính chỉ được phép là biểu thức {{ ... }} đơn.");
        }
        if *attr_expr_count == 0 && !(tag_name == "link" && attr_name == "href") {
            let decoded = decode_html_entities_for_url(attr_static_buf);
            if (tag_name == "a" && attr_name == "href") || attr_name == "xlink:href" {
                validate_safe_href_url(&decoded)?;
            } else {
                validate_safe_url(&decoded)?;
            }
        }
    }
    attr_name.clear();
    *quote = None;
    *attr_has_static_bytes = false;
    *attr_expr_count = 0;
    attr_static_buf.clear();
    Ok(())
}

pub(super) fn validate_html_and_css_context(
    src: &str,
    post_render: bool,
    allow_trusted_theme_markup: bool,
) -> Result<(), &'static str> {
    let lower = src.to_ascii_lowercase();
    let is_full_doctype_doc = lower.trim_start().starts_with("<!doctype");

    // Scan HTML plus RAWTEXT style/script and RCDATA title. SVG depth limits
    // the exceptional <title> permission to actual SVG foreign content.
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut svg_depth = 0_usize;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if lower[i..].starts_with("<!--") {
            if let Some(end) = find_html5_comment_end_in_template(bytes, i) {
                i = end;
                continue;
            }
            return Err("HTML comment không đóng.");
        }
        if bytes.get(i + 1) == Some(&b'?') {
            return Err("Template không được chứa processing instruction `<?`.");
        }
        if bytes.get(i + 1) == Some(&b'!') {
            if lower[i..].starts_with("<!doctype")
                && bytes
                    .get(i + 9)
                    .is_some_and(|b| *b == b'>' || is_ascii_html_whitespace(*b))
            {
                let mut d = i + 9;
                while d < bytes.len() && bytes[d] != b'>' {
                    if matches!(bytes[d], b'<' | b'"' | b'\'' | 0x00..=0x1f | 0x7f) {
                        return Err("<!DOCTYPE> chứa ký tự không hợp lệ.");
                    }
                    d += 1;
                }
                if d >= bytes.len() {
                    return Err("DOCTYPE không đóng.");
                }
                i = d + 1;
                continue;
            }
            return Err("HTML chỉ cho phép comment <!-- hoặc <!DOCTYPE>.");
        }

        let is_close = if bytes.get(i + 1) == Some(&b'/') {
            if !bytes.get(i + 2).is_some_and(u8::is_ascii_alphabetic) {
                return Err("Thẻ đóng HTML phải bắt đầu bằng ký tự chữ cái ASCII.");
            }
            true
        } else if bytes.get(i + 1).is_some_and(u8::is_ascii_alphabetic) {
            false
        } else {
            // Plain text '<' (e.g. "1 < 2") per WHATWG HTML data state.
            i += 1;
            continue;
        };

        let mut j = i + if is_close { 2 } else { 1 };
        let name_start = j;
        while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'-') {
            j += 1;
        }
        let tag_name = lower[name_start..j].to_string();
        if tag_name.is_empty() {
            return Err("Tên thẻ HTML không hợp lệ.");
        }
        if is_close
            && matches!(
                tag_name.as_str(),
                "style" | "title" | "script" | "link" | "meta"
            )
        {
            return Err("Thẻ đóng HTML không hợp lệ ở vị trí này.");
        }
        if FORBIDDEN_TAGS.contains(&tag_name.as_str())
            && !(allow_trusted_theme_markup
                && matches!(
                    tag_name.as_str(),
                    "script" | "svg" | "foreignobject" | "math"
                ))
            && !(tag_name == "title"
                && !is_close
                && (is_full_doctype_doc || (allow_trusted_theme_markup && svg_depth > 0)))
        {
            return Err("Template chứa thẻ HTML bị C-SEC cấm.");
        }

        let k = if post_render {
            find_post_render_tag_end(bytes, j)?
        } else {
            let mut k = j;
            let mut quote: Option<u8> = None;
            while k < bytes.len() {
                let b = bytes[k];
                if let Some(q) = quote {
                    if b == q {
                        quote = None;
                    }
                } else if b == b'"' || b == b'\'' {
                    quote = Some(b);
                } else if b == b'>' {
                    break;
                }
                k += 1;
            }
            if k >= bytes.len() {
                return Err("Thẻ HTML không đóng đúng cú pháp.");
            }
            k
        };

        let attr_slice = &src[j..k];
        if is_close {
            if !attr_slice.trim().is_empty() {
                return Err("Thẻ đóng HTML không được chứa thuộc tính hoặc /.");
            }
            if tag_name == "svg" {
                svg_depth = svg_depth
                    .checked_sub(1)
                    .ok_or("Thẻ </svg> không có thẻ mở.")?;
            }
            i = k + 1;
            continue;
        }

        validate_tag_attributes(
            &tag_name,
            attr_slice,
            post_render,
            allow_trusted_theme_markup,
        )?;
        let self_closing = attr_slice.trim_end().ends_with('/');
        if matches!(tag_name.as_str(), "style" | "title" | "script") && self_closing {
            return Err("Thẻ <style>/<title>/<script> không được tự đóng.");
        }
        if tag_name == "svg" && !self_closing {
            svg_depth = svg_depth
                .checked_add(1)
                .filter(|depth| *depth <= 64)
                .ok_or("SVG lồng nhau quá sâu.")?;
        }

        if tag_name == "style" {
            let (close_start, next_i) = find_rawtext_or_rcdata_end(src, k + 1, "style")?;
            let css_body = &src[k + 1..close_start];
            if !post_render && (css_body.contains("{{") || css_body.contains("{%")) {
                return Err("C-SEC cấm nội suy {{ ... }} bên trong <style>.");
            }
            validate_safe_css(css_body)?;
            i = next_i;
            continue;
        }
        if tag_name == "title" {
            let (_close_start, next_i) = find_rawtext_or_rcdata_end(src, k + 1, "title")?;
            i = next_i;
            continue;
        }
        if tag_name == "script" {
            let (close_start, next_i) = find_rawtext_or_rcdata_end(src, k + 1, "script")?;
            let js_body = &src[k + 1..close_start];
            // HTML5 escaped script states after a comment opener can keep a
            // literal closing tag inside JavaScript instead of closing it.
            // Reject that opener so template data cannot become executable JS.
            if js_body.contains("<!--") || js_body.contains("<![CDATA[") {
                return Err("C-SEC cam HTML escaped state va SVG CDATA trong <script>.");
            }
            if !post_render && (js_body.contains("{{") || js_body.contains("{%")) {
                return Err("C-SEC cấm nội suy Jinja bên trong <script>.");
            }
            i = next_i;
            continue;
        }
        i = k + 1;
    }
    if svg_depth != 0 {
        return Err("SVG thiếu thẻ đóng.");
    }
    Ok(())
}

pub(super) fn validate_tag_attributes(
    tag_name: &str,
    attr_slice: &str,
    post_render: bool,
    allow_trusted_theme_markup: bool,
) -> Result<(), &'static str> {
    let attrs = parse_attributes(attr_slice, post_render)?;

    if tag_name == "meta" {
        if !attrs
            .keys()
            .all(|k| matches!(k.as_str(), "charset" | "name" | "content"))
        {
            return Err("C-SEC chỉ cho phép <meta charset> hoặc <meta name=\"viewport\">.");
        }
        let allowed_meta = attrs.contains_key("charset")
            || attrs
                .get("name")
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("viewport"));
        if !allowed_meta {
            return Err("C-SEC chỉ cho phép <meta charset> hoặc <meta name=\"viewport\">.");
        }
    }
    if tag_name == "link" {
        let rel_ok = attrs
            .get("rel")
            .is_some_and(|v| v.trim().eq_ignore_ascii_case("stylesheet"));
        let href_ok = attrs
            .get("href")
            .is_some_and(|v| is_valid_theme_stylesheet_href(v));
        if !(rel_ok && href_ok) {
            return Err("C-SEC chỉ cho phép <link rel=\"stylesheet\" href=\"/theme/...\">.");
        }
    }

    for (name, val) in attrs {
        if is_dangerous_attr_name(&name, allow_trusted_theme_markup) {
            return Err("Template chứa thuộc tính HTML bị C-SEC cấm.");
        }
        if !post_render
            && forbids_dynamic_interpolation(&name)
            && (val.contains("{{") || val.contains("{%"))
        {
            return Err("C-SEC cấm nội suy bên trong thuộc tính sự kiện/namespace HTML.");
        }

        if name == "style" {
            if !post_render && (val.contains("{{") || val.contains("{%")) {
                return Err("C-SEC cấm nội suy {{ ... }} bên trong thuộc tính style.");
            }
            validate_safe_css(&val)?;
        }

        if matches!(
            name.as_str(),
            "href" | "src" | "poster" | "cite" | "srcset" | "ping" | "xlink:href"
        ) {
            if !post_render && (val.contains("{{") || val.contains("{%")) {
                return Err("URL động trong thuộc tính không nằm trong whitelist typed URL.");
            } else {
                let decoded = decode_html_entities_for_url(&val);
                if name == "srcset" {
                    if decoded.trim().is_empty()
                        || decoded.contains('\0')
                        || decoded.bytes().any(|b| b <= 0x1f || b == 0x7f)
                    {
                        return Err("Thuộc tính srcset không hợp lệ.");
                    }
                    for candidate in decoded.split(',') {
                        let url_part = candidate
                            .split_whitespace()
                            .next()
                            .ok_or("Thuộc tính srcset chứa mục rỗng.")?;
                        validate_safe_url(url_part)?;
                    }
                } else if name == "ping" {
                    if decoded.trim().is_empty()
                        || decoded.contains('\0')
                        || decoded.bytes().any(|b| b <= 0x1f || b == 0x7f)
                    {
                        return Err("Thuộc tính ping không hợp lệ.");
                    }
                    for target in decoded.split_whitespace() {
                        validate_safe_url(target)?;
                    }
                } else if (tag_name == "a" && name == "href") || name == "xlink:href" {
                    validate_safe_href_url(&decoded)?;
                } else {
                    validate_safe_url(&decoded)?;
                }
            }
        }
    }

    Ok(())
}
