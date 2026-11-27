-- 0063_acf_content_structure.sql
-- ACF Pro Content Structure (Đợt 2):
--   - Clean-Slate CPT + Hierarchy (parent_id, menu_order) + CPT settings extensions
--   - 4-Tab Field Groups (domain, location_rules_json, presentation_json, active) + polymorphic targets
--   - Taxonomy Settings + Term Meta (values_json, term_seo, term_field_media_refs, Drop term_extension_values)
--   - Custom Options Pages (custom_options_pages + generalized theme_option_* owner_kind = 'options_page')
-- Schema Object Budget: exact +0 net sqlite_schema objects (-3 - 1 + 0 + 0 + 1 + 2 + 1 = +0).

-- 1. Additive nullable/defaulted columns on existing tables (+0 net sqlite_schema objects)
ALTER TABLE content_field_groups
    ADD COLUMN domain TEXT NOT NULL DEFAULT 'content'
    CHECK (domain IN ('content', 'term', 'options_page'));
ALTER TABLE content_field_groups
    ADD COLUMN location_rules_json TEXT NOT NULL DEFAULT '[]'
    CHECK (json_valid(location_rules_json) AND json_type(location_rules_json) = 'array');
ALTER TABLE content_field_groups
    ADD COLUMN presentation_json TEXT NOT NULL DEFAULT '{}'
    CHECK (json_valid(presentation_json) AND json_type(presentation_json) = 'object');
ALTER TABLE content_field_groups
    ADD COLUMN active INTEGER NOT NULL DEFAULT 1
    CHECK (active IN (0, 1));

ALTER TABLE content_entries
    ADD COLUMN parent_id INTEGER DEFAULT NULL REFERENCES content_entries(id) ON DELETE SET NULL;
ALTER TABLE content_entries
    ADD COLUMN menu_order INTEGER NOT NULL DEFAULT 0;

CREATE INDEX idx_content_entries_parent
    ON content_entries (content_type_key, locale, parent_id, menu_order, id)
    WHERE parent_id IS NOT NULL; -- (+1)

ALTER TABLE entry_revisions
    ADD COLUMN parent_id INTEGER DEFAULT NULL;
ALTER TABLE entry_revisions
    ADD COLUMN menu_order INTEGER NOT NULL DEFAULT 0;

ALTER TABLE entry_autosaves
    ADD COLUMN parent_id_touched INTEGER NOT NULL DEFAULT 0
    CHECK (parent_id_touched IN (0, 1));
ALTER TABLE entry_autosaves
    ADD COLUMN parent_id INTEGER DEFAULT NULL;
ALTER TABLE entry_autosaves
    ADD COLUMN menu_order_touched INTEGER NOT NULL DEFAULT 0
    CHECK (menu_order_touched IN (0, 1));
ALTER TABLE entry_autosaves
    ADD COLUMN menu_order INTEGER DEFAULT NULL;

ALTER TABLE taxonomy_definitions
    ADD COLUMN settings_json TEXT NOT NULL DEFAULT '{}'
    CHECK (json_valid(settings_json) AND json_type(settings_json) = 'object');
ALTER TABLE taxonomy_definitions
    ADD COLUMN rewrite_base TEXT DEFAULT NULL;

ALTER TABLE taxonomy_terms
    ADD COLUMN values_json TEXT NOT NULL DEFAULT '{}'
    CHECK (json_valid(values_json) AND json_type(values_json) = 'object');
ALTER TABLE taxonomy_terms
    ADD COLUMN seo_title TEXT;
ALTER TABLE taxonomy_terms
    ADD COLUMN seo_description TEXT;
ALTER TABLE taxonomy_terms
    ADD COLUMN seo_og_image_id TEXT REFERENCES media_assets(id) ON DELETE SET NULL;
ALTER TABLE taxonomy_terms
    ADD COLUMN seo_canonical_url TEXT;
ALTER TABLE taxonomy_terms
    ADD COLUMN seo_noindex INTEGER NOT NULL DEFAULT 0
    CHECK (seo_noindex IN (0, 1));
ALTER TABLE taxonomy_terms
    ADD COLUMN description TEXT NOT NULL DEFAULT '';
ALTER TABLE taxonomy_terms
    ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;

-- 2. Backfill existing Field Groups (location_rules_json & presentation_json)
UPDATE content_field_groups
   SET domain = 'content',
       active = 1,
       presentation_json = json_object(
           'region', default_region,
           'order', default_order,
           'collapsible', CASE WHEN collapsible = 1 THEN json('true') ELSE json('false') END,
           'collapsed_by_default', json('false'),
           'style', 'card',
           'label_placement', 'top'
       ),
       location_rules_json = CASE
           WHEN EXISTS (
               SELECT 1
                 FROM content_field_group_targets t
                WHERE t.group_id = content_field_groups.group_id
                  AND t.association_state = 'declared'
                  AND t.type_key NOT IN ('pattern', 'section')
           ) THEN (
               SELECT json_group_array(
                   json_array(
                       json_object('param', 'content_type', 'op', 'eq', 'value', t.type_key)
                   )
               )
                 FROM content_field_group_targets t
                WHERE t.group_id = content_field_groups.group_id
                  AND t.association_state = 'declared'
                  AND t.type_key NOT IN ('pattern', 'section')
           )
           WHEN EXISTS (
               SELECT 1
                 FROM content_field_group_targets t
                WHERE t.group_id = content_field_groups.group_id
                  AND t.type_key NOT IN ('pattern', 'section')
           ) THEN (
               SELECT json_group_array(
                   json_array(
                       json_object('param', 'content_type', 'op', 'eq', 'value', t.type_key)
                   )
               )
                 FROM content_field_group_targets t
                WHERE t.group_id = content_field_groups.group_id
                  AND t.type_key NOT IN ('pattern', 'section')
           )
           ELSE '[]'
       END;

-- 3. Rebuild content_field_group_targets as polymorphic STRICT, WITHOUT ROWID (-1 net object: 3 -> 2)
CREATE TABLE content_field_group_targets_0063 (
    group_id TEXT NOT NULL REFERENCES content_field_groups(group_id) ON DELETE CASCADE,
    target_kind TEXT NOT NULL CHECK (target_kind IN ('content_type', 'taxonomy_term', 'options_page')),
    target_key TEXT NOT NULL COLLATE BINARY,
    association_state TEXT NOT NULL DEFAULT 'declared'
        CHECK (association_state IN ('declared', 'withdrawn')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (group_id, target_kind, target_key)
) STRICT, WITHOUT ROWID;

INSERT INTO content_field_group_targets_0063 (
    group_id, target_kind, target_key, association_state, created_at, updated_at
)
SELECT group_id, 'content_type', type_key, association_state, created_at, updated_at
  FROM content_field_group_targets
 WHERE type_key NOT IN ('pattern', 'section');

DROP TABLE content_field_group_targets;
ALTER TABLE content_field_group_targets_0063 RENAME TO content_field_group_targets;
CREATE INDEX idx_content_field_group_targets_target
    ON content_field_group_targets(target_kind, target_key, association_state);

-- 4. Migrate term_extension_values -> taxonomy_terms.values_json & admin_extensions term_panels -> content_field_groups
UPDATE taxonomy_terms
   SET values_json = COALESCE(
       (SELECT json_group_object(k, CASE
            WHEN tp = 'true' THEN json('true')
            WHEN tp = 'false' THEN json('false')
            WHEN tp IN ('object', 'array') THEN json(val)
            ELSE val
        END)
          FROM (
              SELECT je.key AS k,
                     je.value AS val,
                     je.type AS tp,
                     ROW_NUMBER() OVER (
                         PARTITION BY je.key
                         ORDER BY tev.owner_kind ASC, tev.owner_id ASC, tev.local_key ASC
                     ) AS rn
                FROM term_extension_values tev,
                     json_each(tev.values_json) je
               WHERE tev.term_id = taxonomy_terms.id
                 AND je.type != 'null'
          )
         WHERE rn = 1),
       '{}'
   )
 WHERE EXISTS (
     SELECT 1 FROM term_extension_values tev WHERE tev.term_id = taxonomy_terms.id
 );

-- Drop term_extension_values (-3 objects: table + sqlite_autoindex + term_extension_values_owner)
DROP TABLE term_extension_values;

-- Promote admin_extensions kind='term_panel' into Field Groups (domain='term')
CREATE TEMP TABLE _migrated_term_panels_0063 AS
SELECT ae.owner_kind,
       ae.owner_id,
       CASE
           WHEN EXISTS (
               SELECT 1 FROM content_field_groups g
                WHERE g.owner_kind = ae.owner_kind
                  AND g.owner_id = ae.owner_id
                  AND g.local_key = ae.local_key
           ) THEN CASE
               WHEN length(ae.local_key) <= 58 THEN ae.local_key || '__term'
               ELSE substr(ae.local_key, 1, 58) || '__term'
           END
           ELSE ae.local_key
       END AS resolved_local_key,
       COALESCE(NULLIF(json_extract(ae.declaration_json, '$.config.label'), ''), ae.local_key) AS title,
       COALESCE(NULLIF(json_extract(ae.declaration_json, '$.config.kind'), ''), 'category') AS tax_kind,
       CAST(COALESCE(json_extract(ae.declaration_json, '$.config.order'), 50) AS INTEGER) AS sort_order,
       COALESCE(json_extract(ae.declaration_json, '$.config.fields'), '[]') AS fields_json,
       CASE ae.provider_status WHEN 'inactive' THEN 'provider_inactive' ELSE 'active' END AS fg_status
  FROM admin_extensions ae
 WHERE json_extract(ae.declaration_json, '$.kind') = 'term_panel';

INSERT INTO content_field_groups (
    group_id, owner_kind, owner_id, local_key, title, description, fields_json,
    default_region, default_order, collapsible, status, version,
    domain, location_rules_json, presentation_json, active
)
SELECT m.owner_kind || ':' || m.owner_id || ':' || m.resolved_local_key,
       m.owner_kind,
       m.owner_id,
       m.resolved_local_key,
       substr(m.title, 1, 100),
       '',
       m.fields_json,
       'main_bottom',
       m.sort_order,
       1,
       m.fg_status,
       1,
       'term',
       json_array(json_array(json_object('param', 'taxonomy_term', 'op', 'eq', 'value', m.tax_kind))),
       json_object(
           'region', 'main_bottom',
           'order', m.sort_order,
           'collapsible', json('true'),
           'collapsed_by_default', json('false'),
           'style', 'card',
           'label_placement', 'top'
       ),
       1
  FROM _migrated_term_panels_0063 m;

INSERT INTO content_field_group_targets (
    group_id, target_kind, target_key, association_state
)
SELECT m.owner_kind || ':' || m.owner_id || ':' || m.resolved_local_key,
       'taxonomy_term',
       m.tax_kind,
       'declared'
  FROM _migrated_term_panels_0063 m;

DROP TABLE _migrated_term_panels_0063;

DELETE FROM admin_extensions
 WHERE json_extract(declaration_json, '$.kind') = 'term_panel';

-- 5. Clean-Slate CPT field cutover: promote compatible body/featured_image/excerpt -> supports,
--    rename incompatible reserved keys -> *_legacy, migrate remaining $.fields -> user Field Group,
--    then remove $.fields from content_types & backfill archive_per_page / pattern settings.
CREATE TEMP TABLE _legacy_cpt_fields_0063 AS
SELECT ct.key AS cpt_key,
       ct.label AS cpt_label,
       je.key AS idx,
       json_extract(je.value, '$.key') AS field_key,
       json_extract(je.value, '$.type') AS field_type,
       je.value AS raw_spec,
       CASE
           WHEN json_extract(je.value, '$.key') = 'body'
                AND json_extract(je.value, '$.type') IN ('richtext', 'rich_text') THEN 'promote_editor'
           WHEN json_extract(je.value, '$.key') = 'featured_image'
                AND json_extract(je.value, '$.type') = 'image' THEN 'promote_featured_image'
           WHEN json_extract(je.value, '$.key') = 'excerpt'
                AND json_extract(je.value, '$.type') IN ('textarea', 'text') THEN 'promote_excerpt'
           WHEN json_extract(je.value, '$.key') IN (
                'body', 'featured_image', 'excerpt',
                'parent_id', 'menu_order', 'custom_template',
                'primary_category', 'page_layout', 'title', 'slug', 'locale', 'status', 'terms'
           ) THEN 'rename_legacy'
           ELSE 'move_as_is'
       END AS action_kind
  FROM (
      SELECT key,
             COALESCE(json_extract(definition_json, '$.label'), key) AS label,
             definition_json
        FROM content_types
       WHERE key NOT IN ('post', 'page', 'pattern', 'section')
         AND json_type(definition_json, '$.fields') = 'array'
         AND json_array_length(json_extract(definition_json, '$.fields')) > 0
  ) ct,
  json_each(ct.definition_json, '$.fields') je;

-- Rename entry values keys when a reserved Core key had incompatible field type
UPDATE content_entries
   SET values_json = COALESCE(
       (SELECT json_group_object(
                   COALESCE(
                       (SELECT CASE
                                   WHEN EXISTS (SELECT 1 FROM json_each(content_entries.values_json) e2 WHERE e2.key = l.field_key || '_legacy')
                                        THEN j.key
                                   ELSE l.field_key || '_legacy'
                               END
                          FROM _legacy_cpt_fields_0063 l
                         WHERE l.cpt_key = content_entries.content_type_key
                           AND l.action_kind = 'rename_legacy'
                           AND l.field_key = j.key
                         LIMIT 1),
                       j.key
                   ),
                   CASE
                       WHEN j.type = 'true' THEN json('true')
                       WHEN j.type = 'false' THEN json('false')
                       WHEN j.type IN ('object', 'array') THEN json(j.value)
                       ELSE j.value
                   END
               )
          FROM json_each(content_entries.values_json) j),
       '{}'
   )
 WHERE EXISTS (
     SELECT 1 FROM _legacy_cpt_fields_0063 l
      WHERE l.cpt_key = content_entries.content_type_key
        AND l.action_kind = 'rename_legacy'
        AND json_type(content_entries.values_json, '$.' || l.field_key) IS NOT NULL
 );

-- Promote compatible Core supports (editor, featured_image, excerpt) on non-system CPTs
UPDATE content_types
   SET definition_json = json_set(
       definition_json,
       '$.settings.supports',
       COALESCE(
           (SELECT json_group_array(s_val)
              FROM (
                  SELECT DISTINCT s_val
                    FROM (
                        SELECT se.value AS s_val
                          FROM json_each(COALESCE(json_extract(content_types.definition_json, '$.settings.supports'), '[]')) se
                        UNION ALL
                        SELECT CASE l.action_kind
                                   WHEN 'promote_editor' THEN 'editor'
                                   WHEN 'promote_featured_image' THEN 'featured_image'
                                   WHEN 'promote_excerpt' THEN 'excerpt'
                               END AS s_val
                          FROM _legacy_cpt_fields_0063 l
                         WHERE l.cpt_key = content_types.key
                           AND l.action_kind IN ('promote_editor', 'promote_featured_image', 'promote_excerpt')
                    )
                   WHERE s_val IS NOT NULL
              )),
           json('[]')
       )
   )
 WHERE key IN (
     SELECT DISTINCT cpt_key
       FROM _legacy_cpt_fields_0063
      WHERE action_kind IN ('promote_editor', 'promote_featured_image', 'promote_excerpt')
 );

-- Materialize remaining CPT $.fields into User-owned Field Groups (status='active')
CREATE TEMP TABLE _cpt_migrated_groups_0063 AS
SELECT l.cpt_key,
       l.cpt_label,
       CASE
           WHEN NOT EXISTS (
               SELECT 1 FROM content_field_groups g
                WHERE g.owner_kind = 'user'
                  AND g.local_key = CASE WHEN length(l.cpt_key) <= 57 THEN l.cpt_key || '_fields' ELSE substr(l.cpt_key, 1, 57) || '_fields' END
           ) THEN CASE WHEN length(l.cpt_key) <= 57 THEN l.cpt_key || '_fields' ELSE substr(l.cpt_key, 1, 57) || '_fields' END
           ELSE CASE WHEN length(l.cpt_key) <= 48 THEN l.cpt_key || '_migrated_fields' ELSE substr(l.cpt_key, 1, 48) || '_migrated_fields' END
       END AS resolved_local_key,
       (
           SELECT json_group_array(
                      json(
                          CASE
                              WHEN l2.action_kind = 'rename_legacy'
                                  THEN json_set(l2.raw_spec, '$.key', l2.field_key || '_legacy')
                              ELSE l2.raw_spec
                          END
                      )
                  )
             FROM (
                 SELECT *
                   FROM _legacy_cpt_fields_0063 sub
                  WHERE sub.cpt_key = l.cpt_key
                    AND sub.action_kind IN ('rename_legacy', 'move_as_is')
                  ORDER BY CAST(sub.idx AS INTEGER) ASC
             ) l2
       ) AS migrated_fields_json
  FROM _legacy_cpt_fields_0063 l
 WHERE l.action_kind IN ('rename_legacy', 'move_as_is')
 GROUP BY l.cpt_key;

INSERT INTO content_field_groups (
    group_id, owner_kind, owner_id, local_key, title, description, fields_json,
    default_region, default_order, collapsible, status, version,
    domain, location_rules_json, presentation_json, active
)
SELECT 'user:' || mg.resolved_local_key,
       'user',
       NULL,
       mg.resolved_local_key,
       substr(mg.cpt_label || ' Fields', 1, 100),
       '',
       mg.migrated_fields_json,
       'main_bottom',
       50,
       1,
       'active',
       1,
       'content',
       json_array(json_array(json_object('param', 'content_type', 'op', 'eq', 'value', mg.cpt_key))),
       json_object(
           'region', 'main_bottom',
           'order', 50,
           'collapsible', json('true'),
           'collapsed_by_default', json('false'),
           'style', 'card',
           'label_placement', 'top'
       ),
       1
  FROM _cpt_migrated_groups_0063 mg;

INSERT INTO content_field_group_targets (
    group_id, target_kind, target_key, association_state
)
SELECT 'user:' || mg.resolved_local_key,
       'content_type',
       mg.cpt_key,
       'declared'
  FROM _cpt_migrated_groups_0063 mg;

DROP TABLE _cpt_migrated_groups_0063;
DROP TABLE _legacy_cpt_fields_0063;

-- Clean content_types.definition_json (strip $.fields, backfill archive_per_page & Clean-Slate pattern settings)
UPDATE content_types
   SET definition_json = json_remove(definition_json, '$.fields')
 WHERE json_type(definition_json, '$.fields') IS NOT NULL;

UPDATE content_types
   SET definition_json = json_set(definition_json, '$.settings.archive_per_page', 20)
 WHERE COALESCE(json_extract(definition_json, '$.settings.has_archive'), 0) = 1
   AND json_type(definition_json, '$.settings.archive_per_page') IS NULL;

UPDATE content_types
   SET definition_json = json_remove(
       json_set(
           definition_json,
           '$.settings.show_in_menu', json('false'),
           '$.settings.exclude_from_sitemap', json('true'),
           '$.settings.hierarchical', json('false')
       ),
       '$.settings.menu_position'
   )
 WHERE key = 'pattern';

-- 6. Strip obsolete base_field_panels, core:base_fields, and layout:* from content_editor_layouts.layout_json
UPDATE content_editor_layouts
   SET layout_json = json_set(
       json_remove(layout_json, '$.base_field_panels'),
       '$.regions.main_top', COALESCE(
           (SELECT json_group_array(json(je.value))
              FROM json_each(content_editor_layouts.layout_json, '$.regions.main_top') je
             WHERE COALESCE(json_extract(je.value, '$.panel_id'), '') != 'core:base_fields'
               AND COALESCE(json_extract(je.value, '$.panel_id'), '') NOT LIKE 'layout:%'),
           json('[]')
       ),
       '$.regions.main_primary', COALESCE(
           (SELECT json_group_array(json(je.value))
              FROM json_each(content_editor_layouts.layout_json, '$.regions.main_primary') je
             WHERE COALESCE(json_extract(je.value, '$.panel_id'), '') != 'core:base_fields'
               AND COALESCE(json_extract(je.value, '$.panel_id'), '') NOT LIKE 'layout:%'),
           json('[]')
       ),
       '$.regions.main_bottom', COALESCE(
           (SELECT json_group_array(json(je.value))
              FROM json_each(content_editor_layouts.layout_json, '$.regions.main_bottom') je
             WHERE COALESCE(json_extract(je.value, '$.panel_id'), '') != 'core:base_fields'
               AND COALESCE(json_extract(je.value, '$.panel_id'), '') NOT LIKE 'layout:%'),
           json('[]')
       ),
       '$.regions.sidebar', COALESCE(
           (SELECT json_group_array(json(je.value))
              FROM json_each(content_editor_layouts.layout_json, '$.regions.sidebar') je
             WHERE COALESCE(json_extract(je.value, '$.panel_id'), '') != 'core:base_fields'
               AND COALESCE(json_extract(je.value, '$.panel_id'), '') NOT LIKE 'layout:%'),
           json('[]')
       )
   )
 WHERE json_type(layout_json, '$.base_field_panels') IS NOT NULL
    OR instr(layout_json, 'core:base_fields') > 0
    OR instr(layout_json, '"layout:') > 0;

-- 7. Clean historical plugin & theme manifests so #[serde(deny_unknown_fields)] succeeds after cutover
UPDATE plugins
   SET manifest_json = json_set(
       manifest_json,
       '$.declarations.content_types',
       COALESCE(
           (SELECT json_group_array(json(json_remove(je.value, '$.fields')))
              FROM json_each(plugins.manifest_json, '$.declarations.content_types') je),
           json('[]')
       )
   )
 WHERE json_type(manifest_json, '$.declarations.content_types') = 'array'
   AND EXISTS (
       SELECT 1 FROM json_each(plugins.manifest_json, '$.declarations.content_types') je
        WHERE json_type(je.value, '$.fields') IS NOT NULL
   );

UPDATE plugin_revisions
   SET manifest_json = json_set(
       manifest_json,
       '$.declarations.content_types',
       COALESCE(
           (SELECT json_group_array(json(json_remove(je.value, '$.fields')))
              FROM json_each(plugin_revisions.manifest_json, '$.declarations.content_types') je),
           json('[]')
       )
   )
 WHERE json_type(manifest_json, '$.declarations.content_types') = 'array'
   AND EXISTS (
       SELECT 1 FROM json_each(plugin_revisions.manifest_json, '$.declarations.content_types') je
        WHERE json_type(je.value, '$.fields') IS NOT NULL
   );

UPDATE theme_revisions
   SET manifest_json = json_set(
       manifest_json,
       '$.declarations.content_types',
       COALESCE(
           (SELECT json_group_array(json(json_remove(je.value, '$.fields')))
              FROM json_each(theme_revisions.manifest_json, '$.declarations.content_types') je),
           json('[]')
       )
   )
 WHERE json_type(manifest_json, '$.declarations.content_types') = 'array'
   AND EXISTS (
       SELECT 1 FROM json_each(theme_revisions.manifest_json, '$.declarations.content_types') je
        WHERE json_type(je.value, '$.fields') IS NOT NULL
   );

UPDATE theme_revisions
   SET manifest_json = json_remove(manifest_json, '$.admin_extensions.term_panels')
 WHERE json_type(manifest_json, '$.admin_extensions.term_panels') IS NOT NULL;

-- 8. Rebuild seo_public_media_refs adding 'term_seo' (+0 net objects)
CREATE TABLE seo_public_media_refs_0063 (
    source_kind TEXT NOT NULL CHECK (source_kind IN ('entry_seo', 'home_seo', 'term_seo')),
    source_id INTEGER NOT NULL,
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE CASCADE,
    PRIMARY KEY (source_kind, source_id, media_id)
);
INSERT INTO seo_public_media_refs_0063 (source_kind, source_id, media_id)
SELECT source_kind, source_id, media_id FROM seo_public_media_refs;
DROP TABLE seo_public_media_refs;
ALTER TABLE seo_public_media_refs_0063 RENAME TO seo_public_media_refs;
CREATE INDEX idx_seo_public_media_refs_media_id
    ON seo_public_media_refs (media_id);

-- 9. Rebuild all 5 theme_option_* tables supporting owner_kind='options_page' & composite locale keys (+0 net objects)
CREATE TABLE theme_option_values_0063 (
    theme_id TEXT NOT NULL DEFAULT '',
    owner_kind TEXT NOT NULL DEFAULT 'theme'
        CHECK (owner_kind IN ('theme', 'plugin', 'theme_functions', 'options_page')),
    owner_id TEXT NOT NULL,
    option_key TEXT NOT NULL,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (owner_kind, owner_id, option_key)
);
INSERT INTO theme_option_values_0063 (theme_id, owner_kind, owner_id, option_key, value_json, updated_at)
SELECT theme_id, owner_kind, owner_id, option_key, value_json, updated_at
  FROM theme_option_values;
DROP TABLE theme_option_values;
ALTER TABLE theme_option_values_0063 RENAME TO theme_option_values;
CREATE INDEX idx_theme_option_values_owner ON theme_option_values (owner_kind, owner_id);

CREATE TABLE theme_option_versions_0063 (
    theme_id TEXT NOT NULL DEFAULT '',
    owner_kind TEXT NOT NULL DEFAULT 'theme'
        CHECK (owner_kind IN ('theme', 'plugin', 'theme_functions', 'options_page')),
    owner_id TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (owner_kind, owner_id)
);
INSERT INTO theme_option_versions_0063 (theme_id, owner_kind, owner_id, version, updated_at)
SELECT theme_id, owner_kind, owner_id, version, updated_at
  FROM theme_option_versions;
DROP TABLE theme_option_versions;
ALTER TABLE theme_option_versions_0063 RENAME TO theme_option_versions;

CREATE TABLE theme_option_media_refs_0063 (
    theme_id TEXT NOT NULL DEFAULT '',
    owner_kind TEXT NOT NULL DEFAULT 'theme'
        CHECK (owner_kind IN ('theme', 'plugin', 'theme_functions', 'options_page')),
    owner_id TEXT NOT NULL,
    locale TEXT NOT NULL DEFAULT '',
    option_key TEXT NOT NULL,
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
    PRIMARY KEY (owner_kind, owner_id, locale, option_key, media_id)
);
INSERT INTO theme_option_media_refs_0063 (theme_id, owner_kind, owner_id, locale, option_key, media_id)
SELECT theme_id, owner_kind, owner_id, locale, option_key, media_id
  FROM theme_option_media_refs;
DROP TABLE theme_option_media_refs;
ALTER TABLE theme_option_media_refs_0063 RENAME TO theme_option_media_refs;

CREATE TABLE theme_option_locale_values_0063 (
    theme_id TEXT NOT NULL DEFAULT '',
    owner_kind TEXT NOT NULL DEFAULT 'theme'
        CHECK (owner_kind IN ('theme', 'options_page')),
    owner_id TEXT NOT NULL DEFAULT '',
    locale TEXT NOT NULL REFERENCES site_locales(code) ON UPDATE CASCADE ON DELETE RESTRICT,
    option_key TEXT NOT NULL,
    value_json TEXT NOT NULL CHECK (json_valid(value_json)),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (owner_kind, owner_id, locale, option_key)
);
INSERT INTO theme_option_locale_values_0063 (
    theme_id, owner_kind, owner_id, locale, option_key, value_json, updated_at
)
SELECT theme_id, 'theme', theme_id, locale, option_key, value_json, updated_at
  FROM theme_option_locale_values;
DROP TABLE theme_option_locale_values;
ALTER TABLE theme_option_locale_values_0063 RENAME TO theme_option_locale_values;

CREATE TABLE theme_option_locale_versions_0063 (
    theme_id TEXT NOT NULL DEFAULT '',
    owner_kind TEXT NOT NULL DEFAULT 'theme'
        CHECK (owner_kind IN ('theme', 'options_page')),
    owner_id TEXT NOT NULL DEFAULT '',
    locale TEXT NOT NULL REFERENCES site_locales(code) ON UPDATE CASCADE ON DELETE RESTRICT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (owner_kind, owner_id, locale)
);
INSERT INTO theme_option_locale_versions_0063 (
    theme_id, owner_kind, owner_id, locale, version, updated_at
)
SELECT theme_id, 'theme', theme_id, locale, version, updated_at
  FROM theme_option_locale_versions;
DROP TABLE theme_option_locale_versions;
ALTER TABLE theme_option_locale_versions_0063 RENAME TO theme_option_locale_versions;

-- 10. Create Custom Options Pages table (+1 object: STRICT, WITHOUT ROWID)
CREATE TABLE custom_options_pages (
    page_key TEXT PRIMARY KEY NOT NULL COLLATE BINARY CHECK (length(page_key) BETWEEN 1 AND 64),
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 80),
    menu_title TEXT NOT NULL CHECK (length(menu_title) BETWEEN 1 AND 40),
    description TEXT NOT NULL DEFAULT '',
    parent_menu TEXT NOT NULL DEFAULT 'settings'
        CHECK (parent_menu IN ('content', 'appearance', 'system', 'settings', 'top_level')),
    icon_key TEXT DEFAULT NULL,
    position INTEGER DEFAULT NULL CHECK (position IS NULL OR position BETWEEN 0 AND 1000),
    min_edit_role TEXT NOT NULL DEFAULT 'admin' CHECK (min_edit_role IN ('admin', 'editor')),
    owner_kind TEXT NOT NULL CHECK (owner_kind IN ('user', 'theme', 'plugin')),
    owner_id TEXT DEFAULT NULL,
    provider_status TEXT NOT NULL DEFAULT 'active'
        CHECK (provider_status IN ('active', 'provider_inactive')),
    declaration_state TEXT NOT NULL DEFAULT 'declared'
        CHECK (declaration_state IN ('declared', 'withdrawn')),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (
        (owner_kind = 'user' AND owner_id IS NULL)
        OR (owner_kind IN ('theme', 'plugin') AND owner_id IS NOT NULL AND length(owner_id) > 0)
    )
) STRICT, WITHOUT ROWID;

-- 11. Create Term Field Media Refs table + secondary index (+2 objects: table + index)
CREATE TABLE term_field_media_refs (
    term_id INTEGER NOT NULL REFERENCES taxonomy_terms(id) ON DELETE CASCADE,
    locale TEXT NOT NULL DEFAULT '',
    field_key TEXT NOT NULL COLLATE BINARY,
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
    PRIMARY KEY (term_id, locale, field_key, media_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX idx_term_field_media_refs_media
    ON term_field_media_refs(media_id);
