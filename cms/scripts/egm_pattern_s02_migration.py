#!/usr/bin/env python3
"""EGM S02: reproducible SQLite 0044 legacy-Section upgrade contract.

Uses a disposable in-memory DB. Applies the committed SQL migrations verbatim
through 0043, seeds private Section data/media, then applies 0044 and checks
that the migrated data and referential integrity remain intact.
"""

from __future__ import annotations

import json
import sqlite3
from pathlib import Path


MIGRATIONS = Path(__file__).resolve().parents[1] / "migrations"


def apply_through(db: sqlite3.Connection, upper: int) -> None:
    for file in sorted(MIGRATIONS.glob("[0-9][0-9][0-9][0-9]_*.sql")):
        number = int(file.name[:4])
        if number > upper:
            continue
        db.executescript(file.read_text(encoding="utf-8"))


def check() -> None:
    db = sqlite3.connect(":memory:")
    db.execute("PRAGMA foreign_keys = ON")
    apply_through(db, 43)

    legacy_doc = {
        "schema_version": 1,
        "sections": [
            {
                "id": "banner-section",
                "children": [
                    {
                        "id": "banner-row",
                        "children": [
                            {
                                "id": "banner-col",
                                "children": [
                                    {
                                        "id": "banner-heading",
                                        "type": "core/heading",
                                        "props": {"level": 2, "text": "Historical Content"},
                                    }
                                ],
                            }
                        ],
                    }
                ],
            }
        ],
    }
    media_id = "a" * 64
    db.execute(
        """INSERT INTO media_assets
        (id,filename,mime_type,extension,byte_size,width,height,thumb_width,thumb_height)
        VALUES (?,'legacy.png','image/png','png',123,20,20,10,10)""",
        (media_id,),
    )
    cursor = db.execute(
        """INSERT INTO content_entries
        (content_type_key,slug,title,values_json,publication_status,version)
        VALUES ('section','legacy-banner','Legacy Banner','{"original":123}', 'draft',7)"""
    )
    original_id = cursor.lastrowid
    raw_document = json.dumps(legacy_doc, ensure_ascii=False, separators=(",", ":"))
    db.execute(
        "INSERT INTO section_documents(entry_id,document_json) VALUES (?,?)",
        (original_id, raw_document),
    )
    db.execute(
        "INSERT INTO section_document_media_refs(entry_id,media_id,ref_role) VALUES (?,?,'image')",
        (original_id, media_id),
    )
    before = db.execute(
        "SELECT id,slug,title,values_json,version,created_at FROM content_entries WHERE id=?",
        (original_id,),
    ).fetchone()
    db.commit()

    db.executescript((MIGRATIONS / "0044_pattern_cpt_and_refs.sql").read_text(encoding="utf-8"))
    assert db.execute("PRAGMA foreign_key_check").fetchall() == [], "FK damaged"
    pattern = db.execute(
        "SELECT definition_json,owner_kind FROM content_types WHERE key='pattern'"
    ).fetchone()
    assert pattern is not None and pattern[1] == "core"
    settings = json.loads(pattern[0])["settings"]
    assert settings == {
        "public": False,
        "has_archive": False,
        "publication": "immediate",
        "supports": ["builder"],
    }
    assert db.execute("SELECT COUNT(*) FROM content_types WHERE key='section'").fetchone()[0] == 0

    after = db.execute(
        "SELECT id,slug,title,values_json,version,created_at FROM content_entries WHERE id=?",
        (original_id,),
    ).fetchone()
    assert before == after, "legacy entry ID, values, title, or CAS version changed"
    assert db.execute(
        "SELECT content_type_key,content_mode,publication_status FROM content_entries WHERE id=?",
        (original_id,),
    ).fetchone() == ("pattern", "builder", "draft")
    migrated_doc = db.execute(
        "SELECT document_json FROM entry_builder_documents WHERE entry_id=?",
        (original_id,),
    ).fetchone()[0]
    assert json.loads(migrated_doc) == legacy_doc, "legacy AST lost"
    assert db.execute(
        "SELECT media_id,ref_role FROM entry_builder_media_refs WHERE entry_id=?",
        (original_id,),
    ).fetchall() == [(media_id, "image")], "private media pin lost"
    assert db.execute(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name LIKE 'section_%'"
    ).fetchone()[0] == 0

    for query in (
        "UPDATE content_entries SET publication_status='published' WHERE id=?",
        "UPDATE content_entries SET content_type_key='page' WHERE id=?",
    ):
        try:
            db.execute(query, (original_id,))
        except sqlite3.IntegrityError:
            pass
        else:
            raise AssertionError("private Pattern trigger bypassed")

    page_id = db.execute(
        """INSERT INTO content_entries
        (content_type_key,slug,title,values_json,publication_status)
        VALUES ('page','legacy-user','Legacy user','{}','draft')"""
    ).lastrowid
    ref_doc = {
        "schema_version": 1,
        "sections": [
            {
                "id": "parent-section",
                "children": [{
                    "id": "parent-row", "children": [{
                        "id": "parent-col", "children": [{
                            "id": "ref", "type": "core/pattern_ref", "props": {"pattern_id": original_id}
                        }]
                    }]
                }]
            }
        ],
    }
    db.execute(
        "INSERT INTO entry_builder_documents(entry_id,document_json,updated_at) VALUES(?,?,datetime('now'))",
        (page_id, json.dumps(ref_doc)),
    )
    db.execute(
        "INSERT INTO entry_pattern_refs(source_entry_id,target_pattern_id) VALUES (?,?)",
        (page_id, original_id),
    )
    db.execute("DELETE FROM content_entries WHERE id=?", (original_id,))
    assert db.execute("SELECT COUNT(*) FROM entry_pattern_refs").fetchone()[0] == 0
    assert db.execute("SELECT document_json FROM entry_builder_documents WHERE entry_id=?", (page_id,)).fetchone()[0] == json.dumps(ref_doc)
    assert db.execute("PRAGMA foreign_key_check").fetchall() == []
    db.close()
    print("PASS 0044 Section -> Pattern: ID/CAS/AST/media preserved, privacy triggers and FK cascade")


if __name__ == "__main__":
    check()
