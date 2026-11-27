-- 0056: Unified Field Engine content/term cutover & indexed entry field media refs (+0 net schema objects)

-- 1. Rebuild entry_builder_media_refs adding source_kind IN ('builder', 'field') and 'file' ref_role (+0 net schema objects)
CREATE TABLE entry_builder_media_refs_0056 (
    entry_id INTEGER NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
    ref_role TEXT NOT NULL CHECK (ref_role IN ('image', 'video', 'poster', 'file')),
    source_kind TEXT NOT NULL DEFAULT 'builder' CHECK (source_kind IN ('builder', 'field')),
    PRIMARY KEY (entry_id, media_id, ref_role, source_kind)
);

INSERT INTO entry_builder_media_refs_0056 (entry_id, media_id, ref_role, source_kind)
SELECT entry_id, media_id, ref_role, 'builder'
FROM entry_builder_media_refs;

DROP TABLE entry_builder_media_refs;
ALTER TABLE entry_builder_media_refs_0056 RENAME TO entry_builder_media_refs;
CREATE INDEX idx_entry_builder_media_refs_media ON entry_builder_media_refs(media_id);

-- 2. Normalize content_types.definition_json -> $.fields (preserve $.settings, empty fields: [], and field order)
UPDATE content_types
SET definition_json = json_set(
    definition_json,
    '$.fields',
    (
        SELECT json_group_array(json(norm_field))
        FROM (
            SELECT
                CASE
                    WHEN json_extract(f.value, '$.type') = 'image' THEN
                        json_set(
                            json_remove(
                                json_set(
                                    f.value,
                                    '$.type', 'media',
                                    '$.media_kind', 'image'
                                ),
                                '$.default_value', '$.options', '$.min_view_role', '$.min_edit_role', '$.public_exposure'
                            ),
                            '$.acl',
                            json_object(
                                'min_view_role', COALESCE(json_extract(f.value, '$.min_view_role'), 'editor'),
                                'min_edit_role', COALESCE(json_extract(f.value, '$.min_edit_role'), 'editor'),
                                'public_exposure', COALESCE(json_extract(f.value, '$.public_exposure'), 'public')
                            )
                        )
                    ELSE
                        (
                            SELECT
                                CASE
                                    WHEN json_extract(f.value, '$.default_value') IS NOT NULL THEN
                                        json_set(base_obj, '$.default', json_extract(f.value, '$.default_value'))
                                    ELSE base_obj
                                END
                            FROM (
                                SELECT
                                    json_set(
                                        json_remove(
                                            json_set(
                                                f.value,
                                                '$.type',
                                                CASE json_extract(f.value, '$.type')
                                                    WHEN 'short_text' THEN 'text'
                                                    WHEN 'long_text' THEN 'textarea'
                                                    WHEN 'rich_text' THEN 'richtext'
                                                    ELSE json_extract(f.value, '$.type')
                                                END
                                            ),
                                            '$.default_value',
                                            '$.min_view_role',
                                            '$.min_edit_role',
                                            '$.public_exposure',
                                            CASE
                                                WHEN json_extract(f.value, '$.type') = 'select' THEN '$.__keep_options__'
                                                ELSE '$.options'
                                            END
                                        ),
                                        '$.acl',
                                        json_object(
                                            'min_view_role', COALESCE(json_extract(f.value, '$.min_view_role'), 'editor'),
                                            'min_edit_role', COALESCE(json_extract(f.value, '$.min_edit_role'), 'editor'),
                                            'public_exposure', COALESCE(json_extract(f.value, '$.public_exposure'), 'public')
                                        )
                                    ) AS base_obj
                            )
                        )
                END AS norm_field
            FROM json_each(content_types.definition_json, '$.fields') AS f
            ORDER BY CAST(f.key AS INTEGER)
        )
    )
)
WHERE json_array_length(definition_json, '$.fields') > 0;

-- 3. Normalize content_field_groups.fields_json (preserve field order and empty [])
UPDATE content_field_groups
SET fields_json = (
    SELECT json_group_array(json(norm_field))
    FROM (
        SELECT
            CASE
                WHEN json_extract(f.value, '$.type') = 'image' THEN
                    json_set(
                        json_remove(
                            json_set(
                                f.value,
                                '$.type', 'media',
                                '$.media_kind', 'image'
                            ),
                            '$.default_value', '$.options', '$.min_view_role', '$.min_edit_role', '$.public_exposure'
                        ),
                        '$.acl',
                        json_object(
                            'min_view_role', COALESCE(json_extract(f.value, '$.min_view_role'), 'editor'),
                            'min_edit_role', COALESCE(json_extract(f.value, '$.min_edit_role'), 'editor'),
                            'public_exposure', COALESCE(json_extract(f.value, '$.public_exposure'), 'private')
                        )
                    )
                ELSE
                    (
                        SELECT
                            CASE
                                WHEN json_extract(f.value, '$.default_value') IS NOT NULL THEN
                                    json_set(base_obj, '$.default', json_extract(f.value, '$.default_value'))
                                ELSE base_obj
                            END
                        FROM (
                            SELECT
                                json_set(
                                    json_remove(
                                        json_set(
                                            f.value,
                                            '$.type',
                                            CASE json_extract(f.value, '$.type')
                                                WHEN 'short_text' THEN 'text'
                                                WHEN 'long_text' THEN 'textarea'
                                                WHEN 'rich_text' THEN 'richtext'
                                                ELSE json_extract(f.value, '$.type')
                                            END
                                        ),
                                        '$.default_value',
                                        '$.min_view_role',
                                        '$.min_edit_role',
                                        '$.public_exposure',
                                        CASE
                                            WHEN json_extract(f.value, '$.type') = 'select' THEN '$.__keep_options__'
                                            ELSE '$.options'
                                        END
                                    ),
                                    '$.acl',
                                    json_object(
                                        'min_view_role', COALESCE(json_extract(f.value, '$.min_view_role'), 'editor'),
                                        'min_edit_role', COALESCE(json_extract(f.value, '$.min_edit_role'), 'editor'),
                                        'public_exposure', COALESCE(json_extract(f.value, '$.public_exposure'), 'private')
                                    )
                                ) AS base_obj
                        )
                    )
            END AS norm_field
        FROM json_each(content_field_groups.fields_json) AS f
        ORDER BY CAST(f.key AS INTEGER)
    )
)
WHERE json_array_length(fields_json) > 0;

-- 4. Normalize admin_extensions.declaration_json for term_panel ($.config.fields)
UPDATE admin_extensions
SET declaration_json = json_set(
    declaration_json,
    '$.config.fields',
    (
        SELECT json_group_array(json(norm_field))
        FROM (
            SELECT
                json_set(
                    json_remove(f.value, '$.min_role'),
                    '$.acl',
                    json_object(
                        'min_view_role', COALESCE(json_extract(f.value, '$.min_role'), 'editor'),
                        'min_edit_role', COALESCE(json_extract(f.value, '$.min_role'), 'editor'),
                        'public_exposure', 'private'
                    )
                ) AS norm_field
            FROM json_each(admin_extensions.declaration_json, '$.config.fields') AS f
            ORDER BY CAST(f.key AS INTEGER)
        )
    )
)
WHERE json_extract(declaration_json, '$.kind') = 'term_panel'
  AND json_array_length(declaration_json, '$.config.fields') > 0;

-- 5. Backfill entry_builder_media_refs (source_kind = 'field') from BOTH content_types and content_field_groups
INSERT OR IGNORE INTO entry_builder_media_refs (entry_id, media_id, ref_role, source_kind)
SELECT DISTINCT
    e.id AS entry_id,
    ma.id AS media_id,
    ma.kind AS ref_role,
    'field' AS source_kind
FROM content_entries AS e
JOIN (
    SELECT
        ct.key AS type_key,
        json_extract(f.value, '$.key') AS field_key,
        json_extract(f.value, '$.type') AS field_type
    FROM content_types AS ct
    JOIN json_each(ct.definition_json, '$.fields') AS f
    WHERE json_extract(f.value, '$.type') IN ('media', 'gallery')
      AND json_extract(f.value, '$.key') != 'body'
    UNION
    SELECT
        t.type_key AS type_key,
        json_extract(f.value, '$.key') AS field_key,
        json_extract(f.value, '$.type') AS field_type
    FROM content_field_group_targets AS t
    JOIN content_field_groups AS g ON g.group_id = t.group_id
    JOIN json_each(g.fields_json) AS f
    WHERE t.association_state IN ('declared', 'withdrawn')
      AND g.status IN ('active', 'provider_inactive', 'withdrawn')
      AND json_extract(f.value, '$.type') IN ('media', 'gallery')
      AND json_extract(f.value, '$.key') != 'body'
) AS mf ON mf.type_key = e.content_type_key
JOIN media_assets AS ma ON (
    (mf.field_type = 'media'
        AND json_type(e.values_json, '$.' || mf.field_key) = 'text'
        AND ma.id = json_extract(e.values_json, '$.' || mf.field_key))
    OR
    (mf.field_type = 'gallery'
        AND json_type(e.values_json, '$.' || mf.field_key) = 'array'
        AND EXISTS (
            SELECT 1 FROM json_each(e.values_json, '$.' || mf.field_key) AS gi
            WHERE gi.value = ma.id
        ))
);

-- 6. Clean non-core pre-V1 builder_block_definitions containing legacy schema keys
DELETE FROM builder_block_definitions
WHERE owner_kind != 'core'
  AND (
    schema_json LIKE '%"show_if"%'
    OR schema_json LIKE '%"image_ref"%'
    OR schema_json LIKE '%"choices"%'
  );
