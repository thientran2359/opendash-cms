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
pub(super) async fn invoke_callback(
    callback: Callback,
    envelope: HookEnvelope,
) -> Result<(), String> {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::task::Poll;
    let mut future = catch_unwind(AssertUnwindSafe(|| callback(envelope)))
        .map_err(|_| "Core hook panicked".to_string())?;
    std::future::poll_fn(move |cx| {
        match catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(cx))) {
            Ok(poll) => poll,
            Err(_) => Poll::Ready(Err("Core hook panicked".into())),
        }
    })
    .await
}

pub const CORE_HANDLER_TIMEOUT: Duration = Duration::from_millis(250);

pub const DISPATCH_ADMISSION_BUDGET: Duration = Duration::from_secs(5);

pub const MAX_HANDLERS: usize = 32;

pub const ASYNC_HOOK_WORKERS: usize = 2;

pub const ASYNC_HOOK_QUEUE_CAPACITY: usize = 64;

pub const MAX_PREPARE_SAVE_PLUGINS: usize = 8;

pub const PREPARE_SAVE_PHASE_BUDGET: Duration = Duration::from_secs(2);

pub const MAX_CONTACT_FILTER_PLUGINS: usize = 4;

pub const CONTACT_FILTER_PHASE_BUDGET: Duration = Duration::from_secs(2);

pub const MAX_PROJECTION_FILTER_PLUGINS: usize = 8;

pub const PROJECTION_FILTER_PHASE_BUDGET: Duration = Duration::from_secs(5);

tokio::task_local! { pub(super) static IN_DISPATCH: bool; }

pub struct AsyncHookJob {
    pub event: HookEvent,
    pub principal: ExecutionPrincipal,
    pub locale: Option<String>,
    pub type_key: Option<String>,
    pub payload: Value,
    pub dispatch_depth: u8,
    pub lease: crate::operations::MutationLease,
}

#[derive(Clone)]
pub struct AsyncHookQueue {
    pub(super) tx: tokio::sync::mpsc::Sender<AsyncHookJob>,
    pub(super) shared_rx: Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<AsyncHookJob>>>,
    pub(super) maint_rx: tokio::sync::watch::Receiver<bool>,
    pub(super) shut_rx: tokio::sync::watch::Receiver<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContactBeforeSubmitDecision {
    Allow,
    Reject { code: String, message: String },
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentPrepareSavePatch {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub values_patch: std::collections::BTreeMap<String, Value>,
    #[serde(default)]
    pub term_ids: Option<Vec<i64>>,
}

/// Server-owned mutation scope for `filter:content/prepare_save`.
/// Deliberately does NOT implement `Deserialize`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrepareSaveMutationScope {
    Full,
    Metadata {
        requested_title: bool,
        requested_slug: bool,
        taxonomy_key: Option<String>,
        entry_locale: String,
    },
}

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum ContactBeforeSubmitRaw {
    Allow,
    Reject {
        #[serde(default)]
        code: String,
        #[serde(default)]
        message: String,
    },
}

pub(super) struct Handler {
    pub(super) id: String,
    pub(super) event: HookEvent,
    pub(super) priority: i32,
    pub(super) callback: Callback,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ContentBeforeSaveVeto {
    pub code: String,
    pub message: String,
    pub field_errors: std::collections::BTreeMap<String, String>,
}

pub(super) const CONTENT_HOOK_PHASE_BUDGET: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub struct HookOutcome {
    pub handler_id: String,
    pub error: Option<String>,
}

#[derive(Debug)]
pub struct DispatchReport {
    pub envelope: HookEnvelope,
    pub outcomes: Vec<HookOutcome>,
}

pub struct HookDispatcher {
    pub(super) plugin_manager: Arc<PluginManager>,
    pub(super) handlers: Vec<Handler>,
    pub(super) recent_events: Mutex<Vec<HookEnvelope>>,
    pub(super) async_queue: Option<AsyncHookQueue>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HookEnqueueOutcome {
    pub enqueued: u32,
    pub dropped: u32,
}

impl HookDispatcher {
    /// Dispatches post-commit async hooks strictly from a trusted server-generated
    /// `EntryCommitReceipt`. Never inspects untrusted guest JSON output.
    pub fn enqueue_plugin_action_hooks(
        &self,
        principal: ExecutionPrincipal,
        receipt: Option<&crate::storage::entry_lifecycle::EntryCommitReceipt>,
        lease: crate::operations::MutationLease,
    ) -> HookEnqueueOutcome {
        use crate::storage::entry_lifecycle::EntryMutationKind;

        let mut outcome = HookEnqueueOutcome::default();
        let Some(receipt) = receipt else {
            return outcome;
        };
        let type_key = Some(receipt.type_key.clone());
        let locale = Some(receipt.locale.clone());
        match receipt.operation {
            EntryMutationKind::Save => {
                if receipt.publication_after.as_deref() == Some("published") {
                    let accepted = self.enqueue_async(AsyncHookJob {
                        event: HookEvent::ContentAfterPublish,
                        principal,
                        locale: locale.clone(),
                        type_key: type_key.clone(),
                        payload: serde_json::json!({
                            "type_key": &receipt.type_key,
                            "id": receipt.id,
                            "entry_id": receipt.id,
                            "version": receipt.version_after,
                            "locale": &receipt.locale,
                            "publication_status": receipt.publication_after.as_deref(),
                            "translation_group_id": &receipt.translation_group_id,
                        }),
                        dispatch_depth: 0,
                        lease: lease.clone(),
                    });
                    outcome.record(accepted);
                }
                let accepted = self.enqueue_async(AsyncHookJob {
                    event: HookEvent::ContentAfterSave,
                    principal,
                    locale,
                    type_key,
                    payload: serde_json::json!({
                        "type_key": &receipt.type_key,
                        "id": receipt.id,
                        "entry_id": receipt.id,
                        "operation": "plugin_action",
                        "version": receipt.version_after,
                        "locale": &receipt.locale,
                        "translation_group_id": &receipt.translation_group_id,
                    }),
                    dispatch_depth: 0,
                    lease,
                });
                outcome.record(accepted);
            }
            EntryMutationKind::Metadata => {
                if receipt.publication_before != receipt.publication_after {
                    let event = if receipt.publication_after.as_deref() == Some("published") {
                        HookEvent::ContentAfterPublish
                    } else {
                        HookEvent::ContentAfterUnpublish
                    };
                    let accepted = self.enqueue_async(AsyncHookJob {
                        event,
                        principal,
                        locale: locale.clone(),
                        type_key: type_key.clone(),
                        payload: serde_json::json!({
                            "type_key": &receipt.type_key,
                            "id": receipt.id,
                            "entry_id": receipt.id,
                            "version": receipt.version_after,
                            "locale": &receipt.locale,
                            "publication_status": receipt.publication_after.as_deref(),
                            "translation_group_id": &receipt.translation_group_id,
                        }),
                        dispatch_depth: 0,
                        lease: lease.clone(),
                    });
                    outcome.record(accepted);
                }
                let accepted = self.enqueue_async(AsyncHookJob {
                    event: HookEvent::ContentAfterSave,
                    principal,
                    locale,
                    type_key,
                    payload: serde_json::json!({
                        "type_key": &receipt.type_key,
                        "id": receipt.id,
                        "entry_id": receipt.id,
                        "operation": "metadata",
                        "version": receipt.version_after,
                        "locale": &receipt.locale,
                        "translation_group_id": &receipt.translation_group_id,
                    }),
                    dispatch_depth: 0,
                    lease,
                });
                outcome.record(accepted);
            }
            EntryMutationKind::Publish => {
                let accepted = self.enqueue_async(AsyncHookJob {
                    event: HookEvent::ContentAfterPublish,
                    principal,
                    locale,
                    type_key,
                    payload: serde_json::json!({
                        "type_key": &receipt.type_key,
                        "id": receipt.id,
                        "entry_id": receipt.id,
                        "version": receipt.version_after,
                        "locale": &receipt.locale,
                        "publication_status": "published",
                        "translation_group_id": &receipt.translation_group_id,
                    }),
                    dispatch_depth: 0,
                    lease,
                });
                outcome.record(accepted);
            }
            EntryMutationKind::Unpublish => {
                let accepted = self.enqueue_async(AsyncHookJob {
                    event: HookEvent::ContentAfterUnpublish,
                    principal,
                    locale,
                    type_key,
                    payload: serde_json::json!({
                        "type_key": &receipt.type_key,
                        "id": receipt.id,
                        "entry_id": receipt.id,
                        "version": receipt.version_after,
                        "locale": &receipt.locale,
                        "publication_status": "draft",
                        "translation_group_id": &receipt.translation_group_id,
                    }),
                    dispatch_depth: 0,
                    lease,
                });
                outcome.record(accepted);
            }
            EntryMutationKind::Trash => {
                // Trash of a published entry emits ContentAfterUnpublish with lifecycle context;
                // draft Trash emits no publication event. Trash NEVER emits ContentAfterDelete.
                if receipt.publication_before.as_deref() == Some("published") {
                    let accepted = self.enqueue_async(AsyncHookJob {
                        event: HookEvent::ContentAfterUnpublish,
                        principal,
                        locale,
                        type_key,
                        payload: serde_json::json!({
                            "type_key": &receipt.type_key,
                            "id": receipt.id,
                            "entry_id": receipt.id,
                            "version": receipt.version_after,
                            "locale": &receipt.locale,
                            "publication_status": receipt.publication_after.as_deref(),
                            "translation_group_id": &receipt.translation_group_id,
                            "lifecycle_transition": "trash",
                            "trashed_at": receipt.trashed_at.as_deref(),
                        }),
                        dispatch_depth: 0,
                        lease,
                    });
                    outcome.record(accepted);
                }
            }
            EntryMutationKind::Restore => {
                // Only immediate-mode Restore transitioning to published emits ContentAfterPublish;
                // draft Restore emits no publication event.
                if receipt.publication_after.as_deref() == Some("published") {
                    let accepted = self.enqueue_async(AsyncHookJob {
                        event: HookEvent::ContentAfterPublish,
                        principal,
                        locale,
                        type_key,
                        payload: serde_json::json!({
                            "type_key": &receipt.type_key,
                            "id": receipt.id,
                            "entry_id": receipt.id,
                            "version": receipt.version_after,
                            "locale": &receipt.locale,
                            "publication_status": "published",
                            "translation_group_id": &receipt.translation_group_id,
                            "lifecycle_transition": "restore",
                            "published_at": receipt.published_at.as_deref(),
                        }),
                        dispatch_depth: 0,
                        lease,
                    });
                    outcome.record(accepted);
                }
            }
            EntryMutationKind::Purge => {
                // Actual Purge emits ContentAfterDelete exactly once with retained pre-delete metadata.
                let accepted = self.enqueue_async(AsyncHookJob {
                    event: HookEvent::ContentAfterDelete,
                    principal,
                    locale,
                    type_key,
                    payload: serde_json::json!({
                        "type_key": &receipt.type_key,
                        "id": receipt.id,
                        "entry_id": receipt.id,
                        "version": receipt.version_before,
                        "locale": &receipt.locale,
                        "translation_group_id": &receipt.translation_group_id,
                        "lifecycle_transition": "purge",
                        "trashed_at": receipt.trashed_at.as_deref(),
                        "published_at": receipt.published_at.as_deref(),
                    }),
                    dispatch_depth: 0,
                    lease,
                });
                outcome.record(accepted);
            }
        }
        outcome
    }

    pub fn enqueue_entry_lifecycle_hooks(
        &self,
        principal: ExecutionPrincipal,
        receipt: &crate::storage::entry_lifecycle::EntryCommitReceipt,
        lease: crate::operations::MutationLease,
    ) -> HookEnqueueOutcome {
        self.enqueue_plugin_action_hooks(principal, Some(receipt), lease)
    }
}

impl HookDispatcher {
    pub fn new(plugin_manager: Arc<PluginManager>) -> Self {
        let mut dispatcher = Self {
            plugin_manager,
            handlers: Vec::new(),
            recent_events: Mutex::new(Vec::new()),
            async_queue: None,
        };
        dispatcher
            .register_handler("core/page_saved", HookEvent::PageAfterSave, 0, |event| {
                Box::pin(async move {
                    eprintln!("{} {}", event.event.as_str(), event.correlation_id);
                    Ok(())
                })
            })
            .expect("unique built-in hook");
        dispatcher
    }

    pub fn with_async_queue(
        plugin_manager: Arc<PluginManager>,
        maintenance_cancel_rx: tokio::sync::watch::Receiver<bool>,
        shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> Self {
        let mut dispatcher = Self::new(Arc::clone(&plugin_manager));
        dispatcher.async_queue = Some(AsyncHookQueue::spawn(
            plugin_manager,
            maintenance_cancel_rx,
            shutdown_rx,
        ));
        dispatcher
    }

    pub fn enqueue_async(&self, job: AsyncHookJob) -> bool {
        if let Some(ref queue) = self.async_queue {
            queue.enqueue(job)
        } else if job.dispatch_depth < 2 && tokio::runtime::Handle::try_current().is_ok() {
            let pm = Arc::clone(&self.plugin_manager);
            tokio::spawn(async move {
                run_async_hook_job(&pm, job).await;
            });
            true
        } else {
            false
        }
    }

    /// Trusted cooperative callbacks registered before AppState is shared.
    pub fn register_handler<F>(
        &mut self,
        id: &str,
        event: HookEvent,
        priority: i32,
        callback: F,
    ) -> Result<(), &'static str>
    where
        F: Fn(HookEnvelope) -> HookFuture + Send + Sync + 'static,
    {
        if event != HookEvent::PageAfterSave {
            return Err("Only page/after_save is implemented");
        }
        if !opendash_core::id::valid_namespaced_snake_key(id) || !id.starts_with("core/") {
            return Err("Core handler ID must be core/snake_key");
        }
        if self.handlers.iter().any(|h| h.id == id) {
            return Err("Duplicate handler ID");
        }
        if self.handlers.len() >= MAX_HANDLERS {
            return Err("Too many handlers");
        }
        self.handlers.push(Handler {
            id: id.into(),
            event,
            priority,
            callback: Arc::new(callback),
        });
        Ok(())
    }

    pub(super) fn envelope(event: HookEvent, payload: Value) -> HookEnvelope {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        HookEnvelope::new(
            event,
            millis,
            format!("evt_{}", generate_random_token()),
            payload,
        )
    }

    /// Records content lifecycle events without invoking plugin handlers.
    pub fn emit(&self, event: HookEvent, payload: Value) {
        let envelope = Self::envelope(event, payload);
        if let Ok(mut log) = self.recent_events.lock() {
            if log.len() >= 64 {
                log.remove(0);
            }
            log.push(envelope);
        }
    }

    pub async fn dispatch(&self, event: HookEvent, payload: Value) -> DispatchReport {
        let envelope = Self::envelope(event, payload);
        if event != HookEvent::PageAfterSave || IN_DISPATCH.try_with(|v| *v).unwrap_or(false) {
            return DispatchReport {
                envelope,
                outcomes: vec![HookOutcome {
                    handler_id: "dispatcher".into(),
                    error: Some("Unsupported or nested lifecycle dispatch".into()),
                }],
            };
        }
        IN_DISPATCH
            .scope(true, self.dispatch_envelope(envelope))
            .await
    }

    pub(super) async fn dispatch_envelope(&self, envelope: HookEnvelope) -> DispatchReport {
        let started = Instant::now();
        let mut outcomes = Vec::new();
        let mut ordered: Vec<(
            i32,
            String,
            Option<Callback>,
            Option<crate::plugin::host_ops::RuntimeOwner>,
        )> = self
            .handlers
            .iter()
            .filter(|h| h.event == envelope.event)
            .map(|h| (h.priority, h.id.clone(), Some(h.callback.clone()), None))
            .collect();
        match tokio::time::timeout(
            Duration::from_secs(1),
            self.plugin_manager.owner_hook_subscriptions(envelope.event),
        )
        .await
        {
            Ok(Ok(subs)) => ordered.extend(subs.into_iter().map(|sub| {
                let prefix = sub.owner.kind.as_str();
                (
                    100.max(sub.priority),
                    format!("{prefix}/{}", sub.owner.id),
                    None,
                    Some(sub.owner),
                )
            })),
            failure => {
                let error = format!("Subscription discovery failed: {failure:?}");
                eprintln!("{} {error}", envelope.correlation_id);
                outcomes.push(HookOutcome {
                    handler_id: "dispatcher".into(),
                    error: Some(error),
                });
            }
        }
        ordered.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        for (index, (_, id, callback, owner_opt)) in ordered.into_iter().enumerate() {
            let result = if index >= MAX_HANDLERS || started.elapsed() >= DISPATCH_ADMISSION_BUDGET
            {
                Err("Hook dispatch admission budget exceeded".to_string())
            } else if let Some(callback) = callback {
                match tokio::time::timeout(
                    CORE_HANDLER_TIMEOUT,
                    invoke_callback(callback, envelope.clone()),
                )
                .await
                {
                    Ok(result) => result,
                    Err(_) => Err("Core hook timed out".into()),
                }
            } else if let Some(ref owner) = owner_opt {
                // No outer timeout around commit: retain M2 commit handover semantics.
                let legacy_env = serde_json::to_value(&envelope).expect("JSON envelope");
                self.plugin_manager
                    .run_owner_dispatch(
                        owner,
                        EnvelopeKind::Hook,
                        legacy_env,
                        envelope.event.as_str(),
                        envelope.occurred_at_ms,
                        envelope.correlation_id.clone(),
                        envelope.locale.clone(),
                        ExecutionPrincipal::System,
                        0,
                        envelope.payload.clone(),
                        HookExecutionMode::Standard,
                        None,
                    )
                    .await
                    .map(|_| ())
                    .map_err(|(_, response)| {
                        response
                            .error
                            .unwrap_or_else(|| "Extension hook failed".into())
                    })
            } else {
                Ok(())
            };
            if let Err(error) = &result {
                eprintln!("{} {id}: {error}", envelope.correlation_id);
                if let Some(ref owner) = owner_opt {
                    self.plugin_manager
                        .record_owner_hook_failure(owner, &envelope.correlation_id, error)
                        .await;
                }
            }
            outcomes.push(HookOutcome {
                handler_id: id,
                error: result.err(),
            });
        }
        DispatchReport { envelope, outcomes }
    }

    #[allow(dead_code)]
    pub async fn filter_subscriptions(
        &self,
        event: HookEvent,
        type_key: Option<&str>,
    ) -> Result<Vec<(i32, String)>, String> {
        self.plugin_manager
            .filter_subscriptions(event, type_key)
            .await
    }

    #[allow(dead_code)]
    pub async fn dispatch_content_before_save(
        &self,
        type_key: &str,
        actor_role: opendash_core::content::FieldRole,
        payload: Value,
    ) -> Result<(), ContentBeforeSaveVeto> {
        self.dispatch_content_before_save_for_actor(type_key, actor_role, payload)
            .await
    }

    pub async fn dispatch_content_before_save_for_actor(
        &self,
        type_key: &str,
        actor_role: opendash_core::content::FieldRole,
        payload: Value,
    ) -> Result<(), ContentBeforeSaveVeto> {
        if IN_DISPATCH.try_with(|v| *v).unwrap_or(false) {
            return Err(ContentBeforeSaveVeto {
                code: "nested_dispatch".into(),
                message: "Không cho phép gọi lồng nhau trong content/before_save.".into(),
                field_errors: Default::default(),
            });
        }
        let envelope = Self::envelope(HookEvent::ContentBeforeSave, payload);
        IN_DISPATCH
            .scope(true, async {
                let started = Instant::now();
                let subs = match tokio::time::timeout(
                    Duration::from_secs(1),
                    self.plugin_manager
                        .owner_content_hook_subscriptions(HookEvent::ContentBeforeSave, type_key),
                )
                .await
                {
                    Ok(Ok(s)) => s,
                    _ => {
                        return Err(ContentBeforeSaveVeto {
                            code: "hook_discovery_failed".into(),
                            message: "Không thể tra cứu gói mở rộng content/before_save.".into(),
                            field_errors: Default::default(),
                        });
                    }
                };

                for (index, sub) in subs.into_iter().enumerate() {
                    if index >= MAX_HANDLERS || started.elapsed() >= CONTENT_HOOK_PHASE_BUDGET {
                        return Err(ContentBeforeSaveVeto {
                            code: "hook_budget_exceeded".into(),
                            message: "Vượt quá giới hạn xử lý content/before_save.".into(),
                            field_errors: Default::default(),
                        });
                    }
                    let remaining = CONTENT_HOOK_PHASE_BUDGET.saturating_sub(started.elapsed());
                    let mut plugin_env = envelope.clone();
                    self.plugin_manager
                        .filter_content_payload_for_owner(
                            &sub.owner,
                            type_key,
                            actor_role,
                            &mut plugin_env.payload,
                        )
                        .await;
                    let env_val = serde_json::to_value(&plugin_env).expect("JSON envelope");
                    let out = match self
                        .plugin_manager
                        .run_owner_dispatch(
                            &sub.owner,
                            EnvelopeKind::Hook,
                            env_val,
                            plugin_env.event.as_str(),
                            plugin_env.occurred_at_ms,
                            envelope.correlation_id.clone(),
                            plugin_env.locale.clone(),
                            principal_for_field_role(actor_role),
                            0,
                            plugin_env.payload,
                            HookExecutionMode::ContentBeforeSave,
                            Some(remaining),
                        )
                        .await
                    {
                        Ok(run_resp) => run_resp.data.unwrap_or(Value::Null),
                        Err((_status, resp)) => {
                            let err_msg = opendash_core::hooks::sanitize_plain_text(
                                resp.error.as_deref().unwrap_or("Extension hook failed"),
                                256,
                            );
                            self.plugin_manager
                                .record_owner_hook_failure(
                                    &sub.owner,
                                    &envelope.correlation_id,
                                    &err_msg,
                                )
                                .await;
                            return Err(ContentBeforeSaveVeto {
                                code: resp.code.unwrap_or_else(|| "hook_failed".into()),
                                message: err_msg,
                                field_errors: Default::default(),
                            });
                        }
                    };

                    let decision: opendash_core::hooks::ContentBeforeSaveDecision =
                        match serde_json::from_value(out) {
                            Ok(d) => d,
                            Err(_) => {
                                let msg = "Đầu ra content/before_save không hợp lệ.";
                                self.plugin_manager
                                    .record_owner_hook_failure(
                                        &sub.owner,
                                        &envelope.correlation_id,
                                        msg,
                                    )
                                    .await;
                                return Err(ContentBeforeSaveVeto {
                                    code: "invalid_hook_output".into(),
                                    message: msg.into(),
                                    field_errors: Default::default(),
                                });
                            }
                        };

                    match decision {
                        opendash_core::hooks::ContentBeforeSaveDecision::Allow => {}
                        opendash_core::hooks::ContentBeforeSaveDecision::Veto {
                            code,
                            message,
                            field_errors,
                        } => {
                            let clean_code = opendash_core::hooks::sanitize_plain_text(&code, 64);
                            let clean_msg =
                                opendash_core::hooks::sanitize_plain_text(&message, 256);
                            let clean_fields = field_errors
                                .into_iter()
                                .take(32)
                                .map(|(k, v)| {
                                    (
                                        opendash_core::hooks::sanitize_plain_text(&k, 64),
                                        opendash_core::hooks::sanitize_plain_text(&v, 256),
                                    )
                                })
                                .collect();
                            return Err(ContentBeforeSaveVeto {
                                code: if clean_code.is_empty() {
                                    "vetoed".into()
                                } else {
                                    clean_code
                                },
                                message: clean_msg,
                                field_errors: clean_fields,
                            });
                        }
                    }
                }
                Ok(())
            })
            .await
    }

    #[allow(dead_code)]
    pub async fn dispatch_content_after_save(
        &self,
        type_key: &str,
        actor_role: opendash_core::content::FieldRole,
        payload: Value,
    ) -> DispatchReport {
        self.dispatch_content_after_save_for_actor(type_key, actor_role, payload)
            .await
    }

    pub async fn dispatch_content_after_save_for_actor(
        &self,
        type_key: &str,
        actor_role: opendash_core::content::FieldRole,
        payload: Value,
    ) -> DispatchReport {
        let envelope = Self::envelope(HookEvent::ContentAfterSave, payload);
        if IN_DISPATCH.try_with(|v| *v).unwrap_or(false) {
            return DispatchReport {
                envelope,
                outcomes: vec![HookOutcome {
                    handler_id: "dispatcher".into(),
                    error: Some("Nested lifecycle dispatch".into()),
                }],
            };
        }
        IN_DISPATCH
            .scope(true, async {
                let started = Instant::now();
                let mut outcomes = Vec::new();
                let subs = match tokio::time::timeout(
                    Duration::from_secs(1),
                    self.plugin_manager
                        .owner_content_hook_subscriptions(HookEvent::ContentAfterSave, type_key),
                )
                .await
                {
                    Ok(Ok(s)) => s,
                    failure => {
                        let error = format!("Subscription discovery failed: {failure:?}");
                        outcomes.push(HookOutcome {
                            handler_id: "dispatcher".into(),
                            error: Some(error),
                        });
                        return DispatchReport { envelope, outcomes };
                    }
                };
                for (index, sub) in subs.into_iter().enumerate() {
                    let result = if index >= MAX_HANDLERS
                        || started.elapsed() >= CONTENT_HOOK_PHASE_BUDGET
                    {
                        Err("ContentAfterSave phase budget exceeded".to_string())
                    } else {
                        let remaining = CONTENT_HOOK_PHASE_BUDGET.saturating_sub(started.elapsed());
                        let mut plugin_env = envelope.clone();
                        self.plugin_manager
                            .filter_content_payload_for_owner(
                                &sub.owner,
                                type_key,
                                actor_role,
                                &mut plugin_env.payload,
                            )
                            .await;
                        let env_val = serde_json::to_value(&plugin_env).expect("JSON envelope");
                        self.plugin_manager
                            .run_owner_dispatch(
                                &sub.owner,
                                EnvelopeKind::Hook,
                                env_val,
                                plugin_env.event.as_str(),
                                plugin_env.occurred_at_ms,
                                envelope.correlation_id.clone(),
                                plugin_env.locale.clone(),
                                principal_for_field_role(actor_role),
                                0,
                                plugin_env.payload,
                                HookExecutionMode::ContentAfterSave,
                                Some(remaining),
                            )
                            .await
                            .map(|_| ())
                            .map_err(|(_, resp)| {
                                resp.error
                                    .unwrap_or_else(|| "ContentAfterSave hook failed".into())
                            })
                    };
                    if let Err(ref err) = result {
                        eprintln!(
                            "{} {}/{}: {err}",
                            envelope.correlation_id,
                            sub.owner.kind.as_str(),
                            sub.owner.id
                        );
                        self.plugin_manager
                            .record_owner_hook_failure(&sub.owner, &envelope.correlation_id, err)
                            .await;
                    }
                    outcomes.push(HookOutcome {
                        handler_id: format!("{}/{}", sub.owner.kind.as_str(), sub.owner.id),
                        error: result.err(),
                    });
                }
                DispatchReport { envelope, outcomes }
            })
            .await
    }

    pub async fn dispatch_plugin_run(
        &self,
        plugin_id: &str,
        input: Value,
    ) -> Result<PluginRunResponse, (StatusCode, PluginRunResponse)> {
        let envelope = Self::envelope(HookEvent::PluginManualRun, Value::Null);
        self.plugin_manager
            .run_plugin_dispatch(
                plugin_id,
                EnvelopeKind::Manual,
                input.clone(),
                HookEvent::PluginManualRun.as_str(),
                envelope.occurred_at_ms,
                envelope.correlation_id,
                None,
                ExecutionPrincipal::Admin,
                0,
                input,
                HookExecutionMode::Standard,
                None,
            )
            .await
    }

    pub async fn dispatch_content_prepare_save(
        &self,
        type_key: &str,
        actor_role: opendash_core::content::FieldRole,
        effective_schema: &opendash_core::content::EffectiveContentSchema,
        payload: Value,
    ) -> crate::error::ApiResult<ContentPrepareSavePatch> {
        self.dispatch_content_prepare_save_with_scope(
            type_key,
            actor_role,
            effective_schema,
            payload,
            PrepareSaveMutationScope::Full,
            None,
        )
        .await
    }

    pub async fn dispatch_content_prepare_save_with_scope(
        &self,
        type_key: &str,
        actor_role: opendash_core::content::FieldRole,
        effective_schema: &opendash_core::content::EffectiveContentSchema,
        mut payload: Value,
        scope: PrepareSaveMutationScope,
        db: Option<&sqlx::SqlitePool>,
    ) -> crate::error::ApiResult<ContentPrepareSavePatch> {
        use crate::error::ApiError;

        let subs = match tokio::time::timeout(
            Duration::from_secs(1),
            self.plugin_manager
                .owner_filter_subscriptions(HookEvent::FilterContentPrepareSave, Some(type_key)),
        )
        .await
        {
            Ok(Ok(s)) => s,
            _ => {
                return Err(ApiError::unprocessable(
                    "Không thể tra cứu plugin filter:content/prepare_save.",
                ));
            }
        };
        if subs.is_empty() {
            return Ok(ContentPrepareSavePatch::default());
        }

        if let PrepareSaveMutationScope::Metadata {
            requested_title,
            requested_slug,
            ref taxonomy_key,
            ..
        } = scope
            && let Some(obj) = payload.as_object_mut()
        {
            obj.insert(
                "mutation_scope".into(),
                serde_json::json!({
                    "mode": "metadata",
                    "requested_title": requested_title,
                    "requested_slug": requested_slug,
                    "taxonomy_key": taxonomy_key,
                }),
            );
        }

        let started = Instant::now();
        let envelope = Self::envelope(HookEvent::FilterContentPrepareSave, payload);
        let mut running_payload = envelope.payload.clone();
        let mut merged = ContentPrepareSavePatch::default();

        for (index, sub) in subs.into_iter().enumerate() {
            if index >= MAX_PREPARE_SAVE_PLUGINS || started.elapsed() >= PREPARE_SAVE_PHASE_BUDGET {
                return Err(ApiError::unprocessable(
                    "Vượt quá giới hạn xử lý filter:content/prepare_save.",
                ));
            }
            let remaining = PREPARE_SAVE_PHASE_BUDGET.saturating_sub(started.elapsed());
            let mut plugin_payload = running_payload.clone();
            self.plugin_manager
                .filter_content_payload_for_owner(
                    &sub.owner,
                    type_key,
                    actor_role,
                    &mut plugin_payload,
                )
                .await;
            let legacy_env = serde_json::json!({
                "event": HookEvent::FilterContentPrepareSave.as_str(),
                "occurred_at_ms": envelope.occurred_at_ms,
                "correlation_id": &envelope.correlation_id,
                "locale": &envelope.locale,
                "payload": &plugin_payload,
            });
            let resp = match self
                .plugin_manager
                .run_owner_dispatch(
                    &sub.owner,
                    EnvelopeKind::Filter,
                    legacy_env,
                    HookEvent::FilterContentPrepareSave.as_str(),
                    envelope.occurred_at_ms,
                    envelope.correlation_id.clone(),
                    envelope.locale.clone(),
                    principal_for_field_role(actor_role),
                    0,
                    plugin_payload,
                    HookExecutionMode::FilterContentPrepareSave,
                    Some(remaining),
                )
                .await
            {
                Ok(run_resp) => run_resp.data.unwrap_or(Value::Null),
                Err((_status, resp)) => {
                    let msg = opendash_core::hooks::sanitize_plain_text(
                        resp.error
                            .as_deref()
                            .unwrap_or("filter:content/prepare_save thất bại."),
                        256,
                    );
                    if matches!(
                        resp.code.as_deref(),
                        Some("concurrency_limit" | "safe_mode")
                    ) {
                        return Err(ApiError::service_unavailable(msg));
                    }
                    self.plugin_manager
                        .record_owner_hook_failure(&sub.owner, &envelope.correlation_id, &msg)
                        .await;
                    return Err(ApiError::unprocessable(msg));
                }
            };

            let patch: ContentPrepareSavePatch = if resp.is_null() {
                ContentPrepareSavePatch::default()
            } else {
                match serde_json::from_value(resp) {
                    Ok(p) => p,
                    Err(_) => {
                        let msg = "Đầu ra filter:content/prepare_save không hợp lệ.";
                        self.plugin_manager
                            .record_owner_hook_failure(&sub.owner, &envelope.correlation_id, msg)
                            .await;
                        return Err(ApiError::unprocessable(msg));
                    }
                }
            };

            if patch.is_empty() {
                continue;
            }

            let Some(manifest) = self.plugin_manager.cached_owner_manifest(&sub.owner).await else {
                return Err(ApiError::unprocessable(
                    "Không tìm thấy manifest plugin filter:content/prepare_save.",
                ));
            };
            if !manifest.can_write_content_type(type_key) {
                return Err(ApiError::unprocessable(
                    "Plugin không có quyền ghi loại nội dung trong filter:content/prepare_save.",
                ));
            }
            for field_key in patch.values_patch.keys() {
                let Some(field_entry) = effective_schema.effective_fields.get(field_key) else {
                    return Err(ApiError::unprocessable(format!(
                        "Trường '{field_key}' trong values_patch không tồn tại ở '{type_key}'."
                    )));
                };
                if !opendash_core::plugin::plugin_can_write_field_for_owner(
                    sub.owner.kind,
                    &manifest,
                    None,
                    actor_role,
                    type_key,
                    field_entry,
                ) {
                    return Err(ApiError::unprocessable(format!(
                        "Plugin hoặc vai trò hiện tại không có quyền ghi trường '{field_key}' qua filter:content/prepare_save."
                    )));
                }
            }

            match &scope {
                PrepareSaveMutationScope::Full => {
                    if let Some(t) = patch.title {
                        merged.title = Some(t.clone());
                        if let Some(obj) = running_payload.as_object_mut() {
                            obj.insert("title".into(), Value::String(t));
                        }
                    }
                    if let Some(s) = patch.slug {
                        merged.slug = Some(s.clone());
                        if let Some(obj) = running_payload.as_object_mut() {
                            obj.insert("slug".into(), Value::String(s));
                        }
                    }
                    if let Some(term_ids) = patch.term_ids {
                        if let Some(obj) = running_payload.as_object_mut() {
                            obj.insert("term_ids".into(), serde_json::json!(term_ids));
                        }
                        merged.term_ids = Some(term_ids);
                    }
                    for (k, v) in patch.values_patch {
                        if let Some(Value::Object(vals)) =
                            running_payload.get_mut("candidate_values")
                        {
                            if v.is_null() {
                                vals.remove(&k);
                            } else {
                                vals.insert(k.clone(), v.clone());
                            }
                        }
                        merged.values_patch.insert(k, v);
                    }
                }
                PrepareSaveMutationScope::Metadata {
                    requested_title,
                    requested_slug,
                    taxonomy_key,
                    entry_locale,
                } => {
                    let prior_title = running_payload
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();
                    let prior_slug = running_payload
                        .get("slug")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();
                    let prior_values = running_payload
                        .get("candidate_values")
                        .and_then(Value::as_object);
                    let prior_terms: std::collections::BTreeSet<i64> = running_payload
                        .get("term_ids")
                        .and_then(Value::as_array)
                        .map(|arr| arr.iter().filter_map(Value::as_i64).collect())
                        .unwrap_or_default();

                    for (k, v) in &patch.values_patch {
                        let prior_val = prior_values.and_then(|vals| vals.get(k));
                        let is_noop = if v.is_null() {
                            prior_val.is_none() || prior_val == Some(&Value::Null)
                        } else {
                            prior_val == Some(v)
                        };
                        if !is_noop {
                            return Err(ApiError::unprocessable(
                                "metadata_hook_scope_conflict: Plugin filter cố gắng thay đổi trường nội dung ngoài phạm vi Sửa nhanh; hãy mở trình chỉnh sửa đầy đủ (Single Edit).",
                            ));
                        }
                    }

                    if let Some(ref t) = patch.title
                        && t != &prior_title
                    {
                        if !requested_title {
                            return Err(ApiError::unprocessable(
                                "metadata_hook_scope_conflict: Plugin filter cố gắng thay đổi tiêu đề ngoài phạm vi Sửa nhanh; hãy mở trình chỉnh sửa đầy đủ (Single Edit).",
                            ));
                        }
                        merged.title = Some(t.clone());
                        if let Some(obj) = running_payload.as_object_mut() {
                            obj.insert("title".into(), Value::String(t.clone()));
                        }
                    }

                    if let Some(ref s) = patch.slug
                        && s != &prior_slug
                    {
                        if !requested_slug {
                            return Err(ApiError::unprocessable(
                                "metadata_hook_scope_conflict: Plugin filter cố gắng thay đổi slug ngoài phạm vi Sửa nhanh; hãy mở trình chỉnh sửa đầy đủ (Single Edit).",
                            ));
                        }
                        merged.slug = Some(s.clone());
                        if let Some(obj) = running_payload.as_object_mut() {
                            obj.insert("slug".into(), Value::String(s.clone()));
                        }
                    }

                    if let Some(ref term_ids) = patch.term_ids {
                        let mut new_set = std::collections::BTreeSet::new();
                        for &tid in term_ids {
                            if tid <= 0 || !new_set.insert(tid) {
                                return Err(ApiError::unprocessable(
                                    "metadata_hook_scope_conflict: Plugin filter trả về danh sách term_ids không hợp lệ trong phạm vi Sửa nhanh; hãy mở trình chỉnh sửa đầy đủ (Single Edit).",
                                ));
                            }
                        }
                        if new_set != prior_terms {
                            let Some(allowed_key) = taxonomy_key.as_deref() else {
                                return Err(ApiError::unprocessable(
                                    "metadata_hook_scope_conflict: Plugin filter cố gắng thay đổi phân loại ngoài phạm vi Sửa nhanh; hãy mở trình chỉnh sửa đầy đủ (Single Edit).",
                                ));
                            };
                            let Some(pool) = db else {
                                return Err(ApiError::unprocessable(
                                    "metadata_hook_scope_conflict: Không thể xác minh phạm vi phân loại của plugin filter; hãy mở trình chỉnh sửa đầy đủ (Single Edit).",
                                ));
                            };
                            let changed: Vec<i64> = prior_terms
                                .symmetric_difference(&new_set)
                                .copied()
                                .collect();
                            let mut conn = pool.acquire().await?;
                            for tid in changed {
                                let info =
                                    crate::storage::entry_lifecycle::lookup_term_scope_in_conn(
                                        &mut conn, tid,
                                    )
                                    .await?;
                                let valid = info.is_some_and(|(kind, loc, active)| {
                                    active && kind == allowed_key && loc == *entry_locale
                                });
                                if !valid {
                                    return Err(ApiError::unprocessable(
                                        "metadata_hook_scope_conflict: Plugin filter cố gắng thay đổi phân loại ngoài phạm vi Sửa nhanh; hãy mở trình chỉnh sửa đầy đủ (Single Edit).",
                                    ));
                                }
                            }
                            let canonical_ids: Vec<i64> = new_set.into_iter().collect();
                            if let Some(obj) = running_payload.as_object_mut() {
                                obj.insert("term_ids".into(), serde_json::json!(canonical_ids));
                            }
                            merged.term_ids = Some(canonical_ids);
                        }
                    }
                }
            }
        }

        Ok(merged)
    }

    pub async fn dispatch_contact_before_submit(
        &self,
        _form_id: &str,
        submission_payload: Value,
    ) -> crate::error::ApiResult<ContactBeforeSubmitDecision> {
        use crate::error::ApiError;

        let subs = match tokio::time::timeout(
            Duration::from_secs(1),
            self.plugin_manager
                .owner_filter_subscriptions(HookEvent::FilterContactBeforeSubmit, None),
        )
        .await
        {
            Ok(Ok(s)) => s,
            _ => {
                return Err(ApiError::service_unavailable(
                    "Không thể tra cứu plugin kiểm tra biểu mẫu liên hệ.",
                ));
            }
        };
        if subs.is_empty() {
            return Ok(ContactBeforeSubmitDecision::Allow);
        }

        let started = Instant::now();
        let envelope = Self::envelope(HookEvent::FilterContactBeforeSubmit, submission_payload);

        for (index, sub) in subs.into_iter().enumerate() {
            if index >= MAX_CONTACT_FILTER_PLUGINS
                || started.elapsed() >= CONTACT_FILTER_PHASE_BUDGET
            {
                return Err(ApiError::unprocessable(
                    "Vượt quá giới hạn kiểm tra filter:contact/before_submit.",
                ));
            }
            let remaining = CONTACT_FILTER_PHASE_BUDGET.saturating_sub(started.elapsed());
            let legacy_env = serde_json::to_value(&envelope).unwrap_or(Value::Null);
            let out = match self
                .plugin_manager
                .run_owner_dispatch(
                    &sub.owner,
                    EnvelopeKind::Filter,
                    legacy_env,
                    HookEvent::FilterContactBeforeSubmit.as_str(),
                    envelope.occurred_at_ms,
                    envelope.correlation_id.clone(),
                    envelope.locale.clone(),
                    ExecutionPrincipal::Public,
                    0,
                    envelope.payload.clone(),
                    HookExecutionMode::FilterContactBeforeSubmit,
                    Some(remaining),
                )
                .await
            {
                Ok(run_resp) => run_resp.data.unwrap_or(Value::Null),
                Err((_status, resp)) => {
                    let msg = opendash_core::hooks::sanitize_plain_text(
                        resp.error
                            .as_deref()
                            .unwrap_or("filter:contact/before_submit thất bại."),
                        256,
                    );
                    if matches!(
                        resp.code.as_deref(),
                        Some("concurrency_limit" | "safe_mode")
                    ) {
                        return Err(ApiError::service_unavailable(msg));
                    }
                    self.plugin_manager
                        .record_owner_hook_failure(&sub.owner, &envelope.correlation_id, &msg)
                        .await;
                    return Err(ApiError::unprocessable(msg));
                }
            };

            let decision: ContactBeforeSubmitRaw = if out.is_null() {
                ContactBeforeSubmitRaw::Allow
            } else {
                match serde_json::from_value(out) {
                    Ok(d) => d,
                    Err(_) => {
                        let msg = "Đầu ra filter:contact/before_submit không hợp lệ.";
                        self.plugin_manager
                            .record_owner_hook_failure(&sub.owner, &envelope.correlation_id, msg)
                            .await;
                        return Err(ApiError::unprocessable(msg));
                    }
                }
            };

            match decision {
                ContactBeforeSubmitRaw::Allow => {}
                ContactBeforeSubmitRaw::Reject { code, message } => {
                    let clean_code = opendash_core::hooks::sanitize_plain_text(&code, 64);
                    let clean_msg = opendash_core::hooks::sanitize_plain_text(&message, 256);
                    return Ok(ContactBeforeSubmitDecision::Reject {
                        code: if clean_code.is_empty() {
                            "rejected".into()
                        } else {
                            clean_code
                        },
                        message: if clean_msg.is_empty() {
                            "Biểu mẫu liên hệ bị từ chối.".into()
                        } else {
                            clean_msg
                        },
                    });
                }
            }
        }

        Ok(ContactBeforeSubmitDecision::Allow)
    }

    #[allow(dead_code)]
    pub async fn dispatch_projection_filters(
        &self,
        event: HookEvent,
        plugin_id: Option<&str>,
        entry_payload: Value,
    ) -> crate::error::ApiResult<std::collections::BTreeMap<String, Value>> {
        let owner_filter =
            plugin_id.map(|id| (opendash_core::hooks::ExtensionOwnerKind::Plugin, id));
        let raw = self
            .dispatch_projection_filters_for_owners(event, owner_filter, entry_payload)
            .await?;
        let mut results = std::collections::BTreeMap::new();
        for (owner_kind, id, _fence, val) in raw {
            if owner_kind == opendash_core::hooks::ExtensionOwnerKind::Plugin {
                results.insert(id, val);
            }
        }
        Ok(results)
    }

    #[allow(clippy::type_complexity)]
    pub async fn dispatch_projection_filters_for_owners(
        &self,
        event: HookEvent,
        owner_filter: Option<(opendash_core::hooks::ExtensionOwnerKind, &str)>,
        entry_payload: Value,
    ) -> crate::error::ApiResult<
        Vec<(
            opendash_core::hooks::ExtensionOwnerKind,
            String,
            Option<(String, i64)>,
            Value,
        )>,
    > {
        let mut results = Vec::new();
        let type_key = entry_payload
            .get("content_type_key")
            .and_then(Value::as_str)
            .or_else(|| entry_payload.get("type_key").and_then(Value::as_str))
            .map(str::to_owned);

        let subs = match tokio::time::timeout(
            Duration::from_secs(1),
            self.plugin_manager
                .owner_filter_subscriptions(event, type_key.as_deref()),
        )
        .await
        {
            Ok(Ok(s)) => s,
            _ => return Ok(results),
        };
        if subs.is_empty() {
            return Ok(results);
        }

        let started = Instant::now();
        let envelope = Self::envelope(event, entry_payload);

        let mut dispatched = 0usize;
        for sub in subs {
            if let Some((target_kind, target_id)) = owner_filter
                && (sub.owner.kind != target_kind || sub.owner.id != target_id)
            {
                continue;
            }
            if dispatched >= MAX_PROJECTION_FILTER_PLUGINS
                || started.elapsed() >= PROJECTION_FILTER_PHASE_BUDGET
            {
                break;
            }
            dispatched += 1;
            let remaining = PROJECTION_FILTER_PHASE_BUDGET.saturating_sub(started.elapsed());
            let mut plugin_payload = envelope.payload.clone();
            if let Some(tk) = type_key.as_deref() {
                self.plugin_manager
                    .filter_public_content_payload_for_owner(&sub.owner, tk, &mut plugin_payload)
                    .await;
            }
            let legacy_env = serde_json::json!({
                "event": event.as_str(),
                "occurred_at_ms": envelope.occurred_at_ms,
                "correlation_id": &envelope.correlation_id,
                "locale": &envelope.locale,
                "payload": &plugin_payload,
            });
            match self
                .plugin_manager
                .run_owner_dispatch(
                    &sub.owner,
                    EnvelopeKind::Filter,
                    legacy_env,
                    event.as_str(),
                    envelope.occurred_at_ms,
                    envelope.correlation_id.clone(),
                    envelope.locale.clone(),
                    ExecutionPrincipal::Public,
                    0,
                    plugin_payload,
                    HookExecutionMode::FilterProjection,
                    Some(remaining),
                )
                .await
            {
                Ok(run_resp) => {
                    if let Some(val) = run_resp.data
                        && !val.is_null()
                    {
                        results.push((sub.owner.kind, sub.owner.id, sub.theme_fence, val));
                    }
                }
                Err((_status, resp)) => {
                    let msg = opendash_core::hooks::sanitize_plain_text(
                        resp.error.as_deref().unwrap_or("Projection filter failed"),
                        256,
                    );
                    self.plugin_manager
                        .record_owner_hook_failure(&sub.owner, &envelope.correlation_id, &msg)
                        .await;
                }
            }
        }

        Ok(results)
    }

    /// Dispatches dynamic render filters (`filter:render/head`, `filter:render/slot`,
    /// `filter:render/footer`) under `HookExecutionMode::FilterRender`, enforcing
    /// per-subscriber trust sanitization, `ExtensionRenderCache` (Public only),
    /// `1000ms`/call timeout, `1500ms` total phase budget, and fail-open without caching errors.
    pub async fn dispatch_render_hooks(
        &self,
        ctx: &RenderHookContext<'_>,
        slots: &[crate::template_sec::HookSlotInvocation],
        cache: &crate::cache::ExtensionRenderCache,
    ) -> RenderHooksOutput {
        let mut out = RenderHooksOutput::default();
        let clear_slots =
            |slots: &[crate::template_sec::HookSlotInvocation]| -> Vec<(String, String)> {
                slots
                    .iter()
                    .map(|s| (s.nonce.clone(), String::new()))
                    .collect()
            };

        if self.plugin_manager.is_safe_mode() {
            out.slot_replacements = clear_slots(slots);
            return out;
        }

        let head_subs = self
            .plugin_manager
            .owner_render_filter_subscriptions(HookEvent::FilterRenderHead)
            .await
            .unwrap_or_default();
        let footer_subs = self
            .plugin_manager
            .owner_render_filter_subscriptions(HookEvent::FilterRenderFooter)
            .await
            .unwrap_or_default();
        let slot_subs = if slots.is_empty() {
            Vec::new()
        } else {
            self.plugin_manager
                .owner_render_filter_subscriptions(HookEvent::FilterRenderSlot)
                .await
                .unwrap_or_default()
        };

        if head_subs.is_empty() && footer_subs.is_empty() && slot_subs.is_empty() {
            out.slot_replacements = clear_slots(slots);
            return out;
        }

        let started = Instant::now();
        let epoch = crate::cache::SiteCacheEpoch::global().current();

        // 1. filter:render/head
        for sub in &head_subs {
            if let Some(frag) = self
                .execute_render_subscriber(
                    HookEvent::FilterRenderHead,
                    None,
                    None,
                    sub,
                    ctx,
                    epoch,
                    started,
                    cache,
                )
                .await
            {
                out.head_styles.extend(frag.authorized_styles);
                out.head_scripts.extend(frag.authorized_head_scripts);
                out.head_html.push_str(&frag.html);
            }
        }

        // 2. filter:render/slot for each {{ od_hook(slot, args?) }} invocation
        for inv in slots {
            let mut combined = String::new();
            for sub in &slot_subs {
                if let Some(frag) = self
                    .execute_render_subscriber(
                        HookEvent::FilterRenderSlot,
                        Some(&inv.slot),
                        Some(&inv.args),
                        sub,
                        ctx,
                        epoch,
                        started,
                        cache,
                    )
                    .await
                {
                    out.head_styles.extend(frag.authorized_styles);
                    out.head_scripts.extend(frag.authorized_head_scripts);
                    combined.push_str(&frag.html);
                }
            }
            out.slot_replacements.push((inv.nonce.clone(), combined));
        }

        // 3. filter:render/footer
        for sub in &footer_subs {
            if let Some(frag) = self
                .execute_render_subscriber(
                    HookEvent::FilterRenderFooter,
                    None,
                    None,
                    sub,
                    ctx,
                    epoch,
                    started,
                    cache,
                )
                .await
            {
                out.head_styles.extend(frag.authorized_styles);
                out.head_scripts.extend(frag.authorized_head_scripts);
                out.footer_html.push_str(&frag.html);
            }
        }

        out
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn execute_render_subscriber(
        &self,
        event: HookEvent,
        slot_name: Option<&str>,
        slot_args: Option<&Value>,
        sub: &crate::plugin::manager::RenderFilterSubscriber,
        ctx: &RenderHookContext<'_>,
        site_data_epoch: u64,
        phase_started: Instant,
        cache: &crate::cache::ExtensionRenderCache,
    ) -> Option<crate::cache::CachedRenderFragment> {
        let cache_key = if ctx.principal == ExecutionPrincipal::Public {
            crate::cache::canonical_route_key(
                sub.render_cache,
                ctx.route_kind,
                ctx.route_path,
                ctx.canonical_query,
                ctx.is_not_found,
            )
            .map(|route_seg| crate::cache::RenderCacheKey {
                owner_kind: sub.owner.kind,
                owner_id: sub.owner.id.clone(),
                revision_digest_or_generation: sub.revision_key.clone(),
                hook_or_slot_name: match slot_name {
                    Some(s) => format!("slot:{s}"),
                    None => event.as_str().to_owned(),
                },
                arg_hash: crate::cache::hash_render_args(slot_args),
                locale: ctx.locale.to_owned(),
                canonical_route_and_query_or_global: route_seg,
                origin_host: ctx.origin_host.to_owned(),
                site_data_epoch,
            })
        } else {
            None
        };

        if let Some(ref key) = cache_key
            && let Some(hit) = cache.get(key)
        {
            return Some(hit);
        }

        let remaining = RENDER_HOOK_PHASE_BUDGET.saturating_sub(phase_started.elapsed());
        if remaining.is_zero() {
            return None;
        }
        let call_timeout = remaining.min(RENDER_HOOK_CALL_TIMEOUT);

        let mut payload_map = serde_json::Map::new();
        if let Some(slot) = slot_name {
            payload_map.insert("slot".into(), Value::String(slot.to_owned()));
        }
        if let Some(args) = slot_args
            && !args.is_null()
        {
            payload_map.insert("args".into(), args.clone());
        }
        payload_map.insert(
            "route_kind".into(),
            Value::String(ctx.route_kind.to_owned()),
        );
        payload_map.insert(
            "route_path".into(),
            Value::String(ctx.route_path.to_owned()),
        );
        payload_map.insert("locale".into(), Value::String(ctx.locale.to_owned()));
        payload_map.insert("site".into(), ctx.site.clone());
        payload_map.insert("options".into(), ctx.options.clone());
        if let Some(ref entry) = ctx.entry {
            payload_map.insert("entry".into(), entry.clone());
        }
        if let Some(ref entries) = ctx.entries {
            payload_map.insert("entries".into(), entries.clone());
        }
        let payload = Value::Object(payload_map);
        let envelope = Self::envelope(event, payload.clone());
        let legacy_env = serde_json::json!({
            "event": event.as_str(),
            "occurred_at_ms": envelope.occurred_at_ms,
            "correlation_id": &envelope.correlation_id,
            "locale": ctx.locale,
            "payload": &payload,
        });

        let run_resp = self
            .plugin_manager
            .run_owner_dispatch(
                &sub.owner,
                EnvelopeKind::Filter,
                legacy_env,
                event.as_str(),
                envelope.occurred_at_ms,
                envelope.correlation_id,
                Some(ctx.locale.to_owned()),
                ctx.principal,
                0,
                payload,
                HookExecutionMode::FilterRender,
                Some(call_timeout),
            )
            .await
            .ok()?;

        let raw_val = run_resp.data.unwrap_or(Value::Null);
        let trusted = sub.render_trusted_html;
        let sanitized = parse_and_sanitize_render_output(event, &raw_val, trusted)?;

        if let Some(key) = cache_key {
            cache.insert(key, sanitized.clone());
        }
        Some(sanitized)
    }

    pub async fn resolve_extension_route(
        &self,
        pkg: &str,
        method: opendash_core::plugin::RouteMethod,
        subpath: &str,
    ) -> Result<
        Option<crate::plugin::manager::ResolvedExtensionRoute>,
        (axum::http::StatusCode, String),
    > {
        self.plugin_manager
            .resolve_extension_route(pkg, method, subpath)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute_resolved_route(
        &self,
        resolved: crate::plugin::manager::ResolvedExtensionRoute,
        payload: Value,
        execution_mode: HookExecutionMode,
        principal: ExecutionPrincipal,
        timeout: Duration,
        actor: Option<&crate::auth::SessionContext>,
    ) -> Result<
        crate::plugin::manager::ExtensionRouteExecOutput,
        (
            axum::http::StatusCode,
            crate::plugin::manager::PluginRunResponse,
        ),
    > {
        self.plugin_manager
            .execute_resolved_route(
                resolved,
                payload,
                execution_mode,
                principal,
                timeout,
                actor,
                self,
            )
            .await
    }
}

pub const RENDER_HOOK_CALL_TIMEOUT: Duration = Duration::from_millis(1000);

pub const RENDER_HOOK_PHASE_BUDGET: Duration = Duration::from_millis(1500);

pub const MAX_RENDER_SUBSCRIBER_OUTPUT_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone)]
pub struct RenderHookContext<'a> {
    pub route_kind: &'a str,
    pub route_path: &'a str,
    pub canonical_query: &'a str,
    pub is_not_found: bool,
    pub locale: &'a str,
    pub origin_host: &'a str,
    pub site: Value,
    pub options: Value,
    pub entry: Option<Value>,
    pub entries: Option<Value>,
    pub principal: ExecutionPrincipal,
}

#[derive(Debug, Clone, Default)]
pub struct RenderHooksOutput {
    pub head_html: String,
    pub head_styles: Vec<String>,
    pub head_scripts: Vec<String>,
    pub footer_html: String,
    pub slot_replacements: Vec<(String, String)>,
}
