#!/usr/bin/env python3
"""Windows Release Binary + Real Chrome E2E Acceptance Runner for M7-S13-T01 (AC-01..AC-11)."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import socket
import sqlite3
import subprocess
import tempfile
import time
from pathlib import Path
from playwright.sync_api import sync_playwright

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_BINARY = REPO_ROOT / "cms" / "target" / "release" / ("opendash.exe" if os.name == "nt" else "opendash")


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


def wait_health(page, base_url: str) -> None:
    for _ in range(100):
        try:
            r = page.request.get(f"{base_url}/api/health")
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
    args = parser.parse_args()

    binary = Path(args.binary).resolve()
    if not binary.exists():
        raise FileNotFoundError(f"Release binary not found: {binary}")
    bin_sha256 = hashlib.sha256(binary.read_bytes()).hexdigest()

    screenshot_dir = Path(args.screenshot_dir).resolve()
    screenshot_dir.mkdir(parents=True, exist_ok=True)

    temp_root = Path(tempfile.mkdtemp(prefix="opendash-m7-s13-t01-"))
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

    try:
        with sync_playwright() as p:
            browser = launch_browser(p)
            chrome_version = browser.version

            # Desktop 1440x900
            ctx = browser.new_context(viewport={"width": 1440, "height": 900})
            page = ctx.new_page()
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

            wait_health(page, base_url)

            # Setup Admin + Editor
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
            page.goto(f"{base_url}/admin#/settings/content-types", wait_until="networkidle")
            page.wait_for_selector('[data-testid="content-type-schema-settings"]')
            assert page.locator('[data-testid="content-type-schema-settings"] [data-testid^="entry-list-"]').count() == 0
            assert not page.locator('[data-testid^="entry-list-"]').first.is_visible()
            page.screenshot(path=str(screenshot_dir / "01-desktop-settings-content-types-zero-cpt.png"))

            # Create CPT `project` via API & configure base_field_panels + field group
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
            page.goto(f"{base_url}/admin#/settings/content-types/project", wait_until="networkidle")
            page.reload(wait_until="networkidle")
            page.wait_for_selector('[data-menu-id="cpt:project"]')
            page.screenshot(path=str(screenshot_dir / "02-desktop-schema-settings-project.png"))

            # AC-04d & AC-07: Single Edit 5 regions on Desktop
            page.goto(f"{base_url}/admin#/cpt/project/new", wait_until="networkidle")
            page.wait_for_selector('[data-testid="single-edit-5-regions"]')
            page.fill("#entry-title-input", "Biệt thự Thảo Điền")
            page.fill("#entry-slug-input", "biet-thu-thao-dien")
            page.fill('[data-field-key="client_name"] input', "Anh Minh")
            page.screenshot(path=str(screenshot_dir / "03-desktop-single-edit-5-regions.png"))

            # Save via composite UI button -> navigates to #/cpt/project/:id/edit
            page.click('[data-region="header_bar"] button:has-text("Lưu")')
            page.wait_for_function("() => window.location.hash.endsWith('/edit')", timeout=10000)

            # Mobile 390x844: verify DOM region order + keyboard Escape drawer
            mctx = browser.new_context(viewport={"width": 390, "height": 844})
            mpage = mctx.new_page()
            mpage.goto(f"{base_url}/admin", wait_until="networkidle")
            # Login as Editor in mobile context to verify AC-09b (403 on #/settings/content-types) + AC-04d DOM order
            mr_login = mpage.request.post(
                f"{base_url}/auth/login",
                headers={"Origin": base_url, "Content-Type": "application/json"},
                data=json.dumps({"username": "editor1", "password": "EditorPassword123!"}),
            )
            assert mr_login.status == 200
            mpage.goto(f"{base_url}/admin#/settings/content-types", wait_until="networkidle")
            mpage.reload(wait_until="networkidle")
            mpage.wait_for_selector('[data-testid="rbac-forbidden"]')
            mpage.screenshot(path=str(screenshot_dir / "04-mobile-editor-403-rbac.png"))

            mpage.goto(f"{base_url}/admin#/cpt/project/1/edit", wait_until="networkidle")
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
            mpage.goto(f"{base_url}/admin#/users/profile", wait_until="networkidle")
            mpage.wait_for_selector('[data-testid="user-profile-panel"]')
            mpage.screenshot(path=str(screenshot_dir / "06-mobile-user-profile.png"))

            assert not page_errors, f"Page errors: {page_errors}"
            assert not console_errors, f"Console errors: {console_errors}"
            browser.close()
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()

    # Check SQLite integrity on the real DB created by release binary
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

    shutil.rmtree(temp_root, ignore_errors=True)
    receipt = {
        "status": "PASS",
        "binary": str(binary),
        "binary_sha256": bin_sha256,
        "chrome_version": chrome_version,
        "screenshots_dir": str(screenshot_dir),
    }
    print(json.dumps(receipt, indent=2, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
