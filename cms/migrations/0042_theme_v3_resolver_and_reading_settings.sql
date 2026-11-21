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
