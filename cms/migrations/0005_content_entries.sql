-- Entry content is owned by the site, not by the theme/plugin that provided
-- its CPT. Provider deactivation therefore never removes entries.
CREATE TABLE content_entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    content_type_key TEXT NOT NULL REFERENCES content_types(key)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    slug TEXT NOT NULL CHECK (length(slug) BETWEEN 1 AND 64),
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
    values_json TEXT NOT NULL
        CHECK (json_valid(values_json) AND json_type(values_json) = 'object'),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (content_type_key, slug)
);

CREATE INDEX idx_content_entries_type_id
    ON content_entries (content_type_key, id DESC);
