#!/usr/bin/env python3
"""#356 local library + Section COPY native Chrome/SQLite on an isolated site.

NEVER use or touch owner's 127.0.0.1:18964 server, SQLite or browser profile.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import pathlib
import platform
import sqlite3
import subprocess
import tempfile
import traceback

from playwright.sync_api import expect, sync_playwright
from m7_beta_menu_picker_e2e import admin_login, api_json, launch_server, stop_server
from m4_s11_t02_sections_e2e import create_ui_section
from m4_s11_t01_page_builder_e2e import ui_create_page, new_heading
from m5_s08_t01_macos import zip_v2
from m6_s10_t01_admin_extensions_e2e import THEME_JSON, package_zip
from media_smoke import make_png


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def page_scope(page_id: int) -> str:
    return f"/api/admin/builder/overrides/page?type_key=page&entry_id={page_id}"


def run(binary: pathlib.Path, artifacts: pathlib.Path) -> dict:
    assert platform.system() == 'Darwin' and platform.machine() == 'x86_64', 'native macOS Intel required'
    checks: list[str] = []
    errors: list[str] = []
    source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    with tempfile.TemporaryDirectory(prefix='opendash-m5-s12-t01-') as temp:
        root = pathlib.Path(temp).resolve() / 'site'
        root.mkdir(mode=0o700)
        server, base = launch_server(binary, root)
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel='chrome', headless=True)
                try:
                    admin = browser.new_context(viewport={'width': 1440, 'height': 900})
                    page = admin.new_page()
                    page.on('pageerror', lambda exc: errors.append(str(exc)))
                    anon = browser.new_context()
                    assert anon.request.get(base + '/api/admin/library').status == 401
                    api_json(admin, 'POST', base, '/auth/setup', {
                        'username': 'library_admin', 'password': 'AdminPassword123!',
                    }, expected=201)
                    csrf = admin_login(admin, base, 'library_admin', 'AdminPassword123!')
                    assert admin.request.get(base + '/api/admin/library').headers.get('cache-control') == 'no-store'
                    catalog = api_json(admin, 'GET', base, '/api/admin/library')
                    assert catalog['sections'] == [] and catalog['theme_templates'] == []
                    assert catalog['theme_state'] == 'no_active_theme'
                    checks.append('catalog_authentication_and_empty_local_no_cloud_fallback')

                    source_id = create_ui_section(page, base, 'UI library banner', 'library-banner')
                    page.get_by_role('button', name='Mở Section Builder').click()
                    expect(page.get_by_role('heading', name=f'Section Builder #{source_id}')).to_be_visible(timeout=15000)
                    new_heading(page, 'LIBRARY COPY HEADING PRIVATE')
                    page.get_by_role('button', name='Lưu Section').click()
                    expect(page.get_by_test_id('builder-workspace')).to_contain_text('Đã lưu Section', timeout=15000)
                    source_path = f'/api/admin/sections/{source_id}/document'
                    source_open = api_json(admin, 'GET', base, source_path)
                    image = admin.request.post(base + '/api/admin/media', data=make_png(seed=62), headers={
                        'Origin': base, 'X-CSRF-Token': csrf, 'Content-Type': 'image/png',
                        'X-File-Name': 'library-private.png',
                    })
                    assert image.status == 201, image.text()[:300]
                    image_id = image.json()['id']
                    source_document = copy.deepcopy(source_open['document'])
                    source_document['sections'][0]['children'][0]['children'][0]['children'].append({
                        'id': 'library-private-image', 'type': 'core/image',
                        'props': {'media_id': image_id, 'alt': 'Private Section-only image'},
                    })
                    api_json(admin, 'PUT', base, source_path, source_document, csrf,
                             {'If-Match': f'"v{source_open["version"]}"'})
                    assert anon.request.get(base + f'/media/{image_id}/file').status == 404
                    assert anon.request.get(base + '/content/section/library-banner').status == 404
                    checks.append('UI_created_private_Section_with_image_never_public_before_copy')

                    page.goto(base + '/admin/#/appearance/library', wait_until='domcontentloaded')
                    expect(page.get_by_test_id('local-template-library')).to_be_visible(timeout=15000)
                    expect(page.get_by_text('UI library banner')).to_be_visible(timeout=15000)
                    assert page.get_by_test_id('pending-library').count() == 0
                    assert api_json(admin, 'GET', base, '/api/admin/library')['sections'][0]['id'] == source_id
                    assert 'document' not in api_json(admin, 'GET', base, '/api/admin/library')['sections'][0]
                    page.screenshot(path=str(artifacts / 'library-list.png'), full_page=True)
                    checks.append('real_library_navigation_and_read_only_Section_metadata_no_AST_leak')

                    # Actual signed V2 theme active revision: supported roles
                    # are exposed, while page/single/home are marked unsupported.
                    uploaded = admin.request.post(base + '/api/admin/themes', data=zip_v2(), headers={
                        'Origin': base, 'X-CSRF-Token': csrf, 'Content-Type': 'application/zip',
                    })
                    assert uploaded.status == 201, uploaded.text()[:300]
                    themes = api_json(admin, 'GET', base, '/api/admin/themes')
                    api_json(admin, 'PUT', base, '/api/admin/themes/active',
                        {'revision_id': uploaded.json()['revision_id']}, csrf,
                        {'If-Match': f'"v{themes["version"]}"'})
                    catalog = api_json(admin, 'GET', base, '/api/admin/library')
                    assert catalog['theme_state'] == 'active', catalog
                    roles = {item['role']: item for item in catalog['theme_templates']}
                    assert roles['header']['can_apply'] and roles['footer']['can_apply']
                    assert not roles['home']['can_apply'] and not roles['single']['can_apply']
                    assert all('document' not in item for item in catalog['theme_templates'])
                    header_descriptor = roles['header']
                    checks.append('active_theme_local_validated_descriptors_supported_and_unsupported_roles')

                    # Explicit apply uses the existing role/reset/If-Match path.
                    header = '/api/admin/builder/overrides/header'
                    heading = {'schema_version': 1, 'sections': [{
                        'id': 'theme-header', 'children': [{'id': 'theme-row', 'children': [
                            {'id': 'theme-col', 'children': [{'id': 'theme-heading',
                                'type': 'core/heading', 'props': {'level': 2, 'text': 'CUSTOM HEADER'}}]},
                        ]}]},
                    ]}
                    api_json(admin, 'PUT', base, header, heading, csrf, {'If-Match': '"v0"'})
                    apply_path = '/api/admin/library/apply/header'
                    apply_body = {'confirm': True, 'revision_id': roles['header']['revision_id'],
                                  'revision_digest': roles['header']['revision_digest']}
                    assert anon.request.post(base + apply_path, data=json.dumps(apply_body), headers={
                        'Origin': base, 'Content-Type': 'application/json', 'If-Match': '"v1"',
                    }).status == 401
                    assert admin.request.post(base + apply_path, data=json.dumps(apply_body), headers={
                        'Origin': base, 'Content-Type': 'application/json', 'If-Match': '"v1"',
                    }).status == 403
                    assert admin.request.post(base + apply_path, data=json.dumps(apply_body), headers={
                        'Origin': 'https://cross-site.invalid', 'X-CSRF-Token': csrf,
                        'Content-Type': 'application/json', 'If-Match': '"v1"',
                    }).status == 403
                    checks.append('atomic_theme_apply_anonymous_CSRF_Origin_denied_and_catalog_no_store')
                    page.reload(wait_until='domcontentloaded')
                    expect(page.get_by_role('button', name='Áp dụng mẫu theme').first).to_be_visible(timeout=15000)
                    header_card = page.get_by_role('article').filter(has=page.get_by_text('header', exact=True))
                    button = header_card.get_by_role('button', name='Áp dụng mẫu theme')
                    dialogs = []

                    def decline(dialog):
                        dialogs.append(dialog.message)
                        dialog.dismiss()

                    page.on('dialog', decline)
                    button.click()
                    expect(page.get_by_role('status')).to_contain_text('Đã hủy', timeout=12000)
                    assert api_json(admin, 'GET', base, header)['exists'] is True
                    page.remove_listener('dialog', decline)
                    checks.append('theme_apply_cancel_does_not_reset_live_override')

                    def race_before_accept(dialog):
                        current = api_json(admin, 'GET', base, header)
                        changed = copy.deepcopy(heading)
                        changed['sections'][0]['children'][0]['children'][0]['children'][0]['props']['text'] = 'REMOTE HEADER WINS'
                        api_json(admin, 'PUT', base, header, changed, csrf,
                                 {'If-Match': f'"v{current["version"]}"'})
                        dialog.accept()

                    page.on('dialog', race_before_accept)
                    with page.expect_response(lambda r: r.url.endswith(apply_path) and r.request.method == 'POST') as stale_override:
                        button.click()
                    assert stale_override.value.status == 412
                    expect(page.get_by_role('status')).to_contain_text('412', timeout=12000)
                    assert 'REMOTE HEADER WINS' in json.dumps(api_json(admin, 'GET', base, header))
                    page.remove_listener('dialog', race_before_accept)

                    # The user confirms catalog revision A while another Admin
                    # activates revision B. The atomic active-revision CAS must return
                    # 412 and leave the customized global header untouched.
                    other = admin.request.post(base + '/api/admin/themes',
                        data=zip_v2(theme_id='mac-proof-switch', version='1.0.0'), headers={
                            'Origin': base, 'X-CSRF-Token': csrf, 'Content-Type': 'application/zip',
                        })
                    assert other.status == 201, other.text()[:300]
                    def change_active_before_accept(dialog):
                        current_theme = api_json(admin, 'GET', base, '/api/admin/themes')
                        api_json(admin, 'PUT', base, '/api/admin/themes/active',
                            {'revision_id': other.json()['revision_id']}, csrf,
                            {'If-Match': f'"v{current_theme["version"]}"'})
                        dialog.accept()

                    page.on('dialog', change_active_before_accept)
                    with page.expect_response(lambda r: r.url.endswith(apply_path) and r.request.method == 'POST') as stale_theme:
                        button.click()
                    assert stale_theme.value.status == 412
                    expect(page.get_by_role('status')).to_contain_text('412', timeout=12000)
                    assert api_json(admin, 'GET', base, header)['exists'] is True
                    page.remove_listener('dialog', change_active_before_accept)
                    current_theme = api_json(admin, 'GET', base, '/api/admin/themes')
                    api_json(admin, 'PUT', base, '/api/admin/themes/active',
                        {'revision_id': uploaded.json()['revision_id']}, csrf,
                        {'If-Match': f'"v{current_theme["version"]}"'})
                    checks.append('theme_revision_switch_during_confirmation_returns_412_without_override_loss')

                    page.on('dialog', lambda dialog: dialog.accept())
                    with page.expect_response(lambda r: r.url.endswith(apply_path) and r.request.method == 'POST') as applied:
                        button.click()
                    assert applied.value.status == 200
                    expect(page.get_by_role('status')).to_contain_text('đã được áp dụng', timeout=12000)
                    assert api_json(admin, 'GET', base, header)['exists'] is False
                    checks.append('theme_apply_confirmed_CAS_412_preserves_remote_then_valid_reset')

                    page_id = ui_create_page(page, base, 'Library destination Page', 'library-destination',
                                             'Library fallback Page body')
                    page.get_by_role('button', name='Mở Visual Builder').click()
                    expect(page.get_by_role('heading', name=f'Page Builder #{page_id}')).to_be_visible(timeout=15000)
                    page.get_by_test_id('builder-local-library').get_by_role('button', name='Chọn Section', exact=True).click()
                    picker = page.get_by_test_id('builder-local-library')
                    expect(picker.get_by_text('UI library banner', exact=False)).to_be_visible(timeout=12000)
                    for attempt in (1, 2):
                        picker.get_by_role('button', name='Chèn bản sao').click()
                        expect(page.get_by_test_id('builder-workspace')).to_contain_text(
                            'Đã chèn bản SAO', timeout=15000)
                        assert api_json(admin, 'GET', base, page_scope(page_id))['exists'] is False
                        assert api_json(admin, 'GET', base, source_path)['document'] == source_document
                    page.screenshot(path=str(artifacts / 'two-copies-unsaved.png'), full_page=True)
                    assert anon.request.get(base + f'/media/{image_id}/file').status == 404
                    assert anon.request.get(base + '/content/page/library-destination').status == 404
                    checks.append('two_unique_UI_copy_actions_unsaved_no_public_side_effects')

                    page.get_by_role('button', name='Xem trước').click()
                    expect(page.frame_locator('#canvas').locator('body')).to_contain_text(
                        'LIBRARY COPY HEADING PRIVATE', timeout=15000)
                    assert api_json(admin, 'GET', base, page_scope(page_id))['exists'] is False
                    checks.append('authenticated_unsaved_copy_preview_no_persist_or_publish')
                    page.get_by_role('button', name='Lưu override').click()
                    expect(page.get_by_test_id('builder-workspace')).to_contain_text('Đã lưu', timeout=15000)
                    saved = api_json(admin, 'GET', base, page_scope(page_id))
                    assert saved['version'] == 1
                    copied = saved['document']['sections']
                    assert len(copied) == 2 and copied[0] != copied[1]
                    src_ids = set()
                    for section in source_document['sections']:
                        src_ids.add(section['id'])
                        for row in section['children']:
                            src_ids.add(row['id'])
                            for column in row['children']:
                                src_ids.add(column['id'])
                                src_ids.update(element['id'] for element in column['children'])
                    copied_ids = [node['id'] for sec in copied for node in [sec]
                                  + sec['children'] + [col for row in sec['children'] for col in row['children']]
                                  + [el for row in sec['children'] for col in row['children'] for el in col['children']]]
                    assert len(copied_ids) == len(set(copied_ids)) and not src_ids.intersection(copied_ids)
                    assert api_json(admin, 'GET', base, source_path)['document'] == source_document
                    assert anon.request.get(base + f'/media/{image_id}/file').status == 404
                    checks.append('saved_draft_contains_fresh_ID_tree_twice_source_preserved_media_private')

                    entry = api_json(admin, 'GET', base, f'/api/admin/content-types/page/entries/{page_id}')
                    api_json(admin, 'POST', base,
                             f'/api/admin/content-types/page/entries/{page_id}/publication',
                             {'status': 'published'}, csrf, {'If-Match': f'"v{entry["version"]}"'})
                    public = anon.request.get(base + '/content/page/library-destination')
                    assert public.status == 200, public.text()[:300]
                    assert public.text().count('LIBRARY COPY HEADING PRIVATE') == 2
                    assert anon.request.get(base + f'/media/{image_id}/file').status == 200
                    checks.append('only_explicit_published_destination_save_grants_copied_media_public')

                    # Editor can list Section metadata and copy into its exact
                    # own Page Builder, but never see/apply shared theme roles.
                    api_json(admin, 'POST', base, '/api/admin/users', {
                        'username': 'library_editor', 'password': 'EditorPassword123!', 'role': 'editor',
                    }, csrf, expected=201)
                    editor = browser.new_context(viewport={'width': 390, 'height': 844})
                    editor_csrf = admin_login(editor, base, 'library_editor', 'EditorPassword123!')
                    editor_catalog = api_json(editor, 'GET', base, '/api/admin/library')
                    assert len(editor_catalog['sections']) == 1
                    assert editor_catalog['theme_templates'] == [] and editor_catalog['theme_state'] == 'admin_only'
                    assert editor.request.get(base + '/api/admin/builder/overrides/header').status == 403
                    assert editor.request.get(base + '/api/admin/builder/homepage').status == 403
                    forbidden = editor.request.post(base + '/api/admin/builder/overrides/header/reset',
                        data=json.dumps({'confirm': True}), headers={'Origin': base,
                        'X-CSRF-Token': editor_csrf, 'Content-Type': 'application/json', 'If-Match': '"v1"'})
                    assert forbidden.status == 403
                    forbidden_library_apply = editor.request.post(base + '/api/admin/library/apply/header',
                        data=json.dumps({'confirm': True, 'revision_id': header_descriptor['revision_id'],
                                         'revision_digest': header_descriptor['revision_digest']}),
                        headers={'Origin': base, 'X-CSRF-Token': editor_csrf,
                                 'Content-Type': 'application/json', 'If-Match': '"v0"'})
                    assert forbidden_library_apply.status == 403
                    editor_apply = editor.request.post(base + apply_path,
                        data=json.dumps(apply_body), headers={'Origin': base,
                        'X-CSRF-Token': editor_csrf, 'Content-Type': 'application/json',
                        'If-Match': '"v0"'})
                    assert editor_apply.status == 403
                    ep = editor.new_page()
                    ep.goto(base + f'/admin/#/builder?template=page&type=page&entry={page_id}',
                            wait_until='domcontentloaded')
                    expect(ep.get_by_role('heading', name=f'Page Builder #{page_id}')).to_be_visible(timeout=15000)
                    ep.get_by_test_id('builder-local-library').get_by_role('button', name='Chọn Section', exact=True).click()
                    ep.get_by_test_id('builder-local-library').get_by_role('button', name='Chèn bản sao').click()
                    expect(ep.get_by_test_id('builder-workspace')).to_contain_text('Đã chèn bản SAO', timeout=15000)
                    ep.screenshot(path=str(artifacts / 'editor-copy-390.png'), full_page=True)
                    assert ep.evaluate('document.body.scrollWidth <= innerWidth + 2'), '390px horizontal overflow'
                    assert api_json(admin, 'GET', base, page_scope(page_id)) == saved or api_json(admin, 'GET', base, page_scope(page_id))['document'] == saved['document']
                    editor.close()
                    checks.append('Editor_allowed_scoped_copy_no_theme_authority_390px_unsaved')

                    # Switch away from the V2 provider to a supported
                    # home-only theme. The old header provider is withdrawn;
                    # no unpublished Section or Page AST may be mutated.
                    alternative = admin.request.post(base + '/api/admin/themes',
                        data=package_zip(json.loads(THEME_JSON.read_text()), 'theme'), headers={
                            'Origin': base, 'X-CSRF-Token': csrf, 'Content-Type': 'application/zip',
                        })
                    assert alternative.status == 201, alternative.text()[:300]
                    themes = api_json(admin, 'GET', base, '/api/admin/themes')
                    api_json(admin, 'PUT', base, '/api/admin/themes/active',
                        {'revision_id': alternative.json()['revision_id']}, csrf,
                        {'If-Match': f'"v{themes["version"]}"'})
                    assert api_json(admin, 'GET', base, source_path)['document'] == source_document
                    assert api_json(admin, 'GET', base, page_scope(page_id))['document'] == saved['document']
                    fallback = api_json(admin, 'GET', base, '/api/admin/library')
                    assert fallback['theme_state'] == 'active'
                    assert not any(role['role'] == 'header' for role in fallback['theme_templates'])
                    assert all(not role['can_apply'] for role in fallback['theme_templates'])
                    stale_template_apply = admin.request.post(base + '/api/admin/library/apply/header',
                        data=json.dumps({'confirm': True, 'revision_id': header_descriptor['revision_id'],
                                         'revision_digest': header_descriptor['revision_digest']}),
                        headers={'Origin': base, 'X-CSRF-Token': csrf,
                                 'Content-Type': 'application/json', 'If-Match': '"v0"'})
                    assert stale_template_apply.status == 412, stale_template_apply.text()[:300]
                    checks.append('withdrawn_theme_catalog_fallback_preserves_source_and_copy_nodes')
                    checks.append('stale_active_theme_revision_CAS_412_and_editor_apply_403')

                    # Owner-note technical route sweep on the SAME candidate,
                    # site and Chrome account. Screenshots are technical
                    # regression artifacts, not human #302 dogfood signoff.
                    post = api_json(admin, 'POST', base, '/api/admin/content-types/post/entries',
                        {'title': 'Library owner-note Post', 'slug': 'library-owner-post', 'values': {}},
                        csrf, expected=201)
                    term = api_json(admin, 'POST', base, '/api/admin/taxonomy/terms',
                        {'kind': 'category', 'name': 'Library owner-note', 'slug': 'library-owner-note'},
                        csrf, expected=201)
                    assert post['id'] > 0 and term['id'] > 0
                    routes = [
                        ('post-table', '#/posts'),
                        ('page-table', '#/pages'),
                        ('categories', '#/posts/categories'),
                        ('media', '#/media'),
                        ('sections', '#/sections'),
                        ('local-library', '#/appearance/library'),
                        ('appearance-theme', '#/appearance/themes'),
                        ('extensions-plugins', '#/plugins'),
                        ('exact-page-builder', f'#/builder?template=page&type=page&entry={page_id}'),
                    ]
                    for name, hash_route in routes:
                        page.goto(base + '/admin/' + hash_route, wait_until='domcontentloaded')
                        # Posts/Pages are intentionally hosted in <section>,
                        # while Builder keeps an offscreen <main> mounted.
                        # Assert the actual route-specific surface instead of
                        # assuming every Admin view renders a visible <main>.
                        if name == 'post-table':
                            expect(page.locator('section[aria-label="Bài viết"]')).to_be_visible(timeout=16000)
                        elif name == 'page-table':
                            expect(page.locator('section[aria-label="Trang"]')).to_be_visible(timeout=16000)
                        elif name == 'local-library':
                            expect(page.get_by_test_id('local-template-library')).to_be_visible(timeout=16000)
                        elif name == 'sections':
                            expect(page.get_by_test_id('section-workspace')).to_be_visible(timeout=16000)
                        elif name == 'exact-page-builder':
                            expect(page.get_by_test_id('builder-workspace')).to_be_visible(timeout=16000)
                        else:
                            expect(page.locator('body')).to_be_visible(timeout=16000)
                        page.wait_for_load_state('networkidle', timeout=15000)
                        assert page.url.endswith(hash_route), (name, page.url)
                        if name == 'local-library':
                            expect(page.get_by_text('UI library banner')).to_be_visible(timeout=12000)
                        if name == 'sections':
                            expect(page.get_by_text('UI library banner')).to_be_visible(timeout=12000)
                        if name == 'exact-page-builder':
                            expect(page.get_by_role('heading', name=f'Page Builder #{page_id}')).to_be_visible(timeout=12000)
                        page.screenshot(path=str(artifacts / f'owner-note-{name}.png'), full_page=True)
                    checks.append('owner_note_Post_Page_terms_media_tables_extensions_Builder_Section_library_routes_screenshots')

                    with sqlite3.connect(root / 'database/opendash.sqlite') as db:
                        assert db.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
                        assert db.execute('PRAGMA foreign_key_check').fetchall() == []
                    assert not errors, errors
                    checks.append('sqlite_integrity_foreign_keys_and_no_Chrome_pageerrors')
                    anon.close()
                    admin.close()
                finally:
                    browser.close()
        finally:
            stop_server(server)

        resumed, base2 = launch_server(binary, root)
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel='chrome', headless=True)
                try:
                    ctx = browser.new_context()
                    admin_login(ctx, base2, 'library_admin', 'AdminPassword123!')
                    assert api_json(ctx, 'GET', base2, source_path)['document'] == source_document
                    assert api_json(ctx, 'GET', base2, page_scope(page_id))['document'] == saved['document']
                    assert api_json(ctx, 'GET', base2, '/api/admin/library')['sections'][0]['id'] == source_id
                    ctx.close()
                finally:
                    browser.close()
        finally:
            stop_server(resumed)
        checks.append('native_server_restart_preserves_local_catalog_and_page_copy_AST')
    return {'status': 'PASS', 'source_sha': source, 'binary_sha256': digest(binary),
            'platform': 'macOS Intel native', 'checks': checks, 'chrome_js_errors': errors}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=pathlib.Path, required=True)
    parser.add_argument('--artifact-dir', type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.artifact_dir.mkdir(parents=True, exist_ok=True)
    try:
        result = run(args.binary.resolve(), args.artifact_dir.resolve())
    except Exception as exc:
        result = {'status': 'FAIL', 'binary_sha256': digest(args.binary.resolve()),
                  'error': str(exc), 'traceback': traceback.format_exc()}
    (args.artifact_dir / 'receipt.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps(result, ensure_ascii=False, indent=2), flush=True)
    return 0 if result['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
