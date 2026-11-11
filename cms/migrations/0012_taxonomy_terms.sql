-- M4 P09: site-owned category/tag taxonomy.
-- Category ancestry and eight-level subtree checks are serialized by the
-- storage layer under an SQLite writer reservation before every mutation.
CREATE TABLE taxonomy_terms (
    id INTEGER PRIMARY KEY AUTOINCREMENT CHECK (id > 0),
    kind TEXT NOT NULL CHECK (kind IN ('category', 'tag')),
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 120),
    slug TEXT NOT NULL CHECK (
        length(slug) BETWEEN 1 AND 64
        AND slug GLOB '[a-z]*'
        AND slug NOT GLOB '*[^a-z0-9-]*'
    ),
    parent_id INTEGER REFERENCES taxonomy_terms(id) ON DELETE RESTRICT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    CHECK (kind = 'category' OR parent_id IS NULL),
    UNIQUE (kind, slug)
);

CREATE INDEX idx_taxonomy_terms_kind_name_id
    ON taxonomy_terms (kind, name COLLATE BINARY, id);
CREATE INDEX idx_taxonomy_terms_parent_id ON taxonomy_terms (parent_id);

-- Self-FK alone cannot establish the kind of the referenced parent.
CREATE TRIGGER taxonomy_terms_parent_insert
BEFORE INSERT ON taxonomy_terms
WHEN NEW.parent_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'invalid_category_parent')
    WHERE NOT EXISTS (
        SELECT 1 FROM taxonomy_terms
        WHERE id = NEW.parent_id AND kind = 'category'
    );
END;

CREATE TRIGGER taxonomy_terms_parent_update
BEFORE UPDATE OF parent_id, kind ON taxonomy_terms
WHEN NEW.parent_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'invalid_category_parent')
    WHERE NOT EXISTS (
        SELECT 1 FROM taxonomy_terms
        WHERE id = NEW.parent_id AND kind = 'category'
    );
END;

-- Entry memberships and their FK/index belong to M4-S04-T01 (P10);
-- keep migration 0012 independent of the future entry_terms migration.
