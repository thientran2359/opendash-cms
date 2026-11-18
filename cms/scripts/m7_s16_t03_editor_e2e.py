#!/usr/bin/env python3
"""M7-S16-T03: native Chrome publication/taxonomy/legacy Page regression.

All browser sessions, fixture credentials, SQLite roots and ports are isolated.
Never targets the owner's long-lived review site at 127.0.0.1:18964.
"""
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
from m7_beta_draft_navigation_e2e import assert_draft_store_reauth_guards


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def open_editor(page, base: str, kind: str, entry_id: int | None = None) -> None:
    route = 'posts' if kind == 'post' else 'pages'
    suffix = f'{entry_id}/edit' if entry_id is not None else 'new'
    page.goto(f'{base}/admin/#/{route}/{suffix}', wait_until='domcontentloaded')
    expect(page.locator('#entry-title-input')).to_be_visible(timeout=20000)


def taxonomy_panel(page):
    panel = page.locator('[data-panel-id="core:taxonomies"]')
    expect(panel).to_have_count(1, timeout=20000)
    if panel.get_by_role('button', name='Mở rộng').count() > 0:
        panel.get_by_role('button', name='Mở rộng').click()
    expect(panel.get_by_test_id('single-edit-terms')).to_be_visible()
    return panel


def run(binary: pathlib.Path, artifact_dir: pathlib.Path) -> dict:
    checks: list[str] = []
    assert_draft_store_reauth_guards()
    checks.append('actual_TS_RAM_draft_quarantine_role_downgrade_reauth_scrub')
    with tempfile.TemporaryDirectory(prefix='m7-s16-t03-', dir=artifact_dir.parent) as temp:
        site = pathlib.Path(temp) / 'site'
        site.mkdir(mode=0o700)
        server, base = launch_server(binary, site)
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel='chrome', headless=True)
                try:
                    ctx = browser.new_context(viewport={'width': 1440, 'height': 900})
                    errors: list[str] = []
                    page = ctx.new_page()
                    page.on('pageerror', lambda err: errors.append(str(err)))
                    password = 'M7-S16-T03-Temporary-Admin-123!'
                    api_json(ctx, 'POST', base, '/auth/setup',
                             {'username': 'editor_owner', 'password': password}, expected=201)
                    csrf = admin_login(ctx, base, 'editor_owner', password)
                    category = api_json(ctx, 'POST', base, '/api/admin/taxonomy/terms',
                                        {'kind': 'category', 'name': 'Parent Cat', 'slug': 'parent-cat'}, csrf, expected=201)
                    child = api_json(ctx, 'POST', base, '/api/admin/taxonomy/terms',
                                     {'kind': 'category', 'name': 'Child Cat', 'slug': 'child-cat',
                                      'parent_id': category['id']}, csrf, expected=201)
                    tag = api_json(ctx, 'POST', base, '/api/admin/taxonomy/terms',
                                   {'kind': 'tag', 'name': 'Featured Tag', 'slug': 'featured-tag'}, csrf, expected=201)

                    open_editor(page, base, 'page')
                    page.screenshot(path=str(artifact_dir / 'page-new.png'), full_page=True)
                    expect(page.get_by_test_id('single-edit-terms')).to_have_count(0)
                    expect(page.locator('[data-panel-id="core:taxonomies"]')).to_have_count(0)
                    checks.append('default_page_has_no_taxonomy_panel')

                    open_editor(page, base, 'post')
                    panel = taxonomy_panel(page)
                    expect(page.get_by_test_id('single-edit-terms')).to_have_count(1)
                    expect(panel.get_by_label('Child Cat (child-cat)')).to_be_visible()
                    expect(panel.get_by_label('Featured Tag (featured-tag)')).to_be_visible()
                    checks.append('one_configured_post_taxonomy_panel_not_external_duplicate')
                    page.screenshot(path=str(artifact_dir / 'post-new.png'), full_page=True)

                    page.locator('#entry-title-input').fill('T03 Private Post')
                    page.locator('#entry-slug-input').fill('t03-private-post')
                    page.get_by_label('Đường dẫn tùy chỉnh (custom_path)').fill('/t03-public-custom')
                    panel.get_by_label('Child Cat (child-cat)').check()
                    panel.get_by_label('Featured Tag (featured-tag)').check()
                    expect(panel.get_by_role('list', name='Thẻ đã chọn')).to_contain_text('Featured Tag')
                    checks.append('category_tree_and_selected_tag_chip')
                    page.get_by_role('button', name='Lưu nháp', exact=True).first.click()
                    expect(page).to_have_url(re.compile(r'#/posts/\d+/edit$'), timeout=20000)
                    entry_id = int(re.search(r'/posts/(\d+)/edit', page.url).group(1))
                    saved = api_json(ctx, 'GET', base, f'/api/admin/content-types/post/entries/{entry_id}')
                    assert saved['publication_status'] == 'draft', saved
                    assert sorted(saved['term_ids']) == sorted([child['id'], tag['id']]), saved
                    assert ctx.request.get(base + '/t03-public-custom').status == 404
                    checks.append('explicit_draft_save_is_private_with_stable_terms')

                    # Legacy Page can have term links from before the new default.
                    old_page = api_json(ctx, 'POST', base, '/api/admin/content-types/page/entries',
                        {'slug': 't03-legacy-page', 'title': 'Legacy Page', 'values': {},
                         'term_ids': [category['id'], tag['id']]}, csrf, expected=201)
                    old_page_id = int(old_page['id'])
                    open_editor(page, base, 'page', old_page_id)
                    expect(page.get_by_test_id('single-edit-terms')).to_have_count(0)
                    page.locator('#entry-title-input').fill('Legacy Page Revised')
                    action = 'Cập nhật' if old_page['publication_status'] == 'published' else 'Lưu nháp'
                    page.get_by_role('button', name=action, exact=True).first.click()
                    expect(page.get_by_role('status').filter(has_text='Đã lưu thành công')).to_be_visible(timeout=20000)
                    preserved = api_json(ctx, 'GET', base, f'/api/admin/content-types/page/entries/{old_page_id}')
                    assert preserved['title'] == 'Legacy Page Revised'
                    assert sorted(preserved['term_ids']) == sorted([category['id'], tag['id']]), preserved
                    checks.append('legacy_page_term_ids_omitted_preserved_on_unrelated_save')

                    open_editor(page, base, 'page')
                    page.locator('#entry-title-input').fill('T03 Published Page')
                    page.locator('#entry-slug-input').fill('t03-published-page')
                    page.get_by_role('button', name='Đăng bài', exact=True).first.click()
                    expect(page).to_have_url(re.compile(r'#/pages/\d+/edit$'), timeout=20000)
                    new_page_id = int(re.search(r'/pages/(\d+)/edit', page.url).group(1))
                    fresh_page = api_json(ctx, 'GET', base, f'/api/admin/content-types/page/entries/{new_page_id}')
                    assert fresh_page['publication_status'] == 'published'
                    expect(page.get_by_role('button', name='Cập nhật', exact=True).first).to_be_visible()
                    assert ctx.request.get(base + '/content/page/t03-published-page').status == 200
                    checks.append('single_atomic_page_publish_and_public_path')

                    # Restart using the same *disposable* root. The owner review
                    # root/port never appears in this runner.
                    ctx.close()
                    stop_server(server)
                    server, base = launch_server(binary, site)
                    resumed = browser.new_context(viewport={'width': 1440, 'height': 900})
                    csrf2 = admin_login(resumed, base, 'editor_owner', password)
                    saved_after_restart = api_json(resumed, 'GET', base,
                        f'/api/admin/content-types/post/entries/{entry_id}')
                    assert saved_after_restart['publication_status'] == 'draft'
                    assert sorted(saved_after_restart['term_ids']) == sorted([child['id'], tag['id']])
                    assert resumed.request.get(base + '/t03-public-custom').status == 404
                    page2 = resumed.new_page()
                    page2.on('pageerror', lambda err: errors.append(str(err)))
                    open_editor(page2, base, 'post', entry_id)
                    post_tax = taxonomy_panel(page2)
                    expect(post_tax.get_by_label('Child Cat (child-cat)')).to_be_checked()
                    expect(post_tax.get_by_role('list', name='Thẻ đã chọn')).to_contain_text('Featured Tag')
                    checks.append('restart_preserves_private_draft_and_taxonomy_selection')

                    page2.get_by_role('button', name='Đăng bài', exact=True).first.click()
                    expect(page2.get_by_role('button', name='Cập nhật', exact=True).first).to_be_visible(timeout=20000)
                    published = api_json(resumed, 'GET', base,
                        f'/api/admin/content-types/post/entries/{entry_id}')
                    assert published['publication_status'] == 'published'
                    link = page2.get_by_role('link', name='Xem bài viết').first
                    assert link.get_attribute('href') == '/t03-public-custom'
                    assert link.get_attribute('target') == '_blank'
                    assert set(link.get_attribute('rel').split()) == {'noopener', 'noreferrer'}
                    assert resumed.request.get(base + '/t03-public-custom').status == 200
                    checks.append('publish_atomically_and_view_uses_current_custom_route_safe_new_tab')

                    page2.locator('#entry-title-input').fill('T03 Updated Public Post')
                    page2.get_by_role('button', name='Cập nhật', exact=True).first.click()
                    expect(page2.get_by_role('status').filter(has_text='Đã lưu thành công')).to_be_visible(timeout=20000)
                    updated = api_json(resumed, 'GET', base,
                        f'/api/admin/content-types/post/entries/{entry_id}')
                    assert updated['title'] == 'T03 Updated Public Post'
                    assert updated['publication_status'] == 'published'
                    assert resumed.request.get(base + '/t03-public-custom').status == 200
                    checks.append('published_update_keeps_public_route_and_terms')

                    page2.locator('#entry-title-input').fill('T03 Unsaved After Conflict')
                    other = browser.new_context()
                    other_csrf = admin_login(other, base, 'editor_owner', password)
                    api_json(other, 'POST', base,
                             f'/api/admin/content-types/post/entries/{entry_id}/publication',
                             {'status': 'published'}, other_csrf,
                             {'If-Match': f'"v{updated["version"]}"'})
                    page2.get_by_role('button', name='Cập nhật', exact=True).first.click()
                    expect(page2.get_by_role('alert').filter(has_text='412')).to_be_visible(timeout=20000)
                    assert page2.locator('#entry-title-input').input_value() == 'T03 Unsaved After Conflict'
                    assert api_json(resumed, 'GET', base,
                                    f'/api/admin/content-types/post/entries/{entry_id}')['title'] == updated['title']
                    other.close()
                    checks.append('stale_412_retains_unsaved_editor_payload')
                    page2.screenshot(path=str(artifact_dir / 'post-published.png'), full_page=True)
                    # Verify that role revocation locks the editor before re-authentication.
                    user_list = api_json(resumed, 'GET', base, '/api/admin/users')
                    active_user = next(item for item in user_list['items'] if item['username'] == 'editor_owner')
                    manager_password = 'M7-T03-Manager-Test-1234!'
                    api_json(resumed, 'POST', base, '/api/admin/users',
                             {'username': 'manager_t03', 'password': manager_password, 'role': 'admin'},
                             csrf2, expected=201)
                    manager = browser.new_context()
                    manager_csrf = admin_login(manager, base, 'manager_t03', manager_password)
                    demoted = api_json(manager, 'PATCH', base,
                                       f'/api/admin/users/{active_user["id"]}',
                                       {'role': 'editor'}, manager_csrf,
                                       {'If-Match': '"v{}"'.format(active_user['version'])})
                    assert demoted['role'] == 'editor'
                    page2.get_by_role('button', name='Cập nhật', exact=True).first.click()
                    expect(page2.get_by_text('Đăng nhập quản trị')).to_be_visible(timeout=20000)
                    expect(page2.locator('#entry-title-input')).to_have_count(0)
                    checks.append('401_role_downgrade_revokes_session_and_hides_editor')
                    page2.get_by_placeholder('Tên đăng nhập').fill('editor_owner')
                    page2.get_by_placeholder('Mật khẩu').fill(password)
                    page2.get_by_role('button', name='Đăng nhập', exact=True).click()
                    expect(page2.get_by_test_id('admin-layout')).to_be_visible(timeout=20000)
                    assert api_json(resumed, 'GET', base, '/auth/me')['user']['role'] == 'editor'
                    fresh_editor = browser.new_context()
                    admin_login(fresh_editor, base, 'editor_owner', password)
                    fresh_editor_page = fresh_editor.new_page()
                    fresh_editor_page.goto(base + '/admin/#/users/new', wait_until='domcontentloaded')
                    expect(fresh_editor_page.get_by_test_id('rbac-forbidden')).to_be_visible(timeout=20000)
                    fresh_editor.close()
                    checks.append('reauth_downgraded_role_cannot_recover_admin_fields')
                    manager.close()

                    # Explicit user-owned Page eligibility survives the migration
                    # and appears only for its declared kind; historical tag IDs
                    # remain linked even when Tag is no longer eligible.
                    resumed.close()
                    stop_server(server)
                    database = sqlite3.connect(site / 'database/opendash.sqlite')
                    try:
                        database.execute('''
                          INSERT INTO content_type_taxonomy_eligibility
                            (content_type_key,taxonomy_kind,owner_kind,owner_id,
                             declaration_state,provider_status)
                          VALUES ('page','category','user',NULL,'declared','active')
                        ''')
                        database.commit()
                    finally:
                        database.close()
                    server, base = launch_server(binary, site)
                    eligible = browser.new_context()
                    admin_login(eligible, base, 'editor_owner', password)
                    eligible_page = eligible.new_page()
                    open_editor(eligible_page, base, 'page', old_page_id)
                    eligible_panel = taxonomy_panel(eligible_page)
                    expect(eligible_panel.get_by_role('group', name=re.compile('Danh mục'))).to_be_visible()
                    expect(eligible_panel.get_by_role('group', name=re.compile('Thẻ'))).to_have_count(0)
                    eligible_page.locator('#entry-title-input').fill('Legacy Eligible Page Updated')
                    eligible_page.get_by_role('button', name=action, exact=True).first.click()
                    expect(eligible_page.get_by_role('status').filter(has_text='Đã lưu thành công')).to_be_visible(timeout=20000)
                    eligible_state = api_json(eligible, 'GET', base,
                         f'/api/admin/content-types/page/entries/{old_page_id}')
                    assert sorted(eligible_state['term_ids']) == sorted([category['id'], tag['id']])
                    checks.append('non_core_page_grant_ui_and_hidden_legacy_tag_survive_save')
                    eligible.close()

                    stop_server(server)
                    database = sqlite3.connect(site / 'database/opendash.sqlite')
                    try:
                        database.execute('''UPDATE content_type_taxonomy_eligibility
                          SET provider_status='provider_inactive'
                          WHERE content_type_key='page' AND owner_kind='user' AND taxonomy_kind='category' ''')
                        database.commit()
                    finally:
                        database.close()
                    server, base = launch_server(binary, site)
                    inactive = browser.new_context()
                    admin_login(inactive, base, 'editor_owner', password)
                    inactive_page = inactive.new_page()
                    open_editor(inactive_page, base, 'page', old_page_id)
                    expect(inactive_page.get_by_test_id('single-edit-terms')).to_have_count(0)
                    assert sorted(api_json(inactive, 'GET', base,
                         f'/api/admin/content-types/page/entries/{old_page_id}')['term_ids']) == sorted([category['id'], tag['id']])
                    inactive.close()
                    checks.append('inactive_taxonomy_provider_hides_page_panel_without_erasing_old_links')

                    assert not errors, errors
                    checks.append('chrome_page_errors_none')
                    return {'checks': checks, 'chrome_version': browser.version}
                finally:
                    browser.close()
        finally:
            stop_server(server)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=pathlib.Path, required=True)
    parser.add_argument('--artifact-dir', type=pathlib.Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    artifacts = args.artifact_dir.resolve()
    artifacts.mkdir(parents=True, exist_ok=True)
    receipt = {
        'status': 'FAIL', 'source_sha': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        'binary_sha256': digest(binary), 'checks': [],
        'isolation': 'disposable SQLite root and loopback random port, not owner site 18964',
    }
    try:
        result = run(binary, artifacts)
        receipt.update({'status': 'PASS', **result})
    except Exception as error:
        receipt['error'] = f'{type(error).__name__}: {error}'
        traceback.print_exc()
    finally:
        (artifacts / 'receipt.json').write_text(json.dumps(receipt, indent=2, ensure_ascii=False) + '\n')
        print(json.dumps({key: receipt.get(key) for key in ('status', 'source_sha', 'binary_sha256', 'checks', 'error')},
                         ensure_ascii=False))
    return 0 if receipt['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
