#!/usr/bin/env python3
"""M7-S16-T04 real Chrome term lists/full editors; isolated native SQLite."""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import sqlite3
import subprocess
import tempfile
import traceback

from playwright.sync_api import expect, sync_playwright
from m7_beta_menu_picker_e2e import api_json, admin_login, launch_server, stop_server


def sha(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check_dialog(page, accepted: bool, act):
    caught = []
    def receive(dialog):
        caught.append((dialog.type, dialog.message))
        dialog.accept() if accepted else dialog.dismiss()
    page.once('dialog', receive)
    act()
    if not caught: page.wait_for_timeout(300)
    assert len(caught) == 1 and 'chưa lưu' in caught[0][1].lower(), caught


def run(binary: pathlib.Path, artifacts: pathlib.Path) -> dict:
    checks = []
    errors = []
    with tempfile.TemporaryDirectory(prefix='m7-s16-t04-', dir=str(artifacts.parent)) as directory:
        root = pathlib.Path(directory) / 'site'
        root.mkdir(mode=0o700)
        server, base = launch_server(binary, root)
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel='chrome', headless=True)
                try:
                    admin = browser.new_context(viewport={'width': 1440, 'height': 900})
                    password = 'Local-Taxonomy-Admin-Fixture-123456!'
                    api_json(admin, 'POST', base, '/auth/setup',
                             {'username': 'tax_owner', 'password': password}, expected=201)
                    csrf = admin_login(admin, base, 'tax_owner', password, cms_locale="vi")
                    root_term = api_json(admin, 'POST', base, '/api/admin/taxonomy/terms',
                        {'kind': 'category', 'name': 'News Root', 'slug': 'news-root'}, csrf, expected=201)
                    child_term = api_json(admin, 'POST', base, '/api/admin/taxonomy/terms',
                        {'kind': 'category', 'name': 'Child Section', 'slug': 'child-section',
                         'parent_id': root_term['id']}, csrf, expected=201)
                    tag = api_json(admin, 'POST', base, '/api/admin/taxonomy/terms',
                        {'kind': 'tag', 'name': 'Featured Tag', 'slug': 'featured-tag'}, csrf, expected=201)
                    api_json(admin, 'POST', base, '/api/admin/users',
                             {'username': 'tax_editor', 'password': 'Local-Taxonomy-Editor-123456!',
                              'role': 'editor'}, csrf, expected=201)
                    page = admin.new_page()
                    page.on('pageerror', lambda error: errors.append(str(error)))
                    page.goto(base + '/admin/#/posts/tags', wait_until='domcontentloaded')
                    tags_list = page.get_by_test_id('taxonomy-page')
                    expect(tags_list).to_be_visible(timeout=20000)
                    assert tags_list.get_attribute('data-taxonomy-kind') == 'tag'
                    expect(tags_list.get_by_text('Featured Tag', exact=True)).to_be_visible()
                    expect(tags_list.get_by_text('News Root', exact=True)).to_have_count(0)
                    assert page.locator('#taxonomy-name').count() == 0
                    checks.append('tags_route_fixed_kind_no_inline_form')
                    page.screenshot(path=str(artifacts / 'tags-list.png'), full_page=True)

                    page.goto(base + '/admin/#/posts/categories', wait_until='domcontentloaded')
                    cats_list = page.get_by_test_id('taxonomy-page')
                    assert cats_list.get_attribute('data-taxonomy-kind') == 'category'
                    expect(cats_list.get_by_text('News Root', exact=True)).to_be_visible()
                    expect(cats_list.locator(f'article[data-term-id="{child_term["id"]}"] h3')).to_contain_text('Child Section')
                    expect(cats_list.get_by_text('Featured Tag', exact=True)).to_have_count(0)
                    checks.append('categories_route_hierarchy_and_fixed_kind')

                    page.get_by_role('button', name=f'Sửa Child Section, ID {child_term["id"]}').click()
                    expected_hash = '#/posts/categories/{}/edit'.format(child_term['id'])
                    expect(page).to_have_url(base + '/admin/' + expected_hash)
                    term_editor = page.get_by_test_id('term-edit-page')
                    expect(term_editor).to_be_visible(timeout=18000)
                    assert term_editor.get_attribute('data-term-kind') == 'category'
                    assert term_editor.get_attribute('data-term-id') == str(child_term['id'])
                    expect(page.locator('#term-name')).to_have_value('Child Section')
                    expect(page.locator('#term-parent')).to_have_value(str(root_term['id']))
                    assert page.locator('[data-extension-slot="term-before-core"]').count() == 1
                    assert page.locator('[data-extension-slot="term-after-core"]').count() == 1
                    assert page.locator('#taxonomy-name').count() == 0
                    checks.append('full_screen_edit_stable_id_core_fields_extension_slots')
                    page.screenshot(path=str(artifacts / 'category-edit.png'), full_page=True)

                    page.locator('#term-name').fill('Unsaved Child Title')
                    check_dialog(page, False,
                                 lambda: page.get_by_role('button', name='Hủy / Về danh sách').click())
                    assert page.url.endswith(expected_hash)
                    assert page.locator('#term-name').input_value() == 'Unsaved Child Title'
                    unchanged = api_json(admin, 'GET', base, f'/api/admin/taxonomy/terms/{child_term["id"]}')
                    assert unchanged['name'] == 'Child Section'
                    checks.append('cancel_reject_keeps_term_draft_without_SQL_write')

                    check_dialog(page, True,
                                 lambda: page.get_by_role('button', name='Hủy / Về danh sách').click())
                    expect(page).to_have_url(base + '/admin/#/posts/categories')
                    expect(page.get_by_test_id('taxonomy-page')).to_contain_text('Child Section')
                    checks.append('cancel_accept_returns_matching_list_without_mutation')

                    page.goto(base + '/admin/' + expected_hash, wait_until='domcontentloaded')
                    expect(page.locator('#term-name')).to_have_value('Child Section', timeout=20000)
                    page.reload(wait_until='domcontentloaded')
                    expect(page.locator('#term-name')).to_have_value('Child Section', timeout=20000)
                    checks.append('direct_edit_reload_preserves_category_kind_and_ID')

                    page.locator('#term-name').fill('Draft Before History')
                    check_dialog(page, False, lambda: page.evaluate('history.back()'))
                    assert page.url.endswith(expected_hash)
                    assert page.locator('#term-name').input_value() == 'Draft Before History'
                    check_dialog(page, True, lambda: page.evaluate('history.back()'))
                    expect(page).to_have_url(base + '/admin/#/posts/categories')
                    page.evaluate('history.forward()')
                    expect(page.locator('#term-name')).to_have_value('Child Section', timeout=20000)
                    checks.append('history_back_guard_forward_reopens_server_term_without_autosave')

                    page.locator('#term-name').fill('Child Renamed')
                    page.get_by_role('button', name='Lưu thay đổi', exact=True).click()
                    expect(page.get_by_role('status').filter(has_text='Đã lưu')).to_be_visible(timeout=15000)
                    changed = api_json(admin, 'GET', base, f'/api/admin/taxonomy/terms/{child_term["id"]}')
                    assert changed['name'] == 'Child Renamed'
                    assert changed['id'] == child_term['id'] and changed['parent_id'] == root_term['id']
                    checks.append('rename_keeps_stable_id_and_category_parent')

                    page.goto(base + '/admin/#/posts/tags/{}/edit'.format(child_term['id']), wait_until='domcontentloaded')
                    expect(page.get_by_test_id('term-not-found')).to_be_visible(timeout=15000)
                    page.goto(base + '/admin/#/posts/categories/999999/edit', wait_until='domcontentloaded')
                    expect(page.get_by_test_id('term-not-found')).to_be_visible(timeout=15000)
                    assert admin.request.get(base + '/api/admin/taxonomy/terms/999999').status == 404
                    checks.append('wrong_kind_and_unknown_term_return_404_not_other_editor')

                    page.goto(base + '/admin/#/posts/tags/new', wait_until='domcontentloaded')
                    expect(page.get_by_test_id('term-edit-page')).to_be_visible(timeout=15000)
                    assert page.locator('#term-parent').count() == 0
                    page.locator('#term-name').fill('Never Created Tag')
                    check_dialog(page, True,
                                 lambda: page.get_by_role('button', name='Hủy / Về danh sách').click())
                    expect(page).to_have_url(base + '/admin/#/posts/tags')
                    assert all(term['slug'] != 'never-created-tag' for term in
                               api_json(admin,'GET',base,'/api/admin/taxonomy/terms?kind=tag')['items'])
                    checks.append('cancel_new_tag_does_not_insert_record')

                    page.get_by_role('button', name='Thêm mới').click()
                    expect(page.locator('#term-name')).to_be_visible(timeout=15000)
                    page.locator('#term-name').fill('Fresh Tag')
                    page.get_by_role('button', name='Tạo mới').click()
                    expect(page).to_have_url(re.compile(r'#/posts/tags/\d+/edit$'), timeout=20000)
                    new_id = int(re.search(r'/tags/(\d+)/edit$', page.url).group(1))
                    assert api_json(admin,'GET',base,f'/api/admin/taxonomy/terms/{new_id}')['name'] == 'Fresh Tag'
                    checks.append('new_tag_create_redirects_stable_ID_edit_page')

                    page.goto(base + '/admin/#/posts/categories/{}/edit'.format(child_term['id']), wait_until='domcontentloaded')
                    expect(page.locator('#term-name')).to_have_value('Child Renamed', timeout=20000)
                    page.locator('#term-name').fill('Unsaved Against CAS')
                    second = browser.new_context()
                    second_csrf = admin_login(second, base, 'tax_owner', password)
                    prev = api_json(second, 'GET', base, f'/api/admin/taxonomy/terms/{child_term["id"]}')
                    updated = api_json(second, 'PATCH', base, f'/api/admin/taxonomy/terms/{child_term["id"]}',
                                       {'name':'Concurrent Rename'}, second_csrf,
                                       {'If-Match': f'"v{prev["version"]}"'})
                    assert updated['version'] == prev['version'] + 1
                    page.get_by_role('button', name='Lưu thay đổi').click()
                    expect(page.get_by_role('alert')).to_contain_text('412', timeout=18000)
                    assert page.locator('#term-name').input_value() == 'Unsaved Against CAS'
                    page.get_by_role('button', name='Tải bản mới để so sánh').click()
                    expect(page.get_by_text('Concurrent Rename', exact=False)).to_be_visible()
                    page.get_by_role('button', name=re.compile('Tiếp tục với v')).click()
                    assert page.locator('#term-name').input_value() == 'Unsaved Against CAS'
                    page.get_by_role('button', name='Lưu thay đổi').click()
                    expect(page.get_by_role('status').filter(has_text='Đã lưu')).to_be_visible(timeout=20000)
                    assert api_json(admin, 'GET', base,
                                    f'/api/admin/taxonomy/terms/{child_term["id"]}')['name'] == 'Unsaved Against CAS'
                    checks.append('stale_412_draft_retained_latest_version_rebase_explicit')
                    second.close()

                    no_csrf = admin.request.fetch(base + f'/api/admin/taxonomy/terms/{new_id}',
                        method='PATCH', headers={'Origin':base,'If-Match':'"v1"',
                                                 'Content-Type':'application/json'},
                        data=json.dumps({'name':'Invalid CSRF'}))
                    assert no_csrf.status == 403, (no_csrf.status, no_csrf.text())
                    assert api_json(admin,'GET',base,f'/api/admin/taxonomy/terms/{new_id}')['name'] == 'Fresh Tag'
                    checks.append('csrf_missing_write_denied_and_db_unchanged')

                    cycle = admin.request.fetch(base + f'/api/admin/taxonomy/terms/{root_term["id"]}',
                        method='PATCH', headers={'Origin':base,'X-CSRF-Token':csrf,'If-Match':'"v1"',
                                                 'Content-Type':'application/json'},
                        data=json.dumps({'parent_id':child_term['id']}))
                    assert cycle.status == 409, (cycle.status, cycle.text())
                    checks.append('cycle_reparent_rejected_without_mutation')

                    # SQL link identity survives a rename and parent move.
                    linked = api_json(admin, 'POST', base, '/api/admin/content-types/post/entries',
                        {'title':'Term-linked Post','slug':'term-linked-post','values':{},
                         'term_ids':[child_term['id'],tag['id']]}, csrf, expected=201)
                    moved = api_json(admin,'GET',base,f'/api/admin/taxonomy/terms/{child_term["id"]}')
                    moved = api_json(admin,'PATCH',base,f'/api/admin/taxonomy/terms/{child_term["id"]}',
                        {'parent_id':None},csrf,{'If-Match':f'"v{moved["version"]}"'})
                    assert moved['id'] == child_term['id'] and moved['parent_id'] is None
                    linked_after = api_json(admin,'GET',base,
                        f'/api/admin/content-types/post/entries/{linked["id"]}')
                    assert sorted(linked_after['term_ids']) == sorted([child_term['id'],tag['id']])
                    linked_delete = admin.request.fetch(base+f'/api/admin/taxonomy/terms/{child_term["id"]}',
                        method='DELETE',headers={'Origin':base,'X-CSRF-Token':csrf,
                                                 'If-Match':f'"v{moved["version"]}"'})
                    assert linked_delete.status == 409
                    checks.append('rename_reparent_keeps_links_and_linked_delete_rejects_409')

                    parent_id = root_term['id']
                    for depth in range(2, 9):
                        term = api_json(admin,'POST',base,'/api/admin/taxonomy/terms',
                            {'kind':'category','name':f'Depth {depth}','slug':f'depth-{depth}',
                             'parent_id':parent_id},csrf,expected=201)
                        parent_id = term['id']
                    too_deep = admin.request.fetch(base+'/api/admin/taxonomy/terms',method='POST',
                        headers={'Origin':base,'X-CSRF-Token':csrf,'Content-Type':'application/json'},
                        data=json.dumps({'kind':'category','name':'Depth Nine','slug':'depth-nine',
                                         'parent_id':parent_id}))
                    assert too_deep.status == 409, (too_deep.status,too_deep.text())
                    checks.append('depth_eight_allowed_depth_nine_rejected')

                    page.goto(base+'/admin/#/posts/tags',wait_until='domcontentloaded')
                    expect(page.get_by_test_id('taxonomy-page')).to_contain_text('Fresh Tag')
                    page.once('dialog', lambda dialog: dialog.accept())
                    page.get_by_role('button',name=f'Xóa Fresh Tag, ID {new_id}').click()
                    expect(page.get_by_role('status').filter(has_text='Đã xóa')).to_be_visible(timeout=20000)
                    assert admin.request.get(base+f'/api/admin/taxonomy/terms/{new_id}').status == 404
                    checks.append('delete_unlinked_term_removes_only_selected_ID')

                    editor = browser.new_context(viewport={'width':390,'height':844})
                    admin_login(editor,base,'tax_editor','Local-Taxonomy-Editor-123456!')
                    editor_page = editor.new_page()
                    editor_page.goto(base+'/admin/#/posts/tags',wait_until='domcontentloaded')
                    expect(editor_page.get_by_test_id('taxonomy-page')).to_be_visible(timeout=20000)
                    assert editor_page.get_by_test_id('taxonomy-page').get_attribute('data-taxonomy-kind')=='tag'
                    assert editor_page.get_by_role('button',name='Thêm mới').count()==0
                    assert editor_page.locator('button[aria-label^="Sửa "]').count()==0
                    editor_page.goto(base+'/admin/#/posts/categories/{}/edit'.format(root_term['id']),
                                     wait_until='domcontentloaded')
                    expect(editor_page.get_by_test_id('rbac-forbidden')).to_be_visible(timeout=20000)
                    editor_page.goto(base+'/admin/#/posts/categories',wait_until='domcontentloaded')
                    expect(editor_page.get_by_test_id('taxonomy-page')).to_be_visible(timeout=20000)
                    overflow = editor_page.evaluate('document.documentElement.scrollWidth > window.innerWidth')
                    assert not overflow, '390px horizontal overflow in category list'
                    checks.append('editor_list_read_only_edit_403_mobile_390_no_overflow')
                    editor.close()

                    page.goto(base+'/admin/#/posts/categories/{}/edit'.format(root_term['id']),
                              wait_until='domcontentloaded')
                    expect(page.locator('#term-name')).to_have_value('News Root',timeout=20000)
                    page.locator('#term-name').fill('Uncommitted Before Refresh')
                    reloaded_dialog = []
                    page.once('dialog',lambda dialog: (reloaded_dialog.append(dialog.type),dialog.accept()))
                    page.reload(wait_until='domcontentloaded')
                    expect(page.locator('#term-name')).to_have_value('News Root',timeout=20000)
                    assert reloaded_dialog==['beforeunload'],reloaded_dialog
                    assert api_json(admin,'GET',base,
                        f'/api/admin/taxonomy/terms/{root_term["id"]}')['name']=='News Root'
                    checks.append('refresh_beforeunload_prompts_discards_RAM_not_SQL')

                    admin.close()
                    stop_server(server)
                    server, base = launch_server(binary,root)
                    resumed = browser.new_context(viewport={'width':390,'height':844})
                    resume_csrf = admin_login(resumed,base,'tax_owner',password)
                    latest_child = api_json(resumed,'GET',base,
                        f'/api/admin/taxonomy/terms/{child_term["id"]}')
                    assert latest_child['name']=='Unsaved Against CAS' and latest_child['parent_id'] is None
                    restored_post = api_json(resumed,'GET',base,
                        f'/api/admin/content-types/post/entries/{linked["id"]}')
                    assert sorted(restored_post['term_ids'])==sorted([child_term['id'],tag['id']])
                    resumed_page = resumed.new_page()
                    resumed_page.on('pageerror',lambda error: errors.append(str(error)))
                    resumed_page.goto(base+'/admin/#/posts/tags/{}/edit'.format(tag['id']),
                                      wait_until='domcontentloaded')
                    expect(resumed_page.locator('#term-name')).to_have_value('Featured Tag',timeout=20000)
                    assert resumed_page.get_by_test_id('term-edit-page').get_attribute('data-term-kind')=='tag'
                    assert not resumed_page.evaluate('document.documentElement.scrollWidth > window.innerWidth')
                    checks.append('native_restart_retains_route_kind_ID_links_mobile_390')
                    users = api_json(resumed,'GET',base,'/api/admin/users')
                    owner = next(entry for entry in users['items'] if entry['username']=='tax_owner')
                    manager_password = 'Local-Manager-Fixture-12345!'
                    api_json(resumed,'POST',base,'/api/admin/users',
                        {'username':'tax_manager','password':manager_password,'role':'admin'},
                        resume_csrf,expected=201)
                    manager = browser.new_context()
                    manager_csrf = admin_login(manager,base,'tax_manager',manager_password)
                    resumed_page.locator('#term-name').fill('Unsaved Term Secret')
                    changed_role = api_json(manager,'PATCH',base,
                        f'/api/admin/users/{owner["id"]}',{'role':'editor'},
                        manager_csrf,{'If-Match':f'"v{owner["version"]}"'})
                    assert changed_role['role']=='editor'
                    resumed_page.get_by_role('button',name='Lưu thay đổi').click()
                    expect(resumed_page.get_by_text('Đăng nhập quản trị')).to_be_visible(timeout=20000)
                    assert resumed_page.locator('#term-name').count()==0
                    assert api_json(manager,'GET',base,
                                    f'/api/admin/taxonomy/terms/{tag["id"]}')['name']=='Featured Tag'
                    checks.append('revoked_401_hides_term_draft_and_blocks_write')
                    manager.close()
                    resumed.close()
                    assert not errors, errors
                    checks.append('chrome_page_errors_none')
                finally:
                    browser.close()
        finally:
            stop_server(server)
    return {
        'task': 'M7-S16-T04',
        'status': 'PASS',
        'binary_sha256': sha(binary),
        'checks': checks,
        'not_run': ['owner localhost:18964', 'Windows/Linux', 'human dogfood/Beta acceptance'],
        'screenshots_sha256': {p.name: sha(p) for p in sorted(artifacts.glob('*.png'))},
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=pathlib.Path)
    parser.add_argument('--artifact-dir', required=True, type=pathlib.Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    artifacts = args.artifact_dir.resolve()
    artifacts.mkdir(parents=True, exist_ok=True)
    try:
        receipt = run(binary, artifacts)
    except Exception as cause:
        receipt = {
            'task': 'M7-S16-T04', 'status': 'FAIL',
            'binary_sha256': sha(binary),
            'error': str(cause), 'traceback': traceback.format_exc(),
            'not_run': ['owner localhost:18964', 'Windows/Linux', 'human dogfood/Beta acceptance'],
        }
    (artifacts / 'receipt.json').write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps({key: receipt.get(key) for key in ('task', 'status', 'checks', 'error')}, ensure_ascii=False, indent=2))
    return 0 if receipt['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
