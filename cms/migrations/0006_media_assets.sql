-- Media assets are site-owned and survive deactivation of themes/plugins.
-- SHA-256 IDs address canonical files in uploads/media; no arbitrary FS path
-- is stored or accepted from HTTP callers.
CREATE TABLE media_assets (
    id TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 64),
    filename TEXT NOT NULL,
    mime_type TEXT NOT NULL CHECK (mime_type IN ('image/jpeg', 'image/png', 'image/webp')),
    extension TEXT NOT NULL CHECK (extension IN ('jpg', 'png', 'webp')),
    byte_size INTEGER NOT NULL CHECK (byte_size > 0 AND byte_size <= 8388608),
    width INTEGER NOT NULL CHECK (width BETWEEN 1 AND 6000),
    height INTEGER NOT NULL CHECK (height BETWEEN 1 AND 6000),
    thumb_width INTEGER NOT NULL CHECK (thumb_width BETWEEN 1 AND 320),
    thumb_height INTEGER NOT NULL CHECK (thumb_height BETWEEN 1 AND 320),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_media_assets_created ON media_assets(created_at DESC, id DESC);
