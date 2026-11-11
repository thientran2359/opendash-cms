-- M4-S04-T01 (P10): site-owned Post/Page/CPT taxonomy membership.
-- Migration 0013 belongs to Forge P02 builder_homepage (#78), and must merge
-- and be applied before this migration is released.
-- Do not modify the already released 0012_taxonomy_terms.sql checksum.
-- Deleting an entry removes its links, while deleting a linked term is blocked.
CREATE TABLE entry_terms (
    entry_id INTEGER NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
    term_id INTEGER NOT NULL REFERENCES taxonomy_terms(id) ON DELETE RESTRICT,
    PRIMARY KEY (entry_id, term_id)
);

CREATE INDEX idx_entry_terms_term_entry ON entry_terms (term_id, entry_id);
