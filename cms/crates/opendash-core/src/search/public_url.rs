use std::collections::BTreeSet;

use crate::{content::valid_content_type_key, search::sanitize_search_query};

pub const SEARCH_PAGE_SIZE: i64 = 20;
pub const MAX_SEARCH_PAGE: i64 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicSearchQuery {
    pub query: String,
    pub has_query: bool,
    pub content_type: Option<String>,
    pub page: i64,
}

impl Default for PublicSearchQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            has_query: false,
            content_type: None,
            page: 1,
        }
    }
}

pub fn validate_percent_encoding(raw: Option<&str>) -> Result<(), &'static str> {
    let raw = raw.unwrap_or("");
    let bytes = raw.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == b'%'
            && (index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit())
        {
            return Err("Query có mã hóa không hợp lệ.");
        }
    }
    Ok(())
}

fn decode_form_component(raw: &str) -> Result<String, &'static str> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' => {
                let hi = bytes
                    .get(i + 1)
                    .copied()
                    .ok_or("Query có mã hóa không hợp lệ.")?;
                let lo = bytes
                    .get(i + 2)
                    .copied()
                    .ok_or("Query có mã hóa không hợp lệ.")?;
                let hex = [hi, lo];
                let hex_str =
                    std::str::from_utf8(&hex).map_err(|_| "Query có mã hóa không hợp lệ.")?;
                let byte =
                    u8::from_str_radix(hex_str, 16).map_err(|_| "Query có mã hóa không hợp lệ.")?;
                out.push(byte);
                i += 3;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| "Query chứa chuỗi UTF-8 không hợp lệ.")
}

pub fn has_search_param(raw: Option<&str>) -> bool {
    let Some(raw) = raw else {
        return false;
    };
    let trimmed = raw.strip_prefix('?').unwrap_or(raw);
    for pair in trimmed.split('&') {
        if pair.is_empty() {
            continue;
        }
        let key_raw = pair.split_once('=').map_or(pair, |(k, _)| k);
        if key_raw == "s" {
            return true;
        }
        if let Ok(decoded) = decode_form_component(key_raw)
            && decoded == "s"
        {
            return true;
        }
    }
    false
}

pub fn parse_search_query(raw: Option<&str>) -> Result<PublicSearchQuery, &'static str> {
    validate_percent_encoding(raw)?;
    let raw = raw.unwrap_or("");
    let raw = raw.strip_prefix('?').unwrap_or(raw);
    let mut out = PublicSearchQuery::default();
    let mut seen_known = BTreeSet::new();

    for pair in raw.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key_raw, value_raw) = pair.split_once('=').unwrap_or((pair, ""));
        let key = decode_form_component(key_raw)?;
        let value = decode_form_component(value_raw)?;
        match key.as_str() {
            "s" => {
                if !seen_known.insert("s") {
                    return Err("Tham số query không được lặp lại.");
                }
                let sanitized = sanitize_search_query(&value)
                    .map_err(|_| "Từ khóa tìm kiếm vượt quá giới hạn.")?;
                out.has_query = !sanitized.is_empty;
                out.query = sanitized.display_query;
            }
            "type" => {
                if !seen_known.insert("type") {
                    return Err("Tham số query không được lặp lại.");
                }
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    if !valid_content_type_key(trimmed) {
                        return Err("Loại nội dung tìm kiếm không hợp lệ.");
                    }
                    out.content_type = Some(trimmed.to_owned());
                }
            }
            "page" => {
                if !seen_known.insert("page") {
                    return Err("Tham số query không được lặp lại.");
                }
                let page: i64 = value
                    .parse()
                    .map_err(|_| "Số trang tìm kiếm không hợp lệ.")?;
                if !(1..=MAX_SEARCH_PAGE).contains(&page) {
                    return Err("Số trang tìm kiếm vượt quá giới hạn.");
                }
                out.page = page;
            }
            "category_id"
            | "tag_id"
            | "taxonomy_key"
            | "term_id"
            | "include_descendants"
            | "descendants" => {
                return Err("Tham số query không dùng được trên route này.");
            }
            _ => {}
        }
    }

    Ok(out)
}

pub fn build_search_url(
    base_path: &str,
    query: Option<&str>,
    content_type: Option<&str>,
    page: i64,
) -> String {
    let base = if base_path.is_empty() || base_path == "/" {
        "/"
    } else {
        base_path.trim_end_matches('/')
    };
    let Some(query_str) = query else {
        return base.to_owned();
    };
    let mut pairs = url::form_urlencoded::Serializer::new(String::new());
    pairs.append_pair("s", query_str);
    if let Some(type_key) = content_type.map(str::trim).filter(|t| !t.is_empty()) {
        pairs.append_pair("type", type_key);
    }
    if page > 1 {
        pairs.append_pair("page", &page.to_string());
    }
    format!("{base}?{}", pairs.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_percent_encoding_and_detects_search_param() {
        assert!(validate_percent_encoding(Some("s=ki%E1%BA%BFn+tr%C3%BAc")).is_ok());
        assert!(validate_percent_encoding(Some("s=abc%ZZ")).is_err());
        assert!(validate_percent_encoding(Some("%")).is_err());
        assert!(has_search_param(Some("s=")));
        assert!(has_search_param(Some("utm_source=x&s=abc")));
        assert!(has_search_param(Some("%73=abc")));
        assert!(!has_search_param(Some("type=post&page=2")));
    }

    #[test]
    fn parses_search_query_and_rejects_archive_duplicates_or_bad_page() {
        let parsed = parse_search_query(Some(
            "s=Ki%E1%BA%BFn+tr%C3%BAc&type=post&page=2&utm_source=x&utm_source=y",
        ))
        .unwrap();
        assert_eq!(parsed.query, "Kiến trúc");
        assert!(parsed.has_query);
        assert_eq!(parsed.content_type.as_deref(), Some("post"));
        assert_eq!(parsed.page, 2);

        let empty = parse_search_query(Some("s=&type=")).unwrap();
        assert!(!empty.has_query);
        assert_eq!(empty.query, "");
        assert_eq!(empty.content_type, None);
        assert_eq!(empty.page, 1);

        assert!(parse_search_query(Some("s=a&s=b")).is_err());
        assert!(parse_search_query(Some("s=a&category_id=1")).is_err());
        assert!(parse_search_query(Some("s=a&page=0")).is_err());
        assert!(parse_search_query(Some("s=a&page=101")).is_err());
        assert!(parse_search_query(Some("s=a&type=BAD!")).is_err());
        assert!(parse_search_query(Some("s=%FF")).is_err());
    }

    #[test]
    fn formats_canonical_search_urls() {
        assert_eq!(build_search_url("/", None, None, 1), "/");
        assert_eq!(build_search_url("/en/", None, None, 1), "/en");
        assert_eq!(build_search_url("/", Some(""), None, 1), "/?s=");
        assert_eq!(build_search_url("/en", Some(""), None, 1), "/en?s=");
        assert_eq!(
            build_search_url("/en", Some("nhà đẹp"), Some("post"), 1),
            "/en?s=nh%C3%A0+%C4%91%E1%BA%B9p&type=post"
        );
        assert_eq!(build_search_url("/", Some("a"), None, 3), "/?s=a&page=3");
    }
}
