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

#[tokio::test]
async fn maintenance_epoch_advances_on_draining_completion_timeout_and_preserves_cloned_lease() {
    let (root, paths) = temp_site_paths("maint-epoch");
    let coord = SiteCoordinator::acquire(paths).expect("acquire");
    assert_eq!(coord.current_maintenance_epoch(), 0);

    let lease0 = coord.admit_mutation().expect("lease 0");
    assert_eq!(lease0.maintenance_epoch(), 0);
    let cloned0 = lease0.clone();
    assert_eq!(cloned0.maintenance_epoch(), 0);

    // Drain timeout STILL advances maintenance_epoch because draining started.
    let timeout_res = coord
        .begin_maintenance(OperationKind::Reconcile, Duration::from_millis(30))
        .await;
    assert!(matches!(
        timeout_res,
        Err(MaintenanceError::DrainTimeout { active_writers: 2 })
    ));
    assert_eq!(coord.current_maintenance_epoch(), 1);
    // Existing draining lease retains its original epoch 0, even when cloned afterwards.
    assert_eq!(lease0.maintenance_epoch(), 0);
    assert_eq!(lease0.clone().maintenance_epoch(), 0);
    drop(cloned0);
    drop(lease0);

    let lease1 = coord.admit_mutation().expect("lease 1");
    assert_eq!(lease1.maintenance_epoch(), 1);
    drop(lease1);

    // Successful maintenance also increments epoch monotonically.
    let ctx = coord
        .begin_maintenance(OperationKind::Backup, Duration::from_secs(2))
        .await
        .expect("maint ok");
    assert_eq!(coord.current_maintenance_epoch(), 2);
    drop(ctx);

    let lease2 = coord.admit_mutation().expect("lease 2");
    assert_eq!(lease2.maintenance_epoch(), 2);
    drop(lease2);

    // Overflow fails closed without mutating maintenance state.
    {
        let mut guard = coord.inner.state.lock().unwrap();
        guard.maintenance_epoch = u64::MAX;
    }
    assert_eq!(
        coord
            .begin_maintenance(OperationKind::Backup, Duration::from_millis(20))
            .await
            .err(),
        Some(MaintenanceError::EpochOverflow)
    );
    assert!(!coord.is_maintenance_active());
    let _ = std::fs::remove_dir_all(root);
}
