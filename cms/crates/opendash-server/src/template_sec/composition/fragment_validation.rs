use super::*;
/// Strict C-SEC HTML/URL/CSS validator for opt-out untrusted plugin fragments
/// (`render_trusted_html == false`). Forbids `<script>`, `<svg>`, `<style>`, `<meta>`,
/// `<link>`, `<!doctype>`, `<form>`, `on*`, `javascript:`/`data:` URLs, `formaction`/`action`,
/// and unsafe `style="..."` CSS (`url()`, `expression()`, `@import`, etc.).

pub(crate) fn validate_untrusted_extension_html_fragment(src: &str) -> Result<(), &'static str> {
    let lower = src.to_ascii_lowercase();
    for forbidden_prefix in ["<style", "</style", "<meta", "<link", "<!doctype"] {
        let mut cursor = 0;
        while let Some(rel) = lower[cursor..].find(forbidden_prefix) {
            let end = cursor + rel + forbidden_prefix.len();
            let boundary = lower
                .as_bytes()
                .get(end)
                .is_none_or(|b| b.is_ascii_whitespace() || matches!(*b, b'>' | b'/'));
            if boundary {
                return Err("Fragment untrusted chứa thẻ HTML bị C-SEC cấm.");
            }
            cursor = end;
        }
    }
    validate_html_and_css_context(src, true, false, false)
}

pub(crate) fn find_rawtext_or_rcdata_end(
    src: &str,
    body_start: usize,
    tag_name: &str,
) -> Result<(usize, usize), &'static str> {
    let bytes = src.as_bytes();
    let needle: &[u8] = match tag_name {
        "style" => b"</style",
        "script" => b"</script",
        "title" => b"</title",
        _ => return Err("Thẻ raw-text không hợp lệ."),
    };
    let mut pos = body_start;
    while pos + needle.len() <= bytes.len() {
        if bytes[pos..pos + needle.len()].eq_ignore_ascii_case(needle) {
            match bytes.get(pos + needle.len()) {
                Some(&b'>') => return Ok((pos, pos + needle.len() + 1)),
                Some(&b) if is_ascii_html_whitespace(b) || b == b'/' => {
                    return Err("Thẻ đóng raw-text không được chứa khoảng trắng hoặc /.");
                }
                None => return Err("Thẻ đóng raw-text không hoàn chỉnh."),
                Some(_) => {}
            }
        }
        pos += 1;
    }
    Err("Thẻ raw-text thiếu thẻ đóng hợp lệ.")
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PostRenderTagScanState {
    BeforeAttrName,
    AttrName,
    AfterAttrName,
    BeforeAttrValue,
    QuotedValue(u8),
    UnquotedValue,
}

pub(crate) fn find_post_render_tag_end(bytes: &[u8], start: usize) -> Result<usize, &'static str> {
    let mut i = start;
    let mut state = PostRenderTagScanState::BeforeAttrName;
    while i < bytes.len() {
        let b = bytes[i];
        match state {
            PostRenderTagScanState::BeforeAttrName => {
                if b == b'>' {
                    return Ok(i);
                }
                if b.is_ascii_whitespace() || b == b'/' {
                    // stay
                } else if matches!(b, b'"' | b'\'' | b'<' | b'=' | b'`') {
                    return Err("Thẻ HTML chứa ký tự không hợp lệ ở vị trí tên thuộc tính.");
                } else {
                    state = PostRenderTagScanState::AttrName;
                }
            }
            PostRenderTagScanState::AttrName => {
                if b == b'>' {
                    return Ok(i);
                }
                if b.is_ascii_whitespace() {
                    state = PostRenderTagScanState::AfterAttrName;
                } else if b == b'/' {
                    state = PostRenderTagScanState::BeforeAttrName;
                } else if b == b'=' {
                    state = PostRenderTagScanState::BeforeAttrValue;
                } else if matches!(b, b'"' | b'\'' | b'<' | b'`') {
                    return Err("Tên thuộc tính HTML chứa ký tự không hợp lệ.");
                }
            }
            PostRenderTagScanState::AfterAttrName => {
                if b == b'>' {
                    return Ok(i);
                }
                if b.is_ascii_whitespace() {
                    // stay
                } else if b == b'/' {
                    state = PostRenderTagScanState::BeforeAttrName;
                } else if b == b'=' {
                    state = PostRenderTagScanState::BeforeAttrValue;
                } else if matches!(b, b'"' | b'\'' | b'<' | b'`') {
                    return Err("Cú pháp thẻ HTML chứa ký tự không hợp lệ sau tên thuộc tính.");
                } else {
                    state = PostRenderTagScanState::AttrName;
                }
            }
            PostRenderTagScanState::BeforeAttrValue => {
                if b.is_ascii_whitespace() {
                    // stay
                } else if b == b'>' {
                    return Ok(i);
                } else if b == b'"' || b == b'\'' {
                    state = PostRenderTagScanState::QuotedValue(b);
                } else if matches!(b, b'<' | b'=' | b'`') {
                    return Err("Giá trị thuộc tính HTML bắt đầu bằng ký tự không hợp lệ.");
                } else {
                    state = PostRenderTagScanState::UnquotedValue;
                }
            }
            PostRenderTagScanState::QuotedValue(q) => {
                if b == q {
                    state = PostRenderTagScanState::BeforeAttrName;
                }
            }
            PostRenderTagScanState::UnquotedValue => {
                if b == b'>' {
                    return Ok(i);
                }
                if b.is_ascii_whitespace() {
                    state = PostRenderTagScanState::BeforeAttrName;
                } else if matches!(b, b'"' | b'\'' | b'<' | b'=' | b'`') {
                    return Err(
                        "Giá trị thuộc tính HTML không có dấu nháy chứa ký tự không hợp lệ.",
                    );
                }
            }
        }
        i += 1;
    }
    Err("Thẻ HTML không đóng đúng cú pháp.")
}

pub(crate) fn parse_attributes(
    slice: &str,
    post_render: bool,
) -> Result<BTreeMap<String, String>, &'static str> {
    let mut map = BTreeMap::new();
    let bytes = slice.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        // Skip Jinja control/expr tokens inside tag ONLY during pre-render template
        // validation. Post-render HTML must parse every token as raw HTML attributes
        // so `{{ href="javascript:..." }}` cannot smuggle attributes past C-SEC.
        if !post_render
            && bytes[i] == b'{'
            && i + 1 < bytes.len()
            && (bytes[i + 1] == b'%' || bytes[i + 1] == b'{')
        {
            let needle = if bytes[i + 1] == b'%' { "%}" } else { "}}" };
            if let Some(end) = slice[i + 2..].find(needle) {
                i += 2 + end + 2;
                continue;
            }
            return Err("Biểu thức template trong thẻ HTML không đóng.");
        }
        let name_start = i;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'='
            && bytes[i] != b'/'
        {
            if post_render && matches!(bytes[i], b'"' | b'\'' | b'<' | b'>' | b'`') {
                return Err("Tên thuộc tính HTML chứa ký tự không hợp lệ.");
            }
            i += 1;
        }
        let name = slice[name_start..i].to_ascii_lowercase();
        if post_render && name.is_empty() {
            return Err("Cú pháp thuộc tính HTML không hợp lệ.");
        }
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let val = if i < bytes.len() && bytes[i] == b'=' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                let q = bytes[i];
                i += 1;
                let v_start = i;
                while i < bytes.len() && bytes[i] != q {
                    i += 1;
                }
                if i >= bytes.len() {
                    return Err("Giá trị thuộc tính HTML thiếu dấu đóng ngoặc.");
                }
                let v = slice[v_start..i].to_string();
                i += 1;
                v
            } else {
                let v_start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    if post_render && matches!(bytes[i], b'"' | b'\'' | b'<' | b'>' | b'=' | b'`') {
                        return Err(
                            "Giá trị thuộc tính HTML không có dấu nháy chứa ký tự không hợp lệ.",
                        );
                    }
                    i += 1;
                }
                slice[v_start..i].to_string()
            }
        } else {
            String::new()
        };
        if !name.is_empty() && map.insert(name, val).is_some() {
            return Err("Thẻ HTML chứa thuộc tính bị trùng lặp.");
        }
    }
    Ok(map)
}

pub(crate) fn is_forbidden_block_shadow_name(id: &str) -> bool {
    matches!(
        id,
        "options"
            | "site_options"
            | "theme_options"
            | "menus"
            | "od_is_preview"
            | "od_icon"
            | "od_image"
            | "od_hook"
    )
}

pub(crate) const BLOCK_HTML_STEP_OPTIONS: HtmlStepOptions = HtmlStepOptions {
    is_part: false,
    is_full_doctype_doc: false,
    allow_trusted_theme_markup: true,
    allow_search_form: false,
    forbid_style_tag: true,
};
