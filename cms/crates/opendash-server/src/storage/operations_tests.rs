use super::*;

use super::*;
use axum::http::StatusCode;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

static DB_COUNTER: AtomicU64 = AtomicU64::new(0);

async fn setup_test_db(label: &str) -> (std::path::PathBuf, SqlitePool) {
    let id = DB_COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "opendash-ops-ledger-{label}-{}-{id}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let db_path = dir.join("opendash.sqlite");
    let db = crate::storage::init_db(&db_path)
        .await
        .expect("init test db");
    (dir, db)
}

fn hex_id(seed: usize) -> String {
    format!("{seed:064x}")
}

#[tokio::test]
async fn ledger_enforces_cas_412_single_heavy_409_and_200_receipt_cap() {
    let (dir, db) = setup_test_db("cas-cap").await;

    let bad = create_job(
        &db,
        JobCreateInput {
            id: "short".to_string(),
            kind: OperationKind::Backup,
            actor_user_id: None,
            reason: None,
        },
    )
    .await;
    assert_eq!(bad.expect_err("invalid job id").0, StatusCode::BAD_REQUEST);

    let j1 = create_job(
        &db,
        JobCreateInput {
            id: hex_id(1),
            kind: OperationKind::Backup,
            actor_user_id: Some("admin-1".to_string()),
            reason: Some("nightly".to_string()),
        },
    )
    .await
    .expect("create first heavy job");
    assert_eq!(j1.version, 1);
    assert_eq!(j1.state, OperationState::Queued);

    let j2_conflict = create_job(
        &db,
        JobCreateInput {
            id: hex_id(2),
            kind: OperationKind::Restore,
            actor_user_id: Some("admin-1".to_string()),
            reason: None,
        },
    )
    .await;
    assert_eq!(
        j2_conflict.expect_err("second active heavy job").0,
        StatusCode::CONFLICT
    );

    let media_job = create_job(
        &db,
        JobCreateInput {
            id: hex_id(3),
            kind: OperationKind::MediaDelete,
            actor_user_id: None,
            reason: None,
        },
    )
    .await
    .expect("non-heavy media_delete allowed alongside active heavy job");

    let running = transition_job_cas(
        &db,
        &j1.id,
        JobTransitionInput {
            expected_version: 1,
            next_state: OperationState::Running,
            summary_json: None,
            error_code: None,
            error_message: None,
        },
    )
    .await
    .expect("queued -> running");
    assert_eq!(running.version, 2);

    let stale = transition_job_cas(
        &db,
        &j1.id,
        JobTransitionInput {
            expected_version: 1,
            next_state: OperationState::Succeeded,
            summary_json: None,
            error_code: None,
            error_message: None,
        },
    )
    .await;
    assert_eq!(
        stale.expect_err("stale cas version").0,
        StatusCode::PRECONDITION_FAILED
    );

    let finished = transition_job_cas(
        &db,
        &j1.id,
        JobTransitionInput {
            expected_version: 2,
            next_state: OperationState::Succeeded,
            summary_json: Some("{\"ok\":true}".to_string()),
            error_code: None,
            error_message: None,
        },
    )
    .await
    .expect("running -> succeeded");
    assert_eq!(finished.version, 3);

    // Now that j1 is terminal, another heavy job can be created
    let j4 = create_job(
        &db,
        JobCreateInput {
            id: hex_id(4),
            kind: OperationKind::Purge,
            actor_user_id: None,
            reason: None,
        },
    )
    .await
    .expect("heavy job allowed after previous finished");
    assert_eq!(j4.kind, OperationKind::Purge);

    // Startup recovery marks queued/running jobs (media_job + j4) as interrupted
    let interrupted_count = mark_interrupted_on_startup(&db)
        .await
        .expect("mark interrupted");
    assert_eq!(interrupted_count, 2);
    assert_eq!(
        get_job(&db, &media_job.id).await.unwrap().unwrap().state,
        OperationState::Interrupted
    );

    // Cap at 200 receipts
    for idx in 10..225 {
        let job = create_job(
            &db,
            JobCreateInput {
                id: hex_id(idx),
                kind: OperationKind::MediaDelete,
                actor_user_id: None,
                reason: None,
            },
        )
        .await
        .expect("create job for cap test");
        transition_job_cas(
            &db,
            &job.id,
            JobTransitionInput {
                expected_version: 1,
                next_state: OperationState::Succeeded,
                summary_json: None,
                error_code: None,
                error_message: None,
            },
        )
        .await
        .expect("finish cap job");
    }

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM operation_jobs")
        .fetch_one(&db)
        .await
        .expect("count operation_jobs");
    assert!(total <= MAX_RETAINED_OPERATION_RECEIPTS);
    assert_eq!(list_jobs(&db, 500).await.unwrap().len() as i64, total);

    // Maintenance generation increments monotonically
    assert_eq!(get_maintenance_generation(&db).await.unwrap(), 0);
    assert_eq!(
        bump_maintenance_generation(&db, OperationKind::Backup)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        bump_maintenance_generation(&db, OperationKind::Restore)
            .await
            .unwrap(),
        2
    );

    db.close().await;
    let _ = std::fs::remove_dir_all(dir);
}
