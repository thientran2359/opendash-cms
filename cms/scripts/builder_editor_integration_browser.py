"""M4-S09-T01 / #107: integrated Builder Preview/Save Chrome acceptance.

This is deliberately ONE disposable SQLite site: legacy theme, responsive
layout, Field Binding, and two independent published List Blocks are exercised
through the same document while two browser sessions race their saved ETag.
No production DB, uploads or browser profile is accessed.

Usage:
    python cms/scripts/builder_editor_integration_browser.py \
      --binary <EXACT_RELEASE> --browser <CHROME>
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import shutil
import sqlite3
from contextlib import closing
from pathlib import Path
from tempfile import TemporaryDirectory
from urllib.parse import urlencode

from playwright.sync_api import expect, sync_playwright

from builder_editor_browser import save as ui_save, state as browser_state
from builder_homepage_smoke import (
    checked, element, get, preview as api_preview, save as api_save,
)
from builder_list_smoke import create, fragment, set_publication
from content_entries_smoke import api
from content_types_smoke import login, request, running
from media_smoke import raw_request, upload_image
from themes_smoke import activate, install, json_request, theme_zip, themes

ENDPOINT = "/api/admin/builder/homepage"


def db_rows(database: Path) -> dict:
    tables = (
        "_sqlx_migrations", "builder_homepage", "builder_homepage_media",
        "content_types", "content_entries", "entry_terms", "taxonomy_terms",
        "media_assets",
    )
    with closing(sqlite3.connect(database)) as db:
        assert db.execute("PRAGMA integrity_check").fetchone() == ("ok",)
        assert db.execute("PRAGMA foreign_key_check").fetchall() == []
        assert db.execute("SELECT MAX(version) FROM _sqlx_migrations").fetchone() == (14,)
        return {
            name: db.execute(f'SELECT * FROM "{name}" ORDER BY rowid').fetchall()
            for name in tables
        }


def digest(value: object) -> str:
    return hashlib.sha256(json.dumps(
        value, ensure_ascii=False, sort_keys=True, default=str,
    ).encode()).hexdigest()


def backup(database: Path, destination: Path) -> None:
    with closing(sqlite3.connect(database)) as live, closing(sqlite3.connect(destination)) as saved:
        live.backup(saved)


def uploads(site: Path) -> dict[str, str]:
    root = site / "uploads"
    return {
        str(path.relative_to(site)): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in root.rglob("*") if path.is_file()
    } if root.is_dir() else {}


def node(document: dict, node_id: str) -> dict:
    found = [
        item for item in document["sections"][0]["children"][0]["children"][0]["children"]
        if item["id"] == node_id
    ]
    assert len(found) == 1, (node_id, found)
    return found[0]


def browser_login(page, base: str) -> None:
    page.goto(base + "/admin/#/builder")
    page.locator("#auth-username").fill("integrationadmin")
    page.locator("#auth-password").fill("password123")
    page.locator("#auth-login").click()
    expect(page.locator('[data-testid="builder-workspace"]')).to_contain_text("Đã mở", timeout=10000)


def public(base: str, path: str = "/") -> str:
    code, body, headers = raw_request(base, "GET", path)
    assert code == 200, (path, code, body[:300])
    if path == "/":
        assert "script-src 'none'" in headers.get("Content-Security-Policy", "")
    return body.decode("utf-8")


def verify(binary: Path, browser_path: str) -> None:
    with TemporaryDirectory(prefix="opendash-m4-s09-t01-") as directory:
        root = Path(directory)
        site = root / "integration-site"
        database = site / "database" / "opendash.sqlite"
        baseline = root / "before-integration.sqlite"
        after = root / "after-integration.sqlite"

        with running(binary, database) as base:
            checked(request(base, "/auth/setup", {
                "username": "integrationadmin", "password": "password123",
            }), 201, "Setup")
            admin = login(base, "integrationadmin", "password123")
            asset_id = upload_image(base, admin, seed=107)["id"]
            checked(api(base, "POST", "/api/admin/content-types", {
                "key": "projects", "label": "Projects",
                "fields": [{"key": "summary", "label": "Summary", "type": "short_text"}],
            }, admin), 201, "CPT")
            create(base, admin, "projects", "alpha", "Project & Alpha",
                   values={"summary": "Public summary"})
            create(base, admin, "projects", "beta", "Project Beta",
                   values={"summary": "Other summary"})
            p1 = create(base, admin, "post", "first", "Published first")
            p2 = create(base, admin, "post", "second", "Published second")
            private = create(base, admin, "post", "draft", "PRIVATE DRAFT")
            set_publication(base, admin, p1, "post", "published")
            set_publication(base, admin, p2, "post", "published")
            assert private["publication_status"] == "draft"

            a = theme_zip("qa-a/banner",
                          '<article data-theme="A">{{ values.title }}</article>',
                          fields={"title": "Title"})
            b = theme_zip("qa-b/banner",
                          '<article data-theme="B">{{ values.title }}</article>',
                          fields={"title": "Title"})
            checked(install(base, admin, a), 201, "Theme A")
            checked(install(base, admin, b), 201, "Theme B incompatible")
            checked(json_request(base, "POST", "/save", {
                "type": "qa-a/banner", "values": {"title": "LEGACY PERSISTED"},
            }, admin), 200, "M3 homepage")
            old = get(base, admin)
            assert old["version"] == 2 and old["source"] == "legacy"
            document = copy.deepcopy(old["document"])
            legacy_id = document["sections"][0]["children"][0]["children"][0]["children"][0]["id"]
            children = document["sections"][0]["children"][0]["children"][0]["children"]
            children.extend((
                element("core/heading", {
                    "level": 2, "text": "BASELINE HEADING",
                }, "integration-heading"),
                element("core/text", {
                    "text": "Binding fallback",
                    "binding": {
                        "type_key": "projects", "entry_slug": "alpha",
                        "source": {"kind": "title"}, "target": "text",
                    },
                }, "integration-binding"),
                element("core/list", {
                    "type_key": "post", "sort": "id_desc",
                    "page_size": 1, "include_descendants": False,
                }, "integration-posts"),
                element("core/list", {
                    "type_key": "projects", "sort": "id_asc",
                    "page_size": 1, "include_descendants": False,
                }, "integration-projects"),
            ))
            row = document["sections"][0]["children"][0]
            row["responsive"] = {
                "desktop": {"columns_per_row": 1, "gap_px": 0},
                "mobile": {"columns_per_row": 1},
            }
            checked(api_save(base, admin, document, 2), 200, "Baseline mixed layout")
            checked(activate(base, admin, "qa-a/banner", 1), 200, "Theme A activate")
            assert get(base, admin)["version"] == 3
            assert 'data-theme="A"' in public(base)
            assert "BASELINE HEADING" in public(base)
            assert "PRIVATE DRAFT" not in public(base)
            saved_before = get(base, admin)
            assert raw_request(base, "GET", f"/media/{asset_id}/file")[0] == 404

            initial_rows = db_rows(database)
            initial_fingerprint = digest(initial_rows)
            initial_media = uploads(site)
            assert initial_media, "media fixture must contain actual uploaded files"
            backup(database, baseline)
            assert digest(db_rows(baseline)) == initial_fingerprint
            print("PASS #107: old-format theme + binding + responsive + two lists share one SQLite document")

            # Security failures must leave the persisted layout/ETag unmodified.
            checked(api_preview(base, {}, document), 401, "Preview without auth")
            checked(api_preview(base, {"Cookie": admin["Cookie"]}, document), 403, "Preview CSRF missing")
            checked(api_preview(base, {**admin, "Origin": "https://evil.invalid"}, document),
                    403, "Preview cross-origin")
            checked(api_save(base, {"Cookie": admin["Cookie"]}, document, 3),
                    403, "Save CSRF missing")
            checked(api_save(base, {**admin, "Origin": "https://evil.invalid"}, document, 3),
                    403, "Save cross-origin")
            invalid = copy.deepcopy(document)
            node(invalid, "integration-posts")["props"]["page_size"] = 21
            checked(api_preview(base, admin, invalid), 400, "Invalid List page size")
            assert get(base, admin) == saved_before
            assert digest(db_rows(database)) == initial_fingerprint

            with sync_playwright() as pw:
                browser = pw.chromium.launch(executable_path=browser_path)
                context_one = browser.new_context(viewport={"width": 1440, "height": 900})
                context_two = browser.new_context(viewport={"width": 1440, "height": 900})
                first = context_one.new_page()
                second = context_two.new_page()
                errors: list[str] = []
                for page in (first, second):
                    page.on("pageerror", lambda error: errors.append(str(error)))
                    page.on("dialog", lambda dialog: dialog.accept())
                    browser_login(page, base)
                    assert browser_state(page, base)["version"] == 3
                assert context_one.cookies() != [] and context_two.cookies() != []

                # Both Chrome tabs load the *same* revision but edit independently.
                first.locator('[role="treeitem"][data-node-id="integration-heading"]').click()
                second.locator('[role="treeitem"][data-node-id="integration-heading"]').click()
                first.get_by_label("Tiêu đề", exact=True).fill("TAB ONE PREVIEW ONLY")
                second.get_by_label("Tiêu đề", exact=True).fill("TAB TWO CONFLICT")
                assert get(base, admin) == saved_before
                first.get_by_role("button", name="Xem trước").click()
                expect(first.frame_locator("#canvas").locator("body")).to_contain_text("TAB ONE PREVIEW ONLY")
                expect(first.frame_locator("#canvas").locator(".builder-list")).to_have_count(2)
                srcdoc = first.locator("#canvas").get_attribute("srcdoc") or ""
                assert "Project &amp; Alpha" in srcdoc and "Published second" in srcdoc
                assert "PRIVATE DRAFT" not in srcdoc
                assert "builder-list-next" not in srcdoc
                assert "data-builder-responsive" in srcdoc
                assert "@media (width < 768px)" in srcdoc
                assert first.locator("#canvas").get_attribute("sandbox") == ""
                assert first.evaluate("() => document.querySelector('#canvas').contentDocument === null")
                assert "TAB ONE PREVIEW ONLY" not in public(base)
                assert "BASELINE HEADING" in public(base)
                assert get(base, admin) == saved_before
                assert digest(db_rows(database)) == initial_fingerprint

                first.get_by_role("button", name="Canvas cấu trúc").click()
                ui_save(first)
                assert get(base, admin)["version"] == 4
                assert "TAB ONE PREVIEW ONLY" in public(base)
                second.get_by_role("button", name="Lưu homepage").click()
                expect(second.locator('[data-testid="builder-workspace"]').get_by_role("alert")).to_contain_text("412")
                expect(second.get_by_label("Tiêu đề", exact=True)).to_have_value("TAB TWO CONFLICT")
                assert "TAB TWO CONFLICT" not in public(base)
                assert get(base, admin)["version"] == 4
                print("PASS #107 Chrome: draft-only Preview, public Save, two independent tabs -> 200/412")

                # Expire the first tab's server session, then relogin as the
                # same account: its unsaved local draft must survive 401.
                first.get_by_label("Tiêu đề", exact=True).fill("RECOVERED SAME ACCOUNT")
                csrf = first.request.get(base + "/auth/me").json()["csrf_token"]
                expired = first.request.post(base + "/auth/logout", headers={"X-CSRF-Token": csrf})
                assert expired.status == 204, expired.text()
                first.get_by_role("button", name="Lưu homepage").click()
                expect(first.locator("#auth-login")).to_be_visible()
                first.locator("#auth-username").fill("integrationadmin")
                first.locator("#auth-password").fill("password123")
                first.locator("#auth-login").click()
                expect(first.get_by_label("Tiêu đề", exact=True)).to_have_value("RECOVERED SAME ACCOUNT")
                ui_save(first)
                persisted = get(base, admin)
                assert persisted["version"] == 5
                assert node(persisted["document"], "integration-heading")["props"]["text"] == "RECOVERED SAME ACCOUNT"
                assert node(persisted["document"], "integration-binding") == node(document, "integration-binding")
                assert node(persisted["document"], "integration-posts") == node(document, "integration-posts")
                assert node(persisted["document"], "integration-projects") == node(document, "integration-projects")
                assert persisted["document"]["sections"][0]["children"][0]["responsive"] == row["responsive"]
                assert node(persisted["document"], legacy_id) == node(document, legacy_id)

                # List page query keys are independent. Invalid/duplicate
                # values default that list to page 1 without touching the other.
                both = public(base, "/?" + urlencode({
                    "list_page_integration-posts": 2,
                    "list_page_integration-projects": 2,
                    "unrelated": "discard",
                }))
                assert "Published first" in fragment(both, "integration-posts")
                assert "Project Beta" in fragment(both, "integration-projects")
                assert "unrelated=" not in both
                duplicate = public(base, "/?list_page_integration-posts=2"
                                           "&list_page_integration-posts=3"
                                           "&list_page_integration-projects=2")
                assert "Published second" in fragment(duplicate, "integration-posts")
                assert "Project Beta" in fragment(duplicate, "integration-projects")
                malformed = public(base, "/?list_page_integration-posts=02")
                assert "Published second" in fragment(malformed, "integration-posts")
                out = public(base, "/?list_page_integration-posts=99")
                assert "Trang này không có nội dung" in fragment(out, "integration-posts")
                assert "PRIVATE DRAFT" not in (both + duplicate + malformed + out)

                # A → incompatible B → A must not delete saved legacy node.
                exact_document = get(base, admin)["document"]
                checked(activate(base, admin, "qa-b/banner", 2), 200, "Switch incompatible B")
                hidden = public(base)
                assert "LEGACY PERSISTED" not in hidden
                assert "RECOVERED SAME ACCOUNT" in hidden
                assert "Project &amp; Alpha" in hidden
                diagnostic = json.loads(checked(
                    api_preview(base, admin, exact_document), 200,
                    "Theme incompatible Preview",
                )[1])
                assert any(
                    item["node_id"] == legacy_id and item["code"] == "unsupported_by_theme"
                    for item in diagnostic["diagnostics"]
                ), diagnostic
                assert get(base, admin)["document"] == exact_document
                checked(activate(base, admin, "qa-a/banner", 3), 200, "Restore theme A")
                restored = public(base)
                assert "LEGACY PERSISTED" in restored and 'data-theme="A"' in restored
                assert "RECOVERED SAME ACCOUNT" in restored
                assert get(base, admin)["document"] == exact_document
                assert not errors, errors
                assert raw_request(base, "GET", f"/media/{asset_id}/file")[0] == 404
                browser.close()
                print("PASS #107 Chrome: same-account 401 draft recovery; A→B→A preserves legacy anchor")

            after_rows = db_rows(database)
            assert after_rows["builder_homepage"] != initial_rows["builder_homepage"]
            for name in after_rows:
                if name != "builder_homepage":
                    assert after_rows[name] == initial_rows[name], name
            assert uploads(site) == initial_media
            backup(database, after)

        # Restart and rollback use independent sites, not a live DB overwrite.
        with running(binary, database) as base:
            auth = login(base, "integrationadmin", "password123")
            assert get(base, auth)["version"] == 5
            assert "RECOVERED SAME ACCOUNT" in public(base)
            assert "LEGACY PERSISTED" in public(base)
            assert "PRIVATE DRAFT" not in public(base)
            assert get(base, auth)["document"] == exact_document
            assert digest(db_rows(database)) == digest(db_rows(after))
        old_site = root / "rollback-original"
        old_db = old_site / "database" / "opendash.sqlite"
        old_db.parent.mkdir(parents=True)
        shutil.copy2(baseline, old_db)
        shutil.copytree(site / "uploads", old_site / "uploads")
        assert uploads(old_site) == initial_media
        assert digest(db_rows(old_db)) == initial_fingerprint
        with running(binary, old_db) as base:
            auth = login(base, "integrationadmin", "password123")
            assert get(base, auth) == saved_before
            assert "BASELINE HEADING" in public(base)
            assert "TAB ONE PREVIEW ONLY" not in public(base)
            assert "LEGACY PERSISTED" in public(base)
        assert digest(db_rows(old_db)) == initial_fingerprint
        print("PASS #107: restart, SQLite v14/FK/integrity, online snapshot restore and media hashes")
        print(f"PASS #107: integration G4 Chrome/HTTP/SQLite, initial row SHA256={initial_fingerprint}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--browser", required=True)
    args = parser.parse_args()
    verify(args.binary.resolve(), args.browser)
