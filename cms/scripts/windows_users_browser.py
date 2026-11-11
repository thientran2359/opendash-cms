"""Exercise Users UI in embedded Windows OpenDash Admin on desktop and 390px.

Uses a fresh temporary site/SQLite and real Chromium. No persistent fixture is
written into the repository. Requires Python playwright and installed Chromium.
"""
import argparse
from pathlib import Path
from tempfile import TemporaryDirectory

from playwright.sync_api import expect, sync_playwright
from windows_smoke import running


def verify(binary: Path, browser_path: str | None):
    with TemporaryDirectory(prefix="opendash-users-browser-") as folder:
        db = Path(folder) / "cms.sqlite"
        with running(binary, db) as base, sync_playwright() as pw:
            browser = pw.chromium.launch(executable_path=browser_path)
            context = browser.new_context(viewport={"width": 1280, "height": 900})
            page = context.new_page()
            page_errors = []
            page.on("pageerror", lambda error: page_errors.append(str(error)))
            page.on("dialog", lambda dialog: dialog.accept())
            page.goto(base + "/admin/#/users")

            page.locator("#auth-username").fill("browseradmin")
            page.locator("#auth-password").fill("browser-password-1")
            page.locator("#auth-setup").click()
            expect(page.locator("#status")).to_contain_text("Đã tạo")
            page.locator("#auth-login").click()
            expect(page.get_by_role("heading", name="Danh sách tài khoản")).to_be_visible()
            expect(page.get_by_text("1 tài khoản")).to_be_visible()

            # New account form creates both roles with an explicit role choice.
            page.get_by_label("Tên đăng nhập", exact=True).fill("browsereditor")
            page.get_by_label("Mật khẩu", exact=True).fill("editor-password-1")
            page.locator("form").last.locator("select").select_option("editor")
            page.get_by_role("button", name="Tạo tài khoản").click()
            expect(page.get_by_role("row").filter(has_text="browsereditor")).to_be_visible()

            page.get_by_label("Tên đăng nhập", exact=True).fill("browseradmin2")
            page.get_by_label("Mật khẩu", exact=True).fill("admin-password-2")
            page.locator("form").last.locator("select").select_option("admin")
            page.get_by_role("button", name="Tạo tài khoản").click()
            expect(page.get_by_role("row").filter(has_text="browseradmin2")).to_be_visible()
            second_admin = page.get_by_role("row").filter(has_text="browseradmin2")
            second_admin.get_by_role("button", name="Sửa").click()
            expect(page.get_by_label("Tên đăng nhập", exact=True).first).to_be_focused()
            page.locator("form").filter(
                has=page.get_by_role("heading", name="Sửa browseradmin2 (v1)")
            ).locator("select").select_option("editor")
            page.get_by_role("button", name="Lưu thay đổi").click()
            expect(page.get_by_role("row").filter(has_text="browseradmin2")).to_contain_text("Biên tập viên")

            # Stale update must retain the draft and retry with freshly loaded CAS.
            row = page.get_by_role("row").filter(has_text="browsereditor")
            row.get_by_role("button", name="Sửa").click()
            page.get_by_label("Tên đăng nhập", exact=True).first.fill("browser-draft")
            session = page.request.get(base + "/auth/me").json()
            users = page.request.get(base + "/api/admin/users").json()["items"]
            editor = next(user for user in users if user["username"] == "browsereditor")
            changed = page.request.patch(
                base + "/api/admin/users/" + str(editor["id"]),
                headers={"X-CSRF-Token": session["csrf_token"], "If-Match": '"v1"'},
                data={"username": "browser-updated"},
            )
            assert changed.status == 200, changed.text()
            page.get_by_role("button", name="Lưu thay đổi").click()
            expect(page.get_by_role("alert")).to_contain_text("Tài khoản đã được cập nhật")
            expect(page.get_by_label("Tên đăng nhập", exact=True).first).to_have_value("browser-draft")
            page.get_by_role("button", name="Tải lại").click()
            expect(page.get_by_role("heading", name="Sửa browser-updated (v2)")).to_be_visible()
            expect(page.get_by_label("Tên đăng nhập", exact=True).first).to_have_value("browser-draft")
            page.get_by_role("button", name="Lưu thay đổi").click()
            expect(page.get_by_role("row").filter(has_text="browser-draft")).to_be_visible()

            # A 401 must preserve this draft when the same Admin signs back in.
            page.get_by_role("row").filter(has_text="browser-draft").get_by_role("button", name="Sửa").click()
            page.get_by_label("Tên đăng nhập", exact=True).first.fill("draft-after-401")
            logout = page.request.post(
                base + "/auth/logout", headers={"X-CSRF-Token": session["csrf_token"]},
            )
            assert logout.status == 204, logout.text()
            page.get_by_role("button", name="Lưu thay đổi").click()
            expect(page.locator("#auth-login")).to_be_visible()
            page.locator("#auth-username").fill("browseradmin")
            page.locator("#auth-password").fill("browser-password-1")
            page.locator("#auth-login").click()
            expect(page.get_by_role("heading", name="Danh sách tài khoản")).to_be_visible()
            expect(page.get_by_label("Tên đăng nhập", exact=True).first).to_have_value("draft-after-401")
            page.get_by_role("button", name="Hủy").click()

            # Confirmation and password reset revoke the target's existing session.
            row = page.get_by_role("row").filter(has_text="browser-draft")
            row.get_by_role("button", name="Đổi mật khẩu").click()
            page.get_by_label("Mật khẩu mới").fill("editor-password-2")
            page.get_by_role("button", name="Đặt lại mật khẩu").click()
            expect(page.get_by_text("Đã thay mật khẩu", exact=False)).to_be_visible()

            page.set_viewport_size({"width": 390, "height": 844})
            page.reload()
            expect(page.get_by_role("heading", name="Danh sách tài khoản")).to_be_visible()
            expect(page.get_by_role("article", name="Tài khoản browseradmin2")).to_be_visible()
            horizontal = page.evaluate("""() => ({
                viewport: window.innerWidth,
                document: document.documentElement.scrollWidth,
                body: document.body.scrollWidth,
            })""")
            assert horizontal["document"] <= horizontal["viewport"], horizontal
            page.get_by_role("button", name="Làm mới").focus()
            page.keyboard.press("Enter")
            expect(page.get_by_text("3 tài khoản")).to_be_visible()
            # Delete from the mobile layout through the confirmation dialog.
            page.get_by_role("article", name="Tài khoản browseradmin2").get_by_role("button", name="Xóa").click()
            expect(page.get_by_text("2 tài khoản")).to_be_visible()

            # A non-admin must not see Users even with a direct hash URL.
            editor_context = browser.new_context(viewport={"width": 390, "height": 844})
            editor_page = editor_context.new_page()
            editor_users_requests = []
            editor_page.on(
                "request",
                lambda request: editor_users_requests.append(request.url)
                if "/api/admin/users" in request.url else None,
            )
            editor_page.goto(base + "/admin/#/users")
            editor_page.locator("#auth-username").fill("browser-draft")
            editor_page.locator("#auth-password").fill("editor-password-2")
            editor_page.locator("#auth-login").click()
            expect(editor_page.locator("#user-info")).to_contain_text("editor")
            expect(editor_page.get_by_role("heading", name="Bảng điều khiển")).to_be_visible()
            assert editor_page.locator('nav[aria-label="Các mục quản trị"]').get_by_text("Người dùng").count() == 0
            assert not editor_users_requests, f"Editor must not prefetch Admin Users API: {editor_users_requests}"

            # Changing the current Admin's username revokes own session and shows login.
            own_row = page.get_by_role("article", name="Tài khoản browseradmin", exact=True)
            own_row.get_by_role("button", name="Sửa").click()
            page.get_by_label("Tên đăng nhập", exact=True).first.fill("browseradmin-renamed")
            page.get_by_role("button", name="Lưu thay đổi").click()
            expect(page.locator("#auth-login")).to_be_visible()
            expect(page.locator("#status")).to_contain_text("đăng nhập lại")
            page.locator("#auth-username").fill("browseradmin-renamed")
            page.locator("#auth-password").fill("browser-password-1")
            page.locator("#auth-login").click()
            expect(page.get_by_role("heading", name="Danh sách tài khoản")).to_be_visible()
            assert not page_errors, page_errors

            editor_context.close()
            context.close()
            browser.close()
    print("PASS: Users browser desktop/390px, roles, stale draft, reset, keyboard, session revocation")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/opendash.exe"))
    parser.add_argument("--browser", help="Optional installed Chromium executable path")
    args = parser.parse_args()
    verify(args.binary.resolve(), args.browser)
