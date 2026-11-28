use super::*;

/// Scans `<!-- od:... -->` comments leniently to extract referenced core media and pattern IDs.
pub fn extract_block_refs_lenient(raw_body: &str) -> ExtractedBlockRefs {
    extract_block_refs_impl(raw_body, None)
}
