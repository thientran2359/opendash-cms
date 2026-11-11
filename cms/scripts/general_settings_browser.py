"""macOS Chromium acceptance for SITE-01 General Settings with embedded Admin.

Requires Python Playwright and installed Chromium/Chrome:
  python3 cms/scripts/general_settings_browser.py --binary cms/target/debug/opendash
Optional --browser /Applications/Google Chrome.app/Contents/MacOS/Google Chrome
"""
from __future__ import annotations

import argparse
import sqlite3
from contextlib import closing
from pathlib import Path
from tempfile import TemporaryDirectory

from playwright.sync_api import expect, sync_playwright

from content_types_smoke import request, running


def verify(binary: Path, browser_path: str | None):
    with TemporaryDirectory(prefix="opendash-m3-s25-browser-") as folder:
        database = Path(folder) / "site" / "database" / "opendash.sqlite"
        with running(binary, database) as base, sync_playwright() as playwright:
            assert request(base, "/auth/setup", {
                "username": "settingsadmin", "password": "password123",
            })[0] == 201
            browser = playwright.chromium.launch(
                executable_path=browser_path, channel=None if browser_path else "chrome",
                headless=True,
            )
            context = browser.new_context(viewport={"width": 1280, "height": 860})
            errors = []
            context.on("page", lambda page: page.on("pageerror", lambda err: errors.append(str(err))))
            first = context.new_page()
            first.goto(base + "/admin/#/settings")
            expect(first.locator("#auth-username")).to_be_visible()
            first.locator("#auth-username").fill("settingsadmin")
            first.locator("#auth-password").fill("password123")
            first.locator("#auth-login").click()
            expect(first.get_by_role("heading", name="Cài đặt chung")).to_be_visible()
            title = first.get_by_label("Tên website")
            tagline = first.get_by_label("Tagline")
            timezone = first.get_by_label("Múi giờ", exact=True)
            expect(title).to_have_value("OpenDash")
            expect(tagline).to_have_value("")
            expect(timezone).to_have_value("UTC")
            assert "2026f" in first.locator("#od-timezone-help").inner_text()

            first.set_viewport_size({"width": 390, "height": 844})
            assert first.evaluate("document.documentElement.scrollWidth <= window.innerWidth")
            title.focus()
            title.press("ControlOrMeta+A")
            title.press_sequentially("Hải cảng 🦀 <script>QA</script>")
            tagline.fill("Mô tả <img src=x onerror=alert(1)>")
            timezone.select_option("Asia/Ho_Chi_Minh")
            expect(first.get_by_test_id("timezone-preview")).to_contain_text("Mùa hè")
            # P13 requires explicit zone offsets for fixed UTC instants,
            # not just a visible placeholder/zone label in the preview.
            vietnam_samples = first.get_by_test_id("timezone-preview").locator("p")
            assert "19:00" in vietnam_samples.nth(0).inner_text()
            assert "19:00" in vietnam_samples.nth(1).inner_text()
            first.get_by_role("button", name="Lưu cài đặt").click()
            expect(first.get_by_text("Đã lưu cài đặt website.")).to_be_visible()

            second = context.new_page()
            second.set_viewport_size({"width": 390, "height": 844})
            second.goto(base + "/admin/#/settings")
            expect(second.get_by_role("heading", name="Cài đặt chung")).to_be_visible()
            expect(second.get_by_label("Tên website")).to_have_value("Hải cảng 🦀 <script>QA</script>")
            assert second.evaluate("document.documentElement.scrollWidth <= window.innerWidth")

            # Simulate two open tabs with identical version. A stale save must
            # show 412 and preserve the losing tab's unsaved form.
            second.get_by_label("Tên website").fill("Tab hai chưa lưu")
            first.get_by_label("Tên website").fill("Tab một đã lưu")
            first.get_by_role("button", name="Lưu cài đặt").click()
            expect(first.get_by_text("Đã lưu cài đặt website.")).to_be_visible()
            second.get_by_role("button", name="Lưu cài đặt").click()
            expect(second.get_by_role("alert")).to_contain_text("tab khác")
            expect(second.get_by_label("Tên website")).to_have_value("Tab hai chưa lưu")
            second.get_by_role("button", name="Tải lại từ máy chủ").click()
            expect(second.get_by_label("Tên website")).to_have_value("Tab một đã lưu")
            second.get_by_label("Múi giờ", exact=True).select_option("America/New_York")
            expect(second.get_by_test_id("timezone-preview")).to_contain_text("Mùa đông")
            new_york_samples = second.get_by_test_id("timezone-preview").locator("p")
            assert "07:00" in new_york_samples.nth(0).inner_text()
            assert "08:00" in new_york_samples.nth(1).inner_text()
            second.get_by_role("button", name="Lưu cài đặt").click()
            expect(second.get_by_text("Đã lưu cài đặt website.")).to_be_visible()

            with closing(sqlite3.connect(database)) as conn:
                assert conn.execute(
                    "SELECT site_title, timezone FROM site_general_settings WHERE id=1"
                ).fetchone() == ("Tab một đã lưu", "America/New_York")
            assert not errors, errors
            browser.close()

        # Restart actual Rust binary and verify persisted values through the UI.
        with running(binary, database) as base, sync_playwright() as playwright:
            browser = playwright.chromium.launch(
                executable_path=browser_path, channel=None if browser_path else "chrome",
            )
            page = browser.new_page(viewport={"width": 390, "height": 844})
            page.goto(base + "/admin/#/settings")
            page.locator("#auth-username").fill("settingsadmin")
            page.locator("#auth-password").fill("password123")
            page.locator("#auth-login").click()
            expect(page.get_by_label("Tên website")).to_have_value("Tab một đã lưu")
            expect(page.get_by_label("Múi giờ", exact=True)).to_have_value("America/New_York")
            browser.close()
    print("PASS: macOS Settings Chromium, keyboard, 390px, 412 draft, retry, restart")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("cms/target/debug/opendash"))
    parser.add_argument("--browser", help="Optional Chrome/Chromium executable")
    args = parser.parse_args()
    verify(args.binary.resolve(), args.browser)
