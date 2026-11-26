use super::*;

/// Scans `<!-- od:... -->` comments leniently to extract referenced media and
/// pattern IDs even when sibling comments are malformed.
pub fn extract_block_refs_lenient(raw_body: &str) -> ExtractedBlockRefs {
    extract_block_refs_lenient_with_schemas(raw_body, &RegistryCatalog::new())
}
