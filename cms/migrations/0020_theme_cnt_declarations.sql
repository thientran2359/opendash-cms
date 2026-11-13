-- M5-S10-T01: Separate CNT declaration lifecycle state (`declared` | `withdrawn`)
-- from provider availability (`active` | `provider_inactive`) and persist
-- core-taxonomy eligibility (`category` | `tag`) per content type.
ALTER TABLE content_types
ADD COLUMN declaration_state TEXT NOT NULL DEFAULT 'declared'
    CHECK (declaration_state IN ('declared', 'withdrawn'));

CREATE TABLE content_type_taxonomy_eligibility (
    content_type_key TEXT NOT NULL COLLATE BINARY REFERENCES content_types(key) ON DELETE RESTRICT,
    taxonomy_kind TEXT NOT NULL CHECK (taxonomy_kind IN ('category', 'tag')),
    owner_kind TEXT NOT NULL CHECK (owner_kind IN ('core', 'user', 'theme', 'plugin')),
    owner_id TEXT,
    declaration_state TEXT NOT NULL DEFAULT 'declared'
        CHECK (declaration_state IN ('declared', 'withdrawn')),
    provider_status TEXT NOT NULL DEFAULT 'active'
        CHECK (provider_status IN ('active', 'provider_inactive')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (content_type_key, taxonomy_kind),
    CHECK (
        (owner_kind IN ('core', 'user') AND owner_id IS NULL)
        OR (owner_kind IN ('theme', 'plugin') AND owner_id IS NOT NULL AND length(owner_id) > 0)
    )
);
