CREATE TABLE IF NOT EXISTS site_menus (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    menu_key TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    origin_provenance TEXT,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_site_menus_origin_provenance
    ON site_menus (origin_provenance)
    WHERE origin_provenance IS NOT NULL;

CREATE TABLE IF NOT EXISTS site_menu_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    menu_id INTEGER NOT NULL REFERENCES site_menus(id) ON DELETE CASCADE,
    parent_id INTEGER REFERENCES site_menu_items(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    label TEXT NOT NULL,
    target_kind TEXT NOT NULL CHECK (target_kind IN ('entry', 'term', 'external')),
    target_entry_id INTEGER,
    target_term_id INTEGER,
    external_url TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_site_menu_items_sibling_pos
    ON site_menu_items (menu_id, COALESCE(parent_id, 0), position);

CREATE INDEX IF NOT EXISTS idx_site_menu_items_menu_order
    ON site_menu_items (menu_id, COALESCE(parent_id, 0), position, id);

CREATE TABLE IF NOT EXISTS site_menu_slot_assignments (
    theme_id TEXT NOT NULL,
    slot_key TEXT NOT NULL,
    menu_id INTEGER NOT NULL REFERENCES site_menus(id) ON DELETE CASCADE,
    PRIMARY KEY (theme_id, slot_key)
);
