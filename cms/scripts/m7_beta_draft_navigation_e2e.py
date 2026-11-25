#!/usr/bin/env python3
"""M7-S15-T02 F03: native Mac Chrome Admin dirty navigation and safe draft restore.

Uses a disposable SQLite root and the real embedded Admin bundle (no mocks,
localStorage, production accounts, or long-lived browser profile).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import sqlite3
import subprocess
import sys
import tempfile
import time

from playwright.sync_api import expect, sync_playwright

from m7_beta_menu_picker_e2e import admin_login, api_json, launch_server, stop_server


def assert_draft_store_reauth_guards() -> None:
    """Exercise the actual TS RAM store with narrowed effective field ACLs."""
    js = r'''
import assert from 'node:assert/strict';
import { EditorDraftStore } from './cms/admin/src/features/editor/EditorDraftStore.ts';
const store = new EditorDraftStore();
const key = store.makeDraftKey('portfolio', 18);
const saved = {
  site_instance_id: 'site-a', user_id: 7, auth_generation: 1,
  draft_key: key, type_key: 'portfolio', status: 'quarantined',
  schema_token_at_edit: 'old-token', dirty_fields: ['client_name', 'restricted_value', 'custom_path', 'seo'],
  server_values: { client_name: 'Server', restricted_value: 'Server Restricted' },
  payload: {
    id: 18, version: 4, title: 'Title', slug: 'draft-slug',
    publication_status: 'draft', values: { client_name: 'Unsaved Public', restricted_value: 'Unsaved Restricted' },
    cleared_fields: [], term_ids: [], custom_path: '/old-privileged', custom_path_dirty: true,
    seo: { title: 'Old Privileged SEO' }, seo_dirty: true,
  },
};
const current = {
  id: 18, version: 5, title: 'Title', slug: 'draft-slug', publication_status: 'draft',
  values: { client_name: 'Server' }, cleared_fields: [], term_ids: [],
  custom_path: '', seo: { title: null, description: null, noindex: false },
};
const limited = { type_key: 'portfolio', schema_token: 'fresh-role-token',
  effective_fields: { client_name: { editable: true } } };
store.set(saved);
const restored = store.restoreAuthorizedDraft(key, 'site-a', 7, limited, 2, current, false, false);
assert.ok(restored);
assert.equal(restored.payload.values.client_name, 'Unsaved Public');
assert.equal('restricted_value' in restored.payload.values, false);
assert.equal(restored.payload.custom_path, '');
assert.equal(restored.payload.seo.title, null);
assert.equal(restored.payload.version, 4); // stale CAS cannot silently rebase
assert.equal(restored.schema_token_at_edit, 'old-token');
assert.ok(!restored.dirty_fields.includes('restricted_value'));
store.set(saved);
assert.equal(store.restoreAuthorizedDraft(key, 'other-site', 7, limited, 3, current, true, true), undefined);
assert.equal(store.get(key), undefined);
store.set(saved);
assert.equal(store.restoreAuthorizedDraft(key, 'site-a', 8, limited, 3, current, true, true), undefined);
assert.equal(store.get(key), undefined);
console.log('DRAFT_STORE_ROLE_SCRUB_AND_PROVENANCE_PASS');
'''
    node = pathlib.Path.home() / '.nvm/versions/node/v24.22.2/bin/node'
    if not node.is_file():
        raise RuntimeError(f'Node >=22 for TypeScript store test not found: {node}')
    result = subprocess.run([str(node), '--experimental-strip-types', '--input-type=module', '-e', js],
                            cwd=pathlib.Path(__file__).resolve().parents[2],
                            env={**os.environ, 'NODE_NO_WARNINGS':'1'},
                            capture_output=True, text=True, timeout=15)
    assert result.returncode == 0, result.stderr or result.stdout
    assert 'DRAFT_STORE_ROLE_SCRUB_AND_PROVENANCE_PASS' in result.stdout, result.stdout


def dialog_then(page, accepted: bool, action, kind: str = "confirm") -> str:
    seen = []
    def handle(dialog):
        assert dialog.type == kind, (dialog.type, dialog.message)
        seen.append(dialog.message)
        dialog.accept() if accepted else dialog.dismiss()
    page.once("dialog", handle)
    action()
    if not seen:
        # history.forward/hashchange dispatches asynchronously after the
        # JavaScript call returns, unlike direct sidebar click handlers.
        page.wait_for_timeout(250)
    assert len(seen) == 1, ("Expected one confirm", seen)
    if kind == "confirm":
        assert "chưa lưu" in seen[0].lower(), seen
    return seen[0]


def editor_hash(kind: str, entry_id: int) -> str:
    return f"#/cpt/{kind}/{entry_id}/edit" if kind not in ("post", "page") else f"#/{'posts' if kind=='post' else 'pages'}/{entry_id}/edit"


def add_entry(ctx, base, csrf, kind: str, slug: str) -> dict:
    return api_json(ctx, "POST", base, f"/api/admin/content-types/{kind}/entries",
                    {"title": f"Original {kind}", "slug": slug, "values": {}}, csrf, expected=201)


def run(binary: pathlib.Path, artifacts: pathlib.Path) -> int:
    artifacts.mkdir(parents=True, exist_ok=True)
    checks: list[str] = []
    errors: list[str] = []
    assert_draft_store_reauth_guards()
    checks.append('actual_TS_store_quarantine_same_user_role_scrub_site_account_guard')
    with tempfile.TemporaryDirectory(prefix="opendash-m7-s15-t02-") as temp:
        site = pathlib.Path(temp).resolve() / "site"
        site.mkdir()
        proc, base = launch_server(binary, site)
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel="chrome", headless=True)
                chrome_version = browser.version
                context = browser.new_context(viewport={"width": 1440, "height": 900})
                api_json(context, "POST", base, "/auth/setup",
                         {"username": "admin", "password": "F03-Safe-Draft-Admin123!"}, expected=201)
                csrf = admin_login(context, base, "admin", "F03-Safe-Draft-Admin123!", cms_locale="vi")
                api_json(context, "POST", base, "/api/admin/users",
                         {"username": "editor1", "password": "F03-Safe-Draft-Editor123!", "role": "editor"},
                         csrf, expected=201)
                api_json(context, "POST", base, "/api/admin/content-types", {
                    "key": "portfolio", "label": "Portfolio",
                    "fields": [
                        {"key": "client_name", "label": "Client", "type": "short_text", "required": False},
                        {"key": "private_cost", "label": "Private Cost", "type": "number", "required": False,
                         "min_view_role": "admin", "min_edit_role": "admin", "public_exposure": "private"},
                    ],
                }, csrf, expected=201)
                category = api_json(context,"POST",base,"/api/admin/taxonomy/terms",
                                    {"kind":"category","name":"F03 Keep Category","slug":"f03-keep-category"},
                                    csrf,expected=201)
                records = {k: add_entry(context, base, csrf, k, f"f03-{k}-original")
                           for k in ("post", "page", "portfolio")}

                page = context.new_page()
                page.on("pageerror", lambda err: errors.append(str(err)))
                page.goto(base + "/admin#/posts", wait_until="domcontentloaded")
                expect(page.get_by_test_id("entry-list-post")).to_be_visible(timeout=20000)

                # Existing Post/Page/CPT: cancel leaves route+form untouched;
                # accept leaves without writes, re-enter restores all dirty state.
                for kind in ("post", "page", "portfolio"):
                    rec = records[kind]
                    target = editor_hash(kind, int(rec["id"]))
                    page.evaluate("hash => { window.location.hash = hash; }", target)
                    expect(page.locator("#entry-title-input")).to_have_value(f"Original {kind}", timeout=20000)
                    unique = f"Unsaved F03 {kind}"
                    page.locator("#entry-title-input").fill(unique)
                    page.locator("#entry-slug-input").fill(f"f03-edit-{kind}")
                    assert page.get_by_test_id("single-edit-5-regions").is_visible()
                    current_hash = page.evaluate("location.hash")
                    assert current_hash == target, current_hash
                    dialog_then(page, False, lambda: page.locator('[data-menu-id="dashboard"]').click())
                    assert page.evaluate("location.hash") == target
                    assert page.locator("#entry-title-input").input_value() == unique
                    dialog_then(page, True, lambda: page.locator('[data-menu-id="dashboard"]').click())
                    expect(page).to_have_url(base + "/admin#/dashboard")
                    # Native history.Back to previously visited editor; no prompt
                    # from clean dashboard; fetch config+record before draft restore.
                    page.go_back(wait_until="domcontentloaded")
                    expect(page.locator("#entry-title-input")).to_have_value(unique, timeout=20000)
                    assert page.locator("#entry-slug-input").input_value() == f"f03-edit-{kind}"
                    if kind == "post":
                        # Repeated Forward cancellation must not trap or corrupt
                        # history: a later accepted Forward is still possible.
                        dialog_then(page,False,lambda:page.evaluate("history.forward()"))
                        expect(page).to_have_url(base+'/admin'+target,timeout=5000)
                        dialog_then(page,True,lambda:page.evaluate("history.forward()"))
                        expect(page).to_have_url(base+"/admin#/dashboard")
                        page.go_back(wait_until="domcontentloaded")
                        expect(page.locator('#entry-title-input')).to_have_value(unique,timeout=20000)
                    assert api_json(context, "GET", base,
                                    f"/api/admin/content-types/{kind}/entries/{rec['id']}")["title"] == f"Original {kind}"
                    # External hashchange is also guarded; cancel must not trap history.
                    dialog_then(page, False, lambda: page.evaluate("window.location.hash = '#/dashboard'"))
                    expect(page.locator("#entry-title-input")).to_have_value(unique)
                    assert page.evaluate("location.hash") == target, page.evaluate("location.hash")
                    dialog_then(page, True, lambda: page.locator('[data-menu-id="dashboard"]').click())
                checks.append("post_page_cpt_sidebar_cancel_leave_back_restore_no_autosave")
                checks.append("external_hashchange_cancel_restores_history")

                # Dirty metadata alone (status, route, SEO) must be guarded,
                # rehydrate, then Save clears only the submitted baseline.
                rec = add_entry(context, base, csrf, "post", "f03-metadata-only")
                page.evaluate("hash => location.hash = hash", editor_hash("post", int(rec["id"])))
                expect(page.locator("#entry-title-input")).to_have_value("Original post", timeout=20000)
                page.get_by_label("Đường dẫn tùy chỉnh (custom_path)").fill("/f03-custom-route")
                page.get_by_label("SEO Title").fill("F03 Custom SEO")
                page.get_by_label("Trạng thái xuất bản").select_option("published")
                cat = page.get_by_test_id('single-edit-terms').get_by_label('F03 Keep Category (f03-keep-category)')
                expect(cat).to_be_visible(timeout=15000)
                cat.check()
                dialog_then(page, True, lambda: page.locator('[data-menu-id="dashboard"]').click())
                page.go_back(wait_until="domcontentloaded")
                expect(page.get_by_label("SEO Title")).to_have_value("F03 Custom SEO", timeout=20000)
                assert page.get_by_label("Đường dẫn tùy chỉnh (custom_path)").input_value() == "/f03-custom-route"
                assert page.get_by_label("Trạng thái xuất bản").input_value() == "published"
                assert page.get_by_test_id('single-edit-terms').get_by_label('F03 Keep Category (f03-keep-category)').is_checked()
                assert api_json(context,"GET",base,f"/api/admin/content-types/post/entries/{rec['id']}")["publication_status"]=="draft"
                assert api_json(context,"GET",base,f"/api/admin/content-types/post/entries/{rec['id']}")["term_ids"] == []
                checks.append("metadata_only_route_seo_publication_terms_draft_no_public_write")

                # A stale server version after another session saves must not
                # wipe the authorized draft; re-entry keeps exact fields.
                other = browser.new_context()
                other_csrf = admin_login(other,base,"admin","F03-Safe-Draft-Admin123!")
                conflicting = api_json(other,"POST",base,f"/api/admin/content-types/post/entries/{rec['id']}/publication",
                                      {"status":"published"},other_csrf,{"If-Match":f'"v{rec["version"]}"'})
                assert conflicting["version"] > rec["version"]
                page.get_by_role("button",name="Lưu thay đổi").click()
                expect(page.get_by_role("alert")).to_contain_text("412", timeout=20000)
                assert page.get_by_label("SEO Title").input_value()=="F03 Custom SEO"
                dialog_then(page,True,lambda: page.locator('[data-menu-id="dashboard"]').click())
                page.go_back(wait_until="domcontentloaded")
                expect(page.get_by_label("SEO Title")).to_have_value("F03 Custom SEO",timeout=20000)
                checks.append("stale_CAS_412_preserves_active_metadata_draft")
                other.close()

                # Successful Save establishes a clean baseline (only saved
                # snapshot removed) and the next navigation must not prompt.
                clean_rec=add_entry(context,base,csrf,"page","f03-clean-baseline")
                dialog_then(page,True,lambda:page.locator('[data-menu-id="dashboard"]').click())
                page.evaluate('hash => location.hash=hash',editor_hash('page',int(clean_rec['id'])))
                expect(page.locator('#entry-title-input')).to_have_value('Original page',timeout=20000)
                page.locator('#entry-title-input').fill('F03 Saved Page Title')
                page.get_by_role('button',name='Lưu thay đổi').click()
                expect(page.get_by_role('status').filter(has_text='Đã lưu thành công')).to_be_visible(timeout=20000)
                page.locator('[data-menu-id="dashboard"]').click()
                expect(page).to_have_url(base+'/admin#/dashboard')
                assert api_json(context,'GET',base,f'/api/admin/content-types/page/entries/{clean_rec["id"]}')['title']=='F03 Saved Page Title'
                checks.append('successful_save_baseline_clears_only_submitted_draft')

                # Two independent new-form identities survive back/forward.
                page.locator('[data-menu-id="posts"]').click()
                expect(page.get_by_test_id("entry-list-post")).to_be_visible()
                page.get_by_role("button",name="+ Thêm mới").click()
                first_new = page.evaluate("location.hash")
                assert first_new.startswith("#/posts/new?draft="), first_new
                page.locator("#entry-title-input").fill("New Draft One")
                dialog_then(page,True,lambda: page.locator('[data-menu-id="posts"]').click())
                page.get_by_role("button",name="+ Thêm mới").click()
                second_new = page.evaluate("location.hash")
                assert second_new != first_new and second_new.startswith("#/posts/new?draft="), (first_new,second_new)
                page.locator("#entry-title-input").fill("New Draft Two")
                dialog_then(page,True,lambda: page.locator('[data-menu-id="posts"]').click())
                page.go_back(wait_until="domcontentloaded")
                expect(page.locator("#entry-title-input")).to_have_value("New Draft Two",timeout=20000)
                dialog_then(page,True,lambda: page.locator('[data-menu-id="posts"]').click())
                page.evaluate("hash => location.hash = hash", first_new)
                expect(page.locator("#entry-title-input")).to_have_value("New Draft One",timeout=20000)
                assert not any(r['slug'].startswith('new-draft') for r in api_json(context,'GET',base,'/api/admin/content-types/post/entries?limit=50&offset=0'))
                checks.append("two_new_draft_UUIDs_independent_memory_only")

                # Hard reload is protected at the browser-level. Dismiss it and
                # verify active form remains; confirmed reload has no RAM draft.
                dialog_then(page,False,lambda:page.evaluate('location.reload()'),kind='beforeunload')
                expect(page.locator('#entry-title-input')).to_have_value('New Draft One',timeout=20000)
                dialog_then(page,True,lambda:page.evaluate('location.reload()'),kind='beforeunload')
                expect(page.locator('#entry-title-input')).to_have_value('',timeout=20000)
                checks.append('native_beforeunload_warning_cancel_reload_discards_RAM_only')

                # A fresh second account cannot read previously authenticated
                # in-memory drafts when switching sessions in the same tab.
                page.locator('#entry-title-input').fill('Admin Secret Not Saved')
                dialog_then(page,True,lambda: page.get_by_role('button',name='Đăng xuất').click())
                expect(page.get_by_text('Đăng nhập quản trị')).to_be_visible(timeout=20000)
                page.get_by_placeholder('Tên đăng nhập').fill('editor1')
                page.get_by_placeholder('Mật khẩu').fill('F03-Safe-Draft-Editor123!')
                page.get_by_role('button',name='Đăng nhập',exact=True).click()
                expect(page.get_by_test_id('admin-layout')).to_be_visible()
                page.evaluate('hash => location.hash=hash', first_new)
                expect(page.locator('#entry-title-input')).to_have_value('',timeout=20000)
                checks.append('logout_other_account_clears_draft')

                # 390px Editor visible without hidden data or horizontal scroll.
                page.set_viewport_size({"width":390,"height":844})
                page.evaluate("hash => location.hash = hash", editor_hash("post",int(rec["id"])))
                expect(page.locator('#entry-title-input')).to_have_value('Original post',timeout=20000)
                assert page.evaluate('document.documentElement.scrollWidth <= document.documentElement.clientWidth + 1')
                checks.append('mobile_390px_editor_and_publication_boundary')

                # An API response arriving after leaving an editor may not
                # resurrect its DOM or bleed fields into the current route.
                page.set_viewport_size({'width':1440,'height':900})
                def delayed_config(route):
                    time.sleep(0.45)
                    route.continue_()
                page.route('**/api/admin/content-types/portfolio/effective-editor-config',delayed_config)
                page.evaluate('hash => location.hash=hash',editor_hash('portfolio',int(records['portfolio']['id'])))
                page.locator('[data-menu-id="dashboard"]').click()
                expect(page).to_have_url(base+'/admin#/dashboard')
                page.wait_for_timeout(800)
                assert page.locator('#entry-title-input').count()==0
                assert page.locator('[data-field-key="private_cost"]').count()==0
                page.unroute('**/api/admin/content-types/portfolio/effective-editor-config',delayed_config)
                checks.append('delayed_config_response_does_not_rehydrate_stale_editor')

                assert not errors, errors
                context.close();browser.close()
        finally:
            stop_server(proc)
        db = sqlite3.connect(site/'database/opendash.sqlite')
        try:
            assert db.execute('PRAGMA integrity_check').fetchone()==('ok',)
            assert db.execute('PRAGMA foreign_key_check').fetchall()==[]
        finally:db.close()
        checks.append('sqlite_integrity_foreign_keys')

    result = {
        'result':'PASS','task':'M7-S15-T02',
        'source_sha':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),
        'chrome':chrome_version,'checks':checks,'page_errors':errors,
        'site':'isolated temporary localhost SQLite; no live data',
    }
    (artifacts/'m7_beta_draft_navigation_receipt.json').write_text(json.dumps(result,indent=2,ensure_ascii=False),encoding='utf8')
    print(json.dumps(result,ensure_ascii=False),flush=True)
    return 0


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',required=True,type=pathlib.Path)
    parser.add_argument('--artifact-dir',required=True,type=pathlib.Path)
    args=parser.parse_args()
    try:sys.exit(run(args.binary.resolve(strict=True),args.artifact_dir.resolve()))
    except Exception as error:
        print(f'M7-S15-T02 FAIL: {type(error).__name__}: {error}',file=sys.stderr,flush=True)
        raise
