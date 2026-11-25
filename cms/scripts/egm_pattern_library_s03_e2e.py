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
import re
import subprocess
import tempfile
import zipfile

from playwright.sync_api import expect, sync_playwright
from m7_beta_menu_picker_e2e import admin_login, api_json, launch_server, stop_server
from media_smoke import make_png


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


def section_text(section: dict) -> str:
    return " ".join(str(element.get("props", {}).get("text", ""))
                    for row in section["children"]
                    for column in row["children"]
                    for element in column["children"])


def expect_preview_after(page, entry_url: str, action) -> dict:
    """Capture the exact unsaved AST sent by Studio's debounced preview POST."""
    with page.expect_response(
        lambda response: response.url.endswith(entry_url + "/preview")
        and response.request.method == "POST",
        timeout=20000,
    ) as pending:
        action()
    response = pending.value
    assert response.status == 200, response.text()[:450]
    return response.request.post_data_json


def ensure_tree_view(page) -> None:
    for _ in range(4):
        back = page.get_by_test_id("studio-inline-back-btn")
        if back.count() == 0:
            break
        back.click()


def node_locator(page, doc: dict, target_id: str):
    """Expand any collapsed container ancestors along the path to `target_id`."""
    ensure_tree_view(page)
    ancestors: list[str] = []
    found = False
    for sec in doc.get("sections", []):
        if sec["id"] == target_id:
            found = True
            break
        for row in sec.get("children", []):
            if row["id"] == target_id:
                ancestors = [sec["id"]]
                found = True
                break
            for col in row.get("children", []):
                if col["id"] == target_id:
                    ancestors = [sec["id"], row["id"]]
                    found = True
                    break
                for el in col.get("children", []):
                    if el["id"] == target_id:
                        ancestors = [sec["id"], row["id"], col["id"]]
                        found = True
                        break
                if found:
                    break
            if found:
                break
        if found:
            break
    for anc in ancestors:
        loc = page.locator(f'[role="treeitem"][data-node-id="{anc}"]')
        if loc.count() > 0 and loc.get_attribute("aria-expanded") == "false":
            loc.focus()
            loc.press("ArrowRight")
    item = page.locator(f'[role="treeitem"][data-node-id="{target_id}"]')
    expect(item).to_be_visible(timeout=10000)
    return item


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
                    csrf = admin_login(ctx, base, "s03_admin", "AdminPassword123!", cms_locale="vi")
                    library = "/api/admin/patterns/library"
                    starter = api_json(ctx, "GET", base, library)
                    assert all(x["id"].startswith("theme:opendash-starter:") for x in starter["items"]), starter
                    checks.append("auth_guard_and_starter_default_pattern_catalog")

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
                    contact_source = api_json(ctx, "GET", base, library + "/theme/contact")
                    assert contact_source["document"] == ast("contact", "THEME CONTACT COPY SOURCE")
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
                    page.goto(base + f"/admin/#/studio/entry/page/{page_id}",
                              wait_until="domcontentloaded")
                    workspace = page.get_by_test_id("studio-workspace")
                    expect(workspace).to_be_visible(timeout=20000)
                    expect(page.get_by_test_id("studio-mode2-sidebar")).to_be_visible(timeout=20000)
                    expect(page.get_by_test_id("studio-save-draft-btn")).to_be_visible()
                    footer = page.get_by_test_id("studio-sidebar-footer")
                    footer.get_by_test_id("studio-open-pattern-library").click()
                    checks.append("studio_mode2_sidebar_and_pattern_library_entry")

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
                    first = expect_preview_after(
                        page, entry_url, lambda: page.get_by_test_id("insert-theme-pattern-hero").click()
                    )
                    expect(modal).to_have_count(0)
                    expect(page.get_by_test_id("studio-dirty-badge")).to_be_visible(timeout=10000)
                    assert api_json(ctx, "GET", base, entry_url) == before
                    # No node is selected in the initially empty Studio tree. The first
                    # insert exercises the fallback append path.
                    assert [section_text(section) for section in first["sections"]] == [
                        "THEME HERO COPY SOURCE"
                    ], first
                    assert len(node_ids(first)) == 4
                    assert not (node_ids(first) & node_ids(source["document"]))
                    assert api_json(ctx, "GET", base, entry_url) == before
                    checks.append("browser_modal_filter_and_empty_selection_fallback_append")

                    footer.get_by_test_id("studio-open-pattern-library").click()
                    expect(page.get_by_test_id("pattern-library-grid").locator("article")).to_have_count(2)
                    second = expect_preview_after(
                        page, entry_url, lambda: page.get_by_test_id("insert-theme-pattern-contact").click()
                    )
                    expect(page.get_by_test_id("pattern-library-modal")).to_have_count(0)
                    assert [section_text(section) for section in second["sections"]] == [
                        "THEME HERO COPY SOURCE", "THEME CONTACT COPY SOURCE"
                    ], second

                    # Inserting after a selected Section must preserve the next
                    # sibling's position, rather than always appending.
                    first_section_id = second["sections"][0]["id"]
                    node_locator(page, second, first_section_id).click()
                    footer.get_by_test_id("studio-open-pattern-library").click()
                    inserted_after_section = expect_preview_after(
                        page, entry_url, lambda: page.get_by_test_id("insert-theme-pattern-hero").click()
                    )
                    expect(page.get_by_test_id("pattern-library-modal")).to_have_count(0)
                    assert [section_text(section) for section in inserted_after_section["sections"]] == [
                        "THEME HERO COPY SOURCE", "THEME HERO COPY SOURCE",
                        "THEME CONTACT COPY SOURCE"
                    ], inserted_after_section
                    assert inserted_after_section["sections"][0]["id"] == first_section_id
                    assert inserted_after_section["sections"][2]["id"] == second["sections"][1]["id"]
                    checks.append("pattern_insert_after_selected_section_preserves_siblings")

                    # Selection within a Row/Column/Element resolves to its
                    # ancestor Section. A repeated insert gets new IDs again.
                    heading_id = inserted_after_section["sections"][0]["children"][0]["children"][0]["children"][0]["id"]
                    node_locator(page, inserted_after_section, heading_id).click()
                    footer.get_by_test_id("studio-open-pattern-library").click()
                    unsaved = expect_preview_after(
                        page, entry_url, lambda: page.get_by_test_id("insert-theme-pattern-contact").click()
                    )
                    expect(page.get_by_test_id("pattern-library-modal")).to_have_count(0)
                    assert [section_text(section) for section in unsaved["sections"]] == [
                        "THEME HERO COPY SOURCE", "THEME CONTACT COPY SOURCE",
                        "THEME HERO COPY SOURCE", "THEME CONTACT COPY SOURCE"
                    ], unsaved
                    ids = node_ids(unsaved)
                    assert len(unsaved["sections"]) == 4 and len(ids) == 16, unsaved
                    source_ids = node_ids(source["document"]) | node_ids(contact_source["document"])
                    assert not (ids & source_ids)
                    assert unsaved["sections"][0]["id"] == first_section_id
                    assert unsaved["sections"][2]["id"] == inserted_after_section["sections"][1]["id"]
                    assert unsaved["sections"][3]["id"] == second["sections"][1]["id"]
                    assert api_json(ctx, "GET", base, entry_url) == before
                    assert api_json(ctx, "GET", base, library + "/theme/hero") == source
                    assert api_json(ctx, "GET", base, library + "/theme/contact") == contact_source
                    page.screenshot(path=str(artifacts / "pattern-copies-before-save.png"), full_page=True)
                    checks.append("descendant_selection_order_repeated_copy_unique_ids_and_no_unsaved_write")

                    page.on("dialog", lambda dialog: dialog.accept())
                    page.get_by_test_id("studio-save-draft-btn").click()
                    expect(page.get_by_test_id("studio-dirty-badge")).to_have_count(0, timeout=20000)
                    saved = api_json(ctx, "GET", base, entry_url)
                    assert saved["document"] == unsaved
                    assert saved["version"] == before["version"] + 1
                    assert api_json(ctx, "GET", base, library + "/theme/hero") == source
                    assert api_json(ctx, "GET", base, library + "/theme/contact") == contact_source
                    checks.append("save_cas_persists_copy_not_theme_source")

                    # Edit the page copy through Studio Inspector and Save; immutable ZIP content survives.
                    node_locator(page, saved["document"], heading_id).dblclick()
                    page.get_by_label("Tiêu đề", exact=True).fill("EDITED PAGE COPY")
                    page.get_by_test_id("studio-save-draft-btn").click()
                    expect(page.get_by_test_id("studio-dirty-badge")).to_have_count(0, timeout=20000)
                    after_edit = api_json(ctx, "GET", base, entry_url)
                    assert "EDITED PAGE COPY" in json.dumps(after_edit["document"])
                    assert api_json(ctx, "GET", base, library + "/theme/hero") == source
                    assert api_json(ctx, "GET", base, library + "/theme/contact") == contact_source
                    assert api_json(ctx, "GET", base, library + "?q=Welcome")["items"]
                    ensure_tree_view(page)
                    page.screenshot(path=str(artifacts / "pattern-copy-after-edit.png"), full_page=True)
                    checks.append("page_copy_edit_does_not_write_immutable_theme_or_raise_js_errors")

                    # Media insertion inside Studio Add catalog uses the authenticated
                    # picker without saving the Page or exposing private media.
                    uploaded = ctx.request.post(base + "/api/admin/media", data=make_png(seed=53),
                                                headers={"Origin": base, "X-CSRF-Token": csrf,
                                                         "Content-Type": "image/png",
                                                         "X-File-Name": "s03-private.png"})
                    assert uploaded.status == 201, uploaded.text()[:400]
                    media_id = uploaded.json()["id"]
                    assert ctx.request.get(base + f"/api/admin/media/{media_id}/file").status == 200
                    assert anon.request.get(base + f"/api/admin/media/{media_id}/file").status == 401
                    assert anon.request.get(base + f"/media/{media_id}/file").status == 404
                    node_locator(page, after_edit["document"], heading_id).click()
                    page.get_by_role("button", name="Thêm Block").first.click()
                    page.get_by_test_id("studio-open-media-picker").click()
                    picker = page.get_by_role("dialog", name="Chọn media từ Media Library")
                    expect(picker).to_be_visible(timeout=10000)
                    picker.locator('.media-tile[title="s03-private.png"]').click()
                    with_image = expect_preview_after(
                        page, entry_url, lambda: picker.get_by_role("button", name="Dùng media đã chọn").click()
                    )
                    expect(picker).to_have_count(0)
                    new_images = [
                        element for section in with_image["sections"]
                        for row in section["children"]
                        for column in row["children"]
                        for element in column["children"]
                        if element["type"] == "core/image" and element["props"].get("media_id") == media_id
                    ]
                    assert len(new_images) == 1, with_image
                    assert len(node_ids(with_image)) > len(node_ids(after_edit["document"])), with_image
                    assert not (node_ids(with_image) & source_ids)
                    assert api_json(ctx, "GET", base, entry_url) == after_edit
                    assert anon.request.get(base + f"/media/{media_id}/file").status == 404
                    assert api_json(ctx, "GET", base, library + "/theme/hero") == source
                    assert api_json(ctx, "GET", base, library + "/theme/contact") == contact_source
                    ensure_tree_view(page)
                    page.screenshot(path=str(artifacts / "media-picker-unsaved-copy.png"), full_page=True)
                    checks.append("authenticated_media_picker_unsaved_insert_private_acl_and_source_immutable")

                    # Mode 2 Pattern editing (#/studio/entry/pattern/:id), Exit -> #/patterns/:id/edit,
                    # legacy #/builder Pattern redirect, and Editor RBAC rejection.
                    page.goto(base + "/admin/#/patterns/new", wait_until="domcontentloaded")
                    expect(page.get_by_test_id("pattern-workspace")).to_be_visible(timeout=15000)
                    page.get_by_label("Tiêu đề Pattern").fill("S03 Studio Pattern")
                    page.get_by_label("Slug Pattern").fill("s03-studio-pattern")
                    page.get_by_role("button", name="Lưu Pattern").click()
                    expect(page).to_have_url(re.compile(r"#/patterns/[1-9]\d*/edit$"), timeout=15000)
                    pat_id = int(re.search(r"#/patterns/(\d+)/edit", page.url).group(1))
                    page.get_by_role("button", name="Mở Pattern Builder").click()
                    expect(page).to_have_url(base + f"/admin/#/studio/entry/pattern/{pat_id}", timeout=15000)
                    expect(page.get_by_test_id("studio-mode2-sidebar")).to_be_visible(timeout=15000)
                    expect(page.get_by_test_id("studio-save-btn")).to_have_text("Lưu Pattern")
                    page.get_by_role("button", name="Thêm Block").first.click()
                    expect(page.get_by_role("button", name="Heading H1–H6")).to_be_visible()
                    expect(page.get_by_role("button", name="Pattern tái sử dụng")).to_have_count(0)
                    page.get_by_role("button", name="Heading H1–H6").click()
                    page.get_by_label("Tiêu đề", exact=True).fill("PATTERN MODE 2 HEADING")
                    page.get_by_test_id("studio-save-btn").click()
                    expect(page.get_by_test_id("studio-dirty-badge")).to_have_count(0, timeout=15000)
                    pat_doc = api_json(ctx, "GET", base, f"/api/admin/entries/pattern/{pat_id}/builder")
                    assert "PATTERN MODE 2 HEADING" in json.dumps(pat_doc["document"]), pat_doc
                    assert anon.request.get(base + "/content/pattern/s03-studio-pattern").status == 404
                    page.get_by_test_id("studio-exit-btn").click()
                    expect(page).to_have_url(base + f"/admin/#/patterns/{pat_id}/edit", timeout=15000)
                    page.goto(base + f"/admin/#/builder?template=entry&type=pattern&entry={pat_id}", wait_until="domcontentloaded")
                    expect(page).to_have_url(base + f"/admin/#/studio/entry/pattern/{pat_id}", timeout=15000)
                    expect(page.get_by_test_id("studio-mode2-sidebar")).to_be_visible(timeout=15000)
                    checks.append("studio_mode2_pattern_save_exit_and_legacy_builder_redirect")

                    api_json(ctx, "POST", base, "/api/admin/users",
                             {"username": "s03_editor", "password": "EditorPassword123!", "role": "editor"},
                             csrf, expected=201)
                    editor_ctx = browser.new_context(viewport={"width": 1440, "height": 900})
                    admin_login(editor_ctx, base, "s03_editor", "EditorPassword123!")
                    epage = editor_ctx.new_page()
                    epage.goto(base + f"/admin/#/studio/entry/pattern/{pat_id}", wait_until="domcontentloaded")
                    expect(epage.get_by_test_id("studio-workspace")).to_have_count(0, timeout=10000)
                    expect(epage.get_by_test_id("rbac-forbidden")).to_be_visible(timeout=10000)
                    assert editor_ctx.request.get(base + f"/api/admin/entries/pattern/{pat_id}/builder").status == 403
                    editor_ctx.close()
                    checks.append("editor_denied_studio_pattern_mode2_ui_and_api_403")
                    assert not errors, errors
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
