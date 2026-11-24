-- 0051_admin_i18n_and_multilingual_settings.sql
-- Net sqlite_schema delta: +0 objects (219 -> 219, <= MAX_SCHEMA_OBJECTS = 256).

ALTER TABLE site_general_settings
    ADD COLUMN cms_locale TEXT NOT NULL DEFAULT 'en'
    CHECK (length(cms_locale) BETWEEN 2 AND 35);

ALTER TABLE site_locales_state
    ADD COLUMN multilingual_enabled INTEGER NOT NULL DEFAULT 0
    CHECK (multilingual_enabled IN (0, 1));

UPDATE site_locales_state
   SET multilingual_enabled = 1
 WHERE id = 1
   AND (SELECT COUNT(*) FROM site_locales) > 1;

ALTER TABLE site_locales
    ADD COLUMN deleted_at TEXT NULL;
