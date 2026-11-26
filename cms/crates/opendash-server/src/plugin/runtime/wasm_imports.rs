use super::*;

#[derive(Debug, Clone)]
pub struct ActionActorFence {
    pub user_id: i64,
    pub required_role: String,
}

#[derive(Debug, Clone)]
pub struct ActionExecutionOutput {
    pub output: Value,
    pub staged_content_command: Option<host_ops::StagedContentCommand>,
    pub staged_lifecycle_command: Option<host_ops::StagedLifecycleCommand>,
}

pub(super) struct StagedCommitGuard {
    pub(super) conn: Option<sqlx::pool::PoolConnection<sqlx::Sqlite>>,
    pub(super) _initial_busy_timeout: i64,
    pub(super) committed: bool,
}

impl Drop for StagedCommitGuard {
    fn drop(&mut self) {
        if !self.committed
            && let Some(mut conn) = self.conn.take()
        {
            conn.close_on_drop();
        }
    }
}

pub(super) struct ProcessGuard {
    pub(super) child: Arc<std::sync::Mutex<Option<GuardedChild>>>,
    pub(super) worker_thread: Arc<std::sync::Mutex<Option<std::thread::JoinHandle<()>>>>,
    pub(super) active_workers: Arc<AtomicUsize>,
    pub(super) cancellation_token: Arc<AtomicBool>,
    pub(super) cancel_tx: watch::Sender<bool>,
    pub(super) permit: Option<tokio::sync::OwnedSemaphorePermit>,
}

impl ProcessGuard {
    pub(super) fn cleanup(&mut self) {
        self.cancellation_token.store(true, Ordering::SeqCst);
        let _ = self.cancel_tx.send(true);
        if let Ok(mut lock) = self.child.lock()
            && let Some(mut child) = lock.take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Ok(mut lock) = self.worker_thread.lock()
            && let Some(handle) = lock.take()
        {
            let _ = handle.join();
        }
        if self.permit.is_some() {
            self.active_workers.fetch_sub(1, Ordering::SeqCst);
            drop(self.permit.take());
        }
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        self.cleanup();
    }
}

pub const PUBLIC_RENDER_SEMAPHORE_WAIT: Duration = Duration::from_millis(200);
