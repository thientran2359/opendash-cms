//! Unified JavaScript (ES2022+) execution engine powered by `boa_engine = "=0.22.0"`.
//!
//! Runs strictly inside the isolated `--plugin-worker` process with OS memory cap,
//! Boa `RuntimeLimits`, and the frozen `globalThis.od` host SDK bridging directly
//! into `host_ops::handle_host_call`.

use std::cell::RefCell;

use boa_engine::{
    Context, JsArgs, JsError, JsNativeError, JsResult, JsValue, NativeFunction, Script, Source,
    js_string, property::Attribute,
};

use serde_json::Value;

use super::host_ops::{self, HostContext, WorkerExecutionResult};

mod bootstrap_js;
pub use bootstrap_js::*;
mod worker_exec;
pub use worker_exec::*;

#[cfg(test)]
#[path = "../js_runtime_tests.rs"]
mod tests;
