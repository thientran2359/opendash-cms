#!/usr/bin/env python3
"""Bounded native Chrome Admin acceptance runner (M7-S13-T01 / F07 #324).

Normal PASS requires every original DOM/RBAC/role assertion. Fault scenarios
must exit nonzero and clean up only child processes spawned by this runner.
"""

from __future__ import annotations

import argparse
import atexit
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
from pathlib import Path
from playwright.sync_api import sync_playwright
from menu_picker_e2e import set_cms_locale

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_BINARY = REPO_ROOT / "cms" / "target" / "release" / ("opendash.exe" if os.name == "nt" else "opendash")
DEFAULT_WATCHDOG_SECONDS = 180


def write_phase(report_path: Path, phase: str, **details) -> None:
    """Durable diagnostic before every substantial browser phase."""
    report_path.parent.mkdir(parents=True, exist_ok=True)
    temp = report_path.with_suffix(".pending")
    temp.write_text(
        json.dumps({"phase": phase, **details}, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    os.replace(temp, report_path)
    print(f"PHASE {phase}", flush=True)


def terminate_server(proc: subprocess.Popen) -> None:
    if proc.poll() is not None:
        return
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=5)


def terminate_owned_group(proc: subprocess.Popen) -> None:
    """Only the fresh process group/tree created by the supervisor is targeted."""
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
    """Enforce a hard bound even if a browser/network/shutdown call hangs."""
    screenshots = Path(args.screenshot_dir).resolve()
    screenshots.mkdir(parents=True, exist_ok=True)
    phase_file = screenshots / "last-phase.json"
    if phase_file.exists():
        phase_file.unlink()
    cmd = [
        sys.executable, str(Path(__file__).resolve()),
        "--worker", "--binary", str(args.binary),
        "--screenshot-dir", str(screenshots),
        "--scenario", args.scenario, "--phase-file", str(phase_file),
    ]
    if args.keep_site:
        cmd.append("--keep-site")
    worker = subprocess.Popen(
        cmd, cwd=str(REPO_ROOT), stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, text=True,
        start_new_session=(os.name != "nt"),
        creationflags=(subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0),
    )
    timed_out = False
    try:
        output, _ = worker.communicate(timeout=args.watchdog_seconds)
    except subprocess.TimeoutExpired:
        timed_out = True
        terminate_owned_group(worker)
        output, _ = worker.communicate(timeout=5)
    else:
        if worker.returncode != 0:
            # Remove any descendant Chromium processes that survived a worker
            # error; the supervisor never signals unrelated process groups.
            terminate_owned_group(worker)
    if output:
        print(output, end="", flush=True)
    last = {}
    try:
        last = json.loads(phase_file.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        pass
    # The worker cleans up normally. A forced kill may skip its finally block;
    # only remove a positively identified root created by this execution.
    if (timed_out or worker.returncode != 0) and not args.keep_site and isinstance(last.get("site_root"), str):
        root = Path(last["site_root"]).resolve()
        temp_dir = Path(tempfile.gettempdir()).resolve()
        if root.parent == temp_dir and root.name.startswith("opendash-m7-s13-t01-"):
            shutil.rmtree(root, ignore_errors=True)
    if timed_out or worker.returncode != 0:
        receipt = {
            "status": "FAIL", "reason": "watchdog_timeout" if timed_out else "worker_nonzero",
            "phase": last.get("phase", "startup"),
            "watchdog_seconds": args.watchdog_seconds,
            "exit_code": 124 if timed_out else worker.returncode,
            "worker_pid": worker.pid,
            "source_sha": last.get("source_sha"),
            "binary_sha256": last.get("binary_sha256"),
            "scenario": args.scenario,
        }
        (screenshots / "run-receipt.json").write_text(
            json.dumps(receipt, indent=2, ensure_ascii=False) + "\n", encoding="utf-8",
        )
        print(json.dumps(receipt, indent=2), flush=True)
        return 124 if timed_out else (worker.returncode or 1)
    if args.scenario != "normal":
        print("FAIL: fault scenario returned success unexpectedly", flush=True)
        return 1
    receipt = {
        "status": "PASS", "phase": last.get("phase"),
        "source_sha": last.get("source_sha"),
        "binary_sha256": last.get("binary_sha256"),
        "scenario": args.scenario, "exit_code": 0,
        "worker_pid": worker.pid,
    }
    (screenshots / "run-receipt.json").write_text(
        json.dumps(receipt, indent=2, ensure_ascii=False) + "\n", encoding="utf-8",
    )
    return 0


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def launch_browser(p):
    for channel in ("chrome", "msedge", None):
        try:
            if channel:
                return p.chromium.launch(channel=channel, headless=True)
            return p.chromium.launch(headless=True)
        except Exception:
            continue
    raise RuntimeError("No Chrome/Edge/Chromium browser found")


def wait_health(page, base_url: str, proc: subprocess.Popen) -> None:
    for _ in range(50):
        if proc.poll() is not None:
            raise RuntimeError(f"Server exited before health: {proc.returncode}")
        try:
            r = page.request.get(f"{base_url}/api/health", timeout=1000)
            if r.status == 200:
                return
        except Exception:
            pass
        time.sleep(0.1)
    raise RuntimeError(f"Server failed to become healthy at {base_url}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default=str(DEFAULT_BINARY))
    parser.add_argument("--screenshot-dir", default=str(REPO_ROOT / "runtime" / "m7-s13-t01-screenshots"))
    parser.add_argument("--scenario", choices=["normal", "missing-selector", "server-stop", "watchdog-stall"], default="normal")
    parser.add_argument("--watchdog-seconds", type=int, default=DEFAULT_WATCHDOG_SECONDS)
    parser.add_argument("--keep-site", action="store_true", help="Retain only this run's disposable site root for diagnosis")
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--phase-file", help=argparse.SUPPRESS)
    args = parser.parse_args()
    if not (5 <= args.watchdog_seconds <= 3600):
        parser.error("--watchdog-seconds must be between 5 and 3600")
    if not args.worker:
        return supervise(args)

    binary = Path(args.binary).resolve()
    if not binary.exists():
        raise FileNotFoundError(f"Release binary not found: {binary}")
    bin_sha256 = hashlib.sha256(binary.read_bytes()).hexdigest()
    source_sha = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO_ROOT, text=True).strip()

    screenshot_dir = Path(args.screenshot_dir).resolve()
    screenshot_dir.mkdir(parents=True, exist_ok=True)

    temp_root = Path(tempfile.mkdtemp(prefix="opendash-m7-s13-t01-")).resolve()
    if not args.keep_site:
        atexit.register(shutil.rmtree, temp_root, ignore_errors=True)
    phase_file = Path(args.phase_file).resolve() if args.phase_file else screenshot_dir / "last-phase.json"
    def phase(name: str) -> None:
        write_phase(
            phase_file, name, site_root=str(temp_root),
            source_sha=source_sha, binary_sha256=bin_sha256,
            scenario=args.scenario,
        )
    phase("root_created")
    site_root = temp_root / "site"
    site_root.mkdir(parents=True, exist_ok=True)
    port = free_port()
    base_url = f"http://127.0.0.1:{port}"

    env = os.environ.copy()
    env["OPENDASH_SITE_ROOT"] = str(site_root)
    env["OPENDASH_PORT"] = str(port)

    proc = subprocess.Popen(
        [str(binary)],
        cwd=str(REPO_ROOT / "cms"),
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    browser = None
    try:
        with sync_playwright() as p:
            phase("browser_launch")
            browser = launch_browser(p)
            chrome_version = browser.version

            # Desktop 1440x900
            ctx = browser.new_context(viewport={"width": 1440, "height": 900})
            page = ctx.new_page()
            page.set_default_timeout(15000)
            page.set_default_navigation_timeout(20000)
            page_errors = []
            console_errors = []
            page.on("pageerror", lambda e: page_errors.append(str(e)))
            def on_console(m):
                if m.type == "error":
                    txt = m.text
                    url = (m.location or {}).get("url", "")
                    if ("401" in txt and "/auth/me" in url) or ("404" in txt and "favicon.ico" in url):
                        return
                    console_errors.append(f"{txt} ({url})")

            page.on("console", on_console)

            phase("server_health")
            wait_health(page, base_url, proc)

            if args.scenario == "missing-selector":
                phase("fault_missing_selector")
                page.wait_for_selector("#__m7_s15_t05_missing_selector__", timeout=1200)
                raise AssertionError("Missing selector unexpectedly appeared")
            if args.scenario == "server-stop":
                phase("fault_server_stop")
                terminate_server(proc)
                page.goto(f"{base_url}/admin", wait_until="domcontentloaded", timeout=2000)
                raise AssertionError("Stopped server unexpectedly navigated")
            if args.scenario == "watchdog-stall":
                phase("fault_watchdog_stall")
                time.sleep(60)  # Test-only: supervisor must kill this worker by its own deadline.
                raise AssertionError("Watchdog did not stop the test-only stall")

            # Setup Admin + Editor
            phase("admin_and_editor_setup")
            r_setup = page.request.post(
                f"{base_url}/auth/setup",
                headers={"Origin": base_url, "Content-Type": "application/json"},
                data=json.dumps({"username": "admin", "password": "AdminPassword123!"}),
            )
            assert r_setup.status == 201

            r_login = page.request.post(
                f"{base_url}/auth/login",
                headers={"Origin": base_url, "Content-Type": "application/json"},
                data=json.dumps({"username": "admin", "password": "AdminPassword123!"}),
            )
            assert r_login.status == 200
            admin_csrf = r_login.json()["csrf_token"]
            set_cms_locale(page, base_url, admin_csrf, "vi")

            r_ed = page.request.post(
                f"{base_url}/api/admin/users",
                headers={
                    "Origin": base_url,
                    "X-CSRF-Token": admin_csrf,
                    "Content-Type": "application/json",
                },
                data=json.dumps({"username": "editor1", "password": "EditorPassword123!", "role": "editor"}),
            )
            assert r_ed.status == 201

            # AC-09a: With 0 CPTs, Admin sees Settings -> Content Types & Fields, NO entry table/form
            phase("desktop_empty_content_settings")
            page.goto(f"{base_url}/admin#/settings/content-types", wait_until="domcontentloaded")
            page.wait_for_selector('[data-testid="content-type-schema-settings"]')
            assert page.locator('[data-testid="content-type-schema-settings"] [data-testid^="entry-list-"]').count() == 0
            assert not page.locator('[data-testid^="entry-list-"]').first.is_visible()
            page.screenshot(path=str(screenshot_dir / "01-desktop-settings-content-types-zero-cpt.png"))

            # Create CPT `project` via API & configure base_field_panels + field group
            phase("create_cpt")
            r_cpt = page.request.post(
                f"{base_url}/api/admin/content-types",
                headers={
                    "Origin": base_url,
                    "X-CSRF-Token": admin_csrf,
                    "Content-Type": "application/json",
                },
                data=json.dumps({
                    "key": "project",
                    "label": "Dự án Mộc Studio",
                    "fields": [
                        {"key": "client_name", "label": "Khách hàng", "type": "short_text", "required": True},
                        {"key": "area_m2", "label": "Diện tích (m2)", "type": "number", "required": False},
                        {
                            "key": "internal_cost",
                            "label": "Chi phí nội bộ",
                            "type": "number",
                            "required": False,
                            "min_view_role": "admin",
                            "min_edit_role": "admin",
                            "public_exposure": "private",
                        },
                    ],
                }),
            )
            assert r_cpt.status == 201
            schema_token = r_cpt.json()["schema_token"]

            # Reload #/settings/content-types/project and verify dynamic CPT menu appears
            phase("desktop_cpt_navigation")
            page.goto(f"{base_url}/admin#/settings/content-types/project", wait_until="domcontentloaded")
            page.reload(wait_until="domcontentloaded")
            page.wait_for_selector('[data-menu-id="cpt:project"]')
            page.screenshot(path=str(screenshot_dir / "02-desktop-schema-settings-project.png"))

            # AC-04d & AC-07: Single Edit 5 regions on Desktop
            phase("desktop_single_edit")
            page.goto(f"{base_url}/admin#/cpt/project/new", wait_until="domcontentloaded")
            page.wait_for_selector('[data-testid="single-edit-5-regions"]')
            page.fill("#entry-title-input", "Biệt thự Thảo Điền")
            page.fill("#entry-slug-input", "biet-thu-thao-dien")
            page.fill('[data-field-key="client_name"] input', "Anh Minh")
            page.screenshot(path=str(screenshot_dir / "03-desktop-single-edit-5-regions.png"))

            # Save via composite UI button -> navigates to #/cpt/project/:id/edit
            phase("desktop_composite_save")
            page.click('[data-region="header_bar"] button:has-text("Lưu")')
            page.wait_for_function("() => window.location.hash.endsWith('/edit')", timeout=10000)

            # Mobile 390x844: verify DOM region order + keyboard Escape drawer
            phase("mobile_browser")
            mctx = browser.new_context(viewport={"width": 390, "height": 844})
            mpage = mctx.new_page()
            mpage.set_default_timeout(15000)
            mpage.set_default_navigation_timeout(20000)
            mpage.goto(f"{base_url}/admin", wait_until="domcontentloaded")
            # Login as Editor in mobile context to verify AC-09b (403 on #/settings/content-types) + AC-04d DOM order
            mr_login = mpage.request.post(
                f"{base_url}/auth/login",
                headers={"Origin": base_url, "Content-Type": "application/json"},
                data=json.dumps({"username": "editor1", "password": "EditorPassword123!"}),
            )
            assert mr_login.status == 200
            phase("mobile_editor_rbac_403")
            mpage.goto(f"{base_url}/admin#/settings/content-types", wait_until="domcontentloaded")
            mpage.reload(wait_until="domcontentloaded")
            mpage.wait_for_selector('[data-testid="rbac-forbidden"]')
            mpage.screenshot(path=str(screenshot_dir / "04-mobile-editor-403-rbac.png"))

            phase("mobile_editor_region_order")
            mpage.goto(f"{base_url}/admin#/cpt/project/1/edit", wait_until="domcontentloaded")
            mpage.wait_for_selector('[data-testid="single-edit-5-regions"]')
            region_order = mpage.eval_on_selector_all(
                '[data-testid="single-edit-5-regions"] [data-region]',
                "els => els.map(e => e.getAttribute('data-region'))",
            )
            assert region_order == ["header_bar", "main_top", "main_primary", "main_bottom", "sidebar"], (
                f"Unexpected mobile DOM region order: {region_order}"
            )
            mpage.screenshot(path=str(screenshot_dir / "05-mobile-single-edit-390px.png"))

            # Profile screen #/users/profile
            phase("mobile_profile")
            mpage.goto(f"{base_url}/admin#/users/profile", wait_until="domcontentloaded")
            mpage.wait_for_selector('[data-testid="user-profile-panel"]')
            mpage.screenshot(path=str(screenshot_dir / "06-mobile-user-profile.png"))

            assert not page_errors, f"Page errors: {page_errors}"
            assert not console_errors, f"Console errors: {console_errors}"
            phase("browser_assertions_complete")
            browser.close()
    finally:
        if browser is not None:
            try:
                browser.close()
            except Exception:
                pass
        try:
            terminate_server(proc)
        except Exception:
            pass

    # Check SQLite integrity on the real DB created by release binary
    phase("sqlite_integrity")
    db_path = site_root / "database" / "opendash.sqlite"
    assert db_path.exists(), f"Missing DB at {db_path}"
    conn = sqlite3.connect(str(db_path))
    try:
        ic = conn.execute("PRAGMA integrity_check").fetchone()[0]
        fk = conn.execute("PRAGMA foreign_key_check").fetchall()
        assert ic == "ok", f"PRAGMA integrity_check failed: {ic}"
        assert fk == [], f"PRAGMA foreign_key_check failed: {fk}"
    finally:
        conn.close()

    phase("all_assertions_pass")
    receipt = {
        "status": "PASS",
        "binary": str(binary),
        "binary_sha256": bin_sha256,
        "source_sha": source_sha,
        "scenario": args.scenario,
        "site_root": str(temp_root) if args.keep_site else "deleted_on_exit",
        "last_phase": "all_assertions_pass",
        "chrome_version": chrome_version,
        "screenshots_dir": str(screenshot_dir),
    }
    print(json.dumps(receipt, indent=2, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
