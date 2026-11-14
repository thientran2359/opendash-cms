-- M8-S02-T01: Media asset lifecycle, ABA generation tracking, and delete journal

ALTER TABLE media_assets
    ADD COLUMN lifecycle_state TEXT NOT NULL DEFAULT 'active' CHECK (lifecycle_state IN ('active', 'deleting'));

ALTER TABLE media_assets
    ADD COLUMN generation INTEGER NOT NULL DEFAULT 1 CHECK (generation >= 1);

ALTER TABLE media_assets
    ADD COLUMN version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1);

CREATE INDEX IF NOT EXISTS idx_media_assets_lifecycle_created
    ON media_assets (lifecycle_state, created_at DESC, id DESC);

CREATE TABLE IF NOT EXISTS media_generations (
    media_id TEXT PRIMARY KEY CHECK (length(media_id) = 64),
    next_generation INTEGER NOT NULL DEFAULT 1 CHECK (next_generation >= 1)
);

INSERT OR IGNORE INTO media_generations (media_id, next_generation)
SELECT id, generation + 1 FROM media_assets;

CREATE TABLE IF NOT EXISTS media_delete_jobs (
    job_id TEXT PRIMARY KEY CHECK (length(job_id) = 64),
    media_id TEXT NOT NULL CHECK (length(media_id) = 64),
    generation INTEGER NOT NULL CHECK (generation >= 1),
    expected_version INTEGER NOT NULL CHECK (expected_version >= 1),
    phase TEXT NOT NULL CHECK (phase IN ('tombstoned', 'moving', 'quarantined', 'finalized')),
    original_rel_path TEXT NOT NULL,
    thumbnail_rel_path TEXT,
    quarantine_original_rel_path TEXT NOT NULL,
    quarantine_thumbnail_rel_path TEXT,
    original_moved INTEGER NOT NULL DEFAULT 0 CHECK (original_moved IN (0, 1)),
    thumbnail_moved INTEGER NOT NULL DEFAULT 0 CHECK (thumbnail_moved IN (0, 1)),
    error_code TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_media_delete_jobs_active_media
    ON media_delete_jobs (media_id)
    WHERE phase != 'finalized';

CREATE INDEX IF NOT EXISTS idx_media_delete_jobs_phase_created
    ON media_delete_jobs (phase, created_at DESC, job_id DESC);
