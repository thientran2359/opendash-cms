"""Exercise the embedded Admin UI, including recovery of an unsaved edit after 401."""
import argparse
from contextlib import closing
from pathlib import Path
import secrets
import sqlite3
from tempfile import TemporaryDirectory

from playwright.sync_api import expect, sync_playwright
from windows_smoke import running, sample_theme


def verify(binary: Path, browser_path: str | None):
    cms_root = Path(__file__).resolve().parents[1]
    with TemporaryDirectory(prefix="opendash-ui-") as folder:
        database = Path(folder) / "site.sqlite"
        with running(binary, database) as base, sync_playwright() as pw:
            browser = pw.chromium.launch(executable_path=browser_path)
            page = browser.new_page()
            errors = []
            page.on("pageerror", lambda error: errors.append(str(error)))
            page.goto(base + "/admin/")
            expect(page.locator("#status")).to_contain_text("Vui lòng đăng nhập")
            password = secrets.token_hex(14)

            def fill_credentials():
                page.locator("#auth-username").fill("qaadmin")
                page.locator("#auth-password").fill(password)

            fill_credentials()
            page.locator("#auth-setup").click()
            expect(page.locator("#status")).to_contain_text("Đã tạo")
            page.locator("#auth-login").click()
            expect(page.locator("#user-info")).to_contain_text("qaadmin")
            expect(page.get_by_text("HTTP: ok", exact=False)).to_be_visible()
            page.locator("#theme").set_input_files({
                "name": "theme.zip", "mimeType": "application/zip", "buffer": sample_theme()
            })
            title = page.locator("#field-title")
            draft = "OpenDash unsaved edit"
            title.fill(draft)
            page.locator("#preview").click()
            expect(page.frame_locator("#canvas").locator("h2")).to_have_text(draft)
            assert draft not in page.request.get(base + "/").text()

            with closing(sqlite3.connect(database)) as connection, connection:
                connection.execute("UPDATE sessions SET expires_at=0")
            page.locator("#save").click()
            expect(page.locator("#auth-login")).to_be_visible()
            fill_credentials()
            page.locator("#auth-login").click()
            expect(title).to_be_visible()
            expect(title).to_have_value(draft)
            page.locator("#save").click()
            expect(page.locator("#status")).to_have_text("Đã lưu")
            assert draft in page.request.get(base + "/").text()

            page.goto(base + "/builder")
            expect(title).to_have_value(draft)
            plugin_zip = cms_root / "examples/plugins/sample-plugin/sample-plugin.zip"
            page.locator("#plugin-zip").set_input_files(str(plugin_zip))
            page.locator("#btn-run-plugin").click()
            expect(page.locator("#plugin-output")).to_contain_text('"run_count": 1')
            assert not errors, errors
            browser.close()
    print("PASS: Admin, unsaved preview, 401 recovery, save, reload and plugin execution")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/opendash.exe"))
    parser.add_argument("--browser", help="Optional path to an installed Chromium executable")
    args = parser.parse_args()
    verify(args.binary.resolve(), args.browser)
