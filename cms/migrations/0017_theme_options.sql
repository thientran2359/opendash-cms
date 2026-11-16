-- M5-S04-T01: Theme Options (typed schema values + transactional media ACL refs)
CREATE TABLE IF NOT EXISTS theme_option_values (
    theme_id TEXT NOT NULL,
    option_key TEXT NOT NULL,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (theme_id, option_key)
);

CREATE TABLE IF NOT EXISTS theme_option_versions (
    theme_id TEXT PRIMARY KEY,
    version INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS theme_option_media_refs (
    theme_id TEXT NOT NULL,
    option_key TEXT NOT NULL,
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
    PRIMARY KEY (theme_id, option_key)
);
