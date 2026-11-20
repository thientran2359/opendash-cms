-- M3-S31-T01: preserve all historical content and publication statuses.
-- For existing user/provider CPTs, an absent `settings` is the immediate,
-- public, archived legacy default provided by the typed deserializer.
-- Backfill the three protected Core descriptors so their immutable policy is
-- explicit after migration. SQLx applies this forward-only transactionally.
UPDATE content_types
SET definition_json = json_set(
    definition_json,
    '$.settings',
    json_object(
        'public', json('true'),
        'has_archive', json('true'),
        'publication', 'draft_publish',
        'supports', json_array('editor', 'featured_image')
    )
)
WHERE key IN ('post', 'page') AND json_type(definition_json, '$.settings') IS NULL;

UPDATE content_types
SET definition_json = json_set(
    definition_json,
    '$.settings',
    json_object(
        'public', json('false'),
        'has_archive', json('false'),
        'publication', 'draft_publish',
        'supports', json_array()
    )
)
WHERE key = 'section' AND json_type(definition_json, '$.settings') IS NULL;
