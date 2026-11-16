-- M7-S03-T01 canonical entry paths, ID-based redirects, SEO sidecar tables, and media refs.
-- No blanket alias migration: automatic redirect history starts with M7 changes.

CREATE TABLE IF NOT EXISTS entry_public_paths (
    entry_id INTEGER PRIMARY KEY REFERENCES content_entries(id) ON DELETE CASCADE,
    path TEXT UNIQUE NOT NULL
);

CREATE TABLE IF NOT EXISTS entry_redirects (
    source_path TEXT PRIMARY KEY NOT NULL,
    entry_id INTEGER NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_entry_redirects_entry_id
    ON entry_redirects (entry_id);

CREATE TABLE IF NOT EXISTS entry_seo (
    entry_id INTEGER PRIMARY KEY REFERENCES content_entries(id) ON DELETE CASCADE,
    title TEXT,
    description TEXT,
    og_image_id TEXT REFERENCES media_assets(id) ON DELETE SET NULL,
    canonical_url TEXT,
    noindex INTEGER NOT NULL DEFAULT 0 CHECK (noindex IN (0, 1))
);

CREATE TABLE IF NOT EXISTS site_home_seo (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    title TEXT,
    description TEXT,
    og_image_id TEXT REFERENCES media_assets(id) ON DELETE SET NULL,
    canonical_url TEXT,
    noindex INTEGER NOT NULL DEFAULT 0 CHECK (noindex IN (0, 1)),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1)
);

INSERT OR IGNORE INTO site_home_seo (id, title, description, og_image_id, canonical_url, noindex, version)
VALUES (1, NULL, NULL, NULL, NULL, 0, 1);

CREATE TABLE IF NOT EXISTS seo_public_media_refs (
    source_kind TEXT NOT NULL CHECK (source_kind IN ('entry_seo', 'home_seo')),
    source_id INTEGER NOT NULL,
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE CASCADE,
    PRIMARY KEY (source_kind, source_id, media_id)
);

CREATE INDEX IF NOT EXISTS idx_seo_public_media_refs_media_id
    ON seo_public_media_refs (media_id);
