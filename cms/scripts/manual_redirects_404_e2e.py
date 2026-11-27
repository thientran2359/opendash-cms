#!/usr/bin/env python3
"""Playwright E2E acceptance suite for Manual Redirects (301/302), CSV Import/Export, and 404 Log."""
from __future__ import annotations

import argparse
import hashlib
import http.client
import json
import pathlib
import platform
import subprocess
import tempfile
import traceback
from urllib.parse import urlparse

from playwright.sync_api import expect, sync_playwright
from menu_picker_e2e import admin_login, api_json, launch_server, set_cms_locale, stop_server


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def raw_http_get(base: str, path: str) -> tuple[int, dict[str, str], bytes]:
    parsed = urlparse(base)
    conn = http.client.HTTPConnection(parsed.hostname, parsed.port, timeout=10)
    try:
        conn.request("GET", path)
        resp = conn.getresponse()
        body = resp.read()
        headers = {k.lower(): v for k, v in resp.getheaders()}
        return resp.status, headers, body
    finally:
        conn.close()


def run(binary: pathlib.Path, artifacts: pathlib.Path, checks: list[str]) -> dict:
    parent_dir = artifacts.parent.resolve()
    with tempfile.TemporaryDirectory(prefix="redirects-404-e2e-", dir=str(parent_dir)) as temporary:
        root = pathlib.Path(temporary).resolve() / "site"
        root.mkdir(mode=0o700)
        server, base = launch_server(binary, root)
        try:
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(channel="chrome", headless=True)
                try:
                    errors: list[str] = []
                    ctx = browser.new_context(
                        viewport={"width": 1440, "height": 900},
                        accept_downloads=True,
                    )
                    page = ctx.new_page()
                    page.on("pageerror", lambda err: errors.append(str(err)))

                    admin_pw = "Redirects-Admin-Pass-12345!"
                    editor_pw = "Redirects-Editor-Pass-12345!"
                    api_json(
                        ctx,
                        "POST",
                        base,
                        "/auth/setup",
                        {"username": "redir_admin", "password": admin_pw},
                        expected=201,
                    )
                    csrf = admin_login(ctx, base, "redir_admin", admin_pw, cms_locale="en")
                    api_json(
                        ctx,
                        "POST",
                        base,
                        "/api/admin/users",
                        {"username": "redir_editor", "password": editor_pw, "role": "editor"},
                        csrf,
                        expected=201,
                    )

                    live_page = api_json(
                        ctx,
                        "POST",
                        base,
                        "/api/admin/content-types/page/entries",
                        {"slug": "live-page", "title": "Live Page", "values": {"body": "Live"}},
                        csrf,
                        expected=201,
                    )
                    api_json(
                        ctx,
                        "POST",
                        base,
                        f"/api/admin/content-types/page/entries/{live_page['id']}/publication",
                        {"status": "published"},
                        csrf,
                        {"If-Match": f'"v{live_page["version"]}"'},
                        expected=200,
                    )
                    api_json(
                        ctx,
                        "POST",
                        base,
                        "/api/admin/content-types/page/entries",
                        {"slug": "draft-page", "title": "Draft Page", "values": {"body": "Draft"}},
                        csrf,
                        expected=201,
                    )

                    # CUJ 1: Sidebar Tools navigation, CRUD, shadowed warning, filters, public precedence
                    page.goto(base + "/admin/#/dashboard", wait_until="domcontentloaded")
                    nav = page.get_by_role("navigation", name="Admin sections")
                    tools_btn = nav.get_by_role("button", name="Tools")
                    expect(tools_btn).to_be_visible(timeout=20000)
                    tools_btn.click()
                    expect(nav.locator('a[href="#/tools/redirects"]')).to_be_visible(timeout=20000)
                    nav.locator('a[href="#/tools/redirects"]').click()
                    expect(page.get_by_test_id("redirects-tools-panel")).to_be_visible(timeout=20000)
                    expect(page.get_by_test_id("manual-redirects-panel")).to_be_visible()

                    for src, tgt, status, note in [
                        ("/Lien_He.html/", "/live-page", "301", "legacy"),
                        ("/live-page", "/should-not-win", "301", "shadowed check"),
                        ("/draft-page", "/live-page", "301", "draft allows manual redirect"),
                    ]:
                        page.get_by_test_id("redirect-add-btn").click()
                        expect(page.get_by_test_id("redirect-form-modal")).to_be_visible()
                        page.get_by_test_id("redirect-source-input").fill(src)
                        page.get_by_test_id("redirect-target-input").fill(tgt)
                        page.get_by_test_id("redirect-status-select").select_option(status)
                        page.get_by_test_id("redirect-note-input").fill(note)
                        page.get_by_test_id("redirect-save-btn").click()
                        expect(page.get_by_test_id("redirect-form-modal")).to_have_count(0, timeout=15000)

                    expect(page.locator("tr", has_text="/Lien_He.html")).to_be_visible()
                    expect(page.get_by_test_id("redirects-shadowed-banner")).to_be_visible()
                    page.screenshot(path=str(artifacts / "redirects-list-en.png"), full_page=True)

                    code, hdrs, _ = raw_http_get(base, "/Lien_He.html/?utm_source=fb")
                    assert code == 301, (code, hdrs)
                    assert hdrs.get("location") == "/live-page?utm_source=fb", hdrs
                    assert "no-store" in hdrs.get("cache-control", ""), hdrs

                    live_code, _, _ = raw_http_get(base, "/live-page")
                    assert live_code == 200, live_code
                    draft_code, draft_hdrs, _ = raw_http_get(base, "/draft-page")
                    assert draft_code == 301 and draft_hdrs.get("location") == "/live-page", (
                        draft_code,
                        draft_hdrs,
                    )

                    page.get_by_test_id("redirects-state-filter").select_option("shadowed")
                    expect(page.locator("tr", has_text="/Lien_He.html")).to_have_count(0, timeout=15000)
                    expect(page.locator("tr", has_text="/should-not-win")).to_be_visible()
                    page.get_by_test_id("redirects-state-filter").select_option("")
                    expect(page.locator("tr", has_text="/Lien_He.html")).to_be_visible(timeout=15000)
                    page.get_by_test_id("redirects-search-input").fill("Lien_He")
                    expect(page.locator("tr", has_text="/should-not-win")).to_have_count(0, timeout=15000)
                    expect(page.locator("tr", has_text="/Lien_He.html")).to_be_visible()
                    page.get_by_test_id("redirects-search-input").fill("")
                    expect(page.locator("tr", has_text="/should-not-win")).to_be_visible(timeout=15000)
                    checks.append("cuj1_crud_filters_shadowed_and_public_precedence")

                    # CUJ 2: CSV Dry-Run, commit, and UTF-8 BOM formula-safe export
                    page.get_by_test_id("redirect-import-btn").click()
                    expect(page.get_by_test_id("csv-import-modal")).to_be_visible()
                    bad_csv = (
                        "source_path,target_url,status_code,enabled,note\n"
                        '/csv-e2e,/live-page,301,true,"=1+1"\n'
                        "/Lien_He.html,/other,301,true,dup\n"
                        "/bad,javascript:alert(1),301,true,bad\n"
                    )
                    page.get_by_test_id("csv-textarea").fill(bad_csv)
                    page.get_by_test_id("csv-dry-run-btn").click()
                    expect(page.get_by_test_id("csv-preview-summary")).to_be_visible(timeout=15000)
                    expect(page.get_by_test_id("csv-preview-summary")).to_contain_text("Errors: 1")
                    expect(page.get_by_test_id("csv-commit-btn")).to_be_disabled()

                    good_csv = (
                        "source_path,target_url,status_code,enabled,note\n"
                        '/csv-e2e,/live-page,301,true,"=1+1"\n'
                        "/Lien_He.html,/other,301,true,dup\n"
                    )
                    page.get_by_test_id("csv-textarea").fill(good_csv)
                    page.get_by_test_id("csv-conflict-policy").select_option("skip")
                    page.get_by_test_id("csv-dry-run-btn").click()
                    expect(page.get_by_test_id("csv-preview-summary")).to_contain_text("Create: 1")
                    expect(page.get_by_test_id("csv-commit-btn")).to_be_enabled()
                    page.get_by_test_id("csv-commit-btn").click()
                    expect(page.get_by_test_id("csv-import-modal")).to_have_count(0, timeout=15000)
                    expect(page.locator("tr", has_text="/csv-e2e")).to_be_visible()

                    with page.expect_download() as dl_info:
                        page.get_by_test_id("redirect-export-btn").click()
                    dl_path = dl_info.value.path()
                    csv_bytes = pathlib.Path(dl_path).read_bytes()
                    assert csv_bytes.startswith(b"\xef\xbb\xbf"), csv_bytes[:16]
                    csv_text = csv_bytes.decode("utf-8")
                    assert "'=1+1" in csv_text, csv_text
                    checks.append("cuj2_csv_dry_run_commit_and_bom_export")

                    # CUJ 3: 404 Log, #/tools/404-log alias, 1-click redirect, bulk delete, clear
                    for path, expected_status in [
                        ("/missing-e2e-1", 404),
                        ("/missing-e2e-1", 404),
                        ("/missing-e2e-2", 404),
                        ("/missing-e2e-3", 404),
                        ("/Admin/wp-login.php", 404),
                        ("/%61dmin/x", 400),
                    ]:
                        status, _, _ = raw_http_get(base, path)
                        assert status == expected_status, (path, status)

                    page.goto(base + "/admin/#/tools/404-log", wait_until="domcontentloaded")
                    expect(page.get_by_test_id("not-found-log-panel")).to_be_visible(timeout=15000)
                    row1 = page.locator("tr", has_text="/missing-e2e-1")
                    expect(row1).to_be_visible()
                    expect(row1).to_contain_text("2")
                    expect(page.locator("tr", has_text="/Admin/wp-login.php")).to_have_count(0)
                    page.screenshot(path=str(artifacts / "not-found-log-en.png"), full_page=True)

                    row1.get_by_test_id("not-found-create-redirect-btn").click()
                    expect(page.get_by_test_id("redirect-form-modal")).to_be_visible()
                    expect(page).to_have_url(base + "/admin/#/tools/404-log")
                    expect(page.get_by_test_id("redirect-source-input")).to_have_value("/missing-e2e-1")
                    page.get_by_test_id("redirect-target-input").fill("/live-page")
                    page.get_by_test_id("redirect-save-btn").click()
                    expect(page.get_by_test_id("redirect-form-modal")).to_have_count(0, timeout=15000)
                    expect(page.locator("tr", has_text="/missing-e2e-1")).to_have_count(0)
                    one_click_code, one_click_hdrs, _ = raw_http_get(base, "/missing-e2e-1")
                    assert one_click_code == 301 and one_click_hdrs.get("location") == "/live-page"

                    page.locator("tr", has_text="/missing-e2e-2").locator('input[type="checkbox"]').check()
                    page.once("dialog", lambda d: d.accept())
                    page.get_by_test_id("not-found-bulk-delete-btn").click()
                    expect(page.locator("tr", has_text="/missing-e2e-2")).to_have_count(0, timeout=15000)

                    page.get_by_test_id("not-found-clear-btn").click()
                    page.once("dialog", lambda d: d.accept())
                    page.get_by_role("button", name="Clear all 404 logs").click()
                    expect(page.locator("tr", has_text="/missing-e2e-3")).to_have_count(0, timeout=15000)
                    checks.append("cuj3_404_telemetry_one_click_bulk_delete_and_clear")

                    # CUJ 5: 412 conflict rebase, 401 draft recovery, vi + mobile, Editor RBAC
                    page.goto(base + "/admin/#/tools/redirects", wait_until="domcontentloaded")
                    expect(page.locator("tr", has_text="/csv-e2e")).to_be_visible()
                    list_data = api_json(ctx, "GET", base, "/api/admin/redirects?q=csv-e2e")
                    csv_rule = list_data["items"][0]
                    page.locator("tr", has_text="/csv-e2e").get_by_role("button", name="Edit").click()
                    expect(page.get_by_test_id("redirect-form-modal")).to_be_visible()
                    page.get_by_test_id("redirect-target-input").fill("/stale-target")

                    api_json(
                        ctx,
                        "PUT",
                        base,
                        f"/api/admin/redirects/{csv_rule['id']}",
                        {
                            "source_path": csv_rule["source_path"],
                            "target_url": "/rebased-target",
                            "status_code": 302,
                            "enabled": True,
                            "note": "concurrent update",
                        },
                        csrf,
                        {"If-Match": f'"v{csv_rule["version"]}"'},
                        expected=200,
                    )
                    page.get_by_test_id("redirect-save-btn").click()
                    expect(page.get_by_test_id("redirect-rebase-btn")).to_be_visible(timeout=15000)
                    expect(page.get_by_test_id("redirect-save-btn")).to_be_disabled()
                    page.get_by_test_id("redirect-rebase-btn").click()
                    expect(page.get_by_test_id("redirect-target-input")).to_have_value("/rebased-target")
                    page.get_by_test_id("redirect-form-modal").get_by_role("button", name="Cancel").first.click()
                    expect(page.get_by_test_id("redirect-form-modal")).to_have_count(0)

                    # 401 draft recovery via cleared session cookie
                    page.get_by_test_id("redirect-add-btn").click()
                    page.get_by_test_id("redirect-source-input").fill("/unsaved-401")
                    page.get_by_test_id("redirect-target-input").fill("/target-401")
                    ctx.clear_cookies()
                    page.get_by_test_id("redirect-save-btn").click()
                    login_user = page.locator("#auth-main #auth-username")
                    expect(login_user).to_be_visible(timeout=15000)
                    login_user.fill("redir_admin")
                    page.locator("#auth-main #auth-password").fill(admin_pw)
                    page.locator("#auth-main #auth-login").click()
                    expect(page.get_by_test_id("redirect-form-modal")).to_be_visible(timeout=20000)
                    expect(page.get_by_test_id("redirect-source-input")).to_have_value("/unsaved-401")
                    expect(page.get_by_test_id("redirect-target-input")).to_have_value("/target-401")
                    page.get_by_test_id("redirect-form-modal").get_by_role("button", name="Cancel").first.click()
                    checks.append("cuj5_412_rebase_and_401_draft_recovery")

                    # Refresh CSRF after UI re-login and test Vietnamese + 390px mobile
                    csrf = admin_login(ctx, base, "redir_admin", admin_pw, cms_locale="vi")
                    page.set_viewport_size({"width": 390, "height": 844})
                    page.reload(wait_until="domcontentloaded")
                    expect(
                        page.get_by_test_id("redirects-tools-panel").get_by_role(
                            "heading", name="Chuyển hướng & 404"
                        )
                    ).to_be_visible(timeout=20000)
                    page.screenshot(path=str(artifacts / "redirects-vi-mobile.png"), full_page=True)
                    page.goto(base + "/admin/#/tools/404-log", wait_until="domcontentloaded")
                    expect(page.get_by_test_id("not-found-log-panel")).to_be_visible(timeout=15000)
                    expect(page.get_by_test_id("not-found-clear-btn")).to_contain_text("Dọn nhật ký")
                    checks.append("cuj5_vi_locale_and_390px_mobile")

                    # Editor RBAC: UI forbidden & API 403
                    editor_ctx = browser.new_context(viewport={"width": 1280, "height": 800})
                    admin_login(editor_ctx, base, "redir_editor", editor_pw)
                    editor_page = editor_ctx.new_page()
                    for h in ("#/tools/redirects", "#/tools/404-log"):
                        editor_page.goto(base + "/admin/" + h, wait_until="domcontentloaded")
                        expect(editor_page.get_by_test_id("rbac-forbidden")).to_be_visible(timeout=15000)
                    for api_path in ("/api/admin/redirects", "/api/admin/404-logs"):
                        res = editor_ctx.request.get(base + api_path)
                        assert res.status == 403, (api_path, res.status)
                    editor_ctx.close()
                    checks.append("cuj5_editor_rbac_forbidden_and_api_403")

                    assert not errors, errors
                    checks.append("chrome_page_errors_none")
                    return {"chrome_version": browser.version}
                finally:
                    browser.close()
        finally:
            stop_server(server)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--artifact-dir", type=pathlib.Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    artifacts = args.artifact_dir.resolve()
    artifacts.mkdir(mode=0o700, parents=True, exist_ok=True)
    checks: list[str] = []
    result = {
        "task": "manual_redirects_404_e2e",
        "source_sha": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        "binary_sha256": digest(binary),
        "platform": platform.platform(),
        "checks": checks,
        "status": "FAIL",
    }
    try:
        result.update(run(binary, artifacts, checks))
        result["status"] = "PASS"
        return 0
    except Exception as error:
        result["error"] = str(error)
        result["traceback"] = traceback.format_exc(limit=6)
        return 1
    finally:
        result["screenshots_sha256"] = {
            path.name: digest(path) for path in sorted(artifacts.glob("*.png"))
        }
        (artifacts / "receipt.json").write_text(
            json.dumps(result, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
        )
        print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    raise SystemExit(main())
