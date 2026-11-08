-- CPT definitions and their custom-field schemas survive provider deactivation.
-- Deliberately no foreign key to plugins/themes: removing a provider must not
-- cascade into deleting schemas or content managed by the site owner.
CREATE TABLE content_types (
    key TEXT PRIMARY KEY NOT NULL COLLATE BINARY CHECK (length(key) BETWEEN 1 AND 64),
    definition_json TEXT NOT NULL
        CHECK (json_valid(definition_json) AND json_type(definition_json) = 'object'),
    owner_kind TEXT NOT NULL CHECK (owner_kind IN ('core', 'user', 'theme', 'plugin')),
    owner_id TEXT,
    provider_status TEXT NOT NULL DEFAULT 'active'
        CHECK (provider_status IN ('active', 'provider_inactive')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (
        (owner_kind IN ('core', 'user') AND owner_id IS NULL)
        OR (owner_kind IN ('theme', 'plugin') AND owner_id IS NOT NULL AND length(owner_id) > 0)
    )
);
