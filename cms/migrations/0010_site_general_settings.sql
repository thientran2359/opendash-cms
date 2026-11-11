-- SITE-01 singleton. Do not modify legacy site rows or timestamps.
CREATE TABLE site_general_settings (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    site_title TEXT NOT NULL CHECK (length(site_title) BETWEEN 1 AND 120),
    tagline TEXT NOT NULL CHECK (length(tagline) <= 240),
    timezone TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1)
);

INSERT INTO site_general_settings (id, site_title, tagline, timezone, version)
VALUES (1, 'OpenDash', '', 'UTC', 1);
