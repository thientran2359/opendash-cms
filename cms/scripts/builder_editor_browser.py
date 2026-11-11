"""M4-S06-T01 Chrome acceptance for the homepage Tree / Canvas / Inspector.

Run after rebuilding the embedded Admin:
  python cms/scripts/builder_editor_browser.py --binary cms/target/debug/opendash.exe

Each run uses a disposable SQLite site; no real site files or production data.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sqlite3
import subprocess
from contextlib import closing, contextmanager
from pathlib import Path
from queue import Queue
from tempfile import TemporaryDirectory
from threading import Thread

from playwright.sync_api import expect, sync_playwright

URL = "/api/admin/builder/homepage"


@contextmanager
def running(binary: Path, database: Path):
    """Launch a disposable server and wait for its process to release SQLite.

    The shared smoke helper terminates without waiting, which can leave the
    temporary database file locked when Windows removes the fixture directory.
    """
    environment = dict(
        os.environ,
        OPENDASH_PORT="0",
        OPENDASH_SITE_ROOT=str(database.parent),
        OPENDASH_DB_PATH=str(database),
    )
    with subprocess.Popen(
        [str(binary)], env=environment, stdout=subprocess.PIPE,
        text=True, encoding="utf-8",
    ) as process:
        output = Queue()
        assert process.stdout is not None
        Thread(target=lambda: output.put(process.stdout.readline()), daemon=True).start()
        try:
            line = output.get(timeout=20).strip()
            assert line.startswith("OpenDash: http://127.0.0.1:") and line.endswith("/builder"), line
            yield line.removeprefix("OpenDash: ").removesuffix("/builder")
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=10)


def state(page, base):
    result = page.request.get(base + URL)
    assert result.status == 200, result.text()
    payload = result.json()
    assert result.headers.get("etag") == f'"v{payload["version"]}"'
    return payload


def element_at(document, index=0):
    return document["sections"][0]["children"][0]["children"][0]["children"][index]


def title_field(page):
    return page.get_by_label("Tiêu đề", exact=True)


def save(page):
    page.get_by_role("button", name="Lưu homepage").click()
    expect(page.locator('[data-testid="builder-workspace"]')).to_contain_text(
        "Đã lưu", timeout=10000,
    )


def new_tree(page):
    page.get_by_role("button", name="+ Section").click()
    page.get_by_role("button", name="Thêm row vào Section").click()
    page.get_by_role("button", name="Thêm column vào Row").click()
    page.get_by_role("button", name="Thêm element vào Column").click()
    assert page.locator('[data-node-id]').count() == 4


def verify_editor(binary: Path, browser_path: str | None):
    with TemporaryDirectory(prefix="opendash-m4-editor-browser-") as folder:
        database = Path(folder) / "site" / "database" / "opendash.sqlite"
        with running(binary, database) as base, sync_playwright() as pw:
            browser = pw.chromium.launch(executable_path=browser_path)
            context = browser.new_context(viewport={"width": 1440, "height": 900})
            page = context.new_page()
            errors: list[str] = []
            page.on("pageerror", lambda error: errors.append(str(error)))
            dialogs = []

            def accept(dialog):
                dialogs.append(dialog.message)
                dialog.accept()

            page.on("dialog", accept)

            # Reproduce races deterministically while the browser and server run.
            page.add_init_script("""
                (() => {
                  window.__builderTest = { previewDelay: false, saveDelay: false };
                  const original = window.fetch.bind(window);
                  window.fetch = async (input, options) => {
                    const url = typeof input === 'string' ? input : input.url;
                    const method = (options?.method || 'GET').toUpperCase();
                    const result = await original(input, options);
                    if (url.endsWith('/api/admin/builder/homepage/preview') &&
                        window.__builderTest.previewDelay) {
                      await new Promise(resolve => setTimeout(resolve, 700));
                    }
                    if (url.endsWith('/api/admin/builder/homepage') &&
                        method === 'PUT' && window.__builderTest.saveDelay) {
                      await new Promise(resolve => setTimeout(resolve, 700));
                    }
                    return result;
                  };
                })();
            """)
            assert context.request.get(base + URL).status == 401
            page.goto(base + "/admin/#/builder")
            page.locator("#auth-username").fill("editoradmin")
            page.locator("#auth-password").fill("editor-password-123")
            page.locator("#auth-setup").click()
            expect(page.locator("#status")).to_contain_text("Đã tạo")
            page.locator("#auth-login").click()
            workspace = page.locator('[data-testid="builder-workspace"]')
            expect(workspace).to_contain_text("Đã mở homepage")
            assert state(page, base)["source"] == "empty"
            assert state(page, base)["version"] == 1

            new_tree(page)
            element = page.locator('[role="treeitem"]').last
            original_id = element.get_attribute("data-node-id")
            assert original_id and re.fullmatch(r"element-[a-f0-9-]{36}", original_id)
            expect(page.locator(f'[data-canvas-id="{original_id}"]')).to_be_visible()
            expect(element).to_have_attribute("aria-selected", "true")
            title_field(page).fill('Homepage <script>alert("x")</script>')

            # Arrow navigation uses roving tabindex and updates canvas selection.
            first = page.locator('[role="treeitem"]').first
            first.focus()
            page.keyboard.press("ArrowDown")
            row_id = page.locator('[role="treeitem"]').nth(1).get_attribute("data-node-id")
            expect(page.locator('[role="treeitem"]').nth(1)).to_be_focused()
            expect(page.locator(f'[data-canvas-id="{row_id}"]')).to_have_attribute("aria-pressed", "true")
            page.locator(f'[data-canvas-id="{original_id}"]').click()
            expect(element).to_have_attribute("aria-selected", "true")
            expect(title_field(page)).to_have_value('Homepage <script>alert("x")</script>')
            saved = state(page, base)
            assert saved["document"]["sections"] == [] and saved["version"] == 1
            assert "Homepage <script>" not in page.request.get(base + "/").text()

            page.get_by_role("button", name="Xem trước").click()
            expect(page.get_by_role("button", name="Preview HTML")).to_be_enabled()
            expect(page.locator("#canvas")).to_be_visible()
            frame = page.frame_locator("#canvas")
            expect(frame.locator("body")).to_contain_text('Homepage <script>alert("x")</script>')
            sandbox = page.evaluate("""
                () => {
                  const iframe = document.querySelector('#canvas');
                  return { tokens: iframe.getAttribute('sandbox'), sameOrigin: iframe.contentDocument !== null };
                }
            """)
            assert sandbox == {"tokens": "", "sameOrigin": False}, sandbox
            assert state(page, base)["version"] == 1

            page.get_by_role("button", name="Canvas cấu trúc").click()
            save(page)
            first_saved = state(page, base)
            assert first_saved["version"] == 2
            assert element_at(first_saved["document"])["id"] == original_id
            assert element_at(first_saved["document"])["props"]["text"] == 'Homepage <script>alert("x")</script>'
            assert 'Homepage &lt;script&gt;' in page.request.get(base + "/").text()
            page.reload()
            expect(page.locator(f'[data-canvas-id="{original_id}"]')).to_be_visible()
            assert state(page, base)["version"] == 2

            # Change static props without dropping the already stored binding.
            auth = page.request.get(base + "/auth/me").json()
            with_binding = state(page, base)["document"]
            element_at(with_binding)["props"]["binding"] = {
                "type_key": "post", "entry_slug": "missing-post",
                "source": {"kind": "title"}, "target": "text",
            }
            response = page.request.put(
                base + URL, headers={
                    "X-CSRF-Token": auth["csrf_token"], "If-Match": '"v2"',
                }, data=with_binding,
            )
            assert response.status == 200, response.text()
            page.get_by_role("button", name="Tải bản server").click()
            page.locator(f'[data-canvas-id="{original_id}"]').click()
            expect(page.get_by_text("Field Binding hiện có")).to_be_visible()
            title_field(page).fill("Static title retained with binding")
            save(page)
            current = state(page, base)
            assert current["version"] == 4
            assert element_at(current["document"])["props"]["binding"] == {
                "type_key": "post", "entry_slug": "missing-post",
                "source": {"kind": "title"}, "target": "text",
            }
            assert element_at(current["document"])["props"]["text"] == "Static title retained with binding"
            print("PASS: Tree/Canvas keyboard/ID, HTML sandbox opaque, Preview read-only, Save and binding round-trip")

            # The server changes the ETag in another tab; UI must retain the stale draft.
            title_field(page).fill("Uncommitted local after remote edit")
            remote_document = state(page, base)["document"]
            element_at(remote_document)["props"]["text"] = "Remote concurrent title"
            concurrent = page.request.put(
                base + URL, headers={
                    "X-CSRF-Token": auth["csrf_token"], "If-Match": '"v4"',
                }, data=remote_document,
            )
            assert concurrent.status == 200, concurrent.text()
            page.get_by_role("button", name="Lưu homepage").click()
            expect(workspace.get_by_role("alert")).to_contain_text("412")
            expect(title_field(page)).to_have_value("Uncommitted local after remote edit")
            assert element_at(state(page, base)["document"])["props"]["text"] == "Remote concurrent title"
            page.get_by_role("button", name="Xem bản server (cần xác nhận bỏ nháp)").click()
            page.locator(f'[data-canvas-id="{original_id}"]').click()
            expect(title_field(page)).to_have_value("Remote concurrent title")
            assert dialogs and "Bỏ toàn bộ bản nháp" in dialogs[-1]
            print("PASS: 412 retains draft until explicit server reload confirmation")

            # When a PUT snapshot has been sent, new keystrokes remain dirty.
            page.evaluate("window.__builderTest.saveDelay = true")
            title_field(page).fill("Snapshot written first")
            page.get_by_role("button", name="Lưu homepage").click()
            title_field(page).fill("Edit after PUT snapshot")
            expect(workspace).to_contain_text("Các chỉnh sửa mới hơn vẫn chưa được lưu.")
            assert element_at(state(page, base)["document"])["props"]["text"] == "Snapshot written first"
            expect(title_field(page)).to_have_value("Edit after PUT snapshot")
            expect(page.locator(".od-unsaved-label")).to_be_visible()
            page.evaluate("window.__builderTest.saveDelay = false")
            save(page)

            # Stale preview must never supersede newer edits.
            page.evaluate("window.__builderTest.previewDelay = true")
            page.get_by_role("button", name="Xem trước").click()
            title_field(page).fill("Fresh edit beats stale preview")
            page.wait_for_timeout(950)
            expect(page.get_by_role("button", name="Preview HTML")).to_be_disabled()
            expect(page.locator("#canvas")).to_have_count(0)
            expect(title_field(page)).to_have_value("Fresh edit beats stale preview")
            page.evaluate("window.__builderTest.previewDelay = false")
            print("PASS: snapshot PUT leaves later edits dirty and stale Preview response discarded")

            # Session expires; same account may recover unsaved document.
            csrf = page.request.get(base + "/auth/me").json()["csrf_token"]
            logout = page.request.post(base + "/auth/logout", headers={"X-CSRF-Token": csrf})
            assert logout.status == 204, logout.text()
            page.get_by_role("button", name="Lưu homepage").click()
            expect(page.locator("#auth-login")).to_be_visible()
            page.locator("#auth-username").fill("editoradmin")
            page.locator("#auth-password").fill("editor-password-123")
            page.locator("#auth-login").click()
            expect(title_field(page)).to_have_value("Fresh edit beats stale preview")
            save(page)

            # Add a sibling, reorder through Tree, and reload to prove stable IDs.
            page.locator("#builder-element-type").select_option("core/text")
            page.get_by_role("treeitem", name=re.compile("Column")).click()
            page.get_by_role("button", name="Thêm element vào Column").click()
            second_id = page.locator('[role="treeitem"]').last.get_attribute("data-node-id")
            assert second_id != original_id
            expect(page.locator('[aria-label="Inspector"]')).to_contain_text("Văn bản")
            page.locator('[aria-label="Inspector"] textarea').fill("Second sibling")
            page.get_by_role("button", name="Di chuyển Văn bản lên").click()
            save(page)
            children = state(page, base)["document"]["sections"][0]["children"][0]["children"][0]["children"]
            assert [child["id"] for child in children] == [second_id, original_id]
            page.reload()
            expect(page.locator(f'[data-canvas-id="{second_id}"]')).to_be_visible()
            expect(page.locator(f'[data-canvas-id="{original_id}"]')).to_be_visible()
            page.locator(f'[data-canvas-id="{original_id}"]').click()
            print("PASS: sibling add/reorder keeps stable IDs, props and ordering after reload")

            # A deliberate logout clears the editor memory; another user cannot
            # see an expired account's unsaved changes.
            csrf = page.request.get(base + "/auth/me").json()["csrf_token"]
            created = page.request.post(
                base + "/api/admin/users", headers={"X-CSRF-Token": csrf},
                data={"username": "builderguest", "password": "editor-password-456", "role": "editor"},
            )
            assert created.status == 201, created.text()
            title_field(page).fill("PRIVATE unsaved from Admin")
            assert page.request.post(base + "/auth/logout", headers={"X-CSRF-Token": csrf}).status == 204
            page.get_by_role("button", name="Lưu homepage").click()
            expect(page.locator("#auth-login")).to_be_visible()
            page.locator("#auth-username").fill("builderguest")
            page.locator("#auth-password").fill("editor-password-456")
            page.locator("#auth-login").click()
            expect(page.locator('[data-testid="builder-workspace"]')).to_contain_text("Đã mở homepage")
            assert "PRIVATE unsaved from Admin" not in workspace.inner_text()
            expect(page.locator(f'[data-canvas-id="{original_id}"]')).to_be_visible()
            saved_ids = [entry["id"] for entry in
                         state(page, base)["document"]["sections"][0]["children"][0]["children"][0]["children"]]
            assert saved_ids == [second_id, original_id]
            assert not errors, errors
            print("PASS: 401 same-account draft recovery, different-user purge, no JS page errors")
            browser.close()


def verify_unresolved(binary: Path, browser_path: str | None):
    """Historical provider loss must preserve a locked node's index until confirmed deletion."""
    from builder_homepage_smoke import get, save as backend_save, tree, heading, checked
    from themes_smoke import setup

    with TemporaryDirectory(prefix="opendash-m4-editor-unknown-") as folder:
        database = Path(folder) / "site" / "database" / "opendash.sqlite"
        with running(binary, database) as base:
            auth = setup(base)
            document = tree(heading("Safe sibling"))
            checked(backend_save(base, auth, document, 1), 200, "Seed known document")
            assert get(base, auth)["version"] == 2

        historic = tree(heading("Safe sibling"))
        historic["sections"][0]["children"][0]["children"][0]["children"].append(
            {"id": "missing-provider", "type": "other/no-longer-installed",
             "props": {"historical": "do not strip"}},
        )
        with closing(sqlite3.connect(database)) as db:
            db.execute("UPDATE builder_homepage SET document_json=?, version=3 WHERE id=1",
                       (json.dumps(historic),))
            db.commit()

        with running(binary, database) as base, sync_playwright() as pw:
            browser = pw.chromium.launch(executable_path=browser_path)
            context = browser.new_context(viewport={"width": 1440, "height": 900})
            page = context.new_page()
            dialogs = []
            page.on("dialog", lambda dialog: (dialogs.append(dialog.message), dialog.accept()))
            page.goto(base + "/admin/#/builder")
            page.locator("#auth-username").fill("themeadmin")
            page.locator("#auth-password").fill("qa-password-123")
            page.locator("#auth-login").click()
            expect(page.locator('[data-canvas-id="missing-provider"]')).to_contain_text("Không thể hiển thị")
            page.locator('[data-canvas-id="missing-provider"]').click()
            expect(page.get_by_text("Node không resolve được")).to_be_visible()
            page.get_by_role("button", name="Di chuyển Block chưa nhận diện lên").click()
            expect(page.locator('[data-testid="builder-workspace"]')).to_contain_text("giữ nguyên vị trí")
            assert element_at(state(page, base)["document"], 1)["id"] == "missing-provider"
            page.get_by_role("button", name="Xóa Block chưa nhận diện").click()
            assert dialogs and "block chưa resolve" in dialogs[-1]
            save(page)
            assert len(state(page, base)["document"]["sections"][0]["children"][0]["children"][0]["children"]) == 1
            browser.close()
            print("PASS: unknown historical node shown, anchor protected and deleted only after confirmation")


def verify_legacy_preview(binary: Path, browser_path: str | None):
    """Real theme HTML must stay in opaque Preview; new media remains private."""
    from builder_homepage_smoke import checked, get
    from media_smoke import upload_image, raw_request
    from themes_smoke import setup, install, json_request, theme_zip

    with TemporaryDirectory(prefix="opendash-m4-editor-legacy-") as folder:
        database = Path(folder) / "site" / "database" / "opendash.sqlite"
        with running(binary, database) as base:
            auth = setup(base)
            theme = theme_zip(
                "qa-a/banner",
                """<article data-theme="A">{{ values.title }}</article>
                <script>window.top.location.assign('/hijack')</script>""",
                fields={"title": "Title"},
            )
            checked(install(base, auth, theme), 201, "Install theme")
            old = {"type": "qa-a/banner", "values": {"title": "Legacy preview original"}}
            checked(json_request(base, "POST", "/save", old, auth), 200, "Legacy page")
            initial = get(base, auth)
            assert initial["source"] == "legacy" and initial["version"] == 2
            asset = upload_image(base, auth, seed=84)
            media_id = asset["id"]
            assert raw_request(base, "GET", f"/media/{media_id}/file")[0] == 404

            with sync_playwright() as pw:
                browser = pw.chromium.launch(executable_path=browser_path)
                context = browser.new_context(viewport={"width": 1440, "height": 900})
                page = context.new_page()
                page_errors: list[str] = []
                page.on("pageerror", lambda error: page_errors.append(str(error)))
                page.goto(base + "/admin/#/builder")
                page.locator("#auth-username").fill("themeadmin")
                page.locator("#auth-password").fill("qa-password-123")
                page.locator("#auth-login").click()
                expect(page.locator('[data-testid="builder-workspace"]')).to_contain_text("Nguồn: legacy")
                page.get_by_role("button", name="Xem trước").click()
                expect(page.frame_locator("#canvas").locator('article[data-theme="A"]')).to_contain_text("Legacy preview original")
                assert "/admin/#/builder" in page.url and "/hijack" not in page.url
                assert page.evaluate("() => document.querySelector('#canvas').contentDocument === null")
                assert get(base, auth)["version"] == 2

                page.get_by_role("button", name="Canvas cấu trúc").click()
                page.locator("#builder-element-type").select_option("core/image")
                page.get_by_role("treeitem", name=re.compile("Column")).click()
                page.get_by_role("button", name="Thêm element vào Column").click()
                page.get_by_role("button", name="Chọn ảnh trong thư viện").click()
                picker = page.get_by_role("dialog", name="Chọn ảnh từ Media Library")
                expect(picker).to_be_visible()
                picker.locator(".media-tile").first.click()
                picker.get_by_role("button", name="Dùng ảnh đã chọn").click()
                expect(page.get_by_label("Media ID")).to_have_value(media_id)
                page.get_by_label("Mô tả ảnh (alt)").fill("Preview private")
                page.get_by_role("button", name="Xem trước").click()
                img = page.frame_locator("#canvas").locator("img")
                expect(img).to_have_attribute("src", re.compile(r"^data:image/png;base64,"))
                expect(img).to_have_js_property("naturalWidth", 2, timeout=5000)
                assert get(base, auth)["version"] == 2
                assert raw_request(base, "GET", f"/media/{media_id}/file")[0] == 404
                assert not page_errors, page_errors
                browser.close()

        print("PASS: legacy theme HTML in opaque sandbox, top navigation blocked, media remains private during Preview")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("cms/target/debug/opendash.exe"))
    parser.add_argument("--browser", help="Optional Chrome/Chromium executable path")
    args = parser.parse_args()
    verify_editor(args.binary.resolve(), args.browser)
    verify_unresolved(args.binary.resolve(), args.browser)
    verify_legacy_preview(args.binary.resolve(), args.browser)
