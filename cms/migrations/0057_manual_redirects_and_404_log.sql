-- Forward-only migration for Manual Redirects (301/302) & aggregated 404 Log.
-- Adds +6 sqlite_schema objects total (+2 manual_redirects, +4 not_found_logs).

CREATE TABLE manual_redirects (
    id INTEGER PRIMARY KEY,
    source_path TEXT NOT NULL COLLATE BINARY UNIQUE,
    target_url TEXT NOT NULL,
    status_code INTEGER NOT NULL DEFAULT 301 CHECK (status_code IN (301, 302)),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    note TEXT NOT NULL DEFAULT '',
    hit_count INTEGER NOT NULL DEFAULT 0 CHECK (hit_count >= 0),
    last_hit_at TEXT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE not_found_logs (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL COLLATE BINARY UNIQUE,
    hit_count INTEGER NOT NULL DEFAULT 1 CHECK (hit_count >= 1),
    first_seen_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    last_seen_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    last_referrer TEXT,
    last_user_agent TEXT
);

CREATE INDEX idx_not_found_logs_last_seen
    ON not_found_logs (last_seen_at DESC, id DESC);

CREATE INDEX idx_not_found_logs_hit_count
    ON not_found_logs (hit_count DESC, last_seen_at DESC);
