CREATE TABLE builder_block_definitions (
    block_id TEXT PRIMARY KEY NOT NULL,
    owner_kind TEXT NOT NULL CHECK (owner_kind IN ('core', 'theme', 'plugin')),
    owner_id TEXT NOT NULL,
    provider_status TEXT NOT NULL CHECK (provider_status IN ('active', 'provider_inactive')),
    declaration_state TEXT NOT NULL CHECK (declaration_state IN ('declared', 'withdrawn')),
    schema_json TEXT NOT NULL CHECK (json_valid(schema_json)),
    template TEXT NOT NULL,
    revision_digest TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_builder_block_definitions_owner
    ON builder_block_definitions(owner_kind, owner_id, provider_status, declaration_state);

ALTER TABLE plugin_revisions
    ADD COLUMN blocks_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(blocks_json));
