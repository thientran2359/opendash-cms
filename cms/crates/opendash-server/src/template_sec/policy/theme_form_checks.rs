use super::*;

pub(crate) fn is_search_form_tag(tag_name: &str) -> bool {
    matches!(
        tag_name,
        "form" | "input" | "select" | "option" | "optgroup"
    )
}

pub(crate) fn is_non_void_search_form_tag(tag_name: &str) -> bool {
    matches!(tag_name, "form" | "select" | "option" | "optgroup")
}

/// Validates static `<form action="...">` URLs strictly within local non-reserved routes.
pub(crate) fn validate_safe_form_action(raw: &str) -> Result<(), &'static str> {
    let decoded = decode_html_entities_for_url(raw);
    if decoded.bytes().any(|b| b <= 0x1f || b == 0x7f) {
        return Err("C-SEC cấm ký tự điều khiển trong action của <form>.");
    }
    let trimmed = decoded.trim();
    if trimmed.is_empty()
        || trimmed.chars().any(char::is_whitespace)
        || !trimmed.starts_with('/')
        || trimmed.contains('\\')
        || trimmed.contains('?')
        || trimmed.contains('#')
        || trimmed.contains('%')
        || trimmed.contains("//")
        || trimmed.contains("..")
    {
        return Err("C-SEC chỉ cho phép <form action> là đường dẫn nội bộ an toàn.");
    }
    if trimmed != "/" {
        for seg in trimmed[1..].split('/') {
            if seg.is_empty() || seg == "." || seg == ".." {
                return Err("C-SEC cấm phân đoạn rỗng hoặc chấm trong action của <form>.");
            }
        }
        let first_seg = trimmed[1..].split('/').next().unwrap_or("");
        if opendash_core::id::is_reserved_system_root(&first_seg.to_ascii_lowercase()) {
            return Err("C-SEC cấm <form action> trỏ vào tiền tố hệ thống được bảo vệ.");
        }
    }
    Ok(())
}

pub(crate) fn advance_depths(
    depths: HtmlDepths,
    tag_name: &str,
    is_close: bool,
    self_closing: bool,
) -> Result<HtmlDepths, &'static str> {
    let mut next = depths;
    match tag_name {
        "svg" => {
            next.svg = if is_close {
                depths
                    .svg
                    .checked_sub(1)
                    .ok_or("Thẻ </svg> không có thẻ mở.")?
            } else if self_closing {
                depths.svg
            } else {
                depths
                    .svg
                    .checked_add(1)
                    .filter(|d| *d <= 64)
                    .ok_or("SVG lồng nhau quá sâu.")?
            };
        }
        "math" => {
            next.math = if is_close {
                depths
                    .math
                    .checked_sub(1)
                    .ok_or("Thẻ </math> không có thẻ mở.")?
            } else if self_closing {
                depths.math
            } else {
                depths
                    .math
                    .checked_add(1)
                    .filter(|d| *d <= 64)
                    .ok_or("MathML lồng nhau quá sâu.")?
            };
        }
        "form" => {
            next.form = if is_close {
                depths
                    .form
                    .checked_sub(1)
                    .ok_or("Thẻ </form> không có thẻ mở.")?
            } else if self_closing {
                return Err("Thẻ <form> không được tự đóng.");
            } else if depths.form > 0 {
                return Err("C-SEC cấm lồng thẻ <form> bên trong <form>.");
            } else {
                1
            };
        }
        _ => {}
    }
    Ok(next)
}

pub(crate) fn validate_search_form_attr_value(
    tag_name: &str,
    attr_name: &str,
    attr_static_buf: &str,
    attr_expr_count: u8,
    form_has_action: &mut bool,
) -> Result<(), &'static str> {
    match (tag_name, attr_name) {
        ("form", "method") => {
            if attr_expr_count != 0 {
                return Err("C-SEC cấm nội suy động trong thuộc tính method của <form>.");
            }
            let decoded = decode_html_entities_for_url(attr_static_buf);
            if !decoded.trim().eq_ignore_ascii_case("get") {
                return Err("C-SEC chỉ cho phép <form method=\"get\">.");
            }
        }
        ("form", "action") => {
            if attr_expr_count == 0 {
                validate_safe_form_action(attr_static_buf)?;
            }
            *form_has_action = true;
        }
        ("input", "type") => {
            if attr_expr_count != 0 {
                return Err("C-SEC cấm nội suy động trong type của <input>.");
            }
            let decoded = decode_html_entities_for_url(attr_static_buf);
            let ty = decoded.trim().to_ascii_lowercase();
            if !matches!(
                ty.as_str(),
                "search" | "text" | "hidden" | "submit" | "reset" | "radio"
            ) {
                return Err("C-SEC chỉ cho phép input type search/text/hidden/submit/reset/radio.");
            }
        }
        ("button", "type") => {
            if attr_expr_count != 0 {
                return Err("C-SEC cấm nội suy động trong type của <button>.");
            }
            let decoded = decode_html_entities_for_url(attr_static_buf);
            let ty = decoded.trim().to_ascii_lowercase();
            if !matches!(ty.as_str(), "submit" | "reset" | "button") {
                return Err("C-SEC chỉ cho phép button type submit/reset/button.");
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn validate_post_render_form_attrs(
    tag_name: &str,
    attrs: &BTreeMap<String, String>,
) -> Result<(), &'static str> {
    if tag_name == "form" {
        let mut form_has_action = false;
        if let Some(action) = attrs.get("action") {
            validate_search_form_attr_value("form", "action", action, 0, &mut form_has_action)?;
        }
        if !form_has_action {
            return Err("C-SEC yêu cầu <form> có thuộc tính action hợp lệ.");
        }
        if let Some(method) = attrs.get("method") {
            let mut unused = false;
            validate_search_form_attr_value("form", "method", method, 0, &mut unused)?;
        }
    } else if tag_name == "input"
        && let Some(ty) = attrs.get("type")
    {
        let mut unused = false;
        validate_search_form_attr_value("input", "type", ty, 0, &mut unused)?;
    } else if tag_name == "button"
        && let Some(ty) = attrs.get("type")
    {
        let mut unused = false;
        validate_search_form_attr_value("button", "type", ty, 0, &mut unused)?;
    }
    Ok(())
}
