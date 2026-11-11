"""SITE-01 isolated SQLite + HTTP acceptance smoke.

Run from repo root:
  python3 cms/scripts/general_settings_smoke.py --binary cms/target/debug/opendash

Uses a disposable SQLite/site root; never touches a configured live website.
"""

import argparse
import io
import json
import shutil
import sqlite3
from concurrent.futures import ThreadPoolExecutor
from contextlib import closing
from pathlib import Path
from tempfile import TemporaryDirectory
from zipfile import ZIP_DEFLATED, ZipFile

from content_types_smoke import login, request, running
from content_entries_smoke import api
from media_smoke import raw_request


SETTINGS = "/api/admin/settings/general"
TIMEZONES = "/api/admin/settings/timezones"


def check(response, expected, context):
    assert response[0] == expected, (context, expected, response[0], response[1][:400])
    return response


def get(base, path, auth=None):
    return raw_request(base, "GET", path, headers=auth)


def put(base, body, auth, version=None, headers=None):
    combined = {
        **auth,
        "Content-Type": "application/json",
        **({"If-Match": f'"v{version}"'} if version is not None else {}),
        **(headers or {}),
    }
    return raw_request(
        base, "PUT", SETTINGS, json.dumps(body, ensure_ascii=False).encode("utf-8"),
        combined,
    )


def smoke(binary: Path):
    with TemporaryDirectory(prefix="opendash-m3-s25-settings-") as folder:
        db = Path(folder) / "site" / "database" / "opendash.sqlite"
        valid = {
            "site_title": "  Hải cảng <script>alert(1)</script>  ",
            "tagline": "<img src=x onerror=alert(2)> & mô tả",
            "timezone": "Asia/Ho_Chi_Minh",
        }
        with running(binary, db) as base:
            check(get(base, SETTINGS), 401, "guest GET")
            check(get(base, TIMEZONES), 401, "guest catalog")
            check(put(base, valid, {}), 401, "guest PUT")
            check(request(base, "/auth/setup", {
                "username": "qaadmin", "password": "password123",
            }), 201, "setup")
            admin = login(base, "qaadmin", "password123")
            check(request(base, "/api/admin/users", {
                "username": "editor", "password": "password123",
            }, admin), 201, "editor setup")
            editor = login(base, "editor", "password123")
            check(get(base, SETTINGS, editor), 403, "editor GET")
            check(get(base, TIMEZONES, editor), 403, "editor catalog")
            check(put(base, valid, editor, 1), 403, "editor PUT")

            status, data, headers = check(get(base, SETTINGS, admin), 200, "defaults")
            assert json.loads(data) == {
                "site_title": "OpenDash", "tagline": "", "timezone": "UTC",
                "version": 1,
            }
            assert headers["ETag"] == '"v1"'
            assert "no-store" in headers["Cache-Control"]
            status, payload, headers = check(get(base, TIMEZONES, admin), 200, "catalog")
            zones = json.loads(payload)
            assert zones["version"] == "2026f"
            assert len(zones["items"]) == 313
            assert zones["items"] == sorted(zones["items"])
            assert all(t in zones["items"] for t in (
                "UTC", "America/New_York", "Asia/Ho_Chi_Minh",
            ))
            assert "no-store" in headers["Cache-Control"]

            check(put(base, valid, admin), 428, "missing ETag")
            for bad_etag in ('W/"v1"', '"v0"', "v1", '"v1","v2"'):
                check(put(base, valid, admin, headers={"If-Match": bad_etag}),
                      400, "bad ETag")
            check(put(base, valid, {"Cookie": admin["Cookie"]}, 1), 403, "missing CSRF")
            check(put(base, valid, {**admin, "X-CSRF-Token": "wrong"}, 1),
                  403, "bad CSRF")
            check(put(base, valid, admin, 1, {"Origin": "https://evil.invalid"}),
                  403, "bad origin")

            for field, value in (
                ("site_title", ""),
                ("site_title", "a" * 121),
                ("site_title", " \tText"),
                ("tagline", "🦀" * 241),
                ("tagline", "newline\rvalue"),
                ("timezone", "US/Eastern"),
                ("timezone", "../../etc/passwd"),
            ):
                check(put(base, {**valid, field: value}, admin, 1), 400,
                      f"invalid {field}={value[:15]!r}")
            check(put(base, {**valid, "unknown": "field"}, admin, 1), 400,
                  "unknown JSON field")
            assert json.loads(get(base, SETTINGS, admin)[1])["version"] == 1

            status, data, headers = check(put(base, valid, admin, 1), 200, "first save")
            saved = json.loads(data)
            assert saved["site_title"] == valid["site_title"].strip()
            assert saved["tagline"] == valid["tagline"]
            assert saved["timezone"] == "Asia/Ho_Chi_Minh"
            assert saved["version"] == 2 and headers["ETag"] == '"v2"'
            assert "no-store" in headers["Cache-Control"]
            check(put(base, {**valid, "site_title": "tab 2"}, admin, 1),
                  412, "stale second browser tab")
            assert json.loads(get(base, SETTINGS, admin)[1]) == saved
            html_schema = {
                "key": "settings_test", "label": "Tin tức",
                "fields": [{"key": "body", "label": "Body", "type": "rich_text"}],
            }
            check(request(base, "/api/admin/content-types", html_schema, admin),
                  201, "create archive fixture")
            listing = "/content/settings_test"
            html = check(get(base, listing), 200, "public archive")[1].decode()
            assert "&lt;script&gt;" in html and "&lt;img src=x onerror=alert(2)&gt;" in html
            assert "<script>" not in html and "<img src=x" not in html

            item = {
                "title": "Bài viết", "slug": "public-one",
                "values": {"body": "Draft privacy and XSS"},
            }
            created = check(request(
                base, "/api/admin/content-types/settings_test/entries", item, admin,
            ), 201, "create CPT entry")
            assert check(get(base, listing + "/public-one"), 200, "public single")[0] == 200
            html = get(base, listing + "/public-one")[1].decode()
            assert "&lt;script&gt;" in html and "<script>" not in html
            assert json.loads(get(base, SETTINGS, admin)[1])["version"] == 2

            archive = io.BytesIO()
            with ZipFile(archive, "w", ZIP_DEFLATED) as theme:
                theme.writestr("block.json", json.dumps({
                    "id": "settings/banner",
                    "fields": {"title": "Tiêu đề", "note": "Mô tả"},
                }))
                theme.writestr("block.html", "<div>{{ values.title }}</div>")
                theme.writestr("content-list.html",
                               "<html><h1>CUSTOM {{ site.title }}</h1>"
                               "<p>{{ site.tagline }}</p></html>")
                theme.writestr("content-single.html",
                               "<html><h1>CUSTOM {{ site.title }}</h1>"
                               "<p>{{ site.tagline }}</p><p>{{ entry.title }}</p></html>")
            check(raw_request(base, "POST", "/theme", archive.getvalue(), {
                **admin, "Content-Type": "application/zip",
            }), 201, "install custom site template")
            check(api(base, "POST", "/save", {
                "type": "settings/banner",
                "values": {"title": "Homepage giữ nguyên", "note": "Đã lưu"},
            }, admin), 200, "activate custom template via Builder Save")
            for path in (listing, listing + "/public-one"):
                custom = check(get(base, path), 200, "custom site context")[1].decode()
                assert "CUSTOM" in custom and "&lt;script&gt;" in custom
                assert "&lt;img src=x onerror=alert(2)&gt;" in custom
                assert "<script>" not in custom and "<img src=x" not in custom
            assert "Homepage giữ nguyên" in get(base, "/")[1].decode()

            # Two racing writers must yield one win and one stale version.
            with ThreadPoolExecutor(max_workers=2) as executor:
                attempts = list(executor.map(
                    lambda name: put(base, {**valid, "site_title": name}, admin, 2)[0],
                    ("Tab A", "Tab B"),
                ))
            assert sorted(attempts) == [200, 412], attempts
            current = json.loads(get(base, SETTINGS, admin)[1])
            assert current["version"] == 3
            check(put(base, {
                "site_title": "Unicode 🦀" * 10,
                "tagline": "", "timezone": "America/New_York",
            }, admin, 3), 200, "America/New_York valid")

        with closing(sqlite3.connect(db)) as conn:
            row = conn.execute(
                "SELECT site_title, tagline, timezone, version FROM site_general_settings WHERE id=1"
            ).fetchone()
            assert row[2] == "America/New_York" and row[3] == 4
            assert conn.execute(
                "SELECT success FROM _sqlx_migrations WHERE version=10"
            ).fetchone() == (1,)
            assert conn.execute("SELECT COUNT(*) FROM users").fetchone() == (2,)
            assert conn.execute("SELECT COUNT(*) FROM content_entries").fetchone() == (1,)

        with running(binary, db) as base:
            admin = login(base, "qaadmin", "password123")
            saved = json.loads(check(get(base, SETTINGS, admin), 200, "restart")[1])
            assert saved["timezone"] == "America/New_York" and saved["version"] == 4
            assert check(get(base, "/content/settings_test/public-one"),
                         200, "published entry restart")[0] == 200
            assert check(get(base, "/"), 200, "Builder public homepage")[0] == 200
        print("PASS: SITE-01 defaults, ACL, CSRF, CAS/race, Unicode, escaping, persistence, restart")


def upgrade(binary: Path, baseline: Path):
    """Migrate an actual pre-M1.1 database fixture while preserving its homepage."""
    assert baseline.exists(), f"Missing baseline fixture: {baseline}"
    with TemporaryDirectory(prefix="opendash-m3-s25-upgrade-") as folder:
        db = Path(folder) / "site" / "database" / "opendash.sqlite"
        db.parent.mkdir(parents=True)
        shutil.copy2(baseline, db)
        with closing(sqlite3.connect(db)) as conn:
            before_home = conn.execute(
                "SELECT block_id, values_json FROM home_page WHERE id=1"
            ).fetchone()
            before_theme_ids = sorted(row[0] for row in conn.execute(
                "SELECT id FROM theme_blocks"
            ))
            assert before_home and before_theme_ids
            assert conn.execute(
                "SELECT success FROM _sqlx_migrations WHERE version=1"
            ).fetchone() == (1,)

        with running(binary, db) as base:
            assert check(get(base, "/"), 200, "legacy homepage preserved")[0] == 200
            check(request(base, "/auth/setup", {
                "username": "upgradeadmin", "password": "password123",
            }), 201, "upgrade auth setup")
            admin = login(base, "upgradeadmin", "password123")
            defaults = json.loads(check(get(base, SETTINGS, admin), 200,
                                        "upgrade defaults")[1])
            assert defaults == {
                "site_title": "OpenDash", "tagline": "", "timezone": "UTC", "version": 1,
            }

        with closing(sqlite3.connect(db)) as conn:
            assert conn.execute(
                "SELECT block_id, values_json FROM home_page WHERE id=1"
            ).fetchone() == before_home
            assert sorted(row[0] for row in conn.execute(
                "SELECT id FROM theme_blocks"
            )) == before_theme_ids
            assert conn.execute(
                "SELECT success FROM _sqlx_migrations WHERE version=10"
            ).fetchone() == (1,)
            assert conn.execute("SELECT COUNT(*) FROM users").fetchone() == (1,)
            # Simulate an otherwise valid legacy timezone removed from a later
            # catalog. It must remain editable when submitted unchanged.
            conn.execute("UPDATE site_general_settings SET timezone=? WHERE id=1",
                         ("Legacy/Zone",))
            conn.commit()

        with running(binary, db) as base:
            admin = login(base, "upgradeadmin", "password123")
            legacy = json.loads(check(get(base, SETTINGS, admin), 200,
                                      "legacy timezone reads unchanged")[1])
            assert legacy["timezone"] == "Legacy/Zone"
            check(put(base, {
                "site_title": "Legacy restored",
                "tagline": "Không mất dữ liệu",
                "timezone": "Legacy/Zone",
            }, admin, 1), 200, "existing timezone grandfathered")
            check(put(base, {
                "site_title": "Invalid",
                "tagline": "",
                "timezone": "Another/Invalid",
            }, admin, 2), 400, "new invalid timezone forbidden")
        print("PASS: upgrade baseline_0001, homepage/theme preserved and legacy timezone")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("cms/target/debug/opendash"))
    parser.add_argument("--baseline", type=Path, default=Path("cms/fixtures/baseline_0001.sqlite"))
    args = parser.parse_args()
    smoke(args.binary.resolve())
    upgrade(args.binary.resolve(), args.baseline.resolve())
