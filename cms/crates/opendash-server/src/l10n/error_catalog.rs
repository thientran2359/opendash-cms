use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use super::{CatalogRegistry, L10nScope};

const ERROR_CODES_JSON: &str = include_str!("../../../../l10n/error_codes.json");

#[derive(Deserialize)]
struct RawErrorCatalog {
    exact: Vec<(String, String, String)>, // (code, vi, en)
    templates: Vec<(String, Vec<String>, Vec<String>)>, // (code, vi_parts, en_parts)
}

struct CompiledErrorCatalog {
    exact_by_msg: HashMap<String, usize>,
    exact: Vec<(&'static str, String, String)>,
    templates: Vec<(&'static str, Vec<String>, Vec<String>)>,
}

fn compiled() -> &'static CompiledErrorCatalog {
    static CATALOG: OnceLock<CompiledErrorCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let raw: RawErrorCatalog =
            serde_json::from_str(ERROR_CODES_JSON).expect("valid cms/l10n/error_codes.json");
        let mut exact_by_msg = HashMap::with_capacity(raw.exact.len() * 2);
        let mut exact = Vec::with_capacity(raw.exact.len());
        for (idx, (code, vi, en)) in raw.exact.into_iter().enumerate() {
            let code_static: &'static str = Box::leak(code.into_boxed_str());
            exact_by_msg.entry(vi.clone()).or_insert(idx);
            exact_by_msg.entry(en.clone()).or_insert(idx);
            exact.push((code_static, vi, en));
        }
        let templates = raw
            .templates
            .into_iter()
            .map(|(code, vi_p, en_p)| {
                let code_static: &'static str = Box::leak(code.into_boxed_str());
                (code_static, vi_p, en_p)
            })
            .collect();
        CompiledErrorCatalog {
            exact_by_msg,
            exact,
            templates,
        }
    })
}

pub fn translate_from_error_catalog(
    raw_msg: &str,
    locale: &str,
    registry: Option<&CatalogRegistry>,
) -> Option<(&'static str, String)> {
    let cat = compiled();
    let lower = locale.trim().to_ascii_lowercase();
    let is_vi = lower == "vi" || lower.starts_with("vi-");

    if let Some(&idx) = cat.exact_by_msg.get(raw_msg) {
        let &(code_static, ref vi, ref en) = &cat.exact[idx];
        let translated = if let Some(reg) = registry
            && let Some(loaded) = reg.resolve(L10nScope::Core, "opendash", locale)
            && let Some(found) = loaded.catalog.get_with_context(None, en)
        {
            found.to_owned()
        } else if is_vi {
            vi.clone()
        } else {
            en.clone()
        };
        return Some((code_static, translated));
    }

    for &(code_static, ref vi_parts, ref en_parts) in &cat.templates {
        let vi_refs: Vec<&str> = vi_parts.iter().map(String::as_str).collect();
        let en_refs: Vec<&str> = en_parts.iter().map(String::as_str).collect();
        let captured =
            capture_template(raw_msg, &vi_refs).or_else(|| capture_template(raw_msg, &en_refs));
        if let Some(values) = captured {
            let en_template = build_indexed_template(&en_refs);
            let translated_tpl = if let Some(reg) = registry
                && let Some(loaded) = reg.resolve(L10nScope::Core, "opendash", locale)
                && let Some(found) = loaded.catalog.get_with_context(None, &en_template)
            {
                found.to_owned()
            } else if is_vi {
                build_indexed_template(&vi_refs)
            } else {
                en_template
            };
            let mut msg = translated_tpl;
            for (idx, val) in values.into_iter().enumerate() {
                msg = msg.replace(&format!("{{{idx}}}"), val);
            }
            return Some((code_static, msg));
        }
    }
    None
}

fn build_indexed_template(parts: &[&str]) -> String {
    let mut out = String::new();
    for (idx, p) in parts.iter().enumerate() {
        out.push_str(p);
        if idx + 1 < parts.len() {
            out.push_str(&format!("{{{idx}}}"));
        }
    }
    out
}

fn capture_template<'a>(raw: &'a str, pieces: &[&str]) -> Option<Vec<&'a str>> {
    if pieces.is_empty() {
        return None;
    }
    let rest = raw.strip_prefix(pieces[0])?;
    let mut rest = rest.strip_suffix(*pieces.last()?)?;
    let mut values = Vec::with_capacity(pieces.len().saturating_sub(1));
    for delimiter in &pieces[1..pieces.len() - 1] {
        if delimiter.is_empty() {
            return None;
        }
        let (value, remainder) = rest.split_once(delimiter)?;
        values.push(value);
        rest = remainder;
    }
    values.push(rest);
    Some(values)
}

#[cfg(test)]
pub fn all_exact_entries() -> &'static [(&'static str, String, String)] {
    &compiled().exact
}
