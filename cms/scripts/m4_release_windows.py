"""M4-S10-T02 / #109: exact-source Windows x64 release acceptance.

Build the embedded Admin and a locked Rust release first. This harness creates
one genuine M3 SQLite/media fixture using the M3 binary, migrates a copy with
the pinned M4 binary, tests Builder/Chrome/theme/taxonomy/media/security, and
restores the untouched backup using the matching M3 binary. It also runs the
existing standalone Chrome/HTTP negative suites against the SAME M4 release.

    python cms/scripts/m4_release_windows.py --binary PATH/TO/opendash.exe \
      --m3-binary PATH/TO/M3/opendash.exe --release-sha FULL_M4_SHA \
      --browser 'C:/Program Files/Google/Chrome/Application/chrome.exe'

Only TemporaryDirectory sites and SQLite online snapshots are ever written.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import sqlite3
import subprocess
import sys
from contextlib import closing
from importlib.metadata import version as package_version
from tempfile import TemporaryDirectory

from playwright.sync_api import expect, sync_playwright

from builder_editor_browser import state as browser_state
from builder_homepage_smoke import checked, element, get, preview, save
from builder_list_smoke import fragment
from content_entries_smoke import api
from content_types_smoke import login, request, running
from entry_terms_smoke import add_term
from media_smoke import raw_request, upload_image
from themes_smoke import activate, auth_login, install, json_request, request as theme_request, theme_zip, themes

ROOT = Path(__file__).resolve().parents[2]
CMS = ROOT / "cms"
G3_SOURCE = "690dfa1167f11fba701cf8a0cfa4c36cc5ed29a0"
M3_SOURCE = "d2b6bfe1e7b6c2f6895802b8ed609292747d3062"
M3_BINARY_HASH = "85be89f684a57c49dc8f45905ff87c4006c53ffc1b251f192d650d5db34300e9"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def snapshot(database: Path, output: Path) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    with closing(sqlite3.connect(database)) as original, closing(sqlite3.connect(output)) as target:
        original.backup(target)


def uploads(site: Path) -> dict[str, str]:
    root = site / "uploads"
    return {
        str(path.relative_to(site)): sha256(path)
        for path in root.rglob("*") if path.is_file()
    } if root.is_dir() else {}


def db_snapshot(database: Path, expected_max: int) -> dict[str, str]:
    """Fingerprint ALL application tables, excluding ephemeral session records."""
    with closing(sqlite3.connect(database)) as db:
        assert db.execute("PRAGMA integrity_check").fetchone() == ("ok",)
        assert db.execute("PRAGMA foreign_key_check").fetchall() == []
        migrations = db.execute(
            "SELECT version FROM _sqlx_migrations WHERE success=1 ORDER BY version"
        ).fetchall()
        assert migrations and migrations[-1] == (expected_max,), migrations
        names = [row[0] for row in db.execute(
            "SELECT name FROM sqlite_master WHERE type='table' "
            "AND name NOT LIKE 'sqlite_%' ORDER BY name"
        ) if row[0] not in {"sessions", "auth_sessions", "login_sessions"}]
        return {
            name: hashlib.sha256(repr(db.execute(
                f'SELECT * FROM "{name}" ORDER BY rowid'
            ).fetchall()).encode()).hexdigest()
            for name in names
        }


def public(base: str, path: str = "/", *, check_csp: bool = True) -> str:
    code, data, headers = raw_request(base, "GET", path)
    assert code == 200, (path, code, data[:300])
    if path == "/" and check_csp:
        assert "script-src 'none'" in headers.get("Content-Security-Policy", "")
    return data.decode("utf-8")


def leaf(document: dict, identifier: str) -> dict:
    found = [
        child
        for child in document["sections"][0]["children"][0]["children"][0]["children"]
        if child["id"] == identifier
    ]
    assert len(found) == 1, identifier
    return found[0]


def create_entry(base: str, auth: dict, kind: str, slug: str, title: str) -> dict:
    return json.loads(checked(api(
        base, "POST", f"/api/admin/content-types/{kind}/entries",
        {"slug": slug, "title": title, "values": {}}, auth,
    ), 201, f"old {kind}/{slug}")[1])


def verify_upgrade(m4: Path, m3: Path, chrome: Path) -> None:
    with TemporaryDirectory(prefix="opendash-m4-release-windows-") as directory:
        root = Path(directory)
        site = root / "upgrade-m3-to-m4"
        database = site / "database" / "cms.sqlite"
        m3_backup = root / "snapshot-m3.sqlite"
        m4_backup = root / "snapshot-m4.sqlite"
        old_site = root / "restored-m3"
        old_database = old_site / "database" / "cms.sqlite"

        # Use the ACTUAL M3 release executable to create the historical
        # SQLite database, account, ZIP themes, Post/Page and uploaded media.
        with running(m3, database) as base:
            checked(request(base, "/auth/setup", {
                "username": "themeadmin", "password": "qa-password-123",
            }), 201, "M3 account")
            auth = auth_login(base)
            old_public = create_entry(base, auth, "post", "m3-public", "M3 PUBLIC POST")
            old_private = create_entry(base, auth, "post", "m3-private", "PRIVATE M3 DRAFT")
            old_page = create_entry(base, auth, "page", "m3-page", "M3 PUBLIC PAGE")
            for item, kind in ((old_public, "post"), (old_page, "page")):
                checked(api(
                    base, "POST",
                    f'/api/admin/content-types/{kind}/entries/{item["id"]}/publication',
                    {"status": "published"},
                    {**auth, "If-Match": f'"v{item["version"]}"'},
                ), 200, f"M3 publish {kind}")
            assert old_private["publication_status"] == "draft"
            media_id = upload_image(base, auth, seed=109)["id"]
            assert raw_request(base, "GET", f"/media/{media_id}/file")[0] == 404
            a = theme_zip("qa-a/banner",
                          '<article data-theme="A">{{ values.title }}</article>',
                          fields={"title": "Title"})
            b = theme_zip("qa-b/banner",
                          '<article data-theme="B">{{ values.title }}</article>',
                          fields={"title": "Title"}, alias="qa-a/banner")
            c = theme_zip("qa-c/banner",
                          '<article data-theme="C">{{ values.title }}</article>',
                          fields={"title": "Title"})
            for name, package in (("A", a), ("B", b), ("C", c)):
                checked(install(base, auth, package), 201, f"M3 ZIP theme {name}")
            checked(activate(base, auth, "qa-a/banner", themes(base, auth)["version"]),
                    200, "M3 activate A")
            historical = {"type": "qa-a/banner", "values": {"title": "M3 LEGACY HOMEPAGE"}}
            checked(json_request(base, "POST", "/save", historical, auth),
                    200, "M3 legacy homepage")
            checked(theme_request(base, "GET", "/page", headers=auth), 200, "M3 /page")
            assert 'data-theme="A"' in public(base, check_csp=False)
            assert "M3 LEGACY HOMEPAGE" in public(base, check_csp=False)
            assert "PRIVATE M3 DRAFT" not in public(base, check_csp=False)

        old_hashes = db_snapshot(database, 11)
        old_media = uploads(site)
        assert len(old_media) >= 2, old_media
        snapshot(database, m3_backup)
        assert db_snapshot(m3_backup, 11) == old_hashes
        print("PASS #109: genuine M3 binary wrote legacy theme/Post/Page, media, SQLite v0011")

        # M4 runs on the COPY of that M3 site. The original SQLite snapshot
        # and its media hashes remain untouched for old-binary restoration.
        with running(m4, database) as base:
            auth = auth_login(base)
            initial = get(base, auth)
            # The M3 /save row does not maintain Builder's separate CAS
            # counter; 0013 starts the projected Builder revision at v1.
            assert initial["source"] == "legacy" and initial["version"] == 1, initial
            document = copy.deepcopy(initial["document"])
            legacy = document["sections"][0]["children"][0]["children"][0]["children"][0]
            assert legacy["type"] == "legacy/block"
            assert legacy["props"] == {
                "block_id": "qa-a/banner", "values": {"title": "M3 LEGACY HOMEPAGE"},
            }, legacy
            assert "M3 LEGACY HOMEPAGE" in public(base, check_csp=False)
            assert db_snapshot(database, 14)

            cat = add_term(base, auth, "category", "M4 Root", "m4-root")
            child = add_term(base, auth, "category", "M4 Child", "m4-child", cat["id"])
            tag = add_term(base, auth, "tag", "M4 Featured", "m4-featured")
            post_path = f'/api/admin/content-types/post/entries/{old_public["id"]}'
            post = json.loads(checked(api(base, "GET", post_path, headers=auth),
                                      200, "M4 migrated post")[1])
            assigned = json.loads(checked(api(
                base, "PUT", post_path,
                {"slug": post["slug"], "title": post["title"],
                 "values": post["values"], "term_ids": [child["id"], tag["id"]]},
                {**auth, "If-Match": f'"v{post["version"]}"'},
            ), 200, "M4 taxonomy on M3 Post")[1])
            assert set(assigned["term_ids"]) == {child["id"], tag["id"]}

            children = document["sections"][0]["children"][0]["children"][0]["children"]
            children.extend((
                element("core/heading", {"level": 2, "text": "M4 PUBLIC HEADING"},
                        "release-heading"),
                element("core/text", {
                    "text": "Binding fallback",
                    "binding": {
                        "type_key": "post", "entry_slug": "m3-public",
                        "source": {"kind": "title"}, "target": "text",
                    },
                }, "release-binding"),
                element("core/list", {
                    "type_key": "post", "sort": "id_desc", "page_size": 1,
                    "category_id": cat["id"], "tag_id": tag["id"],
                    "include_descendants": True,
                }, "release-list"),
                element("core/image", {"media_id": media_id, "alt": "Migrated media"},
                        "release-image"),
            ))
            row = document["sections"][0]["children"][0]
            row["responsive"] = {
                "desktop": {"columns_per_row": 1, "gap_px": 0},
                "mobile": {"columns_per_row": 1, "gap_px": 0},
            }
            draft = copy.deepcopy(document)
            leaf(draft, "release-heading")["props"]["text"] = "PREVIEW NOT PUBLIC"
            before_preview = get(base, auth)
            checked(preview(base, {}, draft), 401, "guest Preview")
            checked(preview(base, {"Cookie": auth["Cookie"]}, draft), 403, "Preview missing CSRF")
            checked(preview(base, {**auth, "Origin": "http://evil.invalid"}, draft),
                    403, "Preview invalid Origin")
            html = json.loads(checked(preview(base, auth, draft), 200, "M4 draft preview")[1])
            assert "PREVIEW NOT PUBLIC" in html["html"]
            assert "M3 PUBLIC POST" in html["html"]
            assert "PRIVATE M3 DRAFT" not in html["html"]
            assert "data-builder-responsive" in html["html"]
            assert get(base, auth) == before_preview
            assert "PREVIEW NOT PUBLIC" not in public(base, check_csp=False)
            checked(save(base, auth, document, initial["version"]), 200, "M4 convert")
            updated = get(base, auth)
            assert updated["document"] == document and updated["source"] == "layout"
            checked(save(base, auth, document, initial["version"]), 412, "M4 stale CAS")
            assert "M4 PUBLIC HEADING" in public(base)
            assert "M3 PUBLIC POST" in fragment(public(base), "release-list")
            assert "PRIVATE M3 DRAFT" not in public(base)
            assert raw_request(base, "GET", f"/media/{media_id}/file")[0] == 200
            assert raw_request(base, "GET", f"/media/{media_id}/thumbnail")[0] == 200
            assert raw_request(base, "GET", "/api/admin/builder/homepage")[0] == 401
            checked(theme_request(base, "GET", "/page", headers=auth), 409, "M3 writer/reader closed")
            print("PASS #109: M3→M4 migrated public/binding/list/taxonomy/media/CSRF/ETag")

            with sync_playwright() as pw:
                browser = pw.chromium.launch(executable_path=str(chrome))
                context = browser.new_context(viewport={"width": 1440, "height": 900})
                page = context.new_page()
                errors = []
                page.on("pageerror", lambda err: errors.append(str(err)))
                page.goto(base + "/admin/#/builder")
                page.locator("#auth-username").fill("themeadmin")
                page.locator("#auth-password").fill("qa-password-123")
                page.locator("#auth-login").click()
                expect(page.locator('[data-testid="builder-workspace"]')).to_contain_text("Đã mở")
                expect(page.locator('[role="treeitem"][data-node-id="release-heading"]')).to_be_visible()
                page.locator('[role="treeitem"][data-node-id="release-heading"]').click()
                page.get_by_label("Tiêu đề", exact=True).fill("M4 CHROME DRAFT")
                page.get_by_role("button", name="Xem trước").click()
                expect(page.frame_locator("#canvas").locator("body")).to_contain_text("M4 CHROME DRAFT")
                assert "M4 CHROME DRAFT" not in public(base)
                assert browser_state(page, base)["version"] == updated["version"]
                page.get_by_role("button", name="Canvas cấu trúc").click()
                page.get_by_role("button", name="Lưu homepage").click()
                expect(page.locator('[data-testid="builder-workspace"]')).to_contain_text(
                    "Đã lưu", timeout=10000,
                )
                assert "M4 CHROME DRAFT" in public(base)
                page.reload()
                page.locator('[role="treeitem"][data-node-id="release-heading"]').click()
                expect(page.get_by_label("Tiêu đề", exact=True)).to_have_value("M4 CHROME DRAFT")
                assert not errors, errors
                browser.close()

            frozen = get(base, auth)["document"]
            # Compatible B uses an alias for A; incompatible C hides only the
            # legacy node. Returning A must reconstruct the identical original.
            for name, marker in (("qa-b/banner", 'data-theme="B"'),
                                 ("qa-c/banner", 'data-theme="C"'),
                                 ("qa-a/banner", 'data-theme="A"')):
                checked(activate(base, auth, name, themes(base, auth)["version"]),
                        200, "Theme " + name)
                body = public(base)
                if name == "qa-c/banner":
                    assert "M3 LEGACY HOMEPAGE" not in body
                    assert marker not in body
                    diagnostics = json.loads(checked(
                        preview(base, auth, frozen), 200, "Theme C diagnostics",
                    )[1])["diagnostics"]
                    assert {"node_id": legacy["id"], "code": "unsupported_by_theme"} in diagnostics
                else:
                    assert marker in body and "M3 LEGACY HOMEPAGE" in body
                assert "M4 CHROME DRAFT" in body
                assert "M3 PUBLIC POST" in fragment(body, "release-list")
                assert "PRIVATE M3 DRAFT" not in body
                assert get(base, auth)["document"] == frozen
            assert leaf(frozen, legacy["id"]) == legacy
            assert leaf(frozen, "release-binding")["props"]["binding"]["entry_slug"] == "m3-public"
            assert frozen["sections"][0]["children"][0]["responsive"] == row["responsive"]
            print("PASS #109 Chrome: M3 upgraded Editor/Preview/Save/reload; A→B→C→A preserves legacy")

            # The historical M3 themes_browser.py targets obsolete Builder
            # form IDs (#type, #field-title). Exercise the actual M4 Themes
            # panel in Chrome instead, without changing that other task's
            # runner or weakening theme switch acceptance.
            with sync_playwright() as pw:
                browser = pw.chromium.launch(executable_path=str(chrome))
                context = browser.new_context(viewport={"width": 1280, "height": 900})
                page = context.new_page()
                errors = []
                page.on("pageerror", lambda err: errors.append(str(err)))
                page.goto(base + "/admin/#/themes")
                page.locator("#auth-username").fill("themeadmin")
                page.locator("#auth-password").fill("qa-password-123")
                page.locator("#auth-login").click()
                panel = page.locator('[data-testid="themes-panel"]')
                expect(panel).to_be_visible()
                expect(panel.locator('[data-testid="active-theme"]')).to_contain_text(
                    "qa-a/banner"
                )
                for name, marker in (
                    ("qa-b/banner", 'data-theme="B"'),
                    ("qa-c/banner", 'data-theme="C"'),
                    ("qa-a/banner", 'data-theme="A"'),
                ):
                    panel.get_by_role(
                        "button", name=f"Kích hoạt theme {name}"
                    ).click()
                    expect(panel.locator('[data-testid="active-theme"]')).to_contain_text(name)
                    html = public(base)
                    assert ("M3 LEGACY HOMEPAGE" in html) == (name != "qa-c/banner")
                    assert (marker in html) == (name != "qa-c/banner")
                    assert "M4 CHROME DRAFT" in html
                    assert get(base, auth)["document"] == frozen
                page.set_viewport_size({"width": 390, "height": 844})
                page.reload()
                expect(page.locator('[data-testid="themes-panel"]')).to_be_visible()
                assert page.evaluate(
                    "document.documentElement.scrollWidth <= "
                    "document.documentElement.clientWidth + 1"
                )
                assert not errors, errors
                browser.close()
            print("PASS #109 Chrome: M4 Themes UI B→C→A, 390px and saved Builder preservation")

            m4_state = get(base, auth)
            m4_media = uploads(site)
            snapshot(database, m4_backup)
            assert db_snapshot(m4_backup, 14) == db_snapshot(database, 14)
            assert m4_media == uploads(site)
        with running(m4, database) as base:
            auth = auth_login(base)
            assert get(base, auth) == m4_state
            assert "M3 LEGACY HOMEPAGE" in public(base)
            assert "M4 CHROME DRAFT" in public(base)
            assert raw_request(base, "GET", f"/media/{media_id}/file")[0] == 200
        print("PASS #109: restart M4 retains homepage, versions, taxonomy and media ACL")

        # Roll back only by copying the original 0011 SQLite online backup
        # and complete uploaded assets to a NEW site. Never run M3 against
        # the 0014 migrated database.
        old_database.parent.mkdir(parents=True)
        shutil.copy2(m3_backup, old_database)
        shutil.copytree(site / "uploads", old_site / "uploads")
        assert db_snapshot(old_database, 11) == old_hashes
        assert uploads(old_site) == old_media
        with running(m3, old_database) as base:
            auth = auth_login(base)
            restored = json.loads(checked(
                theme_request(base, "GET", "/page", headers=auth), 200, "M3 restored /page"
            )[1])
            assert restored == historical
            assert 'data-theme="A"' in public(base, check_csp=False)
            assert "M3 LEGACY HOMEPAGE" in public(base, check_csp=False)
            assert "M4 CHROME DRAFT" not in public(base, check_csp=False)
            assert raw_request(base, "GET", f"/media/{media_id}/file")[0] == 404
            assert raw_request(base, "GET", f"/api/admin/media/{media_id}/file", headers=auth)[0] == 200
        assert uploads(old_site) == old_media
        for name in ("home_page", "theme_blocks", "content_types", "content_entries",
                     "media_assets", "site_active_theme"):
            assert db_snapshot(old_database, 11)[name] == old_hashes[name], name
        print("PASS #109: old M3 binary reopened v0011 restored SQLite + SHA256 media, no M4 edits leaked")


def run_regressions(binary: Path, m3: Path, chrome: Path) -> None:
    cases: list[tuple[str, list[str]]] = [
        ("builder_editor_integration_browser.py", ["--browser", str(chrome)]),
        ("builder_editor_browser.py", ["--browser", str(chrome)]),
        ("builder_responsive_browser.py", ["--browser", str(chrome)]),
        ("builder_binding_inspector_browser.py", ["--browser", str(chrome)]),
        ("builder_list_browser.py", ["--browser", str(chrome)]),
        ("builder_field_binding_smoke.py", []),
        ("builder_list_smoke.py", []),
        ("builder_homepage_smoke.py", ["--m3-binary", str(m3)]),
        ("entry_terms_smoke.py", []),
        ("public_content_smoke.py", []),
        ("media_smoke.py", []),
        ("themes_smoke.py", ["--baseline-db", "fixtures/baseline_0001.sqlite"]),
        # M3 themes_browser.py explicitly requires removed legacy Builder
        # #type/#field-title UI; equivalent Themes UI coverage runs ABOVE
        # in Chrome against the migrated M4 site.
        ("windows_users_browser.py", ["--browser", str(chrome)]),
        ("general_settings_browser.py", ["--browser", str(chrome)]),
        ("dashboard_browser.py", ["--browser", str(chrome)]),
    ]
    for script, flags in cases:
        command = [
            sys.executable, str(CMS / "scripts" / script), "--binary", str(binary),
            *flags,
        ]
        print("RUN #109 regression:", script, flush=True)
        subprocess.run(command, cwd=CMS, env=os.environ.copy(), check=True)
        print("PASS #109 regression:", script, flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--m3-binary", required=True, type=Path)
    parser.add_argument("--browser", required=True, type=Path)
    parser.add_argument("--release-sha", required=True)
    parser.add_argument("--skip-regressions", action="store_true",
                        help="Debug only: full release acceptance NOT_RUN")
    args = parser.parse_args()
    assert platform.system() == "Windows" and platform.machine().lower() in {"amd64", "x86_64"}, (
        platform.platform(), platform.machine()
    )
    release = args.binary.resolve(strict=True)
    m3 = args.m3_binary.resolve(strict=True)
    chrome = args.browser.resolve(strict=True)
    assert release != m3 and release != chrome
    assert sha256(m3) == M3_BINARY_HASH, "Wrong M3 release binary"
    assert len(args.release_sha) == 40
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    assert head == args.release_sha, (head, args.release_sha)
    assert subprocess.check_output(
        ["git", "diff", "--name-only", args.release_sha, "--",
         "cms/admin", "cms/crates", "cms/migrations", "cms/Cargo.toml",
         "cms/Cargo.lock", "themes", "plugins"],
        cwd=ROOT, text=True,
    ).strip() == "", "Product source changed from release SHA"
    print(f"M4-S10-T02 G3 source {G3_SOURCE}", flush=True)
    print(f"M4 release candidate {head}; M3 genuine source {M3_SOURCE}", flush=True)
    print(f"OS {platform.platform()}, Python {platform.python_version()}, "
          f"Playwright {package_version('playwright')}", flush=True)
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(executable_path=str(chrome))
        print(f"Chrome {browser.version}", flush=True)
        browser.close()
    print(f"M4 release Windows x64 SHA256 {sha256(release)}", flush=True)
    print(f"M3 release Windows x64 SHA256 {sha256(m3)}", flush=True)
    verify_upgrade(release, m3, chrome)
    if args.skip_regressions:
        print("NOT_RUN: 15 regression suites skipped; NOT eligible for G5")
    else:
        run_regressions(release, m3, chrome)
        print("PASS #109 Windows x64 M4 release: historical M3 upgrade/restore + "
              "Chrome M4 Themes + 15 regressions", flush=True)


if __name__ == "__main__":
    main()
