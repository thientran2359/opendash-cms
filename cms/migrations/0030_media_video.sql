-- M7-S07-T01: Rebuild media_assets to support bounded MP4 videos alongside images
-- while preserving all existing image IDs, metadata, timestamps, indexes, and FKs
-- without disabling PRAGMA foreign_keys.

CREATE TABLE _tmp_builder_homepage_media AS
SELECT media_id FROM builder_homepage_media;

CREATE TABLE _tmp_theme_option_media_refs AS
SELECT theme_id, option_key, media_id, owner_kind, owner_id FROM theme_option_media_refs;

CREATE TABLE _tmp_entry_seo AS
SELECT entry_id, title, description, og_image_id, canonical_url, noindex FROM entry_seo;

CREATE TABLE _tmp_site_home_seo AS
SELECT id, title, description, og_image_id, canonical_url, noindex, version FROM site_home_seo;

CREATE TABLE _tmp_seo_public_media_refs AS
SELECT source_kind, source_id, media_id FROM seo_public_media_refs;

DROP TABLE seo_public_media_refs;
DROP TABLE site_home_seo;
DROP TABLE entry_seo;
DROP TABLE theme_option_media_refs;
DROP TABLE builder_homepage_media;

CREATE TABLE media_assets_new (
    id TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 64),
    kind TEXT NOT NULL DEFAULT 'image' CHECK (kind IN ('image', 'video')),
    filename TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    extension TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    thumb_width INTEGER,
    thumb_height INTEGER,
    duration_ms INTEGER,
    video_codec TEXT,
    audio_codec TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (
        (
            kind = 'image'
            AND mime_type IN ('image/jpeg', 'image/png', 'image/webp')
            AND extension IN ('jpg', 'png', 'webp')
            AND byte_size > 0 AND byte_size <= 8388608
            AND width BETWEEN 1 AND 6000
            AND height BETWEEN 1 AND 6000
            AND thumb_width IS NOT NULL AND thumb_width BETWEEN 1 AND 320
            AND thumb_height IS NOT NULL AND thumb_height BETWEEN 1 AND 320
            AND duration_ms IS NULL
            AND video_codec IS NULL
            AND audio_codec IS NULL
        )
        OR
        (
            kind = 'video'
            AND mime_type = 'video/mp4'
            AND extension = 'mp4'
            AND byte_size > 0 AND byte_size <= 104857600
            AND width BETWEEN 1 AND 3840
            AND height BETWEEN 1 AND 2160
            AND thumb_width IS NULL
            AND thumb_height IS NULL
            AND duration_ms IS NOT NULL AND duration_ms BETWEEN 1 AND 600000
            AND video_codec IS NOT NULL
        )
    )
);

INSERT INTO media_assets_new (
    id, kind, filename, mime_type, extension, byte_size, width, height,
    thumb_width, thumb_height, duration_ms, video_codec, audio_codec, created_at
)
SELECT
    id, 'image', filename, mime_type, extension, byte_size, width, height,
    thumb_width, thumb_height, NULL, NULL, NULL, created_at
FROM media_assets;

DROP TABLE media_assets;
ALTER TABLE media_assets_new RENAME TO media_assets;

CREATE INDEX idx_media_assets_created ON media_assets(created_at DESC, id DESC);
CREATE INDEX idx_media_assets_kind_created ON media_assets(kind, created_at DESC, id DESC);

CREATE TABLE builder_homepage_media (
    media_id TEXT PRIMARY KEY NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT
);
INSERT INTO builder_homepage_media (media_id)
SELECT media_id FROM _tmp_builder_homepage_media;

CREATE TABLE theme_option_media_refs (
    theme_id TEXT NOT NULL,
    option_key TEXT NOT NULL,
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
    owner_kind TEXT NOT NULL DEFAULT 'theme' CHECK (owner_kind IN ('theme', 'plugin')),
    owner_id TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (theme_id, option_key)
);
INSERT INTO theme_option_media_refs (theme_id, option_key, media_id, owner_kind, owner_id)
SELECT theme_id, option_key, media_id, owner_kind, owner_id FROM _tmp_theme_option_media_refs;

CREATE TABLE entry_seo (
    entry_id INTEGER PRIMARY KEY REFERENCES content_entries(id) ON DELETE CASCADE,
    title TEXT,
    description TEXT,
    og_image_id TEXT REFERENCES media_assets(id) ON DELETE SET NULL,
    canonical_url TEXT,
    noindex INTEGER NOT NULL DEFAULT 0 CHECK (noindex IN (0, 1))
);
INSERT INTO entry_seo (entry_id, title, description, og_image_id, canonical_url, noindex)
SELECT entry_id, title, description, og_image_id, canonical_url, noindex FROM _tmp_entry_seo;

CREATE TABLE site_home_seo (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    title TEXT,
    description TEXT,
    og_image_id TEXT REFERENCES media_assets(id) ON DELETE SET NULL,
    canonical_url TEXT,
    noindex INTEGER NOT NULL DEFAULT 0 CHECK (noindex IN (0, 1)),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1)
);
INSERT INTO site_home_seo (id, title, description, og_image_id, canonical_url, noindex, version)
SELECT id, title, description, og_image_id, canonical_url, noindex, version FROM _tmp_site_home_seo;

CREATE TABLE seo_public_media_refs (
    source_kind TEXT NOT NULL CHECK (source_kind IN ('entry_seo', 'home_seo')),
    source_id INTEGER NOT NULL,
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE CASCADE,
    PRIMARY KEY (source_kind, source_id, media_id)
);
CREATE INDEX idx_seo_public_media_refs_media_id
    ON seo_public_media_refs (media_id);
INSERT INTO seo_public_media_refs (source_kind, source_id, media_id)
SELECT source_kind, source_id, media_id FROM _tmp_seo_public_media_refs;

DROP TABLE _tmp_seo_public_media_refs;
DROP TABLE _tmp_site_home_seo;
DROP TABLE _tmp_entry_seo;
DROP TABLE _tmp_theme_option_media_refs;
DROP TABLE _tmp_builder_homepage_media;
