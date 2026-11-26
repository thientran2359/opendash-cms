use super::*;

pub type HookFuture = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

pub(super) type Callback = Arc<dyn Fn(HookEnvelope) -> HookFuture + Send + Sync>;

pub(crate) async fn wait_watch_bool(rx: &mut tokio::sync::watch::Receiver<bool>, expected: bool) {
    loop {
        if *rx.borrow_and_update() == expected {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}

impl HookEnqueueOutcome {
    pub(super) fn record(&mut self, accepted: bool) {
        if accepted {
            self.enqueued = self.enqueued.saturating_add(1);
        } else {
            self.dropped = self.dropped.saturating_add(1);
        }
    }
}
