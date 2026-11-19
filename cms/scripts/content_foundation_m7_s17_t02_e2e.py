#!/usr/bin/env python3
"""#377 / M7-S17-T02 isolated native macOS Admin permalink/API acceptance.

This runner verifies the server contract behind the Admin UI. Browser controls,
draft-form retention and screenshots are checked separately once the #377 UI lands.

python3 cms/scripts/content_foundation_m7_s17_t02_e2e.py \
  --binary cms/target/debug/opendash --artifact-dir /tmp/t02-isolated-evidence
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import platform
import sqlite3
import subprocess
import tempfile
import traceback

from playwright.sync_api import sync_playwright
from content_foundation_m4_s12_t01_e2e import api, fetch
from content_foundation_m7_s17_t01_e2e import no_redirect, permalink_input
from m7_beta_menu_picker_e2e import admin_login, launch_server, stop_server


def sha(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def assert_row(ctx, base: str, kind: str, entry_id: int, public_url: str | None) -> None:
    rows = api(ctx, base, "GET", f"/api/admin/content-types/{kind}/entry-table")
    entry = next((item for item in rows["items"] if item["id"] == entry_id), None)
    assert entry is not None, f"Admin table missing {kind}/{entry_id}"
    assert entry["public_url"] == public_url, (entry_id, public_url, entry["public_url"])


def run(binary: pathlib.Path, artifacts: pathlib.Path) -> list[str]:
    checks: list[str] = []
    with tempfile.TemporaryDirectory(prefix="t02-native-", dir=artifacts) as temp:
        root = pathlib.Path(temp) / "site"
        root.mkdir(mode=0o700)
        db = root / "database/opendash.sqlite"
        proc, base = launch_server(binary, root)
        assert base.startswith("http://127.0.0.1:") and not base.endswith(":18964")
        try:
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(channel="chrome", headless=True)
                try:
                    ctx = browser.new_context()
                    guest = browser.new_context()
                    api(ctx, base, "POST", "/auth/setup", 201,
                        {"username": "t02_admin", "password": "Native-T02-Isolated-Password!2026"})
                    csrf = admin_login(ctx, base, "t02_admin", "Native-T02-Isolated-Password!2026")

                    fetch(guest, base, "GET", "/api/admin/content-types/post/entry-table", 401)
                    fetch(guest, base, "GET", "/api/admin/settings/permalinks", 401)
                    settings = api(ctx, base, "GET", "/api/admin/settings/permalinks")
                    assert settings["post_structure"] == "/content/post/%postname%"
                    assert isinstance(settings["version"], int)
                    checks.append("authenticated_settings_defaults_and_private_table")

                    post = api(ctx, base, "POST", "/api/admin/content-types/post/entries", 201,
                               {"title": "T02 Live", "slug": "t02-live", "values": {}}, csrf)
                    draft = api(ctx, base, "POST", "/api/admin/content-types/post/entries", 201,
                                {"title": "T02 Private", "slug": "t02-private", "values": {}}, csrf)
                    post = api(ctx, base, "POST",
                               f'/api/admin/content-types/post/entries/{post["id"]}/publication',
                               200, {"status": "published"}, csrf, post["version"])
                    assert_row(ctx, base, "post", post["id"], "/content/post/t02-live")
                    assert_row(ctx, base, "post", draft["id"], None)
                    preview_path = f'/api/admin/content-types/post/entries/{draft["id"]}/preview'
                    fetch(guest, base, "GET", preview_path, 401)
                    preview = fetch(ctx, base, "GET", preview_path, 200)
                    assert preview.headers.get("cache-control") == "private, no-store"
                    assert "T02 Private" in preview.text()
                    no_redirect(ctx, base, "/content/post/t02-private", 404)
                    checks.append("server_resolved_row_urls_and_authenticated_draft_preview")

                    proposal = permalink_input(settings, post="/blog/%postname%")
                    no_redirect(ctx, base, "/blog/t02-live", 404)
                    no_redirect(ctx, base, "/content/post/t02-live", 200)
                    assert_row(ctx, base, "post", post["id"], "/content/post/t02-live")
                    fetch(ctx, base, "PUT", "/api/admin/settings/permalinks", 428,
                          proposal, csrf)
                    assert api(ctx, base, "GET", "/api/admin/settings/permalinks") == settings
                    saved = api(ctx, base, "PUT", "/api/admin/settings/permalinks", 200,
                                proposal, csrf, settings["version"])
                    assert saved["post_structure"] == "/blog/%postname%"
                    assert saved["version"] == settings["version"] + 1
                    no_redirect(ctx, base, "/blog/t02-live", 200)
                    no_redirect(ctx, base, "/content/post/t02-live", 301, "/blog/t02-live")
                    assert_row(ctx, base, "post", post["id"], "/blog/t02-live")
                    assert_row(ctx, base, "post", draft["id"], None)
                    no_redirect(ctx, base, "/blog/t02-private", 404)
                    checks.append("not_saved_does_not_change_public_and_saved_updates_table")

                    fetch(ctx, base, "PUT", "/api/admin/settings/permalinks", 412,
                          permalink_input(settings, post="/%postname%"), csrf, settings["version"])
                    assert api(ctx, base, "GET", "/api/admin/settings/permalinks") == saved
                    assert_row(ctx, base, "post", post["id"], "/blog/t02-live")
                    checks.append("stale_cas_412_preserves_settings_and_live_view_url")

                    page = api(ctx, base, "POST", "/api/admin/content-types/page/entries", 201,
                               {"title": "T02 Collision Page", "slug": "page-t02", "values": {}}, csrf)
                    api(ctx, base, "PUT",
                        f'/api/admin/content-types/page/entries/{page["id"]}/route', 200,
                        {"custom_path": "/projects/t02-live"}, csrf, page["version"])
                    collision = fetch(ctx, base, "PUT", "/api/admin/settings/permalinks", 409,
                                      permalink_input(saved, post="/projects/%postname%"),
                                      csrf, saved["version"])
                    assert collision.text(), "collision must be diagnosable"
                    assert api(ctx, base, "GET", "/api/admin/settings/permalinks") == saved
                    assert_row(ctx, base, "post", post["id"], "/blog/t02-live")
                    no_redirect(ctx, base, "/blog/t02-live", 200)
                    no_redirect(ctx, base, "/projects/t02-live", 404)  # draft page
                    checks.append("collision_409_preserves_db_and_published_routes")

                    with sqlite3.connect(db) as conn:
                        assert conn.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
                        assert not conn.execute("PRAGMA foreign_key_check").fetchall()
                        row = conn.execute("SELECT version,post_structure FROM site_permalink_settings WHERE id=1").fetchone()
                        assert row == (saved["version"], saved["post_structure"]), row
                    checks.append("sqlite_integrity_and_atomic_config_after_rejected_saves")
                finally:
                    browser.close()
        finally:
            stop_server(proc)

        proc, base = launch_server(binary, root)
        try:
            # New process, same isolated DB; no manual resolver flush.
            assert no_redirect_raw_status(base, "/blog/t02-live") == 200
            assert no_redirect_raw_status(base, "/content/post/t02-live") == 301
            assert no_redirect_raw_status(base, "/blog/t02-private") == 404
            checks.append("restart_preserves_canonical_and_draft_privacy")
        finally:
            stop_server(proc)
    return checks


def no_redirect_raw_status(base: str, path: str) -> int:
    from content_foundation_m7_s17_t01_e2e import no_redirect_raw
    return no_redirect_raw(base, path)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=pathlib.Path)
    parser.add_argument("--artifact-dir", required=True, type=pathlib.Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    artifacts = args.artifact_dir.resolve()
    artifacts.mkdir(parents=True, exist_ok=True)
    source_root = pathlib.Path(__file__).resolve().parents[2]
    receipt = {
        "task": "M7-S17-T02", "suite": "API/native regression (no Admin UI screenshots)",
        "source_sha": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source_root, text=True).strip(),
        "dirty_paths": subprocess.check_output(["git", "status", "--porcelain"], cwd=source_root, text=True).splitlines(),
        "binary_sha256": sha(binary), "runner_sha256": sha(pathlib.Path(__file__).resolve()),
        "platform": platform.platform(), "architecture": platform.machine(), "port_18964_used": False,
    }
    try:
        receipt["checks"] = run(binary, artifacts)
        receipt["result"] = "PASS"
    except Exception as error:
        receipt.update({"result": "FAIL", "error": str(error), "traceback": traceback.format_exc()})
    (artifacts / "receipt.json").write_text(json.dumps(receipt, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps(receipt, indent=2, ensure_ascii=False))
    if receipt["result"] != "PASS":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
