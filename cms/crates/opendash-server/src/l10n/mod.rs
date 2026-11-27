pub mod error_catalog;
pub mod extract;
pub mod registry;

pub use registry::{CatalogRegistry, CatalogSnapshot, L10nScope, PreSerializedCatalog};

pub const EMBEDDED_VI_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/vi.mo"));

#[cfg(test)]
mod tests;
