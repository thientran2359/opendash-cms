use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let po_path = manifest_dir.join("../../l10n/vi.po");
    println!("cargo:rerun-if-changed={}", po_path.display());

    let po_text = fs::read_to_string(&po_path).expect("read cms/l10n/vi.po");
    let mo_bytes =
        opendash_core::l10n::compile_po_to_mo(&po_text).expect("compile cms/l10n/vi.po to .mo");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    fs::write(out_dir.join("vi.mo"), mo_bytes).expect("write OUT_DIR/vi.mo");
}
