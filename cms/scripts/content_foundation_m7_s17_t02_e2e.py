#!/usr/bin/env python3
"""#377 / M7-S17-T02 isolated native macOS Admin permalink/API/Chrome acceptance.

Checks the exact embedded Admin bundle against the provided dist directory before
exercising the UI. Both API and browser fixtures use ephemeral SQLite/loopback.

python3 cms/scripts/content_foundation_m7_s17_t02_e2e.py \
  --binary cms/target/debug/opendash --admin-dist cms/admin/dist \
  --artifact-dir /tmp/t02-isolated-evidence
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import platform
import re
import sqlite3
import subprocess
import tempfile
import traceback

from playwright.sync_api import expect, sync_playwright
from content_foundation_m4_s12_t01_e2e import api, fetch
from content_foundation_m7_s17_t01_e2e import no_redirect, permalink_input
from m7_beta_menu_picker_e2e import admin_login, launch_server, stop_server


def sha(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def assert_row(ctx, base: str, kind: str, entry_id: int, public_url: str | None) -> None:
    rows = api(ctx, base, "GET", f"/api/admin/content-types/{kind}/entry-table")
    entry = next((item for item in rows["items"] if item["id"] == entry_id), None)
    assert entry is not None, f"Admin table missing {kind}/{entry_id}"
    assert entry["public_url"] == public_url, (entry_id, public_url, entry["public_url"])


def run(binary: pathlib.Path, artifacts: pathlib.Path) -> list[str]:
    checks: list[str] = []
    with tempfile.TemporaryDirectory(prefix="t02-native-", dir=artifacts) as temp:
        root = pathlib.Path(temp) / "site"
        root.mkdir(mode=0o700)
        db = root / "database/opendash.sqlite"
        proc, base = launch_server(binary, root)
        assert base.startswith("http://127.0.0.1:") and not base.endswith(":18964")
        try:
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(channel="chrome", headless=True)
                try:
                    ctx = browser.new_context()
                    guest = browser.new_context()
                    api(ctx, base, "POST", "/auth/setup", 201,
                        {"username": "t02_admin", "password": "Native-T02-Isolated-Password!2026"})
                    csrf = admin_login(ctx, base, "t02_admin", "Native-T02-Isolated-Password!2026")

                    fetch(guest, base, "GET", "/api/admin/content-types/post/entry-table", 401)
                    fetch(guest, base, "GET", "/api/admin/settings/permalinks", 401)
                    settings = api(ctx, base, "GET", "/api/admin/settings/permalinks")
                    assert settings["post_structure"] == "/content/post/%postname%"
                    assert isinstance(settings["version"], int)
                    checks.append("authenticated_settings_defaults_and_private_table")

                    post = api(ctx, base, "POST", "/api/admin/content-types/post/entries", 201,
                               {"title": "T02 Live", "slug": "t02-live", "values": {}}, csrf)
                    draft = api(ctx, base, "POST", "/api/admin/content-types/post/entries", 201,
                                {"title": "T02 Private", "slug": "t02-private", "values": {}}, csrf)
                    post = api(ctx, base, "POST",
                               f'/api/admin/content-types/post/entries/{post["id"]}/publication',
                               200, {"status": "published"}, csrf, post["version"])
                    route_path = f'/api/admin/content-types/post/entries/{post["id"]}/route'
                    fetch(guest, base, "GET", route_path, 401)
                    route_resp = fetch(ctx, base, "GET", route_path, 200)
                    route_snapshot = route_resp.json()
                    assert route_resp.headers.get("cache-control") == "no-store"
                    assert route_resp.headers.get("etag") == f'"v{post["version"]}"'
                    assert route_snapshot == {"entry_version": post["version"],
                                              "custom_path": None, "public_url": "/content/post/t02-live"}
                    draft_route = api(ctx, base, "GET",
                                      f'/api/admin/content-types/post/entries/{draft["id"]}/route')
                    assert draft_route["public_url"] is None
                    assert_row(ctx, base, "post", post["id"], "/content/post/t02-live")
                    assert_row(ctx, base, "post", draft["id"], None)
                    preview_path = f'/api/admin/content-types/post/entries/{draft["id"]}/preview'
                    fetch(guest, base, "GET", preview_path, 401)
                    preview = fetch(ctx, base, "GET", preview_path, 200)
                    assert preview.headers.get("cache-control") == "private, no-store"
                    assert "T02 Private" in preview.text()
                    no_redirect(ctx, base, "/content/post/t02-private", 404)
                    checks.append("server_resolved_row_urls_and_authenticated_draft_preview")
                    checks.append("exact_entry_get_matches_version_and_respects_draft_privacy")

                    proposal = permalink_input(settings, post="/blog/%postname%")
                    no_redirect(ctx, base, "/blog/t02-live", 404)
                    no_redirect(ctx, base, "/content/post/t02-live", 200)
                    assert_row(ctx, base, "post", post["id"], "/content/post/t02-live")
                    fetch(ctx, base, "PUT", "/api/admin/settings/permalinks", 428,
                          proposal, csrf)
                    assert api(ctx, base, "GET", "/api/admin/settings/permalinks") == settings
                    saved = api(ctx, base, "PUT", "/api/admin/settings/permalinks", 200,
                                proposal, csrf, settings["version"])
                    assert saved["post_structure"] == "/blog/%postname%"
                    assert saved["version"] == settings["version"] + 1
                    no_redirect(ctx, base, "/blog/t02-live", 200)
                    no_redirect(ctx, base, "/content/post/t02-live", 301, "/blog/t02-live")
                    assert_row(ctx, base, "post", post["id"], "/blog/t02-live")
                    assert_row(ctx, base, "post", draft["id"], None)
                    assert api(ctx, base, "GET", route_path) == {
                        "entry_version": post["version"], "custom_path": None,
                        "public_url": "/blog/t02-live"}
                    no_redirect(ctx, base, "/blog/t02-private", 404)
                    checks.append("not_saved_does_not_change_public_and_saved_updates_table")

                    fetch(ctx, base, "PUT", "/api/admin/settings/permalinks", 412,
                          permalink_input(settings, post="/%postname%"), csrf, settings["version"])
                    assert api(ctx, base, "GET", "/api/admin/settings/permalinks") == saved
                    assert_row(ctx, base, "post", post["id"], "/blog/t02-live")
                    checks.append("stale_cas_412_preserves_settings_and_live_view_url")

                    page = api(ctx, base, "POST", "/api/admin/content-types/page/entries", 201,
                               {"title": "T02 Collision Page", "slug": "page-t02", "values": {}}, csrf)
                    api(ctx, base, "PUT",
                        f'/api/admin/content-types/page/entries/{page["id"]}/route', 200,
                        {"custom_path": "/projects/t02-live"}, csrf, page["version"])
                    collision = fetch(ctx, base, "PUT", "/api/admin/settings/permalinks", 409,
                                      permalink_input(saved, post="/projects/%postname%"),
                                      csrf, saved["version"])
                    assert collision.text(), "collision must be diagnosable"
                    assert api(ctx, base, "GET", "/api/admin/settings/permalinks") == saved
                    assert_row(ctx, base, "post", post["id"], "/blog/t02-live")
                    no_redirect(ctx, base, "/blog/t02-live", 200)
                    no_redirect(ctx, base, "/projects/t02-live", 404)  # draft page
                    checks.append("collision_409_preserves_db_and_published_routes")

                    with sqlite3.connect(db) as conn:
                        assert conn.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
                        assert not conn.execute("PRAGMA foreign_key_check").fetchall()
                        row = conn.execute("SELECT version,post_structure FROM site_permalink_settings WHERE id=1").fetchone()
                        assert row == (saved["version"], saved["post_structure"]), row
                    checks.append("sqlite_integrity_and_atomic_config_after_rejected_saves")
                finally:
                    browser.close()
        finally:
            stop_server(proc)

        proc, base = launch_server(binary, root)
        try:
            # New process, same isolated DB; no manual resolver flush.
            assert no_redirect_raw_status(base, "/blog/t02-live") == 200
            assert no_redirect_raw_status(base, "/content/post/t02-live") == 301
            assert no_redirect_raw_status(base, "/blog/t02-private") == 404
            checks.append("restart_preserves_canonical_and_draft_privacy")
        finally:
            stop_server(proc)
    return checks


def no_redirect_raw_status(base: str, path: str) -> int:
    from content_foundation_m7_s17_t01_e2e import no_redirect_raw
    return no_redirect_raw(base, path)


def verify_embedded_admin(ctx, base: str, admin_dist: pathlib.Path) -> dict[str, str]:
    """Fail if the binary serves any stale/alternate index, JS or CSS assets."""
    index = admin_dist / "index.html"
    expected_index = index.read_bytes()
    response = ctx.request.fetch(base + "/admin/", timeout=20000)
    assert response.status == 200, ("embedded Admin index", response.status)
    assert response.body() == expected_index, "native binary embeds an outdated Admin index"
    assets = sorted(set(re.findall(rb'(?:src|href)="(/assets/[^\"]+)"', expected_index)))
    assert any(path.endswith(b".js") for path in assets), "missing built Admin JavaScript"
    assert any(path.endswith(b".css") for path in assets), "missing built Admin CSS"
    hashes = {"admin/index.html": sha(index)}
    for asset in assets:
        relative = asset.decode("ascii")
        expected = (admin_dist / relative.lstrip("/")).read_bytes()
        served = ctx.request.fetch(base + relative, timeout=20000)
        assert served.status == 200, (relative, served.status)
        assert served.body() == expected, f"binary asset differs from dist: {relative}"
        if relative.endswith(".js"):
            assert b"permalink-settings" in expected, "bundle predates #377 UI"
        hashes[relative] = hashlib.sha256(expected).hexdigest()
    return hashes


def run_admin_ui(binary: pathlib.Path, artifacts: pathlib.Path,
                 admin_dist: pathlib.Path) -> tuple[list[str], dict[str, str]]:
    """Independent site; API suite stays unchanged and keeps its seven checks."""
    checks: list[str] = []
    with tempfile.TemporaryDirectory(prefix="t02-chrome-", dir=artifacts) as temp:
        site = pathlib.Path(temp) / "site"
        site.mkdir(mode=0o700)
        proc, base = launch_server(binary, site)
        assert base.startswith("http://127.0.0.1:") and not base.endswith(":18964")
        try:
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(channel="chrome", headless=True)
                try:
                    ctx = browser.new_context(viewport={"width": 1280, "height": 850})
                    guest = browser.new_context()
                    bundle_hashes = verify_embedded_admin(guest, base, admin_dist)
                    checks.append("embedded_index_js_css_match_prime_dist_byte_for_byte")

                    api(ctx, base, "POST", "/auth/setup", 201,
                        {"username": "t02_ui", "password": "Chrome-T02-Isolated-Password!2026"})
                    csrf = admin_login(ctx, base, "t02_ui", "Chrome-T02-Isolated-Password!2026")
                    post = api(ctx, base, "POST", "/api/admin/content-types/post/entries", 201,
                               {"title": "UI T02 Public", "slug": "ui-live", "values": {}}, csrf)
                    draft = api(ctx, base, "POST", "/api/admin/content-types/post/entries", 201,
                                {"title": "UI T02 Draft", "slug": "ui-private", "values": {}}, csrf)
                    api(ctx, base, "POST",
                        f'/api/admin/content-types/post/entries/{post["id"]}/publication', 200,
                        {"status": "published"}, csrf, post["version"])
                    api(ctx, base, "POST", "/api/admin/content-types", 201,
                        {"key": "ui_books_t02", "label": "UI Books", "fields": []}, csrf)
                    api(ctx, base, "POST", "/api/admin/taxonomy/definitions", 201,
                        {"key": "ui_genre_t02", "label": "UI Genre", "hierarchical": False}, csrf)

                    page = ctx.new_page()
                    try:
                        page.goto(base + "/admin/#/settings/permalinks", wait_until="domcontentloaded")
                        form = page.get_by_test_id("permalink-settings")
                        preview = page.get_by_test_id("permalink-server-preview")
                        preset = page.locator("#permalink-post-preset")
                        save = page.get_by_role("button", name="Lưu permalink")
                        expect(form).to_be_visible(timeout=20000)
                        expect(preset).to_have_value("/content/post/%postname%", timeout=20000)
                        expect(preview.locator('a[href="/content/post/ui-live"]')).to_be_visible(timeout=15000)
                        assert save.is_disabled(), "clean form must not save"
                        page.screenshot(path=str(artifacts / "t02-ui-default.png"), full_page=True)
                        checks.append("native_chrome_admin_form_and_server_owned_preview")

                        # Changing a preset only updates the form; it cannot
                        # cause a live path/redirect or alter a server example.
                        preset.select_option("/blog/%postname%")
                        expect(save).to_be_enabled()
                        expect(preview.locator('a[href="/content/post/ui-live"]')).to_be_visible()
                        no_redirect(ctx, base, "/blog/ui-live", 404)
                        no_redirect(ctx, base, "/content/post/ui-live", 200)
                        assert_row(ctx, base, "post", post["id"], "/content/post/ui-live")
                        page.screenshot(path=str(artifacts / "t02-ui-preset-unsaved.png"), full_page=True)
                        save.click()
                        expect(form.get_by_text("Đã lưu permalink", exact=False)).to_be_visible(timeout=15000)
                        expect(preview.locator('a[href="/blog/ui-live"]')).to_be_visible(timeout=15000)
                        expect(save).to_be_disabled(timeout=15000)
                        no_redirect(ctx, base, "/blog/ui-live", 200)
                        no_redirect(ctx, base, "/content/post/ui-live", 301, "/blog/ui-live")
                        assert_row(ctx, base, "post", post["id"], "/blog/ui-live")
                        checks.append("preset_unchanged_until_save_then_public_and_preview_update")
                        page.screenshot(path=str(artifacts / "t02-ui-preset-saved.png"), full_page=True)

                        # Custom format + CPT/taxonomy bases are all updated
                        # through the Admin form; the server remains authority.
                        preset.select_option("custom")
                        custom = page.locator("#permalink-post-custom")
                        expect(custom).to_be_visible()
                        custom.fill("/stories/%postname%")
                        cpt_base = page.get_by_role("textbox", name="CPT base ui_books_t02")
                        tax_base = page.get_by_role("textbox", name="Taxonomy base ui_genre_t02")
                        expect(cpt_base).to_be_enabled(timeout=15000)
                        expect(tax_base).to_be_enabled(timeout=15000)
                        cpt_base.fill("projects")
                        tax_base.fill("genres")
                        no_redirect(ctx, base, "/stories/ui-live", 404)
                        no_redirect(ctx, base, "/blog/ui-live", 200)
                        expect(preview.locator('a[href="/blog/ui-live"]')).to_be_visible()
                        page.screenshot(path=str(artifacts / "t02-ui-custom-unsaved.png"), full_page=True)
                        save.click()
                        expect(preview.locator('a[href="/stories/ui-live"]')).to_be_visible(timeout=15000)
                        expect(save).to_be_disabled(timeout=15000)
                        committed = api(ctx, base, "GET", "/api/admin/settings/permalinks")
                        assert committed["post_structure"] == "/stories/%postname%"
                        assert committed["content_type_bases"]["ui_books_t02"] == "projects"
                        assert committed["taxonomy_bases"]["ui_genre_t02"] == "genres"
                        assert_row(ctx, base, "post", post["id"], "/stories/ui-live")
                        no_redirect(ctx, base, "/stories/ui-live", 200)
                        no_redirect(ctx, base, "/blog/ui-live", 301, "/stories/ui-live")
                        page.screenshot(path=str(artifacts / "t02-ui-custom-saved.png"), full_page=True)
                        checks.append("custom_post_cpt_taxonomy_form_and_saved_server_paths")

                        # A different entry claims the exact path generated
                        # by a proposed format; the rejected form remains dirty.
                        collision = api(ctx, base, "POST", "/api/admin/content-types/page/entries", 201,
                                        {"title": "UI Collision", "slug": "ui-page", "values": {}}, csrf)
                        api(ctx, base, "PUT",
                            f'/api/admin/content-types/page/entries/{collision["id"]}/route', 200,
                            {"custom_path": "/collide/ui-live"}, csrf, collision["version"])
                        custom.fill("/collide/%postname%")
                        save.click()
                        expect(form.get_by_role("alert")).to_contain_text("409", timeout=15000)
                        expect(custom).to_have_value("/collide/%postname%")
                        expect(save).to_be_enabled()
                        assert api(ctx, base, "GET", "/api/admin/settings/permalinks") == committed
                        expect(preview.locator('a[href="/stories/ui-live"]')).to_be_visible()
                        no_redirect(ctx, base, "/stories/ui-live", 200)
                        page.screenshot(path=str(artifacts / "t02-ui-conflict-409-draft-retained.png"), full_page=True)
                        checks.append("ui_collision_409_keeps_unsaved_input_and_live_url")

                        # A concurrent author changes settings after this page
                        # loaded; the stale save must preserve the local input.
                        custom.fill("/stale/%postname%")
                        remote = api(ctx, base, "PUT", "/api/admin/settings/permalinks", 200,
                                     permalink_input(committed, post="/remote/%postname%"),
                                     csrf, committed["version"])
                        save.click()
                        expect(form.get_by_role("alert")).to_contain_text("412", timeout=15000)
                        expect(custom).to_have_value("/stale/%postname%")
                        expect(form.get_by_text("Cấu hình trên máy chủ mới hơn", exact=False)).to_be_visible()
                        expect(save).to_be_disabled()
                        assert api(ctx, base, "GET", "/api/admin/settings/permalinks") == remote
                        no_redirect(ctx, base, "/remote/ui-live", 200)
                        no_redirect(ctx, base, "/stale/ui-live", 404)
                        page.screenshot(path=str(artifacts / "t02-ui-stale-412-draft-retained.png"), full_page=True)
                        checks.append("ui_stale_412_keeps_input_and_disables_overwrite")

                        # A fresh GET must NOT rebase the pending draft to the
                        # new CAS version; editing again must also stay stale.
                        with page.expect_response(
                            lambda response: response.request.method == "GET"
                            and "/api/admin/settings/permalinks" in response.url,
                            timeout=15000,
                        ) as refreshed:
                            page.get_by_role("button", name="Kiểm tra phiên bản mới").click()
                        assert refreshed.value.status == 200
                        expect(custom).to_have_value("/stale/%postname%", timeout=15000)
                        expect(form.get_by_text("Cấu hình trên máy chủ mới hơn", exact=False)).to_be_visible()
                        custom.fill("/still-stale/%postname%")
                        # A second GET exposes a rebased in-memory CAS version
                        # if changing the stale field accidentally blessed it.
                        with page.expect_response(
                            lambda response: response.request.method == "GET"
                            and "/api/admin/settings/permalinks" in response.url,
                            timeout=15000,
                        ) as refreshed_again:
                            page.get_by_role("button", name="Kiểm tra phiên bản mới").click()
                        assert refreshed_again.value.status == 200
                        expect(custom).to_have_value("/still-stale/%postname%", timeout=15000)
                        expect(form.get_by_text("Cấu hình trên máy chủ mới hơn", exact=False)).to_be_visible()
                        expect(save).to_be_disabled()
                        assert api(ctx, base, "GET", "/api/admin/settings/permalinks") == remote
                        page.screenshot(path=str(artifacts / "t02-ui-stale-recheck-edit.png"), full_page=True)
                        checks.append("ui_stale_check_new_version_and_edit_cannot_bypass_cas")

                        # A direct editor reload must use its own version-matched
                        # server route snapshot, not an entry-table page lookup.
                        published_editor = ctx.new_page()
                        try:
                            published_editor.goto(
                                base + f'/admin/#/posts/{post["id"]}/edit',
                                wait_until="domcontentloaded",
                            )
                            views = published_editor.get_by_role("link", name="Xem bài viết")
                            expect(views.first).to_be_visible(timeout=20000)
                            assert views.count() >= 1
                            assert all(views.nth(index).get_attribute("href") == "/remote/ui-live"
                                       for index in range(views.count()))
                            with published_editor.expect_popup(timeout=15000) as opened:
                                views.first.click()
                            expect(opened.value).to_have_url(
                                re.compile(r"/remote/ui-live$"), timeout=15000,
                            )
                            opened.value.close()
                            published_editor.screenshot(
                                path=str(artifacts / "t02-ui-published-editor-canonical.png"),
                                full_page=True,
                            )
                            checks.append("chrome_direct_published_editor_view_uses_canonical_route")
                        finally:
                            published_editor.close()

                        # Native Chrome verifies editor's authenticated preview
                        # target and a guest's inability to use the same URL.
                        editor = ctx.new_page()
                        try:
                            editor.goto(base + f'/admin/#/posts/{draft["id"]}/edit', wait_until="domcontentloaded")
                            link = editor.get_by_role("link", name=re.compile("Xem trước bản nháp đã lưu"))
                            expect(link).to_be_visible(timeout=20000)
                            preview_path = f'/api/admin/content-types/post/entries/{draft["id"]}/preview'
                            assert link.get_attribute("href") == preview_path
                            assert guest.request.fetch(base + preview_path).status == 401
                            with editor.expect_popup(timeout=15000) as opened:
                                link.click()
                            popup = opened.value
                            popup.wait_for_load_state("domcontentloaded")
                            assert "UI T02 Draft" in popup.locator("body").inner_text(timeout=15000)
                            popup.screenshot(path=str(artifacts / "t02-ui-authenticated-draft-preview.png"), full_page=True)
                            popup.close()
                            no_redirect(ctx, base, "/remote/ui-private", 404)
                            checks.append("chrome_editor_draft_preview_authenticated_and_public_404")
                        finally:
                            editor.close()

                        page.set_viewport_size({"width": 390, "height": 844})
                        expect(form).to_be_visible()
                        expect(custom).to_have_value("/still-stale/%postname%")
                        # Wait for the responsive drawer's CSS transition to
                        # finish; otherwise a resized desktop screenshot can
                        # catch the sidebar midway through sliding offscreen.
                        page.wait_for_function(
                            "() => document.querySelector('#admin-navigation').getBoundingClientRect().right <= 1",
                            timeout=10000,
                        )
                        assert page.evaluate("document.documentElement.scrollWidth <= window.innerWidth + 1"), \
                            "390px Admin page overflows horizontally"
                        page.screenshot(path=str(artifacts / "t02-ui-390px-stale.png"), full_page=True)
                        checks.append("chrome_390px_stale_form_remains_accessible")
                    except Exception:
                        if not page.is_closed():
                            page.screenshot(path=str(artifacts / "t02-ui-failure.png"), full_page=True)
                        raise
                    finally:
                        page.close()
                        guest.close()
                        ctx.close()
                finally:
                    browser.close()
        finally:
            stop_server(proc)
    return checks, bundle_hashes


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=pathlib.Path)
    parser.add_argument("--admin-dist", type=pathlib.Path,
                        help="Exact built Admin dist directory required for byte-for-byte browser acceptance")
    parser.add_argument("--artifact-dir", required=True, type=pathlib.Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    artifacts = args.artifact_dir.resolve()
    artifacts.mkdir(parents=True, exist_ok=True)
    source_root = pathlib.Path(__file__).resolve().parents[2]
    receipt = {
        "task": "M7-S17-T02", "suite": "API/native regression + Chrome Admin UI",
        "source_sha": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source_root, text=True).strip(),
        "dirty_paths": subprocess.check_output(["git", "status", "--porcelain"], cwd=source_root, text=True).splitlines(),
        "binary_sha256": sha(binary), "runner_sha256": sha(pathlib.Path(__file__).resolve()),
        "platform": platform.platform(), "architecture": platform.machine(), "port_18964_used": False,
    }
    try:
        receipt["checks"] = run(binary, artifacts)
        if args.admin_dist:
            admin_dist = args.admin_dist.resolve(strict=True)
            receipt["admin_dist_source"] = str(admin_dist)
            receipt["ui_checks"], receipt["embedded_admin_sha256"] = run_admin_ui(
                binary, artifacts, admin_dist)
        else:
            receipt["ui_checks"] = []
            receipt["ui_skipped_reason"] = "--admin-dist not provided; API suite only"
        receipt["result"] = "PASS"
    except Exception as error:
        receipt.update({"result": "FAIL", "error": str(error), "traceback": traceback.format_exc()})
    receipt["screenshot_sha256"] = {path.name: sha(path) for path in sorted(artifacts.glob("*.png"))}
    (artifacts / "receipt.json").write_text(json.dumps(receipt, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps(receipt, indent=2, ensure_ascii=False))
    if receipt["result"] != "PASS":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
