"""M3-P19 Windows release verification on disposable sites and real Chromium.

Run from cms/ after ``scripts/build.ps1 -Release``::

    python scripts/windows_m3_release_e2e.py --binary target/release/opendash.exe \
      --baseline-binary PATH/TO/PRE-0009/opendash.exe --browser PATH/TO/chrome.exe

The binary under test must be the actual embedded-Admin/Wasmi release build.
The historical binary is mandatory: restoring a snapshot without booting it
cannot establish SQLite rollback compatibility. Never use a production site.
"""

import argparse
from contextlib import closing
import hashlib
from pathlib import Path
import sqlite3
import subprocess
from tempfile import TemporaryDirectory

from playwright.sync_api import expect, sync_playwright

from media_smoke import make_png
from windows_distribution_smoke import assert_migrations, release_migration_versions
from windows_smoke import running, sample_theme


def snapshot(source: Path, target: Path) -> None:
    """SQLite online backup with both connections closed even on assertion failure."""
    target.parent.mkdir(parents=True, exist_ok=True)
    with closing(sqlite3.connect(source)) as src, closing(sqlite3.connect(target)) as dst:
        src.backup(dst)


def home_page_record(database: Path) -> tuple:
    with closing(sqlite3.connect(database)) as db:
        return db.execute("SELECT * FROM home_page").fetchone()


def api_ok(response, status: int, label: str):
    assert response.status == status, f"{label}: expected {status}, got {response.status}: {response.text()[:350]}"
    return response


def browser_flow(binary: Path, chrome: str | None, folder: Path) -> None:
    """Exercise all M3 Admin sections using a single fresh SQLite site."""
    db = folder / "fresh" / "database" / "opendash.sqlite"
    db.parent.mkdir(parents=True, exist_ok=True)
    password = "m3-release-admin-standalone-password"
    editor_password = "m3-release-editor-password"

    with running(binary, db) as base, sync_playwright() as playwright:
        browser = playwright.chromium.launch(executable_path=chrome)
        try:
            admin_context = browser.new_context(viewport={"width": 1280, "height": 900})
            page = admin_context.new_page()
            errors: list[str] = []
            page.on("pageerror", lambda err: errors.append(str(err)))
            api_ok(admin_context.request.get(base + "/api/admin/dashboard"), 401, "guest Dashboard")
            api_ok(admin_context.request.get(base + "/api/admin/users"), 401, "guest Users")
            page.goto(base + "/admin/#/dashboard")
            expect(page.get_by_role("heading", name="Đăng nhập quản trị")).to_be_visible()
            page.locator("#auth-username").fill("m3releaseadmin")
            page.locator("#auth-password").fill(password)
            page.locator("#auth-setup").click()
            expect(page.locator("#status")).to_contain_text("Đã tạo")
            page.locator("#auth-login").click()
            expect(page.get_by_role("heading", name="Thống kê nội dung")).to_be_visible()
            for metric in ("post-draft", "post-published", "page-draft", "page-published"):
                expect(page.locator(f'[data-dashboard-metric="{metric}"]')).to_contain_text("0")
            health = api_ok(admin_context.request.get(base + "/api/health"), 200, "health").json()
            assert health["status"] == "ok" and health["database"] == "ready"
            csrf = api_ok(admin_context.request.get(base + "/auth/me"), 200, "session").json()["csrf_token"]
            auth = {"X-CSRF-Token": csrf}

            # Post and Page: distinct draft and publication paths, stale CAS.
            records = {}
            for kind in ("post", "page"):
                record = api_ok(admin_context.request.post(
                    base + f"/api/admin/content-types/{kind}/entries",
                    headers=auth,
                    data={"slug": f"m3-release-{kind}", "title": f"M3 release {kind}",
                          "values": {"body": "Release content", "featured_image": None}},
                ), 201, f"create {kind}").json()
                records[kind] = record
            for key, value in (("post-draft", "1"), ("page-draft", "1")):
                page.get_by_role("button", name="Làm mới thống kê").click()
                expect(page.locator(f'[data-dashboard-metric="{key}"]')).to_contain_text(value)
            for kind in ("post", "page"):
                record = records[kind]
                publish_url = base + f"/api/admin/content-types/{kind}/entries/{record['id']}/publication"
                cas = {**auth, "If-Match": f'"v{record["version"]}"'}
                api_ok(admin_context.request.post(publish_url, data={"status": "published"}, headers=cas),
                       200, f"publish {kind}")
                api_ok(admin_context.request.post(publish_url, data={"status": "draft"}, headers=cas),
                       412, f"stale {kind} CAS")
                page.locator(f'nav [data-view="{kind}s"]').click() if kind == "post" else page.locator('nav [data-view="pages"]').click()
                expect(page.locator(".od-topbar h1")).to_be_visible()
            page.locator('nav [data-view="dashboard"]').click()
            expect(page.locator('[data-dashboard-metric="post-published"]')).to_contain_text("1")
            expect(page.locator('[data-dashboard-metric="page-published"]')).to_contain_text("1")

            # Media upload + actual Admin UI, including the same SQLite/site root.
            image = api_ok(admin_context.request.post(
                base + "/api/admin/media", data=make_png(seed=37),
                headers={**auth, "Content-Type": "image/png", "X-File-Name": "m3-release.png"},
            ), 201, "media upload").json()
            assert image["id"]
            page.locator('nav [data-view="media"]').click()
            expect(page.locator(".od-topbar h1")).to_contain_text("Thư viện ảnh")

            # Builder preview must not publish draft, Save must publish.
            page.locator('nav [data-view="builder"]').click()
            page.locator("#theme").set_input_files({
                "name": "m3-release-theme.zip", "mimeType": "application/zip", "buffer": sample_theme(),
            })
            expect(page.locator("#field-title")).to_be_visible()
            page.locator("#field-title").fill("M3 released homepage")
            page.locator("#field-note").fill("Preview remains private")
            page.locator("#preview").click()
            expect(page.frame_locator("#canvas").locator("h2")).to_have_text("M3 released homepage")
            assert "M3 released homepage" not in admin_context.request.get(base + "/").text()
            page.locator("#save").click()
            expect(page.locator("#status")).to_have_text("Đã lưu")
            assert "M3 released homepage" in admin_context.request.get(base + "/").text()

            # Themes activation uses Admin UI, not only a direct HTTP request.
            page.locator('nav [data-view="themes"]').click()
            themes_panel = page.locator("[data-testid='themes-panel']")
            expect(themes_panel).to_be_visible()
            themes_panel.get_by_role("button", name="Kích hoạt theme sample/banner").click()
            expect(themes_panel.locator("[data-testid='active-theme']")).to_contain_text("sample/banner")
            assert "M3 released homepage" in admin_context.request.get(base + "/").text()

            # Users + role restrictions across Admin and Editor sessions.
            editor = api_ok(admin_context.request.post(base + "/api/admin/users", headers=auth, data={
                "username": "m3releaseeditor", "password": editor_password, "role": "editor",
            }), 201, "create Editor").json()
            assert editor["role"] == "editor"
            page.locator('nav [data-view="users"]').click()
            expect(page.locator(".od-topbar h1")).to_contain_text("Người dùng")

            # General settings: CAS, forbidden Origin and missing CSRF.
            settings_url = base + "/api/admin/settings/general"
            current = api_ok(admin_context.request.get(settings_url), 200, "GET settings")
            etag = current.headers["etag"]
            settings_body = {"site_title": "OpenDash M3 release", "tagline": "Verified release",
                             "timezone": "Asia/Ho_Chi_Minh"}
            api_ok(admin_context.request.put(settings_url, headers={"If-Match": etag}, data=settings_body),
                   403, "missing CSRF")
            api_ok(admin_context.request.put(settings_url, headers={**auth, "If-Match": etag,
                                                               "Origin": "https://invalid.example"},
                                             data=settings_body), 403, "forbidden Origin")
            api_ok(admin_context.request.put(settings_url, headers={**auth, "If-Match": etag},
                                             data=settings_body), 200, "save settings")
            api_ok(admin_context.request.put(settings_url, headers={**auth, "If-Match": etag},
                                             data=settings_body), 412, "stale settings CAS")
            page.locator('nav [data-view="settings"]').click()
            expect(page.locator(".od-topbar h1")).to_contain_text("Cài đặt")

            # Wasmi plugin runs from the real dedicated Plugins UI.
            page.locator('nav [data-view="plugins"]').click()
            bundle = Path(__file__).resolve().parents[1] / "examples/plugins/sample-plugin/sample-plugin.zip"
            assert bundle.is_file(), "Build sample plugin first with scripts/build.ps1 -Release"
            page.locator("#plugin-zip").set_input_files(str(bundle))
            page.locator("#btn-run-plugin").click()
            expect(page.locator("#plugin-output")).to_contain_text('"run_count": 1')
            expect(page.locator(".od-topbar h1")).to_contain_text("Plugins")

            editor_context = browser.new_context(viewport={"width": 390, "height": 844})
            try:
                editor_page = editor_context.new_page()
                editor_page.goto(base + "/admin/#/dashboard")
                editor_page.locator("#auth-username").fill("m3releaseeditor")
                editor_page.locator("#auth-password").fill(editor_password)
                editor_page.locator("#auth-login").click()
                expect(editor_page.get_by_role("heading", name="Thống kê nội dung")).to_be_visible()
                for forbidden in ("users", "settings", "themes", "plugins"):
                    assert editor_page.locator(f'nav [data-view="{forbidden}"]').count() == 0
                api_ok(editor_context.request.get(base + "/api/admin/users"), 403, "Editor Users")
                api_ok(editor_context.request.get(settings_url), 403, "Editor Settings")
                api_ok(editor_context.request.get(base + "/api/admin/themes"), 403, "Editor Themes")
                api_ok(editor_context.request.get(base + "/api/admin/plugins"), 403, "Editor Plugins")
                assert editor_page.evaluate(
                    "document.documentElement.scrollWidth <= document.documentElement.clientWidth + 1"
                ), "390px Admin overflow"
            finally:
                editor_context.close()

            page.locator("#auth-logout").click()
            expect(page.locator("#auth-login")).to_be_visible()
            api_ok(admin_context.request.get(base + "/api/admin/users"), 401, "post-logout Users")
            assert not errors, f"JS errors: {errors}"
            admin_context.close()
        finally:
            browser.close()

    # Persist after a real process stop/restart on the same site, not a mock.
    assert_migrations(db, release_migration_versions())
    with running(binary, db) as base:
        from windows_distribution_smoke import http_req
        assert "M3 released homepage" in http_req(base, "/")[1]
        assert http_req(base, "/api/health")[0] == 200
        with closing(sqlite3.connect(db)) as conn:
            assert conn.execute("SELECT site_title FROM site_general_settings").fetchone() == (
                "OpenDash M3 release",)
            assert conn.execute("SELECT COUNT(*) FROM media_assets").fetchone()[0] >= 1
    print("PASS: integrated real Chrome Admin, post/page, media, Builder, themes, Users, Settings, Wasmi, RBAC/CAS/CSRF, logout and restart")


def historical_sqlite_upgrade(binary: Path, old_binary: Path, fixture: Path, folder: Path) -> None:
    """Verify both pre-0008 and post-0008 upgrades, then restore old snapshots."""
    versions = release_migration_versions()
    baseline = folder / "pre0008.sqlite"
    snapshot(fixture, baseline)
    record = home_page_record(baseline)
    assert record is not None, "Historical fixture must contain a saved homepage"

    # The genuine older executable must be able to operate the pre-0009 schema.
    post0008 = folder / "post0008.sqlite"
    snapshot(baseline, post0008)
    with running(old_binary, post0008) as old_base:
        from windows_distribution_smoke import http_req
        assert "Baseline Title" in http_req(old_base, "/")[1]
    with closing(sqlite3.connect(post0008)) as conn:
        old_versions = [v for (v,) in conn.execute(
            "SELECT version FROM _sqlx_migrations WHERE success=1 ORDER BY version"
        )]
    assert old_versions and max(old_versions) == 8, (
        "Expected a genuine post-0008/pre-0009 binary, not a newer release", old_versions)

    for label, source in (("pre0008", baseline), ("post0008", post0008)):
        upgraded = folder / f"upgraded-{label}.sqlite"
        snapshot(source, upgraded)
        with running(binary, upgraded) as base:
            from windows_distribution_smoke import http_req
            assert "Baseline Title" in http_req(base, "/")[1], label
            assert http_req(base, "/api/health")[0] == 200
        assert_migrations(upgraded, versions)
        assert home_page_record(upgraded) == record, f"Homepage changed after {label} migration"

        restored = folder / f"restored-{label}.sqlite"
        snapshot(source, restored)
        with running(old_binary, restored) as old_base:
            from windows_distribution_smoke import http_req
            assert "Baseline Title" in http_req(old_base, "/")[1], label
            assert http_req(old_base, "/api/health")[0] == 200
        assert home_page_record(restored) == record, f"Snapshot changed after {label} restore"
    print("PASS: SQLite pre-0008 and post-0008 upgrades to release + original snapshot restore with genuine old binary")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/opendash.exe"))
    parser.add_argument("--baseline-binary", type=Path, required=True,
                        help="Genuine binary from before migration 0009 (must support migration 0008)")
    parser.add_argument("--baseline-db", type=Path, default=Path("fixtures/baseline_0001.sqlite"))
    parser.add_argument("--browser", help="Path to installed Chrome/Chromium browser executable")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    old_binary = args.baseline_binary.resolve(strict=True)
    fixture = args.baseline_db.resolve(strict=True)
    assert binary != old_binary, "Release and pre-0009 binaries must be different"
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    print(f"Windows M3 release test - source HEAD {revision}; release SHA256 {digest}")
    print(f"Old binary: {old_binary}; fixture: {fixture}; browser: {args.browser or 'Playwright Chromium'}")
    with TemporaryDirectory(prefix="opendash-m3-windows-release-") as folder:
        disposable = Path(folder)
        browser_flow(binary, args.browser, disposable)
        historical_sqlite_upgrade(binary, old_binary, fixture, disposable)
    print("PASS: Windows M3 release E2E and historical SQLite recovery (isolated temp sites cleaned)")


if __name__ == "__main__":
    main()
