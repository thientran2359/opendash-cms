use super::*;

pub(crate) async fn invoke_callback(
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

tokio::task_local! { pub(crate) static IN_DISPATCH: bool; }

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
    pub(crate) tx: tokio::sync::mpsc::Sender<AsyncHookJob>,
    pub(crate) shared_rx: Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<AsyncHookJob>>>,
    pub(crate) maint_rx: tokio::sync::watch::Receiver<bool>,
    pub(crate) shut_rx: tokio::sync::watch::Receiver<bool>,
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
pub(crate) enum ContactBeforeSubmitRaw {
    Allow,
    Reject {
        #[serde(default)]
        code: String,
        #[serde(default)]
        message: String,
    },
}

pub(crate) struct Handler {
    pub(crate) id: String,
    pub(crate) event: HookEvent,
    pub(crate) priority: i32,
    pub(crate) callback: Callback,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ContentBeforeSaveVeto {
    pub code: String,
    pub message: String,
    pub field_errors: std::collections::BTreeMap<String, String>,
}

pub(crate) const CONTENT_HOOK_PHASE_BUDGET: Duration = Duration::from_secs(5);

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
    pub(crate) plugin_manager: Arc<PluginManager>,
    pub(crate) handlers: Vec<Handler>,
    pub(crate) recent_events: Mutex<Vec<HookEnvelope>>,
    pub(crate) async_queue: Option<AsyncHookQueue>,
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
