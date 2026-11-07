//! Central CMS Hook & Action Dispatcher (`opendash-server::hooks`).
//!
//! Connects lifecycle events (`HookEvent`) emitted by server routes/use-cases
//! with core listeners and `PluginManager`.

use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use opendash_core::hooks::{HookEnvelope, HookEvent};
use serde_json::Value;

use crate::{
    auth::generate_random_token,
    plugin::{PluginManager, manager::PluginRunResponse},
};

pub struct HookDispatcher {
    plugin_manager: Arc<PluginManager>,
    recent_events: Mutex<Vec<HookEnvelope>>,
}

impl HookDispatcher {
    pub fn new(plugin_manager: Arc<PluginManager>) -> Self {
        Self {
            plugin_manager,
            recent_events: Mutex::new(Vec::new()),
        }
    }

    /// Emits a lifecycle event envelope (`page/after_save`, `content/after_save`, etc.)
    /// into the central dispatcher without blocking or failing the primary transaction.
    pub fn emit(&self, event: HookEvent, payload: Value) -> HookEnvelope {
        let token = generate_random_token();
        let correlation_id = format!("evt_{}", &token[..12.min(token.len())]);
        let envelope = HookEnvelope::new(event, correlation_id, payload);

        if let Ok(mut log) = self.recent_events.lock() {
            if log.len() >= 64 {
                log.remove(0);
            }
            log.push(envelope.clone());
        }

        envelope
    }

    /// Dispatches a manual plugin action run through the central hook bus.
    pub async fn dispatch_plugin_run(
        &self,
        plugin_id: &str,
        input: Value,
    ) -> Result<PluginRunResponse, (StatusCode, PluginRunResponse)> {
        self.emit(
            HookEvent::PluginManualRun,
            serde_json::json!({ "plugin_id": plugin_id }),
        );
        self.plugin_manager.run_plugin(plugin_id, input).await
    }

    #[cfg(test)]
    pub fn recent_event_count(&self) -> usize {
        self.recent_events.lock().map(|g| g.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn records_emitted_lifecycle_events() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let pm = Arc::new(PluginManager::new(pool, false));
        let dispatcher = HookDispatcher::new(pm);

        let env = dispatcher.emit(
            HookEvent::PageAfterSave,
            serde_json::json!({ "block_type": "sample-theme/hero-banner" }),
        );
        assert_eq!(env.event, HookEvent::PageAfterSave);
        assert!(env.correlation_id.starts_with("evt_"));
        assert_eq!(dispatcher.recent_event_count(), 1);
    }
}
