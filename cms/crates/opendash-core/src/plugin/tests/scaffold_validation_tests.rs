use super::{PLUGIN_ABI_VERSION, valid_plugin_id};

#[test]
fn ids_follow_abi_rules() {
    assert_eq!(PLUGIN_ABI_VERSION, 1);
    assert!(valid_plugin_id("sample-plugin"));
    assert!(!valid_plugin_id("a"));
    assert!(!valid_plugin_id("-bad"));
    assert!(!valid_plugin_id("bad-"));
    assert!(!valid_plugin_id("Invalid_Plugin"));
}
