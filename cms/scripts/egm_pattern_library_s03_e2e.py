#!/usr/bin/env python3
"""EGM Spec 2 S03: native Chrome + API proof on a disposable site.

Uses a random loopback port and a fresh temporary site. Never touches port 18964.
Usage: python3 cms/scripts/egm_pattern_library_s03_e2e.py --binary cms/target/debug/opendash
"""
from __future__ import annotations

import argparse
import base64
import hashlib
import io
import json
import pathlib
import platform
import subprocess
import tempfile
import zipfile

from playwright.sync_api import expect, sync_playwright
from m7_beta_menu_picker_e2e import admin_login, api_json, launch_server, stop_server


def ast(prefix: str, label: str) -> dict:
    return {"schema_version": 1, "sections": [{
        "id": prefix + "-section", "children": [{
            "id": prefix + "-row", "children": [{
                "id": prefix + "-column", "children": [{
                    "id": prefix + "-heading", "type": "core/heading",
                    "props": {"level": 2, "text": label},
                }],
            }],
        }],
    }]}


def node_ids(doc: dict) -> set[str]:
    ids: set[str] = set()
    for section in doc["sections"]:
        ids.add(section["id"])
        for row in section["children"]:
            ids.add(row["id"])
            for column in row["children"]:
                ids.add(column["id"])
                for element in column["children"]:
                    ids.add(element["id"])
    return ids


def zip_v3() -> bytes:
    manifest = {
        "manifest_version": 3, "theme_id": "s03-patterns", "name": "S03 Patterns",
        "version": "1.0.0",
        "templates": [{"kind": "page", "file": "page.html"}],
        "patterns": [
            {"key": "hero", "title": "Welcome Hero", "category": "landing",
             "description": "Welcome to our community", "file": "patterns/hero.json",
             "preview_asset": "assets/hero.png"},
            {"key": "contact", "title": "Contact CTA", "category": "cta",
             "description": "Get in touch", "file": "patterns/contact.json"},
        ],
        "assets": ["assets/hero.png"],
    }
    png = base64.b64decode(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/aX8AAAAASUVORK5CYII="
    )
    package = io.BytesIO()
    with zipfile.ZipFile(package, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("theme.json", json.dumps(manifest))
        archive.writestr("page.html", "<main>{{ od_content() }}</main>")
        archive.writestr("patterns/hero.json", json.dumps(ast("hero", "THEME HERO COPY SOURCE")))
        archive.writestr("patterns/contact.json", json.dumps(ast("contact", "THEME CONTACT COPY SOURCE")))
        archive.writestr("assets/hero.png", png)
    return package.getvalue()


def run(binary: pathlib.Path, artifacts: pathlib.Path) -> dict:
    checks: list[str] = []
    errors: list[str] = []
    artifacts.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="opendash-pattern-s03-") as directory:
        root = pathlib.Path(directory).resolve() / "site"
        root.mkdir(mode=0o700)
        server, base = launch_server(binary, root)
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel="chrome", headless=True)
                try:
                    ctx = browser.new_context(viewport={"width": 1440, "height": 900})
                    page = ctx.new_page()
                    page.on("pageerror", lambda error: errors.append(str(error)))
                    anon = browser.new_context()
                    assert anon.request.get(base + "/api/admin/patterns/library").status == 401
                    api_json(ctx, "POST", base, "/auth/setup",
                             {"username": "s03_admin", "password": "AdminPassword123!"}, expected=201)
                    csrf = admin_login(ctx, base, "s03_admin", "AdminPassword123!")
                    library = "/api/admin/patterns/library"
                    no_theme = api_json(ctx, "GET", base, library)
                    assert no_theme["items"] == [], no_theme
                    checks.append("auth_guard_and_no_active_theme_empty_catalog")

                    upload = ctx.request.post(base + "/api/admin/themes", data=zip_v3(), headers={
                        "Origin": base, "X-CSRF-Token": csrf, "Content-Type": "application/zip",
                    })
                    assert upload.status == 201, upload.text()[:400]
                    current = api_json(ctx, "GET", base, "/api/admin/themes")
                    api_json(ctx, "PUT", base, "/api/admin/themes/active",
                             {"revision_id": upload.json()["revision_id"]}, csrf,
                             {"If-Match": f'"v{current["version"]}"'})
                    catalog = api_json(ctx, "GET", base, library)
                    assert {x["key"] for x in catalog["items"]} == {"hero", "contact"}, catalog
                    assert set(catalog["categories"]) == {"all", "landing", "cta"}
                    hero_meta = next(x for x in catalog["items"] if x["key"] == "hero")
                    assert hero_meta["source"] == "theme"
                    assert hero_meta["preview_url"].startswith("/theme/")
                    assert ctx.request.get(base + hero_meta["preview_url"]).status == 200
                    assert [x["key"] for x in api_json(ctx, "GET", base, library + "?category=cta")["items"]] == ["contact"]
                    assert [x["key"] for x in api_json(ctx, "GET", base, library + "?q=COMMUNITY")["items"]] == ["hero"]
                    assert api_json(ctx, "GET", base, library + "?category=cta&q=welcome")["items"] == []
                    source = api_json(ctx, "GET", base, library + "/theme/hero")
                    assert source["source"] == "theme" and source["document"] == ast("hero", "THEME HERO COPY SOURCE")
                    assert ctx.request.get(base + library + "/theme/invalid-key").status == 400
                    assert ctx.request.get(base + library + "/theme/unknown").status == 404
                    checks.append("theme_v3_catalog_category_q_preview_and_immutable_source")

                    created = api_json(ctx, "POST", base, "/api/admin/content-types/page/entries",
                                       {"title": "S03 Draft", "slug": "s03-draft", "values": {}},
                                       csrf, expected=201)
                    page_id = created["id"]
                    entry_url = f"/api/admin/entries/page/{page_id}/builder"
                    before = api_json(ctx, "GET", base, entry_url)
                    assert before["document"] is None
                    page.goto(base + f"/admin/#/builder?template=page&type=page&entry={page_id}",
                              wait_until="domcontentloaded")
                    workspace = page.get_by_test_id("builder-workspace")
                    expect(workspace).to_contain_text("Đã mở", timeout=20000)
                    page.get_by_role("tab", name="Patterns").click()
                    page.get_by_test_id("open-pattern-library").click()
                    modal = page.get_by_test_id("pattern-library-modal")
                    expect(modal).to_be_visible(timeout=15000)
                    expect(page.get_by_test_id("pattern-library-grid").locator("article")).to_have_count(2)
                    modal.get_by_role("button", name="cta").click()
                    expect(page.get_by_test_id("pattern-library-grid").locator("article")).to_have_count(1)
                    modal.get_by_role("button", name="Tất cả").click()
                    modal.get_by_placeholder("Tìm theo tên, mô tả hoặc danh mục…").fill("COMMUNITY")
                    expect(page.get_by_test_id("pattern-library-grid").locator("article")).to_have_count(1)
                    modal.get_by_placeholder("Tìm theo tên, mô tả hoặc danh mục…").fill("")
                    expect(page.get_by_test_id("pattern-library-grid").locator("article")).to_have_count(2)
                    page.get_by_test_id("insert-theme-pattern-hero").click()
                    expect(modal).to_have_count(0)
                    expect(workspace).to_contain_text("Đã chèn bản sao", timeout=10000)
                    assert api_json(ctx, "GET", base, entry_url) == before
                    checks.append("browser_modal_search_filter_and_unsaved_insert")

                    page.get_by_test_id("open-pattern-library").click()
                    expect(page.get_by_test_id("pattern-library-grid").locator("article")).to_have_count(2)
                    page.get_by_test_id("insert-theme-pattern-hero").click()
                    expect(page.get_by_test_id("pattern-library-modal")).to_have_count(0)
                    # Preview request carries both copied sections with globally distinct IDs.
                    with page.expect_response(lambda response: response.url.endswith(entry_url + "/preview")
                                              and response.request.method == "POST") as preview_request:
                        page.get_by_role("button", name="Xem trước").click()
                    preview = preview_request.value
                    assert preview.status == 200, preview.text()[:450]
                    unsaved = preview.request.post_data_json
                    ids = node_ids(unsaved)
                    assert len(unsaved["sections"]) == 2 and len(ids) == 8, unsaved
                    assert not (ids & node_ids(source["document"]))
                    assert all("THEME HERO COPY SOURCE" in json.dumps(section) for section in unsaved["sections"])
                    assert api_json(ctx, "GET", base, entry_url) == before
                    page.get_by_role("button", name="Canvas cấu trúc").click()
                    page.screenshot(path=str(artifacts / "pattern-copies-before-save.png"), full_page=True)
                    checks.append("two_copies_all_node_ids_unique_preview_and_no_persist_before_save")

                    page.on("dialog", lambda dialog: dialog.accept())
                    page.get_by_role("button", name="Lưu Page").click()
                    expect(workspace).to_contain_text("Đã lưu", timeout=20000)
                    saved = api_json(ctx, "GET", base, entry_url)
                    assert saved["document"] == unsaved
                    assert saved["version"] == before["version"] + 1
                    assert api_json(ctx, "GET", base, library + "/theme/hero") == source
                    checks.append("save_cas_persists_copy_not_theme_source")

                    # Edit the page copy through Inspector and Save; immutable ZIP content survives.
                    page.get_by_role("tab", name="Blocks").click()
                    heading_id = unsaved["sections"][0]["children"][0]["children"][0]["children"][0]["id"]
                    page.locator('[role="treeitem"][data-node-id="' + heading_id + '"]').click()
                    page.get_by_label("Tiêu đề", exact=True).fill("EDITED PAGE COPY")
                    page.get_by_role("button", name="Lưu Page").click()
                    expect(workspace).to_contain_text("Đã lưu", timeout=20000)
                    after_edit = api_json(ctx, "GET", base, entry_url)
                    assert "EDITED PAGE COPY" in json.dumps(after_edit["document"])
                    assert api_json(ctx, "GET", base, library + "/theme/hero") == source
                    assert api_json(ctx, "GET", base, library + "?q=Welcome")["items"]
                    assert not errors, errors
                    page.screenshot(path=str(artifacts / "pattern-copy-after-edit.png"), full_page=True)
                    checks.append("page_copy_edit_does_not_write_immutable_theme_or_raise_js_errors")
                finally:
                    browser.close()
        finally:
            stop_server(server)
    result = {
        "source_sha": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        "binary": str(binary), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "host": platform.platform(), "checks": checks, "status": "PASS",
    }
    (artifacts / "receipt.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--artifact-dir", type=pathlib.Path, default=pathlib.Path(".g5-checkouts/evidence/egm-pattern-s03"))
    args = parser.parse_args()
    print(json.dumps(run(args.binary.resolve(), args.artifact_dir.resolve()), indent=2))
