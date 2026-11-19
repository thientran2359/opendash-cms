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
                    expect(cpt_base).to_have_value("")
                    expect(tax_base).to_have_value("")
                    save = page.get_by_role("button", name="Lưu permalink")
                    assert save.is_disabled()
                    checks.append("native_chrome_c4_taxonomy_and_sparse_settings_form")
                    image_file = artifacts / "c4-permalink-default.png"
                    page.screenshot(path=str(image_file), full_page=True)
                    shots[image_file.name] = digest(image_file)

                    # An unchanged full-form save must not materialize provider
                    # defaults into site overrides. Explicit null is tested by
                    # typing and clearing the corresponding Admin form input.
                    cpt_base.fill("temporary-choice")
                    cpt_base.fill("")
                    tax_base.fill("temporary-choice")
                    tax_base.fill("")
                    expect(save).to_be_enabled()
                    save.click()
                    expect(save).to_be_disabled(timeout=20000)
                    explicit_null = api(ctx, base, "GET", "/api/admin/settings/permalinks")
                    assert explicit_null["content_type_bases"]["listing_browser_c4"] is None
                    assert explicit_null["taxonomy_bases"]["genre_browser_c4"] is None
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
                finally:
                    browser.close()
        finally:
            stop_server(proc)
        proc, base = launch_server(binary, root)
        try:
            assert base.startswith("http://127.0.0.1:") and not base.endswith(":18964")
            # Provider and site choices are durable across process restart.
            import urllib.error
            import urllib.request
            with urllib.request.urlopen(base + "/content/listing_browser_c4/browser-entry") as res:
                assert res.status == 200
            checks.append("restart_preserves_site_override_and_provider_declarations")
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
