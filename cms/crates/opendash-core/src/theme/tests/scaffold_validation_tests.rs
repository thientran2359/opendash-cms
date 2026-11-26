use super::valid_block_key;

#[test]
fn requires_valid_namespace_and_name() {
    assert!(valid_block_key("core/heading"));
    assert!(!valid_block_key("heading"));
    assert!(!valid_block_key("core/"));
    assert!(!valid_block_key("core/heading/extra"));
}
