//! Read-only post-commit notifications with bounded, sequential execution.
use crate::{
    auth::generate_random_token,
    plugin::{PluginManager, manager::PluginRunResponse},
};
use axum::http::StatusCode;
use opendash_core::hooks::{HookEnvelope, HookEvent};
use serde_json::Value;
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub type HookFuture = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;
type Callback = Arc<dyn Fn(HookEnvelope) -> HookFuture + Send + Sync>;

// Catch both callback construction and asynchronous polling panics without spawning
// detached tasks. Rust callbacks are trusted/cooperative; Wasm stays in its worker.
async fn invoke_callback(callback: Callback, envelope: HookEnvelope) -> Result<(), String> {
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
tokio::task_local! { static IN_DISPATCH: bool; }

struct Handler {
    id: String,
    event: HookEvent,
    priority: i32,
    callback: Callback,
}
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContentBeforeSaveVeto {
    pub code: String,
    pub message: String,
    pub field_errors: std::collections::BTreeMap<String, String>,
}

const CONTENT_HOOK_PHASE_BUDGET: Duration = Duration::from_secs(5);

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
    plugin_manager: Arc<PluginManager>,
    handlers: Vec<Handler>,
    recent_events: Mutex<Vec<HookEnvelope>>,
}

impl HookDispatcher {
    pub fn new(plugin_manager: Arc<PluginManager>) -> Self {
        let mut dispatcher = Self {
            plugin_manager,
            handlers: Vec::new(),
            recent_events: Mutex::new(Vec::new()),
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

    fn envelope(event: HookEvent, payload: Value) -> HookEnvelope {
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

    async fn dispatch_envelope(&self, envelope: HookEnvelope) -> DispatchReport {
        let started = Instant::now();
        let mut outcomes = Vec::new();
        let mut ordered: Vec<(i32, String, Option<Callback>)> = self
            .handlers
            .iter()
            .filter(|h| h.event == envelope.event)
            .map(|h| (h.priority, h.id.clone(), Some(h.callback.clone())))
            .collect();
        match tokio::time::timeout(
            Duration::from_secs(1),
            self.plugin_manager.hook_subscriptions(envelope.event),
        )
        .await
        {
            Ok(Ok(ids)) => ordered.extend(
                ids.into_iter()
                    .map(|id| (100, format!("plugin/{id}"), None)),
            ),
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
        for (index, (_, id, callback)) in ordered.into_iter().enumerate() {
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
            } else {
                // No outer timeout around commit: retain M2 commit handover semantics.
                self.plugin_manager
                    .run_plugin_correlated(
                        id.strip_prefix("plugin/").unwrap(),
                        serde_json::to_value(&envelope).expect("JSON envelope"),
                        envelope.correlation_id.clone(),
                    )
                    .await
                    .map(|_| ())
                    .map_err(|(_, response)| {
                        response
                            .error
                            .unwrap_or_else(|| "Plugin hook failed".into())
                    })
            };
            if let Err(error) = &result {
                eprintln!("{} {id}: {error}", envelope.correlation_id);
                if let Some(plugin_id) = id.strip_prefix("plugin/") {
                    self.plugin_manager
                        .record_hook_failure(plugin_id, &envelope.correlation_id, error)
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


    pub async fn dispatch_content_before_save(
        &self,
        type_key: &str,
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
                        .content_hook_subscriptions(HookEvent::ContentBeforeSave, type_key),
                )
                .await
                {
                    Ok(Ok(s)) => s,
                    _ => {
                        return Err(ContentBeforeSaveVeto {
                            code: "hook_discovery_failed".into(),
                            message: "Không thể tra cứu plugin content/before_save.".into(),
                            field_errors: Default::default(),
                        });
                    }
                };

                for (index, (_prio, plugin_id)) in subs.into_iter().enumerate() {
                    if index >= MAX_HANDLERS || started.elapsed() >= CONTENT_HOOK_PHASE_BUDGET {
                        return Err(ContentBeforeSaveVeto {
                            code: "hook_budget_exceeded".into(),
                            message: "Vượt quá giới hạn xử lý content/before_save.".into(),
                            field_errors: Default::default(),
                        });
                    }
                    let remaining = CONTENT_HOOK_PHASE_BUDGET.saturating_sub(started.elapsed());
                    let env_val = serde_json::to_value(&envelope).expect("JSON envelope");
                    let out = match self
                        .plugin_manager
                        .run_plugin_with_mode(
                            &plugin_id,
                            env_val,
                            envelope.correlation_id.clone(),
                            opendash_core::hooks::HookExecutionMode::ContentBeforeSave,
                            Some(remaining),
                        )
                        .await
                    {
                        Ok(run_resp) => run_resp.data.unwrap_or(Value::Null),
                        Err((_status, resp)) => {
                            let err_msg = opendash_core::hooks::sanitize_plain_text(
                                resp.error.as_deref().unwrap_or("Plugin hook failed"),
                                256,
                            );
                            self.plugin_manager
                                .record_hook_failure(&plugin_id, &envelope.correlation_id, &err_msg)
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
                                    .record_hook_failure(&plugin_id, &envelope.correlation_id, msg)
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

    pub async fn dispatch_content_after_save(
        &self,
        type_key: &str,
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
                        .content_hook_subscriptions(HookEvent::ContentAfterSave, type_key),
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
                for (index, (_prio, plugin_id)) in subs.into_iter().enumerate() {
                    let result = if index >= MAX_HANDLERS
                        || started.elapsed() >= CONTENT_HOOK_PHASE_BUDGET
                    {
                        Err("ContentAfterSave phase budget exceeded".to_string())
                    } else {
                        let remaining = CONTENT_HOOK_PHASE_BUDGET.saturating_sub(started.elapsed());
                        let env_val = serde_json::to_value(&envelope).expect("JSON envelope");
                        self.plugin_manager
                            .run_plugin_with_mode(
                                &plugin_id,
                                env_val,
                                envelope.correlation_id.clone(),
                                opendash_core::hooks::HookExecutionMode::ContentAfterSave,
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
                        eprintln!("{} plugin/{}: {err}", envelope.correlation_id, plugin_id);
                        self.plugin_manager
                            .record_hook_failure(&plugin_id, &envelope.correlation_id, err)
                            .await;
                    }
                    outcomes.push(HookOutcome {
                        handler_id: format!("plugin/{plugin_id}"),
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
            .run_plugin_correlated(plugin_id, input, envelope.correlation_id)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    #[tokio::test]
    async fn ordered_handlers_continue_after_failure_and_timeout() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        let mut dispatcher = HookDispatcher::new(Arc::new(PluginManager::new(pool, false)));
        let seen = Arc::new(Mutex::new(Vec::new()));
        for (id, priority) in [("core/last", 30), ("core/first", 10)] {
            let seen = seen.clone();
            dispatcher
                .register_handler(id, HookEvent::PageAfterSave, priority, move |env| {
                    let seen = seen.clone();
                    Box::pin(async move {
                        seen.lock().unwrap().push((id, env));
                        Ok(())
                    })
                })
                .unwrap();
        }
        assert!(
            dispatcher
                .register_handler("core/first", HookEvent::PageAfterSave, 0, |_| Box::pin(
                    async { Ok(()) }
                ))
                .is_err()
        );
        dispatcher
            .register_handler("core/fail", HookEvent::PageAfterSave, 15, |_| {
                Box::pin(async { Err("failed".into()) })
            })
            .unwrap();
        dispatcher
            .register_handler("core/slow", HookEvent::PageAfterSave, 20, |_| {
                Box::pin(std::future::pending())
            })
            .unwrap();
        let report = dispatcher
            .dispatch(HookEvent::PageAfterSave, serde_json::json!({"saved": true}))
            .await;
        {
            let seen = seen.lock().unwrap();
            assert_eq!(
                seen.iter().map(|v| v.0).collect::<Vec<_>>(),
                ["core/first", "core/last"]
            );
            assert_eq!(seen[0].1, seen[1].1);
        }
        assert!(report.envelope.occurred_at_ms > 0);
        assert_eq!(
            report.outcomes.iter().filter(|v| v.error.is_some()).count(),
            2
        );
        assert!(
            report
                .outcomes
                .iter()
                .any(|v| v.handler_id == "core/slow" && v.error.is_some())
        );
        let unsupported = dispatcher
            .dispatch(HookEvent::ContentBeforeSave, Value::Null)
            .await;
        assert!(unsupported.outcomes[0].error.is_some());
        let nested = IN_DISPATCH
            .scope(
                true,
                dispatcher.dispatch(HookEvent::PageAfterSave, Value::Null),
            )
            .await;
        assert!(nested.outcomes[0].error.is_some());
    }
}
