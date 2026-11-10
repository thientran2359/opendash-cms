-- M3-S20-T01: fail-closed publication state, preserving legacy CPT behavior.
-- Preflight for conflicting post/page keys MUST run before sqlx migrate.
-- Legacy entries retain the default published state; built-in creation in the
-- storage layer ALWAYS writes draft explicitly.
ALTER TABLE content_entries
    ADD COLUMN publication_status TEXT NOT NULL DEFAULT 'published'
    CHECK (publication_status IN ('draft', 'published'));

CREATE INDEX idx_content_entries_published_type_id
    ON content_entries (content_type_key, publication_status, id DESC);

-- Defend against a future alternate INSERT path accidentally inheriting the
-- legacy published default. Only the explicit publication UPDATE may publish.
CREATE TRIGGER content_entries_core_insert_draft
    BEFORE INSERT ON content_entries
    WHEN NEW.content_type_key IN ('post', 'page')
         AND NEW.publication_status <> 'draft'
BEGIN
    SELECT RAISE(ABORT, 'Post/Page entries must start as draft');
END;

INSERT OR IGNORE INTO content_types (key, definition_json, owner_kind, owner_id, provider_status)
VALUES
('post', '{"key":"post","label":"Bài viết","fields":[{"key":"body","label":"Nội dung","type":"rich_text"},{"key":"featured_image","label":"Ảnh đại diện","type":"image"}]}', 'core', NULL, 'active'),
('page', '{"key":"page","label":"Trang","fields":[{"key":"body","label":"Nội dung","type":"rich_text"},{"key":"featured_image","label":"Ảnh đại diện","type":"image"}]}', 'core', NULL, 'active');
