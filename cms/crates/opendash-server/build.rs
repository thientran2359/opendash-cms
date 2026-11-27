use std::env;
use std::fs;
use std::path::PathBuf;

#[allow(dead_code, unused_imports)]
#[path = "."]
mod l10n {
    #[path = "../opendash-core/src/l10n/catalog.rs"]
    pub mod catalog;
    #[path = "../opendash-core/src/l10n/mo.rs"]
    pub mod mo;
    #[path = "../opendash-core/src/l10n/plural.rs"]
    pub mod plural;
    #[path = "../opendash-core/src/l10n/po.rs"]
    pub mod po;

    pub use catalog::{Catalog, CatalogEntry, CatalogLookup, PotDocument, PotEntry};
    pub use plural::PluralRule;
    pub use po::compile_po_to_mo;

    pub const MAX_MO_BYTES: usize = 4 * 1024 * 1024;
    pub const MAX_PO_BYTES: usize = 2 * 1024 * 1024;
    pub const MAX_CATALOG_ENTRIES: usize = 16_384;
    pub const MAX_ENTRY_BYTES: usize = 64 * 1024;
    pub const MAX_MATERIALIZED_BYTES: usize = 8 * 1024 * 1024;
    pub const MAX_PLURAL_FORMS: usize = 6;

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
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let po_path = manifest_dir.join("../../l10n/vi.po");
    println!("cargo:rerun-if-changed={}", po_path.display());
    for src in [
        "../opendash-core/src/l10n/catalog.rs",
        "../opendash-core/src/l10n/mo.rs",
        "../opendash-core/src/l10n/plural.rs",
        "../opendash-core/src/l10n/po.rs",
    ] {
        println!(
            "cargo:rerun-if-changed={}",
            manifest_dir.join(src).display()
        );
    }

    let po_text = fs::read_to_string(&po_path).expect("read cms/l10n/vi.po");
    let mo_bytes = l10n::compile_po_to_mo(&po_text).expect("compile cms/l10n/vi.po to .mo");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let out_mo = out_dir.join("vi.mo");
    if fs::read(&out_mo).ok().as_deref() != Some(mo_bytes.as_slice()) {
        fs::write(&out_mo, mo_bytes).expect("write OUT_DIR/vi.mo");
    }
}
