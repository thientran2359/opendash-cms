-- 0053_theme_functions_runtime.sql
-- Rebuild 6 bang mo rong theo owner_kind (plugin | theme | theme_functions)
-- voi tong delta dung +0 net schema objects (giu tong objects < 256).

-- 1. Rebuild plugin_kv (bo FK plugins(id), PK theo (owner_kind, plugin_id, key)) (+0)
CREATE TABLE plugin_kv_0053 (
    owner_kind TEXT NOT NULL DEFAULT 'plugin' CHECK (owner_kind IN ('plugin', 'theme')),
    plugin_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (owner_kind, plugin_id, key)
);
INSERT INTO plugin_kv_0053 (owner_kind, plugin_id, key, value_json, updated_at)
SELECT 'plugin', plugin_id, key, value_json, updated_at
  FROM plugin_kv;
DROP TABLE plugin_kv;
ALTER TABLE plugin_kv_0053 RENAME TO plugin_kv;

-- 2. Rebuild plugin_entry_projections (bo FK plugins(id), giu FK content_entries(id), PK (entry_id, owner_kind, plugin_id)) (+0)
CREATE TABLE plugin_entry_projections_0053 (
    entry_id INTEGER NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
    owner_kind TEXT NOT NULL DEFAULT 'plugin' CHECK (owner_kind IN ('plugin', 'theme')),
    plugin_id TEXT NOT NULL,
    locale TEXT NOT NULL DEFAULT 'vi',
    seo_json TEXT,
    ext_json TEXT,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (entry_id, owner_kind, plugin_id)
);
INSERT INTO plugin_entry_projections_0053 (entry_id, owner_kind, plugin_id, locale, seo_json, ext_json, updated_at)
SELECT entry_id, 'plugin', plugin_id, locale, seo_json, ext_json, updated_at
  FROM plugin_entry_projections;
DROP TABLE plugin_entry_projections;
ALTER TABLE plugin_entry_projections_0053 RENAME TO plugin_entry_projections;
CREATE INDEX idx_plugin_entry_projections_plugin ON plugin_entry_projections (owner_kind, plugin_id, locale);

-- 3. Rebuild plugin_logs theo owner_kind (+0)
CREATE TABLE plugin_logs_0053 (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    owner_kind TEXT NOT NULL DEFAULT 'plugin' CHECK (owner_kind IN ('plugin', 'theme')),
    plugin_id TEXT NOT NULL,
    correlation_id TEXT NOT NULL,
    level TEXT NOT NULL,
    message TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO plugin_logs_0053 (id, owner_kind, plugin_id, correlation_id, level, message, created_at)
SELECT id, 'plugin', plugin_id, correlation_id, level, message, created_at
  FROM plugin_logs;
DROP TABLE plugin_logs;
ALTER TABLE plugin_logs_0053 RENAME TO plugin_logs;

-- 4. Rebuild theme_option_values mo rong CHECK owner_kind them 'theme_functions' + tai tao index (+0)
CREATE TABLE theme_option_values_0053 (
    theme_id TEXT NOT NULL,
    owner_kind TEXT NOT NULL DEFAULT 'theme' CHECK (owner_kind IN ('theme', 'plugin', 'theme_functions')),
    owner_id TEXT NOT NULL,
    option_key TEXT NOT NULL,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (owner_kind, owner_id, option_key)
);
INSERT INTO theme_option_values_0053 (theme_id, owner_kind, owner_id, option_key, value_json, updated_at)
SELECT theme_id, owner_kind, owner_id, option_key, value_json, updated_at
  FROM theme_option_values;
DROP TABLE theme_option_values;
ALTER TABLE theme_option_values_0053 RENAME TO theme_option_values;
CREATE INDEX idx_theme_option_values_owner ON theme_option_values (owner_kind, owner_id);

-- 5. Rebuild theme_option_versions mo rong CHECK owner_kind them 'theme_functions' (+0)
CREATE TABLE theme_option_versions_0053 (
    theme_id TEXT NOT NULL,
    owner_kind TEXT NOT NULL DEFAULT 'theme' CHECK (owner_kind IN ('theme', 'plugin', 'theme_functions')),
    owner_id TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (owner_kind, owner_id)
);
INSERT INTO theme_option_versions_0053 (theme_id, owner_kind, owner_id, version, updated_at)
SELECT theme_id, owner_kind, owner_id, version, updated_at
  FROM theme_option_versions;
DROP TABLE theme_option_versions;
ALTER TABLE theme_option_versions_0053 RENAME TO theme_option_versions;

-- 6. Rebuild theme_option_media_refs mo rong CHECK owner_kind them 'theme_functions' (+0)
CREATE TABLE theme_option_media_refs_0053 (
    theme_id TEXT NOT NULL,
    owner_kind TEXT NOT NULL DEFAULT 'theme' CHECK (owner_kind IN ('theme', 'plugin', 'theme_functions')),
    owner_id TEXT NOT NULL,
    option_key TEXT NOT NULL,
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
    PRIMARY KEY (owner_kind, owner_id, option_key, media_id)
);
INSERT INTO theme_option_media_refs_0053 (theme_id, owner_kind, owner_id, option_key, media_id)
SELECT theme_id, owner_kind, owner_id, option_key, media_id
  FROM theme_option_media_refs;
DROP TABLE theme_option_media_refs;
ALTER TABLE theme_option_media_refs_0053 RENAME TO theme_option_media_refs;
