"""Comprehensive authentication, RBAC, session expiry, CSRF, concurrent setup,
and database upgrade tests for OpenDash M1.1.
"""

import argparse
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
import io
import json
import os
from pathlib import Path
from queue import Queue
import sqlite3
import subprocess
from tempfile import TemporaryDirectory
from threading import Thread
from urllib.error import HTTPError
from urllib.request import Request, urlopen
from zipfile import ZIP_DEFLATED, ZipFile


def sample_theme(descriptor="block.json"):
    archive = io.BytesIO()
    with ZipFile(archive, "w", ZIP_DEFLATED) as theme:
        theme.writestr(
            descriptor,
            json.dumps({
                "id": "sample/banner",
                "fields": {"title": "Tiêu đề", "note": "Mô tả"},
            }),
        )
        theme.writestr(
            "block.html",
            "<article><h2>{{ values.title }}</h2><p>{{ values.note }}</p></article>",
        )
    return archive.getvalue()


@contextmanager
def running(binary, database):
    env = dict(os.environ, OPENDASH_PORT="0", OPENDASH_SITE_ROOT=str(database.parent), OPENDASH_DB_PATH=str(database))
    with subprocess.Popen(
        [str(binary)], env=env, stdout=subprocess.PIPE, text=True, encoding="utf-8"
    ) as process:
        output = Queue()
        Thread(target=lambda: output.put(process.stdout.readline()), daemon=True).start()
        try:
            line = output.get(timeout=20).strip()
            assert line.startswith("OpenDash: http://127.0.0.1:") and line.endswith("/builder"), line
            base = line.removeprefix("OpenDash: ").removesuffix("/builder")
            yield base
        finally:
            process.terminate()


def request(base, path, data=None, content_type=None, headers=None):
    req_headers = {}
    if content_type:
        req_headers["Content-Type"] = content_type
    if headers:
        req_headers.update(headers)
    req = Request(base + path, data=data, headers=req_headers)
    try:
        with urlopen(req, timeout=5) as response:
            set_cookie = response.headers.get("Set-Cookie")
            return response.status, response.read().decode("utf-8"), set_cookie
    except HTTPError as error:
        with error:
            set_cookie = error.headers.get("Set-Cookie")
            return error.code, error.read().decode("utf-8"), set_cookie


def test_concurrent_setup(binary):
    print("Testing concurrent setup race condition...")
    with TemporaryDirectory(prefix="opendash-setup-") as folder:
        database = Path(folder) / "cms.sqlite"
        with running(binary, database) as base:
            def do_setup(i):
                body = json.dumps({"username": f"admin{i}", "password": "password123"}).encode()
                return request(base, "/auth/setup", body, "application/json")[0]

            with ThreadPoolExecutor(max_workers=5) as pool:
                statuses = list(pool.map(do_setup, range(5)))

            # Exactly one 201, all others 409
            assert statuses.count(201) == 1, f"Expected exactly one 201, got {statuses}"
            assert statuses.count(409) == 4, f"Expected four 409s, got {statuses}"

        # Check DB
        conn = sqlite3.connect(database)
        user_count = conn.execute("SELECT COUNT(*) FROM users").fetchone()[0]
        init_val = conn.execute("SELECT initialized FROM bootstrap WHERE id = 1").fetchone()[0]
        conn.close()
        assert user_count == 1
        assert init_val == 1

        # Check after restart
        with running(binary, database) as base:
            body = json.dumps({"username": "admin_late", "password": "password123"}).encode()
            assert request(base, "/auth/setup", body, "application/json")[0] == 409


def test_argon2_burst_and_public_responsiveness(binary):
    print("Testing Argon2 concurrency burst while public page remains responsive...")
    with TemporaryDirectory(prefix="opendash-burst-") as folder:
        database = Path(folder) / "cms.sqlite"
        with running(binary, database) as base:
            admin_input = json.dumps({"username": "admin", "password": "password123"}).encode()
            assert request(base, "/auth/setup", admin_input, "application/json")[0] == 201

            def do_login(i):
                return request(base, "/auth/login", admin_input, "application/json")[0]

            def do_public():
                return request(base, "/")[0]

            with ThreadPoolExecutor(max_workers=12) as pool:
                login_futures = [pool.submit(do_login, i) for i in range(8)]
                public_futures = [pool.submit(do_public) for _ in range(5)]

                for pf in public_futures:
                    assert pf.result() == 200

                for lf in login_futures:
                    assert lf.result() == 200


def test_origin_validation(binary):
    print("Testing strict Origin validation (scheme, null, port, host)...")
    with TemporaryDirectory(prefix="opendash-origin-") as folder:
        database = Path(folder) / "cms.sqlite"
        with running(binary, database) as base:
            # Parse server host & port
            host_port = base.removeprefix("http://")
            https_origin = f"https://{host_port}"
            different_port_origin = "http://127.0.0.1:65530"

            admin_input = json.dumps({"username": "admin", "password": "password123"}).encode()

            # 1. Probe setup with https scheme -> must be rejected 403 (server only serves http)
            assert request(base, "/auth/setup", admin_input, "application/json", headers={"Origin": https_origin})[0] == 403
            # 2. Probe setup with null origin -> must be rejected 403
            assert request(base, "/auth/setup", admin_input, "application/json", headers={"Origin": "null"})[0] == 403
            # 3. Probe setup with different port origin -> must be rejected 403
            assert request(base, "/auth/setup", admin_input, "application/json", headers={"Origin": different_port_origin})[0] == 403
            # 4. Probe setup with alien host origin -> must be rejected 403
            assert request(base, "/auth/setup", admin_input, "application/json", headers={"Origin": "http://evil.com"})[0] == 403

            # 5. Legitimate setup with exact http origin -> 201
            assert request(base, "/auth/setup", admin_input, "application/json", headers={"Origin": base})[0] == 201


def test_auth_and_rbac(binary):
    print("Testing auth, validations, RBAC, CSRF, expiry, and logout...")
    with TemporaryDirectory(prefix="opendash-auth-") as folder:
        database = Path(folder) / "cms.sqlite"
        with running(binary, database) as base:
            # 1. Validation errors on setup
            assert request(base, "/auth/setup", b'{"username":"a","password":"password123"}', "application/json")[0] == 400
            assert request(base, "/auth/setup", b'{"username":"admin","password":"123"}', "application/json")[0] == 400
            assert request(base, "/auth/setup", b'{"username":"-admin","password":"password123"}', "application/json")[0] == 400

            # 2. Valid setup
            admin_creds = json.dumps({"username": "admin", "password": "password123"}).encode()
            assert request(base, "/auth/setup", admin_creds, "application/json")[0] == 201

            # 3. Login wrong password
            wrong_creds = json.dumps({"username": "admin", "password": "wrongpassword"}).encode()
            status, body, _ = request(base, "/auth/login", wrong_creds, "application/json")
            assert status == 401
            assert "Tên đăng nhập hoặc mật khẩu không chính xác." in body

            # 4. Login correct
            status, body, cookie_hdr = request(base, "/auth/login", admin_creds, "application/json")
            assert status == 200
            admin_cookie = cookie_hdr.split(";")[0].strip()
            admin_data = json.loads(body)
            admin_csrf = admin_data["csrf_token"]
            assert admin_data["user"]["role"] == "admin"

            # Check that password hash is not in DB as plaintext, and token is hashed
            conn = sqlite3.connect(database)
            stored_hash = conn.execute("SELECT password_hash FROM users WHERE username = 'admin'").fetchone()[0]
            assert "password123" not in stored_hash and stored_hash.startswith("$argon2")
            stored_token_hash = conn.execute("SELECT token_hash FROM sessions").fetchone()[0]
            assert admin_cookie.removeprefix("opendash_session=") != stored_token_hash
            conn.close()

            # 5. Check /auth/me
            status, me_body, _ = request(base, "/auth/me", headers={"Cookie": admin_cookie})
            assert status == 200
            assert json.loads(me_body)["user"]["username"] == "admin"

            # 6. CSRF checks on mutation
            theme_bytes = sample_theme()
            auth_no_csrf = {"Cookie": admin_cookie}
            auth_bad_csrf = {"Cookie": admin_cookie, "X-CSRF-Token": "bad_token"}
            auth_alien_origin = {"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf, "Origin": "http://evil.com"}
            auth_https_origin = {"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf, "Origin": f"https://{base.removeprefix('http://')}"}
            auth_null_origin = {"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf, "Origin": "null"}
            auth_good = {"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf}

            assert request(base, "/theme", theme_bytes, "application/zip", headers=auth_no_csrf)[0] == 403
            assert request(base, "/theme", theme_bytes, "application/zip", headers=auth_bad_csrf)[0] == 403
            assert request(base, "/theme", theme_bytes, "application/zip", headers=auth_alien_origin)[0] == 403
            assert request(base, "/theme", theme_bytes, "application/zip", headers=auth_https_origin)[0] == 403
            assert request(base, "/theme", theme_bytes, "application/zip", headers=auth_null_origin)[0] == 403
            assert request(base, "/theme", theme_bytes, "application/zip", headers=auth_good)[0] == 201

            # 7. Admin creates Editor
            editor_input = json.dumps({"username": "editor1", "password": "editorpassword"}).encode()
            # Unauthenticated create user -> 401
            assert request(base, "/api/admin/users", editor_input, "application/json")[0] == 401
            # Admin create editor -> 201
            st, ed_res, _ = request(base, "/api/admin/users", editor_input, "application/json", headers=auth_good)
            assert st == 201
            assert json.loads(ed_res)["role"] == "editor"
            # Duplicate editor -> 409
            assert request(base, "/api/admin/users", editor_input, "application/json", headers=auth_good)[0] == 409

            # 8. Login as Editor
            editor_login_input = json.dumps({"username": "editor1", "password": "editorpassword"}).encode()
            st, ed_login_res, ed_cookie_hdr = request(base, "/auth/login", editor_login_input, "application/json")
            assert st == 200
            editor_cookie = ed_cookie_hdr.split(";")[0].strip()
            editor_csrf = json.loads(ed_login_res)["csrf_token"]
            editor_auth = {"Cookie": editor_cookie, "X-CSRF-Token": editor_csrf}
            editor_read = {"Cookie": editor_cookie}

            # Editor can read registry & page
            assert request(base, "/registry", headers=editor_read)[0] == 200
            assert request(base, "/page", headers=editor_read)[0] == 200

            # Editor can preview & save
            block_data = json.dumps({"type": "sample/banner", "values": {"title": "Tác phẩm Editor", "note": "Ghi chú"}}).encode()
            assert request(base, "/preview", block_data, "application/json", headers=editor_auth)[0] == 200
            assert request(base, "/save", block_data, "application/json", headers=editor_auth)[0] == 200
            assert "Tác phẩm Editor" in request(base, "/")[1]

            # Editor CANNOT install theme -> 403
            assert request(base, "/theme", theme_bytes, "application/zip", headers=editor_auth)[0] == 403
            # Editor CANNOT create users -> 403
            new_ed = json.dumps({"username": "editor2", "password": "editorpassword"}).encode()
            assert request(base, "/api/admin/users", new_ed, "application/json", headers=editor_auth)[0] == 403

            # 9. Session Expiry check (modify expires_at in DB directly)
            conn = sqlite3.connect(database)
            conn.execute("UPDATE sessions SET expires_at = 0")
            conn.commit()
            conn.close()

            # Now editor_cookie should be expired -> 401
            assert request(base, "/auth/me", headers=editor_read)[0] == 401
            assert request(base, "/registry", headers=editor_read)[0] == 401
            assert request(base, "/save", block_data, "application/json", headers=editor_auth)[0] == 401

            # 10. Logout and cookie reuse check
            st, ed_login_res2, ed_cookie_hdr2 = request(base, "/auth/login", editor_login_input, "application/json")
            ed_cookie2 = ed_cookie_hdr2.split(";")[0].strip()
            ed_csrf2 = json.loads(ed_login_res2)["csrf_token"]
            ed_auth2 = {"Cookie": ed_cookie2, "X-CSRF-Token": ed_csrf2}
            assert request(base, "/auth/me", headers={"Cookie": ed_cookie2})[0] == 200

            # Logout without CSRF -> 403
            assert request(base, "/auth/logout", data=b"", headers={"Cookie": ed_cookie2})[0] == 403
            # Logout valid -> 204
            st, _, logout_set_cookie = request(base, "/auth/logout", data=b"", headers=ed_auth2)
            assert st == 204
            assert "Max-Age=0" in logout_set_cookie

            # Reusing logged out cookie -> 401
            assert request(base, "/auth/me", headers={"Cookie": ed_cookie2})[0] == 401
            assert request(base, "/save", block_data, "application/json", headers=ed_auth2)[0] == 401


def test_database_upgrade(binary, baseline_db_path: Path):
    print(f"Testing upgrade from baseline database fixture ({baseline_db_path})...")
    if not baseline_db_path.exists():
        raise FileNotFoundError(
            f"Baseline database fixture missing at {baseline_db_path}. "
            "Generate it with scripts/make_baseline_fixture.py or pass --baseline-db."
        )

    with TemporaryDirectory(prefix="opendash-upgrade-") as folder:
        test_db = Path(folder) / "cms.sqlite"
        # Clone using SQLite online backup API for consistent snapshot
        src = sqlite3.connect(baseline_db_path)
        dst = sqlite3.connect(test_db)
        src.backup(dst)
        dst.close()
        src.close()

        with running(binary, test_db) as base:
            # 1. Public page still renders Baseline Title from pre-M1.1 database
            pub_status, pub_html, _ = request(base, "/")
            assert pub_status == 200
            assert "Baseline Title" in pub_html, f"Expected 'Baseline Title' in public HTML, got: {pub_html}"

            # 2. Admin endpoints protected on upgraded DB
            assert request(base, "/registry")[0] == 401
            assert request(base, "/page")[0] == 401

            # 3. Setup first admin on upgraded DB
            admin_creds = json.dumps({"username": "admin", "password": "password123"}).encode()
            assert request(base, "/auth/setup", admin_creds, "application/json")[0] == 201

            # 4. Login as admin
            st, login_body, cookie_hdr = request(base, "/auth/login", admin_creds, "application/json")
            assert st == 200
            session_cookie = cookie_hdr.split(";")[0].strip()
            csrf_token = json.loads(login_body)["csrf_token"]
            auth_headers = {"Cookie": session_cookie, "X-CSRF-Token": csrf_token}
            read_headers = {"Cookie": session_cookie}

            # 5. Read saved page from pre-M1.1 baseline
            page_status, page_json, _ = request(base, "/page", headers=read_headers)
            assert page_status == 200
            page_data = json.loads(page_json)
            assert page_data["values"]["title"] == "Baseline Title"

            # 6. Save update
            new_block = json.dumps({
                "type": "sample/banner",
                "values": {"title": "Upgraded Title", "note": "Upgraded Note"},
            }).encode()
            assert request(base, "/save", new_block, "application/json", headers=auth_headers)[0] == 200
            assert "Upgraded Title" in request(base, "/")[1]

        # 7. Restart and verify persistence on upgraded DB
        with running(binary, test_db) as base:
            assert request(base, "/page", headers=read_headers)[0] == 200
            assert "Upgraded Title" in request(base, "/")[1]


def verify_all(binary, baseline_db: Path):
    test_concurrent_setup(binary)
    test_argon2_burst_and_public_responsiveness(binary)
    test_origin_validation(binary)
    test_auth_and_rbac(binary)
    test_database_upgrade(binary, baseline_db)
    print("ALL M1.1 AUTH, RBAC, CSRF, EXPIRY, RACE, ORIGIN, AND UPGRADE TESTS PASSED!")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/opendash.exe"))
    parser.add_argument("--baseline-db", type=Path, default=Path("fixtures/baseline_0001.sqlite"))
    args = parser.parse_args()
    verify_all(args.binary.resolve(), args.baseline_db.resolve())
