//! Example block identifier validation. Theme ZIP parsing and MiniJinja
//! rendering belong to opendash-server, not this crate.

use crate::content::valid_content_type_key;

/// A block key is namespaced, e.g. `core/heading`.
pub fn valid_block_key(value: &str) -> bool {
    let Some((namespace, name)) = value.split_once('/') else {
        return false;
    };
    valid_content_type_key(namespace) && valid_content_type_key(name)
}

#[cfg(test)]
mod tests {
    use super::valid_block_key;

    #[test]
    fn requires_valid_namespace_and_name() {
        assert!(valid_block_key("core/heading"));
        assert!(!valid_block_key("heading"));
        assert!(!valid_block_key("core/"));
        assert!(!valid_block_key("core/heading/extra"));
    }
}
