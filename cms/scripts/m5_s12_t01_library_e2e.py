#!/usr/bin/env python3
"""#356 local library + Section COPY native Chrome/SQLite on an isolated site.

NEVER use or touch owner's 127.0.0.1:18964 server, SQLite or browser profile.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import io
import json
import pathlib
import platform
import re
import sqlite3
import subprocess
import tempfile
import traceback
import zipfile

from playwright.sync_api import expect, sync_playwright
from m7_beta_menu_picker_e2e import admin_login, api_json, launch_server, stop_server
from m4_s11_t02_sections_e2e import create_ui_section
from m4_s11_t01_page_builder_e2e import ui_create_page, new_heading, change_heading, page_scope
from media_smoke import make_png


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def zip_v3(theme_id="library-theme", *, parts=True) -> bytes:
    manifest = {
        "manifest_version": 3, "theme_id": theme_id, "name": theme_id,
        "version": "1.0.0", "templates": [{"kind": "home", "file": "home.html"},
                                         {"kind": "single", "file": "single.html"}],
    }
    members = {"home.html": "<main>THEME HOME{{ od_content() }}</main>",
               "single.html": "<main>{{ od_content() }}</main>"}
    if parts:
        manifest["parts"] = {"header": "header.html", "footer": "footer.html"}
        members.update({"header.html": "<header>THEME HEADER</header>",
                        "footer.html": "<footer>THEME FOOTER</footer>"})
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("theme.json", json.dumps(manifest))
        for name, data in members.items():
            archive.writestr(name, data)
    return buf.getvalue()


def get_header(ctx, base):
    return next((item for item in api_json(ctx, "GET", base, "/api/admin/template-overrides")["items"]
                 if item["canonical_target_key"] == "part:header"), None)


def put_header(ctx, base, doc, csrf, version=None):
    return api_json(ctx, "PUT", base, "/api/admin/template-overrides",
                    {"target": {"kind": "part", "part_key": "header"}, "document": doc},
                    csrf, {"If-Match": f'"v{version}"'} if version is not None else None,
                    expected=201 if version is None else 200)


def shared_preview_checks(page, admin, anon, base, csrf, source_path, image_id, heading, artifacts, checks):
    """Exercise the actual D03 browser request before any destination Save."""
    preview_path = '/api/admin/template-overrides/preview'
    workspace = page.get_by_test_id('builder-workspace')

    def open_role(role):
        page.evaluate("(role) => location.hash = '#/builder?template=' + role", role)
        expect(page.get_by_role('heading', name=f'Override: {role}')).to_be_visible(timeout=15000)
        expect(workspace).to_contain_text('Đã mở', timeout=15000)

    def preview(status=200):
        with page.expect_response(lambda r: r.url.endswith(preview_path) and r.request.method == 'POST') as pending:
            page.get_by_role('button', name='Xem trước').click()
        response = pending.value
        assert response.status == status, response.text()[:400]
        return response

    def snapshot():
        public = anon.request.get(base + '/')
        assert public.status == 200
        return (
            api_json(admin, 'GET', base, '/api/admin/template-overrides'),
            api_json(admin, 'GET', base, '/api/admin/builder/homepage'),
            api_json(admin, 'GET', base, source_path),
            public.text(),
            anon.request.get(base + f'/media/{image_id}/file').status,
        )

    def login_ui(username, password):
        expect(page.locator('#auth-username')).to_be_visible(timeout=15000)
        assert workspace.count() == 0
        page.locator('#auth-username').fill(username)
        page.locator('#auth-password').fill(password)
        page.locator('#auth-login').click()
        expect(page.get_by_role('heading', name='Override: header')).to_be_visible(timeout=20000)
        return api_json(admin, 'GET', base, '/auth/me')['csrf_token']

    assert get_header(admin, base) is None
    before = snapshot()
    open_role('header')
    picker = page.get_by_test_id('builder-local-library')
    picker.get_by_role('button', name='Chọn Section', exact=True).click()
    expect(picker.get_by_text('UI library banner', exact=False)).to_be_visible(timeout=12000)
    picker.get_by_role('button', name='Chèn bản sao').click()
    expect(workspace).to_contain_text('Đã chèn bản SAO', timeout=15000)
    response = preview()
    payload = response.request.post_data_json
    assert payload['target'] == {'kind': 'part', 'part_key': 'header'}
    assert payload.get('preview_entry') is None
    assert 'if-match' not in response.request.all_headers()
    assert 'LIBRARY COPY HEADING PRIVATE' in json.dumps(payload['document'])
    assert response.headers.get('cache-control') == 'no-store'
    assert response.headers.get('x-content-type-options') == 'nosniff'
    iframe = page.locator('#canvas')
    frame = page.frame_locator('#canvas')
    expect(frame.locator('body')).to_contain_text('LIBRARY COPY HEADING PRIVATE', timeout=15000)
    expect(iframe).to_have_attribute('sandbox', '')
    expect(frame.locator('img[alt="Private Section-only image"]')).to_have_attribute('src', re.compile(r'^data:image/png;base64,'))
    assert iframe.evaluate('(node) => node.contentDocument === null')
    assert snapshot() == before and before[-1] == 404
    expect(workspace).to_contain_text('Chưa lưu')
    page.screenshot(path=str(artifacts / 'shared-template-unsaved-preview.png'), full_page=True)
    checks.append('D03_new_target_dirty_preview_private_raster_opaque_sandbox_no_persistence_or_public_change')

    # An unexpected private media MIME must not be converted into trusted data.
    # Intercept only this raster fetch; the preview endpoint itself stays real.
    private_image_url = base + f'/api/admin/media/{image_id}/file'
    page.route(private_image_url, lambda route: route.fulfill(
        status=200, content_type='image/svg+xml', body='<svg xmlns="http://www.w3.org/2000/svg"/>'))
    try:
        preview()
        expect(workspace).to_contain_text('1 ảnh riêng tư chưa hiển thị', timeout=15000)
        expect(frame.locator('img[alt="Private Section-only image"]')).to_have_attribute(
            'src', f'/api/admin/media/{image_id}/file')
        expect(iframe).to_have_attribute('sandbox', '')
    finally:
        page.unroute(private_image_url)
    assert snapshot() == before
    checks.append('D03_private_preview_MIME_allowlist_rejects_non_raster')

    # Keep a real response pending at the client boundary, even if AbortSignal
    # is triggered: the request/target authority checks must also reject it.
    page.evaluate("""() => {
        const original = window.fetch;
        window.fetch = async (...args) => {
            if (!String(args[0]).endsWith('/api/admin/template-overrides/preview')) {
                return original(...args);
            }
            const response = await original(...args);
            return new Promise(resolve => {
                window.releaseD03Preview = () => {
                    window.fetch = original;
                    resolve(response);
                };
            });
        };
    }""")
    preview()
    page.wait_for_function("typeof window.releaseD03Preview === 'function'")
    open_role('footer')  # existing dialog handler accepts discarding header
    new_heading(page, 'D03 FOOTER NEWER DRAFT')
    page.evaluate('window.releaseD03Preview()')
    expect(workspace).to_contain_text('D03 FOOTER NEWER DRAFT')
    expect(page.locator('#canvas')).to_have_count(0)
    response = preview()
    assert response.request.post_data_json['target'] == {'kind': 'part', 'part_key': 'footer'}
    expect(frame.locator('body')).to_contain_text('D03 FOOTER NEWER DRAFT', timeout=15000)
    expect(frame.locator('body')).not_to_contain_text('LIBRARY COPY HEADING PRIVATE')
    assert snapshot() == before
    checks.append('D03_late_response_cannot_replace_new_target_or_draft')

    # A saved version is sent in If-Match; preview never calls the save route.
    saved = put_header(admin, base, heading, csrf)
    open_role('header')
    change_heading(page, 'D03 HEADER LOCAL UNSAVED')
    before_saved = snapshot()
    response = preview()
    assert response.request.all_headers()['if-match'] == f'"v{saved["version"]}"'
    expect(frame.locator('body')).to_contain_text('D03 HEADER LOCAL UNSAVED', timeout=15000)
    assert snapshot() == before_saved
    checks.append('D03_saved_target_preview_uses_CAS_without_mutating_saved_AST_or_public_output')

    api_json(admin, 'POST', base, '/auth/logout', csrf=csrf, expected=204)
    preview(401)
    csrf = login_ui('library_admin', 'AdminPassword123!')
    expect(workspace).to_contain_text('D03 HEADER LOCAL UNSAVED', timeout=15000)
    expect(workspace).to_contain_text('Chưa lưu')
    assert preview().request.all_headers()['if-match'] == f'"v{saved["version"]}"'
    expect(frame.locator('body')).to_contain_text('D03 HEADER LOCAL UNSAVED')
    assert snapshot() == before_saved
    checks.append('D03_401_same_account_relogin_recovers_dirty_target_and_version')

    remote = copy.deepcopy(heading)
    remote['sections'][0]['children'][0]['children'][0]['children'][0]['props']['text'] = 'D03 REMOTE HEADER'
    current = put_header(admin, base, remote, csrf, saved['version'])
    before_stale = snapshot()
    response = preview(412)
    assert response.request.all_headers()['if-match'] == f'"v{saved["version"]}"'
    expect(workspace).to_contain_text('412', timeout=15000)
    expect(workspace).to_contain_text('Chưa lưu')
    page.get_by_role('button', name='Canvas cấu trúc', exact=True).click()
    expect(workspace).to_contain_text('D03 HEADER LOCAL UNSAVED')
    assert snapshot() == before_stale
    checks.append('D03_412_retains_stale_local_AST_and_remote_persisted_state')

    api_json(admin, 'POST', base, '/api/admin/users', {
        'username': 'library_preview_admin', 'password': 'PreviewPassword123!', 'role': 'admin',
    }, csrf, expected=201)
    api_json(admin, 'POST', base, '/auth/logout', csrf=csrf, expected=204)
    preview(401)
    csrf = login_ui('library_preview_admin', 'PreviewPassword123!')
    expect(workspace).to_contain_text('Đã mở', timeout=15000)
    expect(workspace).not_to_contain_text('D03 HEADER LOCAL UNSAVED')
    expect(page.locator('#canvas')).to_have_count(0)
    expect(workspace).to_contain_text('D03 REMOTE HEADER')
    assert preview().request.all_headers()['if-match'] == f'"v{current["version"]}"'
    expect(frame.locator('body')).to_contain_text('D03 REMOTE HEADER')
    assert snapshot() == before_stale
    api_json(admin, 'POST', base, '/auth/logout', csrf=csrf, expected=204)
    preview(401)
    csrf = login_ui('library_admin', 'AdminPassword123!')
    expect(workspace).to_contain_text('Đã mở', timeout=15000)
    api_json(admin, 'POST', base, f'/api/admin/template-overrides/{current["id"]}/reset',
             csrf=csrf, extra={'If-Match': f'"v{current["version"]}"'})
    assert get_header(admin, base) is None
    checks.append('D03_cross_account_login_clears_previous_preview_and_draft')
    return csrf


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

                    # Actual V3 theme catalog lists typed targets read-only.
                    uploaded = admin.request.post(base + '/api/admin/themes', data=zip_v3(), headers={
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
                    assert 'part:header' in roles and 'part:footer' in roles
                    assert all(not item['can_apply'] for item in roles.values())
                    assert all('document' not in item for item in catalog['theme_templates'])
                    checks.append('active_theme_local_validated_descriptors_supported_and_unsupported_roles')

                    # Typed part reset keeps confirmation and the row CAS.
                    heading = {'schema_version': 1, 'sections': [{
                        'id': 'theme-header', 'children': [{'id': 'theme-row', 'children': [
                            {'id': 'theme-col', 'children': [{'id': 'theme-heading',
                                'type': 'core/heading', 'props': {'level': 2, 'text': 'CUSTOM HEADER'}}]},
                        ]}]},
                    ]}
                    header = put_header(admin, base, heading, csrf)
                    reset_path = f'/api/admin/template-overrides/{header["id"]}/reset'
                    def reset_headers(version, origin=base, token=None):
                        headers = {'Origin': origin, 'If-Match': f'"v{version}"'}
                        if token:
                            headers['X-CSRF-Token'] = token
                        return headers
                    assert anon.request.post(base + reset_path, headers=reset_headers(header['version'])).status == 401
                    assert admin.request.post(base + reset_path, headers=reset_headers(header['version'])).status == 403
                    assert admin.request.post(base + reset_path, headers=reset_headers(
                        header['version'], 'https://cross-site.invalid', csrf)).status == 403
                    assert admin.request.post(base + '/api/admin/library/apply/header',
                        headers=reset_headers(header['version'], token=csrf)).status == 404
                    checks.append('typed_reset_anonymous_CSRF_Origin_denied_and_retired_apply_404')
                    page.reload(wait_until='domcontentloaded')
                    button = page.get_by_role('button', name='Reset override')
                    expect(button).to_be_visible(timeout=15000)
                    dialogs = []
                    def decline(dialog):
                        dialogs.append(dialog.message)
                        dialog.dismiss()
                    page.on('dialog', decline)
                    button.click()
                    assert dialogs and 'Reset' in dialogs[-1]
                    assert get_header(admin, base)['version'] == header['version']
                    page.remove_listener('dialog', decline)
                    checks.append('typed_reset_cancel_does_not_reset_live_override')

                    def race_before_accept(dialog):
                        current = get_header(admin, base)
                        changed = copy.deepcopy(heading)
                        changed['sections'][0]['children'][0]['children'][0]['children'][0]['props']['text'] = 'REMOTE HEADER WINS'
                        put_header(admin, base, changed, csrf, current['version'])
                        dialog.accept()

                    page.on('dialog', race_before_accept)
                    with page.expect_response(lambda r: r.url.endswith(reset_path) and r.request.method == 'POST') as stale_override:
                        button.click()
                    assert stale_override.value.status == 412
                    expect(page.get_by_role('status')).to_contain_text('412', timeout=12000)
                    assert 'REMOTE HEADER WINS' in json.dumps(get_header(admin, base))
                    page.remove_listener('dialog', race_before_accept)
                    checks.append('typed_reset_stale_row_CAS_preserves_remote_override')

                    # Active theme can change independently of the site-owned
                    # row: inspection and cancellation preserve the AST. Reset
                    # now resolves the active revision, not a stale catalog role.
                    other = admin.request.post(base + '/api/admin/themes',
                        data=zip_v3('mac-proof-switch'), headers={
                            'Origin': base, 'X-CSRF-Token': csrf, 'Content-Type': 'application/zip',
                        })
                    assert other.status == 201, other.text()[:300]
                    current_theme = api_json(admin, 'GET', base, '/api/admin/themes')
                    api_json(admin, 'PUT', base, '/api/admin/themes/active',
                        {'revision_id': other.json()['revision_id']}, csrf,
                        {'If-Match': f'"v{current_theme["version"]}"'})
                    assert 'REMOTE HEADER WINS' in json.dumps(get_header(admin, base))
                    page.reload(wait_until='domcontentloaded')
                    expect(page.get_by_text('mac-proof-switch', exact=False).first).to_be_visible(timeout=15000)
                    page.on('dialog', lambda dialog: dialog.accept())
                    with page.expect_response(lambda r: r.url.endswith(reset_path) and r.request.method == 'POST') as applied:
                        button.click()
                    assert applied.value.status == 200
                    assert applied.value.json()['next_effective_source']['kind'] == 'active_theme'
                    assert applied.value.json()['next_effective_source']['target_key'] == 'part:header'
                    expect(page.get_by_role('status')).to_contain_text('Đã reset', timeout=12000)
                    assert get_header(admin, base) is None
                    current_theme = api_json(admin, 'GET', base, '/api/admin/themes')
                    api_json(admin, 'PUT', base, '/api/admin/themes/active',
                        {'revision_id': uploaded.json()['revision_id']}, csrf,
                        {'If-Match': f'"v{current_theme["version"]}"'})
                    checks.append('theme_switch_preserves_site_AST_then_reset_resolves_current_active_part')

                    csrf = shared_preview_checks(page, admin, anon, base, csrf, source_path,
                                                 image_id, heading, artifacts, checks)

                    page_id = ui_create_page(page, base, 'Library destination Page', 'library-destination',
                                             'Library fallback Page body')
                    page_initial = api_json(admin, 'GET', base, page_scope(page_id))
                    page.get_by_role('button', name='Mở Visual Builder').click()
                    expect(page.get_by_role('heading', name=f'Page Builder #{page_id}')).to_be_visible(timeout=15000)
                    page.get_by_test_id('builder-local-library').get_by_role('button', name='Chọn Section', exact=True).click()
                    picker = page.get_by_test_id('builder-local-library')
                    expect(picker.get_by_text('UI library banner', exact=False)).to_be_visible(timeout=12000)
                    for attempt in (1, 2):
                        picker.get_by_role('button', name='Chèn bản sao').click()
                        expect(page.get_by_test_id('builder-workspace')).to_contain_text(
                            'Đã chèn bản SAO', timeout=15000)
                        assert api_json(admin, 'GET', base, page_scope(page_id))['document'] is None
                        assert api_json(admin, 'GET', base, source_path)['document'] == source_document
                    page.screenshot(path=str(artifacts / 'two-copies-unsaved.png'), full_page=True)
                    assert anon.request.get(base + f'/media/{image_id}/file').status == 404
                    assert anon.request.get(base + '/content/page/library-destination').status == 404
                    checks.append('two_unique_UI_copy_actions_unsaved_no_public_side_effects')

                    page.get_by_role('button', name='Xem trước').click()
                    expect(page.frame_locator('#canvas').locator('body')).to_contain_text(
                        'LIBRARY COPY HEADING PRIVATE', timeout=15000)
                    assert api_json(admin, 'GET', base, page_scope(page_id))['document'] is None
                    checks.append('authenticated_unsaved_copy_preview_no_persist_or_publish')
                    page.get_by_role('button', name='Lưu Page').click()
                    expect(page.get_by_test_id('builder-workspace')).to_contain_text('Đã lưu', timeout=15000)
                    saved = api_json(admin, 'GET', base, page_scope(page_id))
                    assert saved['version'] == page_initial['version'] + 1 and saved['content_mode'] == 'builder'
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
                    assert editor.request.get(base + '/api/admin/template-overrides').status == 403
                    assert editor.request.get(base + '/api/admin/builder/homepage').status == 403
                    denied_header = put_header(admin, base, heading, csrf)
                    denied_reset = f'/api/admin/template-overrides/{denied_header["id"]}/reset'
                    assert editor.request.post(base + denied_reset, headers=reset_headers(
                        denied_header['version'], token=editor_csrf)).status == 403
                    assert editor.request.put(base + '/api/admin/template-overrides',
                        data=json.dumps({"target":{"kind":"part","part_key":"header"},"document":heading}), headers={
                            'Origin':base, 'X-CSRF-Token':editor_csrf, 'Content-Type':'application/json',
                            'If-Match':f'"v{denied_header["version"]}"',
                        }).status == 403
                    assert get_header(admin, base)['id'] == denied_header['id']
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

                    # Switch away from the V3 provider to a theme without parts. The old header provider is withdrawn;
                    # no unpublished Section or Page AST may be mutated.
                    alternative = admin.request.post(base + '/api/admin/themes',
                        data=zip_v3('library-fallback-theme', parts=False), headers={
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
                    assert not any(role['role'] == 'part:header' for role in fallback['theme_templates'])
                    assert all(not role['can_apply'] for role in fallback['theme_templates'])
                    assert get_header(admin, base)['ast_json'] == heading
                    retired_apply = admin.request.post(base + '/api/admin/library/apply/header',
                        headers=reset_headers(denied_header['version'], token=csrf))
                    assert retired_apply.status == 404
                    reset_to_core = api_json(admin, 'POST', base, denied_reset, csrf=csrf,
                        extra={'If-Match': f'"v{denied_header["version"]}"'})
                    assert reset_to_core['next_effective_source']['kind'] == 'core_fallback'
                    assert get_header(admin, base) is None
                    checks.append('withdrawn_theme_catalog_fallback_preserves_source_and_copy_nodes')
                    checks.append('retired_apply_404_and_typed_reset_current_core_fallback')

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
