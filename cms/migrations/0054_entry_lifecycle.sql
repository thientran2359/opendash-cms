-- Additive entry lifecycle column (NULL = active, non-NULL = UTC trashed_at).
-- Preserves all preexisting IDs, foreign keys, unique constraints, and rows as active.
ALTER TABLE content_entries ADD COLUMN trashed_at TEXT DEFAULT NULL;

-- Composite index for active/Trash table listing, counts, and status filters.
CREATE INDEX idx_content_entries_lifecycle_list
    ON content_entries (content_type_key, locale, trashed_at, publication_status, id);
