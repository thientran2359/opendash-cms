//! Read-only post-commit notifications with bounded, sequential execution.

use super::*;

pub(super) use crate::{
    auth::generate_random_token,
    plugin::{PluginManager, manager::PluginRunResponse},
};

pub(super) use axum::http::StatusCode;

pub(super) use opendash_core::hooks::{
    EnvelopeKind, ExecutionPrincipal, HookEnvelope, HookEvent, HookExecutionMode,
};

pub(super) use serde_json::Value;

pub(super) use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

// Catch both callback construction and asynchronous polling panics without spawning
// detached tasks. Rust callbacks are trusted/cooperative; Wasm stays in its worker.

mod definitions;
pub use definitions::*;
mod dispatch_core;
pub use dispatch_core::*;
mod dispatch_content;
pub use dispatch_content::*;
mod dispatch_async;
pub use dispatch_async::*;
mod render_hooks;
pub use render_hooks::*;
