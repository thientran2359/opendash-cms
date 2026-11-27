//! Pure-Rust full-text search normalization, query sanitization, document
//! extraction, and snippet highlighting primitives (`0` I/O, `0` SQL).

mod extract;
mod normalize;
mod public_url;
mod query;
mod snippet;

pub use extract::*;
pub use normalize::*;
pub use public_url::*;
pub use query::*;
pub use snippet::*;

#[cfg(test)]
mod tests;
