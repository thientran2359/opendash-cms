"""#106 Mac Intel G5: real pre/post List Block binaries and SQLite rollback.

Each invocation creates disposable sites and uploaded media. The old release
binary MUST embed its pre-#106 Admin bundle; the new release embeds #106.
Run from a G5 evidence checkout with --source pinned to G4's merge SHA.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import sqlite3
import sys
from contextlib import closing
from pathlib import Path
from tempfile import TemporaryDirectory
from urllib.parse import urlencode

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
        return {
            table: conn.execute(f'SELECT * FROM "{table}" ORDER BY rowid').fetchall()
            for table in TABLES
        }


def fingerprint(rows: dict) -> str:
    encoded = json.dumps(rows, ensure_ascii=False, sort_keys=True, default=str).encode()
    return hashlib.sha256(encoded).hexdigest()


def media_hashes(site: Path) -> dict[str, str]:
    uploads = site / "uploads"
    return {
        str(path.relative_to(site)): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in uploads.rglob("*") if path.is_file()
    } if uploads.is_dir() else {}


def sqlite_backup(db: Path, target: Path) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    with closing(sqlite3.connect(db)) as live, closing(sqlite3.connect(target)) as backup:
        live.backup(backup)


def duplicate_site(source: Path, snapshot: Path, target: Path) -> Path:
    database = target / "database" / "opendash.sqlite"
    database.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(snapshot, database)
    if (source / "uploads").is_dir():
        shutil.copytree(source / "uploads", target / "uploads")
    return database


def start_chrome(browser, base: str):
    page = browser.new_page(viewport={"width": 1440, "height": 900})
    page.goto(base + "/admin/#/builder")
    page.locator("#auth-username").fill("g5admin")
    page.locator("#auth-password").fill("password123")
    page.locator("#auth-login").click()
    expect(page.locator('[data-testid="builder-workspace"]')).to_contain_text("Đã mở")
    return page


def verify(old_binary: Path, new_binary: Path, chrome: str, scripts: Path) -> None:
    sys.path.insert(0, str(scripts))
    from builder_homepage_smoke import checked, element, get, save as api_save, tree
    from builder_editor_browser import save as ui_save
    from builder_list_smoke import create, set_publication
    from content_entries_smoke import api
    from content_types_smoke import login, request, running
    from media_smoke import raw_request, upload_image

    with TemporaryDirectory(prefix="opendash-m4-list-106-g5-") as directory:
        root = Path(directory)
        site = root / "original-site"
        db = site / "database" / "opendash.sqlite"
        before_snapshot = root / "before-list.sqlite"
        after_snapshot = root / "after-list.sqlite"
        layout = tree(
            element("core/heading", {"level": 2, "text": "ORIGINAL HEADING"}, "heading-original"),
            element("core/text", {
                "text": "STATIC ORIGINAL",
                "binding": {
                    "type_key": "projects", "entry_slug": "alpha",
                    "source": {"kind": "title"}, "target": "text",
                },
            }, "binding-original"),
        )
        layout["sections"][0]["children"][0]["responsive"] = {
            "desktop": {"gap_px": 0}, "mobile": {"columns_per_row": 1},
        }

        # Baseline site and homepage are actually written by the pre-#106
        # binary, never synthesized by direct DB mutation.
        with running(old_binary, db) as base:
            checked(request(base, "/auth/setup", {
                "username": "g5admin", "password": "password123",
            }), 201, "old admin setup")
            admin = login(base, "g5admin", "password123")
            asset_id = upload_image(base, admin, seed=108)["id"]
            checked(api(base, "POST", "/api/admin/content-types", {
                "key": "projects", "label": "Projects",
                "fields": [
                    {"key": "summary", "label": "Summary", "type": "short_text"},
                    {"key": "cover", "label": "Cover", "type": "image"},
                ],
            }, admin), 201, "old CPT")
            first = create(base, admin, "projects", "alpha", "Project & Alpha",
                           values={"summary": "First", "cover": asset_id})
            second = create(base, admin, "projects", "beta", "Project Beta",
                            values={"summary": "Second", "cover": asset_id})
            assert first["publication_status"] == second["publication_status"] == "published"
            secret = create(base, admin, "post", "secret-draft", "PRIVATE DRAFT")
            assert secret["publication_status"] == "draft"
            approved = create(base, admin, "post", "old-post", "Published old post")
            set_publication(base, admin, approved, "post", "published")
            recent = create(base, admin, "post", "new-post", "Published new post")
            set_publication(base, admin, recent, "post", "published")
            checked(api_save(base, admin, layout, 1), 200, "old builder layout")
            saved_baseline = get(base, admin)
            assert saved_baseline["version"] == 2
            assert saved_baseline["document"] == layout
            old_html = raw_request(base, "GET", "/")[1].decode()
            assert "ORIGINAL HEADING" in old_html
            assert "Project &amp; Alpha" in old_html
            assert "PRIVATE DRAFT" not in old_html
            assert "builder-list" not in old_html

        initial_rows = snapshot_rows(db)
        initial_hash = fingerprint(initial_rows)
        media_before = media_hashes(site)
        assert media_before, "actual uploaded bytes must be snapshotted"
        sqlite_backup(db, before_snapshot)
        assert fingerprint(snapshot_rows(before_snapshot)) == initial_hash

        # New Inspector adds core/list to a persisted pre-#106 layout.
        with running(new_binary, db) as base, sync_playwright() as pw:
            browser = pw.chromium.launch(executable_path=chrome)
            page = start_chrome(browser, base)
            errors: list[str] = []
            page.on("pageerror", lambda error: errors.append(str(error)))
            page.locator("#builder-element-type").select_option("core/list")
            page.get_by_role("treeitem", name=re.compile("Column")).first.click()
            page.get_by_role("button", name="Thêm element vào Column").click()
            list_id = page.locator('[role="treeitem"][data-node-id^="element-"]').last.get_attribute("data-node-id")
            assert list_id and list_id not in ("heading-original", "binding-original")
            page.get_by_label("List content type").select_option("projects")
            page.get_by_label("List sort").select_option("id_asc")
            page.get_by_label("List page size").fill("1")
            page.get_by_role("treeitem", name=re.compile("Column")).first.click()
            page.get_by_role("button", name="Thêm element vào Column").click()
            post_list_id = page.locator('[role="treeitem"][data-node-id^="element-"]').last.get_attribute("data-node-id")
            assert post_list_id and post_list_id != list_id
            page.get_by_label("List content type").select_option("post")
            page.get_by_label("List page size").fill("1")

            assert get(base, login(base, "g5admin", "password123")) == saved_baseline
            page.get_by_role("button", name="Xem trước").click()
            expected_list = page.frame_locator("#canvas").locator(".builder-list")
            expect(expected_list).to_have_count(2)
            expect(expected_list.nth(0)).to_contain_text("Project & Alpha")
            expect(expected_list.nth(1)).to_contain_text("Published new post")
            expect(expected_list.nth(0)).not_to_contain_text("Project Beta")
            expect(expected_list.nth(1)).not_to_contain_text("Published old post")
            assert "PRIVATE DRAFT" not in (page.locator("#canvas").get_attribute("srcdoc") or "")
            assert "builder-list-next" not in (page.locator("#canvas").get_attribute("srcdoc") or "")
            assert "builder-list" not in raw_request(base, "GET", "/")[1].decode()
            page.get_by_role("button", name="Canvas cấu trúc").click()
            ui_save(page)
            newer = get(base, login(base, "g5admin", "password123"))
            assert newer["version"] == 3
            newer_layout = newer["document"]
            elements = newer_layout["sections"][0]["children"][0]["children"][0]["children"]
            new_node = next(e for e in elements if e["id"] == list_id)
            post_node = next(e for e in elements if e["id"] == post_list_id)
            assert new_node["type"] == "core/list"
            assert post_node["type"] == "core/list"
            assert post_node["props"] == {
                "type_key": "post", "sort": "id_desc",
                "include_descendants": False, "page_size": 1,
            }, post_node["props"]
            assert new_node["props"] == {
                "type_key": "projects", "sort": "id_asc",
                "include_descendants": False, "page_size": 1,
            }, new_node["props"]
            assert next(e for e in elements if e["id"] == "binding-original")["props"]["binding"] == (
                layout["sections"][0]["children"][0]["children"][0]["children"][1]["props"]["binding"]
            )
            assert newer_layout["sections"][0]["children"][0]["responsive"] == (
                layout["sections"][0]["children"][0]["responsive"]
            )
            html = raw_request(base, "GET", "/")[1].decode()
            assert "Project &amp; Alpha" in html and "PRIVATE DRAFT" not in html
            assert "builder-list-next" in html
            assert "Published new post" in html and "Published old post" not in html
            second_page = raw_request(base, "GET", "/?" + urlencode({
                f"list_page_{list_id}": 2,
                f"list_page_{post_list_id}": 2,
                "irrelevant": "discard",
            }))[1].decode()
            assert "Project Beta" in second_page
            assert "Published old post" in second_page
            assert "Project &amp; Alpha" in second_page, "exact-entry binding remains independent"
            assert "irrelevant=" not in second_page
            assert f"list_page_{post_list_id}" in second_page
            assert f"list_page_{list_id}" in second_page
            assert not errors, errors
            browser.close()

        new_rows = snapshot_rows(db)
        assert initial_rows["builder_homepage"] != new_rows["builder_homepage"]
        for table in TABLES:
            if table != "builder_homepage":
                assert new_rows[table] == initial_rows[table], table
        assert media_hashes(site) == media_before
        sqlite_backup(db, after_snapshot)

        # Old executable on copy of *new* DB may not render the unknown list,
        # but must preserve the node as an immutable historical anchor when
        # editing an unrelated heading and saving through old Chrome Admin UI.
        old_site = root / "old-ui-with-new-document"
        old_db = duplicate_site(site, after_snapshot, old_site)
        with running(old_binary, old_db) as base, sync_playwright() as pw:
            browser = pw.chromium.launch(executable_path=chrome)
            page = start_chrome(browser, base)
            expect(page.get_by_label("List content type")).to_have_count(0)
            page.locator(f'[role="treeitem"][data-node-id="{list_id}"]').click()
            expect(page.locator('[aria-label="Inspector"]')).to_contain_text("Node không resolve")
            page.locator(f'[role="treeitem"][data-node-id="{post_list_id}"]').click()
            expect(page.locator('[aria-label="Inspector"]')).to_contain_text("Node không resolve")
            assert "builder-list" not in raw_request(base, "GET", "/")[1].decode()
            page.locator('[role="treeitem"][data-node-id="heading-original"]').click()
            page.get_by_label("Tiêu đề", exact=True).fill("OLD UI STATIC CHANGE")
            ui_save(page)
            previous_state = get(base, login(base, "g5admin", "password123"))
            previous_nodes = previous_state["document"]["sections"][0]["children"][0]["children"][0]["children"]
            assert next(e for e in previous_nodes if e["id"] == list_id) == new_node
            assert next(e for e in previous_nodes if e["id"] == post_list_id) == post_node
            assert next(e for e in previous_nodes if e["id"] == "binding-original") == (
                next(e for e in elements if e["id"] == "binding-original")
            )
            assert previous_state["document"]["sections"][0]["children"][0]["responsive"] == (
                layout["sections"][0]["children"][0]["responsive"]
            )
            old_html = raw_request(base, "GET", "/")[1].decode()
            assert "OLD UI STATIC CHANGE" in old_html and "Project &amp; Alpha" in old_html
            assert "builder-list" not in old_html and "PRIVATE DRAFT" not in old_html
            browser.close()
        for table in TABLES:
            if table != "builder_homepage":
                assert snapshot_rows(old_db)[table] == initial_rows[table], table
        assert media_hashes(old_site) == media_before

        # Returning to the new runtime reactivates the same saved list after an
        # old-UI edit, without a migration or recomputing cached results.
        with running(new_binary, old_db) as base:
            revived = get(base, login(base, "g5admin", "password123"))["document"]
            assert next(e for e in revived["sections"][0]["children"][0]["children"][0]["children"]
                        if e["id"] == list_id) == new_node
            assert next(e for e in revived["sections"][0]["children"][0]["children"][0]["children"]
                        if e["id"] == post_list_id) == post_node
            assert "Project Beta" in raw_request(
                base, "GET", "/?" + urlencode({
                    f"list_page_{list_id}": 2, f"list_page_{post_list_id}": 2,
                }),
            )[1].decode()
            assert "Published old post" in raw_request(
                base, "GET", "/?" + urlencode({f"list_page_{post_list_id}": 2}),
            )[1].decode()

        # Real old snapshot restored to separate site and matching old binary.
        restored_site = root / "restored-original"
        restored_db = duplicate_site(site, before_snapshot, restored_site)
        assert fingerprint(snapshot_rows(restored_db)) == initial_hash
        assert media_hashes(restored_site) == media_before
        with running(old_binary, restored_db) as base:
            restored = get(base, login(base, "g5admin", "password123"))
            assert restored == saved_baseline
            text = raw_request(base, "GET", "/")[1].decode()
            assert "ORIGINAL HEADING" in text and "Project &amp; Alpha" in text
            assert "builder-list" not in text and "PRIVATE DRAFT" not in text
        assert fingerprint(snapshot_rows(restored_db)) == initial_hash

        print("PASS #106 G5: actual Intel old/new/old Chrome Inspector, two independent lists, unknown-list preservation, dynamic list return")
        print(f"PASS #106 G5: SQLite v0014 snapshot and media hashes restored, pre-fingerprint={initial_hash}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--old-binary", type=Path, required=True)
    parser.add_argument("--new-binary", type=Path, required=True)
    parser.add_argument("--chrome", required=True)
    parser.add_argument("--source", type=Path, required=True)
    args = parser.parse_args()
    verify(
        args.old_binary.resolve(),
        args.new_binary.resolve(),
        args.chrome,
        args.source.resolve() / "cms" / "scripts",
    )
