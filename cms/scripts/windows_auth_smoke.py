"""Comprehensive authentication, RBAC, session expiry, CSRF, concurrent setup,
and database upgrade tests for OpenDash M1.1.
"""

import argparse
from concurrent.futures import ThreadPoolExecutor
from contextlib import closing, contextmanager
import io
import json
import os
from pathlib import Path
from queue import Queue
import sqlite3
import subprocess
from tempfile import TemporaryDirectory
from threading import Barrier, Thread
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


def request(base, path, data=None, content_type=None, headers=None, method=None, timeout=5):
    req_headers = {}
    if content_type:
        req_headers["Content-Type"] = content_type
    if headers:
        req_headers.update(headers)
    req = Request(base + path, data=data, headers=req_headers, method=method)
    try:
        with urlopen(req, timeout=timeout) as response:
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
        # The checked-in baseline is read-only test data. Opening it normally
        # can create -wal/-shm sidecars beside the tracked fixture on SQLite.
        src = sqlite3.connect(
            f"{baseline_db_path.resolve().as_uri()}?mode=ro&immutable=1",
            uri=True,
        )
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


def json_request(base, path, payload=None, headers=None, method=None, timeout=5):
    data = None if payload is None else json.dumps(payload).encode("utf-8")
    return request(base, path, data, "application/json" if data is not None else None,
                   headers=headers, method=method, timeout=timeout)


def login_session(base, username, password):
    status, body, cookie = json_request(base, "/auth/login", {
        "username": username, "password": password,
    })
    assert status == 200, (username, status, body)
    result = json.loads(body)
    return {"Cookie": cookie.split(";")[0], "X-CSRF-Token": result["csrf_token"]}, result


def test_user_management(binary):
    print("Testing Users CRUD, optimistic CAS, roles, session revocation and last Admin...")
    with TemporaryDirectory(prefix="opendash-users-") as folder:
        db = Path(folder) / "cms.sqlite"
        with running(binary, db) as base:
            assert json_request(base, "/auth/setup", {
                "username": "adminroot", "password": "root-password-1",
            })[0] == 201
            admin, admin_info = login_session(base, "adminroot", "root-password-1")
            root_id = admin_info["user"]["id"]
            assert request(base, "/api/admin/users")[0] == 401
            assert json_request(base, "/api/admin/users", {
                "username": "intruder", "password": "password123",
            })[0] == 401
            status, body, _ = request(base, "/api/admin/users", headers={"Cookie": admin["Cookie"]})
            assert status == 200, (status, body)
            page = json.loads(body)
            assert page["total"] == 1 and page["items"][0]["version"] == 1
            assert set(page["items"][0]) == {"id", "username", "role", "version"}
            assert request(base, "/api/admin/users?limit=0", headers=admin)[0] == 400
            assert request(base, "/api/admin/users?offset=-1", headers=admin)[0] == 400

            # Existing POST without 'role' must continue to create Editor.
            editor_body = {"username": "editorone", "password": "editor-password-1"}
            status, body, _ = json_request(base, "/api/admin/users", editor_body, admin)
            assert status == 201, (status, body)
            editor = json.loads(body)
            assert editor["role"] == "editor" and editor["version"] == 1
            assert json_request(base, "/api/admin/users", editor_body, admin)[0] == 409
            editor_auth, _ = login_session(base, "editorone", "editor-password-1")
            assert request(base, "/api/admin/users", headers=editor_auth)[0] == 403
            assert json_request(base, "/api/admin/users", {
                "username": "blockeduser", "password": "password123",
            }, editor_auth)[0] == 403

            update_path = f"/api/admin/users/{root_id}"
            ifmatch = {**admin, "If-Match": '"v1"'}
            assert json_request(base, update_path, {"role": "editor"}, admin, "PATCH")[0] == 428
            assert json_request(base, update_path, {"role": "editor"},
                                {**admin, "If-Match": "v1"}, "PATCH")[0] == 400
            assert json_request(base, update_path, {"role": "editor"},
                                {**admin, "If-Match": '"v99"'}, "PATCH")[0] == 412
            assert json_request(base, update_path, {"role": "editor"}, ifmatch, "PATCH")[0] == 409
            assert request(base, update_path, headers=ifmatch, method="DELETE")[0] == 409
            assert request(base, "/auth/me", headers=admin)[0] == 200
            assert json_request(base, update_path, {"role": "admin"}, ifmatch, "PATCH")[0] == 200
            assert request(base, "/auth/me", headers=admin)[0] == 200
            assert json_request(base, "/api/admin/users/999999", {"role": "admin"},
                                ifmatch, "PATCH")[0] == 404
            assert request(base, "/api/admin/users/999999", headers=ifmatch, method="DELETE")[0] == 404
            for invalid_id in ("0", "-1"):
                assert json_request(base, f"/api/admin/users/{invalid_id}", {"role": "admin"},
                                    ifmatch, "PATCH")[0] == 400
                assert json_request(base, f"/api/admin/users/{invalid_id}/password",
                                    {"password": "valid-password-1"}, ifmatch)[0] == 400
                assert request(base, f"/api/admin/users/{invalid_id}", headers=ifmatch,
                               method="DELETE")[0] == 400
            assert json_request(base, update_path, {"role": "admin"},
                                {**ifmatch, "Origin": "http://evil.example"}, "PATCH")[0] == 403
            assert json_request(base, update_path, {"role": "admin"},
                                {"Cookie": admin["Cookie"], "If-Match": '"v1"'}, "PATCH")[0] == 403

            # Reset revokes every existing session, and the old hash never appears in DTOs.
            editor_path = f"/api/admin/users/{editor['id']}"
            reset_path = editor_path + "/password"
            assert json_request(base, reset_path, {"password": "next-password-1"}, admin)[0] == 428
            assert json_request(base, reset_path, {"password": "next-password-1"}, ifmatch)[0] == 200
            assert request(base, "/auth/me", headers=editor_auth)[0] == 401
            assert json_request(base, "/auth/login", editor_body)[0] == 401
            editor_auth, _ = login_session(base, "editorone", "next-password-1")
            assert request(base, editor_path, headers={**admin, "If-Match": '"v1"'}, method="DELETE")[0] == 412
            assert request(base, editor_path, headers={**admin, "If-Match": '"v2"'}, method="DELETE")[0] == 204
            assert request(base, "/auth/me", headers=editor_auth)[0] == 401
            assert json_request(base, "/auth/login", {
                "username": "editorone", "password": "next-password-1",
            })[0] == 401
            assert json_request(base, "/api/admin/users", {
                "username": "adminsecond", "password": "second-password-1", "role": "admin",
            }, admin)[0] == 201
            second, _ = login_session(base, "adminsecond", "second-password-1")
            # Self-change revokes actor's own session, preserving the other Admin.
            status, body, _ = json_request(base, update_path, {"username": "adminrenamed"}, ifmatch, "PATCH")
            assert status == 200, (status, body)
            assert json.loads(body)["version"] == 2
            assert request(base, "/auth/me", headers=admin)[0] == 401
            assert json_request(base, "/auth/login", {
                "username": "adminroot", "password": "root-password-1",
            })[0] == 401
            new_root, _ = login_session(base, "adminrenamed", "root-password-1")
            assert request(base, "/auth/me", headers=second)[0] == 200

        # Restart retains users/roles/version and existing valid sessions.
        with running(binary, db) as base:
            assert request(base, "/auth/me", headers=new_root)[0] == 200
            assert request(base, "/auth/me", headers=second)[0] == 200
            status, body, _ = request(base, "/api/admin/users?limit=1&offset=1", headers=new_root)
            assert status == 200 and json.loads(body)["total"] == 2
            assert len(json.loads(body)["items"]) == 1
        with closing(sqlite3.connect(db)) as con:
            assert con.execute("SELECT version FROM users WHERE username='adminrenamed'").fetchone() == (2,)
            assert con.execute("SELECT COUNT(*) FROM users WHERE role='admin'").fetchone() == (2,)


def test_user_mutation_races(binary):
    print("Testing concurrent Admin demotion/delete and login/password reset...")
    with TemporaryDirectory(prefix="opendash-user-race-") as folder:
        db = Path(folder) / "cms.sqlite"
        with running(binary, db) as base:
            assert json_request(base, "/auth/setup", {
                "username": "adminone", "password": "first-password-1",
            })[0] == 201
            first, info = login_session(base, "adminone", "first-password-1")
            first_id = info["user"]["id"]
            st, body, _ = json_request(base, "/api/admin/users", {
                "username": "admintwo", "password": "second-password-1", "role": "admin",
            }, first)
            assert st == 201, (st, body)
            second_id = json.loads(body)["id"]
            second, _ = login_session(base, "admintwo", "second-password-1")
            barrier = Barrier(2)

            def demote(id, auth):
                barrier.wait(timeout=10)
                return json_request(base, f"/api/admin/users/{id}", {"role": "editor"},
                                    {**auth, "If-Match": '"v1"'}, "PATCH")[0]

            with ThreadPoolExecutor(max_workers=2) as pool:
                a = pool.submit(demote, first_id, first)
                b = pool.submit(demote, second_id, second)
                statuses = sorted([a.result(), b.result()])
            assert statuses == [200, 409], f"Concurrent demotions must preserve an Admin: {statuses}"
            with closing(sqlite3.connect(db)) as con:
                remaining = con.execute("SELECT id FROM users WHERE role='admin'").fetchall()
            assert len(remaining) == 1
            survivor_id = remaining[0][0]
            survivor_auth = first if survivor_id == first_id else second
            survivor_name = "adminone" if survivor_id == first_id else "admintwo"
            survivor_pwd = "first-password-1" if survivor_id == first_id else "second-password-1"
            assert request(base, "/auth/me", headers=survivor_auth)[0] == 200
            login_session(base, survivor_name, survivor_pwd)

            # Give the last Admin another Admin to reset. Try old-password
            # logins while a password reset is hashing; any sessions issued
            # before reset commit must be revoked by its transaction.
            st, body, _ = json_request(base, "/api/admin/users", {
                "username": "racetarget", "password": "original-race-pass", "role": "admin",
            }, survivor_auth)
            assert st == 201, (st, body)
            target_id = json.loads(body)["id"]
            old_auth, _ = login_session(base, "racetarget", "original-race-pass")
            barrier2 = Barrier(2)

            def reset():
                barrier2.wait(timeout=10)
                return json_request(base, f"/api/admin/users/{target_id}/password",
                                    {"password": "updated-race-pass"},
                                    {**survivor_auth, "If-Match": '"v1"'})[0]

            def login_burst():
                barrier2.wait(timeout=10)
                results = []
                for _ in range(3):
                    results.append(json_request(base, "/auth/login", {
                        "username": "racetarget", "password": "original-race-pass",
                    }))
                return results

            with ThreadPoolExecutor(max_workers=2) as pool:
                reset_future = pool.submit(reset)
                login_future = pool.submit(login_burst)
                reset_status, attempts = reset_future.result(), login_future.result()
            assert reset_status == 200, reset_status
            assert request(base, "/auth/me", headers=old_auth)[0] == 401
            for status, body, cookie in attempts:
                assert status in (200, 401), (status, body)
                if status == 200:
                    assert request(base, "/auth/me", headers={"Cookie": cookie.split(";")[0]})[0] == 401
            assert json_request(base, "/auth/login", {
                "username": "racetarget", "password": "original-race-pass",
            })[0] == 401
            target_auth, _ = login_session(base, "racetarget", "updated-race-pass")
            assert request(base, "/auth/me", headers=survivor_auth)[0] == 200
            delete_barrier = Barrier(2)

            def delete_self(id, version, auth):
                delete_barrier.wait(timeout=10)
                return request(
                    base, f"/api/admin/users/{id}",
                    headers={**auth, "If-Match": f'"v{version}"'}, method="DELETE",
                )[0]

            with ThreadPoolExecutor(max_workers=2) as pool:
                one = pool.submit(delete_self, survivor_id, 1, survivor_auth)
                two = pool.submit(delete_self, target_id, 2, target_auth)
                statuses = sorted([one.result(), two.result()])
            assert statuses == [204, 409], f"Concurrent Admin deletes must preserve one Admin: {statuses}"
            with closing(sqlite3.connect(db)) as con:
                admin_ids = [row[0] for row in con.execute("SELECT id FROM users WHERE role='admin'")]
            assert len(admin_ids) == 1
            remaining_auth = survivor_auth if admin_ids[0] == survivor_id else target_auth
            assert request(base, "/auth/me", headers=remaining_auth)[0] == 200


def test_users_write_lock_conflict(binary):
    print("Testing SQLite write reservation conflict maps to retryable HTTP 503...")
    with TemporaryDirectory(prefix="opendash-users-lock-") as folder:
        db = Path(folder) / "cms.sqlite"
        with running(binary, db) as base:
            assert json_request(base, "/auth/setup", {
                "username": "lockadmin", "password": "lock-password-1",
            })[0] == 201
            admin, _ = login_session(base, "lockadmin", "lock-password-1")
            with closing(sqlite3.connect(db)) as connection:
                connection.execute("BEGIN IMMEDIATE")
                try:
                    status, body, _ = json_request(
                        base, "/api/admin/users",
                        {"username": "blockeduser", "password": "valid-password-1"},
                        admin, timeout=12,
                    )
                    assert status == 503, (status, body)
                    assert request(base, "/auth/me", headers=admin)[0] == 200
                finally:
                    connection.rollback()
            status, body, _ = request(base, "/api/admin/users", headers=admin)
            assert status == 200
            assert all(user["username"] != "blockeduser" for user in json.loads(body)["items"])


def test_user_schema_upgrade(binary, baseline_binary: Path | None = None):
    print("Testing upgrade from pre-0009 users and sessions without changing hashes or cookies...")
    with TemporaryDirectory(prefix="opendash-users-upgrade-") as folder:
        db = Path(folder) / "cms.sqlite"
        # Prefer a genuine pre-0009 binary. A current binary also installs later
        # migrations (e.g. Settings 0010), so dropping only users.version would
        # produce an impossible snapshot that the old SQLx migrator must refuse.
        fixture_binary = baseline_binary if baseline_binary is not None else binary
        with running(fixture_binary, db) as base:
            assert json_request(base, "/auth/setup", {
                "username": "upgradeadmin", "password": "upgrade-password",
            })[0] == 201
            admin, _ = login_session(base, "upgradeadmin", "upgrade-password")
            st, body, _ = json_request(base, "/api/admin/users", {
                "username": "upgradeeditor", "password": "editor-password",
            }, admin)
            assert st == 201, (st, body)
            editor, _ = login_session(base, "upgradeeditor", "editor-password")
        # When the old binary is available it already produced an authentic
        # pre-0009 fixture. Otherwise, support only the explicitly understood
        # 0009/0010 test-only downgrade, refusing unknown newer migrations.
        # No production site, migration source or committed fixture is touched.
        with closing(sqlite3.connect(db)) as con:
            with con:
                before_users = con.execute(
                    "SELECT id, username, password_hash, role, created_at FROM users ORDER BY id",
                ).fetchall()
                before_sessions = con.execute(
                    "SELECT token_hash, user_id, csrf_token, expires_at, created_at "
                    "FROM sessions ORDER BY user_id",
                ).fetchall()
                newer_versions = [row[0] for row in con.execute(
                    "SELECT version FROM _sqlx_migrations WHERE version > 8 ORDER BY version",
                )]
                if baseline_binary is not None:
                    assert not newer_versions, f"Old binary created newer migrations: {newer_versions}"
                else:
                    assert newer_versions in ([9], [9, 10]), (
                        f"Unknown schema versions {newer_versions}; use an actual pre-0009 binary"
                    )
                    if 10 in newer_versions:
                        con.execute("DROP TABLE site_general_settings")
                    con.execute("ALTER TABLE users DROP COLUMN version")
                    con.execute("DELETE FROM _sqlx_migrations WHERE version IN (9, 10)")
            assert con.execute("PRAGMA table_info(users)").fetchall()
            assert not any(c[1] == "version" for c in con.execute("PRAGMA table_info(users)"))
            assert not any(row[0] > 8 for row in con.execute(
                "SELECT version FROM _sqlx_migrations",
            )), "Pre-0009 fixture must not contain later migrations"
            assert con.execute(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='site_general_settings'",
            ).fetchone() == (0,)
        # SQLite online backup creates a pre-migration snapshot for rollback.
        baseline_db = Path(folder) / "snapshot_pre0009.sqlite"
        with closing(sqlite3.connect(db)) as source, closing(sqlite3.connect(baseline_db)) as dest:
            source.backup(dest)
        with running(binary, db) as base:
            assert request(base, "/auth/me", headers=admin)[0] == 200
            assert request(base, "/auth/me", headers=editor)[0] == 200
            status, page, _ = request(base, "/api/admin/users", headers=admin)
            assert status == 200 and all(item["version"] == 1 for item in json.loads(page)["items"])
            login_session(base, "upgradeadmin", "upgrade-password")
            login_session(base, "upgradeeditor", "editor-password")
        with closing(sqlite3.connect(db)) as con:
            assert con.execute(
                "SELECT id, username, password_hash, role, created_at FROM users ORDER BY id",
            ).fetchall() == before_users
            for original in before_sessions:
                assert con.execute(
                    "SELECT token_hash, user_id, csrf_token, expires_at, created_at "
                    "FROM sessions WHERE token_hash = ?", (original[0],),
                ).fetchone() == original
            assert con.execute("PRAGMA foreign_key_check").fetchall() == []
            assert con.execute("SELECT COUNT(*) FROM _sqlx_migrations WHERE version=9").fetchone() == (1,)
        # Rollback is restoring the older schema snapshot, not running a
        # backwards migration. A pre-0009 binary can read this snapshot.
        with closing(sqlite3.connect(baseline_db)) as con:
            assert not any(c[1] == "version" for c in con.execute("PRAGMA table_info(users)"))
            assert con.execute("SELECT COUNT(*) FROM users").fetchone() == (2,)
            assert con.execute("SELECT COUNT(*) FROM _sqlx_migrations WHERE version=9").fetchone() == (0,)
        if baseline_binary is not None:
            print("Verifying restored snapshot using the unmodified baseline binary...")
            with running(baseline_binary, baseline_db) as base:
                assert request(base, "/auth/me", headers=admin)[0] == 200
                assert request(base, "/auth/me", headers=editor)[0] == 200
                login_session(base, "upgradeadmin", "upgrade-password")
                login_session(base, "upgradeeditor", "editor-password")
            with closing(sqlite3.connect(baseline_db)) as con:
                assert not any(c[1] == "version" for c in con.execute("PRAGMA table_info(users)"))
                assert con.execute("SELECT COUNT(*) FROM users").fetchone() == (2,)


def verify_all(binary, baseline_db: Path, baseline_binary: Path | None = None):
    test_concurrent_setup(binary)
    test_argon2_burst_and_public_responsiveness(binary)
    test_origin_validation(binary)
    test_auth_and_rbac(binary)
    test_database_upgrade(binary, baseline_db)
    test_user_management(binary)
    test_user_mutation_races(binary)
    test_users_write_lock_conflict(binary)
    test_user_schema_upgrade(binary, baseline_binary)
    print("ALL M1.1 AUTH, RBAC, CSRF, EXPIRY, RACE, ORIGIN, AND UPGRADE TESTS PASSED!")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/opendash.exe"))
    parser.add_argument("--baseline-db", type=Path, default=Path("fixtures/baseline_0001.sqlite"))
    parser.add_argument("--baseline-binary", type=Path, help="Optional pre-0009 binary to open the restored snapshot")
    args = parser.parse_args()
    verify_all(
        args.binary.resolve(), args.baseline_db.resolve(),
        args.baseline_binary.resolve() if args.baseline_binary else None,
    )
