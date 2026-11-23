-- 0047_multilingual_content_and_theme_l10n.sql
-- Đa ngôn ngữ Nội dung Lớp 1 (Entity-per-Locale + translation_group_id + cấu hình theo locale)
-- và Bản địa hóa Theme v3 Lớp 2 (theme_option_locale_values / versions).
-- Bảo toàn migrations 0001..0046 bất biến và rebuild topo trong 1 transaction dưới PRAGMA foreign_keys = ON.

-- 1. Danh mục ngôn ngữ & Collection CAS / cờ rename_in_progress
CREATE TABLE site_locales (
    code TEXT PRIMARY KEY NOT NULL CHECK (
        length(code) BETWEEN 2 AND 35
        AND code = lower(code)
        AND code GLOB '[a-z]*'
        AND code NOT GLOB '*[^a-z0-9-]*'
    ),
    bcp47_tag TEXT NOT NULL CHECK (length(bcp47_tag) BETWEEN 2 AND 35 AND lower(bcp47_tag) = code),
    label TEXT NOT NULL CHECK (length(label) BETWEEN 1 AND 80),
    direction TEXT NOT NULL DEFAULT 'ltr' CHECK (direction IN ('ltr', 'rtl')),
    is_default INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    position INTEGER NOT NULL DEFAULT 0,
    CHECK (is_default = 0 OR enabled = 1)
);
CREATE UNIQUE INDEX idx_site_locales_single_default ON site_locales(is_default) WHERE is_default = 1;
CREATE INDEX idx_site_locales_enabled_pos ON site_locales(enabled, position, code);

CREATE TABLE site_locales_state (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    rename_in_progress INTEGER NOT NULL DEFAULT 0 CHECK (rename_in_progress IN (0, 1))
);
INSERT INTO site_locales_state (id, version, rename_in_progress) VALUES (1, 1, 0);

INSERT INTO site_locales (code, bcp47_tag, label, direction, is_default, enabled, position)
SELECT lower(content_language),
       content_language,
       CASE lower(content_language)
           WHEN 'vi' THEN 'Tiếng Việt'
           WHEN 'en' THEN 'English'
           ELSE content_language
       END,
       'ltr',
       1,
       1,
       0
  FROM site_public_settings
 WHERE id = 1;

-- 2. Sao lưu sqlite_sequence cho taxonomy_terms và content_entries
CREATE TABLE _seq_0047 (
    tbl TEXT PRIMARY KEY NOT NULL,
    seq INTEGER NOT NULL
);
INSERT INTO _seq_0047 (tbl, seq)
SELECT name, MAX(seq)
  FROM sqlite_sequence
 WHERE name IN ('taxonomy_terms', 'content_entries')
 GROUP BY name;

-- 3. Tạo và nạp 10 bảng *_0047 theo thứ tự topo (trước khi tạo triggers)
CREATE TABLE taxonomy_terms_0047 (
    id INTEGER PRIMARY KEY AUTOINCREMENT CHECK (id > 0),
    kind TEXT NOT NULL REFERENCES taxonomy_definitions(key) ON DELETE RESTRICT,
    locale TEXT NOT NULL REFERENCES site_locales(code) ON UPDATE CASCADE ON DELETE RESTRICT,
    translation_group_id TEXT NOT NULL CHECK (length(translation_group_id) BETWEEN 1 AND 80),
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 120),
    slug TEXT NOT NULL CHECK (
        length(slug) BETWEEN 1 AND 64
        AND slug GLOB '[a-z]*'
        AND slug NOT GLOB '*[^a-z0-9-]*'
    ),
    parent_id INTEGER REFERENCES taxonomy_terms_0047(id) ON DELETE RESTRICT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    UNIQUE (kind, locale, slug),
    UNIQUE (kind, translation_group_id, locale)
);
INSERT INTO taxonomy_terms_0047 (id, kind, locale, translation_group_id, name, slug, parent_id, version)
SELECT id,
       kind,
       (SELECT code FROM site_locales WHERE is_default = 1),
       'tg-' || CAST(id AS TEXT),
       name,
       slug,
       NULL,
       version
  FROM taxonomy_terms
 ORDER BY id ASC;
UPDATE taxonomy_terms_0047
   SET parent_id = (SELECT old.parent_id FROM taxonomy_terms old WHERE old.id = taxonomy_terms_0047.id)
 WHERE EXISTS (SELECT 1 FROM taxonomy_terms old WHERE old.id = taxonomy_terms_0047.id AND old.parent_id IS NOT NULL);

CREATE TABLE content_entries_0047 (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    content_type_key TEXT NOT NULL REFERENCES content_types(key)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    locale TEXT NOT NULL REFERENCES site_locales(code) ON UPDATE CASCADE ON DELETE RESTRICT,
    translation_group_id TEXT NOT NULL CHECK (length(translation_group_id) BETWEEN 1 AND 80),
    slug TEXT NOT NULL CHECK (length(slug) BETWEEN 1 AND 64),
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
    values_json TEXT NOT NULL
        CHECK (json_valid(values_json) AND json_type(values_json) = 'object'),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    publication_status TEXT NOT NULL DEFAULT 'published'
        CHECK (publication_status IN ('draft', 'published')),
    page_layout_key TEXT NULL,
    custom_template_key TEXT NULL,
    content_mode TEXT NOT NULL DEFAULT 'editor'
        CHECK (content_mode IN ('editor', 'builder')),
    primary_category_term_id INTEGER REFERENCES taxonomy_terms_0047(id) ON DELETE SET NULL,
    has_been_published INTEGER NOT NULL DEFAULT 0 CHECK (has_been_published IN (0, 1)),
    UNIQUE (content_type_key, locale, slug),
    UNIQUE (content_type_key, translation_group_id, locale)
);
INSERT INTO content_entries_0047 (
    id, content_type_key, locale, translation_group_id, slug, title, values_json,
    version, created_at, updated_at, publication_status, page_layout_key,
    custom_template_key, content_mode, primary_category_term_id, has_been_published
)
SELECT id,
       content_type_key,
       (SELECT code FROM site_locales WHERE is_default = 1),
       'eg-' || CAST(id AS TEXT),
       slug,
       title,
       values_json,
       version,
       created_at,
       updated_at,
       publication_status,
       page_layout_key,
       custom_template_key,
       content_mode,
       primary_category_term_id,
       has_been_published
  FROM content_entries
 ORDER BY id ASC;

CREATE TABLE entry_terms_0047 (
    entry_id INTEGER NOT NULL REFERENCES content_entries_0047(id) ON DELETE CASCADE,
    term_id INTEGER NOT NULL REFERENCES taxonomy_terms_0047(id) ON DELETE RESTRICT,
    PRIMARY KEY (entry_id, term_id)
);
INSERT INTO entry_terms_0047 (entry_id, term_id)
SELECT entry_id, term_id FROM entry_terms;

CREATE TABLE term_extension_values_0047 (
    term_id INTEGER NOT NULL REFERENCES taxonomy_terms_0047(id) ON DELETE CASCADE,
    owner_kind TEXT NOT NULL CHECK (owner_kind IN ('theme','plugin')),
    owner_id TEXT NOT NULL,
    local_key TEXT NOT NULL,
    values_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(values_json)),
    PRIMARY KEY (term_id, owner_kind, owner_id, local_key)
);
INSERT INTO term_extension_values_0047 (term_id, owner_kind, owner_id, local_key, values_json)
SELECT term_id, owner_kind, owner_id, local_key, values_json FROM term_extension_values;

CREATE TABLE entry_public_paths_0047 (
    entry_id INTEGER PRIMARY KEY REFERENCES content_entries_0047(id) ON DELETE CASCADE,
    locale TEXT NOT NULL REFERENCES site_locales(code) ON UPDATE CASCADE ON DELETE RESTRICT,
    path TEXT NOT NULL,
    UNIQUE (locale, path)
);
INSERT INTO entry_public_paths_0047 (entry_id, locale, path)
SELECT p.entry_id, e.locale, p.path
  FROM entry_public_paths p
  JOIN content_entries_0047 e ON e.id = p.entry_id;

CREATE TABLE entry_redirects_0047 (
    locale TEXT NOT NULL REFERENCES site_locales(code) ON UPDATE CASCADE ON DELETE RESTRICT,
    source_path TEXT NOT NULL,
    entry_id INTEGER NOT NULL REFERENCES content_entries_0047(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (locale, source_path)
);
INSERT INTO entry_redirects_0047 (locale, source_path, entry_id, created_at)
SELECT e.locale, r.source_path, r.entry_id, r.created_at
  FROM entry_redirects r
  JOIN content_entries_0047 e ON e.id = r.entry_id;

CREATE TABLE entry_seo_0047 (
    entry_id INTEGER PRIMARY KEY REFERENCES content_entries_0047(id) ON DELETE CASCADE,
    title TEXT,
    description TEXT,
    og_image_id TEXT REFERENCES media_assets(id) ON DELETE SET NULL,
    canonical_url TEXT,
    noindex INTEGER NOT NULL DEFAULT 0 CHECK (noindex IN (0, 1))
);
INSERT INTO entry_seo_0047 (entry_id, title, description, og_image_id, canonical_url, noindex)
SELECT entry_id, title, description, og_image_id, canonical_url, noindex FROM entry_seo;

CREATE TABLE entry_builder_documents_0047 (
    entry_id INTEGER PRIMARY KEY REFERENCES content_entries_0047(id) ON DELETE CASCADE,
    document_json TEXT NOT NULL CHECK (json_valid(document_json)),
    updated_at TEXT NOT NULL
);
INSERT INTO entry_builder_documents_0047 (entry_id, document_json, updated_at)
SELECT entry_id, document_json, updated_at FROM entry_builder_documents;

CREATE TABLE entry_builder_media_refs_0047 (
    entry_id INTEGER NOT NULL REFERENCES content_entries_0047(id) ON DELETE CASCADE,
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
    ref_role TEXT NOT NULL CHECK (ref_role IN ('image', 'video', 'poster')),
    PRIMARY KEY (entry_id, media_id, ref_role)
);
INSERT INTO entry_builder_media_refs_0047 (entry_id, media_id, ref_role)
SELECT entry_id, media_id, ref_role FROM entry_builder_media_refs;

CREATE TABLE entry_pattern_refs_0047 (
    source_entry_id INTEGER NOT NULL REFERENCES content_entries_0047(id) ON DELETE CASCADE,
    target_pattern_id INTEGER NOT NULL REFERENCES content_entries_0047(id) ON DELETE CASCADE,
    PRIMARY KEY (source_entry_id, target_pattern_id)
);
INSERT INTO entry_pattern_refs_0047 (source_entry_id, target_pattern_id)
SELECT source_entry_id, target_pattern_id FROM entry_pattern_refs;

-- 4. Xóa 10 bảng cũ theo thứ tự phụ thuộc FK an toàn
DROP TABLE entry_pattern_refs;
DROP TABLE entry_builder_media_refs;
DROP TABLE entry_builder_documents;
DROP TABLE entry_seo;
DROP TABLE entry_redirects;
DROP TABLE entry_public_paths;
DROP TABLE term_extension_values;
DROP TABLE entry_terms;
DROP TABLE content_entries;
UPDATE taxonomy_terms SET parent_id = NULL WHERE parent_id IS NOT NULL;
DROP TABLE taxonomy_terms;

-- 5. Đổi tên 10 bảng *_0047 về tên chuẩn
ALTER TABLE taxonomy_terms_0047 RENAME TO taxonomy_terms;
ALTER TABLE content_entries_0047 RENAME TO content_entries;
ALTER TABLE entry_terms_0047 RENAME TO entry_terms;
ALTER TABLE term_extension_values_0047 RENAME TO term_extension_values;
ALTER TABLE entry_public_paths_0047 RENAME TO entry_public_paths;
ALTER TABLE entry_redirects_0047 RENAME TO entry_redirects;
ALTER TABLE entry_seo_0047 RENAME TO entry_seo;
ALTER TABLE entry_builder_documents_0047 RENAME TO entry_builder_documents;
ALTER TABLE entry_builder_media_refs_0047 RENAME TO entry_builder_media_refs;
ALTER TABLE entry_pattern_refs_0047 RENAME TO entry_pattern_refs;

-- 6. Phục hồi sqlite_sequence cho taxonomy_terms và content_entries
DELETE FROM sqlite_sequence WHERE name IN ('taxonomy_terms', 'content_entries');
INSERT INTO sqlite_sequence (name, seq)
SELECT 'taxonomy_terms', MAX(
    COALESCE((SELECT seq FROM _seq_0047 WHERE tbl = 'taxonomy_terms'), 0),
    (SELECT COALESCE(MAX(id), 0) FROM taxonomy_terms)
)
WHERE EXISTS (SELECT 1 FROM _seq_0047 WHERE tbl = 'taxonomy_terms')
   OR (SELECT COALESCE(MAX(id), 0) FROM taxonomy_terms) > 0;
INSERT INTO sqlite_sequence (name, seq)
SELECT 'content_entries', MAX(
    COALESCE((SELECT seq FROM _seq_0047 WHERE tbl = 'content_entries'), 0),
    (SELECT COALESCE(MAX(id), 0) FROM content_entries)
)
WHERE EXISTS (SELECT 1 FROM _seq_0047 WHERE tbl = 'content_entries')
   OR (SELECT COALESCE(MAX(id), 0) FROM content_entries) > 0;
DROP TABLE _seq_0047;

-- 7. Tạo mới & rebuild in-place các bảng cấu hình, menu slots và permalinks
CREATE TABLE site_general_settings_locales (
    locale TEXT PRIMARY KEY NOT NULL REFERENCES site_locales(code) ON UPDATE CASCADE ON DELETE RESTRICT,
    site_title TEXT NOT NULL CHECK (length(site_title) BETWEEN 1 AND 120),
    tagline TEXT NOT NULL CHECK (length(tagline) <= 240),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1)
);
INSERT INTO site_general_settings_locales (locale, site_title, tagline, version)
SELECT (SELECT code FROM site_locales WHERE is_default = 1), site_title, tagline, version
  FROM site_general_settings
 WHERE id = 1;

CREATE TABLE site_reading_settings_0047 (
    locale TEXT PRIMARY KEY NOT NULL REFERENCES site_locales(code) ON UPDATE CASCADE ON DELETE RESTRICT,
    homepage_page_id INTEGER NULL,
    posts_page_id INTEGER NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (homepage_page_id IS NULL OR homepage_page_id > 0),
    CHECK (posts_page_id IS NULL OR posts_page_id > 0),
    CHECK (homepage_page_id IS NULL OR posts_page_id IS NULL OR homepage_page_id != posts_page_id)
);
INSERT INTO site_reading_settings_0047 (locale, homepage_page_id, posts_page_id, version, updated_at)
SELECT (SELECT code FROM site_locales WHERE is_default = 1), homepage_page_id, posts_page_id, version, updated_at
  FROM site_reading_settings
 WHERE id = 1;
DROP TABLE site_reading_settings;
ALTER TABLE site_reading_settings_0047 RENAME TO site_reading_settings;

CREATE TABLE site_home_seo_0047 (
    locale TEXT PRIMARY KEY NOT NULL REFERENCES site_locales(code) ON UPDATE CASCADE ON DELETE RESTRICT,
    title TEXT,
    description TEXT,
    og_image_id TEXT REFERENCES media_assets(id) ON DELETE SET NULL,
    canonical_url TEXT,
    noindex INTEGER NOT NULL DEFAULT 0 CHECK (noindex IN (0, 1)),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1)
);
INSERT INTO site_home_seo_0047 (locale, title, description, og_image_id, canonical_url, noindex, version)
SELECT (SELECT code FROM site_locales WHERE is_default = 1), title, description, og_image_id, canonical_url, noindex, version
  FROM site_home_seo
 WHERE id = 1;
DROP TABLE site_home_seo;
ALTER TABLE site_home_seo_0047 RENAME TO site_home_seo;
DELETE FROM seo_public_media_refs WHERE source_kind = 'home_seo';

CREATE TABLE theme_option_locale_values (
    theme_id TEXT NOT NULL,
    locale TEXT NOT NULL REFERENCES site_locales(code) ON UPDATE CASCADE ON DELETE RESTRICT,
    option_key TEXT NOT NULL,
    value_json TEXT NOT NULL CHECK (json_valid(value_json)),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (theme_id, locale, option_key)
);

CREATE TABLE theme_option_locale_versions (
    theme_id TEXT NOT NULL,
    locale TEXT NOT NULL REFERENCES site_locales(code) ON UPDATE CASCADE ON DELETE RESTRICT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (theme_id, locale)
);

CREATE TABLE site_menu_slot_assignments_0047 (
    theme_id TEXT NOT NULL,
    locale TEXT NOT NULL REFERENCES site_locales(code) ON UPDATE CASCADE ON DELETE RESTRICT,
    slot_key TEXT NOT NULL,
    menu_id INTEGER NOT NULL REFERENCES site_menus(id) ON DELETE CASCADE,
    PRIMARY KEY (theme_id, locale, slot_key)
);
INSERT INTO site_menu_slot_assignments_0047 (theme_id, locale, slot_key, menu_id)
SELECT theme_id, (SELECT code FROM site_locales WHERE is_default = 1), slot_key, menu_id
  FROM site_menu_slot_assignments;
DROP TABLE site_menu_slot_assignments;
ALTER TABLE site_menu_slot_assignments_0047 RENAME TO site_menu_slot_assignments;

CREATE TABLE site_menu_slot_versions_0047 (
    theme_id TEXT NOT NULL,
    locale TEXT NOT NULL REFERENCES site_locales(code) ON UPDATE CASCADE ON DELETE RESTRICT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    PRIMARY KEY (theme_id, locale)
);
INSERT INTO site_menu_slot_versions_0047 (theme_id, locale, version)
SELECT theme_id, (SELECT code FROM site_locales WHERE is_default = 1), version
  FROM site_menu_slot_versions;
DROP TABLE site_menu_slot_versions;
ALTER TABLE site_menu_slot_versions_0047 RENAME TO site_menu_slot_versions;

CREATE TABLE permalink_route_claims_0047 (
    path TEXT PRIMARY KEY COLLATE BINARY NOT NULL,
    target_kind TEXT NOT NULL CHECK (target_kind IN ('home', 'entry', 'term', 'archive')),
    target_id TEXT NOT NULL COLLATE BINARY,
    locale TEXT NOT NULL REFERENCES site_locales(code) ON UPDATE CASCADE ON DELETE RESTRICT,
    UNIQUE (target_kind, target_id, locale)
);
INSERT INTO permalink_route_claims_0047 (path, target_kind, target_id, locale)
SELECT path, target_kind, target_id, (SELECT code FROM site_locales WHERE is_default = 1)
  FROM permalink_route_claims;
DROP TABLE permalink_route_claims;
ALTER TABLE permalink_route_claims_0047 RENAME TO permalink_route_claims;

CREATE TABLE permalink_route_redirects_0047 (
    source_path TEXT PRIMARY KEY COLLATE BINARY NOT NULL,
    target_kind TEXT NOT NULL CHECK (target_kind IN ('home', 'entry', 'term', 'archive')),
    target_id TEXT NOT NULL COLLATE BINARY,
    locale TEXT NOT NULL REFERENCES site_locales(code) ON UPDATE CASCADE ON DELETE RESTRICT,
    origin TEXT NOT NULL DEFAULT 'canonical' CHECK (origin IN ('canonical', 'locale_swap')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
INSERT INTO permalink_route_redirects_0047 (source_path, target_kind, target_id, locale, origin, created_at)
SELECT source_path, target_kind, target_id, (SELECT code FROM site_locales WHERE is_default = 1), 'canonical', created_at
  FROM permalink_route_redirects;
DROP TABLE permalink_route_redirects;
ALTER TABLE permalink_route_redirects_0047 RENAME TO permalink_route_redirects;

-- Tái tạo đầy đủ indexes cũ & indexes theo locale / translation_group_id
CREATE INDEX idx_content_entries_type_id ON content_entries (content_type_key, id DESC);
CREATE INDEX idx_content_entries_published_type_id ON content_entries (content_type_key, publication_status, id DESC);
CREATE INDEX idx_content_entries_public_builder_mode ON content_entries(content_type_key, publication_status, content_mode);
CREATE INDEX idx_content_entries_primary_category ON content_entries(primary_category_term_id) WHERE primary_category_term_id IS NOT NULL;
CREATE INDEX idx_content_entries_type_locale_id ON content_entries (content_type_key, locale, id DESC);
CREATE INDEX idx_content_entries_group ON content_entries (content_type_key, translation_group_id);

CREATE INDEX idx_taxonomy_terms_kind_name_id ON taxonomy_terms (kind, name COLLATE BINARY, id);
CREATE INDEX idx_taxonomy_terms_parent_id ON taxonomy_terms (parent_id);
CREATE INDEX idx_taxonomy_terms_kind_locale_name_id ON taxonomy_terms (kind, locale, name COLLATE BINARY, id);
CREATE INDEX idx_taxonomy_terms_group ON taxonomy_terms (kind, translation_group_id);

CREATE INDEX idx_entry_terms_term_entry ON entry_terms (term_id, entry_id);
CREATE INDEX term_extension_values_owner ON term_extension_values (owner_kind, owner_id, local_key);
CREATE INDEX idx_entry_builder_media_refs_media ON entry_builder_media_refs(media_id);
CREATE INDEX idx_entry_pattern_refs_target ON entry_pattern_refs(target_pattern_id);
CREATE INDEX idx_entry_redirects_entry_id ON entry_redirects (entry_id);
CREATE INDEX idx_permalink_route_claim_target ON permalink_route_claims (target_kind, target_id, locale);
CREATE INDEX idx_permalink_route_redirect_target ON permalink_route_redirects (target_kind, target_id, locale);

-- 8. Tái tạo triggers cũ + 10 triggers toàn vẹn locale
CREATE TRIGGER content_entries_core_insert_draft
    BEFORE INSERT ON content_entries
    WHEN NEW.content_type_key IN ('post', 'page')
         AND NEW.publication_status <> 'draft'
BEGIN
    SELECT RAISE(ABORT, 'Post/Page entries must start as draft');
END;

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

CREATE TRIGGER taxonomy_terms_parent_insert
BEFORE INSERT ON taxonomy_terms
WHEN NEW.parent_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'invalid_taxonomy_parent')
    WHERE NOT EXISTS (
        SELECT 1 FROM taxonomy_terms p JOIN taxonomy_definitions d ON d.key = NEW.kind
        WHERE p.id = NEW.parent_id AND p.kind = NEW.kind AND p.locale = NEW.locale AND d.hierarchical = 1
    );
END;

CREATE TRIGGER taxonomy_terms_parent_update
BEFORE UPDATE OF parent_id, kind, locale ON taxonomy_terms
WHEN NEW.parent_id IS NOT NULL
     AND (SELECT rename_in_progress FROM site_locales_state WHERE id = 1) = 0
BEGIN
    SELECT RAISE(ABORT, 'invalid_taxonomy_parent')
    WHERE NOT EXISTS (
        SELECT 1 FROM taxonomy_terms p JOIN taxonomy_definitions d ON d.key = NEW.kind
        WHERE p.id = NEW.parent_id AND p.kind = NEW.kind AND p.locale = NEW.locale AND d.hierarchical = 1
    );
END;

CREATE TRIGGER taxonomy_terms_locale_guard
BEFORE UPDATE OF locale ON taxonomy_terms
WHEN OLD.locale != NEW.locale
     AND (SELECT rename_in_progress FROM site_locales_state WHERE id = 1) = 0
BEGIN
    SELECT RAISE(ABORT, 'taxonomy_term_locale_locked')
    WHERE (SELECT COUNT(*) FROM site_locales) > 1
       OR (NEW.parent_id IS NOT NULL AND EXISTS (SELECT 1 FROM taxonomy_terms p WHERE p.id = NEW.parent_id AND p.locale != NEW.locale))
       OR EXISTS (SELECT 1 FROM taxonomy_terms c WHERE c.parent_id = NEW.id AND c.locale != NEW.locale)
       OR EXISTS (SELECT 1 FROM entry_terms et JOIN content_entries e ON e.id = et.entry_id WHERE et.term_id = NEW.id AND e.locale != NEW.locale)
       OR EXISTS (SELECT 1 FROM content_entries e WHERE e.primary_category_term_id = NEW.id AND e.locale != NEW.locale);
END;

CREATE TRIGGER content_entries_locale_guard
BEFORE UPDATE OF locale ON content_entries
WHEN OLD.locale != NEW.locale
     AND (SELECT rename_in_progress FROM site_locales_state WHERE id = 1) = 0
BEGIN
    SELECT RAISE(ABORT, 'content_entry_locale_locked')
    WHERE (SELECT COUNT(*) FROM site_locales) > 1
       OR EXISTS (SELECT 1 FROM entry_terms et JOIN taxonomy_terms t ON t.id = et.term_id WHERE et.entry_id = NEW.id AND t.locale != NEW.locale)
       OR (NEW.primary_category_term_id IS NOT NULL AND EXISTS (SELECT 1 FROM taxonomy_terms t WHERE t.id = NEW.primary_category_term_id AND t.locale != NEW.locale))
       OR EXISTS (SELECT 1 FROM site_reading_settings r WHERE (r.homepage_page_id = NEW.id OR r.posts_page_id = NEW.id) AND r.locale != NEW.locale)
       OR EXISTS (SELECT 1 FROM entry_public_paths p WHERE p.entry_id = NEW.id AND p.locale != NEW.locale)
       OR EXISTS (SELECT 1 FROM entry_redirects er WHERE er.entry_id = NEW.id AND er.locale != NEW.locale);
END;

CREATE TRIGGER content_entries_primary_cat_locale_insert
BEFORE INSERT ON content_entries
WHEN NEW.primary_category_term_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'primary_category_locale_mismatch')
    WHERE COALESCE((SELECT locale FROM taxonomy_terms WHERE id = NEW.primary_category_term_id), NEW.locale) != NEW.locale;
END;

CREATE TRIGGER content_entries_primary_cat_locale_update
BEFORE UPDATE OF primary_category_term_id, locale ON content_entries
WHEN NEW.primary_category_term_id IS NOT NULL
     AND (SELECT rename_in_progress FROM site_locales_state WHERE id = 1) = 0
BEGIN
    SELECT RAISE(ABORT, 'primary_category_locale_mismatch')
    WHERE COALESCE((SELECT locale FROM taxonomy_terms WHERE id = NEW.primary_category_term_id), NEW.locale) != NEW.locale;
END;

CREATE TRIGGER entry_terms_locale_insert
BEFORE INSERT ON entry_terms
BEGIN
    SELECT RAISE(ABORT, 'entry_term_locale_mismatch')
    WHERE (SELECT locale FROM content_entries WHERE id = NEW.entry_id) != (SELECT locale FROM taxonomy_terms WHERE id = NEW.term_id);
END;

CREATE TRIGGER entry_terms_locale_update
BEFORE UPDATE ON entry_terms
WHEN (SELECT rename_in_progress FROM site_locales_state WHERE id = 1) = 0
BEGIN
    SELECT RAISE(ABORT, 'entry_term_locale_mismatch')
    WHERE (SELECT locale FROM content_entries WHERE id = NEW.entry_id) != (SELECT locale FROM taxonomy_terms WHERE id = NEW.term_id);
END;

CREATE TRIGGER entry_public_paths_locale_insert
BEFORE INSERT ON entry_public_paths
BEGIN
    SELECT RAISE(ABORT, 'entry_public_path_locale_mismatch')
    WHERE NEW.locale != (SELECT locale FROM content_entries WHERE id = NEW.entry_id);
END;

CREATE TRIGGER entry_public_paths_locale_update
BEFORE UPDATE OF entry_id, locale ON entry_public_paths
WHEN (SELECT rename_in_progress FROM site_locales_state WHERE id = 1) = 0
BEGIN
    SELECT RAISE(ABORT, 'entry_public_path_locale_mismatch')
    WHERE NEW.locale != (SELECT locale FROM content_entries WHERE id = NEW.entry_id);
END;

CREATE TRIGGER entry_redirects_locale_insert
BEFORE INSERT ON entry_redirects
BEGIN
    SELECT RAISE(ABORT, 'entry_redirect_locale_mismatch')
    WHERE NEW.locale != (SELECT locale FROM content_entries WHERE id = NEW.entry_id);
END;

CREATE TRIGGER entry_redirects_locale_update
BEFORE UPDATE OF entry_id, locale ON entry_redirects
WHEN (SELECT rename_in_progress FROM site_locales_state WHERE id = 1) = 0
BEGIN
    SELECT RAISE(ABORT, 'entry_redirect_locale_mismatch')
    WHERE NEW.locale != (SELECT locale FROM content_entries WHERE id = NEW.entry_id);
END;
