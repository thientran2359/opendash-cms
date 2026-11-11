"""M3-S27-T01 embedded Admin Chrome: Themes A/B/C, Builder preservation, 390px."""
import argparse
from pathlib import Path
from tempfile import TemporaryDirectory

from playwright.sync_api import expect, sync_playwright
from windows_smoke import running
from themes_smoke import theme_zip


def verify(binary, chrome):
    a = theme_zip("qa-a/banner", '<h1 data-theme="A">{{ values.title }}</h1><p>{{ values.note }}</p>')
    b = theme_zip("qa-b/banner", '<section data-theme="B"><h2>{{ values.title }}</h2><p>{{ values.note }}</p></section>', alias="qa-a/banner")
    c = theme_zip("qa-c/banner", '<h3 data-theme="C">{{ values.title }}</h3>')
    with TemporaryDirectory(prefix="opendash-themes-chrome-") as folder:
        db = Path(folder) / "cms.sqlite"
        with running(binary, db) as base, sync_playwright() as pw:
            browser = pw.chromium.launch(executable_path=chrome)
            context = browser.new_context(viewport={"width": 1280, "height": 900})
            page = context.new_page()
            errors = []
            page.on("pageerror", lambda error: errors.append(str(error)))
            page.goto(base + "/admin/#/themes")
            page.locator("#auth-username").fill("themebrowser")
            page.locator("#auth-password").fill("password-browser-1")
            page.locator("#auth-setup").click()
            expect(page.locator("#status")).to_contain_text("Đã tạo")
            page.locator("#auth-login").click()
            panel = page.locator("[data-testid='themes-panel']")
            expect(panel).to_be_visible()
            expect(panel.locator("[data-testid='active-theme']")).to_contain_text("legacy")

            for name, data in (("a.zip", a), ("b.zip", b), ("c.zip", c)):
                panel.locator("[data-testid='themes-upload']").set_input_files({
                    "name": name, "mimeType": "application/zip", "buffer": data,
                })
                expect(panel.get_by_text("Đã cài theme", exact=False)).to_be_visible()
            assert panel.locator("[data-theme-id]").count() == 3
            panel.get_by_role("button", name="Kích hoạt theme qa-a/banner").click()
            expect(panel.locator("[data-testid='active-theme']")).to_contain_text("qa-a/banner")

            # A: Builder saves the original owner/block and fields.
            page.locator('nav[aria-label="Các mục quản trị"] [data-view="builder"]').click()
            expect(page.locator("#type")).to_have_value("qa-a/banner")
            page.locator("#field-title").fill("Chrome original block")
            page.locator("#field-note").fill("Preserved values")
            page.locator("#preview").click()
            expect(page.frame_locator("#canvas").locator('[data-theme="A"]')).to_have_text("Chrome original block")
            assert "Chrome original block" not in page.request.get(base + "/").text()
            page.locator("#save").click()
            expect(page.locator("#status")).to_have_text("Đã lưu")
            assert 'data-theme="A"' in page.request.get(base + "/").text()

            # A→B: active alias displays saved block in B markup; draft not overwritten.
            page.locator('nav[aria-label="Các mục quản trị"] [data-view="themes"]').click()
            panel.get_by_role("button", name="Kích hoạt theme qa-b/banner").click()
            expect(panel.locator("[data-testid='theme-compat-ok']")).to_be_visible()
            assert 'data-theme="B"' in page.request.get(base + "/").text()
            page.locator('nav[aria-label="Các mục quản trị"] [data-view="builder"]').click()
            expect(page.locator("#field-title")).to_have_value("Chrome original block")
            page.locator("#field-title").fill("Chrome unsaved B draft")
            page.locator("#preview").click()
            expect(page.frame_locator("#canvas").locator('[data-theme="B"] h2')).to_have_text("Chrome unsaved B draft")
            assert "Chrome unsaved B draft" not in page.request.get(base + "/").text()

            # A→C: hidden public, but unsaved draft remains in mounted Builder.
            page.locator('nav[aria-label="Các mục quản trị"] [data-view="themes"]').click()
            panel.get_by_role("button", name="Kích hoạt theme qa-c/banner").click()
            expect(panel.locator("[data-testid='theme-compat-warning']")).to_be_visible()
            assert "Chrome original block" not in page.request.get(base + "/").text()
            page.locator('nav[aria-label="Các mục quản trị"] [data-view="builder"]').click()
            expect(page.locator("#field-title")).to_have_value("Chrome unsaved B draft")
            page.locator("#preview").click()
            expect(page.locator("[data-testid='builder-visibility']")).to_contain_text("không hỗ trợ")
            assert "Chrome unsaved B draft" not in page.frame_locator("#canvas").locator("body").inner_text()
            page.locator("#save").click()
            expect(page.locator("#status")).to_have_text("Đã lưu")
            assert page.request.get(base + "/page").json()["values"]["title"] == "Chrome unsaved B draft"

            # C→A restores source template, including values saved while C was active.
            page.locator('nav[aria-label="Các mục quản trị"] [data-view="themes"]').click()
            panel.get_by_role("button", name="Kích hoạt theme qa-a/banner").click()
            expect(panel.locator("[data-testid='theme-compat-ok']")).to_be_visible()
            assert "Chrome unsaved B draft" in page.request.get(base + "/").text()
            assert 'data-theme="A"' in page.request.get(base + "/").text()
            page.locator('nav[aria-label="Các mục quản trị"] [data-view="builder"]').click()
            expect(page.locator("#field-title")).to_have_value("Chrome unsaved B draft")
            page.locator("#preview").click()
            expect(page.frame_locator("#canvas").locator('[data-theme="A"]')).to_have_text("Chrome unsaved B draft")

            # A second tab carrying a stale version gets 412, without losing Builder data.
            tab2 = context.new_page()
            tab2.goto(base + "/admin/#/themes")
            other_panel = tab2.locator("[data-testid='themes-panel']")
            expect(other_panel).to_be_visible()
            panel = None
            page.locator('nav[aria-label="Các mục quản trị"] [data-view="themes"]').click()
            page.locator("[data-testid='themes-panel']").get_by_role("button", name="Kích hoạt theme qa-b/banner").click()
            other_panel.get_by_role("button", name="Kích hoạt theme qa-c/banner").click()
            expect(other_panel.get_by_role("alert")).to_contain_text("(412)")
            tab2.close()
            page.locator('nav[aria-label="Các mục quản trị"] [data-view="builder"]').click()
            expect(page.locator("#field-title")).to_have_value("Chrome unsaved B draft")

            # Mobile horizontal overflow & keyboard accessibility.
            page.set_viewport_size({"width": 390, "height": 844})
            page.goto(base + "/admin/#/themes")
            panel = page.locator("[data-testid='themes-panel']")
            expect(panel).to_be_visible()
            assert page.evaluate("document.documentElement.scrollWidth <= document.documentElement.clientWidth + 1")
            button = panel.get_by_role("button", name="Tải lại danh sách")
            button.focus()
            assert button.evaluate("(el) => el === document.activeElement")
            assert not errors, errors
            browser.close()
        with running(binary, db) as base:
            # The final active theme B and Builder contents survive restart.
            assert 'data-theme="B"' in __import__("urllib.request", fromlist=["urlopen"]).urlopen(base + "/").read().decode()
            assert "Chrome unsaved B draft" in __import__("urllib.request", fromlist=["urlopen"]).urlopen(base + "/").read().decode()
    print("PASS: Themes Chrome A→B, A→C, C→A, draft retention, CAS tab, 390px, restart")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("cms/target/debug/opendash"))
    parser.add_argument("--browser", help="Chrome executable path")
    args = parser.parse_args()
    verify(args.binary.resolve(), args.browser)
