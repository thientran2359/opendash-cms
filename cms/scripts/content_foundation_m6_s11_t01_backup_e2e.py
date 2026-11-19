#!/usr/bin/env python3
"""C4 #378: encrypted native backup/restore + snapshot rollback of provider data.

Run against one built binary with an isolated site and random localhost ports.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import platform
import tempfile
import time
import traceback

from playwright.sync_api import sync_playwright
from content_foundation_m4_s12_t01_e2e import api, fetch
from content_foundation_m6_s11_t01_e2e import install, provider_zip
from content_foundation_m7_s17_t01_e2e import no_redirect
from m7_beta_menu_picker_e2e import admin_login, launch_server, stop_server

USER = "c4_backup_admin"
PASSWORD = "Native-C4-Backup-Isolated-Password!2026"
PASS_PHRASE = "c4-isolated-encrypted-backup-passphrase"


def poll_job(ctx, base: str, job_id: str) -> dict:
    for _ in range(240):
        job = api(ctx, base, "GET", f"/api/admin/operations/jobs/{job_id}")
        if job["state"] in ("succeeded", "failed", "cancelled", "interrupted"):
            assert job["state"] == "succeeded", f"Native job failure: {job}"
            return job
        time.sleep(0.1)
    raise AssertionError(f"Job timed out: {job_id}")


def run(binary: pathlib.Path, artifacts: pathlib.Path) -> list[str]:
    checks = []
    with tempfile.TemporaryDirectory(prefix="c4-encrypted-restore-", dir=artifacts) as tmp:
        root = pathlib.Path(tmp) / "source"
        root.mkdir(mode=0o700)
        proc, base = launch_server(binary, root)
        assert base.startswith("http://127.0.0.1:") and not base.endswith(":18964")
        with sync_playwright() as playwright:
            browser = playwright.chromium.launch(channel="chrome", headless=True)
            try:
                ctx = browser.new_context()
                try:
                    api(ctx, base, "POST", "/auth/setup", 201, {
                        "username": USER, "password": PASSWORD,
                    })
                    csrf = admin_login(ctx, base, USER, PASSWORD)
                    install(ctx, base, csrf, provider_zip("1.0.0", "before-c4", "genre-before-c4"))
                    term = api(ctx, base, "POST", "/api/admin/taxonomy/terms", 201, {
                        "kind": "genre_browser_c4", "name": "Backup genre",
                        "slug": "backup-genre",
                    }, csrf)
                    created = api(ctx, base, "POST",
                        "/api/admin/content-types/listing_browser_c4/entries", 201,
                        {"title": "C4 data before snapshot", "slug": "snapshot-c4",
                         "values": {}, "term_ids": [term["id"]]}, csrf)
                    saved = api(ctx, base, "POST",
                        f"/api/admin/content-types/listing_browser_c4/entries/{created['id']}/publication",
                        200, {"status": "published"}, csrf, created["version"])
                    no_redirect(ctx, base, "/before-c4/snapshot-c4", 200)
                    no_redirect(ctx, base, "/genre-before-c4/backup-genre", 200)
                    settings_before = api(ctx, base, "GET", "/api/admin/settings/permalinks")
                    assert settings_before["content_type_bases"] == {}
                    checks.append("provider_default_entry_term_and_sparse_choices_before_backup")

                    backup = api(ctx, base, "POST", "/api/admin/operations/backups", 202,
                        {"passphrase": PASS_PHRASE, "confirm_passphrase": PASS_PHRASE,
                         "reason": "isolated C4 provider snapshot"}, csrf)
                    poll_job(ctx, base, backup["id"])
                    artifact = fetch(ctx, base, "GET",
                        f"/api/admin/operations/backups/{backup['id']}/file", 200).body()
                    assert len(artifact) > 64
                    checks.append("encrypted_snapshot_created_with_native_binary")

                    # Source changes after encrypted snapshot must be rolled back.
                    changed = api(ctx, base, "PUT", "/api/admin/settings/permalinks", 200,
                        {"post_structure": settings_before["post_structure"],
                         "content_type_bases": {"listing_browser_c4": None},
                         "taxonomy_bases": {"genre_browser_c4": None}},
                        csrf, settings_before["version"])
                    assert changed["content_type_bases"]["listing_browser_c4"] is None
                    no_redirect(ctx, base, "/content/listing_browser_c4/snapshot-c4", 200)
                    no_redirect(ctx, base, "/before-c4/snapshot-c4", 301,
                        "/content/listing_browser_c4/snapshot-c4")
                    checks.append("post_snapshot_source_mutation_proved")

                    upload = ctx.request.fetch(
                        base + "/api/admin/operations/restore-uploads", method="POST",
                        headers={"Origin": base, "X-CSRF-Token": csrf,
                                 "Content-Type": "application/octet-stream"},
                        data=artifact, timeout=30000)
                    assert upload.status == 201, (upload.status, upload.text()[:500])
                    upload_id = upload.json()["upload_id"]
                    restored = api(ctx, base, "POST", "/api/admin/operations/restores", 202,
                        {"upload_id": upload_id, "passphrase": PASS_PHRASE,
                         "reason": "isolated C4 provider restore"}, csrf)
                    result = poll_job(ctx, base, restored["id"])
                    summary = json.loads(result["summary_json"])
                    target_root = pathlib.Path(summary["target_root"])
                    assert target_root.is_file() is False and (target_root / "READY.json").exists()
                    checks.append("encrypted_restore_job_finished_and_ready")
                finally:
                    stop_server(proc)

                restored_proc, restored_base = launch_server(binary, target_root)
                assert not restored_base.endswith(":18964")
                try:
                    restored_ctx = browser.new_context()
                    # Restored sessions are intentionally invalid; login again.
                    csrf_restored = admin_login(restored_ctx, restored_base, USER, PASSWORD)
                    status = api(restored_ctx, restored_base, "GET",
                                 "/api/admin/operations/recovery/status")
                    assert status["safe_mode"] is True
                    api(restored_ctx, restored_base, "POST",
                        "/api/admin/operations/recovery/activate", 200,
                        {"password": PASSWORD,
                         "expected_generation": status["expected_generation"]},
                        csrf_restored)
                    restored_settings = api(restored_ctx, restored_base, "GET",
                                            "/api/admin/settings/permalinks")
                    assert restored_settings == settings_before, (
                        restored_settings, settings_before)
                    no_redirect(restored_ctx, restored_base, "/before-c4/snapshot-c4", 200)
                    no_redirect(restored_ctx, restored_base,
                                "/genre-before-c4/backup-genre", 200)
                    restored_entry = api(restored_ctx, restored_base, "GET",
                        f"/api/admin/content-types/listing_browser_c4/entries/{created['id']}")
                    assert restored_entry["term_ids"] == [term["id"]]
                    assert restored_entry["publication_status"] == "published"
                    checks.append("restore_rolls_back_site_overrides_and_preserves_provider_canonical_routes")

                    edited = api(restored_ctx, restored_base, "PUT",
                        f"/api/admin/content-types/listing_browser_c4/entries/{created['id']}",
                        200, {"title": "C4 editable after restore",
                              "slug": "snapshot-c4", "values": {},
                              "term_ids": [term["id"]]},
                        csrf_restored, restored_entry["version"])
                    assert edited["title"] == "C4 editable after restore"
                    no_redirect(restored_ctx, restored_base, "/before-c4/snapshot-c4", 200)
                    checks.append("restored_provider_entry_is_reeditable_after_recovery")
                finally:
                    stop_server(restored_proc)
            finally:
                browser.close()
    return checks


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--artifact-dir", type=pathlib.Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    artifacts = args.artifact_dir.resolve()
    artifacts.mkdir(parents=True, exist_ok=True)
    result = {
        "status": "FAIL", "platform": platform.platform(), "arch": platform.machine(),
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "checks": [], "port_18964_used": False,
    }
    try:
        result["checks"] = run(binary, artifacts)
        result["status"] = "PASS"
        exit_code = 0
    except Exception as err:
        result["error"] = str(err)
        result["traceback"] = traceback.format_exc()
        exit_code = 1
    (artifacts / "receipt.json").write_text(json.dumps(result, ensure_ascii=False, indent=2))
    print(json.dumps({k: result[k] for k in ("status", "checks", "error") if k in result},
                     ensure_ascii=False))
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
