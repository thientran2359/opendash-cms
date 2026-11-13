CREATE TABLE IF NOT EXISTS template_overrides (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    role TEXT NOT NULL CHECK (role IN ('page', 'header', 'footer', 'archive', 'single', 'taxonomy_archive')),
    scope_type_key TEXT,
    scope_entry_id INTEGER,
    ast_json TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    base_theme_id TEXT,
    base_revision_id TEXT,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_template_overrides_scope
    ON template_overrides (role, COALESCE(scope_type_key, ''), COALESCE(CAST(scope_entry_id AS TEXT), ''));
