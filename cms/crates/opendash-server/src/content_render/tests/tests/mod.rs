use super::*;
use opendash_core::field_schema::FieldSpec;

fn assert_ordered_markup(html: &str, fragments: &[&str]) {
    let mut at = 0;
    for fragment in fragments {
        let pos = html[at..]
            .find(fragment)
            .unwrap_or_else(|| panic!("missing {fragment}: {html}"));
        at += pos + fragment.len();
    }
}

mod suite_01;
mod suite_02;
mod unified_field_view_regressions;
