#!/usr/bin/env python3
"""Native browser E2E runner for OpenDash M8-S08-T01 (backup -> restore -> re-auth -> activate -> re-edit -> Save -> public)."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import signal
import shutil
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.parse
import urllib.request
from pathlib import Path

from playwright.sync_api import sync_playwright
from m7_beta_menu_picker_e2e import login_legacy_vi

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_WATCHDOG = 180


def atomic_json(path: Path, contents: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    pending = path.with_suffix(path.suffix + ".pending")
    pending.write_text(json.dumps(contents, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    pending.replace(path)


def source_commit() -> str:
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO_ROOT, text=True).strip()


def sqlite_integrity(db_path: Path) -> None:
    with sqlite3.connect(f"file:{db_path}?mode=ro", uri=True) as conn:
        assert conn.execute("PRAGMA integrity_check").fetchone()[0] == "ok", "SQLite integrity failed"
        assert not conn.execute("PRAGMA foreign_key_check").fetchall(), "SQLite foreign keys failed"


def cleanup_owned_group(proc: subprocess.Popen) -> None:
    """Supervisor signals only the fresh process group it started."""
    if os.name == "nt":
        subprocess.run(
            ["taskkill", "/PID", str(proc.pid), "/T", "/F"],
            capture_output=True, timeout=10, check=False,
        )
    else:
        try:
            os.killpg(proc.pid, signal.SIGTERM)
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    if proc.poll() is None:
        proc.kill()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        pass


def supervise(args: argparse.Namespace) -> int:
    """Bound all browser/server calls, including a blocked Playwright shutdown."""
    temporary = None
    if args.site_root is None or args.artifact_dir is None:
        temporary = Path(tempfile.mkdtemp(prefix="opendash-m8-browser-")).resolve()
    site_root = (args.site_root or temporary / "sites").resolve()
    artifact_dir = (args.artifact_dir or temporary / "artifacts").resolve()
    site_root.mkdir(parents=True, exist_ok=True)
    artifact_dir.mkdir(parents=True, exist_ok=True)
    phase_path = artifact_dir / "last-phase.json"
    phase_path.unlink(missing_ok=True)
    cmd = [
        sys.executable, "-u", str(Path(__file__).resolve()), "--worker",
        "--binary", str(args.binary.resolve()), "--site-root", str(site_root),
        "--artifact-dir", str(artifact_dir), "--timeout", str(args.timeout),
        "--scenario", args.scenario,
    ]
    if args.keep_artifacts:
        cmd.append("--keep-artifacts")
    proc = subprocess.Popen(
        cmd, cwd=REPO_ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
        start_new_session=(os.name != "nt"),
        creationflags=(subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0),
    )
    timed_out = False
    try:
        output, _ = proc.communicate(timeout=args.timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        cleanup_owned_group(proc)
        output, _ = proc.communicate(timeout=5)
    if output:
        print(output, end="", flush=True)
    # Even an exited worker can leave browser/server descendants. Limit
    # teardown to our own process group, never the operator's Chrome/site.
    cleanup_owned_group(proc)
    phase = {}
    try:
        phase = json.loads(phase_path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        pass
    if timed_out or proc.returncode != 0:
        receipt = {
            **phase,
            "status": "FAIL",
            "reason": "watchdog_timeout" if timed_out else "worker_nonzero",
            "exit_code": 124 if timed_out else proc.returncode,
            "worker_pid": proc.pid,
        }
        atomic_json(artifact_dir / "run-receipt.json", receipt)
        print(json.dumps(receipt, indent=2, ensure_ascii=False), flush=True)
        return 124 if timed_out else (proc.returncode or 1)
    if args.scenario != "normal":
        print("FAIL: injected scenario unexpectedly returned 0", flush=True)
        return 1
    return 0

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
    try:
        wait_health(f"http://127.0.0.1:{port}")
    except Exception:
        stop_proc(proc)
        raise
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


def run_e2e(
    binary: Path, site_root: Path, artifact_dir: Path, keep_artifacts: bool, scenario: str
) -> dict:
    artifact_dir.mkdir(parents=True, exist_ok=True)
    source_root = site_root / "source-site"
    source_db = source_root / "database" / "opendash.sqlite"
    if source_root.exists() or source_root.is_symlink():
        raise RuntimeError(f"Refusing to overwrite existing test root: {source_root}")
    source_db.parent.mkdir(parents=True, exist_ok=True)
    progress: dict = {
        "source_sha": source_commit(),
        "binary_sha256": sha256_file(binary),
        "scenario": scenario,
        "source_root": str(source_root),
        "site_root": str(site_root),
        "artifact_dir": str(artifact_dir),
    }

    def phase(name: str, **values) -> None:
        progress.update(values)
        progress["phase"] = name
        atomic_json(artifact_dir / "last-phase.json", progress)
        print(f"PHASE {name}", flush=True)

    phase("source_root_created")

    passphrase = "m8-browser-e2e-passphrase"
    admin_user = "admin"
    admin_pass = "admin-password-123"
    src_port = allocate_port()
    src_url = f"http://127.0.0.1:{src_port}"
    src_proc = None
    tgt_proc = None
    console_errors: list[str] = []
    page_errors: list[str] = []

    try:
        phase("source_server_start")
        src_proc = spawn_server(binary, source_root, source_db, src_port)
        # Setup Admin via HTTP before browser login
        phase("source_admin_setup")
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
            phase("chrome_launch")
            browser, channel_used = launch_browser(p)
            context = browser.new_context(accept_downloads=True)
            page = context.new_page()
            page.set_default_timeout(12000)
            page.set_default_navigation_timeout(15000)

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
            phase("source_login")
            page.goto(f"{src_url}/admin", wait_until="domcontentloaded")
            page.fill("#auth-username", admin_user)
            page.fill("#auth-password", admin_pass)
            login_legacy_vi(page, src_url)
            page.wait_for_selector("#user-panel", timeout=15000)

            phase("source_visual_builder_navigation")
            page.locator('#admin-navigation button[data-menu-id="themes"]').click()
            page.get_by_role("link", name="Visual Builder", exact=True).click()
            page.locator('[data-testid="builder-workspace"]').wait_for(state="visible")
            assert page.url.endswith("#/builder"), "Visual Builder did not navigate to #/builder"
            page.click('button:has-text("+ Section")')
            page.click('button[title="Thêm row"]')
            page.click('button[title="Thêm column"]')
            page.click('button[title="Thêm element"]')
            phase("source_builder_save")
            page.click("#save")
            page.locator("#save").wait_for(state="visible")
            page.wait_for_function("() => document.querySelector('#save')?.disabled === true", timeout=12000)
            page.screenshot(path=str(artifact_dir / "01-source-builder.png"))

            # 2. Navigate to Cài đặt -> Backups & Restore panels, create backup, download .odbackup.age
            phase("source_tools_backup_navigation")
            page.locator('#admin-navigation button[data-menu-id="tools"]').click()
            page.get_by_role("link", name="Sao lưu & Phục hồi", exact=True).click()
            page.get_by_role("heading", name="Sao lưu mã hóa (.odbackup.age)").wait_for()
            backup_form = page.locator('form:has(button:has-text("Tạo bản sao lưu mã hóa"))')
            pw_inputs = backup_form.locator('input[autocomplete="new-password"]')
            pw_inputs.nth(0).fill(passphrase)
            pw_inputs.nth(1).fill(passphrase)
            backup_form.locator('input[placeholder="Ví dụ: Trước khi nâng cấp theme"]').fill("M8 browser E2E backup")
            backup_form.locator('button:has-text("Tạo bản sao lưu mã hóa")').click()
            phase("source_encrypted_backup")
            page.wait_for_selector('a:has-text("Tải xuống")', timeout=30000)

            dl_link = page.locator('a:has-text("Tải xuống")').first
            dl_href = dl_link.get_attribute("href")
            dl_name = dl_link.get_attribute("download") or "backup.odbackup.age"
            assert dl_href, "Admin did not provide a backup download link"
            dl_resp = context.request.get(urllib.parse.urljoin(src_url, dl_href))
            if not dl_resp.ok:
                raise RuntimeError(f"Download {dl_href} failed: {dl_resp.status}")
            backup_file = artifact_dir / dl_name
            backup_file.write_bytes(dl_resp.body())
            backup_sha256 = sha256_file(backup_file)
            assert backup_file.stat().st_size > 128, "Encrypted artifact too small"
            phase("backup_downloaded", backup_file=str(backup_file), backup_sha256=backup_sha256)

            # 3. Upload & Restore to separate root via Admin UI
            phase("source_restore_upload")
            page.set_input_files('input[type="file"][accept*=".age"]', str(backup_file))
            page.fill(
                'input[placeholder="Passphrase của gói backup"]',
                "intentionally-wrong-backup-passphrase" if scenario == "wrong-backup-password" else passphrase,
            )
            page.click('button:has-text("Tải lên & Khôi phục")')

            # Wait for target_root with READY.json under source_root/backups/restored
            restores_dir = source_root / "backups" / "restored"
            if scenario == "wrong-backup-password":
                phase("fault_wrong_backup_password")
                # The actual Admin restore job must reach `failed`. A timeout or
                # a missing restore directory is not sufficient proof of denial.
                page.get_by_text("failed", exact=True).wait_for(state="visible", timeout=30000)
                restore_section = page.locator("section").filter(
                    has=page.get_by_role("heading", name="Khôi phục & Safe Mode", exact=True)
                )
                restore_section.locator("div.border-red-200").wait_for(state="visible")
                assert not list(restores_dir.glob("*/READY.json")), (
                    "Incorrect backup password produced a published recovery root"
                )
                sqlite_integrity(source_db)
                page.screenshot(path=str(artifact_dir / "03-wrong-backup-password-denied.png"))
                raise RuntimeError("EXPECTED_FAIL: encrypted restore rejected incorrect backup passphrase")

            deadline = time.monotonic() + 45.0
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
            assert target_root != source_root and target_root.is_relative_to(source_root), (
                "Restore must publish a separate child root"
            )
            target_db = target_root / "database" / "opendash.sqlite"
            ready_file = target_root / "READY.json"
            config_file = target_root / "config" / "site.json"
            ready = json.loads(ready_file.read_text(encoding="utf-8"))
            config = json.loads(config_file.read_text(encoding="utf-8"))
            assert ready["recovery_safe_mode"] is True, "Restore READY must be Safe Mode"
            assert config["recovery_safe_mode"] is True, "Restore config must be Safe Mode"
            sqlite_integrity(target_db)
            phase(
                "restored_root_ready",
                target_root=str(target_root),
                ready_sha256=sha256_file(ready_file),
                target_config_sha256_before_activation=sha256_file(config_file),
                restore_generation=ready.get("job_id"),
            )

            # 4. Close source page cleanly, stop source server, start target server on restored root
            page.close()
            stop_proc(src_proc)
            src_proc = None
            sqlite_integrity(source_db)
            source_db_sha256 = sha256_file(source_db)
            phase("source_stopped", source_db_sha256=source_db_sha256)
            tgt_port = allocate_port()
            tgt_url = f"http://127.0.0.1:{tgt_port}"
            phase("target_server_start")
            tgt_proc = spawn_server(binary, target_root, target_db, tgt_port)

            # 5. Re-auth on target, activate recovery, re-edit & Save, verify public /
            t_page = context.new_page()
            attach_listeners(t_page)
            t_page.set_default_timeout(12000)
            t_page.set_default_navigation_timeout(15000)
            phase("target_reauthentication")
            t_page.goto(f"{tgt_url}/admin", wait_until="domcontentloaded")
            t_page.locator("#auth-login").wait_for(state="visible")
            t_page.fill("#auth-username", admin_user)
            t_page.fill("#auth-password", admin_pass)
            t_page.click("#auth-login")
            t_page.wait_for_selector("#user-panel", timeout=15000)

            phase("target_safe_mode_before_activation")
            health_before = context.request.get(f"{tgt_url}/api/health").json()
            assert health_before["safe_mode"] is True, "Restored binary did not start in Safe Mode"
            public_pre_activation = context.request.get(f"{tgt_url}/")
            assert "noindex" in public_pre_activation.headers.get("x-robots-tag", "").lower(), (
                "Restored public root lacks Safe Mode noindex"
            )
            t_page.locator('#admin-navigation button[data-menu-id="tools"]').click()
            t_page.get_by_role("link", name="Sao lưu & Phục hồi", exact=True).click()
            t_page.get_by_role("heading", name="Khôi phục & Safe Mode").wait_for()
            t_page.get_by_text("Trạng thái hiện tại: SAFE MODE", exact=False).wait_for(state="visible")
            t_page.screenshot(path=str(artifact_dir / "02-target-safe-mode.png"))

            if scenario == "server-stop":
                phase("fault_server_stop")
                stop_proc(tgt_proc)
                tgt_proc = None
                t_page.reload(wait_until="domcontentloaded", timeout=3000)
                raise RuntimeError("FAIL: stopped target server unexpectedly reloaded")
            if scenario == "watchdog-stall":
                phase("fault_watchdog_stall")
                time.sleep(60)  # Test-only: supervisor must terminate this owned process group.
                raise RuntimeError("FAIL: watchdog unexpectedly allowed the stalled worker")
            if scenario == "wrong-password":
                phase("fault_wrong_password")
                t_page.fill('input[placeholder="Nhập mật khẩu Admin hiện tại"]', "deliberately-incorrect-password")
                t_page.get_by_role("button", name="Mở khóa Safe Mode", exact=True).click()
                # A 401 logs out the browser. Require the actual denial before
                # accepting this negative test (not just a still-true config).
                t_page.get_by_text("Mật khẩu Admin không chính xác.", exact=True).wait_for(state="visible")
                assert context.request.get(f"{tgt_url}/api/health").json()["safe_mode"] is True
                assert "noindex" in context.request.get(f"{tgt_url}/").headers.get("x-robots-tag", "")
                assert json.loads(config_file.read_text())["recovery_safe_mode"] is True
                t_page.screenshot(path=str(artifact_dir / "03-wrong-password-denied.png"))
                raise RuntimeError("EXPECTED_FAIL: wrong Admin password did not activate restored site")

            phase("target_admin_activation")
            t_page.fill('input[placeholder="Nhập mật khẩu Admin hiện tại"]', admin_pass)
            t_page.get_by_role("button", name="Mở khóa Safe Mode", exact=True).click()
            t_page.get_by_text("Trạng thái hiện tại: Hoạt động bình thường", exact=False).wait_for(state="visible")
            assert json.loads(config_file.read_text())["recovery_safe_mode"] is False, (
                "Admin activation did not persist safe_mode=false"
            )
            phase("target_activated", target_config_sha256_after_activation=sha256_file(config_file))
            assert context.request.get(f"{tgt_url}/api/health").json()["safe_mode"] is False

            # Re-edit Builder & Save
            phase("target_visual_builder_navigation")
            t_page.locator('#admin-navigation button[data-menu-id="themes"]').click()
            if scenario == "missing-navigation":
                phase("fault_missing_navigation")
                t_page.get_by_role("link", name="Visual Builder (missing)", exact=True).wait_for(
                    state="visible", timeout=1200
                )
                raise RuntimeError("FAIL: missing navigation link unexpectedly appeared")
            t_page.get_by_role("link", name="Visual Builder", exact=True).click()
            t_page.locator('[data-testid="builder-workspace"]').wait_for(state="visible")
            assert t_page.url.endswith("#/builder"), "Recovered Visual Builder route mismatch"
            t_page.click('button:has-text("+ Section")')
            phase("target_builder_save")
            t_page.click("#save")
            t_page.wait_for_function("() => document.querySelector('#save')?.disabled === true", timeout=12000)
            t_page.screenshot(path=str(artifact_dir / "04-target-builder-saved.png"))

            # Check public /
            phase("target_public_after_activation")
            t_page.goto(f"{tgt_url}/", wait_until="domcontentloaded")
            public_after = context.request.get(f"{tgt_url}/")
            assert "noindex" not in public_after.headers.get("x-robots-tag", "").lower(), (
                "Site is still in Safe Mode after activation"
            )
            body_text = t_page.inner_text("body")
            if not body_text.strip():
                raise RuntimeError("Restored public / rendered empty body")

            # Activation is durable only if the recovered root stays unlocked
            # across a real process restart, not merely in React component state.
            phase("target_restart_activation_persistence")
            t_page.close()
            stop_proc(tgt_proc)
            tgt_proc = None
            tgt_proc = spawn_server(binary, target_root, target_db, tgt_port)
            assert context.request.get(f"{tgt_url}/api/health").json()["safe_mode"] is False, (
                "Recovered root reentered Safe Mode after restart"
            )
            assert json.loads(config_file.read_text(encoding="utf-8"))["recovery_safe_mode"] is False
            restarted_public = context.request.get(f"{tgt_url}/")
            assert restarted_public.ok and "noindex" not in restarted_public.headers.get(
                "x-robots-tag", ""
            ).lower(), "Recovered public route failed after activation restart"

            if console_errors or page_errors:
                raise RuntimeError(
                    f"Browser errors detected: console={console_errors} page={page_errors}"
                )

            sqlite_integrity(source_db)
            sqlite_integrity(target_db)
            assert sha256_file(source_db) == source_db_sha256, "Source DB changed after source server stopped"
            phase(
                "all_assertions_pass",
                source_db_sha256=source_db_sha256,
                target_db_sha256=sha256_file(target_db),
            )
            result = {
                "status": "PASS",
                "channel": channel_used,
                "binary": str(binary),
                "binary_sha256": sha256_file(binary),
                "source_sha": progress["source_sha"],
                "backup_file": str(backup_file),
                "backup_sha256": backup_sha256,
                "source_root": str(source_root),
                "source_db_sha256": source_db_sha256,
                "target_root": str(target_root),
                "ready_sha256": progress["ready_sha256"],
                "target_config_sha256_before_activation": progress["target_config_sha256_before_activation"],
                "target_config_sha256_after_activation": progress["target_config_sha256_after_activation"],
                "target_db_sha256": progress["target_db_sha256"],
                "scenario": scenario,
                "last_phase": "all_assertions_pass",
                "console_errors": 0,
                "page_errors": 0,
            }
            atomic_json(artifact_dir / "run-receipt.json", result)
            print(json.dumps(result, indent=2), flush=True)
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
    parser.add_argument("--timeout", type=int, default=DEFAULT_WATCHDOG, help="Total watchdog timeout in seconds (15–3600)")
    parser.add_argument(
        "--scenario", choices=(
            "normal", "missing-navigation", "server-stop", "wrong-password",
            "wrong-backup-password", "watchdog-stall",
        ),
        default="normal", help="Test-only controlled failure injection",
    )
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()

    if not args.binary.is_file():
        raise SystemExit(f"Binary not found: {args.binary}")
    if not (15 <= args.timeout <= 3600):
        parser.error("--timeout must be between 15 and 3600 seconds")
    if not args.worker:
        return supervise(args)
    temp_root = None
    if args.site_root is None or args.artifact_dir is None:
        temp_root = Path(tempfile.mkdtemp(prefix="opendash-m8-browser-"))
    site_root = args.site_root or (temp_root / "sites")
    artifact_dir = args.artifact_dir or (temp_root / "artifacts")
    site_root.mkdir(parents=True, exist_ok=True)
    artifact_dir.mkdir(parents=True, exist_ok=True)

    try:
        run_e2e(
            args.binary.resolve(), site_root.resolve(), artifact_dir.resolve(),
            args.keep_artifacts, args.scenario,
        )
        return 0
    except Exception as exc:
        print(f"FAIL: {type(exc).__name__}: {exc}", file=sys.stderr, flush=True)
        return 1
    finally:
        if temp_root and not args.keep_artifacts:
            shutil.rmtree(temp_root, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())
