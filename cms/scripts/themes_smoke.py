"""M3-S27 Themes HTTP/SQLite smoke: A→B, A→C, C→A and rollback (isolated sites)."""
import argparse
import io
import json
import shutil
import sqlite3
from contextlib import closing
from pathlib import Path
from tempfile import TemporaryDirectory
from urllib.error import HTTPError
from urllib.request import Request, urlopen
from zipfile import ZIP_DEFLATED, ZipFile

from windows_smoke import running


def theme_zip(identifier, template, fields=None, alias=None, compat=None, extras=None):
    stream = io.BytesIO()
    with ZipFile(stream, "w", ZIP_DEFLATED) as archive:
        archive.writestr("block.json", json.dumps({
            "id": identifier,
            "fields": fields if fields is not None else {"title": "Title", "note": "Note"},
        }))
        archive.writestr("block.html", template)
        if alias is not None:
            archive.writestr("homepage-compat.json", json.dumps(
                {"version": 1, "accepts": alias} if compat is None else compat
            ))
        if extras:
            for name, content in extras.items():
                archive.writestr(name, content)
    return stream.getvalue()


def request(base, method, path, payload=None, headers=None):
    req = Request(base + path, data=payload, method=method, headers=headers or {})
    try:
        with urlopen(req, timeout=20) as response:
            return response.status, response.read().decode("utf-8"), response.headers
    except HTTPError as error:
        with error:
            return error.code, error.read().decode("utf-8"), error.headers


def json_request(base, method, path, body=None, headers=None):
    return request(base, method, path, None if body is None else json.dumps(body).encode(), {
        "Content-Type": "application/json", **(headers or {}),
    })


def assert_status(result, status):
    assert result[0] == status, f"expected {status}, got {result[0]}: {result[1][:300]}"
    return result


def setup(base):
    assert_status(json_request(base, "POST", "/auth/setup", {
        "username": "themeadmin", "password": "qa-password-123",
    }), 201)
    login = assert_status(json_request(base, "POST", "/auth/login", {
        "username": "themeadmin", "password": "qa-password-123",
    }), 200)
    payload = json.loads(login[1])
    cookie = login[2]["Set-Cookie"].split(";")[0]
    return {"Cookie": cookie, "X-CSRF-Token": payload["csrf_token"]}


def auth_login(base):
    login = assert_status(json_request(base, "POST", "/auth/login", {
        "username": "themeadmin", "password": "qa-password-123",
    }), 200)
    return {
        "Cookie": login[2]["Set-Cookie"].split(";")[0],
        "X-CSRF-Token": json.loads(login[1])["csrf_token"],
    }


def install(base, auth, data, endpoint="/api/admin/themes"):
    return request(base, "POST", endpoint, data, {
        **auth, "Content-Type": "application/zip",
    })


def themes(base, auth):
    result = assert_status(request(base, "GET", "/api/admin/themes", headers=auth), 200)
    state = json.loads(result[1])
    assert result[2].get("Cache-Control") == "no-store"
    assert result[2].get("ETag") == f'"v{state["version"]}"'
    return state


def activate(base, auth, identifier, version):
    return json_request(base, "PUT", "/api/admin/themes/active", {"id": identifier}, {
        **auth, "If-Match": f'"v{version}"',
    })


def page_data(db):
    with closing(sqlite3.connect(db)) as conn:
        return conn.execute("SELECT block_id, values_json FROM home_page WHERE id=1").fetchone()


def verify(binary, baseline_db):
    a = theme_zip("qa-a/banner", '<article data-theme="A"><h1>{{ values.title }}</h1><p>{{ values.note }}</p></article>')
    b = theme_zip("qa-b/banner", '<section data-theme="B"><h2>{{ values.title }}</h2><small>{{ values.note }}</small></section>', alias="qa-a/banner")
    c = theme_zip("qa-c/banner", '<main data-theme="C">{{ values.title }}</main>')
    with TemporaryDirectory(prefix="opendash-m3-s27-themes-") as temp:
        db = Path(temp) / "site" / "database" / "cms.sqlite"
        db.parent.mkdir(parents=True)
        with running(binary, db) as base:
            assert_status(request(base, "GET", "/api/admin/themes"), 401)
            auth = setup(base)
            initial = themes(base, auth)
            assert initial == {
                "items": [], "active_id": None, "version": 1, "homepage_compatible": None
            }, initial
            block = {"type": "qa-a/banner", "values": {"title": "Xin chào", "note": "Bản gốc"}}
            payload = json.dumps(block, ensure_ascii=False).encode()
            assert_status(install(base, auth, a, "/theme"), 201)  # legacy install compatibility
            assert_status(install(base, auth, b), 201)
            assert_status(install(base, auth, c), 201)
            assert_status(install(base, auth, a), 409)
            # A well-formed but missing alias or wrong schema must not add a row.
            assert_status(install(base, auth, theme_zip(
                "qa-invalid/banner", "<p>Invalid</p>", alias="missing/block",
            )), 400)
            assert_status(install(base, auth, theme_zip(
                "qa-wrong/banner", "<p>Invalid</p>",
                fields={"title": "Title"}, alias="qa-a/banner",
            )), 400)
            assert_status(install(base, auth, theme_zip(
                "qa-self/banner", "<p>Invalid</p>", alias="qa-self/banner",
            )), 400)
            assert_status(install(base, auth, theme_zip(
                "qa-schema/banner", "<p>Invalid</p>", alias="qa-a/banner",
                compat={"version": 2, "accepts": "qa-a/banner"},
            )), 400)
            assert_status(install(base, auth, theme_zip(
                "qa-extra/banner", "<p>Invalid</p>", extras={"unexpected.txt": "no"},
            )), 400)
            assert len(themes(base, auth)["items"]) == 3

            editor = assert_status(json_request(base, "POST", "/api/admin/users", {
                "username": "themeeditor", "password": "editor-password-1", "role": "editor",
            }, auth), 201)
            assert json.loads(editor[1])["role"] == "editor"
            editor_login = assert_status(json_request(base, "POST", "/auth/login", {
                "username": "themeeditor", "password": "editor-password-1",
            }), 200)
            editor_headers = {
                "Cookie": editor_login[2]["Set-Cookie"].split(";")[0],
                "X-CSRF-Token": json.loads(editor_login[1])["csrf_token"],
            }
            assert_status(request(base, "GET", "/api/admin/themes", headers=editor_headers), 403)
            assert_status(install(base, editor_headers, a), 403)
            assert_status(json_request(base, "PUT", "/api/admin/themes/active", {
                "id": "qa-a/banner",
            }, {**editor_headers, "If-Match": '"v1"'}), 403)
            assert_status(install(base, {"Cookie": auth["Cookie"]}, a), 403)
            assert_status(install(base, {**auth, "Origin": "http://evil.invalid"}, a), 403)
            assert_status(json_request(base, "PUT", "/api/admin/themes/active", {
                "id": "qa-a/banner",
            }, auth), 428)
            assert_status(json_request(base, "PUT", "/api/admin/themes/active", {
                "id": "qa-a/banner",
            }, {**auth, "If-Match": "v1"}), 400)
            assert_status(activate(base, auth, "missing/theme", 1), 404)
            assert_status(json_request(base, "PUT", "/api/admin/themes/active", {
                "id": "qa-a/banner",
            }, {**auth, "If-Match": '"v1"', "Origin": "null"}), 403)

            # Before activation, legacy theme from saved homepage still renders.
            saved = assert_status(request(base, "POST", "/save", payload, {
                **auth, "Content-Type": "application/json",
            }), 200)
            assert 'data-theme="A"' in saved[1]
            assert saved[2].get("X-OpenDash-Block-Visibility") == "rendered"
            assert saved[2].get("Cache-Control") == "no-store"
            assert 'data-theme="A"' in assert_status(request(base, "GET", "/"), 200)[1]
            row_before = page_data(db)

            # A→B: alias support, SAME saved fields rendered with B's template.
            assert_status(activate(base, auth, "qa-a/banner", 1), 200)
            result = assert_status(activate(base, auth, "qa-b/banner", 2), 200)
            assert result[2].get("ETag") == '"v3"'
            assert 'data-theme="B"' in assert_status(request(base, "GET", "/"), 200)[1]
            assert 'Xin chào' in assert_status(request(base, "GET", "/"), 200)[1]
            assert 'data-theme="A"' not in request(base, "GET", "/")[1]
            assert themes(base, auth)["homepage_compatible"] is True
            assert page_data(db) == row_before
            preview = assert_status(request(base, "POST", "/preview", json.dumps({
                "type": "qa-a/banner", "values": {"title": "Draft only", "note": "Note"},
            }).encode(), {**auth, "Content-Type": "application/json"}), 200)
            assert 'data-theme="B"' in preview[1] and 'Draft only' in preview[1]
            assert "Draft only" not in request(base, "GET", "/")[1]

            # A→C: unsupported block hidden, Builder /page JSON exactly preserved.
            before_hidden = page_data(db)
            assert_status(activate(base, auth, "qa-c/banner", 3), 200)
            public = assert_status(request(base, "GET", "/"), 200)
            assert 'data-theme=' not in public[1] and "Xin chào" not in public[1]
            assert public[2].get("X-OpenDash-Block-Visibility") is None
            assert "sandbox" in public[2].get("Content-Security-Policy")
            assert page_data(db) == before_hidden
            assert json.loads(request(base, "GET", "/page", headers=auth)[1]) == block
            assert themes(base, auth)["homepage_compatible"] is False
            unsupported = assert_status(request(base, "POST", "/preview", payload, {
                **auth, "Content-Type": "application/json",
            }), 200)
            assert unsupported[2].get("X-OpenDash-Block-Visibility") == "unsupported"
            assert unsupported[2].get("Cache-Control") == "no-store"
            assert "Xin chào" not in unsupported[1]
            # Save remains valid and preserves new values even though hidden.
            modified = {"type": "qa-a/banner", "values": {"title": "Giữ nguyên dữ liệu", "note": "Restored"}}
            saved_hidden = assert_status(request(base, "POST", "/save", json.dumps(
                modified, ensure_ascii=False).encode(), {
                **auth, "Content-Type": "application/json",
            }), 200)
            assert saved_hidden[2].get("X-OpenDash-Block-Visibility") == "unsupported"
            assert json.loads(request(base, "GET", "/page", headers=auth)[1]) == modified
            assert_status(activate(base, auth, "qa-b/banner", 3), 412)  # stale tab
            assert themes(base, auth)["active_id"] == "qa-c/banner"

            # C→A restores the original block without re-entering data.
            assert_status(activate(base, auth, "qa-a/banner", 4), 200)
            restored = assert_status(request(base, "GET", "/"), 200)
            assert 'data-theme="A"' in restored[1] and "Giữ nguyên dữ liệu" in restored[1]
            assert json.loads(request(base, "GET", "/page", headers=auth)[1]) == modified
            assert themes(base, auth)["version"] == 5
            print("PASS Themes: A→B compatible, A→C hidden/retained, C→A restored, auth/CAS/ZIP", flush=True)

        with running(binary, db) as base:
            auth = auth_login(base)
            assert themes(base, auth)["active_id"] == "qa-a/banner"
            assert "Giữ nguyên dữ liệu" in assert_status(request(base, "GET", "/"), 200)[1]
            assert json.loads(request(base, "GET", "/page", headers=auth)[1]) == modified
        with closing(sqlite3.connect(db)) as conn:
            assert conn.execute("SELECT success FROM _sqlx_migrations WHERE version=11").fetchone() == (1,)
            assert conn.execute("SELECT version,block_id FROM site_active_theme").fetchone() == (5, "qa-a/banner")
            assert conn.execute("SELECT COUNT(*) FROM content_entries").fetchone()[0] == 0
        print("PASS Themes: restart, active pointer, saved block/schema, migration 0011", flush=True)

        # Historical upgrade must preserve the baseline homepage and templates.
        upgrade = Path(temp) / "upgrade" / "cms.sqlite"
        upgrade.parent.mkdir()
        # Historical WAL-mode fixture is immutable, with no -wal/-shm sidecars.
        # SQLite mode=ro alone may try to create them; immutable=1 prevents
        # touching the tracked fixture on a fresh checkout.
        with closing(sqlite3.connect(f"file:{baseline_db}?mode=ro&immutable=1", uri=True)) as src, closing(sqlite3.connect(upgrade)) as dst:
            src.backup(dst)
        old_row = page_data(upgrade)
        with running(binary, upgrade) as base:
            assert_status(request(base, "GET", "/"), 200)
        assert old_row == page_data(upgrade)
        with closing(sqlite3.connect(upgrade)) as conn:
            assert conn.execute("SELECT block_id,version FROM site_active_theme").fetchone() == (None, 1)
            assert conn.execute("SELECT success FROM _sqlx_migrations WHERE version=11").fetchone() == (1,)
        print("PASS Themes: historical SQLite upgrade retains homepage, legacy active NULL", flush=True)


def rollback(binary, old_binary):
    """Never run the pre-0011 binary against an upgraded DB; restore snapshot first."""
    with TemporaryDirectory(prefix="opendash-theme-rollback-") as folder:
        db = Path(folder) / "site" / "cms.sqlite"
        db.parent.mkdir()
        a = theme_zip("qa-a/banner", '<h1 data-theme="A">{{ values.title }}</h1>')
        block = {"type": "qa-a/banner", "values": {"title": "Rollback", "note": "preserved"}}
        with running(old_binary, db) as base:
            auth = setup(base)
            assert_status(install(base, auth, a, "/theme"), 201)
            assert_status(request(base, "POST", "/save", json.dumps(block).encode(), {
                **auth, "Content-Type": "application/json",
            }), 200)
        snapshot = Path(folder) / "before-0011.sqlite"
        with closing(sqlite3.connect(db)) as conn, closing(sqlite3.connect(snapshot)) as dest:
            conn.backup(dest)
        with running(binary, db) as base:
            auth = auth_login(base)
            assert_status(activate(base, auth, "qa-a/banner", 1), 200)
        for suffix in ("", "-wal", "-shm"):
            path = Path(str(db) + suffix)
            if path.exists():
                path.unlink()
        shutil.copy2(snapshot, db)
        with running(old_binary, db) as base:
            auth = auth_login(base)
            assert "Rollback" in assert_status(request(base, "GET", "/"), 200)[1]
            assert json.loads(request(base, "GET", "/page", headers=auth)[1]) == block
        with closing(sqlite3.connect(db)) as conn:
            assert conn.execute("SELECT 1 FROM _sqlx_migrations WHERE version=11").fetchone() is None
        print("PASS Themes: snapshot rollback with pre-0011 binary & untouched homepage", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("cms/target/debug/opendash"))
    parser.add_argument("--baseline-db", type=Path, default=Path("cms/fixtures/baseline_0001.sqlite"))
    parser.add_argument("--baseline-binary", type=Path)
    args = parser.parse_args()
    verify(args.binary.resolve(), args.baseline_db.resolve())
    if args.baseline_binary:
        rollback(args.binary.resolve(), args.baseline_binary.resolve())
