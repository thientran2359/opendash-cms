-- S02: reading settings deliberately keep deleted page IDs so a removed or
-- unpublished assignment remains distinct from an unconfigured home/posts page.
CREATE TABLE site_reading_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    homepage_page_id INTEGER NULL,
    posts_page_id INTEGER NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (homepage_page_id IS NULL OR homepage_page_id > 0),
    CHECK (posts_page_id IS NULL OR posts_page_id > 0),
    CHECK (homepage_page_id IS NULL OR posts_page_id IS NULL OR homepage_page_id != posts_page_id)
);
INSERT INTO site_reading_settings (id, homepage_page_id, posts_page_id, version)
VALUES (1, NULL, NULL, 1);

-- Assignments stay with entry data across theme changes and missing resources.
ALTER TABLE content_entries ADD COLUMN page_layout_key TEXT NULL;
ALTER TABLE content_entries ADD COLUMN custom_template_key TEXT NULL;

-- D02: Page content is entry-owned, independent of the shared template.
ALTER TABLE content_entries ADD COLUMN content_mode TEXT NOT NULL DEFAULT 'editor'
    CHECK (content_mode IN ('editor', 'builder'));
CREATE TABLE entry_builder_documents (
    entry_id INTEGER PRIMARY KEY REFERENCES content_entries(id) ON DELETE CASCADE,
    document_json TEXT NOT NULL CHECK (json_valid(document_json)),
    updated_at TEXT NOT NULL
);
CREATE TABLE entry_builder_media_refs (
    entry_id INTEGER NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
    ref_role TEXT NOT NULL CHECK (ref_role IN ('image', 'video', 'poster')),
    PRIMARY KEY (entry_id, media_id, ref_role)
);
CREATE INDEX idx_entry_builder_media_refs_media ON entry_builder_media_refs(media_id);

-- Owner selections are metadata, never a candidate for public site rendering.
CREATE TABLE owner_template_selections (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    owner_kind TEXT NOT NULL CHECK (owner_kind IN ('theme','plugin')),
    owner_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('page','header','footer','archive','single','taxonomy_archive')),
    scope_type_key TEXT NOT NULL DEFAULT '',
    template_selection_key TEXT NULL,
    ast_json TEXT NOT NULL CHECK (json_valid(ast_json)),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    updated_at TEXT NOT NULL,
    UNIQUE (owner_kind, owner_id, role, scope_type_key)
);
CREATE INDEX idx_owner_template_selections_owner
    ON owner_template_selections(owner_kind, owner_id);

-- Fresh target databases get the Page-only capability; Post stays unchanged.
UPDATE content_types
   SET definition_json = json_set(
       definition_json, '$.settings.supports',
       json_insert(json_extract(definition_json, '$.settings.supports'), '$[#]', 'builder')
   )
 WHERE key = 'page' AND owner_kind = 'core'
   AND NOT EXISTS (
       SELECT 1 FROM json_each(definition_json, '$.settings.supports') WHERE value = 'builder'
   );

-- The approved S02 baseline has no production overrides to convert. Site-owned
-- templates are typed Builder ASTs; identity is a canonical key, not the old
-- role/scope tuple. Recreate projection refs with their media safety contract.
DROP TABLE IF EXISTS template_override_media_refs;
DROP TABLE IF EXISTS template_overrides;
CREATE TABLE template_overrides (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    target_kind TEXT NOT NULL CHECK (target_kind IN (
        'home', 'page', 'page_layout', 'single', 'single_custom', 'archive',
        'taxonomy', 'singular', 'index', 'not_found', 'part'
    )),
    canonical_target_key TEXT NOT NULL UNIQUE,
    target_json TEXT NOT NULL CHECK (json_valid(target_json)),
    ast_json TEXT NOT NULL CHECK (json_valid(ast_json)),
    base_theme_id TEXT NULL,
    base_revision_digest TEXT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX idx_template_overrides_kind ON template_overrides (target_kind);

CREATE TABLE template_override_media_refs (
    override_id INTEGER NOT NULL REFERENCES template_overrides(id) ON DELETE CASCADE,
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
    ref_role TEXT NOT NULL CHECK (ref_role IN ('image', 'video', 'poster')),
    PRIMARY KEY (override_id, media_id, ref_role)
);
CREATE INDEX idx_template_override_media_refs_media
    ON template_override_media_refs (media_id);

-- v3 is a durable revision source format (not a render-time alias). Migration
-- 0015 constrained packages/revisions to v1_legacy or v2. SQLite cannot ALTER
-- a CHECK constraint in place. Rebuild this entire, bounded FK dependency set
-- in the same migrator transaction. Copy each source row before dropping any
-- table; preserve active-selection CAS, legacy mappings and revision digests.
-- No PRAGMA foreign_keys=OFF or writable_schema manipulation is required.
CREATE TABLE theme_packages_0042_copy AS SELECT * FROM theme_packages;
CREATE TABLE theme_revisions_0042_copy AS SELECT * FROM theme_revisions;
CREATE TABLE theme_legacy_mappings_0042_copy AS SELECT * FROM theme_legacy_mappings;
CREATE TABLE theme_active_selection_0042_copy AS SELECT * FROM theme_active_selection;

-- Drop FK children first, then parents; recreate parents first, then children.
DROP TABLE theme_active_selection;
DROP TABLE theme_legacy_mappings;
DROP TABLE theme_revisions;
DROP TABLE theme_packages;

CREATE TABLE theme_packages (
    theme_id TEXT PRIMARY KEY NOT NULL,
    display_name TEXT NOT NULL,
    source_format TEXT NOT NULL CHECK (source_format IN ('v1_legacy', 'v2', 'v3')),
    is_legacy INTEGER NOT NULL DEFAULT 0 CHECK (is_legacy IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO theme_packages (theme_id, display_name, source_format, is_legacy, created_at)
SELECT theme_id, display_name, source_format, is_legacy, created_at
  FROM theme_packages_0042_copy;

CREATE TABLE theme_revisions (
    revision_id TEXT PRIMARY KEY NOT NULL,
    theme_id TEXT NOT NULL REFERENCES theme_packages(theme_id) ON DELETE RESTRICT,
    version TEXT NOT NULL,
    revision_digest TEXT NOT NULL UNIQUE,
    source_format TEXT NOT NULL CHECK (source_format IN ('v1_legacy', 'v2', 'v3')),
    manifest_json TEXT NOT NULL,
    file_digests_json TEXT NOT NULL DEFAULT '{}',
    legacy_block_id TEXT REFERENCES theme_blocks(id) ON DELETE RESTRICT,
    provenance_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (theme_id, version)
);
CREATE INDEX idx_theme_revisions_theme_id ON theme_revisions(theme_id);
INSERT INTO theme_revisions (
    revision_id, theme_id, version, revision_digest, source_format, manifest_json,
    file_digests_json, legacy_block_id, provenance_json, created_at
)
SELECT revision_id, theme_id, version, revision_digest, source_format, manifest_json,
       file_digests_json, legacy_block_id, provenance_json, created_at
  FROM theme_revisions_0042_copy;

CREATE TABLE theme_legacy_mappings (
    block_id TEXT PRIMARY KEY NOT NULL REFERENCES theme_blocks(id) ON DELETE RESTRICT,
    theme_id TEXT NOT NULL UNIQUE REFERENCES theme_packages(theme_id) ON DELETE RESTRICT,
    revision_id TEXT NOT NULL UNIQUE REFERENCES theme_revisions(revision_id) ON DELETE RESTRICT
);
INSERT INTO theme_legacy_mappings (block_id, theme_id, revision_id)
SELECT block_id, theme_id, revision_id FROM theme_legacy_mappings_0042_copy;

CREATE TABLE theme_active_selection (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    revision_id TEXT REFERENCES theme_revisions(revision_id) ON DELETE RESTRICT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1)
);
INSERT INTO theme_active_selection (id, revision_id, version)
SELECT id, revision_id, version FROM theme_active_selection_0042_copy;

DROP TABLE theme_active_selection_0042_copy;
DROP TABLE theme_legacy_mappings_0042_copy;
DROP TABLE theme_revisions_0042_copy;
DROP TABLE theme_packages_0042_copy;
