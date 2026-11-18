#!/usr/bin/env python3
"""M7-S16-T06: real native Chrome entry tables, bulk-CAS, URL history & mobile.

Only disposable private roots and ephemeral loopback ports. Never touches
the owner's restored review site at 127.0.0.1:18964.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import sqlite3
import tempfile
import time
import traceback

from playwright.sync_api import expect, sync_playwright
from m7_beta_menu_picker_e2e import api_json, admin_login, launch_server, stop_server


def sha(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fixture_data(root: pathlib.Path, cat: dict, child: dict, tag: dict) -> dict:
    db = sqlite3.connect(root / 'database/opendash.sqlite', timeout=15)
    posts = []
    try:
        for i in range(125):
            title = f'List Alpha {i:03}'
            if i == 96:
                title = '100% Literal Entry'
            if i == 97:
                title = 'Under_score Literal Entry'
            status = 'published' if i % 5 == 0 else 'draft'
            month = '2026-10' if i < 45 else '2026-11'
            date = f'{month}-03 10:00:00'
            cursor = db.execute(
                'INSERT INTO content_entries'
                '(content_type_key,slug,title,values_json,publication_status,created_at,updated_at)'
                ' VALUES(?,?,?,?,?,?,?)',
                ('post', f'list-alpha-{i:03}', title, '{}', 'draft', date, date),
            )
            ident = cursor.lastrowid
            if status == 'published':
                db.execute('UPDATE content_entries SET publication_status=? WHERE id=?',
                           ('published', ident))
            posts.append(dict(id=ident, status=status, title=title, index=i))
            if i in (1, 2, 3, 46):
                term = cat if i == 1 else child
                db.execute('INSERT INTO entry_terms(entry_id,term_id) VALUES(?,?)',
                           (ident, term['id']))
                if i != 3:
                    db.execute('INSERT INTO entry_terms(entry_id,term_id) VALUES(?,?)',
                               (ident, tag['id']))
        db.execute(
            'INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)'
            ' VALUES(?,?,?,?,?)', ('page', 'list-page', 'List Page Fixture', '{}', 'draft'))
        db.execute("UPDATE content_entries SET publication_status='published' WHERE slug='list-page'")
        db.execute(
            "INSERT INTO content_types(key,definition_json,owner_kind,provider_status,declaration_state,version)"
            " VALUES('portfolio',?,'user','active','declared',1)",
            (json.dumps({'key':'portfolio','label':'Portfolio','fields':[]}),))
        db.execute(
            'INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)'
            ' VALUES(?,?,?,?,?)', ('portfolio', 'list-portfolio', 'Portfolio Fixture', '{}', 'published'))
        db.execute(
            'INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)'
            ' VALUES(?,?,?,?,?)', ('portfolio', 'draft-portfolio', 'Portfolio Draft', '{}', 'draft'))
        db.commit()
    finally:
        db.close()
    return {'posts':posts}


def accept_once(page, expected: str | None = None, accept: bool = True):
    dialogs = []
    def handler(dialog):
        dialogs.append((dialog.type, dialog.message))
        dialog.accept() if accept else dialog.dismiss()
    page.once('dialog', handler)
    def verify():
        if expected:
            assert len(dialogs) == 1 and expected.lower() in dialogs[0][1].lower(), dialogs
        else:
            assert len(dialogs) == 1, dialogs
    return verify


def table(page):
    root = page.get_by_test_id('entry-list-post')
    expect(root).to_be_visible(timeout=20000)
    expect(root.get_by_test_id('entry-table')).to_be_visible(timeout=15000)
    return root


def wait_count(root, count: int):
    expect(root.get_by_role('status').filter(has_text=f'Tổng: {count} mục phù hợp')).to_be_visible(timeout=16000)


def run(binary: pathlib.Path, artifacts: pathlib.Path) -> dict:
    checks: list[str] = []
    errors: list[str] = []
    with tempfile.TemporaryDirectory(prefix='m7-s16-t06-', dir=artifacts.parent) as temp:
        site = pathlib.Path(temp) / 'site'
        site.mkdir(mode=0o700)
        server, base = launch_server(binary, site)
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel='chrome', headless=True)
                try:
                    ctx = browser.new_context(viewport={'width': 1440, 'height': 900})
                    admin_password = 'M7-S16-T06-Local-Admin-Test-123456!'
                    api_json(ctx, 'POST', base, '/auth/setup',
                             {'username':'table_owner','password':admin_password}, expected=201)
                    csrf = admin_login(ctx, base, 'table_owner', admin_password)
                    category = api_json(ctx, 'POST', base, '/api/admin/taxonomy/terms',
                                        {'kind':'category','name':'Table Root','slug':'table-root'}, csrf, expected=201)
                    child = api_json(ctx, 'POST', base, '/api/admin/taxonomy/terms',
                                     {'kind':'category','name':'Table Child','slug':'table-child',
                                      'parent_id':category['id']}, csrf, expected=201)
                    tag = api_json(ctx, 'POST', base, '/api/admin/taxonomy/terms',
                                   {'kind':'tag','name':'Table Tag','slug':'table-tag'}, csrf, expected=201)
                    fixture = fixture_data(site, category, child, tag)
                    page = ctx.new_page()
                    page.on('pageerror', lambda cause: errors.append(str(cause)))
                    page.goto(base + '/admin/#/posts', wait_until='domcontentloaded')
                    expect(page.get_by_test_id('entry-list-post')).to_be_visible(timeout=20000)
                    page.screenshot(path=str(artifacts / 'initial-list.png'), full_page=True)
                    root = table(page)
                    wait_count(root, 125)
                    expect(root.locator('tbody tr[data-entry-id]')).to_have_count(20)
                    assert root.locator('th[scope="col"]').count() >= 5
                    checks.append('125_rows_server_paged_semantic_table_and_accessible_headers')
                    page.screenshot(path=str(artifacts / 'post-list-desktop.png'), full_page=True)

                    root.get_by_role('button', name='Trang sau').click()
                    expect(page).to_have_url(re.compile(r'offset=20'))
                    expect(root.locator('tbody tr[data-entry-id]')).to_have_count(20)
                    assert root.locator('tr[data-entry-id]').first.get_attribute('data-entry-id') != str(fixture['posts'][-1]['id'])
                    checks.append('next_page_uses_remote_page_not_first_page_filter')

                    root.get_by_role('button', name='Trang trước').click()
                    expect(page).to_have_url(base + '/admin/#/posts')
                    search = root.get_by_label('Tìm tiêu đề hoặc slug')
                    search.fill('List Alpha 089')
                    root.get_by_role('button', name='Tìm kiếm').click()
                    expect(page).to_have_url(re.compile(r'q=List\+Alpha\+089'))
                    wait_count(root, 1)
                    assert root.locator('tr[data-entry-id]').first.get_attribute('data-entry-id') == str(fixture['posts'][89]['id'])
                    checks.append('search_matches_row_beyond_100_without_client_page_clipping')
                    page.screenshot(path=str(artifacts / 'post-list-filtered.png'), full_page=True)

                    search.fill('%')
                    root.get_by_role('button', name='Tìm kiếm').click()
                    wait_count(root, 1)
                    expect(root.get_by_text('100% Literal Entry')).to_be_visible()
                    search.fill('_')
                    root.get_by_role('button', name='Tìm kiếm').click()
                    wait_count(root, 1)
                    expect(root.get_by_text('Under_score Literal Entry')).to_be_visible()
                    checks.append('literal_percent_underscore_search_from_UI')

                    root.get_by_role('button', name='Xóa tất cả bộ lọc').click()
                    wait_count(root, 125)
                    root.get_by_test_id('sort-title').click()
                    expect(page).to_have_url(re.compile(r'sort=title'))
                    root.get_by_test_id('sort-title').click()
                    expect(page).to_have_url(re.compile(r'order=asc'))
                    expect(root.locator('tr[data-entry-id]').first).to_contain_text('100% Literal Entry')
                    checks.append('sortable_headings_server_order_and_url_roundtrip')

                    root.get_by_role('button', name='Bản nháp (100)').click()
                    wait_count(root, 100)
                    expect(root.get_by_role('button', name='Đã xuất bản (25)')).to_be_visible()
                    assert root.locator('tbody tr[data-entry-id]').first.get_by_role('link', name='Xem').count() == 0
                    checks.append('status_counts_ignore_status_but_total_filters_draft_no_view')

                    root.get_by_role('button', name='Xóa tất cả bộ lọc').click()
                    wait_count(root, 125)
                    expect(root.get_by_label('Lọc danh mục')).to_be_visible(timeout=20000)
                    expect(root.get_by_label('Lọc thẻ')).to_be_visible(timeout=20000)
                    root.get_by_label('Lọc danh mục').select_option(str(category['id']))
                    wait_count(root, 1)
                    root.get_by_label('Gồm danh mục con').check()
                    wait_count(root, 4)
                    root.get_by_label('Lọc thẻ').select_option(str(tag['id']))
                    wait_count(root, 3)
                    root.get_by_label('Lọc tháng cập nhật').fill('2026-10')
                    wait_count(root, 2)
                    root.get_by_label('Lọc tháng cập nhật').fill('2026-11')
                    wait_count(root, 1)
                    assert root.locator('tr[data-entry-id]').first.get_attribute('data-entry-id') == str(fixture['posts'][46]['id'])
                    assert all(x in page.url for x in ('category_id=', 'tag_id=', 'updated_month=2026-11', 'include_descendants=true'))
                    checks.append('category_tag_descendants_month_AND_before_paging_real_server_counts')

                    page.screenshot(path=str(artifacts / 'post-list-taxonomy-and-month.png'), full_page=True)
                    remembered_hash = page.url.split('#', 1)[1]
                    root.get_by_role('button', name=re.compile('Chỉnh sửa List Alpha 046')).click()
                    expect(page).to_have_url(re.compile(r'#/posts/\d+/edit\?'))
                    expect(page.locator('#entry-title-input')).to_have_value('List Alpha 046', timeout=20000)
                    page.get_by_role('button', name='Về danh sách').click()
                    expect(page).to_have_url(base + '/admin/#' + remembered_hash)
                    wait_count(table(page), 1)
                    page.reload(wait_until='domcontentloaded')
                    wait_count(table(page), 1)
                    assert page.url.endswith('#' + remembered_hash)
                    checks.append('edit_return_reload_preserves_full_query_and_deeplink')

                    page.go_back(wait_until='domcontentloaded')
                    page.go_forward(wait_until='domcontentloaded')
                    expect(page.get_by_test_id('entry-list-post')).to_be_visible(timeout=20000)
                    checks.append('hash_history_back_forward_preserves_table')

                    page.goto(base + '/admin/#/posts?q=List+Alpha+003', wait_until='domcontentloaded')
                    root = table(page)
                    wait_count(root, 1)
                    old = api_json(ctx, 'GET', base,
                                   f'/api/admin/content-types/post/entries/{fixture["posts"][3]["id"]}')
                    root.get_by_label(f'Chọn mục #{old["id"]}').check()
                    newer = api_json(ctx, 'POST', base,
                        f'/api/admin/content-types/post/entries/{old["id"]}/publication',
                        {'status': 'published'}, csrf,
                        {'If-Match': f'"v{old["version"]}"'})
                    assert newer['version'] > old['version']
                    verify = accept_once(page, 'xuất bản')
                    root.get_by_role('button', name='Xuất bản đã chọn').click()
                    verify()
                    expect(root.get_by_text('412 — Mục đã thay đổi', exact=False)).to_be_visible(timeout=20000)
                    expect(root.get_by_label(f'Chọn mục #{old["id"]}')).to_be_checked()
                    checks.append('real_version_conflict_412_retains_failed_selection_and_per_item_result')
                    page.screenshot(path=str(artifacts / 'bulk-412.png'), full_page=True)

                    root.get_by_role('button', name='Xóa tất cả bộ lọc').click()
                    wait_count(root, 125)
                    target = fixture['posts'][4]
                    root.get_by_label(f'Chọn mục #{target["id"]}').check() if root.get_by_label(
                        f'Chọn mục #{target["id"]}').count() else None
                    # Stable search locates the row even when updated_at ordering moves it.
                    root.get_by_label('Tìm tiêu đề hoặc slug').fill('List Alpha 004')
                    root.get_by_role('button', name='Tìm kiếm').click()
                    wait_count(root, 1)
                    root.get_by_label(f'Chọn mục #{target["id"]}').check()
                    verify = accept_once(page, 'xuất bản')
                    root.get_by_role('button', name='Xuất bản đã chọn').click()
                    verify()
                    expect(root.get_by_text(f'#{target["id"]}: xuất bản thành công.')).to_be_visible(timeout=20000)
                    saved = api_json(ctx, 'GET', base, f'/api/admin/content-types/post/entries/{target["id"]}')
                    assert saved['publication_status'] == 'published'
                    checks.append('bulk_publish_confirmation_versioned_write_success_and_server_refresh')

                    root.get_by_label(f'Chọn mục #{target["id"]}').check()
                    verify = accept_once(page, 'chuyển về nháp')
                    root.get_by_role('button', name='Chuyển về nháp').click()
                    verify()
                    expect(root.get_by_text(f'#{target["id"]}: chuyển về nháp thành công.')).to_be_visible(timeout=20000)
                    saved = api_json(ctx, 'GET', base, f'/api/admin/content-types/post/entries/{target["id"]}')
                    assert saved['publication_status'] == 'draft'
                    checks.append('bulk_draft_versioned_write_and_public_view_removed')

                    root.get_by_label(f'Chọn mục #{target["id"]}').check()
                    verify = accept_once(page, 'xóa vĩnh viễn', accept=False)
                    root.get_by_role('button', name='Xóa đã chọn').click()
                    verify()
                    assert ctx.request.get(base + f'/api/admin/content-types/post/entries/{target["id"]}').status == 200
                    verify = accept_once(page, 'xóa vĩnh viễn')
                    root.get_by_role('button', name='Xóa đã chọn').click()
                    verify()
                    expect(root.get_by_text(f'#{target["id"]}: xóa vĩnh viễn thành công.')).to_be_visible(timeout=20000)
                    assert ctx.request.get(base + f'/api/admin/content-types/post/entries/{target["id"]}').status == 404
                    checks.append('bulk_delete_cancel_and_confirm_with_real_durable_delete')

                    root.get_by_role('button', name='Xóa tất cả bộ lọc').click()
                    wait_count(root, 124)
                    mixed = [fixture['posts'][i] for i in (11, 12, 13)]
                    root.get_by_label('Tìm tiêu đề hoặc slug').fill('List Alpha 01')
                    root.get_by_role('button', name='Tìm kiếm').click()
                    wait_count(root, 10)
                    for item in mixed:
                        root.get_by_label(f'Chọn mục #{item["id"]}').check()
                    conflict = mixed[0]
                    stale = api_json(ctx, 'GET', base,
                        f'/api/admin/content-types/post/entries/{conflict["id"]}')
                    api_json(ctx, 'POST', base,
                        f'/api/admin/content-types/post/entries/{conflict["id"]}/publication',
                        {'status':'published'}, csrf, {'If-Match': f'"v{stale["version"]}"'})
                    forbidden = mixed[1]
                    blocked_url = f'**/api/admin/content-types/post/entries/{forbidden["id"]}/publication'
                    page.route(blocked_url, lambda route: route.fulfill(status=403,
                        content_type='application/json', body='{"error":"forbidden"}'))
                    verify = accept_once(page, 'xuất bản')
                    root.get_by_role('button', name='Xuất bản đã chọn').click()
                    verify()
                    success = mixed[2]
                    expect(root.get_by_text(f'#{success["id"]}: xuất bản thành công.')).to_be_visible(timeout=20000)
                    expect(root.get_by_text(f'#{conflict["id"]}: 412', exact=False)).to_be_visible()
                    expect(root.get_by_text(f'#{forbidden["id"]}: 403', exact=False)).to_be_visible()
                    assert root.get_by_label(f'Chọn mục #{conflict["id"]}').is_checked()
                    assert root.get_by_label(f'Chọn mục #{forbidden["id"]}').is_checked()
                    assert not root.get_by_label(f'Chọn mục #{success["id"]}').is_checked()
                    assert api_json(ctx, 'GET', base,
                        f'/api/admin/content-types/post/entries/{success["id"]}')['publication_status'] == 'published'
                    assert api_json(ctx, 'GET', base,
                        f'/api/admin/content-types/post/entries/{forbidden["id"]}')['publication_status'] == 'draft'
                    page.unroute(blocked_url)
                    checks.append('mixed_real_200_412_and_intercepted_403_per_row_retains_only_failures')
                    page.screenshot(path=str(artifacts / 'bulk-mixed.png'), full_page=True)

                    root.get_by_role('button', name='Xóa tất cả bộ lọc').click()
                    wait_count(root, 124)
                    published = root.locator('tr[data-entry-id]').filter(has_text='Đã xuất bản')
                    expect(published.first.get_by_role('link', name=re.compile('Xem công khai'))).to_be_visible()
                    view = published.first.get_by_role('link', name=re.compile('Xem công khai'))
                    assert view.get_attribute('href').startswith('/')
                    assert view.get_attribute('target') == '_blank'
                    assert 'noopener' in view.get_attribute('rel')
                    checks.append('canonical_published_view_safe_new_tab_and_draft_without_public_link')

                    page.goto(base + '/admin/#/pages', wait_until='domcontentloaded')
                    expect(page.get_by_test_id('entry-list-page').get_by_test_id('entry-table')).to_be_visible(timeout=20000)
                    page.goto(base + '/admin/#/cpt/portfolio', wait_until='domcontentloaded')
                    expect(page.get_by_test_id('entry-list-portfolio').get_by_test_id('entry-table')).to_be_visible(timeout=20000)
                    expect(page.get_by_text('Portfolio Fixture')).to_be_visible()
                    assert page.get_by_test_id('entry-list-portfolio').get_by_role(
                        'button', name='Xuất bản đã chọn').count() == 0
                    cpt = page.get_by_test_id('entry-list-portfolio')
                    cpt.get_by_role('button', name='Bản nháp (1)').click()
                    expect(cpt.get_by_text('Portfolio Draft')).to_be_visible()
                    expect(cpt.get_by_text('Portfolio Fixture')).to_have_count(0)
                    checks.append('distinct_pages_and_cpt_server_status_without_fake_bulk_publication')

                    delayed_requests = []
                    def delay_post_response(route):
                        delayed_requests.append(route.request.url)
                        time.sleep(0.45)
                        try:
                            route.continue_()
                        except Exception:
                            # Navigating to another CPT aborts the old fetch.
                            pass
                    page.route('**/api/admin/content-types/post/entry-table*', delay_post_response)
                    page.evaluate("""() => {
                      location.hash = '#/posts?q=Late+Scope';
                      setTimeout(() => { location.hash = '#/cpt/portfolio'; }, 120);
                    }""")
                    expect(page.get_by_test_id('entry-list-portfolio')).to_be_visible(timeout=15000)
                    expect(page.get_by_test_id('entry-list-portfolio').get_by_text(
                        'Portfolio Fixture')).to_be_visible(timeout=15000)
                    page.wait_for_timeout(700)
                    assert delayed_requests, 'late post request was not exercised'
                    assert page.get_by_test_id('entry-list-post').count() == 0
                    assert page.get_by_test_id('entry-list-portfolio').get_by_text('Portfolio Fixture').count() == 1
                    page.unroute('**/api/admin/content-types/post/entry-table*')
                    checks.append('delayed_old_post_response_cannot_repaint_new_cpt_scope')

                    page.route('**/api/admin/content-types/post/entry-table*',
                               lambda route: route.fulfill(status=503,
                                   content_type='application/json',
                                   body='{"error":"temporary test failure"}'))
                    page.goto(base + '/admin/#/posts?q=This+does+not+exist', wait_until='domcontentloaded')
                    root = page.get_by_test_id('entry-list-post')
                    expect(root.get_by_role('alert')).to_be_visible(timeout=20000)
                    assert root.get_by_role('button', name='Thử lại').count() == 1
                    page.unroute('**/api/admin/content-types/post/entry-table*')
                    root.get_by_role('button', name='Thử lại').click()
                    wait_count(root, 0)
                    expect(root.get_by_text('Không tìm thấy nội dung phù hợp', exact=False)).to_be_visible()
                    checks.append('real_network_error_bounded_retry_and_empty_state')

                    page.goto(base + '/admin/#/posts?status=draft', wait_until='domcontentloaded')
                    root = table(page)
                    expect(root.get_by_role('button', name=re.compile('Bản nháp'))).to_have_attribute('aria-pressed', 'true')
                    root.get_by_label('Số mục mỗi trang').select_option('10')
                    expect(page).to_have_url(re.compile(r'limit=10'))
                    expect(root.locator('tr[data-entry-id]')).to_have_count(10)
                    checks.append('page_size_updates_url_and_server_query')
                    page.set_viewport_size({'width': 390, 'height': 844})
                    page.screenshot(path=str(artifacts / 'post-list-390.png'), full_page=True)
                    sizes = page.evaluate('({scroll:document.documentElement.scrollWidth,client:document.documentElement.clientWidth})')
                    assert sizes['scroll'] <= sizes['client'] + 2, sizes
                    assert root.get_by_test_id('entry-table').locator('th[scope="col"]').count() >= 5
                    checks.append('390px_no_root_horizontal_overflow_semantic_table_internal_scroll')

                    assert not errors, errors
                    ctx.close()
                    stop_server(server)
                    server, base = launch_server(binary, site)
                    resumed = browser.new_context()
                    admin_login(resumed, base, 'table_owner', admin_password)
                    assert resumed.request.get(base + f'/api/admin/content-types/post/entries/{target["id"]}').status == 404
                    assert api_json(resumed, 'GET', base,
                        f'/api/admin/content-types/post/entries/{old["id"]}')['publication_status'] == 'published'
                    resumed_page = resumed.new_page()
                    resumed_page.goto(base + '/admin/#/posts', wait_until='domcontentloaded')
                    wait_count(table(resumed_page), 124)
                    checks.append('native_restart_persists_versions_publication_delete_and_counts')
                    resumed.close()
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
        'status': 'FAIL',
        'source_sha': __import__('subprocess').check_output(['git','rev-parse','HEAD'], text=True).strip(),
        'binary_sha256': sha(binary),
        'checks': [],
        'isolation': 'disposable SQLite root, random loopback port; owner site 18964 untouched',
        'not_run': ['Windows native G5','Linux native G5','human dogfood','common RC/V1'],
    }
    try:
        receipt.update(run(binary, artifacts))
        receipt['status'] = 'PASS'
    except Exception as error:
        receipt['error'] = f'{type(error).__name__}: {error}'
        traceback.print_exc()
    finally:
        receipt['screenshots_sha256'] = {
            file.name: sha(file) for file in artifacts.glob('*.png')
        }
        (artifacts / 'receipt.json').write_text(json.dumps(receipt, indent=2, ensure_ascii=False) + '\n')
        print(json.dumps({key: receipt.get(key) for key in
                          ('status','source_sha','binary_sha256','checks','error')}, ensure_ascii=False))
    return 0 if receipt['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
