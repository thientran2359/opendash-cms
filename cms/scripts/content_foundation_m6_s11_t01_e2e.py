#!/usr/bin/env python3
"""C4 #378: native Chrome/Admin + API/SQLite provider declaration acceptance.

Run with an isolated compiled binary and exact Admin dist; no live-site data:
  python3 cms/scripts/content_foundation_m6_s11_t01_e2e.py \
    --binary cms/target/debug/opendash --admin-dist cms/admin/dist \
    --artifact-dir /private/tmp/c4-native-evidence
"""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import pathlib
import platform
import sqlite3
import tempfile
import traceback
import zipfile
from concurrent.futures import ThreadPoolExecutor
from threading import Barrier
from urllib.error import HTTPError
from urllib.request import Request, urlopen

from playwright.sync_api import expect, sync_playwright
from m7_beta_menu_picker_e2e import admin_login, launch_server, stop_server
from content_foundation_m4_s12_t01_e2e import api, fetch
from content_foundation_m7_s17_t01_e2e import no_redirect
from content_foundation_m7_s17_t02_e2e import verify_embedded_admin


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def provider_zip(version: str, cpt_base: str, taxonomy_base: str) -> bytes:
    fixture = pathlib.Path(__file__).resolve().parents[1] / "examples/plugins/sample-plugin/sample-plugin.zip"
    with zipfile.ZipFile(fixture) as old:
        wasm = old.read("plugin.wasm")
        manifest = json.loads(old.read("manifest.json"))
    manifest.update({
        "id": "browser-c4-provider",
        "name": "Browser C4 Provider",
        "version": version,
        "manifest_schema": 2,
        "declarations": {
            "taxonomy_definitions": [{
                "key": "genre_browser_c4", "label": "Genre Browser C4",
                "hierarchical": False, "public": True, "rewrite_base": taxonomy_base,
            }],
            "content_types": [{
                "key": "listing_browser_c4", "label": "Listing Browser C4",
                "fields": [], "taxonomies": ["genre_browser_c4"],
                "rewrite_base": cpt_base,
                "settings": {"public": True, "has_archive": True,
                             "publication": "draft_publish", "supports": ["editor"]},
            }],
        },
    })
    out = io.BytesIO()
    with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as zip_file:
        zip_file.writestr("manifest.json", json.dumps(manifest, sort_keys=True))
        zip_file.writestr("plugin.wasm", wasm)
    return out.getvalue()


def install(ctx, base: str, csrf: str, data: bytes, status: int = 201):
    response = ctx.request.fetch(base + "/api/admin/plugins", method="POST", timeout=30000,
                                 headers={"Content-Type": "application/zip",
                                          "X-CSRF-Token": csrf, "Origin": base}, data=data)
    assert response.status == status, (response.status, response.text()[:400])
    return response.json() if response.body() else {}


def full_permalink_input(snapshot: dict) -> dict:
    """The C3 PUT is a full-form replacement, never a JSON merge-patch.

    Copy both *sparse* site-choice maps verbatim. Missing keys inherit the
    current active provider defaults; explicit null opts out to legacy URLs.
    """
    return {
        "post_structure": snapshot["post_structure"],
        "content_type_bases": dict(snapshot["content_type_bases"]),
        "taxonomy_bases": dict(snapshot["taxonomy_bases"]),
    }


def run(binary: pathlib.Path, dist: pathlib.Path, artifacts: pathlib.Path):
    checks: list[str] = []
    shots: dict[str, str] = {}
    with tempfile.TemporaryDirectory(prefix="c4-native-", dir=artifacts) as temporary:
        root = pathlib.Path(temporary) / "site"
        root.mkdir(mode=0o700)
        proc, base = launch_server(binary, root)
        assert base.startswith("http://127.0.0.1:") and not base.endswith(":18964")
        try:
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(channel="chrome", headless=True)
                try:
                    ctx = browser.new_context(viewport={"width": 1280, "height": 900})
                    guest = browser.new_context()
                    verify_embedded_admin(guest, base, dist)
                    checks.append("embedded_admin_bundle_exact_bytes")
                    api(ctx, base, "POST", "/auth/setup", 201,
                        {"username": "c4_browser", "password": "C4-Browser-Isolated-Password!2026"})
                    csrf = admin_login(ctx, base, "c4_browser", "C4-Browser-Isolated-Password!2026")
                    original = api(ctx, base, "GET", "/api/admin/settings/permalinks")
                    assert original["content_type_bases"] == {}
                    assert original["taxonomy_bases"] == {}
                    install(ctx, base, csrf, provider_zip("1.0.0", "listings-browser", "genres-browser"))
                    settings = api(ctx, base, "GET", "/api/admin/settings/permalinks")
                    assert settings["version"] > original["version"]
                    assert settings["content_type_bases"] == {}
                    assert settings["taxonomy_bases"] == {}
                    checks.append("provider_defaults_do_not_occupy_site_choice_maps")

                    # Even a full-form C3 PUT with unchanged values must NOT
                    # materialize package rewrite defaults into site choices.
                    # ETag/version may advance for a successful full PUT.
                    full_form = api(ctx, base, "PUT", "/api/admin/settings/permalinks", 200,
                                    full_permalink_input(settings), csrf, settings["version"])
                    assert full_form["version"] == settings["version"] + 1, full_form
                    assert full_form["content_type_bases"] == {}
                    assert full_form["taxonomy_bases"] == {}
                    assert api(ctx, base, "GET", "/api/admin/settings/permalinks") == full_form
                    checks.append("provider_active_full_form_noop_put_keeps_site_maps_sparse")

                    taxonomy = api(ctx, base, "GET", "/api/admin/taxonomy/definitions")
                    match = next(tax for tax in taxonomy if tax["key"] == "genre_browser_c4")
                    assert match["owner"] == {"plugin": "browser-c4-provider"} or match["owner"] == {
                        "kind": "plugin", "id": "browser-c4-provider"
                    }, match
                    term = api(ctx, base, "POST", "/api/admin/taxonomy/terms", 201,
                               {"kind": "genre_browser_c4", "name": "Browser fiction",
                                "slug": "fiction-browser"}, csrf)
                    draft = api(ctx, base, "POST",
                                "/api/admin/content-types/listing_browser_c4/entries", 201,
                                {"title": "C4 browser entry", "slug": "browser-entry",
                                 "values": {}, "term_ids": [term["id"]]}, csrf)
                    assert draft["publication_status"] == "draft"
                    no_redirect(ctx, base, "/listings-browser/browser-entry", 404)
                    published = api(ctx, base, "POST",
                        f"/api/admin/content-types/listing_browser_c4/entries/{draft['id']}/publication",
                        200, {"status": "published"}, csrf, draft["version"])
                    assert published["publication_status"] == "published"
                    no_redirect(ctx, base, "/listings-browser/browser-entry", 200)
                    no_redirect(ctx, base, "/genres-browser/fiction-browser", 200)
                    checks.append("custom_cpt_term_and_draft_publish_canonical_provider_urls")

                    # A site-owned CPT/taxonomy has no provider rewrite
                    # declaration. Missing map keys must use legacy routes.
                    local_type = api(ctx, base, "POST", "/api/admin/content-types", 201,
                                     {"key": "site_only_c4", "label": "Site Only C4",
                                      "fields": []}, csrf)
                    api(ctx, base, "POST", "/api/admin/taxonomy/definitions", 201,
                        {"key": "topic_site_c4", "label": "Site Topic C4",
                         "hierarchical": False, "public": True}, csrf)
                    local_term = api(ctx, base, "POST", "/api/admin/taxonomy/terms", 201,
                                     {"kind": "topic_site_c4", "name": "Local Topic",
                                      "slug": "local-topic"}, csrf)
                    api(ctx, base, "PUT", "/api/admin/taxonomy/eligibility/site_only_c4", 200,
                        {"taxonomies": ["topic_site_c4"],
                         "expected_version": local_type["version"]},
                        csrf, local_type["version"])
                    api(ctx, base, "POST", "/api/admin/content-types/site_only_c4/entries", 201,
                        {"title": "Local entry", "slug": "local-entry", "values": {},
                         "term_ids": [local_term["id"]]}, csrf)
                    no_redirect(ctx, base, "/content/site_only_c4/local-entry", 200)
                    no_redirect(ctx, base, "/taxonomy/topic_site_c4/local-topic", 200)
                    checks.append("absent_provider_default_falls_back_to_legacy_cpt_and_taxonomy_urls")

                    page = ctx.new_page()
                    page.goto(base + "/admin/#/taxonomy/definitions", wait_until="domcontentloaded")
                    expect(page.get_by_text("Genre Browser C4").first).to_be_visible(timeout=20000)
                    image_file = artifacts / "c4-taxonomy-browser.png"
                    page.screenshot(path=str(image_file), full_page=True)
                    shots[image_file.name] = digest(image_file)
                    page.goto(base + "/admin/#/settings/permalinks", wait_until="domcontentloaded")
                    form = page.get_by_test_id("permalink-settings")
                    expect(form).to_be_visible(timeout=20000)
                    cpt_base = page.get_by_role("textbox", name="CPT base listing_browser_c4")
                    tax_base = page.get_by_role("textbox", name="Taxonomy base genre_browser_c4")
                    cpt_mode = page.get_by_test_id("cpt-base-mode-listing_browser_c4")
                    tax_mode = page.get_by_test_id("taxonomy-base-mode-genre_browser_c4")
                    cpt_legacy = page.get_by_role("button", name="Dùng đường dẫn legacy cho CPT listing_browser_c4")
                    tax_legacy = page.get_by_role("button", name="Dùng đường dẫn legacy cho Taxonomy genre_browser_c4")
                    cpt_inherit = page.get_by_role("button", name="Kế thừa mặc định cho CPT listing_browser_c4")
                    tax_inherit = page.get_by_role("button", name="Kế thừa mặc định cho Taxonomy genre_browser_c4")
                    expect(cpt_base).to_have_value("")
                    expect(tax_base).to_have_value("")
                    expect(cpt_mode).to_contain_text("Đang kế thừa")
                    expect(tax_mode).to_contain_text("Đang kế thừa")
                    expect(page.get_by_test_id("cpt-base-mode-site_only_c4")).to_contain_text("Đang kế thừa")
                    expect(page.get_by_test_id("taxonomy-base-mode-topic_site_c4")).to_contain_text("Đang kế thừa")
                    assert cpt_inherit.is_disabled() and tax_inherit.is_disabled()
                    save = page.get_by_role("button", name="Lưu permalink")
                    assert save.is_disabled()
                    checks.append("native_chrome_c4_inherited_modes_and_sparse_settings_form")
                    image_file = artifacts / "c4-permalink-default.png"
                    page.screenshot(path=str(image_file), full_page=True)
                    shots[image_file.name] = digest(image_file)

                    # Two explicit actions write JSON null, with no change to
                    # the public site until the complete draft is saved.
                    cpt_legacy.click()
                    tax_legacy.click()
                    expect(cpt_mode).to_contain_text("Đang chọn đường dẫn legacy")
                    expect(tax_mode).to_contain_text("Đang chọn đường dẫn legacy")
                    no_redirect(ctx, base, "/listings-browser/browser-entry", 200)
                    no_redirect(ctx, base, "/genres-browser/fiction-browser", 200)
                    assert api(ctx, base, "GET", "/api/admin/settings/permalinks") == full_form
                    expect(save).to_be_enabled()
                    save.click()
                    expect(save).to_be_disabled(timeout=20000)
                    explicit_null = api(ctx, base, "GET", "/api/admin/settings/permalinks")
                    assert explicit_null["content_type_bases"]["listing_browser_c4"] is None
                    assert explicit_null["taxonomy_bases"]["genre_browser_c4"] is None
                    expect(cpt_mode).to_contain_text("Đang chọn đường dẫn legacy")
                    expect(tax_mode).to_contain_text("Đang chọn đường dẫn legacy")
                    no_redirect(ctx, base, "/content/listing_browser_c4/browser-entry", 200)
                    no_redirect(ctx, base, "/taxonomy/genre_browser_c4/fiction-browser", 200)
                    no_redirect(ctx, base, "/listings-browser/browser-entry", 301,
                                "/content/listing_browser_c4/browser-entry")
                    checks.append("admin_explicit_null_overrides_provider_defaults_and_redirects")
                    image_file = artifacts / "c4-explicit-null-saved.png"
                    page.screenshot(path=str(image_file), full_page=True)
                    shots[image_file.name] = digest(image_file)
                    page.set_viewport_size({"width": 390, "height": 844})
                    expect(form).to_be_visible()
                    expect(cpt_inherit).to_be_visible()
                    expect(tax_inherit).to_be_visible()
                    image_file = artifacts / "c4-permalink-mobile-390.png"
                    page.screenshot(path=str(image_file), full_page=True)
                    shots[image_file.name] = digest(image_file)
                    page.set_viewport_size({"width": 1280, "height": 900})
                    checks.append("native_chrome_responsive_mobile_390_permalink")

                    updated_plugin = install(ctx, base, csrf,
                                             provider_zip("1.0.1", "new-listings-c4", "new-genres-c4"))
                    unchanged = api(ctx, base, "GET", "/api/admin/settings/permalinks")
                    assert unchanged == explicit_null, "provider default changed site override/version"
                    no_redirect(ctx, base, "/content/listing_browser_c4/browser-entry", 200)
                    checks.append("same_owner_update_preserves_explicit_site_choice")
                    disable = ctx.request.fetch(
                        base + "/api/admin/plugins/browser-c4-provider/disable",
                        method="POST", timeout=30000,
                        headers={"X-CSRF-Token": csrf, "Origin": base,
                                 "If-Match": str(updated_plugin["cas_version"])},
                    )
                    assert disable.status == 200, (disable.status, disable.text())
                    disabled = disable.json()
                    no_redirect(ctx, base, "/content/listing_browser_c4/browser-entry", 404)
                    retained = api(ctx, base, "GET",
                        f"/api/admin/content-types/listing_browser_c4/entries/{draft['id']}")
                    assert retained["term_ids"] == [term["id"]]
                    assert api(ctx, base, "GET", "/api/admin/settings/permalinks") == unchanged
                    enable = ctx.request.fetch(
                        base + "/api/admin/plugins/browser-c4-provider/enable",
                        method="POST", timeout=30000,
                        headers={"X-CSRF-Token": csrf, "Origin": base,
                                 "If-Match": str(disabled["cas_version"])},
                    )
                    assert enable.status == 200, (enable.status, enable.text())
                    no_redirect(ctx, base, "/content/listing_browser_c4/browser-entry", 200)
                    assert api(ctx, base, "GET", "/api/admin/settings/permalinks") == unchanged
                    checks.append("disable_reenable_preserves_admin_data_and_site_override")
                    with sqlite3.connect(root / "database/opendash.sqlite") as connection:
                        assert connection.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
                        assert not connection.execute("PRAGMA foreign_key_check").fetchall()
                        owner = connection.execute(
                            "SELECT owner_kind,owner_id,rewrite_base FROM provider_rewrite_defaults "
                            "WHERE resource_kind='content_type' AND resource_key='listing_browser_c4'"
                        ).fetchone()
                        assert owner == ("plugin", "browser-c4-provider", "new-listings-c4"), owner
                    checks.append("sqlite_integrity_owner_and_default_provenance")

                    # UI buttons remove the keys from cloned draft maps. Before
                    # Save, legacy remains the canonical route.
                    cpt_inherit.click()
                    tax_inherit.click()
                    expect(cpt_mode).to_contain_text("Đang kế thừa")
                    expect(tax_mode).to_contain_text("Đang kế thừa")
                    no_redirect(ctx, base, "/content/listing_browser_c4/browser-entry", 200)
                    no_redirect(ctx, base, "/taxonomy/genre_browser_c4/fiction-browser", 200)
                    assert api(ctx, base, "GET", "/api/admin/settings/permalinks") == unchanged
                    save.click()
                    expect(save).to_be_disabled(timeout=20000)
                    inherited = api(ctx, base, "GET", "/api/admin/settings/permalinks")
                    assert inherited["version"] == unchanged["version"] + 1
                    assert inherited["content_type_bases"] == {}
                    assert inherited["taxonomy_bases"] == {}
                    page.reload(wait_until="domcontentloaded")
                    expect(cpt_mode).to_contain_text("Đang kế thừa", timeout=20000)
                    expect(tax_mode).to_contain_text("Đang kế thừa")
                    no_redirect(ctx, base, "/new-listings-c4/browser-entry", 200)
                    no_redirect(ctx, base, "/new-genres-c4/fiction-browser", 200)
                    no_redirect(ctx, base, "/content/listing_browser_c4/browser-entry",
                                301, "/new-listings-c4/browser-entry")
                    no_redirect(ctx, base, "/taxonomy/genre_browser_c4/fiction-browser",
                                301, "/new-genres-c4/fiction-browser")
                    no_redirect(ctx, base, "/listings-browser/browser-entry",
                                301, "/new-listings-c4/browser-entry")
                    checks.append("chrome_explicit_null_to_inherit_removes_both_keys_and_restores_provider_urls")

                    # A non-null site choice wins over the package; removing
                    # that key, not setting it to null, restores inheritance.
                    cpt_base.fill("site-listings-c4")
                    tax_base.fill("site-genres-c4")
                    expect(cpt_mode).to_contain_text("base tùy chỉnh")
                    expect(tax_mode).to_contain_text("base tùy chỉnh")
                    no_redirect(ctx, base, "/new-listings-c4/browser-entry", 200)
                    no_redirect(ctx, base, "/new-genres-c4/fiction-browser", 200)
                    save.click()
                    expect(save).to_be_disabled(timeout=20000)
                    chosen = api(ctx, base, "GET", "/api/admin/settings/permalinks")
                    assert chosen["content_type_bases"]["listing_browser_c4"] == "site-listings-c4"
                    assert chosen["taxonomy_bases"]["genre_browser_c4"] == "site-genres-c4"
                    no_redirect(ctx, base, "/site-listings-c4/browser-entry", 200)
                    no_redirect(ctx, base, "/site-genres-c4/fiction-browser", 200)
                    no_redirect(ctx, base, "/new-listings-c4/browser-entry",
                                301, "/site-listings-c4/browser-entry")
                    no_redirect(ctx, base, "/new-genres-c4/fiction-browser",
                                301, "/site-genres-c4/fiction-browser")
                    checks.append("chrome_custom_base_wins_over_provider_defaults_for_cpt_and_taxonomy")

                    cpt_inherit.focus()
                    page.keyboard.press("Enter")
                    tax_inherit.focus()
                    page.keyboard.press("Enter")
                    expect(cpt_mode).to_contain_text("Đang kế thừa")
                    expect(tax_mode).to_contain_text("Đang kế thừa")
                    no_redirect(ctx, base, "/site-listings-c4/browser-entry", 200)
                    no_redirect(ctx, base, "/site-genres-c4/fiction-browser", 200)
                    save.click()
                    expect(save).to_be_disabled(timeout=20000)
                    cleared = api(ctx, base, "GET", "/api/admin/settings/permalinks")
                    assert cleared["content_type_bases"] == {}
                    assert cleared["taxonomy_bases"] == {}
                    no_redirect(ctx, base, "/new-listings-c4/browser-entry", 200)
                    no_redirect(ctx, base, "/new-genres-c4/fiction-browser", 200)
                    no_redirect(ctx, base, "/site-listings-c4/browser-entry",
                                301, "/new-listings-c4/browser-entry")
                    no_redirect(ctx, base, "/site-genres-c4/fiction-browser",
                                301, "/new-genres-c4/fiction-browser")
                    checks.append("chrome_keyboard_custom_to_inherit_deletes_both_draft_keys")

                    # With both maps sparse again, a provider update changes
                    # the *effective* URLs and MUST advance the C3 CAS version.
                    # A concurrent settings editor holding the prior full-form
                    # snapshot must receive 412, never clobber that update.
                    prior_version = cleared["version"]
                    latest_plugin = install(ctx, base, csrf,
                                            provider_zip("1.0.2", "latest-listings-c4", "latest-genres-c4"))
                    latest = api(ctx, base, "GET", "/api/admin/settings/permalinks")
                    assert latest["version"] == prior_version + 1, (cleared, latest)
                    assert latest["content_type_bases"] == {}
                    assert latest["taxonomy_bases"] == {}
                    fetch(ctx, base, "PUT", "/api/admin/settings/permalinks", 412,
                          full_permalink_input(cleared), csrf, prior_version)
                    assert api(ctx, base, "GET", "/api/admin/settings/permalinks") == latest
                    no_redirect(ctx, base, "/latest-listings-c4/browser-entry", 200)
                    no_redirect(ctx, base, "/latest-genres-c4/fiction-browser", 200)
                    no_redirect(ctx, base, "/new-listings-c4/browser-entry",
                                301, "/latest-listings-c4/browser-entry")
                    no_redirect(ctx, base, "/new-genres-c4/fiction-browser",
                                301, "/latest-genres-c4/fiction-browser")
                    no_redirect(ctx, base, "/content/listing_browser_c4/browser-entry",
                                301, "/latest-listings-c4/browser-entry")
                    checks.append("inherited_provider_update_bumps_c3_cas_and_rejects_stale_put")

                    # UI has the prior baseline. A provider update must make
                    # a later toggle/Save stale; even another toggle + fresh
                    # GET must not silently grant that draft a newer version.
                    cpt_legacy.click()
                    expect(cpt_mode).to_contain_text("Đang chọn đường dẫn legacy")
                    save.click()
                    expect(form.get_by_role("alert")).to_contain_text("412", timeout=20000)
                    expect(cpt_mode).to_contain_text("Đang chọn đường dẫn legacy")
                    tax_legacy.click()
                    expect(tax_mode).to_contain_text("Đang chọn đường dẫn legacy")
                    expect(save).to_be_disabled()
                    page.get_by_role("button", name="Kiểm tra phiên bản mới").click()
                    expect(form.get_by_text("Cấu hình trên máy chủ mới hơn", exact=False)).to_be_visible()
                    expect(cpt_mode).to_contain_text("Đang chọn đường dẫn legacy")
                    expect(tax_mode).to_contain_text("Đang chọn đường dẫn legacy")
                    expect(save).to_be_disabled()
                    assert api(ctx, base, "GET", "/api/admin/settings/permalinks") == latest
                    image_file = artifacts / "c4-ui-stale-412.png"
                    page.screenshot(path=str(image_file), full_page=True)
                    shots[image_file.name] = digest(image_file)
                    page.get_by_role("button", name="Bỏ nháp, dùng phiên bản mới").click()
                    expect(cpt_mode).to_contain_text("Đang kế thừa", timeout=20000)
                    expect(tax_mode).to_contain_text("Đang kế thừa")
                    expect(save).to_be_disabled()
                    checks.append("chrome_provider_cas_412_toggle_and_refresh_keep_draft_until_discard")

                    # An existing published Page claims the proposed CPT
                    # route; a rejected UI override must retain the form
                    # without touching site choices or active public URLs.
                    blocker = api(ctx, base, "POST",
                                  "/api/admin/content-types/page/entries", 201,
                                  {"title": "C4 route blocker", "slug": "c4-conflict-page",
                                   "values": {}}, csrf)
                    blocker = api(ctx, base, "PUT",
                                  f"/api/admin/content-types/page/entries/{blocker['id']}/route",
                                  200, {"custom_path": "/c4-conflict/browser-entry"},
                                  csrf, blocker["version"])
                    cpt_base.fill("c4-conflict")
                    expect(cpt_mode).to_contain_text("base tùy chỉnh")
                    no_redirect(ctx, base, "/latest-listings-c4/browser-entry", 200)
                    save.click()
                    expect(form.get_by_role("alert")).to_contain_text("409", timeout=20000)
                    expect(cpt_base).to_have_value("c4-conflict")
                    expect(save).to_be_enabled()
                    assert api(ctx, base, "GET", "/api/admin/settings/permalinks") == latest
                    no_redirect(ctx, base, "/latest-listings-c4/browser-entry", 200)
                    image_file = artifacts / "c4-ui-conflict-409.png"
                    page.screenshot(path=str(image_file), full_page=True)
                    shots[image_file.name] = digest(image_file)
                    page.get_by_role("button", name="Bỏ thay đổi", exact=True).click()
                    expect(cpt_mode).to_contain_text("Đang kế thừa")
                    expect(save).to_be_disabled()
                    checks.append("chrome_conflicting_cpt_base_409_rolls_back_and_keeps_draft")

                    # A draft belonging to a provider that becomes inactive
                    # must be restorable but not editable/savable while inactive.
                    cpt_base.fill("pending-disabled-c4")
                    tax_base.fill("pending-disabled-tax-c4")
                    response = ctx.request.fetch(
                        base + "/api/admin/plugins/browser-c4-provider/disable",
                        method="POST", timeout=30000,
                        headers={"X-CSRF-Token": csrf, "Origin": base,
                                 "If-Match": str(latest_plugin["cas_version"])},
                    )
                    assert response.status == 200, (response.status, response.text())
                    disabled = response.json()
                    save.click()
                    expect(form.get_by_role("alert")).to_contain_text("412", timeout=20000)
                    page.get_by_role("button", name="Kiểm tra phiên bản mới").click()
                    expect(cpt_base).to_be_disabled()
                    expect(tax_base).to_be_disabled()
                    expect(page.get_by_role("button", name="Kế thừa mặc định cho CPT listing_browser_c4")).to_have_count(0)
                    page.get_by_role("button", name="Bỏ thay đổi chưa lưu của CPT này").click()
                    page.get_by_role("button", name="Bỏ thay đổi chưa lưu của taxonomy này").click()
                    expect(cpt_mode).to_have_count(0)
                    expect(tax_mode).to_have_count(0)
                    assert api(ctx, base, "GET", "/api/admin/settings/permalinks")["content_type_bases"] == {}
                    assert api(ctx, base, "GET", "/api/admin/settings/permalinks")["taxonomy_bases"] == {}
                    page.get_by_role("button", name="Bỏ nháp, dùng phiên bản mới").click()
                    expect(cpt_mode).to_have_count(0)
                    enabled = ctx.request.fetch(
                        base + "/api/admin/plugins/browser-c4-provider/enable",
                        method="POST", timeout=30000,
                        headers={"X-CSRF-Token": csrf, "Origin": base,
                                 "If-Match": str(disabled["cas_version"])},
                    )
                    assert enabled.status == 200, (enabled.status, enabled.text())
                    page.reload(wait_until="domcontentloaded")
                    expect(cpt_mode).to_contain_text("Đang kế thừa", timeout=20000)
                    expect(tax_mode).to_contain_text("Đang kế thừa")
                    expect(save).to_be_disabled()
                    no_redirect(ctx, base, "/latest-listings-c4/browser-entry", 200)
                    checks.append("chrome_inactive_provider_prevents_edits_and_reverts_pending_drafts")

                    # Real server-side session expiration, followed by the
                    # same account signing in again, must retain the draft.
                    cpt_legacy.click()
                    expect(cpt_mode).to_contain_text("Đang chọn đường dẫn legacy")
                    fetch(ctx, base, "POST", "/auth/logout", 204, csrf=csrf)
                    save.click()
                    expect(page.locator("#guest-panel")).to_be_visible(timeout=20000)
                    fetch(ctx, base, "GET", "/api/admin/settings/permalinks", 401)
                    page.locator("#auth-username").fill("c4_browser")
                    page.locator("#auth-password").fill("C4-Browser-Isolated-Password!2026")
                    page.locator("#auth-login").click()
                    expect(cpt_mode).to_contain_text("Đang chọn đường dẫn legacy", timeout=20000)
                    expect(tax_mode).to_contain_text("Đang kế thừa")
                    expect(save).to_be_enabled()
                    assert api(ctx, base, "GET", "/api/admin/settings/permalinks")["content_type_bases"] == {}
                    page.get_by_role("button", name="Bỏ thay đổi", exact=True).click()
                    expect(save).to_be_disabled()
                    checks.append("chrome_real_401_same_account_reauth_restores_unsaved_choice")

                    # Another administrator must not receive the first
                    # account's pending RAM-only permalink draft.
                    csrf = api(ctx, base, "GET", "/auth/me")["csrf_token"]
                    api(ctx, base, "POST", "/api/admin/users", 201,
                        {"username": "c4_other", "password": "C4-Other-Isolated-Password!2026",
                         "role": "admin"}, csrf)
                    tax_legacy.click()
                    fetch(ctx, base, "POST", "/auth/logout", 204, csrf=csrf)
                    save.click()
                    expect(page.locator("#guest-panel")).to_be_visible(timeout=20000)
                    page.locator("#auth-username").fill("c4_other")
                    page.locator("#auth-password").fill("C4-Other-Isolated-Password!2026")
                    page.locator("#auth-login").click()
                    expect(cpt_mode).to_contain_text("Đang kế thừa", timeout=20000)
                    expect(tax_mode).to_contain_text("Đang kế thừa")
                    expect(save).to_be_disabled()
                    assert api(ctx, base, "GET", "/api/admin/settings/permalinks")["taxonomy_bases"] == {}
                    checks.append("chrome_real_401_cross_account_draft_isolation")

                    # Changing an unrelated Post preset must leave provider
                    # site-choice maps sparse in the full-form PUT.
                    page.locator("#permalink-post-preset").select_option("/blog/%postname%")
                    expect(save).to_be_enabled()
                    save.click()
                    expect(save).to_be_disabled(timeout=20000)
                    unrelated = api(ctx, base, "GET", "/api/admin/settings/permalinks")
                    assert unrelated["post_structure"] == "/blog/%postname%"
                    assert unrelated["content_type_bases"] == {}
                    assert unrelated["taxonomy_bases"] == {}
                    no_redirect(ctx, base, "/latest-listings-c4/browser-entry", 200)
                    no_redirect(ctx, base, "/latest-genres-c4/fiction-browser", 200)
                    checks.append("chrome_unrelated_post_save_retains_absent_provider_site_choices")

                    # Two independent HTTP writer connections race on the
                    # same persisted DB: settings vN versus provider v1.0.3.
                    # Both outcomes are valid only if the winner's complete
                    # map and canonical routes are visible atomically.
                    csrf = api(ctx, base, "GET", "/auth/me")["csrf_token"]
                    race_snapshot = api(ctx, base, "GET", "/api/admin/settings/permalinks")
                    assert race_snapshot["content_type_bases"] == {}
                    assert race_snapshot["taxonomy_bases"] == {}
                    cookie = "; ".join(
                        item["name"] + "=" + item["value"] for item in ctx.cookies(base)
                    )
                    gate = Barrier(2)
                    def race_writer(kind: str) -> int:
                        if kind == "settings":
                            payload = full_permalink_input(race_snapshot)
                            payload["content_type_bases"]["listing_browser_c4"] = None
                            payload["taxonomy_bases"]["genre_browser_c4"] = None
                            request = Request(
                                base + "/api/admin/settings/permalinks",
                                method="PUT", data=json.dumps(payload).encode(),
                                headers={"Origin": base, "Cookie": cookie,
                                         "X-CSRF-Token": csrf, "Content-Type": "application/json",
                                         "If-Match": f'\"v{race_snapshot["version"]}\"'},
                            )
                        else:
                            request = Request(
                                base + "/api/admin/plugins", method="POST",
                                data=provider_zip("1.0.3", "race-listings-c4", "race-genres-c4"),
                                headers={"Origin": base, "Cookie": cookie,
                                         "X-CSRF-Token": csrf, "Content-Type": "application/zip"},
                            )
                        gate.wait(timeout=8)
                        try:
                            with urlopen(request, timeout=30) as response:
                                return response.status
                        except HTTPError as error:
                            return error.code
                    with ThreadPoolExecutor(max_workers=2) as pool:
                        outcomes = dict(zip(("settings", "provider"),
                                            pool.map(race_writer, ("settings", "provider"))))
                    assert outcomes["provider"] == 201, outcomes
                    assert outcomes["settings"] in (200, 412), outcomes
                    race_saved = api(ctx, base, "GET", "/api/admin/settings/permalinks")
                    with sqlite3.connect(root / "database/opendash.sqlite") as connection:
                        assert connection.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
                        assert not connection.execute("PRAGMA foreign_key_check").fetchall()
                        defaults = dict(connection.execute(
                            "SELECT resource_kind,rewrite_base FROM provider_rewrite_defaults "
                            "WHERE resource_key IN ('listing_browser_c4','genre_browser_c4')"
                        ).fetchall())
                        assert defaults == {"content_type": "race-listings-c4",
                                            "taxonomy": "race-genres-c4"}, defaults
                    if outcomes["settings"] == 200:
                        assert race_saved["content_type_bases"] == {"listing_browser_c4": None}
                        assert race_saved["taxonomy_bases"] == {"genre_browser_c4": None}
                        restart_cpt = "/content/listing_browser_c4/browser-entry"
                        restart_tax = "/taxonomy/genre_browser_c4/fiction-browser"
                    else:
                        assert race_saved["content_type_bases"] == {}
                        assert race_saved["taxonomy_bases"] == {}
                        restart_cpt = "/race-listings-c4/browser-entry"
                        restart_tax = "/race-genres-c4/fiction-browser"
                    no_redirect(ctx, base, restart_cpt, 200)
                    no_redirect(ctx, base, restart_tax, 200)
                    no_redirect(ctx, base, "/latest-listings-c4/browser-entry", 301, restart_cpt)
                    no_redirect(ctx, base, "/latest-genres-c4/fiction-browser", 301, restart_tax)
                    checks.append(
                        f"simultaneous_settings_{outcomes['settings']}_provider_{outcomes['provider']}"
                        "_atomic_cas_and_canonical"
                    )
                finally:
                    browser.close()
        finally:
            stop_server(proc)
        proc, base = launch_server(binary, root)
        try:
            assert base.startswith("http://127.0.0.1:") and not base.endswith(":18964")
            # Provider and site choices are durable across process restart.
            with urlopen(base + restart_cpt) as res:
                assert res.status == 200
            with urlopen(base + restart_tax) as res:
                assert res.status == 200
            with urlopen(base + "/content/listing_browser_c4/browser-entry") as res:
                assert res.status == 200
            checks.append("restart_preserves_sparse_site_choices_and_provider_declarations")
        finally:
            stop_server(proc)
    return checks, shots


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--admin-dist", type=pathlib.Path, default=pathlib.Path("cms/admin/dist"))
    parser.add_argument("--artifact-dir", type=pathlib.Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    dist = args.admin_dist.resolve(strict=True)
    artifacts = args.artifact_dir.resolve()
    artifacts.mkdir(parents=True, exist_ok=True)
    receipt = {
        "status": "FAIL", "binary": str(binary), "binary_sha256": digest(binary),
        "platform": platform.platform(), "arch": platform.machine(),
        "checks": [], "screenshots": {}, "port_18964_used": False,
    }
    try:
        checks, screenshots = run(binary, dist, artifacts)
        receipt.update(status="PASS", checks=checks, screenshots=screenshots)
        return_code = 0
    except Exception as error:
        receipt["error"] = str(error)
        receipt["traceback"] = traceback.format_exc()
        return_code = 1
    (artifacts / "receipt.json").write_text(json.dumps(receipt, indent=2, ensure_ascii=False))
    print(json.dumps({key: receipt[key] for key in ["status", "checks", "screenshots", "error"] if key in receipt}, ensure_ascii=False))
    return return_code


if __name__ == "__main__":
    raise SystemExit(main())
