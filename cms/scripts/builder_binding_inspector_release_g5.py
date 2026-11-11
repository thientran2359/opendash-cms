"""M4-S05-T02 Mac Intel G5: new Inspector binding, old-UI compatibility & rollback.

Use actual release binaries built from exact pre-#104 and merged #104 SHAs.
All sites/databases/assets are created in TemporaryDirectory; no live CMS data.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sqlite3
import sys
from contextlib import closing
from pathlib import Path
from tempfile import TemporaryDirectory

from playwright.sync_api import expect, sync_playwright

TABLES = [
    "content_types", "content_entries", "taxonomy_terms", "entry_terms",
    "builder_homepage", "builder_homepage_media", "media_assets", "_sqlx_migrations",
]


def snapshot_rows(db: Path) -> dict:
    with closing(sqlite3.connect(db)) as conn:
        assert conn.execute("PRAGMA integrity_check").fetchone() == ("ok",)
        assert conn.execute("PRAGMA foreign_key_check").fetchall() == []
        assert conn.execute("SELECT MAX(version) FROM _sqlx_migrations").fetchone() == (14,)
        return {table: conn.execute(f'SELECT * FROM "{table}" ORDER BY rowid').fetchall()
                for table in TABLES}


def digest(rows: dict) -> str:
    return hashlib.sha256(json.dumps(rows, ensure_ascii=False, default=str, sort_keys=True).encode()).hexdigest()


def media_hashes(root: Path) -> dict[str, str]:
    uploads = root / "uploads"
    return {str(file.relative_to(root)): hashlib.sha256(file.read_bytes()).hexdigest()
            for file in uploads.rglob("*") if file.is_file()} if uploads.is_dir() else {}


def sqlite_backup(source: Path, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    with closing(sqlite3.connect(source)) as src, closing(sqlite3.connect(destination)) as dest:
        src.backup(dest)


def verify(pre_binary: Path, new_binary: Path, chrome: str, scripts: Path) -> None:
    sys.path.insert(0, str(scripts))
    from builder_homepage_smoke import checked, element, get, save as api_save, tree
    from builder_editor_browser import save as ui_save
    from content_entries_smoke import api
    from content_types_smoke import login, request, running
    from media_smoke import raw_request, upload_image

    with TemporaryDirectory(prefix="opendash-m4-s05-t02-g5-") as tmp:
        root = Path(tmp)
        site = root / "site-original"
        db = site / "database" / "opendash.sqlite"
        before_file = root / "pre-inspector.sqlite"
        new_file = root / "with-inspector-binding.sqlite"
        old_doc = tree(element("core/text", {"text": "ORIGINAL STATIC HOMEPAGE"}, "element-bound"))
        with running(pre_binary, db) as base:
            checked(request(base, "/auth/setup", {
                "username": "g5admin", "password": "password123",
            }), 201, "setup on old binary")
            admin = login(base, "g5admin", "password123")
            asset = upload_image(base, admin, seed=83)["id"]
            checked(api(base, "POST", "/api/admin/content-types", {
                "key": "projects", "label": "Projects",
                "fields": [
                    {"key": "summary", "label": "Summary", "type": "short_text"},
                    {"key": "cover", "label": "Cover", "type": "image"},
                ],
            }, admin), 201, "old CPT")
            checked(api(base, "POST", "/api/admin/content-types/projects/entries", {
                "slug": "showcase", "title": "Old project",
                "values": {"summary": "DYNAMIC & SAFE", "cover": asset},
            }, admin), 201, "old published CPT")
            for key, slug in (("post", "post-draft"), ("page", "page-draft")):
                checked(api(base, "POST", f"/api/admin/content-types/{key}/entries", {
                    "slug": slug, "title": "PRIVATE " + key, "values": {},
                }, admin), 201, "old draft " + key)
            checked(api_save(base, admin, old_doc, 1), 200, "old static homepage")
            assert get(base, admin)["document"] == old_doc
            assert "ORIGINAL STATIC HOMEPAGE" in raw_request(base, "GET", "/")[1].decode()

        before_rows = snapshot_rows(db)
        before_hash = digest(before_rows)
        before_media = media_hashes(site)
        sqlite_backup(db, before_file)
        assert digest(snapshot_rows(before_file)) == before_hash

        new_binding = {
            "type_key": "projects", "entry_slug": "showcase",
            "source": {"kind": "field", "key": "summary"}, "target": "text",
        }
        with running(new_binary, db) as base, sync_playwright() as pw:
            browser = pw.chromium.launch(executable_path=chrome)
            context = browser.new_context(viewport={"width": 1440, "height": 900})
            page = context.new_page()
            errors = []
            page.on("pageerror", lambda error: errors.append(str(error)))
            page.goto(base + "/admin/#/builder")
            page.locator("#auth-username").fill("g5admin")
            page.locator("#auth-password").fill("password123")
            page.locator("#auth-login").click()
            expect(page.locator('[data-testid="builder-workspace"]')).to_contain_text("Đã mở")
            page.locator('[role="treeitem"][data-node-id="element-bound"]').click()
            expect(page.get_by_label("Binding content type").locator('option[value="projects"]')).to_have_count(1)
            page.get_by_label("Binding content type").select_option("projects")
            expect(page.get_by_label("Binding entry").locator('option[value="showcase"]')).to_have_count(1)
            page.get_by_label("Binding entry").select_option("showcase")
            page.get_by_label("Binding source field").select_option("field:summary")
            expect(page.get_by_role("button", name="Áp dụng binding")).to_be_enabled()
            page.get_by_role("button", name="Áp dụng binding").click()
            assert get(base, login(base, "g5admin", "password123"))["document"] == old_doc, "draft cannot write before Save"
            page.get_by_role("button", name="Xem trước").click()
            expect(page.frame_locator("#canvas").locator("body")).to_contain_text("DYNAMIC & SAFE")
            assert "ORIGINAL STATIC HOMEPAGE" in raw_request(base, "GET", "/")[1].decode()
            page.get_by_role("button", name="Canvas cấu trúc").click()
            ui_save(page)
            bound = get(base, login(base, "g5admin", "password123"))["document"]
            assert bound["sections"][0]["children"][0]["children"][0]["children"][0]["props"]["binding"] == new_binding
            assert "DYNAMIC &amp; SAFE" in raw_request(base, "GET", "/")[1].decode()
            assert "PRIVATE post" not in raw_request(base, "GET", "/")[1].decode()
            assert not errors, errors
            browser.close()

        new_rows = snapshot_rows(db)
        assert new_rows["builder_homepage"] != before_rows["builder_homepage"]
        for table in TABLES:
            if table != "builder_homepage":
                assert new_rows[table] == before_rows[table], table
        assert media_hashes(site) == before_media
        sqlite_backup(db, new_file)

        # Reverting only the UI (old binary on the **new** saved DB) must not
        # erase the typed binding even when old Inspector edits static props.
        backwards = root / "rollback-ui"
        backwards_db = backwards / "database" / "opendash.sqlite"
        backwards_db.parent.mkdir(parents=True)
        shutil.copy2(new_file, backwards_db)
        if (site / "uploads").exists():
            shutil.copytree(site / "uploads", backwards / "uploads")
        with running(pre_binary, backwards_db) as base, sync_playwright() as pw:
            browser = pw.chromium.launch(executable_path=chrome)
            page = browser.new_page(viewport={"width": 1440, "height": 900})
            page.goto(base + "/admin/#/builder")
            page.locator("#auth-username").fill("g5admin")
            page.locator("#auth-password").fill("password123")
            page.locator("#auth-login").click()
            expect(page.locator('[data-testid="builder-workspace"]')).to_contain_text("Đã mở")
            page.locator('[role="treeitem"][data-node-id="element-bound"]').click()
            expect(page.get_by_text("Field Binding hiện có")).to_be_visible()
            expect(page.get_by_label("Binding content type")).to_have_count(0), "old UI must remain old"
            page.locator('form[aria-label="Thuộc tính phần tử"] textarea').fill("STATIC EDIT UNDER OLD UI")
            ui_save(page)
            data = get(base, login(base, "g5admin", "password123"))["document"]
            props = data["sections"][0]["children"][0]["children"][0]["children"][0]["props"]
            assert props["text"] == "STATIC EDIT UNDER OLD UI" and props["binding"] == new_binding
            assert "DYNAMIC &amp; SAFE" in raw_request(base, "GET", "/")[1].decode()
            browser.close()
        for table in TABLES:
            if table != "builder_homepage":
                assert snapshot_rows(backwards_db)[table] == before_rows[table], table
        assert media_hashes(backwards) == before_media

        # Genuine snapshot restore to matching old binary returns the original
        # homepage/data, rather than running old binary on a migrated state.
        restored = root / "restored-before"
        restored_db = restored / "database" / "opendash.sqlite"
        restored_db.parent.mkdir(parents=True)
        shutil.copy2(before_file, restored_db)
        if (site / "uploads").exists():
            shutil.copytree(site / "uploads", restored / "uploads")
        assert digest(snapshot_rows(restored_db)) == before_hash
        assert media_hashes(restored) == before_media
        with running(pre_binary, restored_db) as base:
            auth = login(base, "g5admin", "password123")
            assert get(base, auth)["document"] == old_doc
            homepage = raw_request(base, "GET", "/")[1].decode()
            assert "ORIGINAL STATIC HOMEPAGE" in homepage and "DYNAMIC &amp; SAFE" not in homepage
        assert digest(snapshot_rows(restored_db)) == before_hash

        print("PASS #104 G5: exact old/new Intel Mac binaries, Chrome UI Save/Preview and old-UI binding preservation")
        print(f"PASS #104 G5: schema 0014, original SQLite+media snapshot restored byte-for-byte; pre-hash={before_hash}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--old-binary", type=Path, required=True)
    parser.add_argument("--new-binary", type=Path, required=True)
    parser.add_argument("--chrome", required=True)
    parser.add_argument("--source", type=Path, required=True)
    args = parser.parse_args()
    verify(args.old_binary.resolve(), args.new_binary.resolve(), args.chrome, args.source.resolve() / "cms" / "scripts")
