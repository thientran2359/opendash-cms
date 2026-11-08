-- A theme block package may optionally supply CPT archive/single templates.
-- Templates follow the active home_page block, but content remains site-owned
-- even when the theme changes or a theme block is not active.
ALTER TABLE theme_blocks ADD COLUMN content_list_template TEXT;
ALTER TABLE theme_blocks ADD COLUMN content_single_template TEXT;
