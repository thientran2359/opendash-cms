"""M4-S08-T01 G5: test old/new release on disposable SQLite and snapshot restore.

Run with --old-binary from pre-task source SHA and --new-binary from the exact
merged release SHA. Both binaries must be built separately and identified in
docs/verification; no live site or user data is opened by this runner.
"""
import argparse
import hashlib
import json
import shutil
import sqlite3
import sys
from contextlib import closing
from pathlib import Path
from tempfile import TemporaryDirectory


TABLES = (
    "content_types",
    "content_entries",
    "taxonomy_terms",
    "entry_terms",
    "builder_homepage",
    "builder_homepage_media",
    "_sqlx_migrations",
)


def fingerprint(db: Path):
    with closing(sqlite3.connect(db)) as con:
        assert con.execute("PRAGMA integrity_check").fetchone() == ("ok",)
        assert con.execute("PRAGMA foreign_key_check").fetchall() == []
        assert con.execute("SELECT MAX(version) FROM _sqlx_migrations").fetchone() == (14,)
        rows = {
            name: con.execute(f'SELECT * FROM "{name}" ORDER BY rowid').fetchall()
            for name in TABLES
        }
    data = json.dumps(rows, ensure_ascii=False, default=str, sort_keys=True).encode()
    return hashlib.sha256(data).hexdigest(), rows


def main(old_binary: Path, new_binary: Path, source_root: Path):
    sys.path.insert(0, str(source_root / "cms" / "scripts"))
    from content_types_smoke import login, request, running
    from content_entries_smoke import api
    from entry_terms_smoke import add_term, expect, new_entry
    from media_smoke import raw_request

    def check_content(base, auth, expected):
        for type_key, item in expected.items():
            value = json.loads(expect(api(base, "GET", f"/api/admin/content-types/{type_key}/entries/{item['id']}", headers=auth), 200, "entry retained")[1])
            assert (value["id"], value["slug"], value["term_ids"], value["publication_status"]) == (
                item["id"], item["slug"], item["term_ids"], item["publication_status"]
            ), (type_key, value)
        assert raw_request(base, "GET", "/content/portfolio/old-portfolio")[0] == 200
        assert raw_request(base, "GET", "/content/post/old-post")[0] == 404
        assert raw_request(base, "GET", "/content/page/old-page")[0] == 404
        assert raw_request(base, "GET", "/")[0] == 200

    with TemporaryDirectory(prefix="opendash-m4-s08-t01-rollback-") as folder:
        site = Path(folder) / "original"
        db = site / "database" / "opendash.sqlite"
        snapshot = Path(folder) / "pre-S08-snapshot.sqlite"
        with running(old_binary, db) as base:
            expect(request(base, "/auth/setup", {"username": "admin", "password": "password123"}), 201, "old setup")
            auth = login(base, "admin", "password123")
            expect(request(base, "/api/admin/content-types", {
                "key": "portfolio", "label": "Portfolio",
                "fields": [{"key": "summary", "label": "Summary", "type": "short_text"}],
            }, auth), 201, "pre-query CPT")
            category = add_term(base, auth, "category", "Root", "root")
            child = add_term(base, auth, "category", "Child", "child", category["id"])
            tag = add_term(base, auth, "tag", "Tag", "tag")
            terms = [child["id"], tag["id"]]
            expected = {
                "post": new_entry(base, auth, "post", "old-post", terms),
                "page": new_entry(base, auth, "page", "old-page", terms),
                "portfolio": new_entry(base, auth, "portfolio", "old-portfolio", terms),
            }
            assert expected["post"]["publication_status"] == "draft"
            assert expected["page"]["publication_status"] == "draft"
            assert expected["portfolio"]["publication_status"] == "published"
            check_content(base, auth, expected)

        pre_hash, pre_rows = fingerprint(db)
        with closing(sqlite3.connect(db)) as src, closing(sqlite3.connect(snapshot)) as dest:
            src.backup(dest)
        assert fingerprint(snapshot)[0] == pre_hash

        with running(new_binary, db) as base:
            auth = login(base, "admin", "password123")
            check_content(base, auth, expected)
            filtered = json.loads(expect(
                api(
                    base, "GET",
                    "/api/admin/content-types/portfolio/entries?category_id="
                    + str(category["id"]) + "&tag_id=" + str(tag["id"])
                    + "&include_descendants=true",
                    headers=auth,
                ), 200, "new filtered archive",
            )[1])
            assert [entry["id"] for entry in filtered] == [expected["portfolio"]["id"]]

        new_hash, new_rows = fingerprint(db)
        assert new_hash == pre_hash and new_rows == pre_rows, "No schema/content mutation allowed for read-only P06 query"

        restored_db = Path(folder) / "restored-old" / "database" / "opendash.sqlite"
        restored_db.parent.mkdir(parents=True)
        shutil.copy2(snapshot, restored_db)
        assert fingerprint(restored_db) == (pre_hash, pre_rows)
        with running(old_binary, restored_db) as base:
            auth = login(base, "admin", "password123")
            check_content(base, auth, expected)
        assert fingerprint(restored_db)[0] == pre_hash
        print(f"PASS G5 M4-S08-T01 SQLite snapshot/reopen/restore: schema=0014 hash={pre_hash}")
        print("PASS old Post/Page/CPT + category/tag links + public draft isolation retained across old/new/old binaries")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--old-binary", type=Path, required=True)
    parser.add_argument("--new-binary", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    args = parser.parse_args()
    main(args.old_binary.resolve(), args.new_binary.resolve(), args.source.resolve())
