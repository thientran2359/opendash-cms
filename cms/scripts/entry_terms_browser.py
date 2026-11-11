"""M4-S04-T01 real Chromium Admin taxonomy picker/filter browser acceptance.

Runs against a disposable site, never an existing CMS database. Requires Python
Playwright and Chromium/Chrome:
  python3 cms/scripts/entry_terms_browser.py --binary cms/target/debug/opendash \
      --chromium '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'
"""

from __future__ import annotations

import argparse
from pathlib import Path
from tempfile import TemporaryDirectory

from playwright.sync_api import expect, sync_playwright

from content_types_smoke import running


def test(binary: Path, chrome: str | None):
    with TemporaryDirectory(prefix="opendash-m4-s04-browser-") as directory:
        db = Path(directory) / "site" / "database" / "opendash.sqlite"
        with running(binary, db) as base, sync_playwright() as pw:
            browser = pw.chromium.launch(executable_path=chrome)
            context = browser.new_context(viewport={"width": 390, "height": 844})
            page = context.new_page()
            page_errors = []
            page.on("pageerror", lambda err: page_errors.append(str(err)))
            page.on("dialog", lambda dialog: dialog.accept())
            page.goto(base + "/admin/#/posts")
            page.locator("#auth-username").fill("termsadmin")
            page.locator("#auth-password").fill("browser-password-123")
            page.locator("#auth-setup").click()
            expect(page.locator("#status")).to_contain_text("Đã tạo")
            page.locator("#auth-login").click()
            expect(page.get_by_role("heading", name="Tất cả bài viết")).to_be_visible()

            auth = page.request.get(base + "/auth/me").json()
            assert auth["user"]["role"] == "admin"

            def post(path, payload, status=201):
                response = page.request.post(
                    base + path, data=payload,
                    headers={"X-CSRF-Token": auth["csrf_token"]},
                )
                assert response.status == status, (path, response.status, response.text())
                return response.json() if response.text() else None

            root = post("/api/admin/taxonomy/terms", {
                "kind": "category", "name": "Tin tức", "slug": "tin-tuc",
            })
            child = post("/api/admin/taxonomy/terms", {
                "kind": "category", "name": "Chuyên mục", "slug": "chuyen-muc",
                "parent_id": root["id"],
            })
            tag = post("/api/admin/taxonomy/terms", {
                "kind": "tag", "name": "Đặc biệt", "slug": "dac-biet",
            })
            post("/api/admin/content-types", {
                "key": "portfolio", "label": "Dự án",
                "fields": [{"key": "summary", "label": "Tóm tắt", "type": "short_text"}],
            })
            def no_overflow():
                dimensions = page.evaluate(
                    "() => ({viewport:innerWidth,doc:document.documentElement.scrollWidth})"
                )
                assert dimensions["viewport"] == 390
                assert dimensions["doc"] <= 390, dimensions

            page.get_by_role("button", name="+ Thêm bài viết").click()
            expect(page.get_by_role("heading", name="Danh mục & Thẻ")).to_be_visible()
            expect(page.get_by_role("region", name="Gắn danh mục và thẻ")).to_be_visible()
            page.get_by_role("textbox", name="Tiêu đề").fill("Bài viết thử nghiệm")
            page.get_by_role("textbox", name="Đường dẫn (slug)", exact=False).fill("bai-viet-thu-nghiem")
            category_checkbox = page.get_by_role("checkbox", name="Chuyên mục (chuyen-muc)")
            tag_checkbox = page.get_by_role("checkbox", name="Đặc biệt (dac-biet)")
            expect(category_checkbox).to_be_visible()
            category_checkbox.focus()
            page.keyboard.press("Space")
            expect(category_checkbox).to_be_checked()
            tag_checkbox.check()
            page.get_by_role("button", name="Lưu bản nháp").click()
            expect(page.get_by_role("heading", name="Chỉnh sửa bài viết")).to_be_visible()
            entries = page.request.get(base + "/api/admin/content-types/post/entries").json()
            assert len(entries) == 1, entries
            saved = entries[0]
            assert set(saved["term_ids"]) == {child["id"], tag["id"]}

            # 412 from a concurrent writer must leave the checked term draft.
            page.get_by_role("textbox", name="Tiêu đề").fill("Bản nhập chưa lưu")
            replacement = {
                "title": "Đã được sửa ở tab khác",
                "slug": saved["slug"], "values": saved["values"],
                "term_ids": [child["id"], tag["id"]],
            }
            concurrent = page.request.put(
                base + f"/api/admin/content-types/post/entries/{saved['id']}",
                data=replacement,
                headers={
                    "X-CSRF-Token": auth["csrf_token"],
                    "If-Match": f'"v{saved["version"]}"',
                },
            )
            assert concurrent.status == 200, concurrent.text()
            page.get_by_role("button", name="Lưu thay đổi").click()
            expect(page.get_by_role("alert")).to_contain_text("412")
            expect(page.get_by_role("textbox", name="Tiêu đề")).to_have_value("Bản nhập chưa lưu")
            expect(category_checkbox).to_be_checked()
            expect(tag_checkbox).to_be_checked()
            page.get_by_role("button", name="← Danh sách").click()
            expect(page.get_by_role("heading", name="Tất cả bài viết")).to_be_visible()
            category = page.get_by_role("combobox", name="Danh mục")
            selected_tag = page.get_by_role("combobox", name="Thẻ")
            category.select_option(str(root["id"]))
            expect(page.get_by_text("Chưa có bài viết nào")).to_be_visible()
            page.get_by_role("checkbox", name="Bao gồm danh mục con").check()
            selected_tag.select_option(str(tag["id"]))
            expect(page.get_by_text("Đã được sửa ở tab khác")).to_be_visible()
            no_overflow()

            # Page uses the same picker; a new page is linked to both terms.
            page.goto(base + "/admin/#/pages")
            page.get_by_role("button", name="+ Thêm trang").click()
            page.get_by_role("textbox", name="Tiêu đề").fill("Trang kiểm thử")
            page.get_by_role("textbox", name="Đường dẫn (slug)", exact=False).fill("trang-kiem-thu")
            page.get_by_role("checkbox", name="Tin tức (tin-tuc)").check()
            page.get_by_role("checkbox", name="Đặc biệt (dac-biet)").check()
            page.get_by_role("button", name="Lưu bản nháp").click()
            pages = page.request.get(base + "/api/admin/content-types/page/entries").json()
            assert len(pages) == 1 and set(pages[0]["term_ids"]) == {root["id"], tag["id"]}
            no_overflow()

            # CPT editor also accepts term IDs through the shared picker.
            page.goto(base + "/admin/#/content")
            page.get_by_role("complementary").get_by_role("button", name="Tải lại").click()
            page.get_by_role("button", name="Dự án portfolio", exact=False).click()
            expect(page.get_by_role("heading", name="Entry: Dự án")).to_be_visible()
            page.get_by_role("button", name="+ Tạo entry").click()
            page.locator("#content-entry-title").fill("Dự án Browser")
            page.locator("#content-entry-slug").fill("du-an-browser")
            page.get_by_role("region", name="Gắn danh mục và thẻ").get_by_role(
                "checkbox", name="Đặc biệt (dac-biet)").check()
            page.get_by_role("button", name="Lưu entry").click()
            cpt_entries = page.request.get(base + "/api/admin/content-types/portfolio/entries").json()
            assert len(cpt_entries) == 1 and cpt_entries[0]["term_ids"] == [tag["id"]]
            no_overflow()
            assert not page_errors, page_errors
            context.close()
            browser.close()
            print("PASS entry_terms browser: Post/Page/CPT picker, 412 draft, filter descendants+tag, keyboard, 390px")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("cms/target/debug/opendash"))
    parser.add_argument("--chromium", default=None)
    args = parser.parse_args()
    test(args.binary.resolve(), args.chromium)
