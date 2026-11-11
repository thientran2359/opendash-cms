-- M3-S27-T01: explicit theme activation. Existing home_page content is site-owned.
-- NULL active block preserves pre-0011 legacy behavior until the first activation.
ALTER TABLE theme_blocks ADD COLUMN compatible_block_id TEXT;

CREATE TABLE site_active_theme (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    block_id TEXT REFERENCES theme_blocks(id),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1)
);

INSERT INTO site_active_theme (id, block_id, version) VALUES (1, NULL, 1);
