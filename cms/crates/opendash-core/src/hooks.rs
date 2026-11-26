//! Core lifecycle hook and action contracts.
//!
//! Defines canonical event identifiers and payload envelopes dispatched by the
//! server when content changes, pages are saved, or admin actions run.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Extension package owner kind sharing the unified worker runtime and storage.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionOwnerKind {
    #[default]
    Plugin,
    Theme,
}

impl ExtensionOwnerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plugin => "plugin",
            Self::Theme => "theme",
        }
    }

    pub fn from_db(raw: &str) -> Option<Self> {
        match raw {
            "plugin" => Some(Self::Plugin),
            "theme" => Some(Self::Theme),
            _ => None,
        }
    }

    pub fn options_owner_kind(self) -> &'static str {
        match self {
            Self::Plugin => "plugin",
            Self::Theme => "theme_functions",
        }
    }

    pub fn secrets_subdir(self) -> &'static str {
        match self {
            Self::Plugin => "plugins",
            Self::Theme => "themes",
        }
    }
}

/// Execution engine selected inside `--plugin-worker`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionRuntimeKind {
    #[default]
    Wasm,
    Js,
}

impl ExtensionRuntimeKind {
    pub fn from_entrypoint(entrypoint: &str) -> Self {
        if entrypoint.ends_with(".js") {
            Self::Js
        } else {
            Self::Wasm
        }
    }
}

/// Canonical lifecycle events, filters, and actions emitted by OpenDash.
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
    // Spec 2 — Lifecycle post-commit events (async)
    #[serde(rename = "content/after_delete")]
    ContentAfterDelete,
    #[serde(rename = "content/after_publish")]
    ContentAfterPublish,
    #[serde(rename = "content/after_unpublish")]
    ContentAfterUnpublish,
    #[serde(rename = "contact/after_submit")]
    ContactAfterSubmit,
    #[serde(rename = "plugin/after_enable")]
    PluginAfterEnable,
    #[serde(rename = "plugin/after_disable")]
    PluginAfterDisable,
    #[serde(rename = "settings/after_save")]
    SettingsAfterSave,
    #[serde(rename = "theme/after_activate")]
    ThemeAfterActivate,
    // Spec 2 — Typed synchronous filters
    #[serde(rename = "filter:content/prepare_save")]
    FilterContentPrepareSave,
    #[serde(rename = "filter:contact/before_submit")]
    FilterContactBeforeSubmit,
    #[serde(rename = "filter:seo/head")]
    FilterSeoHead,
    #[serde(rename = "filter:entry/view")]
    FilterEntryView,
}

impl HookEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookEvent::PageAfterSave => "page/after_save",
            HookEvent::ContentBeforeSave => "content/before_save",
            HookEvent::ContentAfterSave => "content/after_save",
            HookEvent::PluginManualRun => "plugin/manual_run",
            HookEvent::ContentAfterDelete => "content/after_delete",
            HookEvent::ContentAfterPublish => "content/after_publish",
            HookEvent::ContentAfterUnpublish => "content/after_unpublish",
            HookEvent::ContactAfterSubmit => "contact/after_submit",
            HookEvent::PluginAfterEnable => "plugin/after_enable",
            HookEvent::PluginAfterDisable => "plugin/after_disable",
            HookEvent::SettingsAfterSave => "settings/after_save",
            HookEvent::ThemeAfterActivate => "theme/after_activate",
            HookEvent::FilterContentPrepareSave => "filter:content/prepare_save",
            HookEvent::FilterContactBeforeSubmit => "filter:contact/before_submit",
            HookEvent::FilterSeoHead => "filter:seo/head",
            HookEvent::FilterEntryView => "filter:entry/view",
        }
    }

    pub fn is_filter(self) -> bool {
        matches!(
            self,
            Self::FilterContentPrepareSave
                | Self::FilterContactBeforeSubmit
                | Self::FilterSeoHead
                | Self::FilterEntryView
        )
    }

    pub fn is_async_hook(self) -> bool {
        matches!(
            self,
            Self::ContentAfterDelete
                | Self::ContentAfterPublish
                | Self::ContentAfterUnpublish
                | Self::ContactAfterSubmit
                | Self::PluginAfterEnable
                | Self::PluginAfterDisable
                | Self::SettingsAfterSave
                | Self::ThemeAfterActivate
        )
    }
}

/// Structured envelope passed to hook listeners and plugin dispatchers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookEnvelope {
    pub event: HookEvent,
    pub occurred_at_ms: u64,
    pub correlation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translation_group_id: Option<String>,
    pub payload: Value,
}

impl HookEnvelope {
    pub fn new(
        event: HookEvent,
        occurred_at_ms: u64,
        correlation_id: impl Into<String>,
        payload: Value,
    ) -> Self {
        let locale = payload
            .get("locale")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let translation_group_id = payload
            .get("translation_group_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        Self {
            event,
            occurred_at_ms,
            correlation_id: correlation_id.into(),
            locale,
            translation_group_id,
            payload,
        }
    }
}

/// Execution mode for plugin hook invocations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HookExecutionMode {
    #[default]
    Standard,
    ContentBeforeSave,
    ContentAfterSave,
    AsyncPostCommit,
    FilterContentPrepareSave,
    FilterContactBeforeSubmit,
    FilterProjection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPrincipal {
    Admin,
    Editor,
    Public,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvelopeKind {
    Hook,
    Filter,
    Action,
    Manual,
    // Reserved Stage 5-6 seams (rejected in V1 dispatchers)
    Route,
    Cron,
}

pub const PLUGIN_ENVELOPE_V2_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginEnvelopeV2 {
    pub envelope_version: u32,
    pub kind: EnvelopeKind,
    pub event: String,
    pub occurred_at_ms: u64,
    pub correlation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    pub principal: ExecutionPrincipal,
    pub dispatch_depth: u8,
    pub payload: Value,
}

impl PluginEnvelopeV2 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kind: EnvelopeKind,
        event: impl Into<String>,
        occurred_at_ms: u64,
        correlation_id: impl Into<String>,
        locale: Option<String>,
        principal: ExecutionPrincipal,
        dispatch_depth: u8,
        payload: Value,
    ) -> Self {
        Self {
            envelope_version: PLUGIN_ENVELOPE_V2_VERSION,
            kind,
            event: event.into(),
            occurred_at_ms,
            correlation_id: correlation_id.into(),
            locale,
            principal,
            dispatch_depth,
            payload,
        }
    }
}

/// Tagged decision returned by `content/before_save` validators.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum ContentBeforeSaveDecision {
    Allow,
    Veto {
        code: String,
        #[serde(default)]
        message: String,
        #[serde(default)]
        field_errors: std::collections::BTreeMap<String, String>,
    },
}

/// Sanitizes diagnostic strings to length-bounded raw UTF-8 plain text with control chars stripped.
pub fn sanitize_plain_text(raw: &str, max_chars: usize) -> String {
    raw.chars()
        .filter(|ch| !ch.is_control() || *ch == ' ')
        .take(max_chars)
        .collect::<String>()
        .trim()
        .to_string()
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

    #[test]
    fn spec2_events_helpers_and_envelope_v2_roundtrip() {
        let cases = [
            (HookEvent::PageAfterSave, "page/after_save", false, false),
            (
                HookEvent::ContentBeforeSave,
                "content/before_save",
                false,
                false,
            ),
            (
                HookEvent::ContentAfterSave,
                "content/after_save",
                false,
                false,
            ),
            (
                HookEvent::PluginManualRun,
                "plugin/manual_run",
                false,
                false,
            ),
            (
                HookEvent::ContentAfterDelete,
                "content/after_delete",
                false,
                true,
            ),
            (
                HookEvent::ContentAfterPublish,
                "content/after_publish",
                false,
                true,
            ),
            (
                HookEvent::ContentAfterUnpublish,
                "content/after_unpublish",
                false,
                true,
            ),
            (
                HookEvent::ContactAfterSubmit,
                "contact/after_submit",
                false,
                true,
            ),
            (
                HookEvent::PluginAfterEnable,
                "plugin/after_enable",
                false,
                true,
            ),
            (
                HookEvent::PluginAfterDisable,
                "plugin/after_disable",
                false,
                true,
            ),
            (
                HookEvent::SettingsAfterSave,
                "settings/after_save",
                false,
                true,
            ),
            (
                HookEvent::ThemeAfterActivate,
                "theme/after_activate",
                false,
                true,
            ),
            (
                HookEvent::FilterContentPrepareSave,
                "filter:content/prepare_save",
                true,
                false,
            ),
            (
                HookEvent::FilterContactBeforeSubmit,
                "filter:contact/before_submit",
                true,
                false,
            ),
            (HookEvent::FilterSeoHead, "filter:seo/head", true, false),
            (HookEvent::FilterEntryView, "filter:entry/view", true, false),
        ];
        for (ev, s, is_filter, is_async) in cases {
            assert_eq!(ev.as_str(), s);
            assert_eq!(ev.is_filter(), is_filter);
            assert_eq!(ev.is_async_hook(), is_async);
            let serialized = serde_json::to_value(ev).unwrap();
            assert_eq!(serialized, Value::String(s.to_string()));
            assert_eq!(serde_json::from_value::<HookEvent>(serialized).unwrap(), ev);
        }

        let env2 = PluginEnvelopeV2::new(
            EnvelopeKind::Hook,
            "content/after_publish",
            999,
            "corr_123",
            Some("vi".into()),
            ExecutionPrincipal::Editor,
            1,
            json!({ "entry_id": 42 }),
        );
        assert_eq!(env2.envelope_version, 2);
        let raw = serde_json::to_value(&env2).unwrap();
        assert_eq!(raw["envelope_version"], 2);
        assert_eq!(raw["principal"], "editor");
        assert_eq!(raw["kind"], "hook");
        assert_eq!(
            serde_json::from_value::<PluginEnvelopeV2>(raw).unwrap(),
            env2
        );
    }
}
