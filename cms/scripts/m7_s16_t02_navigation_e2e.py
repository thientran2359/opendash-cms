#!/usr/bin/env python3
"""M7-S16-T02 Admin grouped routes and one-screen-per-destination, native Chrome.

Only disposable site roots under an explicit private artifact parent; never the
owner's already-running review site. Preserve baseline failures as receipts.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import platform
import re
import subprocess
import tempfile
import traceback

from playwright.sync_api import expect, sync_playwright
from m7_beta_menu_picker_e2e import admin_login, api_json, launch_server, stop_server, theme_zip


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


# hash, expected feature, unrelated feature that must not be rendered.
ADMIN_ROUTES = [
    ("#/appearance/menus", '[data-testid="studio-panel-menus"]', '[data-testid="themes-panel"]'),
    ("#/appearance/themes", '[data-testid="themes-panel"]', '[data-testid="studio-panel-menus"]'),
    ("#/appearance/options", '[data-testid="studio-panel-theme-options"]', '[data-testid="themes-panel"]'),
    ("#/appearance/content-types", '[data-testid="content-type-schema-settings"]', '[data-testid="themes-panel"]'),
    ("#/settings/content-types", '[data-testid="content-type-schema-settings"]', '[data-testid="themes-panel"]'),
    ("#/appearance/library", '[data-testid="local-template-library"]', '[data-testid="themes-panel"]'),
    ("#/sections", '[data-testid="pattern-workspace"]', '[data-testid="entry-list-post"]'),
    ("#/sections/new", '[data-testid="pattern-workspace"]', '#entry-title-input'),
    ("#/plugins", "#plugin-list-heading", "#plugin-upload-heading"),
    ("#/plugins/install", "#plugin-upload-heading", "#plugin-list-heading"),
    ("#/users", ".od-users-table", 'button:has-text("Create account")'),
    ("#/users/new", 'button:has-text("Create account")', ".od-users-table"),
    ("#/tools/backup", '[aria-label="Encrypted backup"]', "h2:has-text('Storage reconciliation & quarantine')"),
    ("#/tools/media-reconcile", "h2:has-text('Storage reconciliation & quarantine')", '[aria-label="Encrypted backup"]'),
    ("#/tools/health", "h2:has-text('System status')", '[aria-label="Encrypted backup"]'),
    ("#/settings/general", '[data-testid="general-settings"]', '[aria-label="Homepage SEO"]'),
    ("#/settings/reading", '[data-testid="reading-settings"]', '[data-testid="general-settings"]'),
    ("#/settings/multilingual", '[data-testid="multilingual-enabled-checkbox"]', '[data-testid="general-settings"]'),
    ("#/settings/seo", '[aria-label="Homepage SEO"]', '[data-testid="general-settings"]'),
    ("#/settings/contact", "#contact-forms-panel", "#smtp-settings-panel"),
    ("#/settings/smtp", "#smtp-settings-panel", "#contact-forms-panel"),
    ("#/contact/submissions", "#contact-forms-panel", "#smtp-settings-panel"),
    ("#/users/profile", '[data-testid="user-profile-panel"]', '[data-testid="general-settings"]'),
]


def run(binary: pathlib.Path, artifacts: pathlib.Path, checks: list[str]) -> dict:
    with tempfile.TemporaryDirectory(prefix="m7-s16-t02-", dir=str(artifacts.parent)) as temporary:
        root = pathlib.Path(temporary) / "site"
        root.mkdir(mode=0o700)
        server, base = launch_server(binary, root)
        try:
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(channel="chrome", headless=True)
                try:
                    context = browser.new_context(viewport={"width": 1440, "height": 900})
                    page = context.new_page()
                    errors: list[str] = []
                    page.on("pageerror", lambda err: errors.append(str(err)))
                    api_json(context, "POST", base, "/auth/setup", {
                        "username": "nav_admin", "password": "Navigation-Local-Test-12345!",
                    }, expected=201)
                    csrf = admin_login(context, base, "nav_admin", "Navigation-Local-Test-12345!")
                    api_json(context, "POST", base, "/api/admin/users", {
                        "username": "nav_editor", "password": "Navigation-Editor-Test-12345!", "role": "editor",
                    }, csrf, expected=201)
                    page.goto(base + "/admin/#/appearance/menus", wait_until="domcontentloaded")
                    expect(page.get_by_test_id("studio-panel-menus")).to_be_visible(timeout=20000)
                    page.screenshot(path=str(artifacts / "baseline-menus.png"), full_page=True)
                    expect(page.get_by_test_id("themes-panel")).to_have_count(0)
                    checks.append("menus_without_theme_install_or_options")
                    expect(page).to_have_url(base + "/admin/#/studio?panel=menus")
                    expect(page.get_by_test_id("admin-layout")).not_to_be_visible()
                    page.goto(base + "/admin/#/dashboard", wait_until="domcontentloaded")
                    nav = page.get_by_role("navigation", name="Admin sections")
                    expect(nav).to_be_visible()
                    groups = [node.inner_text().casefold() for node in nav.locator(".od-nav-group-title").all()]
                    assert groups == ["content", "appearance", "system"], groups
                    expect(nav.locator('[data-menu-id="patterns"]')).to_be_enabled()
                    expect(nav.locator('[data-menu-id="studio"]')).to_be_enabled()
                    expect(nav.locator('[data-menu-id="themes"]')).to_be_enabled()
                    assert nav.locator('[data-menu-id="sections"]').count() == 0
                    checks.append("three_groups_current_patterns_studio_and_themes_enabled")

                    for index, (hashroute, visible, absent) in enumerate(ADMIN_ROUTES, start=1):
                        page.goto(base + "/admin/" + hashroute, wait_until="domcontentloaded")
                        expect(page.locator(visible).first).to_be_visible(timeout=20000)
                        unrelated = page.locator(absent)
                        if unrelated.count() > 0:
                            expect(unrelated.first).not_to_be_visible()
                        if hashroute in ("#/appearance/menus", "#/appearance/options"):
                            panel = "menus" if hashroute.endswith("menus") else "theme-options"
                            expect(page).to_have_url(base + "/admin/#/studio?panel=" + panel)
                            expect(page.get_by_test_id("admin-layout")).not_to_be_visible()
                        else:
                            expect(page.get_by_test_id("breadcrumb")).to_contain_text("Administration")
                        page.reload(wait_until="domcontentloaded")
                        expect(page.locator(visible).first).to_be_visible(timeout=20000)
                        checks.append("route:" + hashroute)
                        if hashroute in ("#/appearance/menus", "#/appearance/options", "#/tools/health",
                                         "#/settings/contact", "#/plugins/install", "#/users/new"):
                            page.screenshot(path=str(artifacts / f"destination-{index:02d}.png"), full_page=True)
                    checks.append(f"{len(ADMIN_ROUTES)}_distinct_destinations_deeplink_reload_one_feature")
                    # Click through the actual sidebar and verify selected leaf,
                    # breadcrumb, Back, and Forward stay aligned.
                    page.goto(base + "/admin/#/dashboard", wait_until="domcontentloaded")
                    nav.locator('[data-menu-id="themes"]').click()
                    expect(page).to_have_url(base + "/admin/#/appearance/themes")
                    child = nav.locator('a[href="#/appearance/library"]')
                    expect(child).to_be_visible()
                    child.click()
                    expect(page).to_have_url(base + "/admin/#/appearance/library")
                    expect(page.get_by_test_id("local-template-library")).to_be_visible()
                    expect(child).to_have_attribute("aria-current", "page")
                    expect(page.get_by_test_id("breadcrumb")).to_contain_text("Install theme")
                    page.go_back(wait_until="domcontentloaded")
                    expect(page.get_by_test_id("themes-panel")).to_be_visible()
                    expect(nav.locator('a[href="#/appearance/themes"]')).to_have_attribute("aria-current", "page")
                    page.go_forward(wait_until="domcontentloaded")
                    expect(page.get_by_test_id("local-template-library")).to_be_visible()
                    checks.append("sidebar_click_back_forward_breadcrumb_active_leaf")

                    page.goto(base + "/admin/#/content", wait_until="domcontentloaded")
                    expect(page.get_by_test_id("content-type-schema-settings")).to_be_visible()
                    expect(page).to_have_url(re.compile(r"#/appearance/content-types(?:/[^/]+)?$"))
                    page.goto(base + "/admin/#/tools/restore", wait_until="domcontentloaded")
                    page.locator("#backup-tools-tab-restore").click()
                    expect(page.get_by_role("heading", name="Restore & safe mode")).to_be_visible()
                    checks.append("legacy_content_and_restore_aliases")

                    for target, heading in (("#/sections/new", "Create pattern"), ("#/sections", "Patterns")):
                        page.goto(base + "/admin/" + target, wait_until="domcontentloaded")
                        expect(page.get_by_test_id("pattern-workspace").get_by_role("heading", name=heading, exact=True)).to_be_visible()
                        expect(page.locator("#builder-main")).not_to_be_visible()
                    checks.append("legacy_section_routes_show_real_pattern_workspace")

                    page.set_viewport_size({"width": 390, "height": 844})
                    page.goto(base + "/admin/#/dashboard", wait_until="domcontentloaded")
                    page.get_by_role("button", name="Open menu").click()
                    for group in ("Content", "Appearance", "System"):
                        expect(nav.locator(".od-nav-group-title").filter(has_text=group)).to_be_visible()
                    page.screenshot(path=str(artifacts / "mobile-three-groups.png"), full_page=True)
                    page.keyboard.press("Escape")
                    expect(page.get_by_role("button", name="Open menu")).to_have_attribute("aria-expanded", "false")
                    checks.append("390px_grouped_drawer_and_escape")
                    page.set_viewport_size({"width": 1440, "height": 900})
                    page.goto(base + "/admin/#/appearance/themes", wait_until="domcontentloaded")
                    theme_panel = page.get_by_test_id("themes-panel")
                    expect(theme_panel).to_be_visible()
                    theme_panel.get_by_test_id("themes-upload").set_input_files({
                        "name": "nav-test-theme.zip", "mimeType": "application/zip",
                        "buffer": theme_zip("nav-test-theme", "1.0.0"),
                    })
                    expect(theme_panel.get_by_role("status").filter(has_text="Installed")).to_be_visible(timeout=25000)
                    theme_panel.locator('[data-theme-revision-id] input[name="selected-theme-revision"]').check()
                    theme_panel.get_by_role("button", name="Activate selected revision").click()
                    expect(theme_panel.get_by_test_id("active-theme")).to_contain_text("nav-test-theme", timeout=20000)
                    assert theme_panel.get_by_test_id("theme-specific-options-section").count() == 0
                    page.goto(base + "/admin/#/appearance/options", wait_until="domcontentloaded")
                    expect(page.get_by_test_id("theme-specific-options-section")).to_be_visible(timeout=20000)
                    expect(page.get_by_test_id("themes-panel")).to_have_count(0)
                    checks.append("installed_theme_options_only_on_separate_options_screen")
                    editor = browser.new_context(viewport={"width": 390, "height": 844})
                    admin_login(editor, base, "nav_editor", "Navigation-Editor-Test-12345!")
                    editor_page = editor.new_page()
                    editor_page.goto(base + "/admin/#/users/profile", wait_until="domcontentloaded")
                    expect(editor_page.get_by_test_id("user-profile-panel")).to_be_visible(timeout=20000)
                    editor_nav = editor_page.get_by_role("navigation", name="Admin sections")
                    for menu_name in ("studio", "themes", "patterns", "schema", "plugins", "settings", "tools"):
                        assert editor_nav.locator(f'[data-menu-id="{menu_name}"]').count() == 0
                    checks.append("editor_profile_and_admin_menu_visibility")
                    for restricted_hash in (
                        "#/appearance/menus", "#/appearance/options", "#/appearance/content-types",
                        "#/settings/content-types", "#/plugins/install", "#/tools/health",
                        "#/users/new", "#/settings/smtp", "#/settings/reading", "#/settings/multilingual",
                    ):
                        editor_page.goto(base + "/admin/" + restricted_hash, wait_until="domcontentloaded")
                        expect(editor_page.get_by_test_id("rbac-forbidden")).to_be_visible(timeout=18000)
                    checks.append("editor_restricted_routes_show_403")
                    editor.close()
                    stop_server(server)
                    restarted, restarted_base = launch_server(binary, root)
                    try:
                        fresh = browser.new_context()
                        admin_login(fresh, restarted_base, "nav_admin", "Navigation-Local-Test-12345!")
                        reloaded = api_json(fresh, "GET", restarted_base, "/api/admin/themes")
                        assert reloaded["selection"]["theme_id"] == "nav-test-theme", reloaded["selection"]
                        fresh_page = fresh.new_page()
                        fresh_page.goto(restarted_base + "/admin/#/appearance/options", wait_until="domcontentloaded")
                        expect(fresh_page.get_by_test_id("theme-specific-options-section")).to_be_visible(timeout=20000)
                        checks.append("restart_persists_active_theme_and_separate_options_route")
                        fresh.close()
                    finally:
                        stop_server(restarted)
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
        "task": "M7-S16-T02",
        "source_sha": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        "binary_sha256": digest(binary),
        "platform": platform.platform(),
        "checks": checks,
        "status": "FAIL",
        "not_run": ["owner localhost:18964", "Windows/Linux", "full Beta/human dogfood"],
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
