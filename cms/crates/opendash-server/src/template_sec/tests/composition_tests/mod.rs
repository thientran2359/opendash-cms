use super::*;

fn parts(items: &[(&str, &str)]) -> BTreeMap<String, String> {
    items
        .iter()
        .map(|(k, v)| ((*k).into(), (*v).into()))
        .collect()
}

fn compile(
    source: &str,
    parts: &BTreeMap<String, String>,
    kind: CsecContentKind,
) -> Result<CompiledCsecComposition, &'static str> {
    validate_and_compile_csec_composition("root", source, parts, kind)
}

mod od_image_csec_tests;
mod suite_01;
mod suite_02;
mod suite_03;
