pub mod catalog;
pub mod mo;
pub mod plural;
pub mod po;

pub use catalog::{
    Catalog, CatalogEntry, CatalogLookup, PotDocument, PotEntry, validate_placeholders,
};
pub use mo::parse_mo;
pub use plural::PluralRule;
pub use po::{compile_po_to_mo, parse_po};

#[cfg(test)]
mod tests;

pub const MAX_MO_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_PO_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_CATALOG_ENTRIES: usize = 16_384;
pub const MAX_ENTRY_BYTES: usize = 64 * 1024;
pub const MAX_MATERIALIZED_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_PLURAL_FORMS: usize = 6;
pub const MAX_SITE_LANGUAGE_FILES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct L10nError {
    pub message: String,
}

impl L10nError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for L10nError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for L10nError {}

/// Canonicalizes a locale code (`_` -> `-`, lowercase) and validates `^[a-z]{2,3}(-[a-z0-9]{2,8})*$`.
pub fn canonicalize_locale_code(raw: &str) -> Result<String, L10nError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 32 || !trimmed.is_ascii() {
        return Err(L10nError::new("Mã ngôn ngữ không hợp lệ."));
    }
    let normalized = trimmed.replace('_', "-").to_ascii_lowercase();
    let mut parts = normalized.split('-');
    let Some(lang) = parts.next() else {
        return Err(L10nError::new("Mã ngôn ngữ không hợp lệ."));
    };
    if !(2..=3).contains(&lang.len()) || !lang.bytes().all(|b| b.is_ascii_lowercase()) {
        return Err(L10nError::new(
            "Mã ngôn ngữ chính phải gồm 2-3 ký tự ASCII.",
        ));
    }
    for sub in parts {
        if !(2..=8).contains(&sub.len())
            || !sub
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        {
            return Err(L10nError::new(
                "Thành phần phụ của mã ngôn ngữ không hợp lệ.",
            ));
        }
    }
    Ok(normalized)
}

/// Validates a theme/plugin text domain (`kebab-case`, <= 32 chars, `"opendash"` reserved for Core).
pub fn validate_extension_text_domain(domain: &str) -> Result<(), &'static str> {
    if domain == "opendash" {
        return Err("text_domain 'opendash' được dành riêng cho Core CMS.");
    }
    crate::id::valid_kebab_segment_with_limit(domain, 32)
        .then_some(())
        .ok_or("text_domain phải là kebab-case tối đa 32 ký tự.")
}
