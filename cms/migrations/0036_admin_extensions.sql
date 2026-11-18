-- M6-S10-T01. Additive registry and private extension values.
-- Uninstall, disable, theme switch and rollback never delete stored term values.
CREATE TABLE admin_extensions (
  owner_kind TEXT NOT NULL CHECK (owner_kind IN ('theme','plugin')),
  owner_id TEXT NOT NULL,
  local_key TEXT NOT NULL,
  declaration_json TEXT NOT NULL CHECK (json_valid(declaration_json)),
  provider_status TEXT NOT NULL DEFAULT 'active'
    CHECK (provider_status IN ('active','inactive')),
  PRIMARY KEY (owner_kind,owner_id,local_key)
);
CREATE INDEX admin_extensions_owner_status
  ON admin_extensions (provider_status,owner_kind,owner_id);
CREATE TABLE term_extension_values (
  term_id INTEGER NOT NULL REFERENCES taxonomy_terms(id) ON DELETE CASCADE,
  owner_kind TEXT NOT NULL CHECK (owner_kind IN ('theme','plugin')),
  owner_id TEXT NOT NULL,
  local_key TEXT NOT NULL,
  values_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(values_json)),
  PRIMARY KEY (term_id,owner_kind,owner_id,local_key)
);
CREATE INDEX term_extension_values_owner
  ON term_extension_values (owner_kind,owner_id,local_key);
