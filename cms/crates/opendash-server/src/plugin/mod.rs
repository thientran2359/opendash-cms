pub mod egress;
pub mod host_ops;
pub mod js_runtime;
pub mod lifecycle_confirmation;
pub mod manager;
pub mod package;
pub mod runtime;
pub mod worker;

pub use opendash_core::plugin as manifest;

#[cfg(test)]
mod tests;

/// The test runner otherwise starts many real worker processes at once. These
/// integration tests exercise each invocation's real 5s limit (including
/// concurrency within a test), so isolate them from unrelated test workers.
#[cfg(test)]
pub(crate) static PLUGIN_TEST_SERIAL: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);

#[allow(unused_imports)]
pub use manager::{PluginManager, PluginSummary};
#[allow(unused_imports)]
pub use manifest::{PluginManifest, PluginPermissions};
#[allow(unused_imports)]
pub use package::{PluginCheckSummary, PluginPackage, check_plugin_path};
#[allow(unused_imports)]
pub use runtime::{PluginError, PluginRuntime};

#[cfg(test)]
mod review_regressions;
