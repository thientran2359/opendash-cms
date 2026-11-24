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
}

impl std::fmt::Display for MaintenanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyActive => write!(f, "Đã có phiên bảo trì độc quyền đang hoạt động."),
            Self::DrainTimeout { active_writers } => write!(
                f,
                "Hết thời gian chờ drain ({active_writers} tác vụ ghi vẫn đang chạy)."
            ),
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
        guard.active_writers += 1;
        Ok(MutationLease {
            inner: Arc::clone(&self.inner),
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
}

impl std::fmt::Debug for MutationLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MutationLease").finish_non_exhaustive()
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
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_site_paths(label: &str) -> (PathBuf, SitePaths) {
        let id = COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "opendash-ops-unit-{label}-{}-{id}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let site = root.join("site");
        let db = site.join("database").join("opendash.sqlite");
        (root, SitePaths::from_parts(site, db))
    }

    #[test]
    fn clone_keeps_locks_alive_until_last_handle_drops() {
        let (root, paths) = temp_site_paths("clone-lock");
        let coord1 = SiteCoordinator::acquire(paths.clone()).expect("acquire first");
        let coord2 = coord1.clone();

        drop(coord1);
        assert!(matches!(
            SiteCoordinator::acquire(paths.clone()),
            Err(CoordinatorError::Busy { .. })
        ));

        drop(coord2);
        let coord3 = SiteCoordinator::acquire(paths).expect("reacquire after last drop");
        drop(coord3);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn exclusive_maintenance_drains_writers_and_returns_503_retry_after_3() {
        let (root, paths) = temp_site_paths("drain");
        let coord = SiteCoordinator::acquire(paths).expect("acquire");

        let lease = coord.admit_mutation().expect("admit initial writer");
        assert_eq!(coord.active_writers(), 1);

        let coord_bg = coord.clone();
        let maint_task = tokio::spawn(async move {
            coord_bg
                .begin_maintenance(OperationKind::Backup, Duration::from_secs(5))
                .await
        });

        tokio::time::sleep(Duration::from_millis(30)).await;
        let blocked = coord.admit_mutation();
        assert!(blocked.is_err());
        let resp = blocked.err().expect("blocked err").into_response();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            resp.headers()
                .get(axum::http::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok()),
            Some("3")
        );

        drop(lease);
        let ctx = maint_task
            .await
            .expect("join maint")
            .expect("exclusive context");
        assert_eq!(ctx.kind(), OperationKind::Backup);
        assert!(coord.is_maintenance_active());

        drop(ctx);
        assert!(!coord.is_maintenance_active());
        let _lease2 = coord.admit_mutation().expect("admit after maintenance");
        drop(_lease2);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn drain_timeout_fails_busy_and_restores_normal_admission() {
        let (root, paths) = temp_site_paths("timeout");
        let coord = SiteCoordinator::acquire(paths).expect("acquire");

        let lease = coord.admit_mutation().expect("admit writer");
        let res = coord
            .begin_maintenance(OperationKind::Restore, Duration::from_millis(40))
            .await;
        assert!(matches!(
            res,
            Err(MaintenanceError::DrainTimeout { active_writers: 1 })
        ));
        assert!(!coord.is_maintenance_active());

        drop(lease);
        let _lease2 = coord.admit_mutation().expect("admit after timeout");
        drop(_lease2);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn cloned_lease_blocks_maintenance_until_background_task_finishes() {
        let (root, paths) = temp_site_paths("bg-lease");
        let coord = SiteCoordinator::acquire(paths).expect("acquire");

        let http_lease = coord.admit_mutation().expect("admit http handler");
        let bg_lease = http_lease.clone();
        let bg_done = Arc::new(AtomicBool::new(false));
        let bg_done_flag = Arc::clone(&bg_done);

        let bg_handle = tokio::spawn(async move {
            let _lease = bg_lease;
            tokio::time::sleep(Duration::from_millis(80)).await;
            bg_done_flag.store(true, Ordering::SeqCst);
        });

        // HTTP handler returns and drops its lease while background task still runs
        drop(http_lease);
        assert_eq!(coord.active_writers(), 1);

        let ctx = coord
            .begin_maintenance(OperationKind::Backup, Duration::from_secs(5))
            .await
            .expect("maintenance enters after bg finishes");
        assert!(bg_done.load(Ordering::SeqCst));
        drop(ctx);
        bg_handle.await.expect("join bg task");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn cancel_signal_wakes_and_joins_task() {
        let signal = OperationCancelSignal::new();
        let worker_signal = signal.clone();
        let handle = tokio::spawn(async move {
            worker_signal.cancelled().await;
            "cancelled-cleanly"
        });
        tokio::time::sleep(Duration::from_millis(15)).await;
        signal.cancel();
        let outcome = handle.await.expect("join cancelled task");
        assert_eq!(outcome, "cancelled-cleanly");
    }

    #[tokio::test]
    async fn drain_for_shutdown_blocks_new_mutations_waits_for_active_writer_and_reports_timeout() {
        let (root, paths) = temp_site_paths("shutdown-drain");
        let coord = SiteCoordinator::acquire(paths).expect("acquire");

        let lease = coord.admit_mutation().expect("admit active writer");
        let coord_clone = coord.clone();
        let drain_task = tokio::spawn(async move {
            coord_clone
                .drain_for_shutdown(Duration::from_millis(50))
                .await
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        // New mutations are blocked once shutdown drain begins
        assert!(coord.admit_mutation().is_err());
        // Timeout reports active writer count instead of silently succeeding
        assert_eq!(drain_task.await.expect("join drain"), Err(1));

        // Dropping the lease allows subsequent drain_for_shutdown to succeed cleanly
        let coord_clone2 = coord.clone();
        let release_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            drop(lease);
        });
        assert_eq!(
            coord_clone2
                .drain_for_shutdown(Duration::from_secs(2))
                .await,
            Ok(())
        );
        release_task.await.expect("join release");
        let _ = std::fs::remove_dir_all(root);
    }
}
