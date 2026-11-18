#!/usr/bin/env python3
"""#375 custom taxonomy Admin, Builder, DB and public acceptance on an isolated site.

Usage: python3 cms/scripts/content_foundation_m4_s12_t02_e2e.py
    --binary cms/target/debug/opendash --artifact-dir /tmp/c2-t02-evidence
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import platform
import re
import sqlite3
import subprocess
import tempfile
import traceback

from playwright.sync_api import sync_playwright
from m7_beta_menu_picker_e2e import admin_login, launch_server, stop_server
from content_foundation_m4_s12_t01_e2e import api, fetch


def sha(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check(condition, explanation):
    assert condition, explanation


def run(binary: pathlib.Path, artifacts: pathlib.Path) -> dict:
    checks = []
    # macOS /tmp resolves through /var (a symlink). OpenDash deliberately
    # rejects symlink traversal in the site root, so isolate under artifacts.
    with tempfile.TemporaryDirectory(prefix='opendash-c2-t02-', dir=artifacts) as temp:
        site = pathlib.Path(temp) / 'site'
        site.mkdir(mode=0o700)
        server, base = launch_server(binary, site)
        assert base.startswith('http://127.0.0.1:') and ':18964' not in base
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel='chrome', headless=True)
                chrome_version = browser.version
                try:
                    ctx = browser.new_context(viewport={'width': 1280, 'height': 800})
                    api(ctx, base, 'POST', '/auth/setup', 201,
                        {'username': 'c2_t02_admin', 'password': 'Isolated-Only-C2-T02-Pass!2026'})
                    csrf = admin_login(ctx, base, 'c2_t02_admin', 'Isolated-Only-C2-T02-Pass!2026')
                    initial_site_id = api(ctx, base, 'GET', '/auth/me')['site_instance_id']
                    check(initial_site_id and initial_site_id != 'default-site',
                          'site instance ID is available for scoped draft recovery')
                    # Even two installations set up with the same username,
                    # site title and first-admin second must have different
                    # draft scopes. Neither uses the owner's review site.
                    sibling = pathlib.Path(temp) / 'another-site'
                    sibling.mkdir(mode=0o700)
                    sibling_server, sibling_base = launch_server(binary, sibling)
                    try:
                        sibling_ctx = browser.new_context()
                        try:
                            api(sibling_ctx, sibling_base, 'POST', '/auth/setup', 201,
                                {'username':'c2_t02_admin', 'password':'Isolated-Only-C2-T02-Pass!2026'})
                            admin_login(sibling_ctx, sibling_base, 'c2_t02_admin',
                                        'Isolated-Only-C2-T02-Pass!2026')
                            with sqlite3.connect(site / 'database/opendash.sqlite') as conn:
                                original_created = conn.execute('SELECT created_at FROM users WHERE id=1').fetchone()[0]
                            with sqlite3.connect(sibling / 'database/opendash.sqlite') as conn:
                                conn.execute('UPDATE users SET created_at=? WHERE id=1', (original_created,))
                            sibling_id = api(sibling_ctx, sibling_base, 'GET', '/auth/me')['site_instance_id']
                            check(sibling_id and sibling_id != initial_site_id,
                                  'separate DB roots cannot share a draft site identifier')
                            sibling_csrf = api(sibling_ctx, sibling_base, 'GET', '/auth/me')['csrf_token']
                            api(sibling_ctx, sibling_base, 'POST', '/api/admin/users', 201,
                                {'username': 'c2_t02_second_admin',
                                 'password': 'Isolated-Second-Admin!2026', 'role': 'admin'}, sibling_csrf)
                            sibling_second_ctx = browser.new_context()
                            try:
                                admin_login(sibling_second_ctx, sibling_base, 'c2_t02_second_admin',
                                            'Isolated-Second-Admin!2026')
                                with sqlite3.connect(sibling / 'database/opendash.sqlite') as conn:
                                    conn.execute('DELETE FROM users WHERE id=1')
                                check(api(sibling_second_ctx, sibling_base, 'GET', '/auth/me')
                                      ['site_instance_id'] == sibling_id,
                                      'deleting original Admin cannot change site identity')
                            finally:
                                sibling_second_ctx.close()
                        finally:
                            sibling_ctx.close()
                    finally:
                        stop_server(sibling_server)
                    checks.append('distinct_site_roots_and_deleted_first_admin_keep_stable_site_ids')
                    genre = api(ctx, base, 'POST', '/api/admin/taxonomy/definitions', 201,
                        {'key': 'genre_t02', 'label': 'Genre T02', 'hierarchical': True}, csrf)
                    flat = api(ctx, base, 'POST', '/api/admin/taxonomy/definitions', 201,
                        {'key': 'label_t02', 'label': 'Label T02', 'hierarchical': False}, csrf)
                    check(genre['hierarchical'] and not flat['hierarchical'], 'hierarchy metadata')
                    checks.append('definitions_respect_hierarchical_flag')

                    root = api(ctx, base, 'POST', '/api/admin/taxonomy/terms', 201,
                        {'kind': 'genre_t02', 'name': 'Literature', 'slug': 'literature'}, csrf)
                    child = api(ctx, base, 'POST', '/api/admin/taxonomy/terms', 201,
                        {'kind': 'genre_t02', 'name': 'Speculative', 'slug': 'speculative',
                         'parent_id': root['id']}, csrf)
                    fetch(ctx, base, 'POST', '/api/admin/taxonomy/terms', 400,
                        {'kind': 'label_t02', 'name': 'Bad parent', 'slug': 'bad-parent',
                         'parent_id': root['id']}, csrf)
                    checks.append('term_hierarchy_parent_and_cross_taxonomy_rejection')

                    book = api(ctx, base, 'POST', '/api/admin/content-types', 201,
                        {'key': 'books_t02', 'label': 'Books T02', 'fields': []}, csrf)
                    check(api(ctx, base, 'GET', '/api/admin/taxonomy/eligibility/page/effective') == [],
                          'Page defaults to no taxonomy')
                    book = api(ctx, base, 'PUT', '/api/admin/taxonomy/eligibility/books_t02', 200,
                        {'taxonomies': ['genre_t02'], 'expected_version': book['version']},
                        csrf, book['version'])
                    check(api(ctx, base, 'GET', '/api/admin/taxonomy/eligibility/books_t02/effective')
                          == ['genre_t02'], 'effective taxonomy override')
                    configured = api(ctx, base, 'GET',
                        '/api/admin/taxonomy/eligibility/books_t02/configured')
                    check(configured['taxonomies'] == ['genre_t02']
                          and configured['has_override'] and configured['effective'] == ['genre_t02']
                          and configured['type_version'] == book['version'],
                          'configured keys and CPT CAS version come from one snapshot')
                    fetch(ctx, base, 'PUT', '/api/admin/taxonomy/eligibility/books_t02', 412,
                        {'taxonomies': ['label_t02'], 'expected_version': book['version']-1},
                        csrf, book['version']-1)
                    checks.append('cpt_eligibility_cas_and_page_default')

                    tagged = api(ctx, base, 'POST', '/api/admin/content-types/books_t02/entries', 201,
                        {'title': 'Speculative C2 Visible', 'slug': 'visible', 'values': {},
                         'term_ids': [child['id']]}, csrf)
                    other = api(ctx, base, 'POST', '/api/admin/content-types/books_t02/entries', 201,
                        {'title': 'Unrelated C2 Hidden', 'slug': 'hidden', 'values': {}}, csrf)
                    entry_table = api(ctx, base, 'GET',
                        f'/api/admin/content-types/books_t02/entry-table?taxonomy_key=genre_t02&term_id={child["id"]}')
                    check(entry_table['total'] == 1 and entry_table['items'][0]['id'] == tagged['id'],
                          'admin table generic taxonomy filter')
                    root_table = api(ctx, base, 'GET',
                        f'/api/admin/content-types/books_t02/entry-table?taxonomy_key=genre_t02&term_id={root["id"]}&include_descendants=true')
                    check(root_table['total'] == 1, 'descendant filtering')
                    fetch(ctx, base, 'GET',
                        f'/api/admin/content-types/books_t02/entry-table?taxonomy_key=genre_t02', 400)
                    fetch(ctx, base, 'GET',
                        f'/api/admin/content-types/books_t02/entry-table?taxonomy_key=genre_t02&term_id={child["id"]}&category_id=1', 400)
                    checks.append('generic_filter_exact_descendant_half_pair_and_alias_collision')

                    home = api(ctx, base, 'GET', '/api/admin/builder/homepage')
                    doc = {'schema_version': 1, 'sections': [
                        {'id':'t02-section', 'children': [
                            {'id':'t02-row', 'children': [
                                {'id':'t02-column', 'children': [
                                    {'id':'t02-list', 'type':'core/list', 'props': {
                                        'type_key':'books_t02', 'taxonomy_key':'genre_t02',
                                        'term_id':root['id'], 'include_descendants':True,
                                        'page_size':6, 'sort':'id_desc'}}
                                ]}
                            ]}
                        ]}
                    ]}
                    api(ctx, base, 'PUT', '/api/admin/builder/homepage', 200,
                        doc, csrf, home['version'])
                    html = fetch(ctx, base, 'GET', '/', 200).text()
                    check('Speculative C2 Visible' in html and 'Unrelated C2 Hidden' not in html,
                          'public Builder list filters exact custom taxonomy')
                    checks.append('builder_generic_taxonomy_filters_published_public')

                    # Simulate a provider lifecycle transition in this disposable
                    # database. Effective access must disappear, while the full
                    # saved association remains editable without data loss.
                    db = site / 'database/opendash.sqlite'
                    with sqlite3.connect(db) as conn:
                        conn.execute("UPDATE taxonomy_definitions SET provider_status='provider_inactive' WHERE key='genre_t02'")
                    dormant = api(ctx, base, 'GET',
                        '/api/admin/taxonomy/eligibility/books_t02/configured')
                    check(dormant['taxonomies'] == ['genre_t02'] and dormant['effective'] == [],
                          'inactive association still configured')
                    check('Speculative C2 Visible' not in fetch(ctx, base, 'GET', '/', 200).text(),
                          'dormant filter cannot publish saved association')
                    book = api(ctx, base, 'PUT', '/api/admin/taxonomy/eligibility/books_t02', 200,
                        {'taxonomies':['genre_t02', 'label_t02'], 'expected_version':book['version']},
                        csrf, book['version'])
                    with sqlite3.connect(db) as conn:
                        conn.execute("UPDATE taxonomy_definitions SET provider_status='active' WHERE key='genre_t02'")
                    restored = api(ctx, base, 'GET',
                        '/api/admin/taxonomy/eligibility/books_t02/configured')
                    check(restored['taxonomies'] == ['genre_t02', 'label_t02']
                          and restored['effective'] == ['genre_t02', 'label_t02'],
                          'independent eligibility edit preserves dormant association')
                    check('Speculative C2 Visible' in fetch(ctx, base, 'GET', '/', 200).text(),
                          'provider reactivation restores public Builder list')
                    checks.append('provider_inactive_full_selection_preserved_and_reactivated')

                    # Aster P2: visible taxonomy columns must not show '—'
                    # because the table only fetched the currently filtered
                    # kind. Verify both custom memberships with and without a
                    # genre-only filter while label remains a visible column.
                    label_term = api(ctx, base, 'POST', '/api/admin/taxonomy/terms', 201,
                        {'kind':'label_t02', 'name':'Featured Label T02', 'slug':'featured-label-t02'}, csrf)
                    multi = api(ctx, base, 'POST', '/api/admin/content-types/books_t02/entries', 201,
                        {'title':'Multi Taxonomy C2 Visible', 'slug':'multi-taxonomy-c2',
                         'values':{}, 'term_ids':[child['id'], label_term['id']]}, csrf)
                    table_page = ctx.new_page()
                    try:
                        term_requests = []
                        table_page.on('request', lambda request: term_requests.append(request.url)
                            if '/api/admin/taxonomy/terms' in request.url else None)
                        table_page.goto(base + '/admin/#/cpt/books_t02', wait_until='domcontentloaded')
                        multi_row = table_page.locator(f'tr[data-entry-id="{multi["id"]}"]')
                        multi_row.get_by_role('cell', name='Speculative', exact=True).wait_for(timeout=15000)
                        multi_row.get_by_role('cell', name='Featured Label T02', exact=True).wait_for(timeout=15000)
                        projected = [url for url in term_requests if '/terms/resolve?' in url]
                        full_scans = [url for url in term_requests if '/terms?' in url]
                        check(1 <= len(projected) <= 2 and not full_scans,
                              f'entry table must project current row IDs, not full taxonomy catalogs: {term_requests}')
                        table_page.get_by_role('combobox', name='Chọn taxonomy để lọc').select_option('genre_t02')
                        table_page.get_by_role('combobox', name='Lọc theo Genre T02').focus()
                        table_page.get_by_role('combobox', name='Lọc theo Genre T02') \
                            .locator(f'option[value="{child["id"]}"]').wait_for(state='attached', timeout=15000)
                        table_page.get_by_role('combobox', name='Lọc theo Genre T02').select_option(str(child['id']))
                        multi_row.get_by_role('cell', name='Speculative', exact=True).wait_for(timeout=15000)
                        multi_row.get_by_role('cell', name='Featured Label T02', exact=True).wait_for(timeout=15000)
                    finally:
                        table_page.close()
                    checks.append('chrome_entry_table_resolves_all_custom_column_names_with_or_without_filter')

                    # A valid Admin hierarchy change must not make a saved
                    # Builder list abort the public homepage with HTTP 400.
                    # Detach the child first; flat taxonomies cannot have
                    # parent links. User-provided descendant queries remain 400.
                    child = api(ctx, base, 'PATCH',
                        f'/api/admin/taxonomy/terms/{child["id"]}', 200,
                        {'parent_id': None}, csrf, child['version'])
                    genre = api(ctx, base, 'PATCH',
                        '/api/admin/taxonomy/definitions/genre_t02', 200,
                        {'hierarchical': False}, csrf, genre['version'])
                    html = fetch(ctx, base, 'GET', '/', 200).text()
                    check('Speculative C2 Visible' not in html,
                          'incompatible saved Builder descendants filter renders empty')
                    fetch(ctx, base, 'GET',
                        f'/api/admin/content-types/books_t02/entry-table?taxonomy_key=genre_t02&term_id={root["id"]}&include_descendants=true', 400)
                    genre = api(ctx, base, 'PATCH',
                        '/api/admin/taxonomy/definitions/genre_t02', 200,
                        {'hierarchical': True}, csrf, genre['version'])
                    child = api(ctx, base, 'PATCH',
                        f'/api/admin/taxonomy/terms/{child["id"]}', 200,
                        {'parent_id': root['id']}, csrf, child['version'])
                    check('Speculative C2 Visible' in fetch(ctx, base, 'GET', '/', 200).text(),
                          'return to hierarchy restores saved public Builder list')
                    checks.append('hierarchical_to_flat_builder_missing_config_and_strict_api_400')

                    admin = ctx.new_page()
                    admin.goto(base + '/admin/#/taxonomy/definitions', wait_until='domcontentloaded')
                    admin.get_by_text('Genre T02').first.wait_for(timeout=15000)
                    admin.screenshot(path=str(artifacts / 'taxonomy-definitions-desktop.png'), full_page=True)
                    default_cpt = api(ctx, base, 'POST', '/api/admin/content-types', 201,
                                      {'key':'default_t02', 'label':'Default T02', 'fields':[]}, csrf)
                    with sqlite3.connect(db) as conn:
                        conn.execute("INSERT INTO content_type_taxonomy_eligibility"
                                     "(content_type_key,taxonomy_kind,owner_kind,owner_id)"
                                     " VALUES('default_t02','genre_t02','user',NULL)")
                        conn.execute("UPDATE taxonomy_definitions SET provider_status='provider_inactive'"
                                     " WHERE key='genre_t02'")
                    configured = api(ctx, base, 'GET', '/api/admin/taxonomy/eligibility/default_t02/configured')
                    check(not configured['has_override'] and configured['taxonomies'] == ['genre_t02']
                          and configured['effective'] == [], 'dormant provider default exists without site override')
                    # The CPT is created externally after the SPA first
                    # loaded; refresh its list before selecting the new key.
                    admin.get_by_role('button', name='Tải lại', exact=True).click()
                    admin.locator('select').filter(has=admin.locator('option[value="default_t02"]')).wait_for(timeout=15000)
                    admin.get_by_label('Loại nội dung').select_option('default_t02')
                    dormant_checkbox = admin.get_by_role('checkbox', name=re.compile(r'Genre T02 \(genre_t02\)'))
                    dormant_checkbox.wait_for(timeout=15000)
                    check(dormant_checkbox.is_checked(), 'UI retains dormant default as selected')
                    admin.get_by_role('checkbox', name=re.compile(r'Label T02 \(label_t02\)')).check()
                    with admin.expect_response(lambda response: response.request.method == 'PUT'
                            and '/api/admin/taxonomy/eligibility/default_t02' in response.url,
                            timeout=15000) as default_save:
                        admin.get_by_role('button', name='Lưu loại được áp dụng').click()
                    check(default_save.value.status == 200,
                          'first provider-default override saves with matching snapshot CAS')
                    saved = api(ctx, base, 'GET', '/api/admin/taxonomy/eligibility/default_t02/configured')
                    check(saved['has_override'] and saved['taxonomies'] == ['genre_t02','label_t02'],
                          'UI save keeps dormant default on first override')
                    with sqlite3.connect(db) as conn:
                        conn.execute("UPDATE taxonomy_definitions SET provider_status='active' WHERE key='genre_t02'")
                    check(api(ctx, base, 'GET', '/api/admin/taxonomy/eligibility/default_t02/effective')
                          == ['genre_t02','label_t02'], 'reactivation restores default on site override')
                    checks.append('chrome_first_override_preserves_inactive_provider_default')

                    # Aster P2: force the exact invalid response ordering in
                    # DefinitionsPage::rebaseEligibility. The configured GET
                    # returns pre-write keys/vN while listContentTypes sees
                    # post-write vN+1. UI must NOT attach vN+1 to old keys.
                    old_config = api(ctx, base, 'GET',
                        '/api/admin/taxonomy/eligibility/default_t02/configured')
                    default_version = next(t['version'] for t in
                        api(ctx, base, 'GET', '/api/admin/content-types')
                        if t['descriptor']['key'] == 'default_t02')
                    check(old_config['type_version'] == default_version,
                          'configured response includes matching CPT version')
                    definitions_label = admin.get_by_role('checkbox',
                        name=re.compile(r'Label T02 \(label_t02\)'))
                    definitions_label.uncheck()
                    default_cpt = api(ctx, base, 'PUT',
                        '/api/admin/taxonomy/eligibility/default_t02', 200,
                        {'taxonomies':['genre_t02','label_t02','tag'],
                         'expected_version':default_version}, csrf, default_version)
                    admin.get_by_role('button', name='Lưu loại được áp dụng').click()
                    rebase_btn = admin.get_by_role('button',
                        name='Đối chiếu schema mới, giữ lựa chọn đã sửa')
                    rebase_btn.wait_for(timeout=15000)

                    def stale_configured(route):
                        route.fulfill(status=200, content_type='application/json',
                                      body=json.dumps(old_config))
                    old_url = '**/api/admin/taxonomy/eligibility/default_t02/configured'
                    admin.route(old_url, stale_configured)
                    try:
                        rebase_btn.click()
                        admin.get_by_text('Version trong danh sách CPT không khớp snapshot taxonomy', exact=False) \
                            .wait_for(timeout=15000)
                        check(not definitions_label.is_checked()
                              and admin.get_by_role('button', name='Lưu loại được áp dụng').is_disabled(),
                              'stale configured keys plus newer CPT version cannot enable CAS')
                    finally:
                        admin.unroute(old_url, stale_configured)
                    rebase_btn.click()
                    admin.get_by_text('Đã đối chiếu cấu hình taxonomy mới', exact=False) \
                        .wait_for(timeout=15000)
                    tag_def = admin.get_by_role('checkbox', name=re.compile(r'Tag \(tag\)'))
                    check(not definitions_label.is_checked() and tag_def.is_checked(),
                          'rebase reapplies only local removal over concurrent tag addition')
                    with admin.expect_response(lambda response: response.request.method == 'PUT'
                            and '/api/admin/taxonomy/eligibility/default_t02' in response.url,
                            timeout=15000) as save_response:
                        admin.get_by_role('button', name='Lưu loại được áp dụng').click()
                    check(save_response.value.status == 200, 'rebase uses matching CAS version')
                    final_assoc = api(ctx, base, 'GET',
                        '/api/admin/taxonomy/eligibility/default_t02/configured')
                    check(final_assoc['taxonomies'] == ['genre_t02','tag']
                          and final_assoc['type_version'] == default_cpt['version'] + 1,
                          'concurrent unrelated taxonomy association survives rebase/save')
                    checks.append('chrome_definitions_controlled_out_of_order_cas_snapshot_rebase')

                    # Aster P2: Core Category/Tag have fixed keys and hierarchy
                    # but their display label/public flag may be edited by
                    # Admin, not deleted or have hierarchy mutated.
                    core_category = api(ctx, base, 'GET',
                        '/api/admin/taxonomy/definitions/category')
                    core_row = admin.locator('div.rounded-lg.border.p-3').filter(
                        has=admin.locator('span', has_text=re.compile(r'^\(category\)$')))
                    core_row.get_by_role('button', name='Cấu hình').click()
                    core_label = admin.get_by_role('textbox', name=re.compile('Tên hiển thị'))
                    core_public = admin.get_by_role('checkbox', name='Cho phép hiển thị công khai')
                    check(admin.get_by_role('checkbox', name=re.compile('Cho phép phân cấp')).is_disabled(),
                          'Core hierarchy stays immutable')
                    core_label.fill('Category C2 Edited')
                    if core_category['public']:
                        core_public.uncheck()
                    else:
                        core_public.check()
                    with admin.expect_response(lambda response: response.request.method == 'PATCH'
                            and '/api/admin/taxonomy/definitions/category' in response.url,
                            timeout=15000) as core_edit_response:
                        admin.get_by_role('button', name='Lưu định nghĩa').click()
                    check(core_edit_response.value.status == 200,
                          'Admin can save Core category label/public')
                    changed_category = api(ctx, base, 'GET',
                        '/api/admin/taxonomy/definitions/category')
                    check(changed_category['label'] == 'Category C2 Edited'
                          and changed_category['public'] != core_category['public']
                          and changed_category['hierarchical'] == core_category['hierarchical']
                          and changed_category['key'] == 'category',
                          'Core writable fields changed without key/hierarchy mutation')
                    api(ctx, base, 'PATCH', '/api/admin/taxonomy/definitions/category', 200,
                        {'label':core_category['label'], 'public':core_category['public']},
                        csrf, changed_category['version'])
                    checks.append('chrome_core_category_label_public_edit_key_hierarchy_immutable')
                    api(ctx, base, 'POST', '/api/admin/users', 201,
                        {'username':'c2_t02_other', 'password':'Isolated-Other-C2-Admin!2026',
                         'role':'admin'}, csrf)
                    # Expiring a session from the HTTP side must preserve a
                    # form in the mounted SPA, then restore it for the SAME
                    # account and site after signing in again; another admin
                    # must not see or erase the original user's RAM draft.
                    admin.get_by_role('button', name='Thêm taxonomy').click()
                    draft = admin.locator('form').filter(has=admin.get_by_role('heading', name='Tạo định nghĩa'))
                    draft.locator('input').nth(0).fill('draft_t02')
                    draft.locator('input').nth(1).fill('Taxonomy Draft C2')
                    with sqlite3.connect(db) as conn:
                        conn.execute("UPDATE site_general_settings SET site_title='Renamed C2 Site' WHERE id=1")
                    check(api(ctx, base, 'GET', '/auth/me')['site_instance_id'] == initial_site_id,
                          'renaming same site cannot change RAM draft identity')
                    fetch(ctx, base, 'POST', '/auth/logout', 204, csrf=csrf)
                    fetch(ctx, base, 'GET', '/auth/me', 401)
                    admin.get_by_role('button', name='Lưu định nghĩa').click()
                    admin.locator('#auth-login').wait_for(timeout=15000)
                    admin.locator('#auth-username').fill('c2_t02_other')
                    admin.locator('#auth-password').fill('Isolated-Other-C2-Admin!2026')
                    admin.locator('#auth-login').click()
                    admin.get_by_role('button', name='Thêm taxonomy').wait_for(timeout=15000)
                    check(admin.get_by_role('heading', name='Tạo định nghĩa').count() == 0,
                          'other admin does not see original author draft')
                    other_csrf = api(ctx, base, 'GET', '/auth/me')['csrf_token']
                    fetch(ctx, base, 'POST', '/auth/logout', 204, csrf=other_csrf)
                    admin.get_by_role('button', name='Tải lại', exact=True).click()
                    admin.locator('#auth-login').wait_for(timeout=15000)
                    admin.locator('#auth-username').fill('c2_t02_admin')
                    admin.locator('#auth-password').fill('Isolated-Only-C2-T02-Pass!2026')
                    admin.locator('#auth-login').click()
                    draft = admin.locator('form').filter(has=admin.get_by_role('heading', name='Tạo định nghĩa'))
                    draft.locator('input').nth(0).wait_for(timeout=15000)
                    check(draft.locator('input').nth(0).input_value() == 'draft_t02'
                          and draft.locator('input').nth(1).input_value() == 'Taxonomy Draft C2',
                          'same-site and same-account taxonomy draft restored after cross-account 401')
                    csrf = api(ctx, base, 'GET', '/auth/me')['csrf_token']
                    admin.once('dialog', lambda dialog: dialog.accept())
                    admin.get_by_role('button', name='Tải lại', exact=True).click()
                    check(admin.get_by_role('heading', name='Tạo định nghĩa').count() == 0,
                          'explicit reload discards recovered draft')
                    checks.append('chrome_taxonomy_draft_401_cross_account_isolation_restore_and_discard')
                    checks.append('site_identity_stable_across_title_change_and_401')
                    admin.goto(base + '/admin/#/taxonomy/genre_t02/terms', wait_until='domcontentloaded')
                    admin.get_by_text('Literature').first.wait_for(timeout=15000)
                    admin.screenshot(path=str(artifacts / 'taxonomy-terms-desktop.png'), full_page=True)
                    checks.append('chrome_admin_definition_and_term_routes')

                    admin.goto(base + f'/admin/#/taxonomy/genre_t02/terms/{child["id"]}/edit',
                               wait_until='domcontentloaded')
                    admin.locator('#term-name').wait_for(timeout=15000)
                    admin.locator('#term-name').fill('Speculative Unsaved T02')
                    fetch(ctx, base, 'POST', '/auth/logout', 204, csrf=csrf)
                    admin.get_by_role('button', name='Lưu thay đổi').click()
                    admin.locator('#auth-login').wait_for(timeout=15000)
                    admin.locator('#auth-username').fill('c2_t02_admin')
                    admin.locator('#auth-password').fill('Isolated-Only-C2-T02-Pass!2026')
                    admin.locator('#auth-login').click()
                    admin.locator('#term-name').wait_for(timeout=15000)
                    check(admin.locator('#term-name').input_value() == 'Speculative Unsaved T02',
                          'term edit draft preserved after same-site and same-user 401')
                    csrf = api(ctx, base, 'GET', '/auth/me')['csrf_token']
                    admin.once('dialog', lambda dialog: dialog.accept())
                    admin.get_by_role('button', name='Hủy sửa').click()
                    checks.append('chrome_term_editor_401_restore_and_discard')

                    # CPT Settings is a separate mounted screen from the
                    # taxonomy definitions editor. Its pending eligibility
                    # choices must survive 401 for the same account and site.
                    admin.goto(base + '/admin/#/appearance/content-types/books_t02',
                               wait_until='domcontentloaded')
                    taxonomy_settings = admin.get_by_role('region', name='Taxonomy áp dụng cho loại nội dung')
                    label_choice = taxonomy_settings.get_by_role('checkbox', name=re.compile('Label T02'))
                    label_choice.wait_for(timeout=15000)
                    check(label_choice.is_checked(), 'CPT starts with custom label association')
                    label_choice.uncheck()
                    fetch(ctx, base, 'POST', '/auth/logout', 204, csrf=csrf)
                    admin.get_by_role('button', name='Lưu taxonomy áp dụng').click()
                    admin.locator('#auth-login').wait_for(timeout=15000)
                    admin.locator('#auth-username').fill('c2_t02_admin')
                    admin.locator('#auth-password').fill('Isolated-Only-C2-T02-Pass!2026')
                    admin.locator('#auth-login').click()
                    label_choice = admin.get_by_role('region', name='Taxonomy áp dụng cho loại nội dung') \
                        .get_by_role('checkbox', name=re.compile('Label T02'))
                    label_choice.wait_for(timeout=15000)
                    check(not label_choice.is_checked(), 'CPT taxonomy checkbox recovered after same-account 401')
                    csrf = api(ctx, base, 'GET', '/auth/me')['csrf_token']
                    admin.get_by_role('button', name='Hủy lựa chọn chưa lưu').click()
                    check(label_choice.is_checked(), 'explicit CPT draft discard reloads saved selection')
                    checks.append('chrome_cpt_taxonomy_draft_401_restores_and_explicit_discard')

                    # Aster follow-up P2: the *new* CPT creation form previously
                    # dropped incomplete fields on route changes and 401. This
                    # is distinct from the taxonomy association edit above.
                    admin.get_by_role('button', name='+ Tạo loại nội dung mới').click()
                    admin.locator('#content-type-key').fill('unsaved_cpt_t02')
                    admin.locator('#content-type-label').fill('CPT Draft Persist T02')
                    admin.get_by_role('button', name='+ Thêm field').click()
                    admin.locator('[id^="schema-field-key-"]').first.fill('unsaved_field')
                    admin.locator('[id^="schema-field-label-"]').first.fill('Field Draft T02')
                    admin.get_by_text('Bản nhập CPT chưa lưu được giữ', exact=False).wait_for(timeout=15000)
                    admin.once('dialog', lambda dialog: dialog.dismiss())
                    admin.evaluate("window.location.hash = '#/dashboard'")
                    check(admin.locator('#content-type-key').input_value() == 'unsaved_cpt_t02',
                          'dirty create CPT route guard preserves entered key')
                    fetch(ctx, base, 'POST', '/auth/logout', 204, csrf=csrf)
                    admin.get_by_role('button', name='Tạo loại nội dung', exact=True).click()
                    admin.locator('#auth-login').wait_for(timeout=15000)
                    admin.locator('#auth-username').fill('c2_t02_other')
                    admin.locator('#auth-password').fill('Isolated-Other-C2-Admin!2026')
                    admin.locator('#auth-login').click()
                    admin.get_by_role('button', name='+ Tạo loại nội dung mới').wait_for(timeout=15000)
                    check(admin.locator('#content-type-key').count() == 0,
                          'other Admin never inherits previous account CPT creation draft')
                    other_csrf = api(ctx, base, 'GET', '/auth/me')['csrf_token']
                    fetch(ctx, base, 'POST', '/auth/logout', 204, csrf=other_csrf)
                    admin.get_by_role('button', name='Tải lại từ máy chủ (bỏ chỉnh sửa)').click()
                    admin.locator('#auth-login').wait_for(timeout=15000)
                    admin.locator('#auth-username').fill('c2_t02_admin')
                    admin.locator('#auth-password').fill('Isolated-Only-C2-T02-Pass!2026')
                    admin.locator('#auth-login').click()
                    admin.locator('#content-type-key').wait_for(timeout=15000)
                    check(admin.locator('#content-type-key').input_value() == 'unsaved_cpt_t02'
                          and admin.locator('#content-type-label').input_value() == 'CPT Draft Persist T02'
                          and admin.locator('[id^="schema-field-key-"]').first.input_value() == 'unsaved_field'
                          and admin.locator('[id^="schema-field-label-"]').first.input_value() == 'Field Draft T02',
                          'same-account reauth restores full incomplete CPT schema draft')
                    admin.once('dialog', lambda dialog: dialog.accept())
                    admin.get_by_role('button', name='Đóng form / bỏ bản nhập chưa lưu').click()
                    check(admin.locator('#content-type-key').count() == 0,
                          'explicit CPT draft discard closes create form')
                    # Successful creation must not trigger a stale App dirty
                    # prompt *after* POST 201. The newly-created CPT must be
                    # selected automatically, exactly once.
                    admin.get_by_role('button', name='+ Tạo loại nội dung mới').click()
                    admin.locator('#content-type-key').fill('created_cpt_t02')
                    admin.locator('#content-type-label').fill('Created CPT T02')
                    with admin.expect_response(lambda response: response.request.method == 'POST'
                            and response.url.endswith('/api/admin/content-types'), timeout=15000) as created_response:
                        admin.get_by_role('button', name='Tạo loại nội dung', exact=True).click()
                    check(created_response.value.status == 201, 'creating CPT must succeed once')
                    admin.get_by_text('Thông tin Schema: Created CPT T02').wait_for(timeout=15000)
                    check(admin.url.endswith('#/appearance/content-types/created_cpt_t02'),
                          'committed CPT navigates without stale create-dirty prompt')
                    admin.goto(base + '/admin/#/appearance/content-types/books_t02',
                               wait_until='domcontentloaded')
                    csrf = api(ctx, base, 'GET', '/auth/me')['csrf_token']
                    checks.append('chrome_new_cpt_draft_navigation_401_cross_account_and_discard')
                    checks.append('chrome_create_cpt_201_navigates_without_post_commit_dirty_prompt')

                    # Hold the create response until another settings input
                    # becomes dirty. Once POST commits, the handler must not
                    # bypass App.mayLeave and discard this *newer* edit.
                    held_create = []
                    create_route = '**/api/admin/content-types'
                    def hold_create_post(route):
                        if route.request.method == 'POST':
                            held_create.append(route)
                        else:
                            route.continue_()
                    admin.route(create_route, hold_create_post)
                    try:
                        admin.get_by_role('button', name='+ Tạo loại nội dung mới').click()
                        admin.locator('#content-type-key').fill('late_created_cpt_t02')
                        admin.locator('#content-type-label').fill('Late Created CPT T02')
                        admin.get_by_role('button', name='Tạo loại nội dung', exact=True).click()
                        admin.get_by_role('form', name='Cấu hình loại nội dung') \
                            .get_by_role('textbox', name='Tên hiển thị').fill('Books Edited During Create')
                        check(held_create, 'create POST must actually remain delayed')
                        held_create[0].continue_()
                        admin.get_by_role('status').filter(
                            has_text='Đã tạo CPT late_created_cpt_t02, nhưng có thay đổi khác chưa lưu.',
                        ).wait_for(timeout=15000)
                        check(admin.url.endswith('#/appearance/content-types/books_t02')
                              and admin.get_by_role('form', name='Cấu hình loại nội dung')
                                  .get_by_role('textbox', name='Tên hiển thị').input_value() == 'Books Edited During Create',
                              'newly entered settings survive late successful create response')
                        check(any(t['descriptor']['key'] == 'late_created_cpt_t02'
                                  for t in api(ctx, base, 'GET', '/api/admin/content-types')),
                              'concurrent CPT POST committed despite retaining later settings draft')
                        admin.get_by_role('button', name='Tải lại từ máy chủ (bỏ chỉnh sửa)').click()
                    finally:
                        admin.unroute(create_route, hold_create_post)
                    checks.append('chrome_late_create_201_never_discards_concurrent_settings_edit')

                    # CPT *settings* are a separate draft from both taxonomy
                    # overrides and new-CPT creation. Retain field values on
                    # 401, reject an old CAS after remote edit, then rebase
                    # only the locally changed keys onto the new CPT version.
                    settings_form = admin.get_by_role('form', name='Cấu hình loại nội dung')
                    settings_label = settings_form.get_by_role('textbox', name='Tên hiển thị')
                    settings_label.wait_for(timeout=15000)
                    settings_label.fill('Books Local Draft T02')
                    settings_form.get_by_role('checkbox', name='Công khai nội dung (public)').uncheck()
                    fetch(ctx, base, 'POST', '/auth/logout', 204, csrf=csrf)
                    settings_form.get_by_role('button', name='Lưu cấu hình').click()
                    admin.locator('#auth-login').wait_for(timeout=15000)
                    admin.locator('#auth-username').fill('c2_t02_other')
                    admin.locator('#auth-password').fill('Isolated-Other-C2-Admin!2026')
                    admin.locator('#auth-login').click()
                    other_settings = admin.get_by_role('form', name='Cấu hình loại nội dung')
                    other_settings.get_by_role('textbox', name='Tên hiển thị').wait_for(timeout=15000)
                    check(other_settings.get_by_role('textbox', name='Tên hiển thị').input_value() != 'Books Local Draft T02',
                          'other Admin must not inherit saved-in-RAM settings draft')
                    other_csrf = api(ctx, base, 'GET', '/auth/me')['csrf_token']
                    fetch(ctx, base, 'POST', '/auth/logout', 204, csrf=other_csrf)
                    other_settings.get_by_role('button', name='Tải lại từ máy chủ (bỏ chỉnh sửa)').click()
                    admin.locator('#auth-login').wait_for(timeout=15000)
                    admin.locator('#auth-username').fill('c2_t02_admin')
                    admin.locator('#auth-password').fill('Isolated-Only-C2-T02-Pass!2026')
                    admin.locator('#auth-login').click()
                    settings_form = admin.get_by_role('form', name='Cấu hình loại nội dung')
                    settings_label = settings_form.get_by_role('textbox', name='Tên hiển thị')
                    settings_label.wait_for(timeout=15000)
                    check(settings_label.input_value() == 'Books Local Draft T02'
                          and not settings_form.get_by_role('checkbox', name='Công khai nội dung (public)').is_checked(),
                          'same-account settings label/public draft restored after 401')
                    csrf = api(ctx, base, 'GET', '/auth/me')['csrf_token']
                    checks.append('chrome_existing_cpt_settings_401_same_account_and_cross_account_guard')

                    remote_def = {**book['descriptor'],
                        'settings':{**book['descriptor']['settings'], 'has_archive': False}}
                    book = api(ctx, base, 'PUT', '/api/admin/content-types/books_t02', 200,
                        remote_def, csrf, book['version'])
                    with admin.expect_response(lambda response: response.request.method == 'PUT'
                            and '/api/admin/content-types/books_t02' in response.url, timeout=15000) as stale_save:
                        settings_form.get_by_role('button', name='Lưu cấu hình').click()
                    check(stale_save.value.status == 412, 'stale CPT settings CAS must reject concurrent edit')
                    settings_form.get_by_role('button', name='Đối chiếu CPT mới, giữ thay đổi đang nhập').click()
                    check(settings_label.input_value() == 'Books Local Draft T02'
                          and not settings_form.get_by_role('checkbox', name='Công khai nội dung (public)').is_checked()
                          and not settings_form.get_by_role('checkbox', name='Cho phép trang danh sách (has_archive)').is_checked(),
                          'settings rebase retains local label/public and remote archive change')
                    with admin.expect_response(lambda response: response.request.method == 'PUT'
                            and '/api/admin/content-types/books_t02' in response.url, timeout=15000) as rebased_save:
                        settings_form.get_by_role('button', name='Lưu cấu hình').click()
                    check(rebased_save.value.status == 200, 'rebase persists merged settings with latest CAS')
                    saved_book = next(t for t in api(ctx, base, 'GET', '/api/admin/content-types')
                                      if t['descriptor']['key'] == 'books_t02')
                    check(saved_book['descriptor']['label'] == 'Books Local Draft T02'
                          and saved_book['descriptor']['settings']['public'] is False
                          and saved_book['descriptor']['settings']['has_archive'] is False,
                          'saved settings merge preserves both writers')
                    restored_def = {**saved_book['descriptor'], 'label':'Books T02',
                        'settings':{**saved_book['descriptor']['settings'],
                                    'public':True, 'has_archive':True}}
                    book = api(ctx, base, 'PUT', '/api/admin/content-types/books_t02', 200,
                        restored_def, csrf, saved_book['version'])
                    admin.reload(wait_until='domcontentloaded')
                    admin.get_by_role('form', name='Cấu hình loại nội dung').wait_for(timeout=15000)
                    checks.append('chrome_existing_cpt_settings_412_rebase_preserves_unrelated_remote_edit')

                    # A concurrent writer changes CPT associations while the
                    # UI has a local edit. The first save must 412, then the
                    # explicit rebase must keep only the author's delta and
                    # preserve the other writer's new association.
                    label_choice.uncheck()
                    book = api(ctx, base, 'PUT', '/api/admin/taxonomy/eligibility/books_t02', 200,
                        {'taxonomies':['genre_t02','label_t02','tag'],
                         'expected_version':book['version']}, csrf, book['version'])
                    admin.get_by_role('button', name='Lưu taxonomy áp dụng').click()
                    admin.get_by_role('button', name='Đối chiếu với máy chủ, giữ lựa chọn đang sửa') \
                        .wait_for(timeout=15000)
                    admin.get_by_role('button', name='Đối chiếu với máy chủ, giữ lựa chọn đang sửa').click()
                    tag_choice = admin.get_by_role('region', name='Taxonomy áp dụng cho loại nội dung') \
                        .locator('label').filter(has=admin.locator('code', has_text=re.compile(r'^tag$'))) \
                        .locator('input[type="checkbox"]')
                    tag_choice.wait_for(timeout=15000)
                    check(not label_choice.is_checked() and tag_choice.is_checked(),
                          '412 rebase retains local removal and concurrent taxonomy addition')
                    with admin.expect_response(lambda response: response.request.method == 'PUT'
                            and '/api/admin/taxonomy/eligibility/books_t02' in response.url,
                            timeout=15000) as save_reply:
                        admin.get_by_role('button', name='Lưu taxonomy áp dụng').click()
                    check(save_reply.value.status == 200,
                          f'412 rebase must save with refreshed CAS, got {save_reply.value.status}')
                    check(api(ctx, base, 'GET', '/api/admin/taxonomy/eligibility/books_t02/configured')
                          ['taxonomies'] == ['genre_t02','tag'],
                          'rebase CAS persists merged taxonomy selection without silent loss')
                    book = next(item for item in api(ctx, base, 'GET', '/api/admin/content-types')
                                if item['descriptor']['key'] == 'books_t02')
                    checks.append('chrome_cpt_taxonomy_412_rebase_preserves_concurrent_association')

                    # Provider-owned CPT schemas are read-only to site Admins,
                    # but the site-specific taxonomy association is editable.
                    # Exercise both independent permission paths in the UI.
                    with sqlite3.connect(db) as conn:
                        conn.execute("UPDATE content_types SET owner_kind='plugin', owner_id='provider_t02' WHERE key='books_t02'")
                    admin.goto(base + '/admin/#/appearance/content-types/books_t02', wait_until='domcontentloaded')
                    # This may already be the current hash. A hash-only goto
                    # does not necessarily remount the SPA; force a full reload
                    # to observe the externally changed provider ownership.
                    admin.reload(wait_until='domcontentloaded')
                    provider_taxonomy = admin.get_by_role('region', name='Taxonomy áp dụng cho loại nội dung')
                    provider_label = provider_taxonomy.get_by_role('checkbox', name=re.compile('Label T02'))
                    provider_label.wait_for(timeout=15000)
                    provider_from_api = next(t for t in api(ctx, base, 'GET', '/api/admin/content-types')
                        if t['descriptor']['key'] == 'books_t02')
                    check(provider_from_api['owner']['kind'] == 'plugin',
                          f'backend must expose provider ownership, got {provider_from_api["owner"]}')
                    current_screen = admin.get_by_test_id('content-type-schema-settings').inner_text()
                    check('Chủ sở hữu: plugin' in current_screen,
                          f'selected CPT must use provider ownership, UI={current_screen[:400]}')
                    check(admin.get_by_role('button', name='Lưu cấu hình').count() == 0,
                          f'provider-owned CPT schema stays read-only, UI={current_screen[:800]}')
                    provider_label.check()
                    with admin.expect_response(lambda response: response.request.method == 'PUT'
                            and '/api/admin/taxonomy/eligibility/books_t02' in response.url,
                            timeout=15000) as provider_save:
                        admin.get_by_role('button', name='Lưu taxonomy áp dụng').click()
                    check(provider_save.value.status == 200,
                          'Admin may override provider-owned CPT taxonomy associations')
                    provider_config = api(ctx, base, 'GET',
                        '/api/admin/taxonomy/eligibility/books_t02/configured')
                    check(provider_config['taxonomies'] == ['genre_t02','label_t02','tag'],
                          'provider override persists all taxonomies')

                    # Provider-owned CPT has no editable schema settings form.
                    # A successful create response must still be visible if an
                    # independent provider-CPT taxonomy draft becomes dirty
                    # while POST is in flight; no silent navigation or loss.
                    held_provider_create = []
                    provider_create_pattern = '**/api/admin/content-types'
                    def hold_provider_create(route):
                        if route.request.method == 'POST':
                            held_provider_create.append(route)
                        else:
                            route.continue_()
                    admin.route(provider_create_pattern, hold_provider_create)
                    try:
                        admin.get_by_role('button', name='+ Tạo loại nội dung mới').click()
                        admin.locator('#content-type-key').fill('provider_late_cpt_t02')
                        admin.locator('#content-type-label').fill('Provider Late CPT T02')
                        admin.get_by_role('button', name='Tạo loại nội dung', exact=True).click()
                        provider_label.uncheck()
                        check(bool(held_provider_create), 'provider-owned create POST was held')
                        held_provider_create[0].continue_()
                        admin.get_by_role('status').filter(
                            has_text='Đã tạo CPT provider_late_cpt_t02, nhưng có thay đổi khác chưa lưu.'
                        ).wait_for(timeout=15000)
                        check(admin.url.endswith('#/appearance/content-types/books_t02')
                              and not provider_label.is_checked(),
                              'provider taxonomy draft stays unsaved after completed CPT create')
                        check(any(t['descriptor']['key'] == 'provider_late_cpt_t02'
                                  for t in api(ctx, base, 'GET', '/api/admin/content-types')),
                              'provider create operation committed exactly one visible CPT')
                        admin.get_by_role('button', name='Hủy lựa chọn chưa lưu').click()
                        provider_label.wait_for(timeout=15000)
                        check(provider_label.is_checked(),
                              'explicitly discarding provider taxonomy draft restores association')
                    finally:
                        admin.unroute(provider_create_pattern, hold_provider_create)
                    checks.append('chrome_provider_cpt_create_201_notice_preserves_late_taxonomy_draft')
                    with sqlite3.connect(db) as conn:
                        conn.execute("UPDATE content_types SET provider_status='provider_inactive' WHERE key='books_t02'")
                    check(api(ctx, base, 'GET', '/api/admin/taxonomy/eligibility/books_t02/configured')
                          ['taxonomies'] == ['genre_t02','label_t02','tag'],
                          'provider deactivation preserves site association override')
                    with sqlite3.connect(db) as conn:
                        conn.execute("UPDATE content_types SET provider_status='active', owner_kind='user', owner_id=NULL WHERE key='books_t02'")
                    book = next(item for item in api(ctx, base, 'GET', '/api/admin/content-types')
                                if item['descriptor']['key'] == 'books_t02')
                    checks.append('provider_cpt_schema_read_only_but_taxonomy_override_survives_lifecycle')

                    # Two configured GETs may overlap if an Admin presses
                    # Reload while the first selection is still loading.
                    # Release the older reply last; it must not restore an
                    # outdated baseline/checkbox or drop the latest version.
                    race_page = ctx.new_page()
                    try:
                        race_page.goto(base + '/admin/#/taxonomy/definitions', wait_until='domcontentloaded')
                        race_page.get_by_label('Loại nội dung').wait_for(timeout=15000)
                        # The first alphabetical CPT may already be books_t02;
                        # switch to Post before intercepting the books request.
                        race_page.get_by_label('Loại nội dung').select_option('post')
                        race_page.get_by_role('checkbox', name=re.compile(r'Category \(category\)')).wait_for(timeout=15000)
                        stale_snapshot = api(ctx, base, 'GET',
                            '/api/admin/taxonomy/eligibility/books_t02/configured')
                        held = []
                        seen = []
                        def hold_first(route):
                            seen.append(len(seen) + 1)
                            if len(seen) == 1:
                                held.append(route)
                            else:
                                route.continue_()
                        race_pattern = '**/api/admin/taxonomy/eligibility/books_t02/configured'
                        race_page.route(race_pattern, hold_first)
                        with race_page.expect_request(lambda request:
                                '/api/admin/taxonomy/eligibility/books_t02/configured' in request.url,
                                timeout=15000):
                            race_page.get_by_label('Loại nội dung').select_option('books_t02')
                        race_page.wait_for_timeout(200)
                        check(held, 'first configured request was actually delayed')
                        updated_race_cpt = api(ctx, base, 'PUT',
                            '/api/admin/taxonomy/eligibility/books_t02', 200,
                            {'taxonomies':stale_snapshot['taxonomies'] + ['category'],
                             'expected_version':book['version']}, csrf, book['version'])
                        race_page.get_by_role('button', name='Tải lại', exact=True).click()
                        race_page.get_by_role('checkbox', name=re.compile(r'Category \(category\)')).wait_for(timeout=15000)
                        check(race_page.get_by_role('checkbox', name=re.compile(r'Category \(category\)')).is_checked(),
                              'new refresh received updated provider association')
                        held[0].fulfill(status=200, content_type='application/json',
                                        body=json.dumps(stale_snapshot))
                        race_page.wait_for_timeout(200)
                        # A fast current request must win even if a slow old
                        # reply arrives after it.
                        check(race_page.get_by_role('checkbox', name=re.compile(r'Category \(category\)')).is_checked(),
                              'stale configured GET cannot overwrite newer eligibility selection')
                        race_page.unroute(race_pattern, hold_first)
                        book = updated_race_cpt
                    finally:
                        race_page.close()
                    checks.append('chrome_eligibility_late_initial_get_cannot_overwrite_refresh')

                    admin.goto(base + f'/admin/#/cpt/books_t02/{tagged["id"]}/edit',
                               wait_until='domcontentloaded')
                    admin.get_by_text('Genre T02').first.wait_for(timeout=15000)
                    check('Speculative' in admin.locator('body').inner_text(),
                          'Single Edit shows eligible custom taxonomy term')
                    admin.screenshot(path=str(artifacts / 'single-edit-taxonomy-desktop.png'), full_page=True)
                    admin.set_viewport_size({'width':390, 'height':844})
                    admin.screenshot(path=str(artifacts / 'single-edit-390.png'), full_page=True)
                    check(admin.evaluate('document.documentElement.scrollWidth <= window.innerWidth'),
                          'single editor should not overflow 390px viewport')
                    checks.append('single_edit_custom_taxonomy_picker_and_mobile_390')
                    admin.close()

                    book = api(ctx, base, 'PUT', '/api/admin/taxonomy/eligibility/books_t02', 200,
                        {'taxonomies':[], 'expected_version':book['version']}, csrf, book['version'])
                    hidden = fetch(ctx, base, 'GET', '/', 200).text()
                    check('Speculative C2 Visible' not in hidden,
                          'withdrawn taxonomy safely hides Builder list without public 400')
                    persisted = api(ctx, base, 'GET', f'/api/admin/content-types/books_t02/entries/{tagged["id"]}')
                    check(persisted['term_ids'] == [child['id']], 'withdrawal preserves links')
                    checks.append('withdrawn_eligibility_public_safe_and_links_preserved')

                    with sqlite3.connect(db) as conn:
                        check(conn.execute('PRAGMA integrity_check').fetchone()[0] == 'ok', 'integrity')
                        check(conn.execute('PRAGMA foreign_key_check').fetchall() == [], 'FK')
                        check(conn.execute('SELECT COUNT(*) FROM entry_terms WHERE entry_id=?',
                            (tagged['id'],)).fetchone()[0] == 1, 'entry membership')
                    checks.append('sqlite_integrity_fk_and_membership')
                finally:
                    browser.close()
        finally:
            stop_server(server)
        server, base = launch_server(binary, site)
        try:
            with sqlite3.connect(site / 'database/opendash.sqlite') as conn:
                check(conn.execute("SELECT COUNT(*) FROM taxonomy_definitions WHERE key IN ('genre_t02','label_t02')").fetchone()[0] == 2, 'definition restart')
                check(conn.execute("SELECT taxonomies_json FROM content_type_taxonomy_overrides WHERE content_type_key='books_t02'").fetchone()[0] == '[]', 'eligibility restart')
            checks.append('native_restart_preserves_taxonomies_and_cpt_override')
        finally:
            stop_server(server)
    return {'result':'PASS', 'checks':checks, 'chrome_version':chrome_version,
            'port_18964_used':False}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=pathlib.Path, required=True)
    parser.add_argument('--artifact-dir', type=pathlib.Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    artifacts = args.artifact_dir.resolve()
    artifacts.mkdir(parents=True, exist_ok=True)
    repo = pathlib.Path(__file__).resolve().parents[2]
    receipt = {'source_sha': subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip(),
               'source_changed_paths': subprocess.check_output(['git','status','--porcelain'],cwd=repo,text=True).splitlines(),
               'binary_sha256':sha(binary), 'binary_path':str(binary),
               'runner_sha256':sha(pathlib.Path(__file__).resolve()),
               'platform':platform.platform(), 'architecture':platform.machine()}
    try:
        receipt.update(run(binary, artifacts))
        receipt['screenshot_sha256'] = {p.name:sha(p) for p in artifacts.glob('*.png')}
    except Exception as error:
        receipt.update({'result':'FAIL', 'error':str(error), 'traceback':traceback.format_exc()})
    (artifacts / 'receipt.json').write_text(json.dumps(receipt, indent=2, ensure_ascii=False)+'\n')
    print(json.dumps(receipt, indent=2, ensure_ascii=False))
    if receipt['result'] != 'PASS':
        raise SystemExit(1)


if __name__ == '__main__':
    main()
