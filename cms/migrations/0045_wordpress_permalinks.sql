-- 0045_wordpress_permalinks.sql
-- Mặc định WordPress permalinks, Primary Category, cờ has_been_published + triggers tất định, và tái dựng claims.

-- Chuyển mặc định /content/post/%postname% (hoặc cấu trúc chứa /content/ hay prefix literal nay trùng reserved root, '/posts', hoặc CPT/Taxonomy base) về chuẩn root /%postname%
UPDATE site_permalink_settings
   SET post_structure = '/%postname%'
 WHERE id = 1
   AND (
       post_structure = '/content/post/%postname%'
       OR post_structure LIKE '/content/%'
       OR (
           instr(post_structure, '%') > 2
           AND (
               trim(substr(post_structure, 2, instr(post_structure, '%') - 2), '/') IN (
                   'admin','api','assets','auth','builder','content','favicon.ico','forms',
                   'health','media','page','pattern','preview','registry','robots.txt',
                   'save','section','sitemap.xml','sitemaps','static','studio','taxonomy','theme','themes','uploads'
               )
               OR (
                   trim(substr(post_structure, 2, instr(post_structure, '%') - 2), '/') = 'posts'
                   AND (SELECT posts_page_id FROM site_reading_settings WHERE id = 1) IS NULL
               )
               OR EXISTS (
                   SELECT 1 FROM content_types t
                    WHERE t.key NOT IN ('pattern', 'section', 'page', 'post')
                      AND t.provider_status = 'active'
                      AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
                      AND trim(substr(post_structure, 2, instr(post_structure, '%') - 2), '/') = COALESCE(
                          NULLIF(json_extract(site_permalink_settings.content_type_bases_json, '$."' || t.key || '"'), ''),
                          (SELECT d.rewrite_base FROM provider_rewrite_defaults d
                            WHERE d.resource_kind = 'content_type' AND d.resource_key = t.key
                              AND d.provider_status = 'active' AND d.declaration_state = 'declared'
                              AND json_type(site_permalink_settings.content_type_bases_json, '$."' || t.key || '"') IS NULL),
                          t.key
                      )
               )
               OR EXISTS (
                   SELECT 1 FROM taxonomy_definitions td
                    WHERE td.public = 1 AND td.provider_status = 'active' AND td.declaration_state = 'declared'
                      AND trim(substr(post_structure, 2, instr(post_structure, '%') - 2), '/') = COALESCE(
                          NULLIF(json_extract(site_permalink_settings.taxonomy_bases_json, '$."' || td.key || '"'), ''),
                          (SELECT d.rewrite_base FROM provider_rewrite_defaults d
                            WHERE d.resource_kind = 'taxonomy' AND d.resource_key = td.key
                              AND d.provider_status = 'active' AND d.declaration_state = 'declared'
                              AND json_type(site_permalink_settings.taxonomy_bases_json, '$."' || td.key || '"') IS NULL),
                          td.key
                      )
               )
           )
       )
   );

ALTER TABLE content_entries
  ADD COLUMN primary_category_term_id INTEGER REFERENCES taxonomy_terms(id) ON DELETE SET NULL;

ALTER TABLE content_entries
  ADD COLUMN has_been_published INTEGER NOT NULL DEFAULT 0 CHECK (has_been_published IN (0, 1));

UPDATE content_entries
   SET has_been_published = 1
 WHERE publication_status = 'published';

-- Dọn redirect cũ thuộc bản nháp chưa từng xuất bản (nếu có từ các bản 0040..0044)
DELETE FROM entry_redirects
 WHERE entry_id IN (SELECT id FROM content_entries WHERE has_been_published = 0);

DELETE FROM permalink_route_redirects
 WHERE target_kind = 'entry'
   AND target_id IN (SELECT CAST(id AS TEXT) FROM content_entries WHERE has_been_published = 0);

-- 0b. Dọn mọi entry_public_paths có segment đầu thuộc 25 gốc hệ thống (gồm 3 gốc mới 'studio', 'section', 'pattern' — áp dụng cho cả posts_page_id)
DELETE FROM entry_public_paths
 WHERE (
       CASE
         WHEN instr(substr(path, 2), '/') > 0 THEN substr(path, 2, instr(substr(path, 2), '/') - 1)
         ELSE substr(path, 2)
       END
   ) IN (
       'admin','api','assets','auth','builder','content','favicon.ico','forms',
       'health','media','page','pattern','preview','registry','robots.txt',
       'save','section','sitemap.xml','sitemaps','static','studio','taxonomy','theme','themes','uploads'
   );

CREATE INDEX idx_content_entries_primary_category
    ON content_entries(primary_category_term_id)
 WHERE primary_category_term_id IS NOT NULL;

-- Giữ has_been_published đơn điệu (0 -> 1) ở tầng DB trên mọi nhánh INSERT/UPDATE
CREATE TRIGGER content_entries_mark_has_been_published_insert
    AFTER INSERT ON content_entries
    WHEN NEW.publication_status = 'published' AND NEW.has_been_published = 0
BEGIN
    UPDATE content_entries SET has_been_published = 1 WHERE id = NEW.id;
END;

CREATE TRIGGER content_entries_mark_has_been_published_update
    AFTER UPDATE OF publication_status ON content_entries
    WHEN NEW.publication_status = 'published' AND OLD.has_been_published = 0
BEGIN
    UPDATE content_entries SET has_been_published = 1 WHERE id = NEW.id;
END;

-- Tái dựng đồng bộ claims cho site nâng cấp từ 0040..0044 theo đúng thứ tự:
-- (1) Home/homepage_page_id -> (2) Public Active CPT/Post Archives -> (3) Public Active Terms
-- -> (3b) Dọn entry_public_paths va chạm -> (4) Disambiguate slug -> (5) Entries -> (6) Dọn redirects trùng claim.
-- Lưu ý JSON1: dùng json_type(...) IS NULL thay vì json_extract(...) IS NULL để phân biệt explicit JSON null (opt-out) với key omitted;
-- và luôn bọc key bằng cặp dấu nháy kép '$."' || key || '"' để hỗ trợ key có dấu '-'.
DELETE FROM permalink_route_claims;

-- 1. Home hoặc homepage_page_id (giữ '/' làm tombstone ('entry', id) khi homepage_page_id IS NOT NULL khớp permalinks.rs:657-667)
INSERT INTO permalink_route_claims (path, target_kind, target_id)
SELECT '/',
       CASE WHEN r.homepage_page_id IS NOT NULL THEN 'entry' ELSE 'home' END,
       COALESCE(CAST(r.homepage_page_id AS TEXT), 'site')
  FROM site_reading_settings r
 WHERE r.id = 1;

-- 2. Archives (loại trừ 'pattern', 'section', 'page', CPT public=0 hoặc provider_inactive;
-- nếu posts_page_id có slug trùng 25 gốc hệ thống hoặc trỏ Page đã xóa thì rebuild về '/posts')
INSERT INTO permalink_route_claims (path, target_kind, target_id)
SELECT CASE
         WHEN t.key = 'post' THEN COALESCE(
             (SELECT COALESCE(
                         p.path,
                         CASE
                           WHEN e.slug NOT IN (
                               'admin','api','assets','auth','builder','content','favicon.ico','forms',
                               'health','media','page','pattern','preview','registry','robots.txt',
                               'save','section','sitemap.xml','sitemaps','static','studio','taxonomy','theme','themes','uploads'
                           ) THEN '/' || e.slug
                         END
                     )
                FROM site_reading_settings r
                JOIN content_entries e ON e.id = r.posts_page_id
                LEFT JOIN entry_public_paths p ON p.entry_id = e.id
               WHERE r.id = 1 AND e.content_type_key = 'page'),
             '/posts'
         )
         ELSE '/' || COALESCE(
             NULLIF(json_extract(s.content_type_bases_json, '$."' || t.key || '"'), ''),
             (SELECT d.rewrite_base FROM provider_rewrite_defaults d
               WHERE d.resource_kind = 'content_type' AND d.resource_key = t.key
                 AND d.provider_status = 'active' AND d.declaration_state = 'declared'
                 AND json_type(s.content_type_bases_json, '$."' || t.key || '"') IS NULL),
             t.key
         )
       END,
       'archive',
       t.key
  FROM content_types t
  JOIN site_permalink_settings s ON s.id = 1
 WHERE t.key NOT IN ('pattern', 'section', 'page')
   AND t.provider_status = 'active'
   AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
   AND COALESCE(json_extract(t.definition_json, '$.settings.has_archive'), 1) = 1;

-- 3. Public active taxonomy terms: /{base|kind}/{slug}
INSERT INTO permalink_route_claims (path, target_kind, target_id)
SELECT '/' || COALESCE(
           NULLIF(json_extract(s.taxonomy_bases_json, '$."' || tm.kind || '"'), ''),
           (SELECT d.rewrite_base FROM provider_rewrite_defaults d
             WHERE d.resource_kind = 'taxonomy' AND d.resource_key = tm.kind
               AND d.provider_status = 'active' AND d.declaration_state = 'declared'
               AND json_type(s.taxonomy_bases_json, '$."' || tm.kind || '"') IS NULL),
           tm.kind
       ) || '/' || tm.slug,
       'term',
       CAST(tm.id AS TEXT)
  FROM taxonomy_terms tm
  JOIN taxonomy_definitions td ON td.key = tm.kind
  JOIN site_permalink_settings s ON s.id = 1
 WHERE td.public = 1 AND td.provider_status = 'active' AND td.declaration_state = 'declared';

-- 3b. Dọn entry_public_paths cũ (trừ posts_page_id) nếu trùng claim Home/Archive/Term (Steps 1–3),
-- trùng path của một post cố định theo %post_id%, hoặc trùng đường dẫn mặc định của một CPT công khai active khác
DELETE FROM entry_public_paths
 WHERE entry_id IS NOT (SELECT posts_page_id FROM site_reading_settings WHERE id = 1)
   AND (
       EXISTS (
           SELECT 1 FROM permalink_route_claims c
            WHERE c.path = entry_public_paths.path
       )
       OR EXISTS (
           SELECT 1
             FROM content_entries pe
             JOIN site_permalink_settings s ON s.id = 1
             LEFT JOIN site_reading_settings r ON r.id = 1
            WHERE pe.id != entry_public_paths.entry_id
              AND pe.content_type_key = 'post'
              AND s.post_structure NOT LIKE '%postname%'
              AND pe.id IS NOT r.posts_page_id
              AND entry_public_paths.path = REPLACE(s.post_structure, '%post_id%', CAST(pe.id AS TEXT))
       )
       OR EXISTS (
           SELECT 1
             FROM content_entries cpt
             JOIN content_types ct ON ct.key = cpt.content_type_key
             JOIN site_permalink_settings s ON s.id = 1
            WHERE cpt.id != entry_public_paths.entry_id
              AND cpt.content_type_key NOT IN ('pattern', 'section', 'page', 'post')
              AND ct.provider_status = 'active'
              AND COALESCE(json_extract(ct.definition_json, '$.settings.public'), 1) = 1
              AND entry_public_paths.path = '/' || COALESCE(
                  NULLIF(json_extract(s.content_type_bases_json, '$."' || cpt.content_type_key || '"'), ''),
                  (SELECT d.rewrite_base FROM provider_rewrite_defaults d
                    WHERE d.resource_kind = 'content_type' AND d.resource_key = cpt.content_type_key
                      AND d.provider_status = 'active' AND d.declaration_state = 'declared'
                      AND json_type(s.content_type_bases_json, '$."' || cpt.content_type_key || '"') IS NULL),
                  cpt.content_type_key
              ) || '/' || cpt.slug
       )
   );

-- 4. Disambiguate content_entries.slug trên DB nâng cấp (ưu tiên post cố định theo %post_id% > page > post theo %postname%)
WITH post_paths AS (
    SELECT e.id,
           e.content_type_key,
           e.slug,
           CASE
             WHEN e.content_type_key = 'page' OR s.post_structure LIKE '%postname%' THEN 1
             ELSE 0
           END AS can_rename_path,
           CASE
             WHEN e.content_type_key = 'page' THEN '/' || e.slug
             ELSE REPLACE(REPLACE(s.post_structure, '%postname%', e.slug), '%post_id%', CAST(e.id AS TEXT))
           END AS proposed_path
      FROM content_entries e
      JOIN content_types t ON t.key = e.content_type_key
      JOIN site_permalink_settings s ON s.id = 1
      LEFT JOIN entry_public_paths p ON p.entry_id = e.id
      LEFT JOIN site_reading_settings r ON r.id = 1
     WHERE e.content_type_key IN ('page', 'post')
       AND t.provider_status = 'active'
       AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
       AND p.entry_id IS NULL
       AND e.id IS NOT r.homepage_page_id
       AND e.id IS NOT r.posts_page_id
)
UPDATE content_entries
   SET slug = CASE
         WHEN NOT EXISTS (
             SELECT 1 FROM content_entries dup
              WHERE (dup.content_type_key = content_entries.content_type_key OR dup.content_type_key IN ('page', 'post'))
                AND dup.slug = RTRIM(SUBSTR(content_entries.slug, 1, MAX(1, 63 - LENGTH(CAST(content_entries.id AS TEXT)))), '-') || '-' || CAST(content_entries.id AS TEXT)
         )
         AND NOT EXISTS (
             SELECT 1 FROM entry_public_paths epp
              WHERE epp.entry_id != content_entries.id
                AND (
                    epp.path = '/' || RTRIM(SUBSTR(content_entries.slug, 1, MAX(1, 63 - LENGTH(CAST(content_entries.id AS TEXT)))), '-') || '-' || CAST(content_entries.id AS TEXT)
                    OR (
                        content_entries.content_type_key = 'post'
                        AND epp.path = REPLACE(
                            REPLACE((SELECT post_structure FROM site_permalink_settings WHERE id = 1), '%postname%', RTRIM(SUBSTR(content_entries.slug, 1, MAX(1, 63 - LENGTH(CAST(content_entries.id AS TEXT)))), '-') || '-' || CAST(content_entries.id AS TEXT)),
                            '%post_id%', CAST(content_entries.id AS TEXT)
                        )
                    )
                )
         )
         THEN RTRIM(SUBSTR(content_entries.slug, 1, MAX(1, 63 - LENGTH(CAST(content_entries.id AS TEXT)))), '-') || '-' || CAST(content_entries.id AS TEXT)
         ELSE RTRIM(SUBSTR(content_entries.slug, 1, MAX(1, 62 - LENGTH(CAST(content_entries.id AS TEXT)))), '-') || '-u' || CAST(content_entries.id AS TEXT)
       END
 WHERE id IN (
       SELECT c1.id FROM post_paths c1
        WHERE c1.can_rename_path = 1
          AND (
              (
                  c1.proposed_path = '/' || c1.slug
                  AND c1.slug IN (
                      'admin','api','assets','auth','builder','content','favicon.ico','forms',
                      'health','media','page','pattern','preview','registry','robots.txt',
                      'save','section','sitemap.xml','sitemaps','static','studio','taxonomy','theme','themes','uploads'
                  )
              )
              OR EXISTS (SELECT 1 FROM permalink_route_claims c WHERE c.path = c1.proposed_path)
              OR EXISTS (
                  SELECT 1
                    FROM entry_public_paths epp
                    JOIN content_entries ee ON ee.id = epp.entry_id
                    JOIN content_types et ON et.key = ee.content_type_key
                    LEFT JOIN site_reading_settings r2 ON r2.id = 1
                   WHERE epp.path = c1.proposed_path
                     AND ee.id != c1.id
                     AND ee.id IS NOT r2.homepage_page_id
                     AND ee.id IS NOT r2.posts_page_id
                     AND et.key NOT IN ('pattern', 'section')
                     AND et.provider_status = 'active'
                     AND COALESCE(json_extract(et.definition_json, '$.settings.public'), 1) = 1
              )
              OR EXISTS (
                  SELECT 1 FROM post_paths c2
                   WHERE c2.proposed_path = c1.proposed_path
                     AND (
                         c2.can_rename_path = 0
                         OR (
                             c2.can_rename_path = 1
                             AND (
                                 (c2.content_type_key = 'page' AND c1.content_type_key != 'page')
                                 OR (c2.content_type_key = c1.content_type_key AND c2.id < c1.id)
                             )
                         )
                     )
              )
          )
   );

-- 5. Public active Entries (bỏ qua pattern, section, public=0, inactive, homepage_page_id và posts_page_id;
-- Step 3b & Step 4 đã loại bỏ mọi va chạm giữa p.path / CPT default / %post_id% / page-post slug)
INSERT INTO permalink_route_claims (path, target_kind, target_id)
SELECT COALESCE(
           p.path,
           CASE
             WHEN e.content_type_key = 'page' THEN '/' || e.slug
             WHEN e.content_type_key = 'post' THEN
               REPLACE(REPLACE(s.post_structure, '%postname%', e.slug), '%post_id%', CAST(e.id AS TEXT))
             ELSE '/' || COALESCE(
                 NULLIF(json_extract(s.content_type_bases_json, '$."' || e.content_type_key || '"'), ''),
                 (SELECT d.rewrite_base FROM provider_rewrite_defaults d
                   WHERE d.resource_kind = 'content_type' AND d.resource_key = e.content_type_key
                     AND d.provider_status = 'active' AND d.declaration_state = 'declared'
                     AND json_type(s.content_type_bases_json, '$."' || e.content_type_key || '"') IS NULL),
                 e.content_type_key
             ) || '/' || e.slug
           END
       ),
       'entry',
       CAST(e.id AS TEXT)
  FROM content_entries e
  JOIN content_types t ON t.key = e.content_type_key
  JOIN site_permalink_settings s ON s.id = 1
  LEFT JOIN entry_public_paths p ON p.entry_id = e.id
  LEFT JOIN site_reading_settings r ON r.id = 1
 WHERE e.content_type_key NOT IN ('pattern', 'section')
   AND t.provider_status = 'active'
   AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
   AND e.id IS NOT r.homepage_page_id
   AND e.id IS NOT r.posts_page_id;

-- 6. Loại bỏ bất kỳ redirect cũ nào trùng với canonical claim mới
DELETE FROM permalink_route_redirects
 WHERE EXISTS (
     SELECT 1 FROM permalink_route_claims c
      WHERE c.path = permalink_route_redirects.source_path
 );
