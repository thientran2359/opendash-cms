-- M6-S02-T01: Owner-neutral CNT bridge indexes preserving M5 theme declarations
CREATE INDEX IF NOT EXISTS idx_content_types_owner
    ON content_types (owner_kind, owner_id);
CREATE INDEX IF NOT EXISTS idx_taxonomy_eligibility_owner
    ON content_type_taxonomy_eligibility (owner_kind, owner_id);
