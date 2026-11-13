-- M5-S05-T01: Durable journal, logical provenance (excluding revision_id),
-- and tombstones for selective sample import (G-RECOVERY).
CREATE TABLE import_runs (
    run_id TEXT PRIMARY KEY NOT NULL CHECK (length(run_id) BETWEEN 1 AND 128),
    theme_id TEXT NOT NULL CHECK (length(theme_id) BETWEEN 1 AND 64),
    sample_key TEXT NOT NULL CHECK (length(sample_key) BETWEEN 1 AND 64),
    revision_id TEXT NOT NULL CHECK (length(revision_id) BETWEEN 1 AND 128),
    plan_digest TEXT NOT NULL CHECK (length(plan_digest) = 64),
    state TEXT NOT NULL CHECK (
        state IN ('staged', 'files_verified', 'committed', 'failed', 'quarantined')
    ),
    media_manifest_json TEXT NOT NULL DEFAULT '[]'
        CHECK (json_valid(media_manifest_json) AND json_type(media_manifest_json) = 'array'),
    outcome_json TEXT
        CHECK (outcome_json IS NULL OR (json_valid(outcome_json) AND json_type(outcome_json) = 'object')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_import_runs_state_updated
    ON import_runs (state, updated_at);

CREATE TABLE import_provenance (
    id INTEGER PRIMARY KEY AUTOINCREMENT CHECK (id > 0),
    theme_id TEXT NOT NULL CHECK (length(theme_id) BETWEEN 1 AND 64),
    sample_key TEXT NOT NULL CHECK (length(sample_key) BETWEEN 1 AND 64),
    resource_kind TEXT NOT NULL CHECK (resource_kind IN ('pages', 'menus', 'media', 'settings')),
    object_key TEXT NOT NULL CHECK (length(object_key) BETWEEN 1 AND 128),
    revision_id TEXT NOT NULL CHECK (length(revision_id) BETWEEN 1 AND 128),
    revision_digest TEXT NOT NULL CHECK (length(revision_digest) = 64),
    target_id TEXT NOT NULL CHECK (length(target_id) BETWEEN 1 AND 128),
    target_hash TEXT NOT NULL CHECK (length(target_hash) BETWEEN 1 AND 128),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (theme_id, sample_key, resource_kind, object_key)
);

CREATE TABLE import_tombstones (
    theme_id TEXT NOT NULL CHECK (length(theme_id) BETWEEN 1 AND 64),
    sample_key TEXT NOT NULL CHECK (length(sample_key) BETWEEN 1 AND 64),
    resource_kind TEXT NOT NULL CHECK (resource_kind IN ('pages', 'menus', 'media', 'settings')),
    object_key TEXT NOT NULL CHECK (length(object_key) BETWEEN 1 AND 128),
    deleted_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (theme_id, sample_key, resource_kind, object_key)
);
