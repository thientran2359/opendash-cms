pub mod host_ops;
pub mod manager;
pub mod package;
pub mod runtime;
pub mod worker;

pub use opendash_core::plugin as manifest;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub use manager::{PluginManager, PluginSummary};
#[allow(unused_imports)]
pub use manifest::{PluginManifest, PluginPermissions};
#[allow(unused_imports)]
pub use package::PluginPackage;
#[allow(unused_imports)]
pub use runtime::{PluginError, PluginRuntime};

#[cfg(test)]
mod review_regressions;
