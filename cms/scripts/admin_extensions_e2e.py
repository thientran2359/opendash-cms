#!/usr/bin/env python3
"""M6-S10-T01: isolated native browser+SQLite provider columns/actions/term panels.

No network access outside random loopback, never touches the owner's port 18964.
Arguments: --binary PINNED_BINARY --artifact-dir PRIVATE_ARTIFACT_DIR.
"""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import pathlib
import platform
import shutil
import sqlite3
import subprocess
import tempfile
import traceback
import zipfile

from playwright.sync_api import expect, sync_playwright
from menu_picker_e2e import api_json, admin_login, launch_server, stop_server


CMS = pathlib.Path(__file__).resolve().parents[1]
PLUGIN_JSON = CMS / 'fixtures/admin-review/plugin/plugin.json'
THEME_JSON = CMS / 'fixtures/admin-review/theme/theme.json'


def sha(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def uleb(value: int) -> bytes:
    result = bytearray()
    while True:
        digit = value & 127
        value >>= 7
        result.append(digit | (128 if value else 0))
        if not value:
            return bytes(result)


def name(value: str) -> bytes:
    raw = value.encode()
    return uleb(len(raw)) + raw


def section(index: int, body: bytes) -> bytes:
    return bytes([index]) + uleb(len(body)) + body


def deterministic_wasm() -> bytes:
    """Minimal legal Wasm, writes JSON {} through allowed host_write_output."""
    types = b'\x02\x60\x02\x7f\x7f\x01\x7f\x60\x00\x01\x7f'
    imports = b'\x01' + name('opendash_v1') + name('host_write_output') + b'\x00\x00'
    exports = (b'\x02' + name('memory') + b'\x02\x00' +
               name('opendash_run') + b'\x00\x01')
    body = b'\x00\x41\x00\x41\x02\x10\x00\x1a\x41\x00\x0b'
    data = b'\x01\x00\x41\x00\x0b\x02{}'
    return (b'\x00asm\x01\x00\x00\x00' + section(1, types) +
            section(2, imports) + section(3, b'\x01\x01') +
            section(5, b'\x01\x00\x01') + section(7, exports) +
            section(10, b'\x01' + uleb(len(body)) + body) + section(11, data))


def package_zip(manifest: dict, kind: str) -> bytes:
    output = io.BytesIO()
    with zipfile.ZipFile(output, mode='w', compression=zipfile.ZIP_DEFLATED) as arc:
        if kind == 'plugin':
            arc.writestr('manifest.json', json.dumps(manifest, ensure_ascii=False))
            arc.writestr('plugin.wasm', deterministic_wasm())
        else:
            arc.writestr('theme.json', json.dumps(manifest, ensure_ascii=False))
            arc.writestr('single.html', '<article><h1>{{ entry.title }}</h1>{{ od_content() }}</article>')
    return output.getvalue()


def upload(ctx, base: str, kind: str, manifest: dict, csrf: str, expected: int) -> dict:
    url = '/api/admin/plugins' if kind == 'plugin' else '/api/admin/themes'
    response = ctx.request.post(base + url,
        data=package_zip(manifest, kind),
        headers={'Origin': base, 'X-CSRF-Token': csrf, 'Content-Type': 'application/zip'},
        timeout=20000)
    assert response.status == expected, (url, response.status, response.text()[:400])
    return response.json() if response.body() and response.status < 400 else {}


def patch_term(ctx, base: str, term_id: int, version: int, csrf: str, changes: dict,
               expected: int = 200):
    response = ctx.request.fetch(base + f'/api/admin/taxonomy/terms/{term_id}',
        method='PATCH', data=json.dumps(changes),
        headers={'Origin':base, 'X-CSRF-Token':csrf,
                 'If-Match':f'"v{version}"', 'Content-Type':'application/json'},
        timeout=20000)
    assert response.status == expected, (expected,response.status,response.text()[:240])
    return response.json() if expected < 400 else {'error':response.text()}


def term_config(ctx, base: str, term_id: int):
    return api_json(ctx, 'GET', base, f'/api/admin/taxonomy/terms/{term_id}/effective-editor-config')


def list_config(ctx, base: str):
    return api_json(ctx, 'GET', base, '/api/admin/content-types/post/effective-list-config')


def check_sqlite(root: pathlib.Path, term_id: int, panel: str) -> dict:
    with sqlite3.connect(root / 'database/opendash.sqlite') as conn:
        row = conn.execute('SELECT values_json FROM term_extension_values WHERE term_id=? AND owner_kind=? AND owner_id=? AND local_key=?',
                           (term_id, *panel.split('/'))).fetchone()
        assert row, (panel, 'not persisted')
        fk = conn.execute('PRAGMA foreign_key_check').fetchall()
        assert not fk, fk
        return json.loads(row[0])


def verify_old_new_old(predecessor: pathlib.Path, candidate: pathlib.Path,
                       artifacts: pathlib.Path, checks: list[str]) -> None:
    """Only matched, pre-upgrade root is reopened by old binary.

    Never attempt an unsafe SQL downgrade on a DB that saw migration 0036.
    """
    with tempfile.TemporaryDirectory(prefix='m6-s10-t01-rollout-', dir=str(artifacts.parent)) as folder:
        stage = pathlib.Path(folder)
        original = stage / 'old-original'
        original.mkdir(mode=0o700)
        server, base = launch_server(predecessor, original)
        password = 'Rollback-Matched-Local-Password-123456!'
        with sync_playwright() as pw:
            browser = pw.chromium.launch(channel='chrome', headless=True)
            try:
                ctx = browser.new_context()
                try:
                    api_json(ctx,'POST',base,'/auth/setup',
                        {'username':'rollback_admin','password':password},expected=201)
                    csrf = admin_login(ctx,base,'rollback_admin',password)
                    term = api_json(ctx,'POST',base,'/api/admin/taxonomy/terms',
                        {'kind':'tag','name':'Old database term','slug':'old-database-term'},
                        csrf,expected=201)
                    entry = api_json(ctx,'POST',base,'/api/admin/content-types/post/entries',
                        {'title':'Old database Post','slug':'old-database-post','values':{}},
                        csrf,expected=201)
                    old_hash = sha(original/'database/opendash.sqlite')
                finally:
                    ctx.close()
                    stop_server(server)
                candidate_root = stage / 'new-clone'
                shutil.copytree(original,candidate_root)
                checks.append('old_binary_original_site_created_pre_migration_data')

                server, base = launch_server(candidate,candidate_root)
                upgraded = browser.new_context()
                try:
                    admin_login(upgraded,base,'rollback_admin',password)
                    term_new = api_json(upgraded,'GET',base,
                        f'/api/admin/taxonomy/terms/{term["id"]}')
                    post_new = api_json(upgraded,'GET',base,
                        f'/api/admin/content-types/post/entries/{entry["id"]}')
                    assert term_new['name']=='Old database term'
                    assert post_new['title']=='Old database Post'
                    assert list_config(upgraded,base)['schema_version']==1
                    with sqlite3.connect(candidate_root/'database/opendash.sqlite') as conn:
                        assert conn.execute('PRAGMA integrity_check').fetchone()[0]=='ok'
                        assert conn.execute('PRAGMA foreign_key_check').fetchall()==[]
                        assert conn.execute("SELECT COUNT(*) FROM sqlite_master WHERE name='term_extension_values'").fetchone()[0]==1
                    checks.append('new_binary_forward_migration_0036_preserves_old_term_post_and_SQL_integrity')
                finally:
                    upgraded.close()
                    stop_server(server)

                # The rollback binary is paired with its untouched original
                # pre-upgrade root, never the upgraded clone.
                server, base = launch_server(predecessor,original)
                rolled_back = browser.new_context()
                try:
                    admin_login(rolled_back,base,'rollback_admin',password)
                    assert api_json(rolled_back,'GET',base,
                        f'/api/admin/content-types/post/entries/{entry["id"]}')['title']=='Old database Post'
                    assert api_json(rolled_back,'GET',base,
                        f'/api/admin/taxonomy/terms/{term["id"]}')['name']=='Old database term'
                    with sqlite3.connect(original/'database/opendash.sqlite') as conn:
                        assert conn.execute('PRAGMA integrity_check').fetchone()[0]=='ok'
                    checks.append('rollback_old_binary_on_matched_preupgrade_snapshot_never_downgrades_new_DB')
                finally:
                    rolled_back.close()
                    stop_server(server)
            finally:
                browser.close()

def run(binary: pathlib.Path, artifacts: pathlib.Path, checks: list[str]) -> dict:
    js_errors: list[str] = []
    with tempfile.TemporaryDirectory(prefix='m6-s10-t01-', dir=str(artifacts.parent), ignore_cleanup_errors=True) as directory:
        root = pathlib.Path(directory) / 'site'
        root.mkdir(mode=0o700)
        server, base = launch_server(binary, root)
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel='chrome', headless=True)
                try:
                    ctx = browser.new_context(viewport={'width': 1440, 'height': 900})
                    password = 'Review-Provider-Fixture-Local-123456!'
                    api_json(ctx, 'POST', base, '/auth/setup',
                             {'username': 'review_admin', 'password': password}, expected=201)
                    csrf = admin_login(ctx, base, 'review_admin', password, cms_locale="vi")
                    term = api_json(ctx, 'POST', base, '/api/admin/taxonomy/terms',
                        {'kind': 'category', 'name': 'Provider Category', 'slug': 'provider-category'},
                        csrf, expected=201)
                    term_id = term['id']
                    post = api_json(ctx, 'POST', base, '/api/admin/content-types/post/entries',
                        {'title': 'Provider Post', 'slug': 'provider-post',
                         'values': {'body': 'Visible body from server'}}, csrf, expected=201)
                    post_id = post['id']
                    safe_link_post = api_json(ctx, 'POST', base, '/api/admin/content-types/post/entries',
                        {'title':'Safe Provider Link','slug':'provider-safe-link',
                         'values':{'body':'/content/post/provider-post'}},csrf,expected=201)
                    unsafe_link_post = api_json(ctx, 'POST', base, '/api/admin/content-types/post/entries',
                        {'title':'Unsafe Provider Link','slug':'provider-unsafe-link',
                         'values':{'body':'javascript:alert(1)'}},csrf,expected=201)
                    checks.append('isolated_real_post_and_category_created')

                    pre_page = ctx.new_page()
                    pre_page.goto(base + '/admin/#/posts', wait_until='domcontentloaded')
                    expect(pre_page.get_by_test_id('entry-list-post')).to_be_visible(timeout=15000)
                    pre_page.screenshot(path=str(artifacts/'before-provider-list.png'), full_page=True)
                    pre_page.close()
                    base_config = list_config(ctx, base)
                    assert base_config == {'schema_version':1, 'columns':[]}, base_config
                    checks.append('baseline_core_list_no_synthetic_provider_column')

                    plugin = json.loads(PLUGIN_JSON.read_text())
                    bad = json.loads(json.dumps(plugin))
                    bad['admin_extensions']['columns'][0]['local_key'] = 'core_title'
                    upload(ctx, base, 'plugin', bad, csrf, expected=400)
                    bad = json.loads(json.dumps(plugin))
                    bad['admin_extensions']['columns'][0]['action_id'] = 'unknown_action'
                    upload(ctx, base, 'plugin', bad, csrf, expected=400)
                    bad = json.loads(json.dumps(plugin))
                    bad['admin_extensions']['schema_version'] = 2
                    upload(ctx, base, 'plugin', bad, csrf, expected=400)
                    bad = json.loads(json.dumps(plugin))
                    bad['admin_extensions']['columns'].append(bad['admin_extensions']['columns'][0])
                    upload(ctx, base, 'plugin', bad, csrf, expected=400)
                    bad = json.loads(json.dumps(plugin))
                    bad['admin_extensions']['columns'] = [
                        dict(bad['admin_extensions']['columns'][0], local_key=f'over_cap_{i}',
                             action_id=None) for i in range(17)]
                    upload(ctx, base, 'plugin', bad, csrf, expected=400)
                    assert not api_json(ctx, 'GET', base, '/api/admin/plugins')
                    checks.append('reject_namespace_undeclared_action_bad_schema_duplicates_and_17_columns_before_install')

                    installed = upload(ctx, base, 'plugin', plugin, csrf, expected=201)
                    assert installed['id'] == 'review-provider' and installed['enabled']
                    pcols = list_config(ctx, base)['columns']
                    assert len(pcols) == 1 and pcols[0]['label'] == 'Plugin body'
                    assert pcols[0]['action_id'] == 'inspect'
                    checks.append('real_plugin_zip_declarative_column_registry_and_action')

                    action = f'/api/admin/content-types/post/entries/{post_id}/admin-actions/review-provider/inspect'
                    unauth = browser.new_context()
                    response = unauth.request.post(base + action, headers={'Origin':base, 'If-Match':f'"v{post["version"]}"'})
                    assert response.status == 401, response.status
                    unauth.close()
                    without_csrf = ctx.request.post(base + action, headers={'Origin':base,'If-Match':f'"v{post["version"]}"'})
                    assert without_csrf.status in (401,403), without_csrf.status
                    forged = ctx.request.post(base + action.replace('inspect','not_registered'),
                        headers={'Origin':base,'X-CSRF-Token':csrf,'If-Match':f'"v{post["version"]}"'})
                    assert forged.status == 403, forged.status
                    forged_body = ctx.request.post(base + action, data='{"values":{"body":"FORGED"}}',
                        headers={'Origin':base,'X-CSRF-Token':csrf,'If-Match':f'"v{post["version"]}"',
                                 'Content-Type':'application/json'})
                    assert forged_body.status == 400, forged_body.status
                    wrong_origin = ctx.request.post(base + action,
                        headers={'Origin':'https://untrusted.example','X-CSRF-Token':csrf,
                                 'If-Match':f'"v{post["version"]}"'})
                    assert wrong_origin.status in (400,403),wrong_origin.status
                    checks.append('guest_401_csrf_origin_guards_forged_action_403_and_row_JSON_400')

                    page = ctx.new_page()
                    page.on('pageerror', lambda e: js_errors.append(str(e)))
                    page.goto(base + '/admin/#/posts', wait_until='domcontentloaded')
                    row = page.locator(f'tr[data-entry-id="{post_id}"]')
                    expect(row).to_be_visible(timeout=20000)
                    expect(page.locator('th[scope="col"]').filter(has_text='Plugin body')).to_be_visible()
                    expect(row.locator('[data-provider-column="plugin/review-provider/review_body"]')).to_contain_text('Visible body from server')
                    page.once('dialog', lambda dialog: dialog.accept())
                    row.get_by_role('button', name=f'Chạy Plugin body cho mục #{post_id}').click()
                    expect(page.get_by_test_id('provider-action-result')).to_contain_text(
                        f'Đã chạy action cho #{post_id}', timeout=20000)
                    page.screenshot(path=str(artifacts / 'plugin-list-desktop.png'), full_page=True)
                    checks.append('real_chrome_provider_column_and_explicit_confirmed_wasm_action')

                    pc = term_config(ctx, base, term_id)
                    pk = 'plugin/review-provider/review_details'
                    assert len(pc['panels']) == 1 and pc['panels'][0]['key'] == pk
                    changes = {pk: {'private_note':'Saved note', 'reviewed':True}}
                    updated = patch_term(ctx, base, term_id, term['version'], csrf,
                        {'extensions':changes,'schema_token':pc['schema_token']})
                    assert updated['version'] == term['version'] + 1
                    assert updated['extensions'][pk]['private_note'] == 'Saved note'
                    assert check_sqlite(root, term_id, pk)['reviewed'] is True
                    checks.append('atomic_core_extension_CAS_values_durable_and_legacy_GET_additive')

                    patch_term(ctx, base, term_id, term['version'], csrf,
                        {'extensions':{pk:{'private_note':'Stale'}}},
                        expected=412)
                    patch_term(ctx, base, term_id, updated['version'], csrf,
                        {'extensions':{pk:{'private_note':'Wrong token'}},
                         'schema_token':'bad'}, expected=409)
                    current = api_json(ctx,'GET',base,f'/api/admin/taxonomy/terms/{term_id}')
                    assert current['version'] == updated['version']
                    assert check_sqlite(root,term_id,pk)['private_note'] == 'Saved note'
                    checks.append('stale_412_and_wrong_schema_token_409_do_not_mutate_versions_or_values')

                    api_json(ctx,'POST',base,'/api/admin/users',
                        {'username':'review_editor','password':'Review-Editor-Fixture-123456!',
                         'role':'editor'},csrf,expected=201)
                    editor = browser.new_context()
                    editor_csrf = admin_login(editor,base,'review_editor','Review-Editor-Fixture-123456!')
                    editor_columns = list_config(editor,base)['columns']
                    assert len(editor_columns)==1 and not editor_columns[0].get('action_id')
                    forbidden_action = editor.request.post(base + action,
                        headers={'Origin':base,'X-CSRF-Token':editor_csrf,
                                 'If-Match':f'"v{post["version"]}"'})
                    assert forbidden_action.status == 403, forbidden_action.status
                    editable = term_config(editor,base,term_id)
                    assert len(editable['panels']) == 1
                    field_keys = [f['key'] for p in editable['panels'] for f in p['fields']]
                    assert field_keys == ['reviewed'], field_keys
                    limited = api_json(editor,'GET',base,f'/api/admin/taxonomy/terms/{term_id}')
                    assert 'private_note' not in limited['extensions'][pk]
                    patch_term(editor,base,term_id,updated['version'],editor_csrf,
                        {'extensions':{pk:{'private_note':'Forbidden'}},
                         'schema_token':pc['schema_token']}, expected=403)
                    editor_saved = patch_term(editor,base,term_id,updated['version'],editor_csrf,
                        {'extensions':{pk:{'reviewed':False}},
                         'schema_token':editable['schema_token']})
                    assert editor_saved['extensions'][pk]['reviewed'] is False
                    assert 'private_note' not in editor_saved['extensions'][pk]
                    assert check_sqlite(root,term_id,pk)['private_note']=='Saved note'
                    editor.close()
                    checks.append('editor_cannot_read_admin_field_and_can_only_patch_editable_field')

                    page.goto(base + f'/admin/#/posts/categories/{term_id}/edit', wait_until='domcontentloaded')
                    panel = page.get_by_test_id('term-extension-panels')
                    expect(panel).to_be_visible(timeout=20000)
                    note = panel.get_by_label('Private Review Note')
                    expect(note).to_have_value('Saved note')
                    note.fill('Updated through Chrome')
                    page.get_by_role('button', name='Lưu thay đổi', exact=True).click()
                    expect(page.get_by_role('status').filter(has_text='Đã lưu')).to_be_visible(timeout=18000)
                    assert check_sqlite(root,term_id,pk)['private_note'] == 'Updated through Chrome'
                    checks.append('chrome_term_panel_real_atomic_save_and_dirty_state')
                    page.set_viewport_size({'width':390,'height':844})
                    page.screenshot(path=str(artifacts/'term-extension-390.png'),full_page=True)
                    sizes = page.evaluate('({s:document.documentElement.scrollWidth,w:document.documentElement.clientWidth})')
                    assert sizes['s'] <= sizes['w'] + 2, sizes
                    checks.append('390px_term_panel_accessible_without_root_overflow')
                    page.set_viewport_size({'width':1440,'height':900})

                    theme = json.loads(THEME_JSON.read_text())
                    invalid_theme = json.loads(json.dumps(theme))
                    invalid_theme['admin_extensions']['columns'][0]['action_id'] = 'inspect'
                    upload(ctx,base,'theme',invalid_theme,csrf,expected=400)
                    installed_theme = upload(ctx,base,'theme',theme,csrf,expected=201)
                    assert installed_theme['theme_id'] == 'review-theme'
                    theme_list = api_json(ctx,'GET',base,'/api/admin/themes')
                    activated = api_json(ctx,'PUT',base,'/api/admin/themes/active',
                        {'revision_id':installed_theme['revision_id']},csrf,
                        {'If-Match':f'"v{theme_list["version"]}"'})
                    assert activated['theme_id']=='review-theme'
                    config = list_config(ctx,base)
                    assert [c['label'] for c in config['columns']] == ['Theme badge','Theme link','Plugin body']
                    both = term_config(ctx,base,term_id)
                    assert len(both['panels']) == 2
                    checks.append('theme_zip_staged_then_activated_adds_badge_and_panel_without_overriding_core')
                    page.goto(base+'/admin/#/posts',wait_until='domcontentloaded')
                    expect(page.get_by_role('columnheader',name='Theme badge')).to_be_visible(timeout=20000)
                    safe_row = page.locator(f'tr[data-entry-id="{safe_link_post["id"]}"]')
                    safe = safe_row.locator('[data-provider-column="theme/review-theme/same_origin_link"]')
                    expect(safe.get_by_role('link')).to_have_attribute(
                        'href','/content/post/provider-post',timeout=15000)
                    assert safe.get_by_role('link').get_attribute('target')=='_blank'
                    bad_row = page.locator(f'tr[data-entry-id="{unsafe_link_post["id"]}"]')
                    bad_cell = bad_row.locator('[data-provider-column="theme/review-theme/same_origin_link"]')
                    expect(bad_cell.get_by_text('Đường dẫn không hợp lệ')).to_be_visible()
                    assert bad_cell.get_by_role('link').count()==0
                    checks.append('theme_safe_link_uses_same_origin_and_js_url_never_becomes_clickable')
                    page.set_viewport_size({'width':390,'height':844})
                    page.screenshot(path=str(artifacts/'provider-columns-390.png'),full_page=True)
                    sizes = page.evaluate('({s:document.documentElement.scrollWidth,w:document.documentElement.clientWidth})')
                    assert sizes['s'] <= sizes['w'] + 2, sizes
                    assert page.get_by_test_id('entry-table').locator('th[scope="col"]').count() >= 8
                    checks.append('390px_provider_table_scrolls_internally_with_semantic_headers')
                    page.set_viewport_size({'width':1440,'height':900})
                    page.screenshot(path=str(artifacts/'plugin-theme-columns.png'),full_page=True)

                    latest = api_json(ctx,'GET',base,f'/api/admin/taxonomy/terms/{term_id}')
                    tk = 'theme/review-theme/theme_notes'
                    changed = patch_term(ctx,base,term_id,latest['version'],csrf,
                        {'extensions':{tk:{'caption':'Theme saved caption'}},
                         'schema_token':both['schema_token']})
                    assert changed['extensions'][tk]['caption']=='Theme saved caption'
                    checks.append('theme_term_extension_persisted_alongside_plugin_values')

                    # Original core-only PATCH remains compatible; omitted
                    # extensions are preserved exactly, not replaced with {}.
                    old = api_json(ctx,'GET',base,f'/api/admin/taxonomy/terms/{term_id}')
                    changed_core = patch_term(ctx,base,term_id,old['version'],csrf,
                        {'name':'Core edit preserves extensions'})
                    assert changed_core['name']=='Core edit preserves extensions'
                    assert check_sqlite(root,term_id,pk)['private_note']=='Updated through Chrome'
                    assert check_sqlite(root,term_id,tk)['caption']=='Theme saved caption'
                    checks.append('legacy_core_only_patch_omission_preserves_both_provider_values')

                    plugin_list = api_json(ctx,'GET',base,'/api/admin/plugins')
                    plugin_row = next(item for item in plugin_list if item['id']=='review-provider')
                    api_json(ctx,'POST',base,'/api/admin/plugins/review-provider/disable',
                        csrf=csrf,extra={'If-Match':str(plugin_row['cas_version'])})
                    assert [c['label'] for c in list_config(ctx,base)['columns']] == ['Theme badge','Theme link']
                    assert all(p['key']!=pk for p in term_config(ctx,base,term_id)['panels'])
                    assert check_sqlite(root,term_id,pk)['private_note']=='Updated through Chrome'
                    checks.append('disable_withdraws_plugin_UI_but_preserves_private_SQLite_values')

                    plugin_row = next(item for item in api_json(ctx,'GET',base,'/api/admin/plugins')
                                      if item['id']=='review-provider')
                    api_json(ctx,'POST',base,'/api/admin/plugins/review-provider/enable',
                        csrf=csrf,extra={'If-Match':str(plugin_row['cas_version'])})
                    assert len(list_config(ctx,base)['columns'])==3
                    assert term_config(ctx,base,term_id)['extensions'][pk]['private_note']=='Updated through Chrome'
                    checks.append('plugin_enable_restores_contributions_and_preserved_values')

                    with sqlite3.connect(root/'database/opendash.sqlite',timeout=15) as conn:
                        conn.execute("UPDATE plugins SET last_error='simulated worker crash' WHERE id='review-provider'")
                    assert not any(c['owner_kind']=='plugin' and c['owner_id']=='review-provider'
                                   for c in list_config(ctx,base)['columns'])
                    assert check_sqlite(root,term_id,pk)['private_note']=='Updated through Chrome'
                    plugin_row = next(p for p in api_json(ctx,'GET',base,'/api/admin/plugins')
                                      if p['id']=='review-provider')
                    api_json(ctx,'POST',base,'/api/admin/plugins/review-provider/disable',
                        csrf=csrf,extra={'If-Match':str(plugin_row['cas_version'])})
                    plugin_row = next(p for p in api_json(ctx,'GET',base,'/api/admin/plugins')
                                      if p['id']=='review-provider')
                    api_json(ctx,'POST',base,'/api/admin/plugins/review-provider/enable',
                        csrf=csrf,extra={'If-Match':str(plugin_row['cas_version'])})
                    assert 'Plugin body' in [c['label'] for c in list_config(ctx,base)['columns']]
                    checks.append('crash_withdraws_unsafe_provider_until_explicit_admin_reenable')

                    # The immutable revision digest includes admin_extensions.
                    collision = json.loads(json.dumps(plugin))
                    collision['admin_extensions']['columns'][0]['label']='Illicit overwrite'
                    upload(ctx,base,'plugin',collision,csrf,expected=409)
                    assert 'Illicit overwrite' not in [c['label'] for c in list_config(ctx,base)['columns']]
                    checks.append('same_version_changed_only_extension_rejected_before_publication')

                    # Roll forward and back through genuine plugin ZIP revisions.
                    upgrade = json.loads(json.dumps(plugin))
                    upgrade['version']='1.1.0'
                    upgrade['admin_extensions']['columns'][0]['label']='Upgraded body'
                    upload(ctx,base,'plugin',upgrade,csrf,expected=201)
                    assert 'Upgraded body' in [c['label'] for c in list_config(ctx,base)['columns']]
                    assert check_sqlite(root,term_id,pk)['private_note']=='Updated through Chrome'
                    upload(ctx,base,'plugin',plugin,csrf,expected=201)
                    assert 'Plugin body' in [c['label'] for c in list_config(ctx,base)['columns']]
                    checks.append('plugin_extension_upgrade_and_reinstall_old_revision_preserves_values')

                    # A package without admin_extensions must not remove
                    # declarations installed by the previous v2 revision.
                    omitted = json.loads(json.dumps(plugin))
                    omitted['version']='1.2.0'
                    del omitted['admin_extensions']
                    upload(ctx,base,'plugin',omitted,csrf,expected=201)
                    assert 'Plugin body' in [c['label'] for c in list_config(ctx,base)['columns']]
                    checks.append('optional_absent_extension_manifest_retains_registered_contributions')

                    # Combined 32-column cap is checked before publishing ZIP.
                    large = json.loads(json.dumps(plugin))
                    large['id']='large-provider'
                    large['actions']=[]
                    large['admin_extensions']['term_panels']=[]
                    base_column = large['admin_extensions']['columns'][0]
                    large['admin_extensions']['columns']=[
                        dict(base_column,local_key=f'extra_col_{i}',action_id=None) for i in range(16)]
                    upload(ctx,base,'plugin',large,csrf,expected=201)
                    overflow = json.loads(json.dumps(large))
                    overflow['id']='overflow-provider'
                    overflow['admin_extensions']['columns']=[
                        dict(base_column,local_key=f'overflow_{i}',action_id=None) for i in range(15)]
                    upload(ctx,base,'plugin',overflow,csrf,expected=409)
                    assert not any(p['id']=='overflow-provider' for p in
                        api_json(ctx,'GET',base,'/api/admin/plugins'))
                    assert len(list_config(ctx,base)['columns']) == 19
                    checks.append('reject_effective_34_column_collision_atomically_no_partial_provider')

                    # Theme switch to another ZIP withdraws only theme UI;
                    # already saved theme-private term data is still durable.
                    plain_theme = json.loads(json.dumps(theme))
                    plain_theme['theme_id']='review-plain'
                    plain_theme['name']='Review plain'
                    del plain_theme['admin_extensions']
                    plain = upload(ctx,base,'theme',plain_theme,csrf,expected=201)
                    selection = api_json(ctx,'GET',base,'/api/admin/themes')
                    api_json(ctx,'PUT',base,'/api/admin/themes/active',
                        {'revision_id':plain['revision_id']},csrf,
                        {'If-Match':f'"v{selection["version"]}"'})
                    assert not any(c['owner_kind']=='theme' for c in list_config(ctx,base)['columns'])
                    assert check_sqlite(root,term_id,tk)['caption']=='Theme saved caption'
                    checks.append('theme_switch_withdraws_columns_panels_but_preserves_values')

                    selection = api_json(ctx,'GET',base,'/api/admin/themes')
                    api_json(ctx,'PUT',base,'/api/admin/themes/active',
                        {'revision_id':installed_theme['revision_id']},csrf,
                        {'If-Match':f'"v{selection["version"]}"'})
                    assert term_config(ctx,base,term_id)['extensions'][tk]['caption']=='Theme saved caption'
                    checks.append('theme_revision_rollback_restores_extension_values')

                    assert not js_errors, js_errors
                    ctx.close()
                    stop_server(server)
                    server, base = launch_server(binary,root)
                    resumed = browser.new_context()
                    admin_login(resumed,base,'review_admin',password)
                    assert len(list_config(resumed,base)['columns'])==19
                    assert term_config(resumed,base,term_id)['extensions'][pk]['private_note']=='Updated through Chrome'
                    assert term_config(resumed,base,term_id)['extensions'][tk]['caption']=='Theme saved caption'
                    with sqlite3.connect(root/'database/opendash.sqlite') as conn:
                        assert conn.execute('PRAGMA foreign_key_check').fetchall() == []
                        assert conn.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
                    resumed.close()
                    checks.append('native_restart_restores_provider_generation_values_and_SQL_integrity')
                    return {'checks':checks, 'chrome_version':browser.version}
                finally:
                    browser.close()
        finally:
            stop_server(server)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=pathlib.Path,required=True)
    parser.add_argument('--artifact-dir',type=pathlib.Path,required=True)
    parser.add_argument('--predecessor-binary',type=pathlib.Path,
                        help='Optional matched #352 binary for clone-safe old/new/old migration checks')
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    artifacts = args.artifact_dir.resolve()
    artifacts.mkdir(parents=True,exist_ok=True)
    receipt = {
        'status':'FAIL',
        'source_sha':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        'source_tree_sha':subprocess.check_output(['git','rev-parse','HEAD^{tree}'],text=True).strip(),
        'binary_sha256':sha(binary),
        'os':platform.platform(),
        'architecture':platform.machine(),
        'admin_assets_sha256':{
            str(p.relative_to(CMS)):sha(p)
            for p in sorted((CMS/'admin/dist').rglob('*')) if p.is_file()
        },
        'fixture_sha256':{'plugin.json':sha(PLUGIN_JSON),'theme.json':sha(THEME_JSON)},
        'checks':[],
        'isolation':'temporary SQLite site, ephemeral loopback port; owner 18964 untouched',
        'not_run':['Windows native G5','Linux native G5','human dogfood',
                   'Usable Beta acceptance','common RC/V1'],
    }
    predecessor = args.predecessor_binary.resolve(strict=True) if args.predecessor_binary else None
    if predecessor:
        receipt['predecessor_binary_sha256'] = sha(predecessor)
    else:
        receipt['not_run'].append('old/new/old upgrade and rollback (no predecessor binary supplied)')
    try:
        receipt.update(run(binary,artifacts,receipt['checks']))
        if predecessor:
            verify_old_new_old(predecessor,binary,artifacts,receipt['checks'])
        receipt['status']='PASS'
    except Exception as exc:
        receipt['error']=f'{type(exc).__name__}: {exc}'
        traceback.print_exc()
    finally:
        receipt['screenshots_sha256']={p.name:sha(p) for p in artifacts.glob('*.png')}
        (artifacts/'receipt.json').write_text(
            json.dumps(receipt,indent=2,ensure_ascii=False)+'\n')
        print(json.dumps({key:receipt.get(key) for key in
                          ('status','checks','error','binary_sha256')},ensure_ascii=False))
    return 0 if receipt['status']=='PASS' else 1


if __name__=='__main__':
    raise SystemExit(main())
