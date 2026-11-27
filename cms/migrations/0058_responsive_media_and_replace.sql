-- Responsive media variants, lossy WebP optimization, and atomic in-place replace journal

ALTER TABLE media_assets
    ADD COLUMN content_sha256 TEXT CHECK (content_sha256 IS NULL OR (length(content_sha256) = 64 AND content_sha256 GLOB '[0-9a-f]*'));

ALTER TABLE media_assets
    ADD COLUMN variants_ready_version INTEGER NOT NULL DEFAULT 0 CHECK (variants_ready_version >= 0);

ALTER TABLE media_assets
    ADD COLUMN updated_at TEXT;

UPDATE media_assets
SET content_sha256 = id
WHERE content_sha256 IS NULL;

UPDATE media_assets
SET updated_at = created_at
WHERE updated_at IS NULL;

CREATE INDEX idx_media_assets_content_sha256
    ON media_assets (content_sha256, lifecycle_state);

CREATE INDEX idx_media_assets_variants_pending
    ON media_assets (id)
    WHERE kind = 'image'
      AND lifecycle_state = 'active'
      AND variants_ready_version < version;

CREATE TABLE media_variants (
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE CASCADE,
    version INTEGER NOT NULL CHECK (version >= 1),
    spec TEXT NOT NULL CHECK (spec IN ('thumbnail', 'medium', 'large', 'full')),
    format TEXT NOT NULL CHECK (format IN ('jpeg', 'png', 'webp')),
    width INTEGER NOT NULL CHECK (width BETWEEN 1 AND 6000),
    height INTEGER NOT NULL CHECK (height BETWEEN 1 AND 6000),
    byte_size INTEGER NOT NULL CHECK (byte_size > 0),
    relative_path TEXT NOT NULL,
    sha256 TEXT NOT NULL CHECK (length(sha256) = 64 AND sha256 GLOB '[0-9a-f]*'),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (media_id, version, spec, format)
) STRICT, WITHOUT ROWID;

CREATE TABLE media_replace_jobs (
    job_id TEXT PRIMARY KEY CHECK (length(job_id) = 64 AND job_id GLOB '[0-9a-f]*'),
    media_id TEXT NOT NULL CHECK (length(media_id) = 64 AND media_id GLOB '[0-9a-f]*'),
    expected_generation INTEGER NOT NULL CHECK (expected_generation >= 1),
    expected_version INTEGER NOT NULL CHECK (expected_version >= 1),
    new_version INTEGER NOT NULL CHECK (new_version = expected_version + 1),
    old_content_sha256 TEXT NOT NULL CHECK (length(old_content_sha256) = 64 AND old_content_sha256 GLOB '[0-9a-f]*'),
    new_content_sha256 TEXT NOT NULL CHECK (length(new_content_sha256) = 64 AND new_content_sha256 GLOB '[0-9a-f]*'),
    new_kind TEXT NOT NULL CHECK (new_kind IN ('image', 'video')),
    new_filename TEXT NOT NULL CHECK (length(new_filename) BETWEEN 1 AND 255),
    new_mime_type TEXT NOT NULL,
    new_extension TEXT NOT NULL,
    new_byte_size INTEGER NOT NULL,
    new_width INTEGER NOT NULL,
    new_height INTEGER NOT NULL,
    new_thumb_width INTEGER,
    new_thumb_height INTEGER,
    new_duration_ms INTEGER,
    new_video_codec TEXT,
    new_audio_codec TEXT,
    new_variants_json TEXT NOT NULL,
    old_canonical_rel_path TEXT NOT NULL,
    old_thumb_rel_path TEXT,
    staging_rel_dir TEXT NOT NULL,
    quarantine_rel_dir TEXT NOT NULL,
    phase TEXT NOT NULL CHECK (phase IN ('staged', 'moving_old', 'promoting_new', 'completed', 'rolled_back')),
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    purge_after TEXT,
    purged_at TEXT,
    CHECK (
        (
            new_kind = 'image'
            AND new_mime_type IN ('image/jpeg', 'image/png', 'image/webp')
            AND new_extension IN ('jpg', 'png', 'webp')
            AND new_byte_size > 0 AND new_byte_size <= 8388608
            AND new_width BETWEEN 1 AND 6000
            AND new_height BETWEEN 1 AND 6000
            AND new_thumb_width IS NOT NULL AND new_thumb_width BETWEEN 1 AND 320
            AND new_thumb_height IS NOT NULL AND new_thumb_height BETWEEN 1 AND 320
            AND new_duration_ms IS NULL
            AND new_video_codec IS NULL
            AND new_audio_codec IS NULL
        )
        OR
        (
            new_kind = 'video'
            AND new_mime_type = 'video/mp4'
            AND new_extension = 'mp4'
            AND new_byte_size > 0 AND new_byte_size <= 104857600
            AND new_width BETWEEN 1 AND 3840
            AND new_height BETWEEN 1 AND 2160
            AND new_thumb_width IS NULL
            AND new_thumb_height IS NULL
            AND new_duration_ms IS NOT NULL AND new_duration_ms BETWEEN 1 AND 600000
            AND new_video_codec IS NOT NULL
        )
    )
) STRICT, WITHOUT ROWID;

CREATE UNIQUE INDEX idx_media_replace_jobs_single_active
    ON media_replace_jobs (media_id)
    WHERE phase IN ('staged', 'moving_old', 'promoting_new');

CREATE INDEX idx_media_replace_jobs_phase_updated
    ON media_replace_jobs (phase, updated_at);

CREATE INDEX idx_media_replace_jobs_purge
    ON media_replace_jobs (purged_at, purge_after)
    WHERE phase IN ('completed', 'rolled_back');

CREATE TABLE operation_jobs_new (
    id TEXT PRIMARY KEY CHECK (length(id) = 64),
    kind TEXT NOT NULL CHECK (kind IN ('backup', 'restore', 'reconcile', 'purge', 'media_delete', 'media_regenerate')),
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

INSERT INTO operation_jobs_new (
    id, kind, state, version, actor_user_id, reason, summary_json,
    error_code, error_message, requested_at, started_at, finished_at, updated_at
)
SELECT
    id, kind, state, version, actor_user_id, reason, summary_json,
    error_code, error_message, requested_at, started_at, finished_at, updated_at
FROM operation_jobs;

DROP INDEX IF EXISTS idx_operation_jobs_state_requested;
DROP INDEX IF EXISTS idx_operation_jobs_kind_requested;
DROP INDEX IF EXISTS idx_operation_jobs_single_active_heavy;
DROP TABLE operation_jobs;
ALTER TABLE operation_jobs_new RENAME TO operation_jobs;

CREATE INDEX idx_operation_jobs_state_requested
    ON operation_jobs (state, requested_at DESC, id DESC);

CREATE INDEX idx_operation_jobs_kind_requested
    ON operation_jobs (kind, requested_at DESC, id DESC);

CREATE UNIQUE INDEX idx_operation_jobs_single_active_heavy
    ON operation_jobs ((1))
    WHERE kind IN ('backup', 'restore', 'reconcile', 'purge')
      AND state IN ('queued', 'running');
