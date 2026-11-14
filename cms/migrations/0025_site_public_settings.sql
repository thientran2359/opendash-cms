-- M7-S01-T01 site-public settings singleton. Do not modify site_general_settings.
CREATE TABLE site_public_settings (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    public_origin TEXT,
    content_language TEXT NOT NULL CHECK (length(content_language) BETWEEN 1 AND 35),
    indexing_enabled INTEGER NOT NULL DEFAULT 1 CHECK (indexing_enabled IN (0, 1)),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1)
);

INSERT INTO site_public_settings (id, public_origin, content_language, indexing_enabled, version)
VALUES (1, NULL, 'vi', 1, 1);
