fn main() {
    // Build admin assets first: see scripts/build.ps1.
    println!("cargo:rerun-if-changed=../../admin/dist");
}
