-- 0055: Them cot locale vao PK cua theme_option_media_refs (+0 net schema objects)
CREATE TABLE theme_option_media_refs_0055 (
    theme_id TEXT NOT NULL,
    owner_kind TEXT NOT NULL DEFAULT 'theme' CHECK (owner_kind IN ('theme', 'plugin', 'theme_functions')),
    owner_id TEXT NOT NULL,
    locale TEXT NOT NULL DEFAULT '',
    option_key TEXT NOT NULL,
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
    PRIMARY KEY (owner_kind, owner_id, locale, option_key, media_id)
);
INSERT INTO theme_option_media_refs_0055 (theme_id, owner_kind, owner_id, locale, option_key, media_id)
SELECT theme_id, owner_kind, owner_id, '', option_key, media_id
  FROM theme_option_media_refs;
DROP TABLE theme_option_media_refs;
ALTER TABLE theme_option_media_refs_0055 RENAME TO theme_option_media_refs;
