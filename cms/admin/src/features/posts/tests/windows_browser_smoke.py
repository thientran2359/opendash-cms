"""Real Chrome/embedded Rust Admin acceptance for Posts/Pages (M3-S22-T01).

Run from repository root after the release build:
  python cms/admin/src/features/posts/tests/windows_browser_smoke.py
Optional: --binary PATH --browser PATH --screenshots DIRECTORY
Uses a fresh temporary site and SQLite database; never modifies real site data.
"""

import argparse
import json
import re
import secrets
import sqlite3
import sys
from contextlib import closing
from pathlib import Path
from tempfile import TemporaryDirectory

from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[6]
sys.path.insert(0, str(ROOT / "cms" / "scripts"))
from windows_smoke import running, sample_theme  # noqa: E402
from media_smoke import make_png  # noqa: E402


def verify(binary: Path, browser_path: str, screenshots: Path) -> None:
    screenshots.mkdir(parents=True, exist_ok=True)
    with TemporaryDirectory(prefix="opendash-post-page-ui-") as folder:
        db = Path(folder) / "website.sqlite"
        with running(binary, db) as base, sync_playwright() as pw:
            browser = pw.chromium.launch(executable_path=browser_path, headless=True)
            context = browser.new_context(viewport={"width": 1440, "height": 900})
            page = context.new_page()
            page.set_default_timeout(10000)
            page_errors: list[str] = []
            page.on("pageerror", lambda error: page_errors.append(str(error)))
            page.on("dialog", lambda dialog: dialog.accept())
            page.goto(base + "/admin/#/posts")
            expect(page.locator("#auth-login")).to_be_visible()
            password = secrets.token_hex(12)
            page.locator("#auth-username").fill("forgeadmin")
            page.locator("#auth-password").fill(password)
            page.locator("#auth-setup").click()
            expect(page.locator("#status")).to_contain_text("Đã tạo")
            page.locator("#auth-login").click()
            expect(page.locator("#user-info")).to_contain_text("forgeadmin")

            # Existing Builder/Content workspace remains functional and mounted.
            page.locator('button[data-view="builder"]').click()
            builder = page.locator("#builder-main")
            expect(builder).to_be_visible()
            builder.locator("#theme").set_input_files({
                "name": "sample-theme.zip", "mimeType": "application/zip", "buffer": sample_theme(),
            })
            expect(builder.locator("#field-title")).to_be_visible()
            builder.locator("#field-title").fill("Builder đang chỉnh chưa lưu")
            page.locator('button[data-view="content"]').click()
            expect(page.locator("#content-main")).to_be_visible()
            page.locator('button[data-view="posts"]').click()
            page.locator('button[data-view="builder"]').click()
            expect(builder.locator("#field-title")).to_have_value("Builder đang chỉnh chưa lưu")
            page.locator('button[data-view="posts"]').click()

            post = page.locator('section[aria-label="Bài viết"]')
            expect(post.get_by_text("Tất cả bài viết")).to_be_visible()
            post.get_by_role("button", name=re.compile("Thêm bài viết")).click()
            expect(post.get_by_text("Thêm bài viết", exact=True)).to_be_visible()
            fields = post.locator(".od-posts-fields")
            fields.locator('input[type="text"]').nth(0).fill("Bài viết thử nghiệm")
            expect(fields.locator('input[type="text"]').nth(1)).to_have_value("bai-viet-thu-nghiem")
            fields.locator("textarea").fill('Bản nháp <script>window.__xss=1</script>')

            # Existing MediaPicker is reused unchanged and creates an authenticated asset.
            post.get_by_role("button", name="Chọn từ thư viện").click()
            picker = page.locator(".media-modal")
            expect(picker).to_be_visible()
            picker.locator('input[type="file"]').set_input_files({
                "name": "feature.png", "mimeType": "image/png", "buffer": make_png(62)
            })
            expect(picker.get_by_text("Đã upload 1 ảnh.")).to_be_visible()
            picker.get_by_role("button", name="Dùng ảnh đã chọn").click()
            expect(post.locator(".od-posts-featured")).to_be_visible()
            assert post.locator(".od-posts-featured").evaluate("(img) => img.complete && img.naturalWidth > 0")
            media = page.request.get(base + "/api/admin/media?limit=20&offset=0").json()
            assert len(media) == 1
            media_id = media[0]["id"]

            post.get_by_role("button", name="Lưu bản nháp").click()
            expect(post.get_by_text("Đã lưu bản nháp.", exact=False)).to_be_visible()
            slug = "bai-viet-thu-nghiem"
            record_url = "/api/admin/content-types/post/entries"
            entry = page.request.get(base + record_url + "?limit=10&offset=0").json()[0]
            assert entry["publication_status"] == "draft"
            assert entry["values"]["featured_image"] == media_id
            entry_id = entry["id"]
            assert page.request.get(base + "/content/post/" + slug).status == 404
            assert page.request.get(base + "/media/" + media_id + "/file").status == 404
            assert page.request.get(base + "/api/admin/media/" + media_id + "/thumbnail").status == 200
            assert post.get_by_role("link", name=re.compile("Xem trang công khai")).count() == 0

            # Preview is persisted-only, private, no-store and CSP-protected.
            preview_url = base + record_url + "/" + str(entry_id) + "/preview"
            preview_response = page.request.get(preview_url)
            assert preview_response.status == 200
            assert "no-store" in preview_response.headers["cache-control"]
            assert "script-src 'none'" in preview_response.headers["content-security-policy"]
            with page.expect_popup() as popup_info:
                post.get_by_role("button", name=re.compile("Xem trước")).click()
            preview_tab = popup_info.value
            preview_tab.wait_for_url(re.compile("/preview$"), timeout=15000)
            expect(preview_tab.locator("body")).to_contain_text("Bản nháp")
            assert preview_tab.evaluate("window.__xss === undefined")
            preview_tab.close()
            page.screenshot(path=str(screenshots / "forge-post-desktop.png"), full_page=True)

            post.get_by_role("button", name="Xuất bản", exact=True).click()
            expect(post.get_by_text("Đã xuất bản nội dung.")).to_be_visible()
            assert page.request.get(base + "/content/post/" + slug).status == 200
            assert page.request.get(base + "/media/" + media_id + "/file").status == 200
            fields.locator("textarea").fill("Bản đã xuất bản và cập nhật")
            post.get_by_role("button", name="Lưu thay đổi").click()
            expect(post.get_by_text("Trạng thái xuất bản được giữ nguyên.")).to_be_visible()
            assert "cập nhật" in page.request.get(base + "/content/post/" + slug).text()

            # Version-CAS collision must leave the typed, unsaved textarea intact.
            before = page.request.get(base + record_url + "/" + str(entry_id)).json()
            csrf = page.request.get(base + "/auth/me").json()["csrf_token"]
            replacement = {"title": before["title"], "slug": before["slug"], "values": before["values"]}
            remote = page.request.put(
                base + record_url + "/" + str(entry_id),
                data=json.dumps(replacement),
                headers={"Content-Type": "application/json", "X-CSRF-Token": csrf,
                         "If-Match": '"v' + str(before["version"]) + '"'},
            )
            assert remote.status == 200, remote.text()
            fields.locator("textarea").fill("Bản nhập chưa lưu khi gặp 412")
            post.get_by_role("button", name="Lưu thay đổi").click()
            expect(post.get_by_role("alert")).to_contain_text("412")
            expect(fields.locator("textarea")).to_have_value("Bản nhập chưa lưu khi gặp 412")
            post.get_by_role("button", name=re.compile("Tải bản mới để")).click()
            expect(fields.locator("textarea")).to_have_value("Bản đã xuất bản và cập nhật")

            # Expired session recovers same-account unsaved input.
            fields.locator("textarea").fill("Giữ bản nhập qua phiên hết hạn")
            with closing(sqlite3.connect(db)) as connection, connection:
                connection.execute("UPDATE sessions SET expires_at=0")
            post.get_by_role("button", name="Lưu thay đổi").click()
            expect(page.locator("#auth-login")).to_be_visible()
            page.locator("#auth-username").fill("forgeadmin")
            page.locator("#auth-password").fill(password)
            page.locator("#auth-login").click()
            expect(post.locator("textarea")).to_have_value("Giữ bản nhập qua phiên hết hạn")
            post.get_by_role("button", name="Lưu thay đổi").click()
            expect(post.get_by_text("Trạng thái xuất bản được giữ nguyên.")).to_be_visible()

            post.get_by_role("button", name="Hủy xuất bản").click()
            expect(post.get_by_text("Đã hủy xuất bản.", exact=False)).to_be_visible()
            assert page.request.get(base + "/content/post/" + slug).status == 404
            assert page.request.get(base + "/media/" + media_id + "/file").status == 404

            # Keep a local edit across a route change, and verify isolated user state.
            fields.locator("textarea").fill("Chỉ lưu trên máy của Admin")
            page.locator('button[data-view="pages"]').click()
            page.locator('button[data-view="posts"]').click()
            expect(fields.locator("textarea")).to_have_value("Chỉ lưu trên máy của Admin")
            post.get_by_role("button", name="Danh sách").click()
            page.locator('button[data-view="users"]').click()
            users = page.locator('.od-page-container .admin-section form')
            users.locator("input").nth(0).fill("forgeeditor")
            users.locator("input").nth(1).fill(password)
            users.locator('button[type="submit"]').click()
            expect(page.locator("#status")).to_contain_text("Đã tạo")

            # Change account after 401: old admin's React editor must be cleared.
            page.locator('button[data-view="posts"]').click()
            post.get_by_role("button", name=re.compile("Thêm bài viết")).click()
            fields.locator('input[type="text"]').nth(0).fill("Không để lộ bản nhập")
            with closing(sqlite3.connect(db)) as connection, connection:
                connection.execute("UPDATE sessions SET expires_at=0")
            page.locator('button[data-view="pages"]').click()
            page.locator('section[aria-label="Trang"]').get_by_role("button", name="Tải lại").click()
            expect(page.locator("#auth-login")).to_be_visible()
            page.locator("#auth-username").fill("forgeeditor")
            page.locator("#auth-password").fill(password)
            page.locator("#auth-login").click()
            expect(page.locator("#user-info")).to_contain_text("forgeeditor")
            page.locator('button[data-view="posts"]').click()
            expect(post.get_by_text("Tất cả bài viết")).to_be_visible()
            assert post.get_by_text("Không để lộ bản nhập").count() == 0

            # Editor can create/Publish/Unpublish/Delete a Page.
            page.locator('button[data-view="pages"]').click()
            pages = page.locator('section[aria-label="Trang"]')
            pages.get_by_role("button", name=re.compile("Thêm trang")).click()
            page_fields = pages.locator(".od-posts-fields")
            page_fields.locator('input[type="text"]').nth(0).fill("Trang biên tập viên")
            page_fields.locator("textarea").fill("Editor content")
            pages.get_by_role("button", name="Lưu bản nháp").click()
            expect(pages.get_by_text("Đã lưu bản nháp.", exact=False)).to_be_visible()
            assert page.request.get(base + "/content/page/trang-bien-tap-vien").status == 404
            pages.get_by_role("button", name="Xuất bản", exact=True).click()
            expect(pages.get_by_text("Đã xuất bản nội dung.")).to_be_visible()
            assert page.request.get(base + "/content/page/trang-bien-tap-vien").status == 200
            pages.get_by_role("button", name="Hủy xuất bản").click()
            expect(pages.get_by_text("Đã hủy xuất bản.", exact=False)).to_be_visible()
            assert page.request.get(base + "/content/page/trang-bien-tap-vien").status == 404

            # Real Chrome, narrow layout, no horizontal scrolling in both editor and list.
            page.set_viewport_size({"width": 390, "height": 844})
            # Let the AdminShell sidebar's 200ms desktop-to-mobile transform finish before capture.
            page.wait_for_timeout(300)
            page.screenshot(path=str(screenshots / "forge-pages-mobile-editor.png"), full_page=True)
            assert page.evaluate("document.documentElement.scrollWidth <= window.innerWidth + 1"), "Mobile editor overflows"
            mobile_cards = pages.locator(".od-posts-aside > .od-posts-card")
            assert abs(mobile_cards.nth(0).bounding_box()["width"] - mobile_cards.nth(1).bounding_box()["width"]) <= 2, "Mobile featured image card is not full-width"
            pages.get_by_role("button", name=re.compile("Xóa trang")).click()
            expect(pages.get_by_text("Tất cả trang")).to_be_visible()
            page.screenshot(path=str(screenshots / "forge-pages-mobile-list.png"), full_page=True)
            assert page.evaluate("document.documentElement.scrollWidth <= window.innerWidth + 1"), "Mobile list overflows"

            # Actual server pagination with 22 entries (one from Admin, 21 from Editor).
            page.set_viewport_size({"width": 1440, "height": 900})
            page.locator('button[data-view="posts"]').click()
            csrf = page.request.get(base + "/auth/me").json()["csrf_token"]
            for number in range(21):
                created = page.request.post(
                    base + record_url,
                    data=json.dumps({
                        "title": f"Bài viết phân trang {number:02d}",
                        "slug": f"bai-viet-phan-trang-{number:02d}",
                        "values": {"body": "Pagination fixture"},
                    }),
                    headers={"Content-Type": "application/json", "X-CSRF-Token": csrf},
                )
                assert created.status == 201, created.text()
            post.get_by_role("button", name="Tải lại").click()
            expect(post.locator(".od-posts-list-item")).to_have_count(20)
            post.get_by_role("button", name=re.compile("Trang sau")).click()
            expect(post.locator(".od-posts-list-item")).to_have_count(2)
            post.get_by_role("button", name=re.compile("Trang trước")).click()
            expect(post.locator(".od-posts-list-item")).to_have_count(20)

            assert not page_errors, page_errors
            browser.close()
    print("PASS: Chrome Admin/Editor Posts+Pages, featured MediaPicker, draft/private media/preview, publish/save/unpublish, 412, 401, account isolation, deletion, 22-entry pagination, 390px/1440px screenshots")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "cms/target/release/opendash.exe")
    parser.add_argument("--browser", default=r"C:\Program Files\Google\Chrome\Application\chrome.exe")
    parser.add_argument("--screenshots", type=Path, default=ROOT / "docs/verification/screenshots/m3-s22-t01")
    args = parser.parse_args()
    verify(args.binary.resolve(), args.browser, args.screenshots.resolve())
