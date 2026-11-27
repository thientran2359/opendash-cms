-- 0058: Expand template_override_media_refs.ref_role to ('image', 'video', 'poster', 'file', 'any') (+0 net schema objects)
CREATE TABLE template_override_media_refs_0058 (
    override_id INTEGER NOT NULL REFERENCES template_overrides(id) ON DELETE CASCADE,
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
    ref_role TEXT NOT NULL CHECK (ref_role IN ('image', 'video', 'poster', 'file', 'any')),
    PRIMARY KEY (override_id, media_id, ref_role)
);

INSERT INTO template_override_media_refs_0058 (override_id, media_id, ref_role)
SELECT override_id, media_id, ref_role
FROM template_override_media_refs;

DROP TABLE template_override_media_refs;
ALTER TABLE template_override_media_refs_0058 RENAME TO template_override_media_refs;

CREATE INDEX idx_template_override_media_refs_media
    ON template_override_media_refs (media_id);
