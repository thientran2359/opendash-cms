//! Core lifecycle hook and action contracts.
//!
//! Defines canonical event identifiers and payload envelopes dispatched by the
//! server when content changes, pages are saved, or admin actions run.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Canonical lifecycle events and actions emitted by OpenDash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum HookEvent {
    #[serde(rename = "page/after_save")]
    PageAfterSave,
    #[serde(rename = "content/before_save")]
    ContentBeforeSave,
    #[serde(rename = "content/after_save")]
    ContentAfterSave,
    #[serde(rename = "plugin/manual_run")]
    PluginManualRun,
}

impl HookEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookEvent::PageAfterSave => "page/after_save",
            HookEvent::ContentBeforeSave => "content/before_save",
            HookEvent::ContentAfterSave => "content/after_save",
            HookEvent::PluginManualRun => "plugin/manual_run",
        }
    }
}

/// Structured envelope passed to hook listeners and plugin dispatchers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookEnvelope {
    pub event: HookEvent,
    pub occurred_at_ms: u64,
    pub correlation_id: String,
    pub payload: Value,
}

impl HookEnvelope {
    pub fn new(
        event: HookEvent,
        occurred_at_ms: u64,
        correlation_id: impl Into<String>,
        payload: Value,
    ) -> Self {
        Self {
            event,
            occurred_at_ms,
            correlation_id: correlation_id.into(),
            payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn creates_canonical_hook_envelope() {
        let env = HookEnvelope::new(
            HookEvent::PageAfterSave,
            123,
            "evt_001",
            json!({ "block_type": "sample-theme/hero-banner" }),
        );
        assert_eq!(env.event.as_str(), "page/after_save");
        assert_eq!(env.correlation_id, "evt_001");
        let json = serde_json::to_value(&env).unwrap();
        assert_eq!(json["event"], "page/after_save");
        assert_eq!(json["occurred_at_ms"], 123);
        assert_eq!(serde_json::from_value::<HookEnvelope>(json).unwrap(), env);
    }
}
