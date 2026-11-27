#!/usr/bin/env python3
"""M4-S11-T01: native macOS Chrome acceptance on one disposable site.

Never connects to the owner's 18964 process/site. Usage:
  python3 cms/scripts/page_builder_scope_e2e.py \
    --binary /path/to/pinned/opendash --artifact-dir /path/to/private/results
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import pathlib
import re
import sqlite3
import subprocess
import tempfile
import traceback

from playwright.sync_api import expect, sync_playwright
from menu_picker_e2e import api_json, admin_login, launch_server, stop_server
from media_smoke import make_png


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def page_scope(page_id: int) -> str:
    return f"/api/admin/entries/page/{page_id}/builder"


def editor_page(base: str, page_id: int) -> str:
    return f"{base}/admin/#/pages/{page_id}/edit"


def ui_create_page(page, base: str, title: str, slug: str, body: str = "") -> int:
    page.goto(base + "/admin/#/pages/new", wait_until="domcontentloaded")
    page.locator("#entry-title-input").fill(title)
    slug_input = page.locator("#entry-slug-input")
    if slug_input.count() == 0 or not slug_input.is_visible():
        page.locator('div:has(> span:text-is("Đường dẫn:"))').get_by_role("button", name="Sửa").click()
    slug_input.fill(slug)
    if body:
        page.locator("#content-field-body").fill(body)
    page.get_by_role("button", name="Lưu nháp").first.click()
    expect(page).to_have_url(re.compile(r"#/pages/[1-9]\d*/edit$"), timeout=20000)
    return int(re.search(r"#/pages/(\d+)/edit", page.url).group(1))


def ensure_tree_view(page) -> None:
    for _ in range(4):
        back = page.get_by_test_id("studio-inline-back-btn")
        if back.count() == 0:
            break
        back.click()


def new_heading(page, value: str) -> None:
    ensure_tree_view(page)
    page.get_by_role("button", name="Thêm Block").first.click()
    page.get_by_role("button", name="Heading H1–H6").click()
    page.get_by_label("Tiêu đề", exact=True).fill(value)


def change_heading(page, value: str) -> None:
    ensure_tree_view(page)
    for _ in range(3):
        collapsed = page.locator('[role="treeitem"][aria-expanded="false"]')
        if collapsed.count() == 0:
            break
        collapsed.first.focus()
        collapsed.first.press("ArrowRight")
    page.locator('[role="treeitem"]').last.dblclick()
    page.get_by_label("Tiêu đề", exact=True).fill(value)

def single_heading(text: str) -> dict:
    return {"schema_version":1,"sections":[{"id":"section","children":[
        {"id":"row","children":[{"id":"column","children":[
            {"id":"heading","type":"core/heading","props":{"level":2,"text":text}}
        ]}]}]}]}


def test_page_scope(binary: pathlib.Path, artifacts: pathlib.Path) -> dict:
    checks: list[str] = []
    js_errors: list[str] = []
    source = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    with tempfile.TemporaryDirectory(prefix="opendash-m4-s11-t01-", ignore_cleanup_errors=True) as tmp:
        site = pathlib.Path(tmp).resolve() / "site"
        site.mkdir(mode=0o700)
        server, base = launch_server(binary, site)
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel="chrome", headless=True)
                try:
                    admin = browser.new_context(viewport={"width":1440, "height":900})
                    page = admin.new_page()
                    page.on("pageerror", lambda e: js_errors.append(str(e)))
                    api_json(admin, "POST", base, "/auth/setup",
                             {"username":"site_admin","password":"AdminPassword123!"},expected=201)
                    csrf = admin_login(admin, base, "site_admin", "AdminPassword123!", cms_locale="vi")
                    # Existing site header and footer must be composed once;
                    # Page-specific Save cannot mutate shared ASTs or site
                    # Reading Settings (the supported homepage configuration).
                    reading_initial = api_json(admin,"GET",base,"/api/admin/settings/reading")
                    shared = {}
                    for role, title in [("header","SITE HEADER EXACTLY ONCE"),
                                        ("footer","SITE FOOTER EXACTLY ONCE")]:
                        path = "/api/admin/template-overrides"
                        assert not any(row["canonical_target_key"] == f"part:{role}"
                                       for row in api_json(admin,"GET",base,path)["items"])
                        api_json(admin,"PUT",base,path,
                                 {"target":{"kind":"part","part_key":role},"document":single_heading(title)},
                                 csrf,expected=201)
                        shared[role] = next(row for row in api_json(admin,"GET",base,path)["items"]
                                            if row["canonical_target_key"] == f"part:{role}")
                    a = ui_create_page(page, base, "Page A from UI", "scope-page-a",
                                       "ORIGINAL PAGE A BODY FOR FALLBACK")
                    b = ui_create_page(page, base, "Page B from UI", "scope-page-b")
                    assert a != b
                    a_initial = api_json(admin,"GET",base,page_scope(a))
                    b_initial = api_json(admin,"GET",base,page_scope(b))
                    assert a_initial["content_mode"] == b_initial["content_mode"] == "editor"
                    assert a_initial["had_freeform_wrap"] is True and b_initial["document"] is None
                    checks.append("two_independent_draft_pages_created_in_real_Admin")
                    api_json(admin, "POST", base, "/api/admin/users",
                             {"username":"page_editor","password":"EditorPassword123!","role":"editor"},
                             csrf,expected=201)

                    # Open actual Page editor, then its Studio Mode 2 builder button.
                    reading_calls = []
                    def record_request(req):
                        if "/api/admin/settings/reading" in req.url:
                            reading_calls.append(req.url)
                    page.on("request",record_request)
                    page.goto(editor_page(base, a),wait_until="domcontentloaded")
                    page.get_by_test_id("open-ux-builder-toolbar").click()
                    expect(page).to_have_url(
                        base + f"/admin/#/studio/entry/page/{a}",timeout=20000)
                    expect(page.get_by_test_id("studio-workspace")).to_be_visible(timeout=20000)
                    expect(page.get_by_test_id("studio-mode2-sidebar")).to_be_visible(timeout=20000)
                    expect(page.get_by_test_id("studio-save-draft-btn")).to_be_visible(timeout=20000)
                    assert reading_calls == [],reading_calls
                    assert api_json(admin,"GET",base,page_scope(a))["entry_id"] == a
                    assert api_json(admin,"GET",base,page_scope(b))["document"] is None
                    checks.append("Page_editor_to_exact_scope_no_homepage_requests")
                    page.screenshot(path=str(artifacts/"before-page-a.png"),full_page=True)

                    new_heading(page,"PAGE A DESIGN PRIVATE")
                    expect(page.get_by_test_id("studio-dirty-badge")).to_be_visible(timeout=15000)
                    expect(page.frame_locator("#canvas").locator("body")).to_contain_text(
                        "PAGE A DESIGN PRIVATE",timeout=20000)
                    assert api_json(admin,"GET",base,page_scope(a))["content_mode"] == "editor"
                    page.get_by_test_id("studio-save-draft-btn").click()
                    expect(page.get_by_test_id("studio-dirty-badge")).to_have_count(0,timeout=20000)
                    a_doc = api_json(admin,"GET",base,page_scope(a))
                    assert a_doc["version"] == a_initial["version"] + 1 and a_doc["content_mode"] == "builder" and "PAGE A DESIGN PRIVATE" in json.dumps(a_doc)
                    assert api_json(admin,"GET",base,page_scope(b))["document"] is None
                    guest = browser.new_context()
                    assert guest.request.get(base + "/scope-page-a").status == 404
                    uploaded = admin.request.post(base+"/api/admin/media",
                        data=make_png(seed=54),
                        headers={"Origin":base,"X-CSRF-Token":csrf,
                                 "Content-Type":"image/png","X-File-Name":"draft-page.png"})
                    assert uploaded.status == 201,uploaded.text()[:300]
                    media_id = uploaded.json()["id"]
                    public_media = base+f"/media/{media_id}/file"
                    assert guest.request.get(public_media).status == 404
                    draft_media = copy.deepcopy(a_doc["document"])
                    draft_media["sections"][0]["children"][0]["children"][0]["children"].append({
                        "id":"private-draft-image","type":"core/image",
                        "props":{"media_id":media_id,"alt":"Page A private image"}
                    })
                    preview_media = admin.request.post(base+
                        page_scope(a) + "/preview",
                        data=json.dumps(draft_media),
                        headers={"Origin":base,"Content-Type":"application/json","X-CSRF-Token":csrf})
                    assert preview_media.status == 200,preview_media.text()[:250]
                    assert preview_media.headers.get("cache-control") == "no-store"
                    assert f"/api/admin/media/{media_id}/file" in preview_media.text()
                    assert guest.request.get(public_media).status == 404
                    api_json(admin,"PUT",base,page_scope(a),draft_media,csrf,{"If-Match":f'"v{a_doc["version"]}"'})
                    assert guest.request.get(public_media).status == 404
                    a_doc = api_json(admin,"GET",base,page_scope(a))
                    assert a_doc["version"] == a_initial["version"] + 2 and a_doc["document"] == draft_media
                    guest.close()
                    checks.append("preview_unsaved_then_save_A_only_draft_never_public")
                    checks.append("draft_Page_image_private_after_preview_and_saved_AST")
                    page.screenshot(path=str(artifacts/"after-page-a-saved.png"),full_page=True)

                    # Dirty target A -> B must cancel both URL and local AST;
                    # the next accepted transition must open clean Page B.
                    change_heading(page,"PAGE A UNSAVED MUST STAY")
                    expect(page.get_by_test_id("studio-dirty-badge")).to_be_visible()
                    dialogs: list[str] = []
                    def decline(dialog):
                        dialogs.append(dialog.message)
                        dialog.dismiss()
                    page.on("dialog",decline)
                    page.evaluate(f"location.hash = '#/studio/entry/page/{b}'")
                    expect(page).to_have_url(re.compile(rf"entry/page/{a}$"),timeout=10000)
                    expect(page.get_by_label("Tiêu đề", exact=True)).to_have_value("PAGE A UNSAVED MUST STAY")
                    assert dialogs,dialogs
                    page.remove_listener("dialog",decline)
                    def accept_confirm(dialog):
                        dialog.accept()
                    page.on("dialog",accept_confirm)
                    page.evaluate(f"location.hash = '#/studio/entry/page/{b}'")
                    expect(page).to_have_url(re.compile(rf"entry/page/{b}$"),timeout=10000)
                    expect(page.get_by_test_id("studio-mode2-sidebar")).to_be_visible(timeout=15000)
                    expect(page.get_by_test_id("studio-dirty-badge")).to_have_count(0)
                    assert api_json(admin,"GET",base,page_scope(a))["document"] == a_doc["document"]
                    assert api_json(admin,"GET",base,"/api/admin/settings/reading") == reading_initial
                    for role in shared:
                        assert next(row for row in api_json(admin,"GET",base,"/api/admin/template-overrides")["items"] if row["canonical_target_key"] == f"part:{role}") == shared[role]
                    checks.append("dirty_target_cancel_retains_url_AST_discard_loads_B_only")

                    new_heading(page,"PAGE B PUBLISHED DESIGN")
                    page.get_by_test_id("studio-save-draft-btn").click()
                    expect(page.get_by_test_id("studio-dirty-badge")).to_have_count(0,timeout=15000)
                    b_doc = api_json(admin,"GET",base,page_scope(b))
                    assert b_doc["version"] == b_initial["version"] + 1 and b_doc["document"] != a_doc["document"]
                    assert api_json(admin,"GET",base,page_scope(a))["document"] == a_doc["document"]
                    b_entry = api_json(admin,"GET",base,
                                       f"/api/admin/content-types/page/entries/{b}")
                    api_json(admin,"POST",base,
                             f"/api/admin/content-types/page/entries/{b}/publication",
                             {"status":"published"},csrf,{"If-Match":f'"v{b_entry["version"]}"'})
                    b_doc = api_json(admin,"GET",base,page_scope(b))
                    assert b_doc["version"] == b_entry["version"] + 1
                    public_b = admin.request.get(base + "/scope-page-b")
                    assert public_b.status == 200
                    assert "PAGE B PUBLISHED DESIGN" in public_b.text()
                    assert "PAGE A DESIGN PRIVATE" not in public_b.text()
                    assert public_b.text().count("SITE HEADER EXACTLY ONCE") == 1
                    assert public_b.text().count("SITE FOOTER EXACTLY ONCE") == 1
                    checks.append("Page_B_published_exact_canonical_public_no_A_or_homepage_contamination")

                    # Force 401 while the Admin has a modified, unsaved AST.
                    # Same-account login must recover that AST without exposing
                    # it to the anonymous login view.
                    page.reload(wait_until="domcontentloaded")
                    expect(page.get_by_test_id("studio-mode2-sidebar")).to_be_visible(timeout=15000)
                    change_heading(page,"PAGE B SAME ACCOUNT RECOVERY")
                    expect(page.get_by_test_id("studio-dirty-badge")).to_be_visible(timeout=15000)
                    expect(page.frame_locator("#canvas").locator("body")).to_contain_text(
                        "PAGE B SAME ACCOUNT RECOVERY",timeout=15000)
                    api_json(admin,"POST",base,"/auth/logout",csrf=csrf,expected=204)
                    page.get_by_test_id("studio-save-btn").click()
                    expect(page.locator("#auth-username")).to_be_visible(timeout=15000)
                    expect(page.get_by_test_id("studio-workspace")).not_to_be_visible()
                    page.locator("#auth-username").fill("site_admin")
                    page.locator("#auth-password").fill("AdminPassword123!")
                    page.locator("#auth-login").click()
                    expect(page.get_by_test_id("studio-mode2-sidebar")).to_be_visible(timeout=20000)
                    expect(page.get_by_test_id("studio-dirty-badge")).to_be_visible(timeout=15000)
                    expect(page.frame_locator("#canvas").locator("body")).to_contain_text(
                        "PAGE B SAME ACCOUNT RECOVERY",timeout=15000)
                    csrf = api_json(admin,"GET",base,"/auth/me")["csrf_token"]
                    checks.append("401_quarantines_preview_same_account_relogin_recovers_dirty_AST")

                    # Another authenticated session commits while Chrome is
                    # dirty. 412 must keep the local AST while protecting the
                    # other writer's persisted state and public rendering.
                    change_heading(page,"PAGE B LOCAL STALE 412")
                    remote = copy.deepcopy(b_doc["document"])
                    remote["sections"][0]["children"][0]["children"][0]["children"][0]["props"]["text"] = "PAGE B REMOTE WINS"
                    api_json(admin,"PUT",base,page_scope(b),remote,csrf,
                             {"If-Match":f'"v{b_doc["version"]}"'})
                    previous_version = b_doc["version"]
                    b_doc = api_json(admin,"GET",base,page_scope(b))
                    assert b_doc["version"] == previous_version + 1
                    page.get_by_test_id("studio-save-btn").click()
                    expect(page.get_by_test_id("studio-sidebar-alert")).to_contain_text("412",timeout=15000)
                    expect(page.get_by_label("Tiêu đề", exact=True)).to_have_value("PAGE B LOCAL STALE 412")
                    assert api_json(admin,"GET",base,page_scope(b))["document"] == remote
                    assert "PAGE B REMOTE WINS" in admin.request.get(
                        base+"/scope-page-b").text()
                    page.get_by_test_id("studio-sidebar-alert").get_by_role("button",name="Tải lại").click()
                    expect(page.get_by_test_id("studio-sidebar-alert")).to_have_count(0,timeout=15000)
                    expect(page.frame_locator("#canvas").locator("body")).to_contain_text(
                        "PAGE B REMOTE WINS",timeout=15000)
                    checks.append("412_keeps_local_AST_until_confirmed_reload_and_public_remote_wins")

                    # Direct deep link (including legacy #/builder redirect) reads
                    # persisted Page A, publishes it, then reset keeps fallback body.
                    page.goto(base + f"/admin/#/builder?template=page&type=page&entry={a}",
                              wait_until="domcontentloaded")
                    expect(page).to_have_url(base + f"/admin/#/studio/entry/page/{a}",timeout=15000)
                    expect(page.get_by_test_id("studio-mode2-sidebar")).to_be_visible(timeout=15000)
                    expect(page.frame_locator("#canvas").locator("body")).to_contain_text(
                        "PAGE A DESIGN PRIVATE",timeout=15000)
                    page.get_by_test_id("studio-publish-btn").click()
                    expect(page.get_by_test_id("studio-dirty-badge")).to_have_count(0,timeout=15000)
                    assert "PAGE A DESIGN PRIVATE" in admin.request.get(base+"/scope-page-a").text()
                    assert admin.request.get(public_media).status == 200
                    a_pub = api_json(admin,"GET",base,page_scope(a))
                    api_json(admin,"POST",base,page_scope(a)+"/reset",
                             data={"confirm":True},csrf=csrf,extra={"If-Match":f'"v{a_pub["version"]}"'})
                    assert api_json(admin,"GET",base,page_scope(b))["document"] == b_doc["document"]
                    a_entry = api_json(admin,"GET",base,
                        f"/api/admin/content-types/page/entries/{a}")
                    assert "ORIGINAL PAGE A BODY FOR FALLBACK" in a_entry["values"]["body"]
                    assert "<!-- od:" not in a_entry["values"]["body"]
                    assert a_entry["publication_status"] == "published"
                    assert a_entry["content_mode"] == "editor"
                    public_a = admin.request.get(base+"/scope-page-a")
                    assert public_a.status == 200
                    assert "ORIGINAL PAGE A BODY FOR FALLBACK" in public_a.text()
                    assert admin.request.get(base+f"/media/{media_id}/file").status == 404
                    assert public_a.text().count("SITE HEADER EXACTLY ONCE") == 1
                    assert public_a.text().count("SITE FOOTER EXACTLY ONCE") == 1
                    checks.append("direct_deep_link_reload_A_then_reset_only_A")
                    checks.append("body_fallback_and_header_footer_once_after_Page_A_reset")

                    # Editor is allowed Page scope but forbidden global Studio,
                    # Reading Settings, Pattern Studio, or entry-scoped other types.
                    editor = browser.new_context(viewport={"width":390,"height":844})
                    ec = admin_login(editor,base,"page_editor","EditorPassword123!")
                    epage = editor.new_page()
                    epage.goto(editor_page(base,b),wait_until="domcontentloaded")
                    epage.get_by_test_id("open-ux-builder-toolbar").click()
                    expect(epage).to_have_url(base+f"/admin/#/studio/entry/page/{b}",timeout=20000)
                    expect(epage.get_by_test_id("studio-mode2-sidebar")).to_be_visible(timeout=20000)
                    editor_scope = editor.request.get(base + page_scope(b))
                    assert editor_scope.status == 200
                    for path in ["/api/admin/settings/reading",
                                 "/api/admin/template-overrides",
                                 "/api/admin/studio/targets"]:
                        assert editor.request.get(base + path).status == 403,path
                    assert editor.request.post(
                        base + "/api/admin/patterns",
                        data=json.dumps({"title":"Editor Pattern","slug":"editor-pattern"}),
                        headers={"Origin":base,"Content-Type":"application/json","X-CSRF-Token":ec},
                    ).status == 403
                    for path, expected in [
                        ("/api/admin/entries/page/999999/builder",404),
                        (f"/api/admin/entries/post/{b}/builder",403),
                        (f"/api/admin/builder/overrides/page?type_key=page&entry_id={b}",404)]:
                        assert editor.request.get(base + path).status == expected,path
                    assert editor.request.post(
                        base + page_scope(b) + "/preview",
                        data=json.dumps(b_doc["document"]),
                        headers={"Origin":base,"Content-Type":"application/json"}).status == 403
                    assert editor.request.post(
                        base + page_scope(b) + "/preview",
                        data=json.dumps(b_doc["document"]),
                        headers={"Origin":"https://invalid.example","Content-Type":"application/json",
                                 "X-CSRF-Token":ec}).status == 403
                    epage.screenshot(path=str(artifacts/"editor-page-b-390.png"),full_page=True)
                    checks.append("Editor_390px_exact_Page_and_global_API_CSRF_Origin_guards")
                    editor.close()

                    # Same user is downgraded by another Admin. The old
                    # privileged AST cannot survive the new role even though
                    # the user ID remains unchanged.
                    extra_admin = api_json(admin,"POST",base,"/api/admin/users",
                        {"username":"downgrade_user","password":"DowngradePassword123!","role":"admin"},
                        csrf,expected=201)
                    downgraded = browser.new_context()
                    tcsrf = admin_login(downgraded,base,"downgrade_user","DowngradePassword123!",cms_locale="vi")
                    tpage = downgraded.new_page()
                    tpage.goto(base+f"/admin/#/studio/entry/page/{b}",
                               wait_until="domcontentloaded")
                    expect(tpage.get_by_test_id("studio-mode2-sidebar")).to_be_visible(timeout=15000)
                    change_heading(tpage,"SECRET PRIVILEGED DRAFT NEVER RECOVER")
                    expect(tpage.get_by_test_id("studio-dirty-badge")).to_be_visible()
                    api_json(admin,"PATCH",base,f"/api/admin/users/{extra_admin['id']}",
                        {"role":"editor"},csrf,
                        {"If-Match":f'"v{extra_admin["version"]}"'})
                    tpage.get_by_test_id("studio-save-btn").click()
                    expect(tpage.locator("#auth-username")).to_be_visible(timeout=15000)
                    tpage.locator("#auth-username").fill("downgrade_user")
                    tpage.locator("#auth-password").fill("DowngradePassword123!")
                    tpage.locator("#auth-login").click()
                    expect(tpage.get_by_test_id("studio-mode2-sidebar")).to_be_visible(timeout=15000)
                    expect(tpage.get_by_test_id("studio-dirty-badge")).to_have_count(0)
                    expect(tpage.frame_locator("#canvas").locator("body")).not_to_contain_text(
                        "SECRET PRIVILEGED DRAFT NEVER RECOVER")
                    checks.append("role_downgrade_revokes_session_discards_prior_admin_AST")

                    # A different account logs in through the same tab after
                    # 401. No draft, preview, or permission from user #1 leaks.
                    change_heading(tpage,"ACCOUNT ONE PRIVATE UNSAVED")
                    expect(tpage.get_by_test_id("studio-dirty-badge")).to_be_visible()
                    tcsrf = api_json(downgraded,"GET",base,"/auth/me")["csrf_token"]
                    api_json(downgraded,"POST",base,"/auth/logout",csrf=tcsrf,expected=204)
                    tpage.get_by_test_id("studio-save-btn").click()
                    expect(tpage.locator("#auth-username")).to_be_visible(timeout=15000)
                    tpage.locator("#auth-username").fill("page_editor")
                    tpage.locator("#auth-password").fill("EditorPassword123!")
                    tpage.locator("#auth-login").click()
                    expect(tpage.get_by_test_id("studio-mode2-sidebar")).to_be_visible(timeout=15000)
                    expect(tpage.get_by_test_id("studio-dirty-badge")).to_have_count(0)
                    expect(tpage.frame_locator("#canvas").locator("body")).not_to_contain_text(
                        "ACCOUNT ONE PRIVATE UNSAVED")
                    checks.append("account_switch_in_same_tab_clears_previous_account_AST")
                    downgraded.close()

                    with sqlite3.connect(site/"database/opendash.sqlite") as db:
                        assert db.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
                        assert db.execute("PRAGMA foreign_key_check").fetchall() == []
                        rows = db.execute(
                            "SELECT canonical_target_key FROM template_overrides ORDER BY id"
                        ).fetchall()
                        assert set(rows) == {("part:header",),("part:footer",)},rows
                        assert db.execute("SELECT entry_id FROM entry_builder_documents").fetchall() == []
                        assert db.execute("SELECT entry_id FROM entry_builder_media_refs").fetchall() == []
                    checks.append("SQLite_exact_scope_unified_comment_blocks_integrity")
                    assert not js_errors,js_errors
                    checks.append("Chrome_no_uncaught_page_errors")
                    admin.close()
                finally:
                    browser.close()
        finally:
            stop_server(server)
        # Startup from same immutable disposable database, with a new random port.
        restarted, base2 = launch_server(binary,site)
        try:
            with sync_playwright() as pw:
                browser2 = pw.chromium.launch(channel="chrome",headless=True)
                try:
                    ctx2 = browser2.new_context()
                    admin_login(ctx2,base2,"site_admin","AdminPassword123!")
                    assert api_json(ctx2,"GET",base2,page_scope(a))["content_mode"] == "editor"
                    assert api_json(ctx2,"GET",base2,page_scope(b))["document"] == b_doc["document"]
                    assert "PAGE B REMOTE WINS" in ctx2.request.get(
                        base2+"/scope-page-b").text()
                    ctx2.close()
                finally:
                    browser2.close()
        finally:
            stop_server(restarted)
        checks.append("native_server_restart_preserves_scoped_B_and_reset_A")
    return {"status":"PASS","source_sha":source,"binary_sha256":digest(binary),
            "platform":"macOS Intel native" if pathlib.Path("/Applications").exists() else "native",
            "checks":checks,"chrome_js_errors":js_errors}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary",type=pathlib.Path,required=True)
    ap.add_argument("--artifact-dir",type=pathlib.Path,required=True)
    args = ap.parse_args()
    args.artifact_dir.mkdir(parents=True,exist_ok=True)
    try:
        result = test_page_scope(args.binary.resolve(),args.artifact_dir.resolve())
    except Exception as exc:
        result = {"status":"FAIL","binary_sha256":digest(args.binary.resolve()),
                  "error":str(exc),"traceback":traceback.format_exc()}
    (args.artifact_dir/"receipt.json").write_text(
        json.dumps(result,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    print(json.dumps(result,ensure_ascii=False,indent=2),flush=True)
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
