use super::*;

pub fn global_site_option_defaults() -> BTreeMap<String, Value> {
    let fields = global_site_option_fields();
    let mut out = BTreeMap::new();
    for spec in &fields {
        if !spec.is_ui_only() {
            out.insert(spec.key.clone(), default_value_for_field_spec(spec));
        }
    }
    out
}
