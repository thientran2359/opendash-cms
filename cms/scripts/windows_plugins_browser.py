"""M3-S28-T01 Plugins Admin Chrome E2E: permissions, recovery, security and 390px."""
import argparse
import json
from pathlib import Path
import secrets
from tempfile import TemporaryDirectory

from playwright.sync_api import expect, sync_playwright
from windows_plugin_smoke import request, running


def verify(binary: Path, browser_path: str | None) -> None:
    sample_zip = Path(__file__).resolve().parents[1] / "examples/plugins/sample-plugin/sample-plugin.zip"
    assert sample_zip.exists(), sample_zip
    with TemporaryDirectory(prefix="opendash-m3-plugins-") as folder:
        database = Path(folder) / "site.sqlite"
        password = secrets.token_hex(16)
        with running(binary, database) as base, sync_playwright() as pw:
            browser = pw.chromium.launch(executable_path=browser_path)
            context = browser.new_context(viewport={"width": 1280, "height": 860})
            page = context.new_page()
            errors = []
            page.on("pageerror", lambda error: errors.append(str(error)))
            assert page.request.get(base + "/api/admin/plugins").status == 401

            page.goto(base + "/admin/#/plugins")
            expect(page.locator("#auth-login")).to_be_visible()
            page.locator("#auth-username").fill("qaadmin")
            page.locator("#auth-password").fill(password)
            page.locator("#auth-setup").click()
            expect(page.locator("#status")).to_contain_text("Đã tạo")
            page.locator("#auth-login").click()
            expect(page.locator("#user-info")).to_contain_text("qaadmin")
            expect(page.locator("#plugin-management-section")).to_be_visible()
            expect(page.locator("#plugin-list-heading")).to_contain_text("(0)")

            # Invalid ZIP must not add a plugin or create a phantom successful state.
            page.locator("#plugin-zip").set_input_files({
                "name": "bad.zip", "mimeType": "application/zip", "buffer": b"not a zip",
            })
            expect(page.locator("#status")).to_contain_text("ZIP")
            expect(page.locator("#plugin-list-heading")).to_contain_text("(0)")

            # Initial install does not require confirmation when there is no existing ID.
            page.locator("#plugin-zip").set_input_files(str(sample_zip))
            plugin = page.locator('[data-plugin-id="sample-plugin"]')
            expect(plugin).to_be_visible()
            expect(plugin.get_by_text("Đang bật", exact=True)).to_be_visible()
            expect(plugin.get_by_text("SQLite: kv_read", exact=True)).to_be_visible()
            expect(plugin.get_by_text("HTTP:", exact=False).first).to_be_visible()
            expect(page.locator("#plugin-list-heading")).to_contain_text("(1)")

            page.locator("#btn-run-plugin").click()
            expect(page.locator("#plugin-output")).to_contain_text('"run_count": 1')

            plugin.get_by_role("button", name="Tắt plugin Sample Transformer Plugin").click()
            expect(plugin.get_by_text("Đã tắt", exact=True)).to_be_visible()
            expect(page.locator("#btn-run-plugin")).to_be_disabled()
            # Disabled invocation rejection is asserted in windows_plugin_smoke.py.

            # Enable requires explicit acknowledgement of manifest permissions.
            dialogs = []
            def approve(dialog):
                dialogs.append(dialog.message)
                dialog.accept()

            page.once("dialog", approve)
            plugin.get_by_role("button", name="Bật plugin Sample Transformer Plugin").click()
            expect(plugin.get_by_text("Đang bật", exact=True)).to_be_visible()
            assert any("kv_read" in message and "HTTP:" in message for message in dialogs)

            # A second ZIP upload warns about replace-and-enable before submitting.
            page.once("dialog", approve)
            page.locator("#plugin-zip").set_input_files(str(sample_zip))
            expect(page.locator("#status")).to_contain_text("Đã cài đặt plugin")
            assert any("THAY THẾ" in message and "BẬT LẠI" in message for message in dialogs)

            # Failed list refresh retains last known server state and offers retry.
            def fail_get(route):
                if route.request.method == "GET":
                    route.fulfill(status=503, body="Tạm thời không thể truy cập plugins")
                else:
                    route.continue_()

            page.route("**/api/admin/plugins", fail_get)
            page.get_by_role("button", name="Tải lại danh sách").click()
            expect(page.get_by_text("Không cập nhật được danh sách", exact=False)).to_be_visible()
            expect(plugin).to_be_visible()
            page.unroute("**/api/admin/plugins", fail_get)
            page.get_by_role("button", name="Thử tải lại").click()
            expect(page.get_by_text("Không cập nhật được danh sách", exact=False)).to_have_count(0)

            # An error reported by the server must not hide the disable/recovery action.
            def show_worker_error(route):
                if route.request.method == "GET":
                    response = route.fetch()
                    items = response.json()
                    for item in items:
                        if item["id"] == "sample-plugin":
                            item["last_error"] = "Worker timeout"
                    route.fulfill(status=200, content_type="application/json", body=json.dumps(items))
                else:
                    route.continue_()

            page.route("**/api/admin/plugins", show_worker_error)
            page.get_by_role("button", name="Tải lại danh sách").click()
            expect(plugin.get_by_text("Worker timeout", exact=False)).to_be_visible()
            plugin.get_by_role("button", name="Tắt plugin Sample Transformer Plugin").click()
            expect(plugin.get_by_text("Đã tắt", exact=True)).to_be_visible()
            page.unroute("**/api/admin/plugins", show_worker_error)

            # One Admin is not allowed to leak management capabilities to Editor UI/API.
            cookies = context.cookies(base)
            session_cookie = next(cookie for cookie in cookies if cookie["name"].startswith("opendash"))
            csrf = page.request.get(base + "/auth/me").json()["csrf_token"]
            status, payload, _ = request(
                base, "/api/admin/users",
                json.dumps({"username": "qaeditor", "password": password}).encode(),
                "application/json",
                headers={"Cookie": session_cookie["name"] + "=" + session_cookie["value"],
                         "X-CSRF-Token": csrf},
            )
            assert status == 201, (status, payload)
            editor = browser.new_context(viewport={"width": 390, "height": 844})
            editor_login = editor.request.post(base + "/auth/login",
                data={"username": "qaeditor", "password": password})
            assert editor_login.status == 200, editor_login.text()
            editor_page = editor.new_page()
            editor_page.goto(base + "/admin/#/plugins")
            expect(editor_page.locator('[data-view="plugins"]')).to_have_count(0)
            expect(editor_page.locator("#plugin-management-section")).to_have_count(0)
            assert editor_page.request.get(base + "/api/admin/plugins").status == 403
            editor_csrf = editor_login.json()["csrf_token"]
            assert editor_page.request.post(base + "/api/admin/plugins/sample-plugin/enable",
                headers={"X-CSRF-Token": editor_csrf}).status == 403
            editor.close()

            # 390px mobile must remain usable, keyboard reachable and without page overflow.
            page.set_viewport_size({"width": 390, "height": 844})
            expect(page.locator("#plugin-management-section")).to_be_visible()
            expect(plugin.get_by_role("button", name="Bật plugin Sample Transformer Plugin")).to_be_visible()
            assert page.evaluate("document.documentElement.scrollWidth <= document.documentElement.clientWidth + 1"), (
                "Plugins UI overflows a 390px viewport"
            )
            page.get_by_role("button", name="Tải lại danh sách").focus()
            assert page.evaluate("document.activeElement.textContent.includes('Tải lại danh sách')")
            assert not errors, errors
            context.close()
            browser.close()

        # Server restart: plugin state and manifest permissions must remain.
        with running(binary, database) as base:
            status, payload, cookie = request(
                base, "/auth/login",
                json.dumps({"username": "qaadmin", "password": password}).encode(),
                "application/json",
            )
            assert status == 200, (status, payload)
            csrf = json.loads(payload)["csrf_token"]
            headers = {"Cookie": cookie.split(";")[0], "X-CSRF-Token": csrf}
            status, payload, _ = request(base, "/api/admin/plugins", headers=headers)
            assert status == 200
            plugins = json.loads(payload)
            assert len(plugins) == 1 and plugins[0]["enabled"] is False, plugins
            assert "kv_write" in plugins[0]["permissions"]["sqlite"]
            status, payload, _ = request(
                base, "/api/admin/plugins/sample-plugin/run", b'{}', "application/json",
                headers=headers,
            )
            assert status == 400 and json.loads(payload)["code"] == "disabled", (status, payload)
    print("PASS: Plugins browser install/replace, manifest, retry, worker-error UX, RBAC, 390px")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/opendash.exe"))
    parser.add_argument("--browser", help="Optional path to installed Chrome/Chromium")
    args = parser.parse_args()
    verify(args.binary.resolve(), args.browser)
