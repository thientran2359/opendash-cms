use std::collections::BTreeMap;
use std::fs;

use opendash_core::l10n::compile_po_to_mo;

use super::registry::CatalogSource;
use super::*;
use crate::cache::SiteCacheEpoch;
use crate::paths::SitePaths;

#[test]
fn catalog_registry_precedence_parent_fallback_and_epoch_bump() {
    let root = std::env::temp_dir().join(format!("od_l10n_s01_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let paths = SitePaths::from_parts(root.clone(), root.join("database/opendash.sqlite"));

    let embedded_vi = compile_po_to_mo(
        r#"
msgid "Save"
msgstr "Lưu (embedded)"
"#,
    )
    .unwrap();
    let site_vi = compile_po_to_mo(
        r#"
msgid "Save"
msgstr "Lưu (site override)"
"#,
    )
    .unwrap();

    fs::create_dir_all(paths.core_languages_dir()).unwrap();
    fs::write(paths.core_languages_dir().join("opendash-vi.mo"), &site_vi).unwrap();
    fs::write(
        paths.core_languages_dir().join(".tmp-ignored.mo"),
        b"not-a-mo",
    )
    .unwrap();

    let mut embedded = BTreeMap::new();
    embedded.insert("vi".to_owned(), embedded_vi);

    let reg = CatalogRegistry::new_empty();
    let before = SiteCacheEpoch::global().current();
    reg.reload_from_site_paths(&paths, embedded, BTreeMap::new(), BTreeMap::new())
        .unwrap();
    let after = SiteCacheEpoch::global().current();
    assert!(after > before);

    let resolved = reg.resolve(L10nScope::Core, "opendash", "vi_VN").unwrap();
    assert_eq!(resolved.source, CatalogSource::SiteOverride);
    assert_eq!(resolved.catalog.get("Save"), "Lưu (site override)");

    let _ = fs::remove_dir_all(&root);
}
