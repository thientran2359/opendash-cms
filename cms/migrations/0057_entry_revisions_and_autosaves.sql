-- 0057_entry_revisions_and_autosaves.sql
-- Additive revision history and per-user server autosaves for content_entries (including pattern CPT).
-- +6 sqlite_schema objects total (2 tables, 2 unique autoindexes, 2 secondary indexes; 0 triggers).

CREATE TABLE entry_revisions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entry_id INTEGER NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
    content_type_key TEXT NOT NULL,
    revision_number INTEGER NOT NULL CHECK (revision_number >= 1),
    content_hash TEXT NOT NULL CHECK (length(content_hash) = 64),
    source TEXT NOT NULL CHECK (
        source IN (
            'create',
            'save',
            'builder_save',
            'builder_reset',
            'quick_edit',
            'seo_save',
            'pattern_save',
            'plugin_save',
            'restore',
            'import',
            'baseline'
        )
    ),
    restored_from_revision_number INTEGER DEFAULT NULL CHECK (
        restored_from_revision_number IS NULL OR restored_from_revision_number >= 1
    ),
    author_user_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
    author_username TEXT NOT NULL DEFAULT 'system',
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
    slug TEXT NOT NULL CHECK (length(slug) BETWEEN 1 AND 64),
    content_mode TEXT NOT NULL DEFAULT 'editor' CHECK (content_mode IN ('editor', 'builder')),
    values_json TEXT NOT NULL CHECK (json_valid(values_json) AND json_type(values_json) = 'object'),
    builder_document_json TEXT DEFAULT NULL CHECK (
        builder_document_json IS NULL OR (json_valid(builder_document_json) AND json_type(builder_document_json) = 'object')
    ),
    term_ids_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(term_ids_json) AND json_type(term_ids_json) = 'array'),
    primary_category_term_id INTEGER DEFAULT NULL,
    page_layout_key TEXT DEFAULT NULL,
    custom_template_key TEXT DEFAULT NULL,
    seo_json TEXT DEFAULT NULL CHECK (
        seo_json IS NULL OR (json_valid(seo_json) AND json_type(seo_json) = 'object')
    ),
    entry_version INTEGER NOT NULL CHECK (entry_version >= 1),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (entry_id, revision_number)
);

CREATE INDEX idx_entry_revisions_author ON entry_revisions (author_user_id, id DESC);

CREATE TABLE entry_autosaves (
    entry_id INTEGER NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    content_type_key TEXT NOT NULL,
    source_surface TEXT NOT NULL CHECK (source_surface IN ('editor', 'studio')),
    base_version INTEGER NOT NULL CHECK (base_version >= 1),
    client_seq INTEGER NOT NULL DEFAULT 1 CHECK (client_seq >= 1),
    title TEXT NOT NULL DEFAULT '' CHECK (length(title) <= 200),
    content_mode TEXT NOT NULL DEFAULT 'editor' CHECK (content_mode IN ('editor', 'builder')),
    values_json TEXT NOT NULL CHECK (json_valid(values_json) AND json_type(values_json) = 'object'),
    cleared_fields_json TEXT NOT NULL DEFAULT '[]' CHECK (
        json_valid(cleared_fields_json) AND json_type(cleared_fields_json) = 'array'
    ),
    builder_document_json TEXT DEFAULT NULL CHECK (
        builder_document_json IS NULL OR (json_valid(builder_document_json) AND json_type(builder_document_json) = 'object')
    ),
    term_ids_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(term_ids_json) AND json_type(term_ids_json) = 'array'),
    primary_category_touched INTEGER NOT NULL DEFAULT 0 CHECK (primary_category_touched IN (0, 1)),
    primary_category_term_id INTEGER DEFAULT NULL,
    page_layout_touched INTEGER NOT NULL DEFAULT 0 CHECK (page_layout_touched IN (0, 1)),
    page_layout_key TEXT DEFAULT NULL,
    custom_template_touched INTEGER NOT NULL DEFAULT 0 CHECK (custom_template_touched IN (0, 1)),
    custom_template_key TEXT DEFAULT NULL,
    seo_touched INTEGER NOT NULL DEFAULT 0 CHECK (seo_touched IN (0, 1)),
    seo_json TEXT DEFAULT NULL CHECK (
        seo_json IS NULL OR (json_valid(seo_json) AND json_type(seo_json) = 'object')
    ),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (entry_id, user_id)
);

CREATE INDEX idx_entry_autosaves_user ON entry_autosaves (user_id, updated_at DESC);
