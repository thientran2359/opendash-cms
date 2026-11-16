CREATE TABLE IF NOT EXISTS contact_site_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    rate_salt TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS contact_forms (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    slug TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled')),
    schema_json TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS contact_submissions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    form_id INTEGER NOT NULL REFERENCES contact_forms(id) ON DELETE RESTRICT,
    form_version INTEGER NOT NULL,
    schema_snapshot_json TEXT NOT NULL,
    values_json TEXT NOT NULL,
    reply_to_email TEXT,
    notification_state TEXT NOT NULL DEFAULT 'disabled'
        CHECK (notification_state IN ('pending', 'sending', 'sent', 'failed', 'unknown', 'disabled')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_contact_submissions_form_id
    ON contact_submissions(form_id, id DESC);

CREATE TABLE IF NOT EXISTS contact_submission_dedup (
    form_id INTEGER NOT NULL REFERENCES contact_forms(id) ON DELETE CASCADE,
    nonce TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    submission_id INTEGER NOT NULL REFERENCES contact_submissions(id) ON DELETE CASCADE,
    expires_at INTEGER NOT NULL,
    PRIMARY KEY (form_id, nonce)
);

CREATE INDEX IF NOT EXISTS idx_contact_submission_dedup_expires
    ON contact_submission_dedup(expires_at);

CREATE TABLE IF NOT EXISTS contact_rate_buckets (
    bucket_key TEXT PRIMARY KEY,
    count INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_contact_rate_buckets_expires
    ON contact_rate_buckets(expires_at);
