-- Forward-only extension: do not rebuild users/sessions or change password hashes.
ALTER TABLE users ADD COLUMN version INTEGER NOT NULL DEFAULT 1 CHECK(version >= 1);
