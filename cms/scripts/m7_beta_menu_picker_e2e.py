#!/usr/bin/env python3
"""M7-S15-T01 F01: real Chrome/Admin menu picker against an isolated native binary.

No production roots or persisted cookies are used. Bulk pagination fixtures are
created through the existing authenticated API; representative Post/Page/CPT
and the menu itself are created through the actual Admin UI.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import re
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request

from playwright.sync_api import expect, sync_playwright


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def wait_for_health(base: str, process: subprocess.Popen) -> None:
    for _ in range(150):
        if process.poll() is not None:
            raise AssertionError(f"Native OpenDash exited early: {process.returncode}")
        try:
            with urllib.request.urlopen(base + "/api/health", timeout=1) as response:
                if response.status == 200:
                    return
        except (OSError, TimeoutError):
            time.sleep(0.1)
    raise AssertionError("Native OpenDash health did not become ready")


def launch_server(binary: pathlib.Path, root: pathlib.Path) -> tuple[subprocess.Popen, str]:
    port = free_port()
    base = f"http://127.0.0.1:{port}"
    env = os.environ.copy()
    env["OPENDASH_SITE_ROOT"] = str(root)
    env["OPENDASH_DB_PATH"] = str(root / "database/opendash.sqlite")
    env["OPENDASH_PORT"] = str(port)
    env.pop("OPENDASH_PUBLIC_ORIGIN", None)
    proc = subprocess.Popen([str(binary)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    wait_for_health(base, proc)
    return proc, base


def stop_server(proc: subprocess.Popen) -> None:
    if proc.poll() is not None:
        return
    proc.terminate()
    try:
        proc.wait(timeout=8)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=8)


def api_json(ctx, method: str, base: str, path: str, data=None, csrf=None, extra=None, expected=200):
    headers = dict(extra or {})
    if csrf:
        headers["X-CSRF-Token"] = csrf
    if data is not None:
        headers["Content-Type"] = "application/json"
    if method != "GET":
        headers["Origin"] = base
    response = ctx.request.fetch(
        base + path, method=method, headers=headers,
        data=json.dumps(data) if data is not None else None,
        timeout=20000,
    )
    assert response.status == expected, (method, path, response.status, response.text()[:260])
    if not response.body():
        return {}
    return response.json()


def admin_login(ctx, base: str, username: str, password: str) -> str:
    body = api_json(ctx, "POST", base, "/auth/login", {"username": username, "password": password})
    return body["csrf_token"]


def create_ui_entry(page, ctx, base: str, kind: str, title: str, slug: str) -> int:
    route = "posts" if kind == "post" else "pages" if kind == "page" else f"cpt/{kind}"
    page.goto(base + f"/admin#/{route}/new", wait_until="domcontentloaded")
    page.locator("#entry-title-input").wait_for(timeout=20000)
    page.locator("#entry-title-input").fill(title)
    page.locator("#entry-slug-input").fill(slug)
    page.get_by_label("Trạng thái xuất bản").select_option("draft")
    page.get_by_role("button", name="Tạo & Lưu nội dung").click()
    expect(page).to_have_url(re.compile(rf"#/{re.escape(route)}/\d+/edit$"), timeout=20000)
    page.get_by_label("Trạng thái xuất bản").select_option("published")
    page.get_by_role("button", name="Lưu thay đổi").click()
    expect(page.get_by_text("Đã lưu thành công", exact=False)).to_be_visible(timeout=20000)
    entries = api_json(ctx, "GET", base, f"/api/admin/content-types/{kind}/entries?limit=50&offset=0")
    row = next((entry for entry in entries if entry["slug"] == slug), None)
    assert row and row["publication_status"] == "published", (kind, slug, entries)
    return int(row["id"])


def visible_entry_picker(page):
    return page.get_by_role("combobox", name="Chọn bài/trang")


def create_ui_menu(page, base: str) -> None:
    page.goto(base + "/admin#/appearance/menus", wait_until="domcontentloaded")
    page.locator("#od-menu-new-key").wait_for(timeout=20000)
    page.locator("#od-menu-new-key").fill("primary_nav")
    page.locator("#od-menu-new-title").fill("G5 Menu")
    page.get_by_role("button", name="Tạo menu", exact=True).click()
    page.get_by_test_id("menu-editor").wait_for(timeout=20000)


def open_ui_menu(page, base: str) -> None:
    page.goto(base + "/admin#/appearance/menus", wait_until="domcontentloaded")
    page.get_by_role("button", name="G5 Menu (primary_nav)").wait_for(timeout=20000)
    page.get_by_role("button", name="G5 Menu (primary_nav)").click()
    page.get_by_test_id("menu-editor").wait_for(timeout=20000)


def run(binary: pathlib.Path, artifact_dir: pathlib.Path) -> int:
    artifact_dir.mkdir(parents=True, exist_ok=True)
    binary_sha = hashlib.sha256(binary.read_bytes()).hexdigest()
    source_sha = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    with tempfile.TemporaryDirectory(prefix="opendash-m7-s15-menu-") as temporary:
        root = pathlib.Path(temporary).resolve() / "site"
        root.mkdir()
        server, base = launch_server(binary, root)
        passed = []
        try:
            with sync_playwright() as p:
                browser = p.chromium.launch(channel="chrome", headless=True)
                browser_version = browser.version
                context = browser.new_context(viewport={"width": 1440, "height": 900})
                page = context.new_page()
                errors = []
                page.on("pageerror", lambda error: errors.append(str(error)))

                api_json(context, "POST", base, "/auth/setup",
                         {"username": "admin", "password": "G5-Test-Admin-123456"}, expected=201)
                csrf = admin_login(context, base, "admin", "G5-Test-Admin-123456")
                # Empty picker uses no phantom options, even though Menu editor exists.
                create_ui_menu(page, base)
                page.get_by_role("button", name="+ Thêm mục").click()
                page.get_by_role("combobox", name="Loại đích").select_option("entry")
                assert visible_entry_picker(page).locator("option").count() == 1
                passed.append("empty_picker")

                # Representative content authored through actual Admin screens.
                page.goto(base + "/admin#/settings/content-types", wait_until="domcontentloaded")
                page.get_by_role("button", name="+ Tạo loại nội dung mới").click()
                page.locator("#content-type-key").fill("portfolio")
                page.locator("#content-type-label").fill("Danh mục dự án")
                page.get_by_role("button", name="Tạo loại nội dung", exact=True).click()
                expect(page.get_by_role("button", name="Danh mục dự án (portfolio)", exact=True)).to_be_visible(timeout=20000)
                post_id = create_ui_entry(page, context, base, "post", "G5 UI Post", "g5-ui-post")
                page_id = create_ui_entry(page, context, base, "page", "G5 UI Page", "g5-ui-page")
                cpt_id = create_ui_entry(page, context, base, "portfolio", "G5 UI CPT", "g5-ui-cpt")
                passed.append("ui_authored_post_page_cpt")

                # The Post created in UI must move beyond the first 50 rows.
                for index in range(50):
                    created = api_json(context, "POST", base, "/api/admin/content-types/post/entries",
                                       {"title": f"Bulk {index:02d}", "slug": f"g5-bulk-{index:02d}", "values": {}},
                                       csrf, expected=201)
                    api_json(context, "POST", base,
                             f"/api/admin/content-types/post/entries/{created['id']}/publication",
                             {"status": "published"}, csrf, {"If-Match": '"v1"'})
                hidden = api_json(context, "POST", base, "/api/admin/content-types/post/entries",
                                  {"title": "Secret unpublished", "slug": "g5-secret-draft", "values": {}},
                                  csrf, expected=201)
                draft_id = int(hidden["id"])

                open_ui_menu(page, base)
                page.get_by_role("button", name="+ Thêm mục").click()
                page.get_by_role("combobox", name="Loại đích").select_option("entry")
                selector = visible_entry_picker(page)
                assert selector.locator(f"option[value='{post_id}']").count() == 0
                assert selector.locator(f"option[value='{page_id}']").count() == 1
                assert selector.locator(f"option[value='{cpt_id}']").count() == 1
                assert selector.locator(f"option[value='{draft_id}']").count() == 0
                more = page.get_by_role("button", name="Tải thêm bài/trang/CPT")
                expect(more).to_be_visible()
                more.click()
                expect(selector.locator(f"option[value='{post_id}']")).to_have_count(1, timeout=20000)
                assert selector.locator(f"option[value='{draft_id}']").count() == 0
                passed.append("bounded_cpt_pagination_and_no_draft")

                targets = [("Post menu", post_id), ("Page menu", page_id), ("CPT menu", cpt_id)]
                for index, (label, entry_id) in enumerate(targets):
                    if index:
                        page.get_by_role("button", name="+ Thêm mục").click()
                        page.get_by_role("combobox", name="Loại đích").nth(index).select_option("entry")
                    page.get_by_role("textbox", name="Nhãn mục menu").nth(index).fill(label)
                    visible_entry_picker(page).nth(index).select_option(str(entry_id))
                page.get_by_role("button", name="Lưu menu").click()
                expect(page.get_by_text("Đã lưu menu.")).to_be_visible(timeout=20000)
                menu_page = api_json(context, "GET", base, "/api/admin/menus?limit=50")
                menu_id = next(m["id"] for m in menu_page["items"] if m["menu_key"] == "primary_nav")
                menu = api_json(context, "GET", base, f"/api/admin/menus/{menu_id}")
                assert {item["target_entry_id"] for item in menu["items"]} == {post_id, page_id, cpt_id}
                stable_item_ids = {item["id"] for item in menu["items"]}
                preview = api_json(context, "POST", base, f"/api/admin/menus/{menu_id}/preview",
                                   {"title": "G5 Menu", "items": [
                                       {"id": item["id"], "label": item["label"], "position": item["position"],
                                        "target_kind": "entry", "target_entry_id": item["target_entry_id"]}
                                       for item in menu["items"]
                                   ]}, csrf)
                assert len(preview["items"]) == 3, preview
                for item in preview["items"]:
                    link = item["url"]
                    assert link.startswith("/content/")
                    assert context.request.get(base + link).status == 200, link
                assert context.request.get(base + "/content/post/g5-secret-draft").status == 404
                passed.append("menu_save_and_public_routes")

                # Unauthorized/Editor API probes leave the Admin's unsaved form untouched.
                page.get_by_role("textbox", name="Nhãn mục menu").first.fill("Unsaved after 401 and 403")
                fresh = p.request.new_context()
                assert fresh.get(base + f"/api/admin/menus/{menu_id}").status == 401
                api_json(context, "POST", base, "/api/admin/users",
                         {"username": "editor1", "password": "G5-Test-Editor-123456", "role": "editor"},
                         csrf, expected=201)
                editor = p.request.new_context()
                editor_login = editor.post(base + "/auth/login",
                                           headers={"Origin": base, "Content-Type": "application/json"},
                                           data=json.dumps({"username": "editor1", "password": "G5-Test-Editor-123456"}))
                assert editor_login.status == 200
                assert editor.get(base + f"/api/admin/menus/{menu_id}").status == 403
                assert page.get_by_role("textbox", name="Nhãn mục menu").first.input_value() == "Unsaved after 401 and 403"
                editor.dispose()
                fresh.dispose()

                # Version conflict arrives after another Admin update; keep the draft.
                changed = api_json(context, "PUT", base, f"/api/admin/menus/{menu_id}",
                                   {"title": "G5 Menu", "items": [
                                       {"id": item["id"], "label": item["label"], "position": item["position"],
                                        "target_kind": "entry", "target_entry_id": item["target_entry_id"]}
                                       for item in menu["items"]
                                   ]}, csrf, {"If-Match": f'"v{menu["menu"]["version"]}"'})
                assert changed["menu"]["version"] > menu["menu"]["version"]
                page.get_by_role("button", name="Lưu menu").click()
                expect(page.get_by_role("alert")).to_contain_text("Bản nháp chưa lưu vẫn được giữ lại", timeout=20000)
                assert page.get_by_role("textbox", name="Nhãn mục menu").first.input_value() == "Unsaved after 401 and 403"
                passed.append("auth_and_stale_412_preserve_draft")
                assert not errors, f"Browser page errors: {errors}"
                context.close()
                browser.close()

            stop_server(server)
            server = None
            restarted, restart_base = launch_server(binary, root)
            server = restarted
            with sync_playwright() as p:
                browser = p.chromium.launch(channel="chrome", headless=True)
                context = browser.new_context()
                csrf = admin_login(context, restart_base, "admin", "G5-Test-Admin-123456")
                recovered = api_json(context, "GET", restart_base, f"/api/admin/menus/{menu_id}")
                assert {item["id"] for item in recovered["items"]} == stable_item_ids
                assert {item["target_entry_id"] for item in recovered["items"]} == {post_id, page_id, cpt_id}
                page = context.new_page()
                open_ui_menu(page, restart_base)
                options = page.get_by_role("combobox", name="Chọn bài/trang")
                assert {int(options.nth(i).input_value()) for i in range(3)} == {post_id, page_id, cpt_id}
                for slug, key in (("g5-ui-post", "post"), ("g5-ui-page", "page"), ("g5-ui-cpt", "portfolio")):
                    assert context.request.get(restart_base + f"/content/{key}/{slug}").status == 200
                passed.append("restart_ids_public_content")
                context.close()
                browser.close()

            result = {
                "result": "PASS", "task": "M7-S15-T01", "source_sha": source_sha,
                "binary_sha256": binary_sha, "chrome": browser_version,
                "checks": passed, "ui_menu_items": sorted(stable_item_ids),
                "entry_ids": {"post": post_id, "page": page_id, "portfolio": cpt_id},
                "fixture_post_count": 52,
                "security": "401/403/412/404 verified; no live site touched",
            }
            (artifact_dir / "m7_beta_menu_picker_receipt.json").write_text(
                json.dumps(result, indent=2), encoding="utf-8"
            )
            print(json.dumps(result, ensure_ascii=False), flush=True)
            return 0
        finally:
            if server:
                stop_server(server)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=pathlib.Path)
    parser.add_argument("--artifact-dir", required=True, type=pathlib.Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    artifact_dir = args.artifact_dir.resolve()
    try:
        return run(binary, artifact_dir)
    except Exception as error:
        print(f"M7-S15-T01 FAIL: {type(error).__name__}: {error}", file=sys.stderr, flush=True)
        raise


if __name__ == "__main__":
    sys.exit(main())
