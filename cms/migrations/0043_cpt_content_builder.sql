-- EGM Spec 2 / S01: generic CPT content Builder.
-- Migration 0042 already created content_entries.content_mode and the shared
-- entry_builder_documents / entry_builder_media_refs tables, keyed by entry ID.
-- Do not duplicate or migrate them; preserve every existing AST and editor body.
-- Index the public mode/capability candidate scan for arbitrary CPTs, not Page
-- alone. Capabilities remain effective-schema policy, not additional SQL flags.
CREATE INDEX idx_content_entries_public_builder_mode
    ON content_entries(content_type_key, publication_status, content_mode);

-- Migration 0038's historical Core Page seed had has_archive=true. Align an
-- untouched Core Page (version 1) with the Approved generic Builder default:
-- Page content is singular, not a standalone Page CPT archive. A site owner
-- who already customized Page behavior (version > 1) keeps their selection.
UPDATE content_types
   SET definition_json = json_set(definition_json, '$.settings.has_archive', json('false'))
 WHERE key = 'page' AND owner_kind = 'core' AND version = 1
   AND json_extract(definition_json, '$.settings.has_archive') = 1;
