-- M7-S16-T03: Post keeps core category/tag defaults. Page no longer opts in
-- through Core; explicit user/provider-owned grants and legacy entry_terms stay.
-- A withdrawn core Page declaration is retained for audit, not deleted.
INSERT INTO content_type_taxonomy_eligibility
    (content_type_key, taxonomy_kind, owner_kind, owner_id,
     declaration_state, provider_status)
SELECT 'post', 'category', 'core', NULL, 'declared', 'active'
WHERE EXISTS (SELECT 1 FROM content_types WHERE key = 'post')
ON CONFLICT(content_type_key, taxonomy_kind) DO UPDATE SET
    declaration_state = 'declared', provider_status = 'active',
    updated_at = datetime('now')
WHERE content_type_taxonomy_eligibility.owner_kind = 'core';

INSERT INTO content_type_taxonomy_eligibility
    (content_type_key, taxonomy_kind, owner_kind, owner_id,
     declaration_state, provider_status)
SELECT 'post', 'tag', 'core', NULL, 'declared', 'active'
WHERE EXISTS (SELECT 1 FROM content_types WHERE key = 'post')
ON CONFLICT(content_type_key, taxonomy_kind) DO UPDATE SET
    declaration_state = 'declared', provider_status = 'active',
    updated_at = datetime('now')
WHERE content_type_taxonomy_eligibility.owner_kind = 'core';

UPDATE content_type_taxonomy_eligibility
   SET declaration_state = 'withdrawn', updated_at = datetime('now')
 WHERE content_type_key = 'page' AND owner_kind = 'core'
   AND declaration_state = 'declared';
