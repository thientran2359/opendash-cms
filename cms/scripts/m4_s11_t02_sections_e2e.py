#!/usr/bin/env python3
"""#355 native Chrome + SQLite Section contract on one disposable site.

NEVER connect to the owner's port 18964 or reuse a real browser profile.
Usage: python3 cms/scripts/m4_s11_t02_sections_e2e.py --binary /path/opendash --artifact-dir /private/path
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import pathlib
import re
import shutil
import sqlite3
import subprocess
import tempfile
import time
import traceback

from playwright.sync_api import expect, sync_playwright
from m7_beta_menu_picker_e2e import admin_login, api_json, free_port, launch_server, stop_server
from m4_s11_t01_page_builder_e2e import new_heading, change_heading
from m6_s10_t01_admin_extensions_e2e import PLUGIN_JSON, THEME_JSON, upload as upload_provider
from media_smoke import make_png


def sha(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def create_ui_section(page, base: str, title: str, slug: str) -> int:
    page.goto(base + "/admin/#/sections/new", wait_until="domcontentloaded")
    expect(page.get_by_test_id("section-workspace")).to_be_visible(timeout=20000)
    page.get_by_role("textbox", name="Tiêu đề Section").fill(title)
    page.get_by_role("textbox", name="Slug Section").fill(slug)
    page.get_by_role("button", name="Lưu Section").click()
    expect(page).to_have_url(re.compile(r"#/sections/[1-9]\d*/edit$"), timeout=20000)
    return int(re.search(r"#/sections/(\d+)/edit", page.url).group(1))


def check_upgrade_and_collision(binary: pathlib.Path, old_binary: pathlib.Path, artifacts: pathlib.Path) -> list[str]:
    checks = []
    with tempfile.TemporaryDirectory(prefix="opendash-m4-s11-t02-upgrade-") as tmp:
        work = pathlib.Path(tmp).resolve()
        legacy = work / "legacy-site"
        legacy.mkdir(mode=0o700)
        proc, base = launch_server(old_binary, legacy)
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel="chrome", headless=True)
                try:
                    ctx = browser.new_context()
                    api_json(ctx,"POST",base,"/auth/setup",
                             {"username":"old_admin","password":"OldPassword123!"},expected=201)
                    csrf = admin_login(ctx,base,"old_admin","OldPassword123!")
                    prior = api_json(ctx,"POST",base,"/api/admin/content-types/page/entries",
                        {"title":"Pre-upgrade Page must survive","slug":"preupgrade-survive","values":{}},csrf,expected=201)
                    old_id = prior["id"]
                    ctx.close()
                finally:
                    browser.close()
        finally:
            stop_server(proc)
        old_db = legacy / "database/opendash.sqlite"
        with sqlite3.connect(old_db) as db:
            assert not db.execute("SELECT 1 FROM _sqlx_migrations WHERE version=37").fetchone()
            assert not db.execute("SELECT name FROM sqlite_master WHERE name='section_documents'").fetchone()
        clean = work / "clean-upgrade"
        collision = work / "collision-upgrade"
        shutil.copytree(legacy, clean)
        shutil.copytree(legacy, collision)
        with sqlite3.connect(collision / "database/opendash.sqlite") as db:
            db.execute("INSERT INTO content_types(key,definition_json,owner_kind,owner_id,provider_status,declaration_state) VALUES(?,?,'user',NULL,'active','declared')",
                ("section",json.dumps({"key":"section","label":"User Section","fields":[]})))
            db.commit()

        # A collision must stop BEFORE migration, without claiming a user key.
        port = free_port()
        env = dict(os.environ, OPENDASH_SITE_ROOT=str(collision),
                   OPENDASH_DB_PATH=str(collision / "database/opendash.sqlite"),
                   OPENDASH_PORT=str(port))
        env.pop("OPENDASH_PUBLIC_ORIGIN",None)
        stderr = artifacts / "upgrade-collision-stderr.log"
        with stderr.open("wb") as log:
            colliding = subprocess.Popen([str(binary)],env=env,stdout=subprocess.DEVNULL,stderr=log)
            try: exit_status = colliding.wait(timeout=18)
            except subprocess.TimeoutExpired:
                colliding.terminate()
                colliding.wait(timeout=8)
                raise AssertionError("Colliding section bootstrap did not fail closed")
        assert exit_status != 0
        assert "section" in stderr.read_text(encoding="utf-8",errors="replace").lower()
        with sqlite3.connect(collision / "database/opendash.sqlite") as db:
            assert db.execute("SELECT owner_kind,definition_json FROM content_types WHERE key='section'").fetchone()[0] == 'user'
            assert not db.execute("SELECT 1 FROM _sqlx_migrations WHERE version=37").fetchone()
            assert db.execute("SELECT title FROM content_entries WHERE id=?",(old_id,)).fetchone()[0] == 'Pre-upgrade Page must survive'
        checks.append("0036_noncore_section_collision_fail_closed_before_migration_old_data_intact")

        upgraded, clean_base = launch_server(binary,clean)
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel="chrome",headless=True)
                try:
                    ctx = browser.new_context()
                    new_csrf = admin_login(ctx,clean_base,"old_admin","OldPassword123!")
                    assert api_json(ctx,"GET",clean_base,
                        f"/api/admin/content-types/page/entries/{old_id}")["title"] == "Pre-upgrade Page must survive"
                    assert api_json(ctx,"GET",clean_base,"/api/admin/sections") == []
                    api_json(ctx,"POST",clean_base,"/api/admin/sections",
                             {"title":"New Section","slug":"new-section"},new_csrf,expected=201)
                    ctx.close()
                finally:
                    browser.close()
        finally:
            stop_server(upgraded)
        with sqlite3.connect(clean / "database/opendash.sqlite") as db:
            assert db.execute("SELECT 1 FROM _sqlx_migrations WHERE version=37").fetchone()[0] == 1
            assert db.execute("PRAGMA integrity_check").fetchone()[0] == 'ok'
            assert db.execute("PRAGMA foreign_key_check").fetchall() == []
            assert db.execute("SELECT COUNT(*) FROM section_documents").fetchone()[0] == 1
        checks.append("0036_to_0037_forward_upgrade_preserves_Page_new_private_Section_and_SQLite_integrity")

        # Safe old→new→old sequence: rollback must use the unmodified
        # pre-upgrade snapshot, never run the old binary against a 0037 DB.
        rolled_back, rollback_base = launch_server(old_binary, legacy)
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel="chrome", headless=True)
                try:
                    ctx = browser.new_context()
                    admin_login(ctx, rollback_base, "old_admin", "OldPassword123!")
                    assert api_json(ctx, "GET", rollback_base,
                        f"/api/admin/content-types/page/entries/{old_id}")["title"] == "Pre-upgrade Page must survive"
                    assert ctx.request.get(rollback_base + "/api/admin/sections").status == 404
                    ctx.close()
                finally:
                    browser.close()
        finally:
            stop_server(rolled_back)
        with sqlite3.connect(old_db) as db:
            assert not db.execute("SELECT 1 FROM _sqlx_migrations WHERE version=37").fetchone()
            assert db.execute("PRAGMA integrity_check").fetchone()[0] == 'ok'
        checks.append("old_new_old_rollback_on_immutable_preupgrade_snapshot_no_downgrade")
    return checks


def poll_operation(ctx, base: str, job_id: str) -> dict:
    deadline = time.monotonic() + 70
    while time.monotonic() < deadline:
        job = api_json(ctx, "GET", base, f"/api/admin/operations/jobs/{job_id}")
        if job["state"] in ("succeeded", "failed", "canceled"):
            assert job["state"] == "succeeded", job
            return job
        time.sleep(0.2)
    raise AssertionError("Private backup/restore operation did not complete")


def check_encrypted_backup_restore(binary: pathlib.Path, artifacts: pathlib.Path) -> list[str]:
    """Native encrypted backup → separate restore root → actual server startup."""
    with tempfile.TemporaryDirectory(prefix="opendash-m4-s11-t02-backup-") as tmp:
        source = pathlib.Path(tmp).resolve() / "source-site"
        source.mkdir(mode=0o700)
        server, base = launch_server(binary, source)
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel="chrome",headless=True)
                try:
                    ctx = browser.new_context()
                    api_json(ctx,"POST",base,"/auth/setup",
                             {"username":"backup_admin","password":"BackupPassword123!"},expected=201)
                    csrf = admin_login(ctx,base,"backup_admin","BackupPassword123!")
                    section = api_json(ctx,"POST",base,"/api/admin/sections",
                        {"title":"Section must survive encrypted restore","slug":"survive-restore"},csrf,expected=201)
                    sid = section["id"]
                    document = {"schema_version":1,"sections":[{"id":"restore-sec","children":[
                        {"id":"restore-row","children":[{"id":"restore-col","children":[
                            {"id":"restore-heading","type":"core/heading",
                             "props":{"level":2,"text":"ENCRYPTED RESTORE SECTION AST"}}
                        ]}]}]}]}
                    api_json(ctx,"PUT",base,f"/api/admin/sections/{sid}/document",document,csrf,
                             {"If-Match":'"v1"'})
                    passphrase = "Section-M4-T02-private-backup-passphrase"
                    backup = api_json(ctx,"POST",base,"/api/admin/operations/backups",
                        {"passphrase":passphrase,"confirm_passphrase":passphrase,
                         "reason":"M4-S11-T02 private Section preservation"},csrf,expected=202)
                    poll_operation(ctx,base,backup["id"])
                    content = ctx.request.get(base+f'/api/admin/operations/backups/{backup["id"]}/file')
                    assert content.status == 200,content.text()[:150]
                    encrypted = content.body()
                    assert len(encrypted) > 64
                    (artifacts / "encrypted-backup-sha256.txt").write_text(
                        hashlib.sha256(encrypted).hexdigest()+"\n",encoding="ascii")
                    upload = ctx.request.post(base+"/api/admin/operations/restore-uploads",data=encrypted,
                        headers={"Origin":base,"X-CSRF-Token":csrf,"Content-Type":"application/octet-stream"})
                    assert upload.status == 201,upload.text()[:300]
                    upload_id = upload.json()["upload_id"]
                    restoration = api_json(ctx,"POST",base,"/api/admin/operations/restores",
                        {"upload_id":upload_id,"passphrase":passphrase,
                         "reason":"M4-S11-T02 restore Section"},csrf,expected=202)
                    restored = poll_operation(ctx,base,restoration["id"])
                    summary = json.loads(restored["summary_json"])
                    restored_root = pathlib.Path(summary["target_root"])
                    assert (restored_root/"READY.json").is_file()
                    restored_db = restored_root/"database/opendash.sqlite"
                    assert restored_db.is_file()
                    ctx.close()
                finally:
                    browser.close()
        finally:
            stop_server(server)

        with sqlite3.connect(restored_db) as db:
            assert db.execute("PRAGMA integrity_check").fetchone()[0] == 'ok'
            assert db.execute("PRAGMA foreign_key_check").fetchall() == []
            assert db.execute("SELECT publication_status FROM content_entries WHERE id=? AND content_type_key='section'",(sid,)).fetchone() == ('draft',)
            raw = db.execute("SELECT document_json FROM section_documents WHERE entry_id=?",(sid,)).fetchone()[0]
            assert json.loads(raw) == document

        target, base2 = launch_server(binary,restored_root)
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel="chrome",headless=True)
                try:
                    ctx2 = browser.new_context()
                    admin_login(ctx2,base2,"backup_admin","BackupPassword123!")
                    assert api_json(ctx2,"GET",base2,f"/api/admin/sections/{sid}/document")["document"] == document
                    assert ctx2.request.get(base2+"/content/section/survive-restore").status == 404
                    ctx2.close()
                finally:
                    browser.close()
        finally:
            stop_server(target)
    return ["encrypted_backup_restore_complete_private_Section_AST_integrity_native_restart"]


def run(binary: pathlib.Path, artifacts: pathlib.Path) -> dict:
    checks: list[str] = []
    errors: list[str] = []
    source = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    with tempfile.TemporaryDirectory(prefix="opendash-m4-s11-t02-") as tmp:
        root = pathlib.Path(tmp).resolve() / "site"
        root.mkdir(mode=0o700)
        server, base = launch_server(binary, root)
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel="chrome", headless=True)
                try:
                    admin = browser.new_context(viewport={"width": 1440, "height": 900})
                    page = admin.new_page()
                    page.on("pageerror", lambda exc: errors.append(str(exc)))
                    api_json(admin, "POST", base, "/auth/setup", {"username": "site_admin", "password": "AdminPassword123!"}, expected=201)
                    csrf = admin_login(admin, base, "site_admin", "AdminPassword123!")
                    homepage_before = api_json(admin, "GET", base, "/api/admin/builder/homepage")
                    a = create_ui_section(page, base, "Section A UI", "section-a")
                    b = create_ui_section(page, base, "Section B UI", "section-b")
                    assert a != b
                    checks.append("Chrome_UI_created_two_independent_private_Sections")
                    page.goto(base + "/admin/#/sections", wait_until="domcontentloaded")
                    expect(page.get_by_text("Section A UI")).to_be_visible(timeout=15000)
                    expect(page.get_by_text("Section B UI")).to_be_visible(timeout=15000)
                    checks.append("Section_list_routes_reopen_both_entries")

                    homepage_calls = []
                    page.on("request", lambda req: homepage_calls.append(req.url)
                            if "/api/admin/builder/homepage" in req.url else None)
                    page.goto(base + f"/admin/#/sections/{a}/edit", wait_until="domcontentloaded")
                    page.get_by_role("button", name="Mở Section Builder").click()
                    expect(page).to_have_url(base + f"/admin/#/builder?template=section&type=section&entry={a}", timeout=20000)
                    expect(page.get_by_role("heading", name=f"Section Builder #{a}")).to_be_visible(timeout=20000)
                    expect(page.get_by_test_id("builder-workspace")).to_contain_text("Đã mở", timeout=20000)
                    assert not homepage_calls, homepage_calls
                    checks.append("Section_editor_Builder_exact_ID_never_fetches_homepage")
                    page.screenshot(path=str(artifacts / "section-a-before.png"), full_page=True)

                    new_heading(page, "PRIVATE SECTION A DESIGN")
                    page.get_by_role("button", name="Xem trước").click()
                    expect(page.frame_locator("#canvas").locator("body")).to_contain_text("PRIVATE SECTION A DESIGN", timeout=20000)
                    url_a = f"/api/admin/sections/{a}/document"
                    url_b = f"/api/admin/sections/{b}/document"
                    assert api_json(admin, "GET", base, url_a)["document"]["sections"] == []
                    page.get_by_role("button", name="Lưu Section").click()
                    expect(page.get_by_test_id("builder-workspace")).to_contain_text("Đã lưu Section", timeout=20000)
                    doc_a = api_json(admin, "GET", base, url_a)
                    assert doc_a["entry_id"] == a and doc_a["version"] == 2
                    assert "PRIVATE SECTION A DESIGN" in json.dumps(doc_a)
                    assert api_json(admin, "GET", base, url_b)["document"]["sections"] == []
                    assert api_json(admin, "GET", base, "/api/admin/builder/homepage") == homepage_before
                    checks.append("Authenticated_preview_read_only_save_Section_A_CAS_B_and_homepage_isolated")

                    # A private Section media pin never grants public URL access.
                    uploaded = admin.request.post(base + "/api/admin/media", data=make_png(seed=78),
                        headers={"Origin":base,"X-CSRF-Token":csrf,
                                 "Content-Type":"image/png","X-File-Name":"section-private.png"})
                    assert uploaded.status == 201, uploaded.text()[:300]
                    image_id = uploaded.json()["id"]
                    media_design = copy.deepcopy(doc_a["document"])
                    media_design["sections"][0]["children"][0]["children"][0]["children"].append({
                        "id": "private-image", "type": "core/image", "props": {"media_id": image_id, "alt": "Private Section"},
                    })
                    private_preview = admin.request.post(base + url_a + "/preview", data=json.dumps(media_design),
                        headers={"Origin":base,"X-CSRF-Token":csrf,"Content-Type":"application/json"})
                    assert private_preview.status == 200
                    assert private_preview.headers.get("cache-control") == "no-store"
                    anon = browser.new_context()
                    assert anon.request.get(base + f"/media/{image_id}/file").status == 404
                    doc_a = api_json(admin, "PUT", base, url_a, media_design, csrf,
                                     {"If-Match":f'"v{doc_a["version"]}"'})
                    assert anon.request.get(base + f"/media/{image_id}/file").status == 404
                    assert anon.request.get(base + "/content/section").status == 404
                    assert anon.request.get(base + "/content/section/section-a").status == 404
                    assert anon.request.get(base + url_a).status == 401
                    anon.close()
                    checks.append("Private_Section_media_refs_no_public_media_or_archive_or_single")

                    # A private Section pin is a first-class 409 media deletion
                    # blocker, not just a FK failure after media has vanished.
                    meta = admin.request.get(base + f"/api/admin/media/{image_id}")
                    assert meta.status == 200,meta.text()[:300]
                    media_delete = admin.request.delete(base + f"/api/admin/media/{image_id}", headers={
                        "Origin":base,"X-CSRF-Token":csrf,"If-Match":meta.headers["etag"],
                    })
                    assert media_delete.status == 409,media_delete.text()[:300]
                    pins = media_delete.json()["pins"]
                    assert any(p["source_kind"] == "section_document" and p["entity_id"] == str(a)
                               for p in pins["pins"]),pins
                    assert api_json(admin,"GET",base,f"/api/admin/media/{image_id}")["lifecycle_state"] == "active"
                    checks.append("Private_Section_media_delete_fails_409_atomically_and_remains_active")

                    # A metadata update changes the shared version but retains its AST.
                    updated = api_json(admin, "PUT", base, f"/api/admin/sections/{a}",
                        {"title":"Section A renamed","slug":"section-a"}, csrf,
                        {"If-Match":f'"v{doc_a["version"]}"'})
                    assert updated["version"] == doc_a["version"] + 1
                    assert api_json(admin, "GET", base, url_a)["document"] == media_design
                    # The existing Chrome Builder version is stale; 412 retains local draft.
                    change_heading(page, "SECTION A LOCAL DIRTY 412")
                    page.get_by_role("button", name="Lưu Section").click()
                    expect(page.get_by_test_id("builder-workspace")).to_contain_text("412", timeout=15000)
                    assert "SECTION A LOCAL DIRTY 412" in page.get_by_test_id("builder-workspace").inner_text()
                    assert api_json(admin, "GET", base, url_a)["document"] == media_design
                    checks.append("Metadata_CAS_preserves_AST_and_Chrome_412_preserves_unsaved_draft")

                    # Switch targets: Cancel must retain URL/AST; Discard must show B.
                    dialogs = []
                    def decline(dialog): dialogs.append(dialog.message); dialog.dismiss()
                    def accept(dialog): dialogs.append(dialog.message); dialog.accept()
                    page.on("dialog", decline)
                    page.evaluate(f"location.hash = '#/builder?template=section&type=section&entry={b}'")
                    expect(page).to_have_url(re.compile(rf"entry={a}$"), timeout=12000)
                    assert "SECTION A LOCAL DIRTY 412" in page.get_by_test_id("builder-workspace").inner_text()
                    assert dialogs and "Bỏ nháp" in dialogs[-1]
                    page.remove_listener("dialog", decline)
                    page.on("dialog", accept)
                    page.evaluate(f"location.hash = '#/builder?template=section&type=section&entry={b}'")
                    expect(page).to_have_url(re.compile(rf"entry={b}$"), timeout=12000)
                    expect(page.get_by_role("heading", name=f"Section Builder #{b}")).to_be_visible()
                    expect(page.get_by_test_id("builder-workspace")).to_contain_text("Đã mở", timeout=15000)
                    assert "SECTION A LOCAL DIRTY 412" not in page.get_by_test_id("builder-workspace").inner_text()
                    checks.append("Dirty_target_Cancel_preserves_AST_URL_Discard_switches_exact_Section_B")

                    new_heading(page, "SECTION B ONLY")
                    page.get_by_role("button", name="Lưu Section").click()
                    expect(page.get_by_test_id("builder-workspace")).to_contain_text("Đã lưu Section", timeout=15000)
                    doc_b = api_json(admin, "GET", base, url_b)
                    assert doc_b["version"] == 2 and doc_b["document"] != doc_a["document"]
                    assert api_json(admin, "GET", base, url_a)["document"] == media_design
                    checks.append("Section_B_design_isolated_from_A_and_homepage")

                    # Exercise actual provider lifecycle on the same isolated
                    # site. Core Section documents do not belong to a theme or
                    # plugin and must survive install/activate/withdraw cycles.
                    provider_plugin = json.loads(PLUGIN_JSON.read_text(encoding="utf-8"))
                    upload_provider(admin, base, "plugin", provider_plugin, csrf, expected=201)
                    assert api_json(admin, "GET", base, url_a)["document"] == media_design
                    plugin_row = next(p for p in api_json(admin, "GET", base, "/api/admin/plugins")
                                      if p["id"] == provider_plugin["id"])
                    api_json(admin, "POST", base, f'/api/admin/plugins/{provider_plugin["id"]}/disable',
                             csrf=csrf, extra={"If-Match": str(plugin_row["cas_version"])})
                    assert api_json(admin, "GET", base, url_a)["document"] == media_design
                    assert api_json(admin, "GET", base, url_b)["document"] == doc_b["document"]
                    plugin_row = next(p for p in api_json(admin, "GET", base, "/api/admin/plugins")
                                      if p["id"] == provider_plugin["id"])
                    api_json(admin, "POST", base, f'/api/admin/plugins/{provider_plugin["id"]}/enable',
                             csrf=csrf, extra={"If-Match": str(plugin_row["cas_version"])})
                    assert api_json(admin, "GET", base, url_a)["document"] == media_design
                    checks.append("real_plugin_install_disable_reenable_preserves_both_private_Section_ASTs")

                    provider_theme = json.loads(THEME_JSON.read_text(encoding="utf-8"))
                    first_theme = upload_provider(admin, base, "theme", provider_theme, csrf, expected=201)
                    alt_theme = copy.deepcopy(provider_theme)
                    alt_theme["theme_id"] = "section-test-alternative"
                    alt_theme["name"] = "Section test alternative"
                    alt_theme.pop("admin_extensions", None)
                    second_theme = upload_provider(admin, base, "theme", alt_theme, csrf, expected=201)
                    for theme_revision in (first_theme, second_theme, first_theme):
                        summary = api_json(admin, "GET", base, "/api/admin/themes")
                        api_json(admin, "PUT", base, "/api/admin/themes/active",
                                 {"revision_id": theme_revision["revision_id"]}, csrf,
                                 {"If-Match": f'"v{summary["version"]}"'})
                        assert api_json(admin, "GET", base, url_a)["document"] == media_design
                        assert api_json(admin, "GET", base, url_b)["document"] == doc_b["document"]
                    checks.append("real_theme_switch_and_rollback_preserves_private_Section_ASTs")

                    # Editor has exact private Section rights but not global templates.
                    api_json(admin, "POST", base, "/api/admin/users", {
                        "username":"section_editor","password":"EditorPassword123!","role":"editor",
                    }, csrf, expected=201)
                    editor = browser.new_context(viewport={"width":390,"height":844})
                    editor_csrf = admin_login(editor, base, "section_editor", "EditorPassword123!")
                    editor_page = editor.new_page()
                    editor_page.goto(base + f"/admin/#/sections/{b}/edit", wait_until="domcontentloaded")
                    editor_page.get_by_role("button", name="Mở Section Builder").click()
                    expect(editor_page.get_by_role("heading", name=f"Section Builder #{b}")).to_be_visible(timeout=20000)
                    assert editor.request.get(base + url_b).status == 200
                    assert editor.request.get(base + "/api/admin/builder/homepage").status == 403
                    assert editor_page.locator("#builder-role-target").is_disabled()
                    bad_csrf = editor.request.post(base + url_b + "/preview", data=json.dumps(doc_b["document"]),
                        headers={"Origin":base,"Content-Type":"application/json"})
                    assert bad_csrf.status == 403
                    bad_origin = editor.request.post(base + url_b + "/preview", data=json.dumps(doc_b["document"]),
                        headers={"Origin":"https://invalid.example","X-CSRF-Token":editor_csrf,"Content-Type":"application/json"})
                    assert bad_origin.status == 403
                    for path in ["/api/admin/sections/99999999/document", "/api/admin/sections/0/document"]:
                        assert editor.request.get(base + path).status in (400,404)
                    editor_page.screenshot(path=str(artifacts / "section-editor-390.png"), full_page=True)
                    assert editor_page.evaluate("document.body.scrollWidth <= window.innerWidth + 2"), "390px body horizontal overflow"
                    change_heading(editor_page,"EDITOR SAVED PRIVATE SECTION B")
                    editor_page.get_by_role("button",name="Lưu Section").click()
                    expect(editor_page.get_by_test_id("builder-workspace")).to_contain_text("Đã lưu Section",timeout=15000)
                    doc_b = api_json(editor,"GET",base,url_b)
                    assert "EDITOR SAVED PRIVATE SECTION B" in json.dumps(doc_b)
                    editor.close()
                    checks.append("Editor_exact_Section_scope_CAN_save_no_global_CSRF_Origin_and_390px")

                    # Generic APIs must not mutate Section metadata or document.
                    injection = api_json(admin, "GET", base, f"/api/admin/sections/{a}")
                    generic_put = admin.request.put(base + f"/api/admin/content-types/section/entries/{a}",
                        data=json.dumps({"title":"Injected","slug":"section-a","values":{"body":"leak"}}),
                        headers={"Origin":base,"X-CSRF-Token":csrf,"Content-Type":"application/json",
                                 "If-Match":f'"v{injection["version"]}"'})
                    assert generic_put.status in (400,403,422)
                    assert api_json(admin, "GET", base, url_a)["document"] == media_design
                    checks.append("Generic_Section_field_injection_rejected_without_document_loss")

                    # Invalid Builder AST cannot change either the Section
                    # document or its shared entry version.
                    invalid_doc = copy.deepcopy(doc_b["document"])
                    invalid_doc["sections"][0]["children"][0]["children"][0]["children"][0] = {
                        "id":"invalid-media-element", "type":"core/image",
                        "props":{"media_id":"nonexistent-image-id","alt":"must reject"},
                    }
                    invalid = admin.request.put(base+url_b,data=json.dumps(invalid_doc),headers={
                        "Origin":base,"X-CSRF-Token":csrf,"Content-Type":"application/json",
                        "If-Match":f'"v{doc_b["version"]}"',
                    })
                    assert invalid.status == 400,invalid.text()[:200]
                    assert api_json(admin,"GET",base,url_b) == doc_b
                    checks.append("Invalid_AST_rejected_atomically_version_and_persisted_document_unchanged")

                    # Section deletion cascades exactly its own private
                    # document + pins, never references owned by A or B.
                    c = api_json(admin,"POST",base,"/api/admin/sections",
                        {"title":"Disposable Section C","slug":"disposable-c"},csrf,expected=201)
                    api_json(admin,"PUT",base,f'/api/admin/sections/{c["id"]}/document',
                        media_design,csrf,{"If-Match":'"v1"'})
                    deleted = admin.request.delete(base+f'/api/admin/sections/{c["id"]}',headers={
                        "Origin":base,"X-CSRF-Token":csrf,"If-Match":'"v2"',
                    })
                    assert deleted.status == 204,deleted.text()[:200]
                    assert admin.request.get(base+f'/api/admin/sections/{c["id"]}/document').status == 404
                    checks.append("Section_delete_cascades_only_owned_document_and_media_pins")

                    with sqlite3.connect(root / "database/opendash.sqlite") as db:
                        assert db.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
                        assert db.execute("PRAGMA foreign_key_check").fetchall() == []
                        rows = db.execute("SELECT e.id,e.publication_status,sd.entry_id FROM content_entries e JOIN section_documents sd ON sd.entry_id=e.id WHERE e.content_type_key='section' ORDER BY e.id").fetchall()
                        assert rows == [(a,"draft",a),(b,"draft",b)], rows
                        pins = db.execute("SELECT entry_id,media_id,ref_role FROM section_document_media_refs").fetchall()
                        assert (a,image_id,"image") in pins
                        assert not any(row[0] == c["id"] for row in pins)
                    checks.append("SQLite_private_documents_and_section_specific_media_pins_integrity")
                    assert not errors,errors
                    checks.append("Chrome_no_uncaught_JS_errors")
                    page.screenshot(path=str(artifacts / "section-b-saved.png"), full_page=True)
                    admin.close()
                finally:
                    browser.close()
        finally:
            stop_server(server)

        restarted, base2 = launch_server(binary, root)
        try:
            with sync_playwright() as pw:
                browser2 = pw.chromium.launch(channel="chrome", headless=True)
                try:
                    ctx2 = browser2.new_context()
                    admin_login(ctx2, base2, "site_admin", "AdminPassword123!")
                    assert api_json(ctx2, "GET", base2, f"/api/admin/sections/{a}/document")["document"] == media_design
                    assert api_json(ctx2, "GET", base2, f"/api/admin/sections/{b}/document")["document"] == doc_b["document"]
                    assert ctx2.request.get(base2 + "/content/section/section-b").status == 404
                    ctx2.close()
                finally:
                    browser2.close()
        finally:
            stop_server(restarted)
        checks.append("native_server_restart_preserves_A_B_private_Section_AST")
    return {"status":"PASS","source_sha":source,"binary_sha256":sha(binary),
            "platform":"macOS Intel native","checks":checks,"chrome_js_errors":errors}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", type=pathlib.Path, required=True)
    ap.add_argument("--artifact-dir", type=pathlib.Path, required=True)
    ap.add_argument("--baseline-binary", type=pathlib.Path, default=None,
                    help="Pre-0037 binary for collision + forward migration tests")
    args = ap.parse_args()
    args.artifact_dir.mkdir(parents=True, exist_ok=True)
    try:
        result = run(args.binary.resolve(), args.artifact_dir.resolve())
        if args.baseline_binary:
            result["checks"].extend(check_upgrade_and_collision(
                args.binary.resolve(), args.baseline_binary.resolve(), args.artifact_dir.resolve()))
        else:
            result["upgrade_collision_status"] = "NOT_RUN"
        result["checks"].extend(check_encrypted_backup_restore(
            args.binary.resolve(), args.artifact_dir.resolve()))
    except Exception as exc:
        result = {"status":"FAIL", "binary_sha256":sha(args.binary.resolve()),
                  "error":str(exc), "traceback":traceback.format_exc()}
    (args.artifact_dir / "receipt.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result,ensure_ascii=False,indent=2),flush=True)
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
