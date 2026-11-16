-- M6-S01-T01: Plugin package revisions, monotonic revision_generation, and CAS version.
ALTER TABLE plugins ADD COLUMN active_revision_id INTEGER;
ALTER TABLE plugins ADD COLUMN revision_generation INTEGER NOT NULL DEFAULT 1;
ALTER TABLE plugins ADD COLUMN cas_version INTEGER NOT NULL DEFAULT 1;

CREATE TABLE IF NOT EXISTS plugin_revisions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    plugin_id TEXT NOT NULL,
    version TEXT NOT NULL,
    revision_digest TEXT NOT NULL,
    manifest_schema INTEGER NOT NULL DEFAULT 1,
    manifest_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(plugin_id, version),
    UNIQUE(plugin_id, revision_digest),
    FOREIGN KEY (plugin_id) REFERENCES plugins(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_plugin_revisions_plugin_id
    ON plugin_revisions(plugin_id, id DESC);
