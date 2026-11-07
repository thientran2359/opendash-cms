//! Content domain identifiers and validation primitives.
//! Delegates identifier rules to `crate::id` as the single source of truth.

use crate::id::valid_snake_key;

/// Valid identifiers start with a lowercase ASCII letter and contain only
/// lowercase letters, digits, or underscores (`1..=64` chars).
pub fn valid_content_type_key(value: &str) -> bool {
    valid_snake_key(value)
}

#[cfg(test)]
mod tests {
    use super::valid_content_type_key;

    #[test]
    fn accepts_custom_post_type_keys() {
        assert!(valid_content_type_key("post"));
        assert!(valid_content_type_key("portfolio_item2"));
    }

    #[test]
    fn rejects_unstable_or_invalid_keys() {
        assert!(!valid_content_type_key(""));
        assert!(!valid_content_type_key("2post"));
        assert!(!valid_content_type_key("Post"));
        assert!(!valid_content_type_key("post/type"));
    }
}
