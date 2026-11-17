"""Independent M5 #157 native Mac Intel M4->M5->M4 disposable-site probe.

Uses the genuine pinned native executables, a full pre-M5 site copy, HTTP,
SQLite integrity, and real headless Chrome. Never points at a user site.
"""

import argparse
import hashlib
import io
import json
import platform
import shutil
import sqlite3
import sys
from pathlib import Path
from tempfile import TemporaryDirectory
from zipfile import ZIP_DEFLATED, ZipFile


def require(result, status, description):
    assert result[0] == status, (description, status, result[0], str(result[1])[:450])
    print("PASS", description, flush=True)
    return result


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def zip_v2(theme_id="mac-proof-a", version="1.0.0"):
    document = {
        "manifest_version": 2,
        "theme_id": theme_id,
        "name": "M5 Mac proof " + theme_id,
        "version": version,
        "templates": {
            "header": "templates/header.html",
            "home": "templates/home.html",
            "single": "templates/single.html",
            "footer": "templates/footer.html",
        },
    }
    if theme_id == "mac-proof-a":
        document["options"] = [
            {"key": "brand", "label": "Brand", "type": "string", "default": "Default M5", "public": True}
        ]
    output = io.BytesIO()
    with ZipFile(output, "w", ZIP_DEFLATED) as z:
        z.writestr("theme.json", json.dumps(document))
        header = '<header data-proof="mac-header"><h1>{{ options.brand }}</h1></header>' if theme_id == "mac-proof-a" else '<header data-proof="mac-header">B Header</header>'
        z.writestr("templates/header.html", header)
        z.writestr("templates/home.html", f'<main data-proof="mac-home"><p>M5 Native Home {theme_id} {version}</p></main>')
        z.writestr("templates/single.html", '<main data-proof="mac-single">{{ entry.title }}</main>')
        z.writestr("templates/footer.html", '<footer data-proof="mac-footer">Native Footer</footer>')
    return output.getvalue()


def check_database(database, expected_min_version):
    with sqlite3.connect(database) as db:
        assert db.execute("PRAGMA integrity_check").fetchone() == ("ok",)
        assert db.execute("PRAGMA foreign_key_check").fetchall() == []
        version = db.execute("SELECT max(version) FROM _sqlx_migrations WHERE success=1").fetchone()[0]
        assert version >= expected_min_version, version
        rows = db.execute("SELECT id, slug, title FROM content_entries WHERE slug='m4-old-post'").fetchall()
        assert len(rows) == 1, rows
        return version, rows[0]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--m4", required=True, type=Path)
    ap.add_argument("--m5", required=True, type=Path)
    ap.add_argument("--chrome", required=True, type=Path)
    ap.add_argument("--scratch", required=True, type=Path)
    ap.add_argument("--scripts", type=Path, default=Path(__file__).resolve().parent)
    args = ap.parse_args()
    assert platform.system() == "Darwin" and platform.machine() == "x86_64"
    sys.path.insert(0, str(args.scripts))
    from content_types_smoke import running
    from content_entries_smoke import api
    from themes_smoke import theme_zip, setup, auth_login, install, activate, themes, json_request, request
    from playwright.sync_api import sync_playwright

    m4 = args.m4.resolve(strict=True)
    m5 = args.m5.resolve(strict=True)
    chrome = args.chrome.resolve(strict=True)
    print("EXPECTED_SOURCE_M4=a176414bba5b7b85f408137ef0e0b964c2329843", flush=True)
    print("EXPECTED_SOURCE_M5=c59d2f6ca5b2f4b2df0ef38cde4a77253c031af1", flush=True)
    assert digest(m4) == "c584c327fbeadea51023281fa66c2e7cc9524cd6956c809506e1359dd59eb9c9"
    assert digest(m5) == "c2a5c1dba67989ce9938cc26186d924518c669d297ea9c32c1d55ac847c3add5"
    print("M4_BINARY_SHA256=" + digest(m4), flush=True)
    print("M5_BINARY_SHA256=" + digest(m5), flush=True)

    with TemporaryDirectory(prefix="m5-157-", dir=args.scratch) as td:
        root = Path(td)
        site = root / "site"
        database = site / "database" / "opendash.sqlite"
        with running(m4, database) as base:
            auth = setup(base)
            legacy_theme = theme_zip("m4-legacy/banner", '<h1>{{ values.title }}</h1>')
            require(install(base, auth, legacy_theme), 201, "M4 native ZIP v1 install")
            require(activate(base, auth, "m4-legacy/banner", themes(base, auth)["version"]), 200, "M4 native ZIP v1 activate")
            require(json_request(base, "POST", "/save", {
                "type": "m4-legacy/banner", "values": {"title": "M4 Historical Homepage", "note": "M4 legacy"}
            }, auth), 200, "M4 native legacy homepage save")
            created = require(api(base, "POST", "/api/admin/content-types/post/entries", {
                "title": "M4 Historical Post", "slug": "m4-old-post", "values": {}
            }, auth), 201, "M4 native Post create")
            post = json.loads(created[1])
            require(api(base, "POST", f'/api/admin/content-types/post/entries/{post["id"]}/publication', {
                "status": "published"
            }, {**auth, "If-Match": f'"v{post["version"]}"'}), 200, "M4 native Post publish")
            assert "M4 Historical Homepage" in request(base, "GET", "/")[1]
            assert "M4 Historical Post" in request(base, "GET", "/content/post/m4-old-post")[1]
        m4_version, old_record = check_database(database, 14)
        assert m4_version == 14, m4_version
        snapshot = root / "pre-m5-snapshot"
        shutil.copytree(site, snapshot)
        assert check_database(snapshot / "database" / "opendash.sqlite", 14)[1] == old_record
        print("PASS PRE_M5_M4_NATIVE_SITE_SNAPSHOT", flush=True)

        with running(m5, database) as base:
            auth = auth_login(base)
            assert check_database(database, 20)[1] == old_record
            assert "M4 Historical Post" in request(base, "GET", "/content/post/m4-old-post")[1]
            print("PASS M4_TO_M5_NATIVE_FORWARD_MIGRATION_AND_DATA", flush=True)
            assert request(base, "GET", "/api/admin/themes")[0] == 401
            zip_bytes = zip_v2()
            headers = {**auth, "Origin": base, "Content-Type": "application/zip"}
            assert request(base, "POST", "/api/admin/themes", zip_bytes, {
                "Cookie": auth["Cookie"], "Origin": base, "Content-Type": "application/zip"
            })[0] == 403
            installed = require(request(base, "POST", "/api/admin/themes", zip_bytes, headers), 201,
                                "M5 native theme ZIP v2 install")
            revision = json.loads(installed[1])["revision_id"]
            theme_version = themes(base, auth)["version"]
            require(json_request(base, "PUT", "/api/admin/themes/active", {"revision_id": revision}, {
                **headers, "If-Match": f'"v{theme_version}"', "Content-Type": "application/json"
            }), 200, "M5 native A v2 activate")
            assert json_request(base, "PUT", "/api/admin/themes/active", {"revision_id": revision}, {
                **headers, "If-Match": f'"v{theme_version}"', "Content-Type": "application/json"
            })[0] == 412
            print("PASS M5_NATIVE_UNAUTH_CSRF_CAS_NEGATIVES", flush=True)
            require(json_request(base, "PUT", "/api/themes/mac-proof-a/options", {
                "values": {"brand": "M5 Verified Brand"}
            }, {**headers, "If-Match": '"v1"', "Content-Type": "application/json"}), 200,
                "M5 native Theme Options Save")
            home = request(base, "GET", "/")
            assert home[0] == 200 and "M5 Verified Brand" in home[1] and "Native Footer" in home[1]
            print("PASS M5_NATIVE_THEME_HEADER_FOOTER_OPTIONS_PUBLIC", flush=True)

            with sync_playwright() as pw:
                browser = pw.chromium.launch(executable_path=str(chrome), headless=True)
                page = browser.new_page(viewport={"width": 1280, "height": 900})
                errors = []
                page.on("pageerror", lambda e: errors.append("page: " + str(e)))
                def capture_console(message):
                    if message.type != "error":
                        return
                    location = (message.location or {}).get("url", "")
                    # These requests are expected before a browser has logged in.
                    if ("404" in message.text and "favicon.ico" in location) or (
                        "401" in message.text and "/auth/me" in location
                    ):
                        return
                    errors.append("console: " + message.text + " @ " + location)

                page.on("console", capture_console)
                page.goto(base + "/", wait_until="networkidle")
                assert page.locator('header[data-proof="mac-header"]').count() == 1
                assert page.locator('footer[data-proof="mac-footer"]').count() == 1
                assert "M5 Verified Brand" in page.locator("header").inner_text()
                page.goto(base + "/admin/#/themes", wait_until="networkidle")
                assert page.locator("body").count() == 1
                assert not errors, errors
                print("PASS M5_REAL_CHROME_PUBLIC_ADMIN_ZERO_PAGE_CONSOLE_ERRORS", flush=True)
                browser.close()

            zip_b = zip_v2("mac-proof-b")
            installed_b = require(request(base, "POST", "/api/admin/themes", zip_b, headers), 201,
                                  "M5 native B v2 install")
            revision_b = json.loads(installed_b[1])["revision_id"]
            require(json_request(base, "PUT", "/api/admin/themes/active", {"revision_id": revision_b}, {
                **headers, "If-Match": f'"v{themes(base, auth)["version"]}"',
                "Content-Type": "application/json"
            }), 200, "M5 native A to B activate")
            assert "mac-proof-b" in request(base, "GET", "/")[1]
            assert check_database(database, 20)[1] == old_record
            print("PASS M5_NATIVE_B_MISSING_A_PRESERVATION", flush=True)

            require(api(base, "POST", "/api/admin/users", {
                "username": "m5editor", "password": "EditorPassword123!", "role": "editor"
            }, auth), 201, "M5 create Editor user")
            editor_login = require(json_request(base, "POST", "/auth/login", {
                "username": "m5editor", "password": "EditorPassword123!"
            }), 200, "M5 login Editor")
            editor_auth = {
                "Cookie": editor_login[2]["Set-Cookie"].split(";")[0],
                "X-CSRF-Token": json.loads(editor_login[1])["csrf_token"],
            }
            new_post = require(api(base, "POST", "/api/admin/content-types/post/entries", {
                "slug": "m5-editor-post", "title": "M5 Editor Post", "values": {}
            }, editor_auth), 201, "M5 Editor creates Post while B active")
            new_record = json.loads(new_post[1])
            assert new_record["id"] != old_record[0]
            print("PASS M5_NATIVE_EDITOR_RBAC_DATA", flush=True)

        # Real native binary restart before switching to the upgraded A revision.
        with running(m5, database) as base:
            auth = auth_login(base)
            assert check_database(database, 20)[1] == old_record
            assert "mac-proof-b" in request(base, "GET", "/")[1]
            assert json.loads(api(
                base, "GET", f'/api/admin/content-types/post/entries/{new_record["id"]}',
                headers=auth
            )[1])["title"] == "M5 Editor Post"
            installed_a2 = require(request(base, "POST", "/api/admin/themes",
                                            zip_v2("mac-proof-a", "2.0.0"),
                                            {**auth, "Origin": base, "Content-Type": "application/zip"}),
                                   201, "M5 native A2 upgraded revision install")
            revision_a2 = json.loads(installed_a2[1])["revision_id"]
            require(json_request(base, "PUT", "/api/admin/themes/active", {
                "revision_id": revision_a2
            }, {**auth, "Origin": base,
                "If-Match": f'"v{themes(base, auth)["version"]}"'}), 200,
                "M5 native A2 after B and restart")
            assert "mac-proof-a 2.0.0" in request(base, "GET", "/")[1]
            assert check_database(database, 20)[1] == old_record
            assert json.loads(api(
                base, "GET", f'/api/admin/content-types/post/entries/{new_record["id"]}',
                headers=auth
            )[1])["title"] == "M5 Editor Post"
            print("PASS M5_NATIVE_A1_B_EDITOR_RESTART_A2_PRESERVATION", flush=True)

        restored = root / "restored-m4"
        shutil.copytree(snapshot, restored)
        restored_db = restored / "database" / "opendash.sqlite"
        assert check_database(restored_db, 14)[1] == old_record
        with running(m4, restored_db) as base:
            auth_login(base)
            assert "M4 Historical Homepage" in request(base, "GET", "/")[1]
            assert "M4 Historical Post" in request(base, "GET", "/content/post/m4-old-post")[1]
        assert check_database(restored_db, 14)[1] == old_record
        print("PASS M4_MATCHING_NATIVE_BINARY_PRE_M5_SNAPSHOT_ROLLBACK", flush=True)
    print("PASS M5_157_NATIVE_MACOS_PROBE", flush=True)


if __name__ == "__main__":
    main()
