"""M4-S07-T01 responsive acceptance against a disposable SQLite site and real Chrome.

Run after npm build then cargo build (the binary embeds the Admin bundle):
  python cms/scripts/builder_responsive_browser.py --binary cms/target/debug/opendash.exe --browser "C:/Program Files/Google/Chrome/Application/chrome.exe"
"""

from __future__ import annotations

import argparse
import copy
import json
import re
import shutil
import sqlite3
from contextlib import closing
from pathlib import Path
from tempfile import TemporaryDirectory

from playwright.sync_api import expect, sync_playwright

from builder_editor_browser import running, save, state


def original_document() -> dict:
    children = []
    for index, title in enumerate(("One", "Two", "Three"), 1):
        props: dict = {"text": title}
        if index == 3:
            # Existing P05 binding must never be overwritten by layout editing.
            props["binding"] = {
                "type_key": "post", "entry_slug": "unavailable",
                "source": {"kind": "title"}, "target": "text",
            }
        children.append({
            "id": f"column-{index}",
            "children": [{"id": f"element-{index}", "type": "core/text", "props": props}],
        })
    return {"schema_version": 1, "sections": [
        {"id": "section-root", "children": [{"id": "row-root", "children": children}]},
    ]}


def current_grid(frame, selector: str = 'div.builder-row[data-node-id="row-root"]') -> dict:
    return frame.locator(selector).evaluate("""el => {
      const style = getComputedStyle(el);
      return {
        display: style.display,
        columns: style.gridTemplateColumns.split(' ').filter(Boolean).length,
        gap: style.gap,
        padding: style.paddingTop,
      };
    }""")


def public_grid(page) -> dict:
    return current_grid(page)


def verify(binary: Path, browser_path: str | None) -> None:
    with TemporaryDirectory(prefix="opendash-m4-responsive-") as folder:
        site = Path(folder) / "site"
        database = site / "database" / "opendash.sqlite"
        snapshot = Path(folder) / "pre-responsive.sqlite"
        before = original_document()
        with running(binary, database) as base, sync_playwright() as playwright:
            browser = playwright.chromium.launch(executable_path=browser_path)
            context = browser.new_context(viewport={"width": 1440, "height": 900})
            page = context.new_page()
            failures: list[str] = []
            page.on("pageerror", lambda error: failures.append(str(error)))
            page.goto(base + "/admin/#/builder")
            page.locator("#auth-username").fill("responsiveadmin")
            page.locator("#auth-password").fill("responsive-pass-123")
            page.locator("#auth-setup").click()
            expect(page.locator("#status")).to_contain_text("Đã tạo")
            page.locator("#auth-login").click()
            expect(page.locator('[data-testid="builder-workspace"]')).to_contain_text("Đã mở")
            csrf = page.request.get(base + "/auth/me").json()["csrf_token"]
            seeded = page.request.put(
                base + "/api/admin/builder/homepage",
                headers={"X-CSRF-Token": csrf, "If-Match": '"v1"'},
                data=before,
            )
            assert seeded.status == 200, seeded.text()
            assert state(page, base)["version"] == 2
            page.get_by_role("button", name="Tải bản server").click()
            expect(page.get_by_role("treeitem", name="Row row-root")).to_be_visible()
            expect(page.locator("#save")).to_be_disabled()

            # Record a byte-accurate online SQLite snapshot before first
            # responsive metadata Save; it can be restored to a separate site.
            with closing(sqlite3.connect(database)) as live, closing(sqlite3.connect(snapshot)) as backup:
                live.backup(backup)
                stored = backup.execute(
                    "SELECT document_json, version FROM builder_homepage WHERE id=1"
                ).fetchone()
                assert stored and stored[1] == 2
                assert json.loads(stored[0]) == before

            page.get_by_role("treeitem", name="Row row-root").click()
            page.get_by_label("Số cột mỗi hàng").fill("3")
            page.get_by_label("Gap (px)").fill("0")
            page.get_by_label("Padding (px)").fill("0")
            page.get_by_label("Viewport Builder").select_option("tablet")
            page.get_by_label("Số cột mỗi hàng").fill("2")
            page.get_by_label("Padding (px)").fill("12")
            page.get_by_label("Viewport Builder").select_option("mobile")
            page.get_by_label("Số cột mỗi hàng").fill("1")
            page.get_by_label("Padding (px)").fill("0")
            assert state(page, base)["version"] == 2, "Inspector edits must stay local"
            page.get_by_role("treeitem", name="Column column-1").click()
            page.get_by_label("Viewport Builder").select_option("desktop")
            page.get_by_label("Căn chữ").select_option("center")
            page.get_by_label("Viewport Builder").select_option("mobile")
            page.get_by_label("Căn chữ").select_option("end")

            # The preview iframe has the actual selected CSS viewport width.
            page.get_by_role("button", name="Xem trước").click()
            expect(page.locator("#canvas")).to_be_visible()
            iframe = page.locator("#canvas")
            frame = page.frame_locator("#canvas")
            expect(frame.locator("main[data-builder-version='1']")).to_be_visible()
            assert iframe.evaluate("el => el.getBoundingClientRect().width") == 390
            assert iframe.get_attribute("sandbox") == ""
            assert page.evaluate("() => document.querySelector('#canvas').contentDocument === null")
            preview_html = iframe.get_attribute("srcdoc")
            assert "data-builder-responsive" in preview_html
            expect(frame.locator("style[data-builder-responsive]")).to_have_count(1)
            preview_css = frame.locator("style[data-builder-responsive]").text_content()
            assert "@media (width < 1024px)" in preview_html
            assert "@media (width < 768px)" in preview_html
            column = frame.locator('div.builder-column[data-node-id="column-1"]')
            expect(column).to_have_css("text-align", "end")
            iframe.evaluate("(el) => el.style.width = '834px'")
            expect(column).to_have_css("text-align", "center")

            # Integer CSS-pixel edges and fractional widths are exercised on
            # the iframe's *actual layout width*, not a visual CSS transform.
            # Chromium quantizes nested iframe layout viewports to an integer
            # at half-pixel boundaries. Quarter-pixel inputs retain the lower
            # CSS viewport while still probing fractional CSS lengths.
            for width, expected in [
                (767, 1), (767.25, 1), (768, 2), (1023, 2),
                (1023.25, 2), (1024, 3), (1280, 3),
            ]:
                iframe.evaluate("(el, value) => el.style.width = value + 'px'", width)
                page.wait_for_timeout(40)
                computed = current_grid(frame)
                assert computed["display"] == "grid" and computed["columns"] == expected, (
                    width, computed,
                )
                if width == 767:
                    assert computed["gap"] == "0px" and computed["padding"] == "0px", computed
                if width == 768:
                    assert computed["padding"] == "12px", computed
            assert state(page, base)["version"] == 2, "Preview must not persist responsive edits"
            print("PASS: 767/767.25/768/1023/1023.25/1024 CSS-pixel breakpoints, 3/2/1 grid and zero")

            page.get_by_role("button", name="Canvas cấu trúc").click()
            page.get_by_role("treeitem", name="Row row-root").click()
            page.get_by_label("Viewport Builder").select_option("tablet")
            # Check the React structural Canvas matches the selected viewport.
            grid = page.locator('[data-structural-grid="true"]')
            expect(grid).to_have_css("display", "grid")
            assert len(grid.evaluate("el => getComputedStyle(el).gridTemplateColumns.split(' ').filter(Boolean)")) == 2
            save(page)
            saved = state(page, base)
            assert saved["version"] == 3
            row = saved["document"]["sections"][0]["children"][0]
            assert row["responsive"]["desktop"] == {
                "columns_per_row": 3, "gap_px": 0, "padding_px": 0,
            }
            assert row["responsive"]["tablet"]["columns_per_row"] == 2
            assert row["responsive"]["mobile"]["columns_per_row"] == 1
            assert saved["document"]["sections"][0]["children"][0]["children"][2]["children"][0]["props"]["binding"] == (
                before["sections"][0]["children"][0]["children"][2]["children"][0]["props"]["binding"]
            )
            page.get_by_label("Viewport Builder").select_option("mobile")
            page.get_by_label("Viewport Builder").select_option("desktop")
            expect(page.locator("#save")).to_be_disabled()

            # The exact server CSS is shared by public and Preview, and public
            # computed style is checked independently in a real top-level page.
            public = context.new_page()
            public.goto(base + "/")
            css = public.locator("style[data-builder-responsive]").text_content()
            assert preview_css == css
            for width, columns, padding in [(767, 1, "0px"), (768, 2, "12px"), (1023, 2, "12px"), (1024, 3, "0px")]:
                public.set_viewport_size({"width": width, "height": 850})
                result = public_grid(public)
                assert result["display"] == "grid" and result["columns"] == columns, (width, result)
                assert result["padding"] == padding, (width, result)
            public.close()
            print("PASS: Save/public shares server CSS with Preview, binding metadata preserved")

            # Removing an override falls back to the wider viewport field by
            # field, while the other mobile override remains unchanged.
            page.get_by_label("Viewport Builder").select_option("tablet")
            page.get_by_role("button", name="Bỏ override columns_per_row").click()
            expect(page.get_by_label("Số cột mỗi hàng")).to_have_value("")
            assert "Kế thừa: 3" in page.get_by_label("Số cột mỗi hàng").get_attribute("placeholder")
            save(page)
            reread = state(page, base)
            assert reread["version"] == 4
            assert "columns_per_row" not in reread["document"]["sections"][0]["children"][0]["responsive"]["tablet"]
            page.reload()
            expect(page.locator('[data-testid="builder-workspace"]')).to_contain_text('ETag: "v4"')
            page.get_by_role("treeitem", name="Row row-root").click()
            page.get_by_label("Viewport Builder").select_option("tablet")
            assert page.get_by_label("Số cột mỗi hàng").input_value() == ""
            page.get_by_role("button", name="Xem trước").click()
            preview_grid = page.frame_locator("#canvas").locator('div.builder-row[data-node-id="row-root"]')
            expect(preview_grid).to_have_css("grid-template-columns", re.compile(r"px.*px.*px"))
            print("PASS: per-field cascading/reset and persisted overrides after Save/reopen")

            # Server rejects wrong JSON types, fields, contexts and limits;
            # rejected requests leave the current ETag and DB document intact.
            for bad in [
                {"responsive": {"tablet": {"gap_px": 5}}, "node": "section"},
                {"responsive": {"mobile": {"columns_per_row": 7}}, "node": "row"},
                {"responsive": {"desktop": {"columns_per_row": 0}}, "node": "row"},
                {"responsive": {"tablet": {"padding_px": -2}}, "node": "row"},
                {"responsive": {"tablet": {"padding_px": 1.5}}, "node": "row"},
                {"responsive": {"tablet": {"color": "red"}}, "node": "row"},
                {"responsive": {"tablet": {"text_align": "justify"}}, "node": "column"},
                {"responsive": {"phone": {"padding_px": 1}}, "node": "section"},
            ]:
                mutated = copy.deepcopy(reread["document"])
                section = mutated["sections"][0]
                row = section["children"][0]
                column = row["children"][0]
                target = {"section": section, "row": row, "column": column}[bad["node"]]
                target["responsive"] = bad["responsive"]
                rejected = page.request.put(
                    base + "/api/admin/builder/homepage",
                    headers={"X-CSRF-Token": csrf, "If-Match": '"v4"'},
                    data=mutated,
                )
                assert rejected.status == 400, (bad, rejected.status, rejected.text())
                assert state(page, base)["version"] == 4
            assert not failures, failures
            browser.close()
            print("PASS: malformed responsive payload denied; ETag and current DB unchanged")

        # Restart using the same site: metadata and order survive process exit.
        with running(binary, database) as base, sync_playwright() as playwright:
            browser = playwright.chromium.launch(executable_path=browser_path)
            page = browser.new_page(viewport={"width": 768, "height": 900})
            page.goto(base + "/")
            assert public_grid(page)["columns"] == 3
            page.set_viewport_size({"width": 767, "height": 900})
            assert public_grid(page)["columns"] == 1
            browser.close()

        # A complete pre-edit online SQLite snapshot is readable after being
        # restored to an isolated alternative site. No user site is modified.
        rollback = Path(folder) / "rollback" / "database" / "opendash.sqlite"
        rollback.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(snapshot, rollback)
        with closing(sqlite3.connect(rollback)) as db:
            saved_raw, saved_version = db.execute(
                "SELECT document_json, version FROM builder_homepage WHERE id=1"
            ).fetchone()
            assert saved_version == 2 and json.loads(saved_raw) == before
            assert db.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        with running(binary, rollback) as base:
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(executable_path=browser_path)
                page = browser.new_page(viewport={"width": 1024, "height": 850})
                page.goto(base + "/")
                assert page.locator("style[data-builder-responsive]").count() == 0
                assert public_grid(page)["display"] != "grid"
                browser.close()
        print("PASS: restart persistence and isolated pre-metadata SQLite snapshot restore")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("cms/target/debug/opendash.exe"))
    parser.add_argument("--browser", type=str, default=None)
    args = parser.parse_args()
    verify(args.binary.resolve(), args.browser)
