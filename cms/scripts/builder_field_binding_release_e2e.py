"""G5 Intel macOS release acceptance for P05 with genuine pre-P05 rollback binary.

Uses disposable SQLite/temp site, backs up online SQLite + image assets and
restores the snapshot into a DIFFERENT site before launching the old binary.
Never connects to an existing OpenDash site.
"""
import argparse
import hashlib
import json
import shutil
import sqlite3
from contextlib import closing
from pathlib import Path
from tempfile import TemporaryDirectory

from playwright.sync_api import sync_playwright
from builder_homepage_smoke import checked, element, get, preview, save, tree
from content_entries_smoke import api
from content_types_smoke import login, request, running
from media_smoke import raw_request, upload_image


def fingerprint(db):
    tables = ["content_types", "content_entries", "taxonomy_terms", "entry_terms",
              "media_assets", "builder_homepage", "builder_homepage_media", "_sqlx_migrations"]
    with closing(sqlite3.connect(db)) as con:
        assert con.execute("PRAGMA integrity_check").fetchone() == ("ok",)
        assert con.execute("PRAGMA foreign_key_check").fetchall() == []
        assert con.execute("SELECT MAX(version) FROM _sqlx_migrations").fetchone() == (14,)
        state = {table: con.execute(f'SELECT * FROM "{table}" ORDER BY rowid').fetchall()
                 for table in tables}
    return hashlib.sha256(json.dumps(state, ensure_ascii=False, default=str).encode()).hexdigest(), state


def assets(root):
    base = root / "uploads"
    return {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in base.rglob("*") if p.is_file()} if base.exists() else {}


def binding(type_key, slug, target, field=None):
    return {"type_key": type_key, "entry_slug": slug,
            "source": {"kind": "field", "key": field} if field else {"kind": "title"},
            "target": target}


def check(new_binary, old_binary, chrome):
    with TemporaryDirectory(prefix="opendash-m4-p05-g5-release-") as folder:
        root = Path(folder)
        original_root = root / "original_site"
        original_db = original_root / "database" / "opendash.sqlite"
        snapshot = root / "pre-P05-snapshot.sqlite"
        old_doc = tree(element("core/text", {"text": "PRE-P05 static Builder"}, "old-static"))
        with running(old_binary, original_db) as base:
            checked(request(base, "/auth/setup", {"username": "releaseadmin", "password": "password123"}),
                    201, "old setup")
            auth = login(base, "releaseadmin", "password123")
            media = upload_image(base, auth, seed=42)["id"]
            checked(api(base, "POST", "/api/admin/content-types", {
                "key": "projects", "label": "Projects",
                "fields": [
                    {"key": "caption", "label": "Caption", "type": "short_text"},
                    {"key": "cover", "label": "Cover", "type": "image"},
                ],
            }, auth), 201, "old CPT")
            cpt = json.loads(checked(api(base, "POST", "/api/admin/content-types/projects/entries", {
                "slug": "legacy-project", "title": "Preserve CPT",
                "values": {"caption": "Bound & preserved", "cover": media},
            }, auth), 201, "old CPT entry")[1])
            post = json.loads(checked(api(base, "POST", "/api/admin/content-types/post/entries", {
                "slug": "draft-post", "title": "PRIVATE PRE-P05 POST", "values": {},
            }, auth), 201, "old Post")[1])
            page = json.loads(checked(api(base, "POST", "/api/admin/content-types/page/entries", {
                "slug": "draft-page", "title": "PRIVATE PRE-P05 PAGE", "values": {},
            }, auth), 201, "old Page")[1])
            assert post["publication_status"] == "draft" and page["publication_status"] == "draft"
            checked(save(base, auth, old_doc, 1), 200, "old static homepage")
            old_state = get(base, auth)
            assert old_state["document"] == old_doc and old_state["version"] == 2
            assert "PRE-P05 static Builder" in checked(raw_request(base, "GET", "/"), 200, "old public")[1].decode()
            checked(raw_request(base, "GET", f"/media/{media}/file"), 200, "old public CPT image")

        before_hash, before_rows = fingerprint(original_db)
        original_assets = assets(original_root)
        with closing(sqlite3.connect(original_db)) as src, closing(sqlite3.connect(snapshot)) as dest:
            src.backup(dest)
        backup_assets = root / "asset_snapshot"
        if (original_root / "uploads").exists():
            shutil.copytree(original_root / "uploads", backup_assets)

        with running(new_binary, original_db) as base:
            auth = login(base, "releaseadmin", "password123")
            assert get(base, auth) == old_state, "new binary must preserve old static Builder"
            for type_key, entry in (("post", post), ("page", page), ("projects", cpt)):
                got = json.loads(checked(api(base, "GET", f"/api/admin/content-types/{type_key}/entries/{entry['id']}",
                                             headers=auth), 200, "retained old entry")[1])
                assert (got["id"], got["title"], got["slug"]) == (entry["id"], entry["title"], entry["slug"])
            doc = tree(
                element("core/text", {"text": "", "binding": binding("projects", "legacy-project", "text", "caption")}, "bound-cpt"),
                element("core/heading", {"level": 2, "text": "", "binding": binding("post", "draft-post", "text")}, "bound-draft"),
            )
            pv = json.loads(checked(preview(base, auth, doc), 200, "new preview old fixture")[1])
            assert "PRIVATE PRE-P05 POST" in pv["html"]
            assert "PRIVATE PRE-P05 POST" not in checked(raw_request(base, "GET", "/"), 200, "old public unchanged by preview")[1].decode()
            checked(save(base, auth, doc, old_state["version"]), 200, "new save binding")
            public = checked(raw_request(base, "GET", "/"), 200, "new published-only binding")[1].decode()
            assert "Bound &amp; preserved" in public and "PRIVATE PRE-P05 POST" not in public
            checked(raw_request(base, "GET", f"/media/{media}/file"), 200, "CPT media ACL kept")

            with sync_playwright() as pw:
                browser = pw.chromium.launch(executable_path=str(chrome))
                browser_page = browser.new_page(viewport={"width": 390, "height": 844})
                page_errors = []
                browser_page.on("pageerror", lambda e: page_errors.append(str(e)))
                response = browser_page.goto(base + "/", wait_until="load")
                assert response.status == 200
                assert browser_page.get_by_text("Bound & preserved").count() == 1
                assert browser_page.get_by_text("PRIVATE PRE-P05 POST").count() == 0
                dimensions = browser_page.evaluate("() => [innerWidth, document.documentElement.scrollWidth]")
                assert dimensions[0] == 390 and dimensions[1] <= 390, dimensions
                assert not page_errors, page_errors
                browser.close()
        after_hash, after_rows = fingerprint(original_db)
        assert after_rows["content_entries"] == before_rows["content_entries"]
        assert after_rows["content_types"] == before_rows["content_types"]
        assert after_rows["media_assets"] == before_rows["media_assets"]
        assert after_rows["_sqlx_migrations"] == before_rows["_sqlx_migrations"]
        assert after_hash != before_hash, "Builder Save should change only layout"

        restored_root = root / "restored_old_site"
        restored_db = restored_root / "database" / "opendash.sqlite"
        restored_db.parent.mkdir(parents=True)
        shutil.copy2(snapshot, restored_db)
        if backup_assets.exists():
            shutil.copytree(backup_assets, restored_root / "uploads")
        restored_hash, restored_rows = fingerprint(restored_db)
        assert restored_hash == before_hash and restored_rows == before_rows
        assert assets(restored_root) == original_assets, "media files must be restored byte-for-byte"
        with running(old_binary, restored_db) as base:
            auth = login(base, "releaseadmin", "password123")
            assert get(base, auth) == old_state
            public = checked(raw_request(base, "GET", "/"), 200, "old binary after snapshot restore")[1].decode()
            assert "PRE-P05 static Builder" in public and "Bound &amp; preserved" not in public
            for key, item in (("post", post), ("page", page), ("projects", cpt)):
                checked(api(base, "GET", f"/api/admin/content-types/{key}/entries/{item['id']}", headers=auth),
                        200, "old binary content restored")
            checked(raw_request(base, "GET", f"/media/{media}/file"), 200, "restored image")
        assert fingerprint(restored_db)[0] == before_hash
    print("PASS G5: exact Mac release source, old Post/Page/CPT/Builder retained, live preview/public+Chrome, SQLite and media snapshot matching old-binary restore")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--binary", type=Path, required=True)
    p.add_argument("--old-binary", type=Path, required=True)
    p.add_argument("--chrome", type=Path, required=True)
    a = p.parse_args()
    check(a.binary.resolve(), a.old_binary.resolve(), a.chrome.resolve())
