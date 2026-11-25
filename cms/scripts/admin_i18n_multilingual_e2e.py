#!/usr/bin/env python3
"""S05: real Admin browser journeys against an isolated release binary (0051)."""
from __future__ import annotations

import argparse
import json
import pathlib
import sqlite3
import tempfile
from contextlib import closing

from playwright.sync_api import expect, sync_playwright

from m7_beta_menu_picker_e2e import launch_server, stop_server
from media_smoke import make_png


GENERAL = "/api/admin/settings/general"
LOCALES = "/api/admin/locales"


def run(binary: pathlib.Path, artifacts: pathlib.Path) -> None:
    artifacts.mkdir(parents=True, exist_ok=True)
    checks = []
    with tempfile.TemporaryDirectory(prefix="opendash-admin-i18n-", ignore_cleanup_errors=True) as temporary:
        root = pathlib.Path(temporary).resolve()
        db = root / "database/opendash.sqlite"
        proc, base = launch_server(binary, root)
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel="chrome", headless=True)
                ctx = browser.new_context(viewport={"width": 1440, "height": 1000}, locale="vi-VN")
                anon = browser.new_context()
                page = ctx.new_page()
                page.set_default_timeout(20000)
                csrf = ""

                def api(method, path, data=None, etag=None, status=200):
                    headers = {"Origin": base, "X-CSRF-Token": csrf}
                    if etag is not None:
                        headers["If-Match"] = etag
                    response = ctx.request.fetch(base + path, method=method,
                                                 data=data, headers=headers)
                    assert response.status == status, (path, response.status, response.text()[:400])
                    return response

                def go(route):
                    page.goto(base + "/admin#" + route)

                def rows(sql, params=()):
                    with closing(sqlite3.connect(db)) as conn:
                        return conn.execute(sql, params).fetchall()

                def bootstrap(configured, effective):
                    result = anon.request.get(base + "/api/i18n/admin")
                    assert result.status == 200
                    data = result.json()
                    assert data["configured_locale"] == configured
                    assert data["effective_locale"] == effective
                    assert data["effective_bcp47"] == ("vi-VN" if effective == "vi" else "en-US")
                    assert data["effective_direction"] == "ltr"
                    assert data["configured_installed"] == (configured in ("en", "vi"))
                    return data

                def auth_errors():
                    messages = []
                    for path in ("/auth/me", GENERAL, "/api/themes/starter/options", "/api/admin/builder/blocks", "/api/admin/themes"):
                        response = anon.request.get(base + path)
                        assert response.status == 401, (path, response.status)
                        assert response.headers["x-opendash-error-code"] == "auth.unauthorized"
                        messages.append(response.text())
                    assert len(set(messages)) == 1
                    return messages[0]

                def save_cms(code, button, heading, effective):
                    select = page.get_by_test_id("cms-locale-select")
                    select.select_option(code)
                    if code in ("ja", "ar"):
                        expect(page.get_by_test_id("cms-locale-fallback")).to_be_visible()
                    page.evaluate("window.__i18nJourney = 12345")
                    with page.expect_response(lambda r: GENERAL in r.url and r.request.method == "PUT") as saved:
                        page.get_by_test_id("general-settings").locator("form").first.get_by_role(
                            "button", name=button, exact=True).click()
                    assert saved.value.status == 200, saved.value.text()
                    assert saved.value.request.headers.get("if-match")
                    expect(page.locator("#general-settings-heading")).to_have_text(heading)
                    expect(page.locator("html")).to_have_attribute("lang", "vi-VN" if effective == "vi" else "en-US")
                    expect(page.locator("html")).to_have_attribute("dir", "ltr")
                    assert page.evaluate("window.__i18nJourney") == 12345, "unexpected page reload"
                    expect(select).to_have_value(code)
                    bootstrap(code, effective)
                    assert rows("SELECT cms_locale FROM site_general_settings WHERE id=1") == [(code,)]

                # CUJ 1: the first-run form must be English even with a vi browser preference.
                bootstrap("en", "en")
                en_error = auth_errors()
                assert en_error and en_error.isascii()
                go("/settings/general")
                expect(page.locator("#auth-setup")).to_have_text("Set up admin (first time)")
                expect(page.locator("#auth-login")).to_have_text("Sign in")
                page.locator("#auth-username").fill("s05admin")
                page.locator("#auth-password").fill("password123")
                with page.expect_response(lambda r: r.url.endswith("/auth/setup") and r.request.method == "POST") as setup:
                    page.locator("#auth-setup").click()
                assert setup.value.status == 201, setup.value.text()
                page.locator("#auth-username").fill("s05admin")
                page.locator("#auth-password").fill("password123")
                with page.expect_response(lambda r: r.url.endswith("/auth/login") and r.request.method == "POST") as login:
                    page.locator("#auth-login").click()
                assert login.value.status == 200, login.value.text()
                csrf = login.value.json()["csrf_token"]
                expect(page.get_by_test_id("cms-locale-select")).to_have_value("en")
                expect(page.locator("#general-settings-heading")).to_have_text("General settings")
                expect(page.get_by_test_id("timezone-preview")).to_contain_text("Summer")
                old_general = api("GET", GENERAL)
                save_cms("vi", "Save settings", "Cài đặt chung", "vi")
                expect(page.get_by_test_id("timezone-preview")).to_contain_text("Mùa hè")
                expect(page).to_have_title("OpenDash — Tổng quát")
                assert auth_errors() == "Cần đăng nhập."
                stale = {k: old_general.json()[k] for k in ("site_title", "tagline", "timezone", "cms_locale")}
                failure = api("PUT", GENERAL, stale, old_general.headers["etag"], 412)
                assert failure.headers["x-opendash-error-code"] == "request.precondition_failed"
                assert not failure.text().isascii()
                go("/studio")
                expect(page.get_by_role("navigation", name="Điều hướng Tùy chỉnh giao diện")).to_be_visible()
                go("/settings/general")
                expect(page.locator("#od-content-language")).to_have_value("vi")
                page.screenshot(path=artifacts / "cuj1-vi.png")
                checks.append("CUJ-1")
                print("PASS CUJ-1", flush=True)

                # CUJ 2: unsupported LTR and RTL choices persist but render English.
                for code, button in (("ja", "Lưu cài đặt"), ("ar", "Save settings")):
                    save_cms(code, button, "General settings", "en")
                    expect(page.get_by_test_id("cms-locale-fallback")).to_be_visible()
                    page.reload()
                    expect(page.get_by_test_id("cms-locale-select")).to_have_value(code)
                    expect(page.get_by_test_id("cms-locale-fallback")).to_be_visible()
                    expect(page.locator("html")).to_have_attribute("lang", "en-US")
                    expect(page.locator("html")).to_have_attribute("dir", "ltr")
                    assert auth_errors() == en_error
                    page.screenshot(path=artifacts / f"cuj2-{code}.png")
                save_cms("en", "Save settings", "General settings", "en")
                checks.append("CUJ-2")
                print("PASS CUJ-2", flush=True)

                def multilingual():
                    go("/settings/multilingual")
                    expect(page.get_by_test_id("locale-row-vi")).to_be_visible()
                    return page.get_by_test_id("multilingual-enabled-checkbox")

                def add_locale(code, label):
                    page.get_by_test_id("multilingual-tabs-languages").click()
                    page.get_by_test_id("add-locale-btn").click()
                    page.get_by_test_id("new-locale-code").fill(code)
                    page.get_by_test_id("new-locale-label").fill(label)
                    page.get_by_test_id("add-locale-form").locator("button[type=submit]").click()
                    expect(page.get_by_test_id("locale-row-" + code)).to_be_visible()

                def remove_locale(code):
                    page.get_by_test_id("multilingual-tabs-languages").click()
                    page.get_by_test_id("delete-locale-" + code).click()
                    expect(page.get_by_test_id("delete-locale-modal")).to_be_visible()
                    with page.expect_response(lambda r: r.url.endswith(LOCALES + "/" + code)
                                              and r.request.method == "DELETE") as result:
                        page.get_by_test_id("confirm-delete-locale").click()
                    assert result.value.status == 200, result.value.text()
                    expect(page.get_by_test_id("locale-row-" + code)).to_have_count(0)
                    return result.value.json()

                def entry(locale, slug, title, terms=()):
                    result = api("POST", "/api/admin/content-types/post/entries",
                                 {"title": title, "slug": slug, "locale": locale,
                                  "term_ids": list(terms), "values": {}}, status=201).json()
                    api("POST", f"/api/admin/content-types/post/entries/{result['id']}/publication",
                        {"status": "published"}, '"v1"')
                    return result["id"]

                def category(locale, slug, parent=None):
                    data = {"kind": "category", "name": slug, "slug": slug, "locale": locale}
                    if parent is not None:
                        data["parent_id"] = parent
                    return api("POST", "/api/admin/taxonomy/terms", data, status=201).json()["id"]

                def content_snapshot(locale):
                    # Exclude public route claims, which are intentionally rebuilt by the toggle.
                    tables = ("content_entries", "taxonomy_terms", "site_general_settings_locales",
                              "site_reading_settings", "site_home_seo", "site_menu_slot_assignments",
                              "site_menu_slot_versions", "entry_public_paths", "entry_redirects",
                              "plugin_entry_projections", "theme_option_locale_values",
                              "theme_option_locale_versions")
                    return {table: rows(f"SELECT * FROM {table} WHERE locale=? ORDER BY rowid", (locale,))
                            for table in tables}

                # CUJ 3: seed real published translated content; toggle only through Admin UI.
                public = api("GET", "/api/admin/settings/public")
                api("PUT", "/api/admin/settings/public",
                    {"public_origin": base, "indexing_enabled": True, "content_language": "vi"},
                    public.headers["etag"])
                toggle = multilingual()
                expect(toggle).not_to_be_checked()
                toggle.click()
                expect(toggle).to_be_checked()
                add_locale("en", "English")
                vi_post = entry("vi", "s05-original", "S05 original")
                translated = api("POST", f"/api/admin/content-types/post/entries/{vi_post}/translations",
                                 {"target_locale": "en", "title": "S05 English", "slug": "s05-english"},
                                 '"v2"', 201).json()["id"]
                api("POST", f"/api/admin/content-types/post/entries/{translated}/publication",
                    {"status": "published"}, '"v1"')
                category("en", "s05-english-category")
                # Seed retained menu/SEO records on the disposable DB, independently of the UI
                # under test; the corresponding writer APIs have their own integration suites.
                with closing(sqlite3.connect(db)) as conn, conn:
                    conn.execute("PRAGMA foreign_keys=ON")
                    menu = conn.execute("INSERT INTO site_menus(menu_key,title) VALUES('s05-shared','Shared menu') RETURNING id").fetchone()[0]
                    conn.execute("INSERT INTO site_menu_slot_assignments(theme_id,locale,slot_key,menu_id) VALUES('opendash-starter','en','primary',?)", (menu,))
                    conn.execute("INSERT INTO site_home_seo(locale,title) VALUES('en','English home')")
                before = content_snapshot("en")
                assert before["content_entries"] and before["taxonomy_terms"] and before["site_home_seo"]
                public_en = anon.request.get(base + "/en/s05-english")
                assert public_en.status == 200, public_en.text()[:400]
                assert 'hreflang="en"' in public_en.text()
                assert anon.request.get(base + "/en/posts").status == 200
                assert 'aria-label="Language"' in public_en.text()
                sitemap = anon.request.get(base + "/sitemaps/1.xml")
                assert sitemap.status == 200 and "s05-english" in sitemap.text()
                go(f"/posts/{vi_post}/edit")
                expect(page.get_by_test_id("entry-translations-panel")).to_be_visible()
                for route, ready, locale_control in (
                    ("/posts/categories", "taxonomy-page", "taxonomy-locale-filter"),
                    ("/appearance/menus", "studio-panel-menus", "studio-preview-locale-bar"),
                    ("/studio", "studio-workspace", "studio-preview-locale-bar"),
                ):
                    go(route)
                    expect(page.get_by_test_id(ready)).to_be_visible()
                    expect(page.get_by_test_id(locale_control)).to_be_visible()
                go("/settings/seo")
                expect(page.get_by_test_id("seo-locale-tabs")).to_be_visible()
                toggle = multilingual()
                toggle.click()
                expect(page.get_by_test_id("disable-multilingual-modal")).to_be_visible()
                page.get_by_test_id("confirm-disable-multilingual").click()
                expect(toggle).not_to_be_checked()
                assert rows("SELECT multilingual_enabled FROM site_locales_state") == [(0,)]
                assert before == content_snapshot("en")
                assert anon.request.get(base + "/en/posts").status == 404
                assert anon.request.get(base + "/en/s05-english").status == 404
                original = anon.request.get(base + "/s05-original")
                assert original.status == 200 and 'hreflang=' not in original.text()
                assert 'aria-label="Ngôn ngữ"' not in original.text()
                sitemap = anon.request.get(base + "/sitemaps/1.xml")
                assert sitemap.status == 200 and "s05-english" not in sitemap.text()
                assert "/en/" not in sitemap.text() and "s05-original" in sitemap.text()
                go("/settings/general")
                expect(page.locator("#od-content-language")).to_be_disabled()
                go(f"/posts/{vi_post}/edit")
                expect(page.locator("#entry-title-input")).to_be_visible()
                expect(page.get_by_test_id("entry-translations-panel")).to_have_count(0)
                for route, ready, locale_control in (
                    ("/posts/categories", "taxonomy-page", "taxonomy-locale-filter"),
                    ("/appearance/menus", "studio-panel-menus", "studio-preview-locale-bar"),
                    ("/studio", "studio-workspace", "studio-preview-locale-bar"),
                ):
                    go(route)
                    expect(page.get_by_test_id(ready)).to_be_visible()
                    expect(page.get_by_test_id(locale_control)).to_have_count(0)
                go("/settings/seo")
                expect(page.get_by_label("Homepage SEO", exact=True)).to_be_visible()
                expect(page.get_by_test_id("seo-locale-tabs")).to_have_count(0)
                toggle = multilingual()
                page.screenshot(path=artifacts / "cuj3-disabled.png")
                toggle.click()
                expect(toggle).to_be_checked()
                assert before == content_snapshot("en")
                assert anon.request.get(base + "/en/s05-english").status == 200
                go(f"/posts/{vi_post}/edit")
                expect(page.get_by_test_id("entry-translations-panel")).to_be_visible()
                checks.append("CUJ-3")
                print("PASS CUJ-3", flush=True)

                # CUJ 4: hard-delete empty locale, soft-delete data, revive, then purge.
                multilingual()
                expect(page.get_by_test_id("delete-locale-vi")).to_have_count(0)
                snapshot = api("GET", LOCALES)
                failure = api("DELETE", LOCALES + "/vi", etag=snapshot.headers["etag"], status=422)
                assert failure.headers["x-opendash-error-code"] == "locale.cannot_delete_default"
                add_locale("de", "Deutsch")
                add_locale("fr", "Français")
                parent = category("fr", "s05-parent")
                child = category("fr", "s05-child", parent)
                fr_post = entry("fr", "s05-french", "S05 French", [child])
                uploaded = ctx.request.post(base + "/api/admin/media", data=make_png(seed=55),
                                            headers={"Origin": base, "X-CSRF-Token": csrf,
                                                     "Content-Type": "image/png", "X-File-Name": "s05-shared.png"})
                assert uploaded.status == 201, uploaded.text()
                media = uploaded.json()["id"]
                with closing(sqlite3.connect(db)) as conn, conn:
                    conn.execute("PRAGMA foreign_keys=ON")
                    pattern_ids = {}
                    for code in ("vi", "de", "fr"):
                        pattern_ids[code] = conn.execute(
                            "INSERT INTO content_entries(content_type_key,locale,translation_group_id,slug,title,values_json,publication_status) VALUES('pattern',?,?,?,'Shared pattern','{}','draft') RETURNING id",
                            (code, "s05-pattern-" + code, "s05-pattern")).fetchone()[0]
                    conn.execute("INSERT INTO entry_seo(entry_id,og_image_id) VALUES(?,?)", (fr_post, media))
                    conn.execute("INSERT INTO seo_public_media_refs(source_kind,source_id,media_id) VALUES('entry_seo',?,?)", (fr_post, media))
                    conn.execute("INSERT INTO site_home_seo(locale,og_image_id) VALUES('fr',?)", (media,))
                    conn.execute("INSERT INTO site_menu_slot_assignments(theme_id,locale,slot_key,menu_id) VALUES('opendash-starter','fr','primary',?)", (menu,))
                    conn.execute("INSERT INTO entry_redirects(entry_id,locale,source_path) VALUES(?,'fr','/old-s05-french')", (fr_post,))
                    conn.execute("INSERT INTO plugins(id,name,version,enabled,manifest_json,wasm_blob) VALUES('s05-plugin','Test','1.0.0',0,'{}',X'')")
                    conn.execute("INSERT INTO plugin_entry_projections(entry_id,plugin_id,locale,seo_json) VALUES(?,'s05-plugin','fr','{}')", (fr_post,))
                hard = remove_locale("de")
                assert hard["soft_deleted"] is False and hard["orphan_total"] == 0
                assert rows("SELECT code FROM site_locales WHERE code='de'") == []
                assert rows("SELECT locale FROM content_entries WHERE id=?", (pattern_ids["de"],)) == [("vi",)]
                retained = content_snapshot("fr")
                soft = remove_locale("fr")
                assert soft["soft_deleted"] is True
                assert rows("SELECT deleted_at FROM site_locales WHERE code='fr'")[0][0]
                assert retained == content_snapshot("fr")
                assert anon.request.get(base + "/fr/s05-french").status == 404
                page.get_by_test_id("multilingual-tabs-orphans").click()
                orphan_row = page.get_by_test_id("orphan-locale-row-fr")
                expect(orphan_row).to_be_visible()
                orphan = next(i for i in api("GET", LOCALES + "/orphans").json()["items"] if i["code"] == "fr")
                counts = orphan["counts"]
                assert counts["entries"] == 1 and counts["terms"] == 2
                assert counts["settings"] >= 1 and counts["menu_slots"] >= 1
                assert counts["routes"] >= 1 and counts["projections"] == 1 and counts["media_seo_refs"] == 1
                assert counts["total"] == soft["orphan_total"] == sum(v for k, v in counts.items() if k != "total")
                for n, key in enumerate(("entries", "terms", "settings", "menu_slots", "routes",
                                         "projections", "media_seo_refs", "total"), 1):
                    expect(orphan_row.locator("td").nth(n)).to_have_text(str(counts[key]))
                add_locale("fr", "Français")
                expect(orphan_row).to_have_count(0)
                assert retained == content_snapshot("fr")
                assert anon.request.get(base + "/fr/s05-french").status == 200
                remove_locale("fr")
                page.get_by_test_id("multilingual-tabs-orphans").click()
                page.get_by_test_id("purge-orphan-fr").click()
                expect(page.get_by_test_id("purge-orphan-modal")).to_be_visible()
                confirm = page.get_by_test_id("purge-confirm-input")
                submit = page.get_by_test_id("purge-confirm-submit")
                confirm.fill("de")
                expect(submit).to_be_disabled()
                confirm.fill("fr")
                expect(submit).to_be_enabled()
                page.screenshot(path=artifacts / "cuj4-confirm.png")
                submit.click()
                expect(orphan_row).to_have_count(0)
                assert rows("SELECT code FROM site_locales WHERE code='fr'") == []
                assert all(not records for records in content_snapshot("fr").values())
                assert rows("SELECT id FROM media_assets WHERE id=?", (media,)) == [(media,)]
                assert rows("SELECT id FROM site_menus WHERE id=?", (menu,)) == [(menu,)]
                patterns = rows("SELECT locale,slug FROM content_entries WHERE id IN (?,?,?) ORDER BY id",
                                tuple(pattern_ids.values()))
                assert len(patterns) == 3 and all(locale == "vi" for locale, _ in patterns)
                assert len({slug for _, slug in patterns}) == 3
                assert rows("SELECT * FROM seo_public_media_refs WHERE media_id=?", (media,)) == []
                assert ctx.request.get(base + f"/api/admin/media/{media}/file").status == 200
                assert rows("PRAGMA foreign_key_check") == []
                checks.append("CUJ-4")
                print("PASS CUJ-4", flush=True)
                browser.close()
        finally:
            stop_server(proc)
    (artifacts / "admin_i18n_multilingual_receipt.json").write_text(
        json.dumps({"checks": checks, "binary": str(binary)}, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=pathlib.Path,
                        default=pathlib.Path(__file__).resolve().parents[1] / "target/release/opendash")
    parser.add_argument("--artifact-dir", type=pathlib.Path, default=pathlib.Path("cms/target/s05-e2e"))
    args = parser.parse_args()
    run(args.binary.resolve(), args.artifact_dir.resolve())
