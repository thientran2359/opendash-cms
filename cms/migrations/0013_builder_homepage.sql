-- M4-S02-T01: site-owned homepage Builder singleton.
-- NULL means the original M3 home_page is still authoritative. Do not copy
-- or transform the M3 page at migration time: Open and Preview are read-only.
CREATE TABLE builder_homepage (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    version INTEGER NOT NULL CHECK (version >= 1),
    document_json TEXT
);

INSERT INTO builder_homepage (id, version, document_json) VALUES (1, 1, NULL);

-- Public media references are derived from the saved, renderable core/image
-- elements. Never add draft references here; saving a new tree replaces this
-- set atomically with the singleton document under the same SQLite writer.
CREATE TABLE builder_homepage_media (
    media_id TEXT PRIMARY KEY NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT
);
