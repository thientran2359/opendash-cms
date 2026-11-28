#!/usr/bin/env python3
"""Playwright Chrome E2E acceptance suite for Admin Sidebar IA & Routing Overhaul (CUJ 1-5)."""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import platform
import subprocess
import tempfile
import traceback

from playwright.sync_api import expect, sync_playwright
from e2e_helpers import admin_login, api_json, launch_server, set_cms_locale, stop_server


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(binary: pathlib.Path, artifacts: pathlib.Path, checks: list[str]) -> dict:
    parent_dir = artifacts.parent.resolve()
    with tempfile.TemporaryDirectory(prefix="sidebar-ia-e2e-", dir=str(parent_dir)) as temporary:
        root = pathlib.Path(temporary).resolve() / "site"
        root.mkdir(mode=0o700)
        server, base = launch_server(binary, root)
        try:
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(channel="chrome", headless=True)
                try:
                    errors: list[str] = []
                    ctx = browser.new_context(viewport={"width": 1440, "height": 900})
                    page = ctx.new_page()
                    page.on("pageerror", lambda err: errors.append(str(err)))

                    admin_pw = "Sidebar-Admin-Pass-12345!"
                    editor_pw = "Sidebar-Editor-Pass-12345!"
                    api_json(
                        ctx,
                        "POST",
                        base,
                        "/auth/setup",
                        {"username": "ia_admin", "password": admin_pw},
                        expected=201,
                    )
                    csrf = admin_login(ctx, base, "ia_admin", admin_pw, cms_locale="en")
                    api_json(
                        ctx,
                        "POST",
                        base,
                        "/api/admin/users",
                        {"username": "ia_editor", "password": editor_pw, "role": "editor"},
                        csrf,
                        expected=201,
                    )
                    # Create a custom CPT with built-in category and tag taxonomies to test plural labels.
                    created_cpt = api_json(
                        ctx,
                        "POST",
                        base,
                        "/api/admin/content-types",
                        {
                            "key": "project",
                            "label": "Projects",
                        },
                        csrf,
                        expected=201,
                    )
                    cpt_ver = created_cpt["version"]
                    api_json(
                        ctx,
                        "PUT",
                        base,
                        "/api/admin/taxonomy/eligibility/project",
                        {"taxonomies": ["category", "tag"], "expected_version": cpt_ver},
                        csrf,
                        {"If-Match": f'"v{cpt_ver}"'},
                        expected=200,
                    )

                    # Login through UI so AdminShell mounts authenticated session cleanly.
                    page.goto(base + "/admin/#/dashboard", wait_until="domcontentloaded")
                    expect(page.get_by_test_id("admin-layout")).to_be_visible(timeout=20000)

                    nav = page.locator("#admin-navigation nav")

                    # Verify reorganized Admin sidebar groups, ordering, and CPT plurals.
                    menu_ids = nav.locator("[data-menu-id]").evaluate_all(
                        "els => els.map(el => el.getAttribute('data-menu-id'))"
                    )
                    assert menu_ids == [
                        "dashboard",
                        "posts",
                        "pages",
                        "cpt:project",
                        "media",
                        "contact",
                        "themes",
                        "studio",
                        "patterns",
                        "schema",
                        "plugins",
                        "users",
                        "tools",
                        "settings",
                    ], menu_ids

                    nav.locator('button[data-menu-id="cpt:project"]').click()
                    expect(nav.locator('a[href="#/cpt/project/taxonomy/category"]')).to_have_text("Categories")
                    expect(nav.locator('a[href="#/cpt/project/taxonomy/tag"]')).to_have_text("Tags")
                    checks.append("admin_sidebar_order_and_cpt_plurals")

                    # CUJ 1: Pure accordion expand without navigating + Themes Install vs Installed + Template Library under Patterns.
                    themes_btn = nav.locator('button[data-menu-id="themes"]')
                    themes_btn.click()
                    expect(themes_btn).to_have_attribute("aria-expanded", "true")
                    expect(page).to_have_url(base + "/admin/#/dashboard")

                    nav.locator('a[href="#/appearance/themes/install"]').click()
                    expect(page).to_have_url(base + "/admin/#/appearance/themes/install")
                    expect(page.get_by_test_id("themes-upload")).to_be_visible(timeout=15000)
                    expect(page.get_by_test_id("themes-installed-link")).to_be_visible()

                    page.get_by_test_id("themes-installed-link").click()
                    expect(page).to_have_url(base + "/admin/#/appearance/themes")
                    expect(page.get_by_test_id("themes-upload")).to_have_count(0)
                    expect(page.get_by_test_id("themes-install-link")).to_be_visible()

                    nav.locator('button[data-menu-id="patterns"]').click()
                    nav.locator('a[href="#/appearance/library"]').click()
                    expect(page).to_have_url(base + "/admin/#/appearance/library")
                    expect(page.get_by_test_id("local-template-library")).to_be_visible(timeout=15000)
                    expect(nav.locator('button[data-menu-id="patterns"]')).to_have_attribute(
                        "class", __import__("re").compile(r"od-nav-link-active")
                    )
                    expect(page.get_by_test_id("breadcrumb")).to_have_text(
                        "Administration / Appearance / Patterns / Template library"
                    )
                    checks.append("cuj1_themes_install_vs_installed_and_patterns_library")

                    # CUJ 2: Customize Studio -> Menus submenu, Contact canonical/alias + bidirectional SMTP, SEO -> Redirects, Media -> Reconcile.
                    nav.locator('button[data-menu-id="studio"]').click()
                    menus_link = nav.locator('a[href="#/studio/menus"]')
                    expect(menus_link).to_be_visible()
                    menus_link.click()
                    expect(page).to_have_url(base + "/admin/#/studio/menus")
                    expect(page.get_by_test_id("studio-panel-menus")).to_be_visible(timeout=20000)

                    page.goto(base + "/admin/#/settings/contact", wait_until="domcontentloaded")
                    expect(page).to_have_url(base + "/admin/#/contact", timeout=15000)
                    expect(page.get_by_test_id("contact-smtp-link")).to_be_visible(timeout=15000)
                    page.get_by_test_id("contact-smtp-link").click()
                    expect(page).to_have_url(base + "/admin/#/settings/smtp")
                    expect(page.get_by_test_id("smtp-contact-link")).to_be_visible(timeout=15000)
                    page.get_by_test_id("smtp-contact-link").click()
                    expect(page).to_have_url(base + "/admin/#/contact")

                    page.goto(base + "/admin/#/contact/submissions", wait_until="domcontentloaded")
                    expect(page.locator("#contact-panel-inbox")).to_be_visible(timeout=15000)

                    page.goto(base + "/admin/#/settings/seo", wait_until="domcontentloaded")
                    expect(page.get_by_test_id("breadcrumb")).to_have_text(
                        "Administration / System / Settings / Homepage SEO"
                    )
                    expect(page.get_by_test_id("seo-redirects-link")).to_be_visible(timeout=15000)
                    page.get_by_test_id("seo-redirects-link").click()
                    expect(page).to_have_url(base + "/admin/#/tools/redirects")

                    nav.locator('a[href="#/media"]').click()
                    expect(page).to_have_url(base + "/admin/#/media")
                    expect(page.get_by_test_id("media-reconcile-link")).to_be_visible(timeout=15000)
                    page.screenshot(path=str(artifacts / "admin-sidebar-en.png"), full_page=False)
                    checks.append("cuj2_menus_contact_smtp_seo_media_crosslinks")

                    # CUJ 3: Pure accordion while post draft is dirty + collapsing active parent + Ctrl/Cmd-Click popup.
                    nav.locator('button[data-menu-id="posts"]').click()
                    nav.locator('a[href="#/posts/new"]').click()
                    title_input = page.locator("#entry-title-input")
                    expect(title_input).to_be_visible(timeout=20000)
                    title_input.fill("Unsaved Accordion Draft")
                    dialog_fired = []
                    def reject_dialog(dialog):
                        dialog_fired.append(dialog.message)
                        dialog.dismiss()
                    page.on("dialog", reject_dialog)
                    plugins_btn = nav.locator('button[data-menu-id="plugins"]')
                    plugins_btn.click()
                    expect(plugins_btn).to_have_attribute("aria-expanded", "true")
                    assert not dialog_fired, f"Accordion expand must not trigger leave dialog: {dialog_fired}"
                    # Active Posts parent can be collapsed cleanly.
                    posts_btn = nav.locator('button[data-menu-id="posts"]')
                    expect(posts_btn).to_have_attribute("aria-expanded", "true")
                    posts_btn.click()
                    expect(posts_btn).to_have_attribute("aria-expanded", "false")
                    # Ctrl/Meta-click on leaf link opens a new tab without leaving dirty post.
                    modifier = "Meta" if platform.system() == "Darwin" else "Control"
                    with ctx.expect_page() as popup_info:
                        nav.locator('a[href="#/media"]').click(modifiers=[modifier])
                    popup_page = popup_info.value
                    popup_page.wait_for_load_state("domcontentloaded")
                    assert popup_page.url.endswith("/admin/#/media"), popup_page.url
                    popup_page.close()
                    assert "#/posts/new" in page.url
                    page.remove_listener("dialog", reject_dialog)
                    checks.append("cuj3_pure_accordion_and_modifier_click")

                    # CUJ 5: Content structure canonical routes, aliases, and Vietnamese l10n.
                    page.once("dialog", lambda d: d.accept())
                    nav.locator('a[href="#/dashboard"]').click()
                    expect(page).to_have_url(base + "/admin/#/dashboard", timeout=15000)

                    nav.locator('button[data-menu-id="schema"]').click()
                    nav.locator('a[href="#/content-structure/content-types"]').click()
                    expect(page).to_have_url(__import__("re").compile(r"#/content-structure/content-types"))
                    nav.locator('a[href="#/content-structure/taxonomies"]').click()
                    expect(page).to_have_url(base + "/admin/#/content-structure/taxonomies")
                    page.goto(base + "/admin/#/taxonomy/definitions", wait_until="domcontentloaded")
                    expect(page).to_have_url(base + "/admin/#/taxonomy/definitions")
                    page.goto(base + "/admin/#/content", wait_until="domcontentloaded")
                    expect(page).to_have_url(__import__("re").compile(r"#/content-structure/content-types"))

                    # Verify Topbar + Sidebar Footer Profile links for Admin.
                    page.get_by_test_id("topbar-profile-link").click()
                    expect(page).to_have_url(base + "/admin/#/users/profile")
                    expect(page.get_by_test_id("user-profile-panel")).to_be_visible(timeout=15000)
                    expect(page.get_by_test_id("breadcrumb")).to_have_text("Administration / My profile")

                    # Switch CMS locale to Vietnamese and verify translated sidebar & new labels.
                    set_cms_locale(ctx, base, csrf, "vi")
                    page.reload(wait_until="domcontentloaded")
                    studio_btn = nav.locator('button[data-menu-id="studio"]')
                    expect(studio_btn).to_contain_text("Tùy chỉnh Studio", timeout=15000)
                    studio_btn.click()
                    expect(nav.locator('a[href="#/studio/menus"]')).to_have_text("Menus", timeout=15000)
                    nav.locator('button[data-menu-id="themes"]').click()
                    expect(nav.locator('a[href="#/appearance/themes/install"]')).to_have_text("Cài theme")
                    nav.locator('button[data-menu-id="patterns"]').click()
                    expect(nav.locator('a[href="#/appearance/library"]')).to_have_text("Thư viện mẫu")
                    nav.locator('button[data-menu-id="settings"]').click()
                    expect(nav.locator('a[href="#/settings/seo"]')).to_have_text("SEO trang chủ")
                    page.screenshot(path=str(artifacts / "admin-sidebar-vi.png"), full_page=False)
                    set_cms_locale(ctx, base, csrf, "en")
                    checks.append("cuj5_content_structure_and_vi_l10n")

                    # CUJ 4: Editor clean sidebar (no SYSTEM/APPEARANCE), mobile drawer profile, and 403 guards.
                    editor_ctx = browser.new_context(viewport={"width": 1440, "height": 900})
                    editor_page = editor_ctx.new_page()
                    editor_page.on("pageerror", lambda err: errors.append(str(err)))
                    api_json(
                        editor_ctx,
                        "POST",
                        base,
                        "/auth/login",
                        {"username": "ia_editor", "password": editor_pw},
                    )
                    editor_page.goto(base + "/admin/#/dashboard", wait_until="domcontentloaded")
                    ed_nav = editor_page.locator("#admin-navigation nav")
                    expect(ed_nav.locator('[data-menu-id="dashboard"]')).to_be_visible(timeout=20000)
                    expect(ed_nav.locator('[data-nav-group="appearance"]')).to_have_count(0)
                    expect(ed_nav.locator('[data-nav-group="system"]')).to_have_count(0)
                    expect(ed_nav.locator('[data-menu-id="contact"]')).to_have_count(0)
                    expect(ed_nav.locator('[data-menu-id="users"]')).to_have_count(0)

                    # Editor Media library does NOT show Admin-only Media reconciliation link.
                    ed_nav.locator('a[href="#/media"]').click()
                    expect(editor_page.get_by_test_id("media-reconcile-link")).to_have_count(0)

                    # Editor Taxonomy page does NOT show Admin-only Taxonomy definitions button.
                    editor_page.goto(base + "/admin/#/posts/categories", wait_until="domcontentloaded")
                    expect(editor_page.get_by_test_id("taxonomy-page")).to_be_visible(timeout=15000)
                    expect(editor_page.get_by_role("button", name="Taxonomy definitions")).to_have_count(0)

                    # Editor Topbar profile works cleanly.
                    editor_page.get_by_test_id("topbar-profile-link").click()
                    expect(editor_page).to_have_url(base + "/admin/#/users/profile")
                    expect(editor_page.get_by_test_id("breadcrumb")).to_have_text("Administration / My profile")

                    # Editor blocked on all Admin-only routes.
                    for forbidden_hash in (
                        "#/contact",
                        "#/appearance/themes/install",
                        "#/appearance/library",
                        "#/content-structure/content-types",
                        "#/content-structure/taxonomies",
                        "#/users",
                        "#/tools/redirects",
                        "#/settings/general",
                    ):
                        editor_page.goto(base + "/admin/" + forbidden_hash, wait_until="domcontentloaded")
                        expect(editor_page.get_by_test_id("rbac-forbidden")).to_be_visible(timeout=15000)

                    # Mobile 390x844 drawer profile flow for Editor.
                    editor_page.set_viewport_size({"width": 390, "height": 844})
                    editor_page.goto(base + "/admin/#/dashboard", wait_until="domcontentloaded")
                    expect(editor_page.get_by_test_id("topbar-profile-link")).to_be_hidden()
                    editor_page.get_by_role("button", name="Open menu").click()
                    sidebar_profile = editor_page.get_by_test_id("sidebar-profile-link")
                    expect(sidebar_profile).to_be_visible(timeout=10000)
                    sidebar_profile.click()
                    expect(editor_page).to_have_url(base + "/admin/#/users/profile")
                    expect(editor_page.get_by_test_id("user-profile-panel")).to_be_visible(timeout=15000)
                    editor_ctx.close()
                    checks.append("cuj4_editor_clean_sidebar_mobile_profile_and_403")

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
        "task": "admin_sidebar_ia_e2e",
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
