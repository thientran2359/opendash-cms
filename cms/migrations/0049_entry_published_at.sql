ALTER TABLE content_entries ADD COLUMN published_at TEXT;

UPDATE content_entries
   SET published_at = updated_at
 WHERE publication_status = 'published'
   AND published_at IS NULL;
