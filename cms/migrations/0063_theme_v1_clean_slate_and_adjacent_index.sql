-- 0063: Clean-Slate Theme Format V1 + remove 'home' override + partial index for Prev/Next
CREATE TABLE theme_packages_0063_copy AS SELECT * FROM theme_packages;
CREATE TABLE theme_revisions_0063_copy AS SELECT * FROM theme_revisions;
CREATE TABLE theme_active_selection_0063_copy AS SELECT * FROM theme_active_selection;

DROP TABLE theme_active_selection;
DROP TABLE theme_legacy_mappings;
DROP TABLE theme_revisions;
DROP TABLE theme_packages;

CREATE TABLE theme_packages (
    theme_id TEXT PRIMARY KEY NOT NULL,
    display_name TEXT NOT NULL,
    source_format TEXT NOT NULL DEFAULT 'v1' CHECK (source_format IN ('v1')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO theme_packages (theme_id, display_name, source_format, created_at)
SELECT DISTINCT p.theme_id, p.display_name, 'v1', p.created_at
  FROM theme_packages_0063_copy p
 WHERE EXISTS (
       SELECT 1 FROM theme_revisions_0063_copy r
        WHERE r.theme_id = p.theme_id
          AND r.source_format = 'v1'
          AND json_valid(r.manifest_json)
          AND CASE WHEN json_valid(r.manifest_json)
                   THEN json_extract(r.manifest_json, '$.manifest_version')
              END = 1
 );

CREATE TABLE theme_revisions (
    revision_id TEXT PRIMARY KEY NOT NULL,
    theme_id TEXT NOT NULL REFERENCES theme_packages(theme_id) ON DELETE RESTRICT,
    version TEXT NOT NULL,
    revision_digest TEXT NOT NULL UNIQUE,
    source_format TEXT NOT NULL DEFAULT 'v1' CHECK (source_format IN ('v1')),
    manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
    file_digests_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(file_digests_json)),
    provenance_json TEXT NOT NULL CHECK (json_valid(provenance_json)),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (theme_id, version)
);
CREATE INDEX idx_theme_revisions_theme_id ON theme_revisions(theme_id);
INSERT INTO theme_revisions (
    revision_id, theme_id, version, revision_digest, source_format,
    manifest_json, file_digests_json, provenance_json, created_at
)
SELECT revision_id, theme_id, version, revision_digest, 'v1',
       manifest_json, file_digests_json, provenance_json, created_at
  FROM theme_revisions_0063_copy
 WHERE source_format = 'v1'
   AND json_valid(manifest_json)
   AND CASE WHEN json_valid(manifest_json)
            THEN json_extract(manifest_json, '$.manifest_version')
       END = 1;

CREATE TABLE theme_active_selection (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    revision_id TEXT REFERENCES theme_revisions(revision_id) ON DELETE RESTRICT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1)
);
INSERT INTO theme_active_selection (id, revision_id, version)
SELECT s.id,
       CASE WHEN EXISTS (SELECT 1 FROM theme_revisions tr WHERE tr.revision_id = s.revision_id)
            THEN s.revision_id ELSE NULL END,
       s.version
  FROM theme_active_selection_0063_copy s;

DROP TABLE theme_active_selection_0063_copy;
DROP TABLE theme_revisions_0063_copy;
DROP TABLE theme_packages_0063_copy;

-- Remove 'home' override rows (template_override_media_refs cascades via ON DELETE CASCADE; keep 'search')
DELETE FROM template_overrides WHERE target_kind = 'home' OR canonical_target_key = 'home';

-- Partial expression index serving Prev/Next adjacent entry queries (Slice S02)
CREATE INDEX IF NOT EXISTS idx_content_entries_pub_order
    ON content_entries(content_type_key, locale, publication_status, COALESCE(published_at, updated_at) DESC, id DESC)
    WHERE publication_status = 'published' AND trashed_at IS NULL;
