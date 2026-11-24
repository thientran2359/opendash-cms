-- 0050_plugin_extensibility_and_projections.sql
-- Sua khoa chinh 3 bang theme_option_* theo (owner_kind, owner_id, ...) (+0 net schema objects)
-- va tao bang plugin_entry_projections + index cho SSR write-time projections (+2 net schema objects).

-- 1. Rebuild theme_option_values voi PRIMARY KEY (owner_kind, owner_id, option_key) (+0 net objects)
CREATE TABLE theme_option_values_0050 (
    theme_id TEXT NOT NULL,
    owner_kind TEXT NOT NULL DEFAULT 'theme' CHECK (owner_kind IN ('theme', 'plugin')),
    owner_id TEXT NOT NULL,
    option_key TEXT NOT NULL,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (owner_kind, owner_id, option_key)
);
INSERT INTO theme_option_values_0050 (theme_id, owner_kind, owner_id, option_key, value_json, updated_at)
SELECT theme_id,
       owner_kind,
       CASE WHEN owner_id = '' THEN theme_id ELSE owner_id END,
       option_key,
       value_json,
       updated_at
  FROM theme_option_values;
DROP TABLE theme_option_values;
ALTER TABLE theme_option_values_0050 RENAME TO theme_option_values;
CREATE INDEX idx_theme_option_values_owner ON theme_option_values (owner_kind, owner_id);

-- 2. Rebuild theme_option_versions voi PRIMARY KEY (owner_kind, owner_id) (+0 net objects)
CREATE TABLE theme_option_versions_0050 (
    theme_id TEXT NOT NULL,
    owner_kind TEXT NOT NULL DEFAULT 'theme' CHECK (owner_kind IN ('theme', 'plugin')),
    owner_id TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (owner_kind, owner_id)
);
INSERT INTO theme_option_versions_0050 (theme_id, owner_kind, owner_id, version, updated_at)
SELECT theme_id,
       owner_kind,
       CASE WHEN owner_id = '' THEN theme_id ELSE owner_id END,
       version,
       updated_at
  FROM theme_option_versions;
DROP TABLE theme_option_versions;
ALTER TABLE theme_option_versions_0050 RENAME TO theme_option_versions;

-- 3. Rebuild theme_option_media_refs voi PRIMARY KEY (owner_kind, owner_id, option_key, media_id) (+0 net objects)
CREATE TABLE theme_option_media_refs_0050 (
    theme_id TEXT NOT NULL,
    owner_kind TEXT NOT NULL DEFAULT 'theme' CHECK (owner_kind IN ('theme', 'plugin')),
    owner_id TEXT NOT NULL,
    option_key TEXT NOT NULL,
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
    PRIMARY KEY (owner_kind, owner_id, option_key, media_id)
);
INSERT INTO theme_option_media_refs_0050 (theme_id, owner_kind, owner_id, option_key, media_id)
SELECT theme_id,
       owner_kind,
       CASE WHEN owner_id = '' THEN theme_id ELSE owner_id END,
       option_key,
       media_id
  FROM theme_option_media_refs;
DROP TABLE theme_option_media_refs;
ALTER TABLE theme_option_media_refs_0050 RENAME TO theme_option_media_refs;

-- 4. Bang materialize write-time projections cho public SSR (+2 objects: table + index)
CREATE TABLE plugin_entry_projections (
    entry_id INTEGER NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
    plugin_id TEXT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    locale TEXT NOT NULL DEFAULT 'vi',
    seo_json TEXT,
    ext_json TEXT,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (entry_id, plugin_id)
);
CREATE INDEX idx_plugin_entry_projections_plugin ON plugin_entry_projections (plugin_id, locale);
