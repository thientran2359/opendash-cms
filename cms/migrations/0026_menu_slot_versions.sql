-- M7-S02-T01 aggregate version per theme for menu slot assignments.
CREATE TABLE IF NOT EXISTS site_menu_slot_versions (
    theme_id TEXT PRIMARY KEY NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1)
);

INSERT OR IGNORE INTO site_menu_slot_versions (theme_id, version)
SELECT DISTINCT theme_id, 1 FROM site_menu_slot_assignments;
