#!/usr/bin/env python3
"""Native browser E2E runner for OpenDash M8-S08-T01 (backup -> restore -> re-auth -> activate -> re-edit -> Save -> public)."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request
from pathlib import Path

from playwright.sync_api import sync_playwright


def allocate_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        while chunk := f.read(65536):
            h.update(chunk)
    return h.hexdigest()


def wait_health(base_url: str, timeout_s: float = 15.0) -> None:
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        try:
            with urllib.request.urlopen(f"{base_url}/api/health", timeout=1.5) as resp:
                if resp.status == 200:
                    return
        except Exception:
            pass
        time.sleep(0.15)
    raise RuntimeError(f"Server did not become healthy at {base_url}")


def spawn_server(binary: Path, site_root: Path, db_path: Path, port: int) -> subprocess.Popen[bytes]:
    env = os.environ.copy()
    env["OPENDASH_SITE_ROOT"] = str(site_root)
    env["OPENDASH_DB_PATH"] = str(db_path)
    env["OPENDASH_PORT"] = str(port)
    proc = subprocess.Popen(
        [str(binary)],
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    wait_health(f"http://127.0.0.1:{port}")
    return proc


def stop_proc(proc: subprocess.Popen[bytes] | None) -> None:
    if proc is None or proc.poll() is not None:
        return
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=5)


def launch_browser(p):
    errors = []
    for channel in ("chrome", "msedge", None):
        try:
            kwargs = {"headless": True}
            if channel is not None:
                kwargs["channel"] = channel
            browser = p.chromium.launch(**kwargs)
            return browser, channel or "chromium"
        except Exception as exc:
            errors.append(f"{channel or 'chromium'}: {exc}")
    raise RuntimeError("Failed to launch Chromium/Chrome/Edge: " + " | ".join(errors))


def run_e2e(binary: Path, site_root: Path, artifact_dir: Path, keep_artifacts: bool) -> dict:
    artifact_dir.mkdir(parents=True, exist_ok=True)
    source_root = site_root / "source-site"
    source_db = source_root / "database" / "opendash.sqlite"
    source_db.parent.mkdir(parents=True, exist_ok=True)

    passphrase = "m8-browser-e2e-passphrase"
    admin_user = "admin"
    admin_pass = "admin-password-123"
    src_port = allocate_port()
    src_url = f"http://127.0.0.1:{src_port}"
    src_proc = spawn_server(binary, source_root, source_db, src_port)
    tgt_proc = None
    console_errors: list[str] = []
    page_errors: list[str] = []

    try:
        # Setup Admin via HTTP before browser login
        req = urllib.request.Request(
            f"{src_url}/auth/setup",
            data=json.dumps({"username": admin_user, "password": admin_pass}).encode("utf-8"),
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        with urllib.request.urlopen(req, timeout=5) as resp:
            if resp.status != 201:
                raise RuntimeError(f"/auth/setup returned {resp.status}")

        with sync_playwright() as p:
            browser, channel_used = launch_browser(p)
            context = browser.new_context(accept_downloads=True)
            page = context.new_page()

            def attach_listeners(pg):
                def on_console(msg):
                    if msg.type != "error":
                        return
                    loc_url = (msg.location or {}).get("url", "")
                    text = msg.text or ""
                    # Ignore expected pre-login /api/admin/me 401 probe and browser favicon.ico 404.
                    if "401 (Unauthorized)" in text and ("/api/admin/me" in loc_url or "/auth/me" in loc_url or not loc_url):
                        return
                    if "404 (Not Found)" in text and "favicon.ico" in loc_url:
                        return
                    console_errors.append(f"{text} ({loc_url})")

                pg.on("console", on_console)
                pg.on("pageerror", lambda err: page_errors.append(str(err)))

            attach_listeners(page)

            # 1. Login on source & create initial Builder homepage content + Save
            page.goto(f"{src_url}/admin", wait_until="networkidle")
            page.fill("#auth-username", admin_user)
            page.fill("#auth-password", admin_pass)
            page.click("#auth-login")
            page.wait_for_selector("#user-panel", timeout=15000)

            page.click('button:has-text("Builder")')
            page.click('button:has-text("+ Section")')
            page.click('button[title="Thêm row"]')
            page.click('button[title="Thêm column"]')
            page.click('button[title="Thêm element"]')
            page.click("#save")
            page.wait_for_timeout(800)

            # 2. Navigate to Cài đặt -> Backups & Restore panels, create backup, download .odbackup.age
            page.click('button:has-text("Cài đặt")')
            backup_form = page.locator('form:has(button:has-text("Tạo bản sao lưu mã hóa"))')
            pw_inputs = backup_form.locator('input[autocomplete="new-password"]')
            pw_inputs.nth(0).fill(passphrase)
            pw_inputs.nth(1).fill(passphrase)
            backup_form.locator('input[placeholder="Ví dụ: Trước khi nâng cấp theme"]').fill("M8 browser E2E backup")
            backup_form.locator('button:has-text("Tạo bản sao lưu mã hóa")').click()
            page.wait_for_selector('a:has-text("Tải xuống")', timeout=30000)

            dl_link = page.locator('a:has-text("Tải xuống")').first
            dl_href = dl_link.get_attribute("href")
            dl_name = dl_link.get_attribute("download") or "backup.odbackup.age"
            dl_resp = context.request.get(f"{src_url}{dl_href}")
            if not dl_resp.ok:
                raise RuntimeError(f"Download {dl_href} failed: {dl_resp.status}")
            backup_file = artifact_dir / dl_name
            backup_file.write_bytes(dl_resp.body())
            backup_sha256 = sha256_file(backup_file)

            # 3. Upload & Restore to separate root via Admin UI
            page.set_input_files('input[type="file"][accept*=".age"]', str(backup_file))
            page.fill('input[placeholder="Passphrase của gói backup"]', passphrase)
            page.click('button:has-text("Tải lên & Khôi phục")')

            # Wait for target_root with READY.json under source_root/backups/restored
            restores_dir = source_root / "backups" / "restored"
            deadline = time.monotonic() + 30.0
            ready_dirs: list[Path] = []
            while time.monotonic() < deadline:
                if restores_dir.is_dir():
                    ready_dirs = [
                        d for d in restores_dir.iterdir()
                        if d.is_dir() and (d / "READY.json").is_file()
                    ]
                    if ready_dirs:
                        break
                page.wait_for_timeout(250)
            if not ready_dirs:
                raise RuntimeError(f"No READY.json directory found in {restores_dir}")
            target_root = ready_dirs[0]
            target_db = target_root / "database" / "opendash.sqlite"

            # 4. Close source page cleanly, stop source server, start target server on restored root
            page.close()
            stop_proc(src_proc)
            src_proc = None
            tgt_port = allocate_port()
            tgt_url = f"http://127.0.0.1:{tgt_port}"
            tgt_proc = spawn_server(binary, target_root, target_db, tgt_port)

            # 5. Re-auth on target, activate recovery, re-edit & Save, verify public /
            t_page = context.new_page()
            attach_listeners(t_page)
            t_page.goto(f"{tgt_url}/admin", wait_until="networkidle")
            t_page.fill("#auth-username", admin_user)
            t_page.fill("#auth-password", admin_pass)
            t_page.click("#auth-login")
            t_page.wait_for_selector("#user-panel", timeout=15000)

            t_page.click('button:has-text("Cài đặt")')
            t_page.fill('input[placeholder="Nhập mật khẩu Admin hiện tại"]', admin_pass)
            t_page.click('button:has-text("Mở khóa Safe Mode")')
            t_page.wait_for_timeout(800)

            # Re-edit Builder & Save
            t_page.click('button:has-text("Builder")')
            t_page.click('button:has-text("+ Section")')
            t_page.click("#save")
            t_page.wait_for_timeout(800)

            # Check public /
            t_page.goto(f"{tgt_url}/", wait_until="networkidle")
            body_text = t_page.inner_text("body")
            if not body_text.strip():
                raise RuntimeError("Restored public / rendered empty body")

            if console_errors or page_errors:
                raise RuntimeError(
                    f"Browser errors detected: console={console_errors} page={page_errors}"
                )

            result = {
                "status": "PASS",
                "channel": channel_used,
                "binary": str(binary),
                "binary_sha256": sha256_file(binary),
                "backup_file": str(backup_file),
                "backup_sha256": backup_sha256,
                "target_root": str(target_root),
                "console_errors": 0,
                "page_errors": 0,
            }
            print(json.dumps(result, indent=2))
            return result
    finally:
        stop_proc(src_proc)
        stop_proc(tgt_proc)
        if not keep_artifacts:
            shutil.rmtree(source_root, ignore_errors=True)


def main() -> int:
    parser = argparse.ArgumentParser(description="OpenDash M8 Browser E2E Runner")
    parser.add_argument("--binary", required=True, type=Path, help="Path to opendash binary")
    parser.add_argument("--site-root", type=Path, help="Workspace root for test sites")
    parser.add_argument("--artifact-dir", type=Path, help="Directory to save downloaded .odbackup.age")
    parser.add_argument("--keep-artifacts", action="store_true", help="Keep temporary site dirs")
    parser.add_argument("--timeout", type=int, default=90, help="Watchdog timeout in seconds")
    args = parser.parse_args()

    if not args.binary.is_file():
        raise SystemExit(f"Binary not found: {args.binary}")

    # Hard watchdog to prevent hung browser/child processes on Windows.
    def _watchdog():
        print(f"ERROR: m8_browser_e2e watchdog expired after {args.timeout}s", file=sys.stderr)
        os._exit(2)

    timer = threading.Timer(args.timeout, _watchdog)
    timer.daemon = True
    timer.start()

    temp_root = None
    if args.site_root is None or args.artifact_dir is None:
        temp_root = Path(tempfile.mkdtemp(prefix="opendash-m8-browser-"))
    site_root = args.site_root or (temp_root / "sites")
    artifact_dir = args.artifact_dir or (temp_root / "artifacts")
    site_root.mkdir(parents=True, exist_ok=True)
    artifact_dir.mkdir(parents=True, exist_ok=True)

    try:
        run_e2e(args.binary.resolve(), site_root.resolve(), artifact_dir.resolve(), args.keep_artifacts)
        sys.stdout.flush()
        # Avoid Windows Playwright browser.close() hang after all assertions and cleanup complete.
        os._exit(0)
    except Exception as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        sys.stderr.flush()
        os._exit(1)


if __name__ == "__main__":
    main()
