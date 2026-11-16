CREATE TABLE IF NOT EXISTS site_smtp_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    host TEXT NOT NULL DEFAULT '',
    port INTEGER NOT NULL DEFAULT 587 CHECK (port BETWEEN 1 AND 65535),
    tls_mode TEXT NOT NULL DEFAULT 'starttls' CHECK (tls_mode IN ('starttls', 'implicit_tls')),
    username TEXT NOT NULL DEFAULT '',
    secret_ref TEXT NULL,
    from_address TEXT NOT NULL DEFAULT '',
    recipient_address TEXT NOT NULL DEFAULT '',
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT OR IGNORE INTO site_smtp_settings (id) VALUES (1);

CREATE TABLE IF NOT EXISTS contact_notification_attempts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    submission_id INTEGER NOT NULL REFERENCES contact_submissions(id) ON DELETE CASCADE,
    attempt_number INTEGER NOT NULL CHECK (attempt_number BETWEEN 1 AND 3),
    state TEXT NOT NULL CHECK (state IN ('pending', 'sending', 'sent', 'failed', 'unknown', 'disabled')),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    transport_snapshot_json TEXT NOT NULL,
    started_at TEXT NULL,
    finished_at TEXT NULL,
    diagnostic_code TEXT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(submission_id, attempt_number)
);

CREATE INDEX IF NOT EXISTS idx_contact_notification_attempts_sub
    ON contact_notification_attempts(submission_id, attempt_number);
