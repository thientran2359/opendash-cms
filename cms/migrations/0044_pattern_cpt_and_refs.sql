-- EGM S02: promote the private Section CPT into a private, reusable Pattern.
-- Keep the existing entry IDs, version, AST and media pins. All statements run
-- in SQLx's migration transaction (foreign_keys=ON).
--
-- The original Section trigger prevents changing content_type_key and
-- content_entries has an ON UPDATE RESTRICT FK. Create the new parent first,
-- migrate all FK children, then retire the original parent.
DROP TRIGGER content_entries_section_private_insert;
DROP TRIGGER content_entries_section_private_update;
DROP TRIGGER section_media_protect_lifecycle;

INSERT INTO content_types (
    key, definition_json, owner_kind, owner_id, provider_status,
    created_at, updated_at, declaration_state, version
)
SELECT
    'pattern',
    json_set(
        definition_json,
        '$.key', 'pattern', '$.label', 'Patterns',
        '$.settings', json_object(
            'public', json('false'),
            'has_archive', json('false'),
            'publication', 'immediate',
            'supports', json_array('builder')
        )
    ),
    owner_kind, owner_id, provider_status, created_at, updated_at,
    declaration_state, version
FROM content_types WHERE key = 'section';

UPDATE content_entries
SET content_type_key = 'pattern', content_mode = 'builder'
WHERE content_type_key = 'section';

-- Preserve any recorded associations and editor layout metadata if a site
-- already stored them against the old key. None are intended for a private
-- Pattern, but removing them would silently discard site-owned data.
UPDATE content_type_taxonomy_eligibility
SET content_type_key = 'pattern' WHERE content_type_key = 'section';
UPDATE content_type_taxonomy_overrides
SET content_type_key = 'pattern' WHERE content_type_key = 'section';
UPDATE content_field_group_targets
SET type_key = 'pattern' WHERE type_key = 'section';
UPDATE content_editor_layouts
SET type_key = 'pattern' WHERE type_key = 'section';

INSERT OR REPLACE INTO entry_builder_documents (entry_id, document_json, updated_at)
SELECT entry_id, document_json, datetime('now') FROM section_documents;

INSERT OR IGNORE INTO entry_builder_media_refs (entry_id, media_id, ref_role)
SELECT entry_id, media_id, ref_role FROM section_document_media_refs;

DROP TABLE section_document_media_refs;
DROP TABLE section_documents;
DELETE FROM content_types WHERE key = 'section';

-- A Pattern cannot become public, and changing its CPT key would permit
-- bypassing this restriction. Access to its AST is Admin-only at the API.
CREATE TRIGGER content_entries_pattern_private_insert
BEFORE INSERT ON content_entries
WHEN NEW.content_type_key = 'pattern' AND NEW.publication_status != 'draft'
BEGIN
    SELECT RAISE(ABORT, 'Pattern must remain private');
END;

CREATE TRIGGER content_entries_pattern_private_update
BEFORE UPDATE OF publication_status, content_type_key ON content_entries
WHEN (NEW.content_type_key = 'pattern' AND NEW.publication_status != 'draft')
     OR (OLD.content_type_key = 'pattern' AND NEW.content_type_key != 'pattern')
BEGIN
    SELECT RAISE(ABORT, 'Pattern must remain private');
END;

-- The AST is canonical; this index is a transactional, 1-hop projection of
-- core/pattern_ref nodes in each entry Builder document.
CREATE TABLE entry_pattern_refs (
    source_entry_id INTEGER NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
    target_pattern_id INTEGER NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
    PRIMARY KEY (source_entry_id, target_pattern_id)
);
CREATE INDEX idx_entry_pattern_refs_target ON entry_pattern_refs(target_pattern_id);
