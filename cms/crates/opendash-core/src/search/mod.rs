//! Pure-Rust full-text search normalization, query sanitization, document
//! extraction, and snippet highlighting primitives (`0` I/O, `0` SQL).

mod extract;
mod normalize;
mod query;
mod snippet;

pub use extract::*;
pub use normalize::*;
pub use query::*;
pub use snippet::*;

#[cfg(test)]
mod tests;
