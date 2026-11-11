"""M4-S08-T02 real Chrome Builder List Block E2E (disposable SQLite).

Run after npm build and cargo build so the binary embeds the current Admin.
    python cms/scripts/builder_list_browser.py --binary <BINARY> --browser <CHROME>
"""
from __future__ import annotations

import argparse
import re
from pathlib import Path
from tempfile import TemporaryDirectory
from urllib.parse import urlencode

from playwright.sync_api import expect, sync_playwright

from builder_editor_browser import new_tree, running, save, state
from builder_list_smoke import create, fragment, set_publication
from content_entries_smoke import api
from content_types_smoke import login, request
from entry_terms_smoke import add_term


def new_list(page):
    page.locator('#builder-element-type').select_option('core/list')
    page.get_by_role('treeitem', name=re.compile('Column')).first.click()
    page.get_by_role('button', name='Thêm element vào Column').click()
    result = page.locator('[role="treeitem"][data-node-id^="element-"]').last.get_attribute('data-node-id')
    assert result
    return result


def verify(binary: Path, chrome: str):
    with TemporaryDirectory(prefix='opendash-m4-list-chrome-') as folder:
        db = Path(folder) / 'site' / 'database' / 'opendash.sqlite'
        with running(binary, db) as base:
            assert request(base, '/auth/setup', {
                'username': 'listadmin', 'password': 'password123',
            })[0] == 201
            admin = login(base, 'listadmin', 'password123')
            assert api(base, 'POST', '/api/admin/content-types', {
                'key': 'projects', 'label': 'Projects',
                'fields': [{'key': 'summary', 'label': 'Summary', 'type': 'short_text'}],
            }, admin)[0] == 201
            category = add_term(base, admin, 'category', 'Root', 'root')
            tag = add_term(base, admin, 'tag', 'Featured', 'featured')
            post1 = create(base, admin, 'post', 'first-post', 'First & post', [category['id'], tag['id']])
            post2 = create(base, admin, 'post', 'second-post', 'Second post', [category['id'], tag['id']])
            draft = create(base, admin, 'post', 'draft-post', 'PRIVATE DRAFT', [category['id'], tag['id']])
            set_publication(base, admin, post1, 'post', 'published')
            set_publication(base, admin, post2, 'post', 'published')
            assert draft['publication_status'] == 'draft'
            create(base, admin, 'projects', 'alpha', 'Project Alpha')
            create(base, admin, 'projects', 'beta', 'Project Beta')

            with sync_playwright() as pw:
                browser = pw.chromium.launch(executable_path=chrome)
                context = browser.new_context(viewport={'width': 1440, 'height': 900})
                page = context.new_page()
                errors = []
                page.on('pageerror', lambda error: errors.append(str(error)))
                page.goto(base + '/admin/#/builder')
                page.locator('#auth-username').fill('listadmin')
                page.locator('#auth-password').fill('password123')
                page.locator('#auth-login').click()
                expect(page.locator('[data-testid="builder-workspace"]')).to_contain_text('Đã mở')
                new_tree(page)
                first_id = new_list(page)
                expect(page.get_by_label('List content type')).to_have_value('post')
                expect(page.get_by_label('List category').locator(f'option[value="{category["id"]}"]')).to_have_count(1)
                page.get_by_label('List category').select_option(str(category['id']))
                page.get_by_label('List tag').select_option(str(tag['id']))
                page.get_by_label('List include descendants').check()
                page.get_by_label('List page size').fill('1')
                expect(page.locator(f'[data-canvas-id="{first_id}"]')).to_contain_text('Danh sách post')

                second_id = new_list(page)
                page.get_by_label('List content type').select_option('projects')
                page.get_by_label('List sort').select_option('id_asc')
                page.get_by_label('List page size').fill('1')
                page.get_by_role('treeitem', name=re.compile('Row')).first.click()
                page.get_by_label('Gap (px)').fill('0')
                page.get_by_label('Viewport Builder').select_option('mobile')
                page.get_by_label('Số cột mỗi hàng').fill('1')
                page.get_by_label('Viewport Builder').select_option('desktop')
                assert state(page, base)['version'] == 1, 'Inspector must edit only local draft'

                page.get_by_role('button', name='Xem trước').click()
                expect(page.frame_locator('#canvas').locator('.builder-list')).to_have_count(2)
                preview = page.locator('#canvas').get_attribute('srcdoc')
                assert 'Second post' in fragment(preview, first_id)
                assert 'Project Alpha' in fragment(preview, second_id)
                assert 'PRIVATE DRAFT' not in preview
                assert 'builder-list-next' not in preview, 'Preview always first page'
                assert state(page, base)['version'] == 1, 'Preview must not persist'
                page.get_by_role('button', name='Canvas cấu trúc').click()
                save(page)
                saved = state(page, base)
                assert saved['version'] == 2
                first, second = saved['document']['sections'][0]['children'][0]['children'][0]['children'][-2:]
                assert first['id'] == first_id and second['id'] == second_id
                assert first['props'] == {
                    'type_key': 'post', 'page_size': 1, 'sort': 'id_desc',
                    'category_id': category['id'], 'tag_id': tag['id'], 'include_descendants': True,
                }, first['props']
                assert second['props'] == {
                    'type_key': 'projects', 'page_size': 1,
                    'sort': 'id_asc', 'include_descendants': False,
                }, second['props']
                row = saved['document']['sections'][0]['children'][0]
                assert row['responsive']['desktop']['gap_px'] == 0
                assert row['responsive']['mobile']['columns_per_row'] == 1

                # Truly independent public pagination (node IDs are generated by
                # the Browser, not special-cased on the server).
                url = '/?' + urlencode({f'list_page_{first_id}': 2, f'list_page_{second_id}': 2})
                public = page.request.get(base + url)
                assert public.status == 200, public.text()
                assert 'First &amp; post' in fragment(public.text(), first_id)
                assert 'Project Beta' in fragment(public.text(), second_id)
                assert 'PRIVATE DRAFT' not in public.text()
                assert 'list_page_' + second_id in fragment(public.text(), first_id)
                assert 'list_page_' + first_id in fragment(public.text(), second_id)

                page.reload()
                expect(page.locator('[data-testid="builder-workspace"]')).to_contain_text('Đã mở')
                page.get_by_role('treeitem', name=re.compile(re.escape(first_id))).click()
                expect(page.get_by_label('List category')).to_have_value(str(category['id']))
                expect(page.get_by_label('List tag')).to_have_value(str(tag['id']))
                expect(page.get_by_label('List include descendants')).to_be_checked()
                expect(page.get_by_label('List page size')).to_have_value('1')
                page.get_by_role('treeitem', name=re.compile(re.escape(second_id))).click()
                expect(page.get_by_label('List content type')).to_have_value('projects')
                expect(page.get_by_label('List sort')).to_have_value('id_asc')

                # An unavailable CPT survives Inspector/Save and remains
                # editable: the lookup only changes visual guidance.
                page.get_by_role('treeitem', name=re.compile(re.escape(first_id))).click()
                doc = state(page, base)['document']
                doc['sections'][0]['children'][0]['children'][0]['children'][-2]['props']['type_key'] = 'deleted_provider'
                csrf = page.request.get(base + '/auth/me').json()['csrf_token']
                res = page.request.put(base + '/api/admin/builder/homepage', headers={
                    'X-CSRF-Token': csrf, 'If-Match': '"v2"',
                }, data=doc)
                assert res.status == 200, res.text()
                page.get_by_role('button', name='Tải bản server').click()
                page.get_by_role('treeitem', name=re.compile(re.escape(first_id))).click()
                expect(page.get_by_label('List content type')).to_have_value('deleted_provider')
                expect(page.get_by_text('CPT hoặc category/tag tham chiếu không còn tồn tại.')).to_be_visible()
                page.get_by_label('List sort').select_option('id_asc')
                save(page)
                persisted = state(page, base)['document']
                changed = persisted['sections'][0]['children'][0]['children'][0]['children'][-2]['props']
                assert changed['type_key'] == 'deleted_provider' and changed['sort'] == 'id_asc'
                assert 'Danh sách chưa khả dụng' in page.request.get(base + '/').text()
                # The UI must reject a ninth List before Save, independently
                # of the server's typed validator.
                for _ in range(6):
                    new_list(page)
                list_count = page.locator('[role="treeitem"][data-node-id^="element-"]').count()
                assert list_count == 9, 'one heading plus exactly eight list nodes'
                page.get_by_role('treeitem', name=re.compile('Column')).first.click()
                page.get_by_role('button', name='Thêm element vào Column').click()
                assert page.locator('[role="treeitem"][data-node-id^="element-"]').count() == list_count
                expect(page.locator('[data-testid="builder-workspace"]')).to_contain_text('Vị trí thêm node không hợp lệ')
                assert not errors, errors
                browser.close()

    print('PASS #106 Chrome: Inspector Post/CPT/category/tag/sort/size, responsive/binding-safe draft, Preview first, independent public pagination, Save/reopen/ETag, missing CPT round-trip')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--browser', required=True)
    args = parser.parse_args()
    verify(args.binary.resolve(), args.browser)
