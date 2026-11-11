"""Real Chromium Admin taxonomy acceptance on a disposable SQLite site.

From repository root (requires Python Playwright and Chromium):
  python3 cms/scripts/taxonomy_terms_browser.py --binary cms/target/debug/opendash

Never operates against the user's configured site.
"""

from __future__ import annotations

import argparse
from pathlib import Path
from tempfile import TemporaryDirectory

from playwright.sync_api import expect, sync_playwright

from content_types_smoke import running


def verify(binary: Path, chromium: str | None):
    with TemporaryDirectory(prefix="opendash-m4-s03-browser-") as root:
        db = Path(root) / "site" / "database" / "opendash.sqlite"
        with running(binary, db) as base, sync_playwright() as playwright:
            browser = playwright.chromium.launch(executable_path=chromium)
            context = browser.new_context(viewport={"width": 390, "height": 844})
            page = context.new_page()
            page_errors = []
            page.on("pageerror", lambda error: page_errors.append(str(error)))
            page.on("dialog", lambda dialog: dialog.accept())
            page.goto(base + "/admin/#/taxonomy")

            page.locator("#auth-username").fill("taxonomyadmin")
            page.locator("#auth-password").fill("taxonomy-password-123")
            page.locator("#auth-setup").click()
            expect(page.locator("#status")).to_contain_text("Đã tạo")
            page.locator("#auth-login").click()
            expect(page.get_by_role("heading", name="Danh mục và thẻ")).to_be_visible()

            def no_overflow():
                sizes = page.evaluate("() => ({width: innerWidth, scroll: document.documentElement.scrollWidth})")
                assert sizes["width"] == 390 and sizes["scroll"] <= 390, sizes

            no_overflow()
            page.get_by_label("Tên", exact=True).fill("Tin tức Browser")
            expect(page.locator("#taxonomy-slug")).to_have_value("tin-tuc-browser")
            page.get_by_role("button", name="Tạo mới").click()
            expect(page.get_by_role("heading", name="Tin tức Browser")).to_be_visible()
            terms = page.request.get(base + "/api/admin/taxonomy/terms?kind=category").json()["items"]
            root = next(term for term in terms if term["slug"] == "tin-tuc-browser")

            page.get_by_role("button", name=f"Sửa Tin tức Browser, ID {root['id']}").click()
            page.get_by_label("Tên", exact=True).fill("Tin tức đã sửa")
            page.get_by_role("button", name="Lưu thay đổi").click()
            expect(page.get_by_role("heading", name="Tin tức đã sửa")).to_be_visible()
            after = page.request.get(base + "/api/admin/taxonomy/terms?kind=category").json()["items"][0]
            assert after["id"] == root["id"] and after["version"] == 2

            page.get_by_role("button", name="Thêm mới").click()
            page.get_by_label("Tên", exact=True).fill("Chuyên mục con")
            page.locator("#taxonomy-parent").select_option(str(root["id"]))
            page.get_by_role("button", name="Tạo mới").click()
            expect(page.get_by_role("heading", name="Chuyên mục con")).to_be_visible()
            page.get_by_role("button", name=f"Xóa Tin tức đã sửa, ID {root['id']}").click()
            expect(page.get_by_role("alert")).to_contain_text("mục con")

            # A second browser tab writes the same term; the UI must retain draft
            # after 412 and allow explicit conflict resolution.
            page.get_by_role("button", name=f"Sửa Tin tức đã sửa, ID {root['id']}").click()
            page.get_by_label("Tên", exact=True).fill("Bản nhập trước xung đột")
            auth = page.request.get(base + "/auth/me").json()
            changed = page.request.patch(
                base + f"/api/admin/taxonomy/terms/{root['id']}",
                headers={"X-CSRF-Token": auth["csrf_token"], "If-Match": '"v2"'},
                data={"name": "Thay đổi từ tab khác"},
            )
            assert changed.status == 200, changed.text()
            page.get_by_role("button", name="Lưu thay đổi").click()
            expect(page.get_by_role("alert")).to_contain_text("412")
            expect(page.get_by_label("Tên", exact=True)).to_have_value("Bản nhập trước xung đột")
            page.get_by_role("button", name="Tải bản mới để so sánh").click()
            page.get_by_role("button", name="Tiếp tục với v3 (giữ bản nhập)").click()
            page.get_by_role("button", name="Lưu thay đổi").click()
            expect(page.get_by_role("heading", name="Bản nhập trước xung đột")).to_be_visible()

            # Native button keyboard activation and a flat tag with same slug
            # as the category are handled separately by kind.
            tag_switch = page.get_by_role("button", name="Thẻ (Tag)")
            tag_switch.focus()
            page.keyboard.press("Enter")
            expect(tag_switch).to_have_attribute("aria-pressed", "true")
            page.get_by_label("Tên", exact=True).fill("Tin tức Browser")
            page.locator("#taxonomy-slug").fill("tin-tuc-browser")
            page.get_by_role("button", name="Tạo mới").click()
            expect(page.get_by_role("heading", name="Tin tức Browser")).to_be_visible()
            assert page.locator("#taxonomy-parent").count() == 0
            no_overflow()

            # Editor can list tags/categories, but mutation controls disappear.
            created = page.request.post(
                base + "/api/admin/users",
                headers={"X-CSRF-Token": auth["csrf_token"]},
                data={
                    "username": "taxonomyeditor", "password": "editor-password-123",
                    "role": "editor",
                },
            )
            assert created.status == 201, created.text()
            page.get_by_role("button", name="Đăng xuất").click()
            page.locator("#auth-username").fill("taxonomyeditor")
            page.locator("#auth-password").fill("editor-password-123")
            page.locator("#auth-login").click()
            page.goto(base + "/admin/#/taxonomy")
            expect(page.get_by_role("heading", name="Danh mục và thẻ")).to_be_visible()
            expect(page.get_by_text("Biên tập viên có thể xem danh sách")).to_be_visible()
            assert page.get_by_role("button", name="Thêm mới").count() == 0
            assert page.get_by_role("button", name="Tạo mới").count() == 0
            no_overflow()
            assert not page_errors, page_errors
            browser.close()
            print("PASS: 390px Chromium category/tag CRUD, child delete 409, CAS draft, keyboard, Editor read-only")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("cms/target/debug/opendash"))
    parser.add_argument("--chromium", help="Optional absolute Chromium executable path")
    args = parser.parse_args()
    verify(args.binary.resolve(), args.chromium)
