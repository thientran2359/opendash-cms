-- Public Site Search (S01, 0058): SQLite FTS5 contentless_delete=1 index + template_overrides ('search') + operation_jobs ('search_reindex').
-- Adds +5 net sqlite_schema objects (entry_search_fts and its 4 shadow tables: _data, _idx, _config, _docsize).
-- Rebuilds template_overrides (+ template_override_media_refs) and operation_jobs cleanly without PRAGMA foreign_keys=OFF.

CREATE VIRTUAL TABLE entry_search_fts USING fts5(
    title,
    summary,
    body,
    terms,
    tokenize = 'unicode61 remove_diacritics 2',
    content = '',
    contentless_delete = 1
);

-- Copy-and-rebuild template_overrides + template_override_media_refs to allow target_kind = 'search' (+0 net objects).
CREATE TABLE template_overrides_0058_copy AS SELECT * FROM template_overrides;
CREATE TABLE template_override_media_refs_0058_copy AS SELECT * FROM template_override_media_refs;

DROP TABLE template_override_media_refs;
DROP TABLE template_overrides;

CREATE TABLE template_overrides (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    target_kind TEXT NOT NULL CHECK (target_kind IN (
        'home', 'page', 'page_layout', 'single', 'single_custom', 'archive',
        'taxonomy', 'singular', 'index', 'not_found', 'search', 'part'
    )),
    canonical_target_key TEXT NOT NULL UNIQUE,
    target_json TEXT NOT NULL CHECK (json_valid(target_json)),
    ast_json TEXT NOT NULL CHECK (json_valid(ast_json)),
    base_theme_id TEXT NULL,
    base_revision_digest TEXT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

INSERT INTO template_overrides (
    id,
    target_kind,
    canonical_target_key,
    target_json,
    ast_json,
    base_theme_id,
    base_revision_digest,
    version,
    updated_at
)
SELECT
    id,
    target_kind,
    canonical_target_key,
    target_json,
    ast_json,
    base_theme_id,
    base_revision_digest,
    version,
    updated_at
FROM template_overrides_0058_copy;

CREATE INDEX idx_template_overrides_kind ON template_overrides (target_kind);

CREATE TABLE template_override_media_refs (
    override_id INTEGER NOT NULL REFERENCES template_overrides(id) ON DELETE CASCADE,
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
    ref_role TEXT NOT NULL CHECK (ref_role IN ('image', 'video', 'poster')),
    PRIMARY KEY (override_id, media_id, ref_role)
);

INSERT INTO template_override_media_refs (override_id, media_id, ref_role)
SELECT override_id, media_id, ref_role
FROM template_override_media_refs_0058_copy;

CREATE INDEX idx_template_override_media_refs_media
    ON template_override_media_refs (media_id);

DROP TABLE template_override_media_refs_0058_copy;
DROP TABLE template_overrides_0058_copy;

-- Copy-and-rebuild operation_jobs to allow kind = 'search_reindex' in CHECK and single-active-heavy partial index (+0 net objects).
CREATE TABLE operation_jobs_0058_copy AS SELECT * FROM operation_jobs;

DROP TABLE operation_jobs;

CREATE TABLE operation_jobs (
    id TEXT PRIMARY KEY CHECK (length(id) = 64),
    kind TEXT NOT NULL CHECK (kind IN ('backup', 'restore', 'reconcile', 'purge', 'media_delete', 'search_reindex')),
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

INSERT INTO operation_jobs (
    id,
    kind,
    state,
    version,
    actor_user_id,
    reason,
    summary_json,
    error_code,
    error_message,
    requested_at,
    started_at,
    finished_at,
    updated_at
)
SELECT
    id,
    kind,
    state,
    version,
    actor_user_id,
    reason,
    summary_json,
    error_code,
    error_message,
    requested_at,
    started_at,
    finished_at,
    updated_at
FROM operation_jobs_0058_copy;

CREATE INDEX idx_operation_jobs_state_requested
    ON operation_jobs (state, requested_at DESC, id DESC);

CREATE INDEX idx_operation_jobs_kind_requested
    ON operation_jobs (kind, requested_at DESC, id DESC);

CREATE UNIQUE INDEX idx_operation_jobs_single_active_heavy
    ON operation_jobs ((1))
    WHERE kind IN ('backup', 'restore', 'reconcile', 'purge', 'search_reindex')
      AND state IN ('queued', 'running');

DROP TABLE operation_jobs_0058_copy;
