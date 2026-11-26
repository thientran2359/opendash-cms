use std::fs::{File, OpenOptions, TryLockError};

use std::io;

use std::path::PathBuf;

use std::sync::atomic::{AtomicBool, Ordering};

use std::sync::{Arc, Mutex};

use std::time::{Duration, Instant};

use axum::http::{StatusCode, header};

use axum::response::{IntoResponse, Response};

use tokio::sync::Notify;

use crate::error::ApiError;

use crate::paths::SitePaths;

use crate::storage::operations::OperationKind;

pub const MAX_MAINTENANCE_DRAIN: Duration = Duration::from_secs(30);

pub const MAINTENANCE_RETRY_AFTER_SECS: &str = "3";

#[derive(Debug)]
pub enum CoordinatorError {
    Busy { lock_path: PathBuf },
    Io(io::Error),
}

impl std::fmt::Display for CoordinatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy { lock_path } => write!(
                f,
                "Another OpenDash instance already holds lock {}",
                lock_path.display()
            ),
            Self::Io(err) => write!(f, "Site coordinator I/O error: {err}"),
        }
    }
}

impl std::error::Error for CoordinatorError {}

impl From<io::Error> for CoordinatorError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationAdmissionError {
    MaintenanceActive { kind: OperationKind },
}

impl std::fmt::Display for MutationAdmissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MaintenanceActive { kind } => write!(
                f,
                "Hệ thống đang tạm khóa ghi để thực hiện bảo trì ({}). Vui lòng thử lại sau.",
                kind.as_str()
            ),
        }
    }
}

impl IntoResponse for MutationAdmissionError {
    fn into_response(self) -> Response {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            [
                (header::RETRY_AFTER, MAINTENANCE_RETRY_AFTER_SECS),
                (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            ],
            self.to_string(),
        )
            .into_response()
    }
}

impl From<MutationAdmissionError> for ApiError {
    fn from(err: MutationAdmissionError) -> Self {
        ApiError::service_unavailable(err.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaintenanceError {
    AlreadyActive,
    DrainTimeout { active_writers: usize },
    EpochOverflow,
}

impl std::fmt::Display for MaintenanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyActive => write!(f, "Đã có phiên bảo trì độc quyền đang hoạt động."),
            Self::DrainTimeout { active_writers } => write!(
                f,
                "Hết thời gian chờ drain ({active_writers} tác vụ ghi vẫn đang chạy)."
            ),
            Self::EpochOverflow => write!(f, "Tràn bộ đếm chu kỳ bảo trì trong tiến trình."),
        }
    }
}

impl std::error::Error for MaintenanceError {}

#[derive(Debug, Clone, Copy)]
struct ActiveMaintenance {
    kind: OperationKind,
    #[allow(dead_code)]
    draining: bool,
}

#[derive(Debug, Default)]
struct AdmissionState {
    active_writers: usize,
    maintenance: Option<ActiveMaintenance>,
    maintenance_epoch: u64,
    shutting_down: bool,
}

struct CoordinatorInner {
    paths: SitePaths,
    _locks: Vec<File>,
    state: Mutex<AdmissionState>,
    notify: Notify,
    maintenance_cancel_tx: tokio::sync::watch::Sender<bool>,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
}

/// Process-wide instance coordinator holding exclusive canonical site + DB locks
/// and mediating normal mutation leases vs. exclusive maintenance windows.
#[derive(Clone)]
pub struct SiteCoordinator {
    inner: Arc<CoordinatorInner>,
}

impl SiteCoordinator {
    /// Canonicalizes `paths`, rejects symlinks/reparse points, and acquires exclusive
    /// non-blocking OS locks on `<site_root>/runtime/site.lock` and `<db>.opendash.lock`
    /// in sorted canonical-path order.
    pub fn acquire(paths: SitePaths) -> Result<Self, CoordinatorError> {
        let canonical = paths.canonicalize_for_runtime()?;
        let lock_paths = canonical.ordered_lock_paths();
        let mut acquired = Vec::with_capacity(lock_paths.len());

        for lock_path in lock_paths {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(&lock_path)?;
            match file.try_lock() {
                Ok(()) => acquired.push(file),
                Err(TryLockError::WouldBlock) => {
                    return Err(CoordinatorError::Busy { lock_path });
                }
                Err(TryLockError::Error(err)) => {
                    return Err(CoordinatorError::Io(err));
                }
            }
        }

        let (maintenance_cancel_tx, _) = tokio::sync::watch::channel(false);
        let (shutdown_tx, _) = tokio::sync::watch::channel(false);

        Ok(Self {
            inner: Arc::new(CoordinatorInner {
                paths: canonical,
                _locks: acquired,
                state: Mutex::new(AdmissionState::default()),
                notify: Notify::new(),
                maintenance_cancel_tx,
                shutdown_tx,
            }),
        })
    }

    pub fn maintenance_cancel_rx(&self) -> tokio::sync::watch::Receiver<bool> {
        self.inner.maintenance_cancel_tx.subscribe()
    }

    pub fn shutdown_rx(&self) -> tokio::sync::watch::Receiver<bool> {
        self.inner.shutdown_tx.subscribe()
    }

    pub fn notify_shutdown(&self) {
        let _ = self.inner.shutdown_tx.send(true);
        let _ = self.inner.maintenance_cancel_tx.send(true);
    }

    #[allow(dead_code)]
    pub fn paths(&self) -> &SitePaths {
        &self.inner.paths
    }

    #[allow(dead_code)]
    pub fn active_writers(&self) -> usize {
        self.inner
            .state
            .lock()
            .expect("coordinator admission mutex poisoned")
            .active_writers
    }

    #[allow(dead_code)]
    pub fn is_maintenance_active(&self) -> bool {
        self.inner
            .state
            .lock()
            .expect("coordinator admission mutex poisoned")
            .maintenance
            .is_some()
    }

    pub fn current_maintenance_epoch(&self) -> u64 {
        self.inner
            .state
            .lock()
            .expect("coordinator admission mutex poisoned")
            .maintenance_epoch
    }

    /// Admits a normal mutating request unless exclusive maintenance is draining or active.
    pub fn admit_mutation(&self) -> Result<MutationLease, MutationAdmissionError> {
        let mut guard = self
            .inner
            .state
            .lock()
            .expect("coordinator admission mutex poisoned");
        if guard.shutting_down {
            return Err(MutationAdmissionError::MaintenanceActive {
                kind: OperationKind::Reconcile,
            });
        }
        if let Some(maint) = guard.maintenance {
            return Err(MutationAdmissionError::MaintenanceActive { kind: maint.kind });
        }
        let maintenance_epoch = guard.maintenance_epoch;
        guard.active_writers += 1;
        Ok(MutationLease {
            inner: Arc::clone(&self.inner),
            maintenance_epoch,
        })
    }

    /// Blocks new mutations, waits up to `min(drain_timeout, 30s)` for active `MutationLease`s
    /// to finish, and returns an unforgeable `ExclusiveOpsContext` token.
    pub async fn begin_maintenance(
        &self,
        kind: OperationKind,
        drain_timeout: Duration,
    ) -> Result<ExclusiveOpsContext, MaintenanceError> {
        let effective_timeout = drain_timeout.min(MAX_MAINTENANCE_DRAIN);
        let deadline = Instant::now() + effective_timeout;

        {
            let mut guard = self
                .inner
                .state
                .lock()
                .expect("coordinator admission mutex poisoned");
            if guard.maintenance.is_some() {
                return Err(MaintenanceError::AlreadyActive);
            }
            let next_epoch = guard
                .maintenance_epoch
                .checked_add(1)
                .ok_or(MaintenanceError::EpochOverflow)?;
            guard.maintenance_epoch = next_epoch;
            guard.maintenance = Some(ActiveMaintenance {
                kind,
                draining: true,
            });
            let _ = self.inner.maintenance_cancel_tx.send(true);
            if guard.active_writers == 0 {
                guard.maintenance = Some(ActiveMaintenance {
                    kind,
                    draining: false,
                });
                return Ok(ExclusiveOpsContext {
                    inner: Arc::clone(&self.inner),
                    kind,
                    _private: (),
                });
            }
        }

        loop {
            let notified = self.inner.notify.notified();
            let remaining = {
                let mut guard = self
                    .inner
                    .state
                    .lock()
                    .expect("coordinator admission mutex poisoned");
                if guard.active_writers == 0 {
                    guard.maintenance = Some(ActiveMaintenance {
                        kind,
                        draining: false,
                    });
                    return Ok(ExclusiveOpsContext {
                        inner: Arc::clone(&self.inner),
                        kind,
                        _private: (),
                    });
                }
                guard.active_writers
            };

            let now = Instant::now();
            if now >= deadline {
                let mut guard = self
                    .inner
                    .state
                    .lock()
                    .expect("coordinator admission mutex poisoned");
                let writers = guard.active_writers;
                if writers == 0 {
                    guard.maintenance = Some(ActiveMaintenance {
                        kind,
                        draining: false,
                    });
                    return Ok(ExclusiveOpsContext {
                        inner: Arc::clone(&self.inner),
                        kind,
                        _private: (),
                    });
                }
                guard.maintenance = None;
                let _ = self.inner.maintenance_cancel_tx.send(false);
                self.inner.notify.notify_waiters();
                return Err(MaintenanceError::DrainTimeout {
                    active_writers: writers.max(remaining),
                });
            }

            let wait_for = deadline.saturating_duration_since(now);
            if tokio::time::timeout(wait_for, notified).await.is_err() {
                let mut guard = self
                    .inner
                    .state
                    .lock()
                    .expect("coordinator admission mutex poisoned");
                let writers = guard.active_writers;
                if writers == 0 {
                    guard.maintenance = Some(ActiveMaintenance {
                        kind,
                        draining: false,
                    });
                    return Ok(ExclusiveOpsContext {
                        inner: Arc::clone(&self.inner),
                        kind,
                        _private: (),
                    });
                }
                guard.maintenance = None;
                let _ = self.inner.maintenance_cancel_tx.send(false);
                self.inner.notify.notify_waiters();
                return Err(MaintenanceError::DrainTimeout {
                    active_writers: writers,
                });
            }
        }
    }
}

impl SiteCoordinator {
    /// Blocks all new mutations and waits up to `timeout` for active `MutationLease`s
    /// (including background SMTP tasks) to complete before shutdown closes storage/locks.
    pub async fn drain_for_shutdown(&self, timeout: Duration) -> Result<(), usize> {
        let deadline = Instant::now() + timeout;
        {
            let mut guard = self
                .inner
                .state
                .lock()
                .expect("coordinator admission mutex poisoned");
            guard.shutting_down = true;
            let _ = self.inner.shutdown_tx.send(true);
            let _ = self.inner.maintenance_cancel_tx.send(true);
            if guard.active_writers == 0 {
                return Ok(());
            }
        }

        loop {
            let notified = self.inner.notify.notified();
            {
                let guard = self
                    .inner
                    .state
                    .lock()
                    .expect("coordinator admission mutex poisoned");
                if guard.active_writers == 0 {
                    return Ok(());
                }
            }
            let now = Instant::now();
            if now >= deadline {
                let writers = self
                    .inner
                    .state
                    .lock()
                    .expect("coordinator admission mutex poisoned")
                    .active_writers;
                return if writers == 0 { Ok(()) } else { Err(writers) };
            }
            let wait_for = deadline.saturating_duration_since(now);
            let _ = tokio::time::timeout(wait_for, notified).await;
        }
    }
}

/// Active writer lease; releasing it notifies any draining maintenance window.
pub struct MutationLease {
    inner: Arc<CoordinatorInner>,
    maintenance_epoch: u64,
}

impl MutationLease {
    pub fn maintenance_epoch(&self) -> u64 {
        self.maintenance_epoch
    }
}

impl std::fmt::Debug for MutationLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MutationLease")
            .field("maintenance_epoch", &self.maintenance_epoch)
            .finish_non_exhaustive()
    }
}

impl Clone for MutationLease {
    fn clone(&self) -> Self {
        let mut guard = self
            .inner
            .state
            .lock()
            .expect("coordinator admission mutex poisoned");
        guard.active_writers += 1;
        Self {
            inner: Arc::clone(&self.inner),
            maintenance_epoch: self.maintenance_epoch,
        }
    }
}

/// Axum extractor that acquires a `MutationLease` from `SiteCoordinator` in `AppState`
/// before any body buffering or storage/filesystem writes occur.
pub struct AdmittedMutation(pub MutationLease);

impl<S> axum::extract::FromRequestParts<S> for AdmittedMutation
where
    Arc<SiteCoordinator>: axum::extract::FromRef<S>,
    S: Send + Sync,
{
    type Rejection = MutationAdmissionError;

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let coordinator = <Arc<SiteCoordinator> as axum::extract::FromRef<S>>::from_ref(state);
        coordinator.admit_mutation().map(Self)
    }
}

impl Drop for MutationLease {
    fn drop(&mut self) {
        let mut guard = self
            .inner
            .state
            .lock()
            .expect("coordinator admission mutex poisoned");
        guard.active_writers = guard.active_writers.saturating_sub(1);
        if guard.active_writers == 0 {
            self.inner.notify.notify_waiters();
        }
    }
}

/// Unforgeable token proving exclusive maintenance mode is active and all writers drained.
pub struct ExclusiveOpsContext {
    inner: Arc<CoordinatorInner>,
    kind: OperationKind,
    _private: (),
}

impl ExclusiveOpsContext {
    #[allow(dead_code)]
    pub fn kind(&self) -> OperationKind {
        self.kind
    }
}

impl Drop for ExclusiveOpsContext {
    fn drop(&mut self) {
        let mut guard = self
            .inner
            .state
            .lock()
            .expect("coordinator admission mutex poisoned");
        guard.maintenance = None;
        let _ = self.inner.maintenance_cancel_tx.send(false);
        self.inner.notify.notify_waiters();
    }
}

/// Cooperative cancellation signal for long-running maintenance tasks.
#[derive(Clone, Default)]
pub struct OperationCancelSignal {
    cancelled: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl OperationCancelSignal {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    #[allow(dead_code)]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    #[allow(dead_code)]
    pub async fn cancelled(&self) {
        loop {
            let notified = self.notify.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}

#[cfg(test)]
#[path = "operations_tests.rs"]
mod tests;
