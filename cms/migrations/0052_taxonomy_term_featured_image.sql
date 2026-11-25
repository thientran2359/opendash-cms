ALTER TABLE taxonomy_terms
    ADD COLUMN featured_image_media_id TEXT
    REFERENCES media_assets(id) ON DELETE RESTRICT;

CREATE INDEX IF NOT EXISTS idx_taxonomy_terms_featured_image_media_id
    ON taxonomy_terms(featured_image_media_id)
    WHERE featured_image_media_id IS NOT NULL;
