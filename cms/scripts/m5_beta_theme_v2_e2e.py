#!/usr/bin/env python3
"""M5-S11-T01: real Chrome v2 revision selection, import and preservation on a disposable Mac site.

Runs an existing native OpenDash executable, not a mocked UI/server. The ZIP
fixture derives from the tracked M5 demo website with a bounded sample import.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import pathlib
import sqlite3
import subprocess
import sys
import tempfile
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo

from playwright.sync_api import expect, sync_playwright

from m7_beta_menu_picker_e2e import admin_login, api_json, launch_server, stop_server


def add_zip_file(archive: ZipFile, name: str, data: bytes | str) -> None:
    # Fixed metadata makes the fixture ZIP digest and revision ID reproducible.
    entry = ZipInfo(name, date_time=(2020, 1, 1, 0, 0, 0))
    entry.compress_type = ZIP_DEFLATED
    entry.external_attr = 0o644 << 16
    archive.writestr(entry, data)


def theme_zip(theme_id: str, version: str, *, sample: bool = False) -> bytes:
    demo = pathlib.Path(__file__).resolve().parents[1] / "fixtures/demo-website/theme-moc-studio"
    manifest = json.loads((demo / "theme.json").read_text(encoding="utf-8"))
    manifest.update({"theme_id": theme_id, "version": version, "name": f"F02 {theme_id}"})
    if sample:
        manifest["sample"] = {
            "sample_key": "starter",
            "pages": [{
                "object_key": "starter_page", "slug": "f02-starter", "title": "F02 Starter",
                "content_type": "page", "values": {},
            }],
            "menus": [{
                "object_key": "starter_nav", "menu_key": "f02_starter_nav", "title": "F02 Starter Nav",
                "slot_key": "primary", "items": [
                    {"label": "Starter", "target_kind": "entry", "target_object_key": "starter_page"},
                ],
            }],
            "settings": [{
                "object_key": "starter_brand", "option_key": "studio_eyebrow",
                "value": "F02 Sample Brand",
            }],
        }
    memory = io.BytesIO()
    with ZipFile(memory, "w", ZIP_DEFLATED) as archive:
        add_zip_file(archive,"theme.json",json.dumps(manifest,ensure_ascii=False,sort_keys=True))
        for file in sorted((demo / "templates").glob("*.html")):
            add_zip_file(archive,"templates/"+file.name,file.read_bytes())
        add_zip_file(archive,"assets/style.css",(demo / "assets/style.css").read_bytes())
    return memory.getvalue()


def legacy_zip() -> bytes:
    stream = io.BytesIO()
    with ZipFile(stream, "w", ZIP_DEFLATED) as archive:
        add_zip_file(archive,"block.json",'{"id":"f02legacy/banner","fields":{"title":"Title"}}')
        add_zip_file(archive,"block.html","<h1>{{ values.title }}</h1>")
    return stream.getvalue()


def use_upload(panel, title: str, content: bytes) -> None:
    panel.get_by_test_id("themes-upload").set_input_files({
        "name": title, "mimeType": "application/zip", "buffer": content,
    })
    expect(panel.get_by_role("status").filter(has_text="Đã cài")).to_be_visible(timeout=20000)


def pick(panel, revision_id: str) -> None:
    panel.locator(f'[data-theme-revision-id="{revision_id}"] input[name="selected-theme-revision"]').check()
    assert panel.locator(f'[data-theme-revision-id="{revision_id}"] input').is_checked()


def run(binary: pathlib.Path, artifacts: pathlib.Path) -> int:
    artifacts.mkdir(parents=True, exist_ok=True)
    z1 = theme_zip("f02-editorial", "1.0.0", sample=True)
    z2 = theme_zip("f02-editorial", "2.0.0", sample=True)
    zb = theme_zip("f02-secondary", "1.0.0")
    checks = []
    errors: list[str] = []
    binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
    source_sha = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    with tempfile.TemporaryDirectory(prefix="opendash-m5-s11-t01-") as tmp:
        root = pathlib.Path(tmp).resolve() / "site"
        root.mkdir()
        server, base = launch_server(binary, root)
        try:
            with sync_playwright() as play:
                browser = play.chromium.launch(channel="chrome", headless=True)
                browser_version = browser.version
                ctx = browser.new_context(viewport={"width": 1440, "height": 900})
                api_json(ctx, "POST", base, "/auth/setup", {"username": "admin", "password": "F02-AdminPassword123!"}, expected=201)
                csrf = admin_login(ctx, base, "admin", "F02-AdminPassword123!")
                page = ctx.new_page()
                page.on("pageerror", lambda e: errors.append(str(e)))
                page.goto(base + "/admin#/appearance/themes", wait_until="domcontentloaded")
                panel = page.get_by_test_id("themes-panel")
                expect(panel).to_be_visible(timeout=20000)
                expect(panel.get_by_text("Đã cài (0 legacy · 0 revision v2)")).to_be_visible()

                # Existing live-ish site data is created before themes/import.
                existing = api_json(ctx, "POST", base, "/api/admin/content-types/post/entries",
                                    {"title": "Preserve F02", "slug": "f02-keep", "values": {}}, csrf, expected=201)
                api_json(ctx, "POST", base,
                         f"/api/admin/content-types/post/entries/{existing['id']}/publication",
                         {"status": "published"}, csrf, {"If-Match": f'"v{existing["version"]}"'})

                use_upload(panel, "legacy.zip", legacy_zip())
                expect(panel.locator('[data-theme-id="f02legacy/banner"]')).to_be_visible()
                panel.get_by_role("button", name="Kích hoạt theme f02legacy/banner").click()
                expect(panel.get_by_test_id("active-theme")).to_contain_text("f02legacy/banner")
                checks.append("legacy_zip_install_list_activate")

                use_upload(panel, "editorial-1.zip", z1)
                expect(panel.get_by_test_id("theme-v2-revisions").locator("[data-theme-revision-id]")).to_have_count(1)
                a_summary = api_json(ctx, "GET", base, "/api/admin/themes")
                rev1 = next(v for v in a_summary["revisions"] if v["theme_id"] == "f02-editorial")
                assert a_summary["active_id"] == "f02legacy/banner", a_summary
                # Same canonical ZIP: duplicate 200 via actual Admin upload; one revision.
                with page.expect_response(lambda r: r.url.endswith("/api/admin/themes") and r.request.method == "POST") as upload_response:
                    use_upload(panel, "editorial-1-again.zip", z1)
                assert upload_response.value.status == 200
                assert len([v for v in api_json(ctx,"GET",base,"/api/admin/themes")["revisions"] if v["theme_id"]=="f02-editorial"]) == 1
                use_upload(panel, "editorial-2.zip", z2)
                installed = api_json(ctx, "GET", base, "/api/admin/themes")
                candidates = [r for r in installed["revisions"] if r["theme_id"] == "f02-editorial"]
                assert len(candidates) == 2 and candidates[0]["version"] == "1.0.0" and candidates[1]["version"] == "2.0.0"
                assert set(candidates[0]) == {"revision_id", "theme_id", "version", "revision_digest", "source_format"}
                rev2 = candidates[1]
                assert panel.get_by_text("Đã cài (1 legacy · 2 revision v2)").is_visible()
                checks.append("v2_201_duplicate_200_projection_sorted_no_leak")

                # No implicit activation on upload; explicit CAS activate first revision.
                pick(panel, rev1["revision_id"])
                panel.get_by_role("button", name="Kích hoạt revision đã chọn").click()
                expect(panel.get_by_test_id("active-theme")).to_contain_text("f02-editorial (v2)")
                expect(panel.locator(f'[data-theme-revision-id="{rev1["revision_id"]}"]')).to_contain_text("Đang kích hoạt")
                assert api_json(ctx,"GET",base,"/api/admin/themes")["selection"]["revision_id"] == rev1["revision_id"]
                checks.append("explicit_v2_activation_CAS_and_active_badge")

                # Builder owns the role selector (no unused deep-link parameter).
                panel.get_by_role("navigation",name="Chỉnh sửa template theme").get_by_role("link",name="Mở Visual Builder").click()
                expect(page).to_have_url(base+"/admin#/builder")
                builder_roles = page.get_by_role("combobox",name="Role Builder")
                for role in ("header", "footer", "single", "archive"):
                    builder_roles.select_option(role)
                    expect(builder_roles).to_have_value(role)
                    expect(page.get_by_test_id("builder-workspace")).to_be_visible()
                page.goto(base+"/admin#/appearance/themes")
                checks.append("header_footer_single_archive_builder_role_access")

                # Nonempty-site selective sample dry-run + cancel stays read-only.
                expect(panel.get_by_test_id("sample-import-box")).to_be_visible()
                before = api_json(ctx,"GET",base,"/api/admin/content-types/page/entries?limit=50&offset=0")
                panel.get_by_role("button",name="Xem trước (Dry-run)").click()
                expect(panel.get_by_text("Plan digest:")).to_be_visible(timeout=20000)
                panel.get_by_role("button",name="Huỷ xem trước").click()
                assert api_json(ctx,"GET",base,"/api/admin/content-types/page/entries?limit=50&offset=0") == before
                panel.get_by_role("button",name="Xem trước (Dry-run)").click()
                expect(panel.get_by_text("Plan digest:")).to_be_visible(timeout=20000)
                acknowledge = panel.get_by_text("Tôi xác nhận thay đổi menu/options có hiệu lực công khai.")
                if acknowledge.is_visible():
                    acknowledge.locator("xpath=..//input").check()
                panel.get_by_role("button",name="Thực thi nhập mẫu").click()
                expect(panel.get_by_text("Đã nhập:")).to_be_visible(timeout=20000)
                pages = api_json(ctx,"GET",base,"/api/admin/content-types/page/entries?limit=50&offset=0")
                assert sum(item["slug"]=="f02-starter" for item in pages)==1
                assert api_json(ctx,"GET",base,f"/api/admin/content-types/post/entries/{existing['id']}")["slug"] == "f02-keep"
                panel.get_by_role("button",name="Xem trước (Dry-run)").click()
                expect(panel.get_by_text("Plan digest:")).to_be_visible(timeout=20000)
                panel.get_by_role("button",name="Thực thi nhập mẫu").click()
                expect(panel.get_by_text("Đã nhập:")).to_be_visible(timeout=20000)
                assert sum(item["slug"]=="f02-starter" for item in api_json(ctx,"GET",base,"/api/admin/content-types/page/entries?limit=50&offset=0")) == 1
                menu_rows = api_json(ctx,"GET",base,"/api/admin/menus?limit=50")["items"]
                starter_menu = next(row for row in menu_rows if row["menu_key"]=="f02_starter_nav")
                starter_menu_id = starter_menu["id"]
                options = api_json(ctx,"GET",base,"/api/themes/f02-editorial/options")
                assert options["values"]["studio_eyebrow"]=="F02 Sample Brand"
                # Save an actual authored layout override before A→B.
                header_open = api_json(ctx,"GET",base,"/api/admin/builder/overrides/header")
                document = {"schema_version":1,"sections":[{
                    "id":"sec-f02","children":[{"id":"row-f02","children":[{
                        "id":"col-f02","children":[{
                            "id":"heading-f02","type":"core/heading",
                            "props":{"level":2,"text":"F02 Authored Header"},
                        }],
                    }]}],
                }]}
                api_json(ctx,"PUT",base,"/api/admin/builder/overrides/header",document,csrf,
                         {"If-Match":f'"v{header_open["version"]}"'})
                saved_header = api_json(ctx,"GET",base,"/api/admin/builder/overrides/header")
                assert saved_header["document"]["sections"][0]["children"][0]["children"][0]["children"][0]["props"]["text"]=="F02 Authored Header"
                checks.append("sample_dryrun_cancel_execute_idempotent_nonempty_site_and_menu_options")
                checks.append("authored_header_override_saved")

                # Stale tab keeps its checked revision after another Admin switches.
                other = ctx.new_page()
                other.on("pageerror", lambda e: errors.append(str(e)))
                other.goto(base+"/admin#/appearance/themes")
                other_panel = other.get_by_test_id("themes-panel")
                expect(other_panel).to_be_visible()
                pick(other_panel,rev2["revision_id"])
                pick(panel,rev2["revision_id"])
                panel.get_by_role("button",name="Kích hoạt revision đã chọn").click()
                expect(panel.locator(f'[data-theme-revision-id="{rev2["revision_id"]}"]')).to_contain_text("Đang kích hoạt")
                other_panel.get_by_role("button",name="Kích hoạt revision đã chọn").click()
                expect(other_panel.get_by_role("alert")).to_contain_text("412",timeout=20000)
                assert other_panel.locator(f'[data-theme-revision-id="{rev2["revision_id"]}"] input').is_checked()
                other.close()
                checks.append("stale_412_preserves_chosen_revision")

                # RBAC/unauth negative; corrupt marker fails closed and active remains.
                fresh = play.request.new_context()
                assert fresh.get(base+"/api/admin/themes").status == 401
                fresh.dispose()
                api_json(ctx,"POST",base,"/api/admin/users",{"username":"editor","password":"F02-EditorPassword123!","role":"editor"},csrf,expected=201)
                editor = play.request.new_context()
                assert editor.post(base+"/auth/login",data=json.dumps({"username":"editor","password":"F02-EditorPassword123!"}),headers={"Origin":base,"Content-Type":"application/json"}).status == 200
                assert editor.get(base+"/api/admin/themes").status == 403
                editor.dispose()
                marker = root / "themes" / "revisions" / rev1["revision_digest"] / ".complete"
                marker_data = marker.read_bytes()
                marker.unlink()
                try:
                    current_selection = api_json(ctx,"GET",base,"/api/admin/themes")["selection"]
                    blocked = ctx.request.put(base+"/api/admin/themes/active", headers={
                        "Origin":base,"X-CSRF-Token":csrf,
                        "Content-Type":"application/json",
                        "If-Match":f'"v{current_selection["version"]}"',
                    }, data=json.dumps({"revision_id":rev1["revision_id"]}))
                    assert blocked.status == 409, (blocked.status,blocked.text())
                    pick(panel,rev1["revision_id"])
                    panel.get_by_role("button",name="Kích hoạt revision đã chọn").click()
                    expect(panel.get_by_role("alert")).to_contain_text("Revision trên đĩa",timeout=20000)
                    assert api_json(ctx,"GET",base,"/api/admin/themes")["selection"]["revision_id"] == rev2["revision_id"]
                finally:
                    marker.write_bytes(marker_data)
                checks.append("401_403_corrupt_digest_409_no_selection_mutation")

                # A2→B switch retains site records; A1/A2 package identity remains stable.
                use_upload(panel,"secondary.zip",zb)
                b = next(r for r in api_json(ctx,"GET",base,"/api/admin/themes")["revisions"] if r["theme_id"]=="f02-secondary")
                pick(panel,b["revision_id"])
                panel.get_by_role("button",name="Kích hoạt revision đã chọn").click()
                expect(panel.get_by_test_id("active-theme")).to_contain_text("f02-secondary (v2)")
                assert api_json(ctx,"GET",base,f"/api/admin/content-types/post/entries/{existing['id']}")["slug"] == "f02-keep"
                assert sum(item["slug"]=="f02-starter" for item in api_json(ctx,"GET",base,"/api/admin/content-types/page/entries?limit=50&offset=0")) == 1
                assert any(row["id"]==starter_menu_id and row["menu_key"]=="f02_starter_nav"
                           for row in api_json(ctx,"GET",base,"/api/admin/menus?limit=50")["items"])
                assert api_json(ctx,"GET",base,"/api/themes/f02-editorial/options")["values"]["studio_eyebrow"]=="F02 Sample Brand"
                assert api_json(ctx,"GET",base,"/api/admin/builder/overrides/header")["document"]["sections"][0]["children"][0]["children"][0]["children"][0]["props"]["text"]=="F02 Authored Header"
                checks.append("switch_A2_to_B_preserves_content_menu_options_layout")
                page.set_viewport_size({"width":390,"height":844})
                page.goto(base+"/admin#/appearance/themes")
                expect(panel).to_be_visible()
                assert page.evaluate("document.documentElement.scrollWidth <= document.documentElement.clientWidth + 1"), "390px horizontal overflow"
                reload_button=panel.get_by_role("button",name="Tải lại danh sách")
                reload_button.focus()
                assert reload_button.evaluate("el => document.activeElement === el")
                checks.append("mobile_390px_keyboard_accessible")
                assert not errors,errors
                ctx.close()
                browser.close()

            stop_server(server)
            server = None
            restarted,restart_base = launch_server(binary,root)
            server = restarted
            with sync_playwright() as p:
                browser = p.chromium.launch(channel="chrome",headless=True)
                ctx = browser.new_context()
                admin_login(ctx,restart_base,"admin","F02-AdminPassword123!")
                summary = api_json(ctx,"GET",restart_base,"/api/admin/themes")
                assert summary["selection"]["revision_id"]==b["revision_id"]
                assert len([r for r in summary["revisions"] if r["source_format"]=="v2"])==3
                assert api_json(ctx,"GET",restart_base,f"/api/admin/content-types/post/entries/{existing['id']}")["slug"]=="f02-keep"
                assert any(row["id"]==starter_menu_id for row in api_json(ctx,"GET",restart_base,"/api/admin/menus?limit=50")["items"])
                assert api_json(ctx,"GET",restart_base,"/api/themes/f02-editorial/options")["values"]["studio_eyebrow"]=="F02 Sample Brand"
                assert api_json(ctx,"GET",restart_base,"/api/admin/builder/overrides/header")["document"]["sections"][0]["children"][0]["children"][0]["children"][0]["props"]["text"]=="F02 Authored Header"
                pg=ctx.new_page();pg.goto(restart_base+"/admin#/appearance/themes")
                expect(pg.get_by_test_id("active-theme")).to_contain_text("f02-secondary (v2)")
                assert pg.get_by_test_id("theme-v2-revisions").locator("[data-theme-revision-id]").count()==3
                ctx.close();browser.close()
            checks.append("restart_accurate_v2_list_selection_content_menu_options_layout")
            db = sqlite3.connect(root/"database/opendash.sqlite")
            try:
                assert db.execute("PRAGMA integrity_check").fetchone()==("ok",)
                assert db.execute("PRAGMA foreign_key_check").fetchall()==[]
            finally:
                db.close()
            checks.append("sqlite_integrity_foreign_keys")

            result = {
                "result":"PASS", "task":"M5-S11-T01", "source_sha":source_sha,
                "binary_sha256":binary_hash,"chrome_version":browser_version,
                "zip_sha256":{"v2_a1":hashlib.sha256(z1).hexdigest(),"v2_a2":hashlib.sha256(z2).hexdigest(),"v2_b":hashlib.sha256(zb).hexdigest()},
                "checks":checks,"revisions":{"a1":rev1["revision_id"],"a2":rev2["revision_id"],"b":b["revision_id"]},
                "browser_page_errors":errors,"data":"isolated SQLite; no production site",
            }
            (artifacts/"m5_beta_theme_v2_receipt.json").write_text(json.dumps(result,indent=2,ensure_ascii=False),encoding="utf8")
            print(json.dumps(result,ensure_ascii=False),flush=True)
            return 0
        finally:
            if server:
                stop_server(server)


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary",required=True,type=pathlib.Path)
    parser.add_argument("--artifact-dir",required=True,type=pathlib.Path)
    args=parser.parse_args()
    try:
        sys.exit(run(args.binary.resolve(strict=True),args.artifact_dir.resolve()))
    except Exception as error:
        print(f"M5-S11-T01 FAIL: {type(error).__name__}: {error}",file=sys.stderr,flush=True)
        raise
