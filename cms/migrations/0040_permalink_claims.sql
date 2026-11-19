-- M7-S17-T01: one durable canonical namespace, including historical paths.
-- Preserve the legacy site URLs on upgrade. New redirects begin only after a
-- later canonical change; existing M7-S03 redirects retain their stable IDs.
CREATE TABLE site_permalink_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    post_structure TEXT NOT NULL DEFAULT '/content/post/%postname%',
    content_type_bases_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(content_type_bases_json)),
    taxonomy_bases_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(taxonomy_bases_json))
);
INSERT INTO site_permalink_settings (id) VALUES (1);

CREATE TABLE permalink_route_claims (
    path TEXT PRIMARY KEY COLLATE BINARY NOT NULL,
    target_kind TEXT NOT NULL CHECK (target_kind IN ('home', 'entry', 'term', 'archive')),
    target_id TEXT NOT NULL COLLATE BINARY,
    UNIQUE (target_kind, target_id)
);
CREATE INDEX idx_permalink_route_claim_target
    ON permalink_route_claims (target_kind, target_id);

CREATE TABLE permalink_route_redirects (
    source_path TEXT PRIMARY KEY COLLATE BINARY NOT NULL,
    target_kind TEXT NOT NULL CHECK (target_kind IN ('entry', 'term', 'archive')),
    target_id TEXT NOT NULL COLLATE BINARY,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX idx_permalink_route_redirect_target
    ON permalink_route_redirects (target_kind, target_id);

INSERT INTO permalink_route_claims (path, target_kind, target_id)
VALUES ('/', 'home', 'site');

-- Existing custom paths have precedence over generated legacy entry URLs.
INSERT INTO permalink_route_claims (path, target_kind, target_id)
SELECT COALESCE(p.path, '/content/' || e.content_type_key || '/' || e.slug),
       'entry', CAST(e.id AS TEXT)
  FROM content_entries e
  LEFT JOIN entry_public_paths p ON p.entry_id = e.id
 WHERE e.content_type_key != 'section';

INSERT INTO permalink_route_claims (path, target_kind, target_id)
SELECT '/content/' || t.key, 'archive', t.key
  FROM content_types t
 WHERE t.key != 'section'
   AND COALESCE(json_extract(t.definition_json, '$.settings.has_archive'), 1) = 1;

INSERT INTO permalink_route_claims (path, target_kind, target_id)
SELECT '/taxonomy/' || t.kind || '/' || t.slug, 'term', CAST(t.id AS TEXT)
  FROM taxonomy_terms t;

-- Migration0027 recorded intentional redirects; avoid introducing aliases for
-- all untouched entries. 404 remains correct if a historical target is deleted.
INSERT INTO permalink_route_redirects (source_path, target_kind, target_id, created_at)
SELECT r.source_path, 'entry', CAST(r.entry_id AS TEXT), r.created_at
  FROM entry_redirects r
 WHERE NOT EXISTS (
   SELECT 1 FROM permalink_route_claims c WHERE c.path = r.source_path
 );
