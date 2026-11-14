-- M8-S01-T01: Operation coordinator job ledger and maintenance generation

CREATE TABLE IF NOT EXISTS operation_jobs (
    id TEXT PRIMARY KEY CHECK (length(id) = 64),
    kind TEXT NOT NULL CHECK (kind IN ('backup', 'restore', 'reconcile', 'purge', 'media_delete')),
    state TEXT NOT NULL CHECK (state IN ('queued', 'running', 'succeeded', 'failed', 'cancelled', 'interrupted')),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    actor_user_id TEXT,
    reason TEXT,
    summary_json TEXT,
    error_code TEXT,
    error_message TEXT,
    requested_at TEXT NOT NULL DEFAULT (datetime('now')),
    started_at TEXT,
    finished_at TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_operation_jobs_state_requested
    ON operation_jobs (state, requested_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_operation_jobs_kind_requested
    ON operation_jobs (kind, requested_at DESC, id DESC);

CREATE UNIQUE INDEX IF NOT EXISTS idx_operation_jobs_single_active_heavy
    ON operation_jobs ((1))
    WHERE kind IN ('backup', 'restore', 'reconcile', 'purge')
      AND state IN ('queued', 'running');

CREATE TABLE IF NOT EXISTS maintenance_generation (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    generation INTEGER NOT NULL DEFAULT 0 CHECK (generation >= 0),
    last_kind TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT OR IGNORE INTO maintenance_generation (id, generation, last_kind)
VALUES (1, 0, NULL);
