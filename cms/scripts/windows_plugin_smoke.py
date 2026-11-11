"""End-to-end smoke tests for OpenDash M2 Plugin System:
- Wasmi runtime host integration
- Plugin ZIP package validation (traversal, missing files, invalid manifest)
- RBAC enforcement (Editor receives 403, Unauthenticated receives 401)
- CSRF & Origin protection
- AssemblyScript sample plugin installation & execution (JSON in -> KV store -> JSON out)
- SQLite KV persistence across executions and server restarts
- Plugin disable/enable recovery lifecycle
"""

import argparse
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


@contextmanager
def running(binary, database, allow_loopback=True):
    env = dict(
        os.environ,
        OPENDASH_PORT="0",
        OPENDASH_SITE_ROOT=str(database.parent), OPENDASH_DB_PATH=str(database),
        OPENDASH_PLUGIN_ALLOW_LOOPBACK="1" if allow_loopback else "0",
    )
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
        with urlopen(req, timeout=10) as response:
            set_cookie = response.headers.get("Set-Cookie")
            return response.status, response.read().decode("utf-8"), set_cookie
    except HTTPError as error:
        with error:
            set_cookie = error.headers.get("Set-Cookie")
            return error.code, error.read().decode("utf-8"), set_cookie


def make_bad_zip(filename_in_zip, content_bytes):
    archive = io.BytesIO()
    with ZipFile(archive, "w", ZIP_DEFLATED) as z:
        z.writestr(filename_in_zip, content_bytes)
    return archive.getvalue()


def test_plugin_rbac_and_csrf(binary):
    print("Testing Plugin RBAC, CSRF, and Origin enforcement...")
    with TemporaryDirectory(prefix="opendash-plugin-rbac-") as folder:
        database = Path(folder) / "cms.sqlite"
        with running(binary, database) as base:
            # 1. Unauthenticated checks
            status, _, _ = request(base, "/api/admin/plugins")
            assert status == 401, f"Expected 401 for unauthenticated GET /api/admin/plugins, got {status}"

            status, _, _ = request(base, "/api/admin/plugins", b"dummy", "application/zip")
            assert status == 401, f"Expected 401 for unauthenticated POST /api/admin/plugins, got {status}"

            # 2. Setup Admin
            status, _, _ = request(
                base,
                "/auth/setup",
                json.dumps({"username": "admin", "password": "adminpassword"}).encode(),
                "application/json",
            )
            assert status == 201

            # Login Admin
            status, body, cookie = request(
                base,
                "/auth/login",
                json.dumps({"username": "admin", "password": "adminpassword"}).encode(),
                "application/json",
            )
            assert status == 200
            admin_csrf = json.loads(body)["csrf_token"]
            admin_cookie = cookie.split(";")[0]

            # 3. Create Editor
            status, body, _ = request(
                base,
                "/api/admin/users",
                json.dumps({"username": "editor1", "password": "editorpassword"}).encode(),
                "application/json",
                headers={"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf},
            )
            assert status == 201

            # Login Editor
            status, body, cookie = request(
                base,
                "/auth/login",
                json.dumps({"username": "editor1", "password": "editorpassword"}).encode(),
                "application/json",
            )
            assert status == 200
            editor_csrf = json.loads(body)["csrf_token"]
            editor_cookie = cookie.split(";")[0]

            # 4. Editor must receive 403 Forbidden on all plugin endpoints
            status, _, _ = request(
                base,
                "/api/admin/plugins",
                headers={"Cookie": editor_cookie},
            )
            assert status == 403, f"Expected 403 for Editor GET /api/admin/plugins, got {status}"

            status, _, _ = request(
                base,
                "/api/admin/plugins",
                b"PK...",
                "application/zip",
                headers={"Cookie": editor_cookie, "X-CSRF-Token": editor_csrf},
            )
            assert status == 403, f"Expected 403 for Editor POST /api/admin/plugins, got {status}"

            status, _, _ = request(
                base,
                "/api/admin/plugins/sample-plugin/run",
                b"{}",
                "application/json",
                headers={"Cookie": editor_cookie, "X-CSRF-Token": editor_csrf},
            )
            assert status == 403, f"Expected 403 for Editor POST /api/admin/plugins/run, got {status}"

            for action in ("enable", "disable"):
                status, _, _ = request(
                    base,
                    f"/api/admin/plugins/sample-plugin/{action}",
                    b"",
                    "application/json",
                    headers={"Cookie": editor_cookie, "X-CSRF-Token": editor_csrf},
                )
                assert status == 403, f"Editor must not {action} plugins, got {status}"

            # 5. Admin CSRF failure
            status, _, _ = request(
                base,
                "/api/admin/plugins",
                b"PK...",
                "application/zip",
                headers={"Cookie": admin_cookie, "X-CSRF-Token": "wrong-token"},
            )
            assert status == 403, f"Expected 403 for invalid CSRF token, got {status}"

            # 6. Admin Origin spoofing failure
            status, _, _ = request(
                base,
                "/api/admin/plugins",
                b"PK...",
                "application/zip",
                headers={
                    "Cookie": admin_cookie,
                    "X-CSRF-Token": admin_csrf,
                    "Origin": "http://evil-attacker.com",
                },
            )
            assert status == 403, f"Expected 403 for spoofed Origin, got {status}"

    print("RBAC, CSRF, and Origin checks passed!")


def test_plugin_package_validation(binary):
    print("Testing Plugin ZIP package validation...")
    with TemporaryDirectory(prefix="opendash-plugin-val-") as folder:
        database = Path(folder) / "cms.sqlite"
        with running(binary, database) as base:
            # Setup & Login Admin
            request(
                base,
                "/auth/setup",
                json.dumps({"username": "admin", "password": "adminpassword"}).encode(),
                "application/json",
            )
            _, body, cookie = request(
                base,
                "/auth/login",
                json.dumps({"username": "admin", "password": "adminpassword"}).encode(),
                "application/json",
            )
            admin_csrf = json.loads(body)["csrf_token"]
            admin_cookie = cookie.split(";")[0]

            # 1. Non-ZIP content
            status, resp, _ = request(
                base,
                "/api/admin/plugins",
                b"this is plain text, not a zip",
                "application/zip",
                headers={"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf},
            )
            assert status == 400, f"Expected 400 for invalid ZIP, got {status}"

            # 2. Path traversal in ZIP
            traversal_zip = make_bad_zip("../evil.wasm", b"\x00asm\x01\x00\x00\x00")
            status, resp, _ = request(
                base,
                "/api/admin/plugins",
                traversal_zip,
                "application/zip",
                headers={"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf},
            )
            assert status == 400, f"Expected 400 for path traversal ZIP, got {status}"
            assert "path traversal" in resp or "không an toàn" in resp

            # 3. Missing manifest.json
            missing_manifest_zip = make_bad_zip("plugin.wasm", b"\x00asm\x01\x00\x00\x00")
            status, resp, _ = request(
                base,
                "/api/admin/plugins",
                missing_manifest_zip,
                "application/zip",
                headers={"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf},
            )
            assert status == 400, f"Expected 400 for missing manifest, got {status}"
            assert "manifest.json" in resp

            # 4. Missing plugin.wasm
            missing_wasm_zip = make_bad_zip(
                "manifest.json",
                json.dumps({
                    "id": "test-plugin",
                    "version": "1.0.0",
                    "api_version": 1,
                    "name": "Test",
                    "permissions": {},
                }).encode(),
            )
            status, resp, _ = request(
                base,
                "/api/admin/plugins",
                missing_wasm_zip,
                "application/zip",
                headers={"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf},
            )
            assert status == 400, f"Expected 400 for missing wasm, got {status}"
            assert "plugin.wasm" in resp

            # 5. Invalid api_version (not 1)
            archive = io.BytesIO()
            with ZipFile(archive, "w", ZIP_DEFLATED) as z:
                z.writestr(
                    "manifest.json",
                    json.dumps({
                        "id": "test-plugin",
                        "version": "1.0.0",
                        "api_version": 99,
                        "name": "Test",
                        "permissions": {},
                    }),
                )
                z.writestr("plugin.wasm", b"\x00asm\x01\x00\x00\x00")
            status, resp, _ = request(
                base,
                "/api/admin/plugins",
                archive.getvalue(),
                "application/zip",
                headers={"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf},
            )
            assert status == 400, f"Expected 400 for invalid api_version, got {status}"
            assert "api_version 1" in resp

    print("Package validation checks passed!")


def test_sample_plugin_full_lifecycle(binary):
    print("Testing AssemblyScript sample plugin installation, execution, KV persistence, and restart...")
    sample_zip_path = Path("examples/plugins/sample-plugin/sample-plugin.zip")
    assert sample_zip_path.exists(), f"Sample plugin ZIP not found at {sample_zip_path}"
    sample_zip_bytes = sample_zip_path.read_bytes()

    with TemporaryDirectory(prefix="opendash-plugin-e2e-") as folder:
        database = Path(folder) / "cms.sqlite"

        # --- Phase 1: Install & Execute ---
        with running(binary, database) as base:
            # Setup & Login Admin
            request(
                base,
                "/auth/setup",
                json.dumps({"username": "admin", "password": "adminpassword"}).encode(),
                "application/json",
            )
            _, body, cookie = request(
                base,
                "/auth/login",
                json.dumps({"username": "admin", "password": "adminpassword"}).encode(),
                "application/json",
            )
            admin_csrf = json.loads(body)["csrf_token"]
            admin_cookie = cookie.split(";")[0]

            # Install plugin
            status, body, _ = request(
                base,
                "/api/admin/plugins",
                sample_zip_bytes,
                "application/zip",
                headers={"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf},
            )
            assert status == 201, f"Expected 201 for plugin install, got {status}: {body}"
            installed_data = json.loads(body)
            assert installed_data["id"] == "sample-plugin"
            assert installed_data["enabled"] is True

            # List plugins
            status, body, _ = request(
                base,
                "/api/admin/plugins",
                headers={"Cookie": admin_cookie},
            )
            assert status == 200
            plugins_list = json.loads(body)
            assert len(plugins_list) == 1
            assert plugins_list[0]["id"] == "sample-plugin"
            assert plugins_list[0]["permissions"]["sqlite"] == ["kv_read", "kv_write"]

            # A rejected ZIP cannot replace a working installed plugin.
            status, _, _ = request(
                base, "/api/admin/plugins", b"not a zip", "application/zip",
                headers={"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf},
            )
            assert status == 400
            status, body, _ = request(base, "/api/admin/plugins", headers={"Cookie": admin_cookie})
            assert status == 200
            assert len(json.loads(body)) == 1 and json.loads(body)[0]["enabled"] is True

            # Run plugin invocation 1
            input_payload1 = json.dumps({"text": "antigravity m2 test"}).encode()
            status, body, _ = request(
                base,
                "/api/admin/plugins/sample-plugin/run",
                input_payload1,
                "application/json",
                headers={"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf},
            )
            assert status == 200, f"Expected 200 for run 1, got {status}: {body}"
            run1_resp = json.loads(body)
            assert run1_resp["success"] is True, run1_resp
            assert run1_resp["data"]["status"] == "ok"
            assert run1_resp["data"]["transformed"] == "ANTIGRAVITY M2 TEST"
            assert run1_resp["data"]["run_count"] == 1
            assert "correlation_id" in run1_resp

            # Run plugin invocation 2
            input_payload2 = json.dumps({"text": "wasmi runtime host"}).encode()
            status, body, _ = request(
                base,
                "/api/admin/plugins/sample-plugin/run",
                input_payload2,
                "application/json",
                headers={"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf},
            )
            assert status == 200
            run2_resp = json.loads(body)
            assert run2_resp["success"] is True
            assert run2_resp["data"]["transformed"] == "WASMI RUNTIME HOST"
            assert run2_resp["data"]["run_count"] == 2

            # Run plugin invocation 3 with complex JSON: escaped quotes, newlines, backslashes, unicode
            complex_str = 'probe "quote" and \n newline \\ backslash \u0041'
            input_payload3 = json.dumps({"text": complex_str, "extra": {"a": [1, 2]}}).encode()
            status, body, _ = request(
                base,
                "/api/admin/plugins/sample-plugin/run",
                input_payload3,
                "application/json",
                headers={"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf},
            )
            assert status == 200, f"Expected 200 for run 3 (complex JSON), got {status}: {body}"
            run3_resp = json.loads(body)
            assert run3_resp["success"] is True
            assert run3_resp["data"]["transformed"] == complex_str.upper()
            assert run3_resp["data"]["run_count"] == 3

            # Disable plugin
            status, body, _ = request(
                base,
                "/api/admin/plugins/sample-plugin/disable",
                b"",
                "application/json",
                headers={"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf},
            )
            assert status == 200, f"Expected 200 for disable, got {status}"

            # Running disabled plugin must fail with 400 code "disabled"
            status, body, _ = request(
                base,
                "/api/admin/plugins/sample-plugin/run",
                input_payload1,
                "application/json",
                headers={"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf},
            )
            assert status == 400, f"Expected 400 for disabled plugin, got {status}"
            disabled_resp = json.loads(body)
            assert disabled_resp["code"] == "disabled"

            # Enable plugin again
            status, body, _ = request(
                base,
                "/api/admin/plugins/sample-plugin/enable",
                b"",
                "application/json",
                headers={"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf},
            )
            assert status == 200

            # Run after re-enabling
            status, body, _ = request(
                base,
                "/api/admin/plugins/sample-plugin/run",
                input_payload1,
                "application/json",
                headers={"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf},
            )
            assert status == 200
            assert json.loads(body)["data"]["run_count"] == 4

            # Install is replace-and-enable: an existing disabled plugin becomes
            # enabled without losing its persisted plugin KV content.
            status, _, _ = request(
                base, "/api/admin/plugins/sample-plugin/disable", b"", "application/json",
                headers={"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf},
            )
            assert status == 200
            status, body, _ = request(
                base, "/api/admin/plugins", sample_zip_bytes, "application/zip",
                headers={"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf},
            )
            assert status == 201 and json.loads(body)["enabled"] is True
            status, body, _ = request(
                base, "/api/admin/plugins/sample-plugin/run", input_payload1, "application/json",
                headers={"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf},
            )
            assert status == 200 and json.loads(body)["data"]["run_count"] == 5

        # --- Phase 2: Server Restart Persistence ---
        print("Verifying persistence across server restart...")
        with running(binary, database) as base:
            # Login Admin on restarted server
            status, body, cookie = request(
                base,
                "/auth/login",
                json.dumps({"username": "admin", "password": "adminpassword"}).encode(),
                "application/json",
            )
            assert status == 200
            admin_csrf = json.loads(body)["csrf_token"]
            admin_cookie = cookie.split(";")[0]

            # Verify plugin is still listed and enabled
            status, body, _ = request(
                base,
                "/api/admin/plugins",
                headers={"Cookie": admin_cookie},
            )
            assert status == 200
            plugins_list = json.loads(body)
            assert len(plugins_list) == 1
            assert plugins_list[0]["id"] == "sample-plugin"
            assert plugins_list[0]["enabled"] is True

            # Run plugin: run_count must be 6, confirming replacement and restart preserved KV.
            input_payload = json.dumps({"text": "after server restart"}).encode()
            status, body, _ = request(
                base,
                "/api/admin/plugins/sample-plugin/run",
                input_payload,
                "application/json",
                headers={"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf},
            )
            assert status == 200
            run_after_restart = json.loads(body)
            assert run_after_restart["success"] is True
            assert run_after_restart["data"]["transformed"] == "AFTER SERVER RESTART"
            assert run_after_restart["data"]["run_count"] == 6, (
                f"Expected run_count 6 after restart, got {run_after_restart['data']['run_count']}"
            )

        # Check SQLite DB directly
        conn = sqlite3.connect(database)
        cur = conn.cursor()
        cur.execute("SELECT value_json FROM plugin_kv WHERE plugin_id = 'sample-plugin' AND key = 'run_count'")
        row = cur.fetchone()
        assert row is not None and row[0] == "6", f"Expected SQLite KV '6', got {row}"
        conn.close()

    print("Sample plugin lifecycle and restart persistence verified successfully!")


def test_body_limits_and_boundaries(binary):
    print("Testing payload body limits and boundaries (Plugin ZIP 5MB vs Theme ZIP 64KB, JSON 1MB)...")
    with TemporaryDirectory(prefix="opendash-limits-") as folder:
        database = Path(folder) / "cms.sqlite"
        with running(binary, database) as base:
            # Setup admin
            status, _, _ = request(
                base,
                "/auth/setup",
                json.dumps({"username": "admin", "password": "adminpassword"}).encode(),
                "application/json",
            )
            assert status == 201

            # Login admin
            status, body, cookie = request(
                base,
                "/auth/login",
                json.dumps({"username": "admin", "password": "adminpassword"}).encode(),
                "application/json",
            )
            assert status == 200
            admin_cookie = cookie.split(";")[0]
            admin_csrf = json.loads(body)["csrf_token"]
            headers = {"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf}

            # 1. Plugin ZIP limits:
            # Case A: Plugin ZIP 111 KB (111,005 bytes: sample plugin wasm + manifest + large README.md)
            sample_zip = Path("examples/plugins/sample-plugin/sample-plugin.zip")
            assert sample_zip.exists(), "sample-plugin.zip must exist"
            with ZipFile(sample_zip, "r") as z_in:
                manifest_data = z_in.read("manifest.json")
                wasm_data = z_in.read("plugin.wasm")

            import base64

            archive_111k = io.BytesIO()
            with ZipFile(archive_111k, "w", ZIP_DEFLATED) as z:
                z.writestr("manifest.json", manifest_data)
                z.writestr("plugin.wasm", wasm_data)
                # Random base64 text does not compress, reaching ~111,000 bytes
                z.writestr("README.md", base64.b64encode(os.urandom(70000)).decode("ascii"))
            zip_111k_bytes = archive_111k.getvalue()
            assert len(zip_111k_bytes) > 65536, f"Expected >64KB, got {len(zip_111k_bytes)}"

            status, body, _ = request(
                base,
                "/api/admin/plugins",
                zip_111k_bytes,
                "application/zip",
                headers=headers,
            )
            assert status == 201, f"Expected 201 for 111KB plugin ZIP (limit 5MB), got {status}: {body}"

            # Case B: Plugin ZIP > 5 MiB (rejected with 413)
            huge_zip_bytes = b"PK\x05\x06" + b"\x00" * (5 * 1024 * 1024 + 1024)
            status, body, _ = request(
                base,
                "/api/admin/plugins",
                huge_zip_bytes,
                "application/zip",
                headers=headers,
            )
            assert status == 413, f"Expected 413 for plugin ZIP > 5MB, got {status}: {body}"

            # 2. Plugin JSON input limits:
            # Case A: JSON input ~68 KB (> 64KB theme limit, but < 1MB plugin limit)
            json_70k_bytes = json.dumps({"text": "hello"}).encode()[:-1] + b" " * 68000 + b"}"
            assert len(json_70k_bytes) > 65536
            status, body, _ = request(
                base,
                "/api/admin/plugins/sample-plugin/run",
                json_70k_bytes,
                "application/json",
                headers=headers,
            )
            assert status == 200, f"Expected 200 for 70KB JSON input (limit 1MB), got {status}: {body}"
            res_70k = json.loads(body)
            assert res_70k["success"] is True

            # Case B: JSON input > 1 MiB (rejected with 413)
            huge_text = "A" * (1024 * 1024 + 2048)
            json_huge_bytes = json.dumps({"text": huge_text}).encode()
            status, body, _ = request(
                base,
                "/api/admin/plugins/sample-plugin/run",
                json_huge_bytes,
                "application/json",
                headers=headers,
            )
            assert status == 413, f"Expected 413 for JSON input > 1MB, got {status}: {body}"

            # 3. Theme ZIP limit (64 KiB):
            # Case A: Theme ZIP > 64 KiB (rejected with 413)
            huge_theme_bytes = b"PK\x05\x06" + b"\x00" * (70 * 1024)
            status, body, _ = request(
                base,
                "/theme",
                huge_theme_bytes,
                "application/zip",
                headers=headers,
            )
            assert status == 413, f"Expected 413 for theme ZIP > 64KB, got {status}: {body}"

    print("Payload body limits and boundary separation verified successfully!")


def main():
    parser = argparse.ArgumentParser(description="OpenDash M2 Plugin Smoke Tests")
    parser.add_argument(
        "--binary",
        default="target/debug/opendash.exe",
        help="Path to opendash executable",
    )
    args = parser.parse_args()
    binary = Path(args.binary).resolve()
    assert binary.exists(), f"Binary {binary} not found. Run cargo build first."

    test_plugin_rbac_and_csrf(binary)
    test_plugin_package_validation(binary)
    test_sample_plugin_full_lifecycle(binary)
    test_body_limits_and_boundaries(binary)
    print("\nALL M2 PLUGIN SMOKE TESTS PASSED!")


if __name__ == "__main__":
    main()
