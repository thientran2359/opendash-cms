CREATE TABLE IF NOT EXISTS site_global_option_version (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    version INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
INSERT OR IGNORE INTO site_global_option_version (id, version) VALUES (1, 1);

CREATE TABLE IF NOT EXISTS site_global_option_values (
    option_key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS site_global_option_media_refs (
    option_key TEXT NOT NULL,
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
    PRIMARY KEY (option_key, media_id)
);
CREATE INDEX IF NOT EXISTS idx_site_global_option_media_refs_media
    ON site_global_option_media_refs(media_id);
