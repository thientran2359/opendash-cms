-- M6-S11-T01 / C4: provider rewrite defaults are separate from site choices.
-- No backfill is possible from the old site maps: an explicit site NULL must
-- remain distinct from an absent choice. Existing 0040 URLs stay unchanged.
CREATE TABLE provider_rewrite_defaults (
    resource_kind TEXT NOT NULL CHECK (resource_kind IN ('content_type', 'taxonomy')),
    resource_key TEXT NOT NULL,
    owner_kind TEXT NOT NULL CHECK (owner_kind IN ('plugin', 'theme')),
    owner_id TEXT NOT NULL,
    rewrite_base TEXT,
    provider_status TEXT NOT NULL DEFAULT 'active'
        CHECK (provider_status IN ('active', 'provider_inactive')),
    declaration_state TEXT NOT NULL DEFAULT 'declared'
        CHECK (declaration_state IN ('declared', 'withdrawn')),
    PRIMARY KEY (resource_kind, resource_key)
);
CREATE INDEX idx_provider_rewrite_defaults_owner
    ON provider_rewrite_defaults(owner_kind, owner_id);
