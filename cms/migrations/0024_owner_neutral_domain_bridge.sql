-- M6-S05-T01: Owner-neutral domain bridge for settings, template selections, and Builder blocks.
ALTER TABLE theme_option_values
    ADD COLUMN owner_kind TEXT NOT NULL DEFAULT 'theme' CHECK (owner_kind IN ('theme', 'plugin'));
ALTER TABLE theme_option_values
    ADD COLUMN owner_id TEXT NOT NULL DEFAULT '';
UPDATE theme_option_values SET owner_id = theme_id WHERE owner_id = '';

ALTER TABLE theme_option_versions
    ADD COLUMN owner_kind TEXT NOT NULL DEFAULT 'theme' CHECK (owner_kind IN ('theme', 'plugin'));
ALTER TABLE theme_option_versions
    ADD COLUMN owner_id TEXT NOT NULL DEFAULT '';
UPDATE theme_option_versions SET owner_id = theme_id WHERE owner_id = '';

ALTER TABLE theme_option_media_refs
    ADD COLUMN owner_kind TEXT NOT NULL DEFAULT 'theme' CHECK (owner_kind IN ('theme', 'plugin'));
ALTER TABLE theme_option_media_refs
    ADD COLUMN owner_id TEXT NOT NULL DEFAULT '';
UPDATE theme_option_media_refs SET owner_id = theme_id WHERE owner_id = '';

ALTER TABLE template_overrides
    ADD COLUMN owner_kind TEXT NOT NULL DEFAULT 'theme' CHECK (owner_kind IN ('theme', 'plugin'));
ALTER TABLE template_overrides
    ADD COLUMN owner_id TEXT NOT NULL DEFAULT '';
ALTER TABLE template_overrides
    ADD COLUMN template_selection_key TEXT;
UPDATE template_overrides SET owner_id = COALESCE(base_theme_id, '') WHERE owner_id = '';

CREATE INDEX IF NOT EXISTS idx_theme_option_values_owner
    ON theme_option_values (owner_kind, owner_id);
CREATE INDEX IF NOT EXISTS idx_template_overrides_owner
    ON template_overrides (owner_kind, owner_id);
