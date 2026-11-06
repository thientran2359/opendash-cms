//! Small example of validating identifiers for future custom post types.
//! Actual CPT definitions and field schemas belong to a later implementation.

/// Valid identifiers start with a lowercase ASCII letter and contain only
/// lowercase letters, digits, or underscores.
pub fn valid_content_type_key(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
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
