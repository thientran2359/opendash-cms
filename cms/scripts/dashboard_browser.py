"""M3-S26-T01 real Chrome Dashboard checks, including 390px, loading/error/retry and roles.

Uses an isolated temporary SQLite site and current embedded Admin bundle.
From cms/: python scripts/dashboard_browser.py --binary target/debug/opendash.exe --browser PATH
"""
import argparse
from pathlib import Path
from tempfile import TemporaryDirectory

from playwright.sync_api import expect, sync_playwright
from windows_smoke import running


def metric(page, key, value):
    card = page.locator(f'[data-dashboard-metric="{key}"]')
    expect(card).to_be_visible()
    expect(card).to_contain_text(str(value))


def verify(binary: Path, browser_path: str | None):
    with TemporaryDirectory(prefix="opendash-dashboard-browser-") as folder:
        database = Path(folder) / "site.sqlite"
        with running(binary, database) as base, sync_playwright() as pw:
            browser = pw.chromium.launch(executable_path=browser_path)
            admin_context = browser.new_context(viewport={"width": 1280, "height": 900})
            page = admin_context.new_page()
            errors = []
            page.on("pageerror", lambda err: errors.append(str(err)))

            # Delay only Dashboard GETs to assert that the UI distinguishes loading
            # from a legitimate zero. The actual request still reaches the server.
            page.add_init_script("""
                (() => {
                    const originalFetch = window.fetch.bind(window);
                    window.fetch = (input, options) => {
                        const url = typeof input === 'string' ? input : input.url;
                        if (url.includes('/api/admin/dashboard')) {
                            return new Promise(resolve => setTimeout(resolve, 650))
                                .then(() => originalFetch(input, options));
                        }
                        return originalFetch(input, options);
                    };
                })();
            """)
            assert admin_context.request.get(base + "/api/admin/dashboard").status == 401
            page.goto(base + "/admin/#/dashboard")
            page.locator("#auth-username").fill("dashboardadmin")
            page.locator("#auth-password").fill("dashboard-password-1")
            page.locator("#auth-setup").click()
            expect(page.locator("#status")).to_contain_text("Đã tạo")
            page.locator("#auth-login").click()
            expect(page.get_by_role("heading", name="Thống kê nội dung")).to_be_visible()
            expect(page.locator('[data-dashboard-metric="post-draft"]')).to_contain_text("Đang tải")
            for key in ("post-draft", "post-published", "page-draft", "page-published"):
                metric(page, key, 0)

            # The old quick links remain usable (now also directly including Pages).
            quick_links = page.locator(".od-dashboard .od-quick-link")
            expect(quick_links.filter(has_text="Bài viết")).to_be_visible()
            expect(quick_links.filter(has_text="Trang")).to_be_visible()
            quick_links.filter(has_text="Trang").click()
            expect(page.get_by_role("heading", name="Thống kê nội dung")).to_have_count(0)
            page.locator('[data-view="dashboard"]').click()
            expect(page.get_by_role("heading", name="Thống kê nội dung")).to_be_visible()
            metric(page, "post-published", 0)

            # HTTP 503: no misleading zeros, explicit retry, quick links still work.
            def unavailable(route):
                route.fulfill(status=503, content_type="text/plain", body="Đang bảo trì Dashboard")

            page.route("**/api/admin/dashboard", unavailable)
            page.get_by_role("button", name="Làm mới thống kê").click()
            expect(page.get_by_role("alert")).to_contain_text("Không tải được thống kê")
            expect(page.locator('[data-dashboard-metric="post-draft"]')).to_contain_text("—")
            expect(quick_links.filter(has_text="Bài viết")).to_be_visible()
            page.unroute("**/api/admin/dashboard", unavailable)
            page.get_by_role("button", name="Thử lại").click()
            expect(page.get_by_role("alert")).to_have_count(0)
            metric(page, "page-draft", 0)

            session = admin_context.request.get(base + "/auth/me").json()
            csrf = session["csrf_token"]

            def create(kind, slug):
                response = admin_context.request.post(
                    base + f"/api/admin/content-types/{kind}/entries",
                    headers={"X-CSRF-Token": csrf},
                    data={"slug": slug, "title": slug,
                          "values": {"body": "Hello Dashboard", "featured_image": None}},
                )
                assert response.status == 201, response.text()
                return response.json()

            post = create("post", "dashboard-post")
            page_record = create("page", "dashboard-page")
            status = admin_context.request.post(
                base + f"/api/admin/content-types/page/entries/{page_record['id']}/publication",
                headers={"X-CSRF-Token": csrf, "If-Match": f'"v{page_record["version"]}"'},
                data={"status": "published"},
            )
            assert status.status == 200, status.text()
            page.get_by_role("button", name="Làm mới thống kê").click()
            metric(page, "post-draft", 1)
            metric(page, "post-published", 0)
            metric(page, "page-draft", 0)
            metric(page, "page-published", 1)

            # Navigating away and back re-fetches, not a stale cached metric.
            post_pub = admin_context.request.post(
                base + f"/api/admin/content-types/post/entries/{post['id']}/publication",
                headers={"X-CSRF-Token": csrf, "If-Match": f'"v{post["version"]}"'},
                data={"status": "published"},
            )
            assert post_pub.status == 200, post_pub.text()
            quick_links.filter(has_text="Bài viết").click()
            expect(page.locator('[data-view="posts"]')).to_have_attribute("aria-current", "page")
            page.locator('[data-view="dashboard"]').click()
            metric(page, "post-draft", 0)
            metric(page, "post-published", 1)

            # Editor sees the same aggregate and only role-authorized shortcuts.
            created_editor = admin_context.request.post(
                base + "/api/admin/users",
                headers={"X-CSRF-Token": csrf},
                data={"username": "dashboardeditor", "password": "editor-password-1", "role": "editor"},
            )
            assert created_editor.status == 201, created_editor.text()
            editor_context = browser.new_context(viewport={"width": 390, "height": 844})
            editor = editor_context.new_page()
            editor.goto(base + "/admin/#/dashboard")
            editor.locator("#auth-username").fill("dashboardeditor")
            editor.locator("#auth-password").fill("editor-password-1")
            editor.locator("#auth-login").click()
            expect(editor.get_by_role("heading", name="Thống kê nội dung")).to_be_visible()
            metric(editor, "post-published", 1)
            metric(editor, "page-published", 1)
            assert editor_context.request.get(base + "/api/admin/dashboard").status == 200
            assert editor.locator('[data-view="plugins"]').count() == 0
            assert editor.locator('[data-view="users"]').count() == 0
            assert editor.locator('[data-view="settings"]').count() == 0
            assert editor.locator(".od-dashboard .od-quick-link").filter(has_text="Bài viết").count() == 1
            assert editor.locator(".od-dashboard .od-quick-link").filter(has_text="Trang").count() == 1

            width = editor.evaluate("({ viewport: window.innerWidth, document: document.documentElement.scrollWidth })")
            assert width["document"] <= width["viewport"], width
            editor.get_by_role("button", name="Làm mới thống kê").focus()
            editor.keyboard.press("Enter")
            metric(editor, "page-published", 1)

            assert not errors, errors
            editor_context.close()
            admin_context.close()
            browser.close()
    print("PASS: Dashboard UI zero/loading/503/retry, refresh, roles/shortcuts, keyboard and 390px")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/opendash.exe"))
    parser.add_argument("--browser", help="Optional Chrome/Chromium executable path")
    args = parser.parse_args()
    verify(args.binary.resolve(), args.browser)
