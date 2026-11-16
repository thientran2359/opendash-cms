-- M5-S01-T01: Theme packages, immutable revisions, legacy block mapping,
-- and singleton active selection pointer. Preserves 0011 NULL/explicit state.

CREATE TABLE theme_packages (
    theme_id TEXT PRIMARY KEY NOT NULL,
    display_name TEXT NOT NULL,
    source_format TEXT NOT NULL CHECK (source_format IN ('v1_legacy', 'v2')),
    is_legacy INTEGER NOT NULL DEFAULT 0 CHECK (is_legacy IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE theme_revisions (
    revision_id TEXT PRIMARY KEY NOT NULL,
    theme_id TEXT NOT NULL REFERENCES theme_packages(theme_id) ON DELETE RESTRICT,
    version TEXT NOT NULL,
    revision_digest TEXT NOT NULL UNIQUE,
    source_format TEXT NOT NULL CHECK (source_format IN ('v1_legacy', 'v2')),
    manifest_json TEXT NOT NULL,
    file_digests_json TEXT NOT NULL DEFAULT '{}',
    legacy_block_id TEXT REFERENCES theme_blocks(id) ON DELETE RESTRICT,
    provenance_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (theme_id, version)
);

CREATE INDEX idx_theme_revisions_theme_id ON theme_revisions(theme_id);

CREATE TABLE theme_legacy_mappings (
    block_id TEXT PRIMARY KEY NOT NULL REFERENCES theme_blocks(id) ON DELETE RESTRICT,
    theme_id TEXT NOT NULL UNIQUE REFERENCES theme_packages(theme_id) ON DELETE RESTRICT,
    revision_id TEXT NOT NULL UNIQUE REFERENCES theme_revisions(revision_id) ON DELETE RESTRICT
);

CREATE TABLE theme_active_selection (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    revision_id TEXT REFERENCES theme_revisions(revision_id) ON DELETE RESTRICT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1)
);

-- Adapt each distinct legacy full block ID to its own reserved legacy identity.
INSERT INTO theme_packages (theme_id, display_name, source_format, is_legacy)
SELECT
    'legacy-v1-' || lower(hex(cast(id AS BLOB))),
    id,
    'v1_legacy',
    1
FROM theme_blocks;

INSERT INTO theme_revisions (
    revision_id,
    theme_id,
    version,
    revision_digest,
    source_format,
    manifest_json,
    file_digests_json,
    legacy_block_id,
    provenance_json
)
SELECT
    'rev-legacy-v1-' || lower(hex(cast(id AS BLOB))),
    'legacy-v1-' || lower(hex(cast(id AS BLOB))),
    '1.0.0-legacy',
    'legacy-v1-' || lower(hex(cast(id AS BLOB))),
    'v1_legacy',
    json_object(
        'legacy_block_id', id,
        'compatible_block_id', compatible_block_id,
        'fields_json', fields_json
    ),
    '{}',
    id,
    json_object('migrated_from', '0011_theme_activation', 'block_id', id)
FROM theme_blocks;

INSERT INTO theme_legacy_mappings (block_id, theme_id, revision_id)
SELECT
    id,
    'legacy-v1-' || lower(hex(cast(id AS BLOB))),
    'rev-legacy-v1-' || lower(hex(cast(id AS BLOB)))
FROM theme_blocks;

INSERT INTO theme_active_selection (id, revision_id, version)
SELECT
    1,
    CASE
        WHEN a.block_id IS NULL THEN NULL
        ELSE (SELECT m.revision_id FROM theme_legacy_mappings m WHERE m.block_id = a.block_id)
    END,
    a.version
FROM site_active_theme a
WHERE a.id = 1;
