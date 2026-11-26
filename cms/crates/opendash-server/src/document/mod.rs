mod head_and_seo;
mod normalize;
mod types;

pub use head_and_seo::*;
pub use normalize::*;
pub use types::*;

#[cfg(test)]
#[path = "tests/tests.rs"]
mod tests;
