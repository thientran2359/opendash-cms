-- M4-S12-T01 / #374: one canonical taxonomy registry for all term kinds.
-- 0012, 0014 and 0036 are immutable; rebuild their dependent tables within
-- the SQLx migration transaction without disabling SQLite foreign keys.
CREATE TABLE taxonomy_definitions (
    key TEXT PRIMARY KEY COLLATE BINARY NOT NULL,
    label TEXT NOT NULL CHECK (length(label) BETWEEN 1 AND 100),
    hierarchical INTEGER NOT NULL DEFAULT 0 CHECK (hierarchical IN (0, 1)),
    public INTEGER NOT NULL DEFAULT 1 CHECK (public IN (0, 1)),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    owner_kind TEXT NOT NULL CHECK (owner_kind IN ('core', 'user', 'theme', 'plugin')),
    owner_id TEXT,
    provider_status TEXT NOT NULL DEFAULT 'active'
        CHECK (provider_status IN ('active', 'provider_inactive')),
    declaration_state TEXT NOT NULL DEFAULT 'declared'
        CHECK (declaration_state IN ('declared', 'withdrawn')),
    CHECK ((owner_kind IN ('core', 'user') AND owner_id IS NULL)
           OR (owner_kind IN ('theme', 'plugin') AND owner_id IS NOT NULL AND length(owner_id) > 0))
);
INSERT INTO taxonomy_definitions
    (key, label, hierarchical, public, owner_kind)
VALUES ('category', 'Category', 1, 1, 'core'),
       ('tag', 'Tag', 0, 1, 'core');

-- Copy terms first with detached parents, then reattach their original IDs
-- after all new rows exist. This supports reparented trees whose parent ID
-- may be numerically larger than a child without deferring FK enforcement.
CREATE TABLE taxonomy_terms_0039 (
    id INTEGER PRIMARY KEY AUTOINCREMENT CHECK (id > 0),
    kind TEXT NOT NULL REFERENCES taxonomy_definitions(key) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 120),
    slug TEXT NOT NULL CHECK (
        length(slug) BETWEEN 1 AND 64
        AND slug GLOB '[a-z]*'
        AND slug NOT GLOB '*[^a-z0-9-]*'
    ),
    parent_id INTEGER REFERENCES taxonomy_terms_0039(id) ON DELETE RESTRICT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    UNIQUE (kind, slug)
);
-- AUTOINCREMENT is historical state, not only MAX(live id). If term ID 1000
-- was deleted before upgrade, never reuse it after rebuilding the table.
CREATE TABLE taxonomy_sequence_0039 (seq INTEGER NOT NULL);
INSERT INTO taxonomy_sequence_0039 (seq)
SELECT COALESCE((SELECT seq FROM sqlite_sequence WHERE name='taxonomy_terms'), 0);
INSERT INTO taxonomy_terms_0039 (id, kind, name, slug, parent_id, version)
SELECT id, kind, name, slug, NULL, version FROM taxonomy_terms ORDER BY id;
UPDATE taxonomy_terms_0039
   SET parent_id = (SELECT old.parent_id FROM taxonomy_terms old
                     WHERE old.id = taxonomy_terms_0039.id);

-- Replace referencing tables BEFORE the old term table. Their PKs, values,
-- owners and all extension JSON survive unchanged. No live user data is read.
CREATE TABLE entry_terms_0039 (
    entry_id INTEGER NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
    term_id INTEGER NOT NULL REFERENCES taxonomy_terms_0039(id) ON DELETE RESTRICT,
    PRIMARY KEY (entry_id, term_id)
);
INSERT INTO entry_terms_0039 (entry_id, term_id)
SELECT entry_id, term_id FROM entry_terms;

CREATE TABLE term_extension_values_0039 (
    term_id INTEGER NOT NULL REFERENCES taxonomy_terms_0039(id) ON DELETE CASCADE,
    owner_kind TEXT NOT NULL CHECK (owner_kind IN ('theme','plugin')),
    owner_id TEXT NOT NULL,
    local_key TEXT NOT NULL,
    values_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(values_json)),
    PRIMARY KEY (term_id, owner_kind, owner_id, local_key)
);
INSERT INTO term_extension_values_0039
    (term_id, owner_kind, owner_id, local_key, values_json)
SELECT term_id, owner_kind, owner_id, local_key, values_json
  FROM term_extension_values;

DROP TABLE term_extension_values;
DROP TABLE entry_terms;
-- Remove old self-references so DROP cannot encounter an immediate RESTRICT
-- against rows already copied to taxonomy_terms_0039.
UPDATE taxonomy_terms SET parent_id = NULL WHERE parent_id IS NOT NULL;
DROP TABLE taxonomy_terms;
ALTER TABLE taxonomy_terms_0039 RENAME TO taxonomy_terms;
DELETE FROM sqlite_sequence WHERE name = 'taxonomy_terms';
INSERT INTO sqlite_sequence (name, seq)
SELECT 'taxonomy_terms', MAX(
    (SELECT seq FROM taxonomy_sequence_0039),
    (SELECT COALESCE(MAX(id), 0) FROM taxonomy_terms)
);
DROP TABLE taxonomy_sequence_0039;
ALTER TABLE entry_terms_0039 RENAME TO entry_terms;
ALTER TABLE term_extension_values_0039 RENAME TO term_extension_values;

CREATE INDEX idx_taxonomy_terms_kind_name_id
    ON taxonomy_terms (kind, name COLLATE BINARY, id);
CREATE INDEX idx_taxonomy_terms_parent_id ON taxonomy_terms (parent_id);
CREATE INDEX idx_entry_terms_term_entry ON entry_terms (term_id, entry_id);
CREATE INDEX term_extension_values_owner
    ON term_extension_values (owner_kind, owner_id, local_key);

-- A typed hierarchy guard applies to custom taxonomies as well as category.
CREATE TRIGGER taxonomy_terms_parent_insert
BEFORE INSERT ON taxonomy_terms
WHEN NEW.parent_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'invalid_taxonomy_parent')
    WHERE NOT EXISTS (
        SELECT 1 FROM taxonomy_terms p JOIN taxonomy_definitions d ON d.key = NEW.kind
        WHERE p.id = NEW.parent_id AND p.kind = NEW.kind AND d.hierarchical = 1
    );
END;
CREATE TRIGGER taxonomy_terms_parent_update
BEFORE UPDATE OF parent_id, kind ON taxonomy_terms
WHEN NEW.parent_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'invalid_taxonomy_parent')
    WHERE NOT EXISTS (
        SELECT 1 FROM taxonomy_terms p JOIN taxonomy_definitions d ON d.key = NEW.kind
        WHERE p.id = NEW.parent_id AND p.kind = NEW.kind AND d.hierarchical = 1
    );
END;

-- Eligibility now references the registry, preserving every old owner/state.
CREATE TABLE content_type_taxonomy_eligibility_0039 (
    content_type_key TEXT NOT NULL COLLATE BINARY REFERENCES content_types(key) ON DELETE RESTRICT,
    taxonomy_kind TEXT NOT NULL REFERENCES taxonomy_definitions(key) ON DELETE RESTRICT,
    owner_kind TEXT NOT NULL CHECK (owner_kind IN ('core', 'user', 'theme', 'plugin')),
    owner_id TEXT,
    declaration_state TEXT NOT NULL DEFAULT 'declared'
        CHECK (declaration_state IN ('declared', 'withdrawn')),
    provider_status TEXT NOT NULL DEFAULT 'active'
        CHECK (provider_status IN ('active', 'provider_inactive')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (content_type_key, taxonomy_kind),
    CHECK ((owner_kind IN ('core', 'user') AND owner_id IS NULL)
           OR (owner_kind IN ('theme', 'plugin') AND owner_id IS NOT NULL AND length(owner_id) > 0))
);
INSERT INTO content_type_taxonomy_eligibility_0039
    (content_type_key, taxonomy_kind, owner_kind, owner_id,
     declaration_state, provider_status, created_at, updated_at)
SELECT content_type_key, taxonomy_kind, owner_kind, owner_id,
       declaration_state, provider_status, created_at, updated_at
  FROM content_type_taxonomy_eligibility;
DROP TABLE content_type_taxonomy_eligibility;
ALTER TABLE content_type_taxonomy_eligibility_0039
    RENAME TO content_type_taxonomy_eligibility;
CREATE INDEX IF NOT EXISTS idx_cnt_eligibility_owner
    ON content_type_taxonomy_eligibility (owner_kind, owner_id);

-- Site override is explicit, so an absent row retains provider/core defaults.
CREATE TABLE content_type_taxonomy_overrides (
    content_type_key TEXT PRIMARY KEY REFERENCES content_types(key) ON DELETE RESTRICT,
    taxonomies_json TEXT NOT NULL CHECK (json_valid(taxonomies_json)),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
