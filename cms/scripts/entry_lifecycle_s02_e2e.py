#!/usr/bin/env python3
"""Slice S02 real Chrome Playwright E2E: Trash, Restore, Purge, Reading guard,
Editor vs Admin RBAC, direct /edit + Studio Mode 2 trashed guard, dormant retained-Trash
maintenance in ContentTypeSchemaSettings, plugin host confirmation UI, EN/VI, mobile.

Always runs against a disposable temp site root and ephemeral loopback port;
never touches port 18964.
"""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import pathlib
import re
import sqlite3
import subprocess
import tempfile
import traceback
import zipfile

from playwright.sync_api import expect, sync_playwright
from m7_beta_menu_picker_e2e import api_json, admin_login, launch_server, stop_server


def sha(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def accept_once(page, expected: str | None = None, accept: bool = True):
    dialogs = []

    def handler(dialog):
        dialogs.append((dialog.type, dialog.message))
        dialog.accept() if accept else dialog.dismiss()

    page.once("dialog", handler)

    def verify():
        if expected:
            assert len(dialogs) == 1 and expected.lower() in dialogs[0][1].lower(), dialogs
        else:
            assert len(dialogs) == 1, dialogs

    return verify


def encode_leb_u32(val: int) -> bytes:
    out = bytearray()
    while True:
        b = val & 0x7F
        val >>= 7
        if val:
            out.append(b | 0x80)
        else:
            out.append(b)
            return bytes(out)


def encode_leb_s32(val: int) -> bytes:
    out = bytearray()
    more = True
    while more:
        b = val & 0x7F
        val >>= 7
        if (val == 0 and (b & 0x40) == 0) or (val == -1 and (b & 0x40) != 0):
            more = False
        else:
            b |= 0x80
        out.append(b)
    return bytes(out)


def section(sec_id: int, payload: bytes) -> bytes:
    return bytes([sec_id]) + encode_leb_u32(len(payload)) + payload


def encode_str(s: str) -> bytes:
    b = s.encode("utf-8")
    return encode_leb_u32(len(b)) + b


def build_host_call_wasm(req_json: str) -> bytes:
    """Build a valid minimal Wasm module invoking opendash_v1.host_call(req) and host_write_output."""
    req_bytes = req_json.encode("utf-8")
    # Type section: 0: (i32,i32,i32,i32)->i32, 1: (i32,i32)->i32, 2: ()->i32
    types = (
        encode_leb_u32(3)
        + b"\x60\x04\x7f\x7f\x7f\x7f\x01\x7f"
        + b"\x60\x02\x7f\x7f\x01\x7f"
        + b"\x60\x00\x01\x7f"
    )
    # Import section: opendash_v1.host_call (idx 0), opendash_v1.host_write_output (idx 1)
    imports = (
        encode_leb_u32(2)
        + encode_str("opendash_v1") + encode_str("host_call") + b"\x00" + encode_leb_u32(0)
        + encode_str("opendash_v1") + encode_str("host_write_output") + b"\x00" + encode_leb_u32(1)
    )
    # Function section: 1 func of type 2 (idx 2)
    funcs = encode_leb_u32(1) + encode_leb_u32(2)
    # Memory section: min 2 pages
    mems = encode_leb_u32(1) + b"\x00" + encode_leb_u32(2)
    # Export section: memory + opendash_run (func 2)
    exports = (
        encode_leb_u32(2)
        + encode_str("memory") + b"\x02" + encode_leb_u32(0)
        + encode_str("opendash_run") + b"\x00" + encode_leb_u32(2)
    )
    # Code section: 1 local (i32) -> $n = host_call(1024, len, 8192, 16384); host_write_output(8192, $n); return 0
    body = (
        encode_leb_u32(1) + encode_leb_u32(1) + b"\x7f"
        + b"\x41" + encode_leb_s32(1024)
        + b"\x41" + encode_leb_s32(len(req_bytes))
        + b"\x41" + encode_leb_s32(8192)
        + b"\x41" + encode_leb_s32(16384)
        + b"\x10" + encode_leb_u32(0)
        + b"\x21" + encode_leb_u32(0)
        + b"\x41" + encode_leb_s32(8192)
        + b"\x20" + encode_leb_u32(0)
        + b"\x10" + encode_leb_u32(1)
        + b"\x1a"
        + b"\x41" + encode_leb_s32(0)
        + b"\x0b"
    )
    code = encode_leb_u32(1) + encode_leb_u32(len(body)) + body
    # Data section: memory 0, offset 1024, req_bytes
    data = (
        encode_leb_u32(1)
        + b"\x00" + b"\x41" + encode_leb_s32(1024) + b"\x0b"
        + encode_leb_u32(len(req_bytes)) + req_bytes
    )
    return (
        b"\x00asm\x01\x00\x00\x00"
        + section(1, types)
        + section(2, imports)
        + section(3, funcs)
        + section(5, mems)
        + section(7, exports)
        + section(10, code)
        + section(11, data)
    )


def build_plugin_zip(manifest: dict, wasm: bytes) -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as zf:
        zf.writestr("manifest.json", json.dumps(manifest))
        zf.writestr("plugin.wasm", wasm)
    return buf.getvalue()


def run(binary: pathlib.Path, artifacts: pathlib.Path) -> dict:
    checks: list[str] = []
    errors: list[str] = []
    with tempfile.TemporaryDirectory(prefix="s02-e2e-", dir=artifacts.parent) as temp:
        site = pathlib.Path(temp) / "site"
        site.mkdir(mode=0o700)
        server, base = launch_server(binary, site)
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel="chrome", headless=True)
                try:
                    ctx = browser.new_context(viewport={"width": 1440, "height": 900})
                    admin_pw = "S02-Admin-Password-123!"
                    editor_pw = "S02-Editor-Password-123!"
                    api_json(ctx, "POST", base, "/auth/setup", {"username": "admin", "password": admin_pw}, expected=201)
                    csrf = admin_login(ctx, base, "admin", admin_pw)
                    api_json(ctx, "POST", base, "/api/admin/users",
                             {"username": "editor", "password": editor_pw, "role": "editor"}, csrf, expected=201)

                    # Create Post, Home Page (assigned in Reading), normal Page (for Studio Mode 2 guard),
                    # Immediate CPT + entry, and dormant CPT + trashed entry.
                    post = api_json(ctx, "POST", base, "/api/admin/content-types/post/entries",
                                    {"title": "S02 Lifecycle Post", "slug": "s02-lifecycle-post", "values": {"body": "Hello S02"}}, csrf, expected=201)
                    post2 = api_json(ctx, "POST", base, "/api/admin/content-types/post/entries",
                                     {"title": "S02 Plugin Purge Target", "slug": "s02-plugin-purge", "values": {"body": "To purge by plugin"}}, csrf, expected=201)
                    home_page = api_json(ctx, "POST", base, "/api/admin/content-types/page/entries",
                                         {"title": "S02 Assigned Home", "slug": "s02-assigned-home", "values": {"body": "Home"}}, csrf, expected=201)
                    api_json(ctx, "POST", base, f'/api/admin/content-types/page/entries/{home_page["id"]}/publication',
                             {"status": "published"}, csrf, {"If-Match": '"v1"'})
                    studio_page = api_json(ctx, "POST", base, "/api/admin/content-types/page/entries",
                                           {"title": "S02 Studio Guard Page", "slug": "s02-studio-guard", "values": {"body": "Builder body"}}, csrf, expected=201)
                    reading = ctx.request.get(base + "/api/admin/settings/reading")
                    reading_etag = reading.headers["etag"]
                    api_json(ctx, "PUT", base, "/api/admin/settings/reading",
                             {"homepage_page_id": home_page["id"], "posts_page_id": None}, csrf, {"If-Match": reading_etag})

                    # Immediate CPT `catalog`
                    api_json(ctx, "POST", base, "/api/admin/content-types", {
                        "key": "catalog",
                        "label": "Catalog",
                        "fields": [],
                        "settings": {"public": True, "has_archive": False, "publication": "immediate", "supports": ["editor"]},
                    }, csrf, expected=201)
                    cat_entry = api_json(ctx, "POST", base, "/api/admin/content-types/catalog/entries",
                                         {"title": "Immediate Item", "slug": "immediate-item", "values": {}}, csrf, expected=201)

                    # Dormant CPT `legacy_cpt` with 1 trashed row
                    api_json(ctx, "POST", base, "/api/admin/content-types", {
                        "key": "legacy_cpt",
                        "label": "Legacy CPT",
                        "fields": [],
                        "settings": {"public": False, "has_archive": False, "publication": "draft_publish", "supports": ["editor"]},
                    }, csrf, expected=201)
                    legacy_entry = api_json(ctx, "POST", base, "/api/admin/content-types/legacy_cpt/entries",
                                            {"title": "Dormant Trash Item", "slug": "dormant-trash", "values": {}}, csrf, expected=201)
                    api_json(ctx, "POST", base, f'/api/admin/content-types/legacy_cpt/entries/{legacy_entry["id"]}/trash',
                             {}, csrf, {"If-Match": '"v1"'})
                    with sqlite3.connect(site / "database/opendash.sqlite") as db:
                        db.execute(
                            "UPDATE content_types SET owner_kind='plugin', owner_id='old-plugin', provider_status='provider_inactive' WHERE key='legacy_cpt'"
                        )
                        db.commit()

                    # Trash post2 and install a v0.1.1 plugin with Declarative UI + Purge action
                    api_json(ctx, "POST", base, f'/api/admin/content-types/post/entries/{post2["id"]}/trash',
                             {}, csrf, {"If-Match": '"v1"'})
                    purge_wasm = build_host_call_wasm(json.dumps({
                        "op": "content_purge",
                        "type_key": "post",
                        "entry_id": post2["id"],
                        "expected_version": 2,
                    }))
                    purge_manifest = {
                        "id": "s02-purge-ui",
                        "version": "1.0.0",
                        "api_version": 1,
                        "manifest_schema": 3,
                        "min_host_version": "0.1.1",
                        "name": "S02 Purge UI",
                        "permissions": {"content_purge": ["post"]},
                        "actions": [{
                            "id": "purge_post",
                            "title": "Purge Post",
                            "required_role": "admin",
                            "capabilities": ["content_purge"],
                        }],
                    }
                    inst = ctx.request.post(
                        base + "/api/admin/plugins",
                        headers={"X-CSRF-Token": csrf, "Content-Type": "application/zip", "Origin": base},
                        data=build_plugin_zip(purge_manifest, purge_wasm),
                    )
                    assert inst.status == 201, (inst.status, inst.text())

                    page = ctx.new_page()
                    page.on("pageerror", lambda err: errors.append(str(err)))

                    # 1. Posts Trash -> Restore -> Trash -> Purge flow
                    page.goto(base + "/admin/#/posts", wait_until="domcontentloaded")
                    root = page.get_by_test_id("entry-list-post")
                    expect(root.get_by_test_id(f'entry-trash-btn-{post["id"]}')).to_be_visible(timeout=20000)
                    verify = accept_once(page, "trash")
                    root.get_by_test_id(f'entry-trash-btn-{post["id"]}').click()
                    verify()
                    expect(root.get_by_test_id("entry-status-tab-trash")).to_contain_text("(2)", timeout=15000)
                    root.get_by_test_id("entry-status-tab-trash").click()
                    expect(root.get_by_test_id(f'entry-restore-btn-{post["id"]}')).to_be_visible(timeout=15000)
                    expect(root.get_by_test_id(f'entry-purge-btn-{post["id"]}')).to_be_visible()
                    page.screenshot(path=str(artifacts / "posts-trash-view.png"), full_page=True)

                    # Restore post back to draft
                    root.get_by_test_id(f'entry-restore-btn-{post["id"]}').click()
                    expect(root.get_by_test_id("entry-status-tab-trash")).to_contain_text("(1)", timeout=15000)
                    checks.append("admin_trash_and_restore_post_flow")

                    # 2. Reading-assigned Page disabled Trash + Reading Settings link
                    page.goto(base + "/admin/#/pages", wait_until="domcontentloaded")
                    pages_root = page.get_by_test_id("entry-list-page")
                    expect(pages_root.get_by_test_id(f'entry-trash-btn-{home_page["id"]}')).to_be_disabled(timeout=15000)
                    expect(pages_root.get_by_test_id(f'reading-blocked-hint-{home_page["id"]}')).to_be_visible()
                    expect(pages_root.get_by_test_id(f'reading-settings-link-{home_page["id"]}')).to_be_visible()
                    page.screenshot(path=str(artifacts / "pages-reading-blocked.png"), full_page=True)
                    checks.append("reading_assigned_page_trash_blocked_with_settings_link")

                    # 3. Direct /edit and Studio Mode 2 on trashed page show Trash guard + Open Trash button
                    api_json(ctx, "POST", base, f'/api/admin/content-types/page/entries/{studio_page["id"]}/trash',
                             {}, csrf, {"If-Match": '"v1"'})
                    page.goto(base + f'/admin/#/pages/{studio_page["id"]}/edit', wait_until="domcontentloaded")
                    expect(page.get_by_test_id("single-edit-trashed-guard")).to_be_visible(timeout=15000)
                    expect(page.get_by_test_id("single-edit-open-trash-btn")).to_be_visible()
                    page.goto(base + f'/admin/#/studio/entry/page/{studio_page["id"]}', wait_until="domcontentloaded")
                    expect(page.get_by_test_id("studio-open-trash-link")).to_be_visible(timeout=15000)
                    checks.append("direct_edit_and_studio_mode2_trashed_guards")

                    # 4. Immediate CPT Trash -> Restore with public exposure confirm -> Published
                    page.goto(base + "/admin/#/cpt/catalog", wait_until="domcontentloaded")
                    cat_root = page.get_by_test_id("entry-list-catalog")
                    expect(cat_root.get_by_test_id(f'entry-trash-btn-{cat_entry["id"]}')).to_be_visible(timeout=15000)
                    v = accept_once(page, "trash")
                    cat_root.get_by_test_id(f'entry-trash-btn-{cat_entry["id"]}').click()
                    v()
                    cat_root.get_by_test_id("entry-status-tab-trash").click()
                    expect(cat_root.get_by_test_id(f'entry-restore-btn-{cat_entry["id"]}')).to_be_visible(timeout=15000)
                    v = accept_once(page, "public")
                    cat_root.get_by_test_id(f'entry-restore-btn-{cat_entry["id"]}').click()
                    v()
                    expect(cat_root.get_by_test_id("entry-status-tab-trash")).to_contain_text("(0)", timeout=15000)
                    checks.append("immediate_cpt_restore_public_warning_confirm")

                    # 5. Dormant Retained Trash panel in ContentTypeSchemaSettings
                    page.goto(base + "/admin/#/settings/content-types/legacy_cpt", wait_until="domcontentloaded")
                    retained_panel = page.get_by_test_id("retained-trash-maintenance")
                    expect(retained_panel).to_be_visible(timeout=15000)
                    expect(retained_panel.get_by_text("Dormant Trash Item")).to_be_visible()
                    page.screenshot(path=str(artifacts / "dormant-retained-trash.png"), full_page=True)
                    v = accept_once(page, "permanently delete")
                    retained_panel.get_by_test_id(f'retained-trash-purge-{legacy_entry["id"]}').click()
                    v()
                    expect(retained_panel.get_by_text("No retained Trash entries")).to_be_visible(timeout=15000)
                    checks.append("admin_dormant_retained_trash_purge_panel")

                    # 6. Core-owned PluginLifecycleConfirmation UI from Declarative Plugin UI (cancel + confirm)
                    page.goto(base + "/admin/#/plugins", wait_until="domcontentloaded")
                    expect(page.locator('article[data-plugin-id="s02-purge-ui"]')).to_be_visible(timeout=15000)
                    page.locator('article[data-plugin-id="s02-purge-ui"]').get_by_role("button", name=re.compile("Select(?:ed)? to run", re.I)).click()
                    page.get_by_role("tab", name=re.compile("Run|Test", re.I)).click()
                    run_btn = page.get_by_role("button", name="Thực thi")
                    expect(run_btn).to_be_visible(timeout=15000)
                    run_btn.click()
                    conf = page.get_by_test_id("plugin-lifecycle-confirmation")
                    expect(conf).to_be_visible(timeout=15000)
                    expect(conf).to_contain_text("S02 Plugin Purge Target")
                    page.screenshot(path=str(artifacts / "plugin-lifecycle-confirmation.png"), full_page=True)
                    conf.get_by_test_id("plugin-lifecycle-cancel-btn").click()
                    expect(conf).to_have_count(0, timeout=15000)
                    # Re-run and Confirm -> physically deletes post2
                    run_btn.click()
                    expect(conf).to_be_visible(timeout=15000)
                    conf.get_by_test_id("plugin-lifecycle-confirm-btn").click()
                    expect(conf).to_have_count(0, timeout=15000)
                    expect(page.locator("pre").filter(has_text='"operation": "purge"')).to_be_visible(timeout=15000)
                    checks.append("plugin_lifecycle_confirmation_cancel_and_confirm")

                    # 7. Mobile viewport (390px) no horizontal overflow on Trash tab
                    page.set_viewport_size({"width": 390, "height": 844})
                    page.goto(base + "/admin/#/pages?status=trash", wait_until="domcontentloaded")
                    expect(page.get_by_test_id("entry-list-page")).to_be_visible(timeout=15000)
                    sizes = page.evaluate("({scroll:document.documentElement.scrollWidth,client:document.documentElement.clientWidth})")
                    assert sizes["scroll"] <= sizes["client"] + 2, sizes
                    page.screenshot(path=str(artifacts / "trash-mobile-390.png"), full_page=True)
                    checks.append("mobile_390_no_horizontal_overflow")

                    # 8. Editor RBAC: sees Trash & Restore, NEVER sees Purge button
                    ed_ctx = browser.new_context(viewport={"width": 1440, "height": 900})
                    admin_login(ed_ctx, base, "editor", editor_pw)
                    ed_page = ed_ctx.new_page()
                    ed_page.on("pageerror", lambda err: errors.append(str(err)))
                    ed_page.goto(base + "/admin/#/pages?status=trash", wait_until="domcontentloaded")
                    ed_root = ed_page.get_by_test_id("entry-list-page")
                    expect(ed_root.get_by_test_id(f'entry-restore-btn-{studio_page["id"]}')).to_be_visible(timeout=15000)
                    assert ed_root.get_by_test_id(f'entry-purge-btn-{studio_page["id"]}').count() == 0
                    ed_ctx.close()
                    checks.append("editor_rbac_trash_restore_without_purge")

                    assert not errors, errors
                    ctx.close()
                    return {"checks": checks, "chrome_version": browser.version}
                finally:
                    browser.close()
        finally:
            stop_server(server)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--artifact-dir", type=pathlib.Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    artifacts = args.artifact_dir.resolve()
    artifacts.mkdir(parents=True, exist_ok=True)
    receipt = {
        "status": "FAIL",
        "source_sha": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        "binary_sha256": sha(binary),
        "checks": [],
    }
    try:
        receipt.update(run(binary, artifacts))
        receipt["status"] = "PASS"
    except Exception as exc:
        receipt["error"] = f"{type(exc).__name__}: {exc}"
        traceback.print_exc()
    finally:
        receipt["screenshots_sha256"] = {f.name: sha(f) for f in artifacts.glob("*.png")}
        (artifacts / "receipt.json").write_text(json.dumps(receipt, indent=2, ensure_ascii=False) + "\n")
        print(json.dumps({k: receipt.get(k) for k in ("status", "source_sha", "checks", "error")}, ensure_ascii=False))
    return 0 if receipt["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
