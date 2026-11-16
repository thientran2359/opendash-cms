-- M7-S07-T02: Typed video/poster public ACL projection tables for Builder homepage
-- and template overrides. Preview/draft/unpublished/inactive states never populate
-- or qualify for public delivery.

CREATE TABLE IF NOT EXISTS builder_homepage_video_refs (
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
    ref_role TEXT NOT NULL CHECK (ref_role IN ('video', 'poster')),
    PRIMARY KEY (media_id, ref_role)
);

CREATE TABLE IF NOT EXISTS template_override_media_refs (
    override_id INTEGER NOT NULL REFERENCES template_overrides(id) ON DELETE CASCADE,
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
    ref_role TEXT NOT NULL CHECK (ref_role IN ('image', 'video', 'poster')),
    PRIMARY KEY (override_id, media_id, ref_role)
);

CREATE INDEX IF NOT EXISTS idx_template_override_media_refs_media
    ON template_override_media_refs (media_id);
