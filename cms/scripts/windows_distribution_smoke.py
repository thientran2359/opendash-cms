"""Verification of OpenDash single Windows release binary distribution (M1.3).

Executes an isolated opendash.exe in a clean temporary directory with:
  1. CWD set to the isolated folder (no admin/, no src/, no repo files)
  2. PATH stripped of Node.js, npm, and Rust/Cargo
  3. Verification that Admin HTML and all referenced assets load correctly with exact MIME types
  4. Verification that unknown assets return 404 NOT FOUND (never the HTML shell)
  5. Full auth, RBAC, multi-theme install, schema normalization on preview/save (empty inputs & block switching)
  6. Migration and upgrade from baseline database fixture (fixtures/baseline_0001.sqlite)
  7. Fail-fast verification of scripts/build.ps1
  8. Exact binary size measurement
"""

import argparse
from contextlib import contextmanager
import io
import json
import os
from pathlib import Path
import re
import shutil
import sqlite3
import subprocess
import sys
from tempfile import TemporaryDirectory
import time
from urllib.error import HTTPError
from urllib.parse import urlparse
from urllib.request import Request, urlopen
from zipfile import ZIP_DEFLATED, ZipFile


def clean_env_for_distribution() -> dict[str, str]:
    """Return environment with Node, npm, and Rust paths stripped from PATH."""
    env = os.environ.copy()
    current_path = env.get("PATH", "")
    filtered_entries = []
    for entry in current_path.split(os.pathsep):
        entry_lower = entry.lower()
        if any(tool in entry_lower for tool in ["node", "npm", "cargo", "rust"]):
            continue
        filtered_entries.append(entry)
    env["PATH"] = os.pathsep.join(filtered_entries)
    return env


def sample_banner_theme() -> bytes:
    archive = io.BytesIO()
    with ZipFile(archive, "w", ZIP_DEFLATED) as theme:
        theme.writestr(
            "block.json",
            json.dumps({
                "id": "dist/banner",
                "fields": {"headline": "Tiêu đề biểu ngữ", "content": "Nội dung"},
            }),
        )
        theme.writestr(
            "block.html",
            "<section><h1>{{ values.headline }}</h1><p>{{ values.content }}</p></section>",
        )
    return archive.getvalue()


def sample_notice_theme() -> bytes:
    archive = io.BytesIO()
    with ZipFile(archive, "w", ZIP_DEFLATED) as theme:
        theme.writestr(
            "block.json",
            json.dumps({
                "id": "dist/notice",
                "fields": {"alert_msg": "Thông điệp", "level": "Mức độ"},
            }),
        )
        theme.writestr(
            "block.html",
            "<aside class=\"notice {{ values.level }}\"><p>{{ values.alert_msg }}</p></aside>",
        )
    return archive.getvalue()


def http_req(
    base: str,
    path: str,
    data: bytes | None = None,
    content_type: str | None = None,
    headers: dict[str, str] | None = None,
) -> tuple[int, str, dict[str, str], bytes]:
    url = base + path
    hdrs = dict(headers or {})
    if content_type:
        hdrs["Content-Type"] = content_type
    if "Origin" not in hdrs and data is not None:
        parsed = urlparse(base)
        hdrs["Origin"] = f"{parsed.scheme}://{parsed.netloc}"

    req = Request(url, data=data, headers=hdrs)
    try:
        with urlopen(req) as resp:
            body = resp.read()
            resp_headers = {k.lower(): v for k, v in resp.headers.items()}
            return resp.status, body.decode("utf-8", "replace"), resp_headers, body
    except HTTPError as err:
        body = err.read()
        resp_headers = {k.lower(): v for k, v in err.headers.items()}
        return err.code, body.decode("utf-8", "replace"), resp_headers, body


@contextmanager
def running_dist_binary(exe_path: Path, cwd: Path, db_path: Path | None, site_root: Path | None = None):
    env = clean_env_for_distribution()
    env["OPENDASH_PORT"] = "0"
    for key, value in (("OPENDASH_DB_PATH", db_path), ("OPENDASH_SITE_ROOT", site_root)):
        env.pop(key, None)
        if value is not None:
            env[key] = str(value)

    proc = subprocess.Popen(
        [str(exe_path)],
        cwd=str(cwd),
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
    )
    try:
        line = proc.stdout.readline().strip()
        assert line.startswith("OpenDash: "), f"Unexpected output: {line}"
        base = line.removeprefix("OpenDash: ").removesuffix("/builder")
        yield base
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()


def test_distribution_assets_and_404(dist_exe: Path, work_dir: Path):
    print("1. Testing embedded assets serving and strict 404 for unknown paths...")
    db_file = work_dir / "assets_test.sqlite"
    with running_dist_binary(dist_exe, work_dir, db_file) as base:
        # Check /builder HTML
        status, html, headers, _ = http_req(base, "/builder")
        assert status == 200, f"Expected 200 for /builder, got {status}"
        assert "text/html" in headers.get("content-type", "").lower()
        assert headers.get("x-content-type-options") == "nosniff"
        assert '<div id="root">' in html, "Missing React root in /builder"

        for alias in ("/admin", "/admin/", "/builder/"):
            assert http_req(base, alias)[:2] == (200, html)
        health_status, health_json, _, _ = http_req(base, "/api/health")
        assert health_status == 200
        assert json.loads(health_json) == {"status": "ok", "database": "ready", "scaffold": False}

        # Find JS and CSS assets referenced in index.html
        js_matches = re.findall(r'src=["\'](/assets/[^"\']+\.js)["\']', html)
        css_matches = re.findall(r'href=["\'](/assets/[^"\']+\.css)["\']', html)

        assert len(js_matches) > 0, f"No JS assets found in index.html: {html}"
        assert len(css_matches) > 0, f"No CSS assets found in index.html: {html}"

        js_path = js_matches[0]
        css_path = css_matches[0]

        # Verify JS asset
        js_status, js_text, js_headers, js_bytes = http_req(base, js_path)
        assert js_status == 200, f"Expected 200 for {js_path}, got {js_status}"
        assert "javascript" in js_headers.get("content-type", "").lower()
        assert js_headers.get("x-content-type-options") == "nosniff"
        assert len(js_bytes) > 5000, f"JS bundle too small: {len(js_bytes)} bytes"

        # Verify CSS asset
        css_status, css_text, css_headers, css_bytes = http_req(base, css_path)
        assert css_status == 200, f"Expected 200 for {css_path}, got {css_status}"
        assert "text/css" in css_headers.get("content-type", "").lower()
        assert css_headers.get("x-content-type-options") == "nosniff"
        assert len(css_bytes) > 500, f"CSS bundle too small: {len(css_bytes)} bytes"

        for path, expected in ((js_path, js_bytes), (css_path, css_bytes)):
            status, _, headers, content = http_req(base, "/admin" + path)
            assert status == 200 and content == expected
            assert headers["content-type"] == http_req(base, path)[2]["content-type"]
        assert http_req(base, "/admin/assets/missing.js")[0] == 404

        # Verify unknown assets MUST return 404 Not Found (never HTML fallback)
        un_status, un_text, un_headers, _ = http_req(base, "/assets/nonexistent_file_12345.js")
        assert un_status == 404, f"Expected 404 for unknown asset, got {un_status}"
        assert "<div id=\"root\">" not in un_text, "Unknown asset must not return HTML shell fallback"

        un_css_status, un_css_text, _, _ = http_req(base, "/assets/unknown_style.css")
        assert un_css_status == 404, f"Expected 404 for unknown CSS, got {un_css_status}"
        assert "<div id=\"root\">" not in un_css_text


def test_distribution_auth_content_flow(dist_exe: Path, work_dir: Path):
    print("2. Testing auth, RBAC, theme ZIP, schema normalization, preview, save, restart...")
    db_file = work_dir / "flow_test.sqlite"
    admin_creds = json.dumps({"username": "admin", "password": "password123"}).encode()
    editor_creds = json.dumps({"username": "editor1", "password": "password123"}).encode()

    with running_dist_binary(dist_exe, work_dir, db_file) as base:
        # Initial public page
        st, body, _, _ = http_req(base, "/")
        assert st == 200 and "Chưa lưu block." in body

        # Setup first admin
        st, _, _, _ = http_req(base, "/auth/setup", admin_creds, "application/json")
        assert st == 201

        # Login admin
        st, body, headers, _ = http_req(base, "/auth/login", admin_creds, "application/json")
        assert st == 200
        admin_cookie = headers["set-cookie"].split(";")[0].strip()
        admin_csrf = json.loads(body)["csrf_token"]
        admin_auth = {"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf}
        admin_read = {"Cookie": admin_cookie}

        # Admin creates Editor
        st, body, _, _ = http_req(base, "/api/admin/users", editor_creds, "application/json", headers=admin_auth)
        assert st == 201
        assert json.loads(body)["role"] == "editor"

        # Admin uploads first theme ZIP (dist/banner)
        banner_theme = sample_banner_theme()
        st, _, _, _ = http_req(base, "/theme", banner_theme, "application/zip", headers=admin_auth)
        assert st == 201

        # Check registry
        st, body, _, _ = http_req(base, "/registry", headers=admin_read)
        assert st == 200
        registry = json.loads(body)
        assert "dist/banner" in registry

        # Test A: First upload -> preview WITHOUT typing anything (normalized empty strings for all fields)
        empty_banner_block = json.dumps({
            "type": "dist/banner",
            "values": {"headline": "", "content": ""},
        }).encode()
        st, preview_html, _, _ = http_req(base, "/preview", empty_banner_block, "application/json", headers=admin_auth)
        assert st == 200, f"Expected 200 for untyped normalized preview, got {st}"

        # Unnormalized empty object {} must fail with 400
        bad_empty_block = json.dumps({"type": "dist/banner", "values": {}}).encode()
        assert http_req(base, "/preview", bad_empty_block, "application/json", headers=admin_auth)[0] == 400

        # Upload second theme ZIP with completely different fields (dist/notice: alert_msg, level)
        notice_theme = sample_notice_theme()
        st, _, _, _ = http_req(base, "/theme", notice_theme, "application/zip", headers=admin_auth)
        assert st == 201

        # Check registry now contains both
        st, body, _, _ = http_req(base, "/registry", headers=admin_read)
        assert st == 200
        registry = json.loads(body)
        assert "dist/banner" in registry and "dist/notice" in registry

        # Test B: Switching between 2 blocks with different schema -> preview & save
        # When user selects dist/notice in UI, values are normalized to only dist/notice fields
        notice_payload = json.dumps({
            "type": "dist/notice",
            "values": {"alert_msg": "Hệ thống bảo trì lúc 0h", "level": "warning"},
        }).encode()
        st, preview_html, _, _ = http_req(base, "/preview", notice_payload, "application/json", headers=admin_auth)
        assert st == 200
        assert "Hệ thống bảo trì lúc 0h" in preview_html

        # Save notice
        st, _, _, _ = http_req(base, "/save", notice_payload, "application/json", headers=admin_auth)
        assert st == 200
        assert "Hệ thống bảo trì lúc 0h" in http_req(base, "/")[1]

        # Verify cross-schema payload without normalization (mixing fields) is rejected with 400
        dirty_payload = json.dumps({
            "type": "dist/notice",
            "values": {"headline": "old", "alert_msg": "new", "level": "warning"},
        }).encode()
        assert http_req(base, "/preview", dirty_payload, "application/json", headers=admin_auth)[0] == 400

        # Switch back to dist/banner, normalized payload saves cleanly
        banner_payload = json.dumps({
            "type": "dist/banner",
            "values": {"headline": "Tiêu đề bản phân phối", "content": "Nội dung thử nghiệm"},
        }).encode()
        st, _, _, _ = http_req(base, "/save", banner_payload, "application/json", headers=admin_auth)
        assert st == 200
        assert "Tiêu đề bản phân phối" in http_req(base, "/")[1]

        # Test M2 Plugin installation and execution in standalone distribution
        sample_plugin_zip = Path("examples/plugins/sample-plugin/sample-plugin.zip").read_bytes()
        st, body, _, _ = http_req(base, "/api/admin/plugins", sample_plugin_zip, "application/zip", headers=admin_auth)
        assert st == 201, f"Expected 201 for plugin install in distribution, got {st}: {body}"

        run_input = json.dumps({"text": "dist check"}).encode()
        st, body, _, _ = http_req(base, "/api/admin/plugins/sample-plugin/run", run_input, "application/json", headers=admin_auth)
        assert st == 200, f"Expected 200 for plugin run in distribution, got {st}: {body}"
        run_data = json.loads(body)
        assert run_data["success"] is True
        assert run_data["data"]["transformed"] == "DIST CHECK"
        assert run_data["data"]["run_count"] == 1

        # Admin logout
        st, _, _, _ = http_req(base, "/auth/logout", b"", headers=admin_auth)
        assert st == 204

        # Login as Editor
        st, body, headers, _ = http_req(base, "/auth/login", editor_creds, "application/json")
        assert st == 200
        editor_cookie = headers["set-cookie"].split(";")[0].strip()
        editor_csrf = json.loads(body)["csrf_token"]
        editor_auth = {"Cookie": editor_cookie, "X-CSRF-Token": editor_csrf}

        # Editor cannot create users -> 403
        st, _, _, _ = http_req(base, "/api/admin/users", b'{"username":"other","password":"password123"}', "application/json", headers=editor_auth)
        assert st == 403

        # Editor cannot upload theme -> 403
        st, _, _, _ = http_req(base, "/theme", banner_theme, "application/zip", headers=editor_auth)
        assert st == 403

        # Editor cannot access or run plugins -> 403
        st, _, _, _ = http_req(base, "/api/admin/plugins", headers={"Cookie": editor_cookie})
        assert st == 403
        st, _, _, _ = http_req(base, "/api/admin/plugins/sample-plugin/run", b"{}", "application/json", headers=editor_auth)
        assert st == 403

        # Editor can edit content
        editor_block = json.dumps({
            "type": "dist/banner",
            "values": {"headline": "Biên tập bởi Editor", "content": "Nội dung cập nhật"},
        }).encode()
        st, _, _, _ = http_req(base, "/save", editor_block, "application/json", headers=editor_auth)
        assert st == 200

        # Public page updated by Editor
        st, body, _, _ = http_req(base, "/")
        assert "Biên tập bởi Editor" in body

    # Verify persistence after restarting standalone binary
    with running_dist_binary(dist_exe, work_dir, db_file) as base:
        st, body, _, _ = http_req(base, "/")
        assert st == 200 and "Biên tập bởi Editor" in body

        # Re-login Admin and check plugin run_count is 2
        st, body, headers, _ = http_req(base, "/auth/login", admin_creds, "application/json")
        assert st == 200
        admin_cookie = headers["set-cookie"].split(";")[0].strip()
        admin_csrf = json.loads(body)["csrf_token"]
        admin_auth = {"Cookie": admin_cookie, "X-CSRF-Token": admin_csrf}
        st, body, _, _ = http_req(base, "/api/admin/plugins/sample-plugin/run", run_input, "application/json", headers=admin_auth)
        assert st == 200
        assert json.loads(body)["data"]["run_count"] == 2


def test_distribution_database_upgrade(dist_exe: Path, work_dir: Path, baseline_db: Path):
    print("3. Testing database migration and upgrade from pre-M1.1 baseline fixture...")
    assert baseline_db.exists(), f"Baseline fixture missing at {baseline_db}"
    upgraded_db = work_dir / "upgraded.sqlite"

    # Copy fixture safely
    src = sqlite3.connect(baseline_db)
    dst = sqlite3.connect(upgraded_db)
    src.backup(dst)
    dst.close()
    src.close()

    with running_dist_binary(dist_exe, work_dir, upgraded_db) as base:
        # Public page works immediately from existing baseline content
        st, body, _, _ = http_req(base, "/")
        assert st == 200
        assert "Baseline Title" in body

        # Unauthenticated calls to protected routes fail with 401
        assert http_req(base, "/registry")[0] == 401
        assert http_req(base, "/page")[0] == 401

        # Setup first admin on upgraded DB
        admin_creds = json.dumps({"username": "admin", "password": "password123"}).encode()
        assert http_req(base, "/auth/setup", admin_creds, "application/json")[0] == 201

        # Login as admin
        st, body, headers, _ = http_req(base, "/auth/login", admin_creds, "application/json")
        assert st == 200
        session_cookie = headers["set-cookie"].split(";")[0].strip()
        csrf_token = json.loads(body)["csrf_token"]
        auth_headers = {"Cookie": session_cookie, "X-CSRF-Token": csrf_token}
        read_headers = {"Cookie": session_cookie}

        # Page retains baseline data
        st, body, _, _ = http_req(base, "/page", headers=read_headers)
        assert st == 200
        assert json.loads(body)["values"]["title"] == "Baseline Title"

        # Update page
        new_block = json.dumps({
            "type": "sample/banner",
            "values": {"title": "Release Dist Title", "note": "Release Dist Note"},
        }).encode()
        assert http_req(base, "/save", new_block, "application/json", headers=auth_headers)[0] == 200
        assert "Release Dist Title" in http_req(base, "/")[1]

    # Restart and verify persistence on upgraded DB
    with running_dist_binary(dist_exe, work_dir, upgraded_db) as base:
        assert "Release Dist Title" in http_req(base, "/")[1]


def test_build_script_fail_fast():
    print("4. Testing scripts/build.ps1 fail-fast and lockfile enforcement...")
    # Exercise the actual build script without moving the workspace lockfile.
    with TemporaryDirectory(prefix="opendash-build-check-") as folder:
        sandbox = Path(folder)
        scripts = sandbox / "scripts"
        scripts.mkdir()
        script = scripts / "build.ps1"
        shutil.copy2(Path(__file__).with_name("build.ps1"), script)
        args = ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(script)]
        missing = subprocess.run(args, cwd=sandbox, capture_output=True, text=True)
        assert missing.returncode and "package-lock.json missing" in missing.stderr
        admin = sandbox / "admin"
        admin.mkdir()
        (admin / "package-lock.json").write_text("{}")
        (sandbox / "npm.cmd").write_text("@exit /b 77\n")
        env = dict(os.environ, PATH=str(sandbox) + os.pathsep + os.environ["PATH"])
        failure = subprocess.run(args, cwd=sandbox, env=env, capture_output=True, text=True)
        assert failure.returncode and "npm ci failed (77)" in failure.stderr


def verify_distribution(source_exe: Path, baseline_db: Path):
    source_exe = source_exe.resolve()
    baseline_db = baseline_db.resolve()

    assert source_exe.exists(), f"Release binary not found at {source_exe}"
    binary_size = source_exe.stat().st_size
    print(f"=== OpenDash Distribution Verification (M1.3) ===")
    print(f"Binary path: {source_exe}")
    print(f"Binary size: {binary_size} bytes ({binary_size / (1024 * 1024):.2f} MB)")
    print(f"Baseline DB: {baseline_db}")
    print("=" * 48)

    with TemporaryDirectory(prefix="opendash-dist-isolated-") as folder:
        work_dir = Path(folder)
        dist_exe = work_dir / source_exe.name
        shutil.copy2(source_exe, dist_exe)

        print(f"Isolated workdir: {work_dir}")
        print(f"Copied binary to: {dist_exe}")

        launch = work_dir / "launch"
        launch.mkdir()
        for root in (None, work_dir / "separate-site"):
            with running_dist_binary(dist_exe, launch, None, root) as base:
                assert http_req(base, "/api/health")[0] == 200
                db = (root or work_dir) / "database" / "opendash.sqlite"
                assert db.is_file() and not (launch / "database").exists()
                with sqlite3.connect(db.as_uri() + "?mode=ro", uri=True) as conn:
                    assert conn.execute("SELECT count(*) FROM _sqlx_migrations WHERE success").fetchone() == (3,)
                conn.close()

        test_distribution_assets_and_404(dist_exe, work_dir)
        test_distribution_auth_content_flow(dist_exe, work_dir)
        test_distribution_database_upgrade(dist_exe, work_dir, baseline_db)
        test_build_script_fail_fast()

    print("\nALL M1.3 SINGLE WINDOWS RELEASE BINARY DISTRIBUTION TESTS PASSED!")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/opendash.exe"))
    parser.add_argument("--baseline-db", type=Path, default=Path("fixtures/baseline_0001.sqlite"))
    args = parser.parse_args()
    verify_distribution(args.binary, args.baseline_db)
