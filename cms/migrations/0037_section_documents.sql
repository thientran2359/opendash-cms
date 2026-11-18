-- M4-S11-T02. A Section belongs to the site and is never published.
-- preflight_builtin_key_collisions MUST reject existing non-core `section`
-- BEFORE SQLx reaches this migration. Do not overwrite provider/user data.
INSERT OR IGNORE INTO content_types (key, definition_json, owner_kind, owner_id, provider_status, declaration_state)
VALUES ('section', '{"key":"section","label":"Section","fields":[]}', 'core', NULL, 'active', 'declared');

-- Section visibility cannot be escalated by any generic entry writer, SQL
-- backfill, provider hook or future HTTP route. No public URL/status exists.
CREATE TRIGGER content_entries_section_private_insert
BEFORE INSERT ON content_entries
WHEN NEW.content_type_key = 'section' AND NEW.publication_status != 'draft'
BEGIN
    SELECT RAISE(ABORT, 'Section must remain private');
END;
CREATE TRIGGER content_entries_section_private_update
BEFORE UPDATE OF publication_status, content_type_key ON content_entries
WHEN (NEW.content_type_key = 'section' AND NEW.publication_status != 'draft')
     OR (OLD.content_type_key = 'section' AND NEW.content_type_key != 'section')
BEGIN
    SELECT RAISE(ABORT, 'Section must remain private');
END;

CREATE TABLE section_documents (
    entry_id INTEGER PRIMARY KEY NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
    document_json TEXT NOT NULL CHECK (json_valid(document_json) AND json_type(document_json) = 'object')
);

-- Private pin inventory. Never join this table into public media ACL queries.
-- The DB-level deletion guard preserves Section media even when a legacy
-- media deletion path does not yet display this Section-specific pin kind.
CREATE TABLE section_document_media_refs (
    entry_id INTEGER NOT NULL REFERENCES section_documents(entry_id) ON DELETE CASCADE,
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
    ref_role TEXT NOT NULL CHECK (ref_role IN ('image', 'video', 'poster')),
    PRIMARY KEY (entry_id, media_id, ref_role)
);
CREATE INDEX idx_section_document_media_refs_media ON section_document_media_refs(media_id);

-- Defense in depth for restore/maintenance and future lifecycle entry points:
-- Section pins must never be bypassed by changing the media lifecycle state.
CREATE TRIGGER section_media_protect_lifecycle
BEFORE UPDATE OF lifecycle_state ON media_assets
WHEN OLD.lifecycle_state = 'active' AND NEW.lifecycle_state != 'active'
     AND EXISTS (SELECT 1 FROM section_document_media_refs WHERE media_id = OLD.id)
BEGIN
    SELECT RAISE(ABORT, 'Private Section media is referenced');
END;
