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
from m4_s11_t01_page_builder_e2e import ui_create_page, new_heading, change_heading, page_scope
from media_smoke import make_png


def create_ui_pattern(page, base: str, title: str, slug: str) -> int:
    page.goto(base + '/admin/#/patterns/new', wait_until='domcontentloaded')
    expect(page.get_by_test_id('pattern-workspace')).to_be_visible(timeout=20000)
    page.get_by_label('Tiêu đề Pattern').fill(title)
    page.get_by_label('Slug Pattern').fill(slug)
    page.get_by_role('button', name='Lưu Pattern').click()
    expect(page).to_have_url(re.compile(r'#/patterns/[1-9]\d*/edit$'), timeout=20000)
    return int(re.search(r'#/patterns/(\d+)/edit', page.url).group(1))


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def zip_v3(theme_id='library-theme', *, parts=True) -> bytes:
    banner_pattern = {
        'schema_version': 1,
        'sections': [{
            'id': 'pat-sec',
            'children': [{
                'id': 'pat-row',
                'children': [{
                    'id': 'pat-col',
                    'children': [{
                        'id': 'pat-head',
                        'type': 'core/heading',
                        'props': {'level': 2, 'text': 'THEME BANNER COPY'},
                    }],
                }],
            }],
        }],
    }
    manifest = {
        'manifest_version': 3,
        'theme_id': theme_id,
        'name': theme_id,
        'version': '1.0.0',
        'templates': [{'kind': 'home', 'file': 'home.html'}, {'kind': 'single', 'file': 'single.html'}],
        'patterns': [{
            'key': 'banner',
            'title': 'Theme Banner Copy',
            'category': 'cta',
            'description': 'Theme pattern copy source',
            'file': 'patterns/banner.json',
        }],
    }
    members = {
        'home.html': '<main>THEME HOME{{ od_content() }}</main>',
        'single.html': '<main>{{ od_content() }}</main>',
        'patterns/banner.json': json.dumps(banner_pattern),
    }
    if parts:
        manifest['parts'] = {'header': 'header.html', 'footer': 'footer.html'}
        members.update({
            'header.html': '<header>THEME HEADER</header>',
            'footer.html': '<footer>THEME FOOTER</footer>',
        })
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr('theme.json', json.dumps(manifest))
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
    """Exercise the actual Studio Mode 1 preview request before any destination Save."""
    preview_path = '/api/admin/studio/preview'
    workspace = page.get_by_test_id('studio-workspace')

    def open_role(role):
        page.goto(base + f'/admin/#/studio?panel=templates&template=part%3A{role}', wait_until='domcontentloaded')
        expect(page.get_by_test_id('studio-panel-templates')).to_be_visible(timeout=15000)
        ready = page.get_by_test_id('studio-init-template-override').or_(
            page.get_by_role('button', name='Thêm Block').first
        )
        expect(ready).to_be_visible(timeout=15000)
        init_btn = page.get_by_test_id('studio-init-template-override')
        if init_btn.count() > 0:
            init_btn.click()

    def expect_studio_preview(action, status=200):
        with page.expect_response(
            lambda r: r.url.endswith(preview_path) and r.request.method == 'POST',
            timeout=20000,
        ) as pending:
            action()
        response = pending.value
        assert response.status == status, response.text()[:400]
        return response

    def snapshot():
        public = anon.request.get(base + '/')
        assert public.status == 200
        return (
            api_json(admin, 'GET', base, '/api/admin/template-overrides'),
            api_json(admin, 'GET', base, source_path),
            public.text(),
            anon.request.get(base + f'/media/{image_id}/file').status,
        )

    def login_ui(username, password):
        expect(page.locator('#auth-username')).to_be_visible(timeout=15000)
        expect(workspace).not_to_be_visible()
        page.locator('#auth-username').fill(username)
        page.locator('#auth-password').fill(password)
        page.locator('#auth-login').click()
        expect(page.get_by_test_id('studio-workspace')).to_be_visible(timeout=20000)
        return api_json(admin, 'GET', base, '/auth/me')['csrf_token']

    assert get_header(admin, base) is None
    before = snapshot()
    open_role('header')
    response = expect_studio_preview(lambda: new_heading(page, 'HEADER PREVIEW DRAFT'))
    payload = response.request.post_data_json
    assert payload['target'] == {'kind': 'part', 'part_key': 'header'}
    assert response.headers.get('cache-control') == 'no-store'
    assert response.headers.get('x-content-type-options') == 'nosniff'
    iframe = page.locator('#canvas')
    frame = page.frame_locator('#canvas')
    expect(frame.locator('body')).to_contain_text('HEADER PREVIEW DRAFT', timeout=15000)
    expect(iframe).to_have_attribute('sandbox', 'allow-scripts')
    assert iframe.evaluate('(node) => node.contentDocument === null')
    assert snapshot() == before and before[-1] == 404
    expect(page.get_by_test_id('studio-dirty-badge')).to_be_visible()
    page.screenshot(path=str(artifacts / 'shared-template-unsaved-preview.png'), full_page=True)
    checks.append('D03_new_target_dirty_preview_opaque_sandbox_no_persistence_or_public_change')

    # Saved override 401 re-login recovers unsaved Studio draft for same user,
    # while cross-account re-login clears previous user's unsaved AST.
    saved = put_header(admin, base, heading, csrf)
    def accept_any(dialog):
        try:
            dialog.accept()
        except Exception:
            pass
    page.on('dialog', accept_any)
    page.evaluate("location.hash = '#/dashboard'")
    expect(page.locator('#dashboard-main')).to_be_visible(timeout=15000)
    page.reload(wait_until='domcontentloaded')
    expect(page.locator('#dashboard-main')).to_be_visible(timeout=15000)
    page.remove_listener('dialog', accept_any)
    open_role('header')
    change_heading(page, 'D03 HEADER LOCAL UNSAVED')
    before_saved = snapshot()
    expect(page.get_by_test_id('studio-dirty-badge')).to_be_visible(timeout=15000)
    expect(frame.locator('body')).to_contain_text('D03 HEADER LOCAL UNSAVED', timeout=15000)
    api_json(admin, 'POST', base, '/auth/logout', csrf=csrf, expected=204)
    page.get_by_test_id('studio-save-btn').click()
    csrf = login_ui('library_admin', 'AdminPassword123!')
    expect(page.get_by_test_id('studio-dirty-badge')).to_be_visible(timeout=15000)
    expect(frame.locator('body')).to_contain_text('D03 HEADER LOCAL UNSAVED', timeout=15000)
    assert snapshot() == before_saved
    checks.append('D03_401_same_account_relogin_recovers_dirty_target_and_version')

    remote = copy.deepcopy(heading)
    remote['sections'][0]['children'][0]['children'][0]['children'][0]['props']['text'] = 'D03 REMOTE HEADER'
    current = put_header(admin, base, remote, csrf, saved['version'])
    before_stale = snapshot()
    page.get_by_test_id('studio-save-btn').click()
    expect(page.get_by_test_id('studio-sidebar-alert')).to_contain_text('412', timeout=15000)
    expect(page.get_by_label('Tiêu đề', exact=True)).to_have_value('D03 HEADER LOCAL UNSAVED')
    assert snapshot() == before_stale
    checks.append('D03_412_retains_stale_local_AST_and_remote_persisted_state')

    api_json(admin, 'POST', base, '/api/admin/users', {
        'username': 'library_preview_admin', 'password': 'PreviewPassword123!', 'role': 'admin',
    }, csrf, expected=201)
    api_json(admin, 'POST', base, '/auth/logout', csrf=csrf, expected=204)
    page.get_by_test_id('studio-save-btn').click()
    csrf = login_ui('library_preview_admin', 'PreviewPassword123!')
    expect(page.get_by_test_id('studio-dirty-badge')).to_have_count(0, timeout=15000)
    expect(frame.locator('body')).not_to_contain_text('D03 HEADER LOCAL UNSAVED')
    expect(frame.locator('body')).to_contain_text('D03 REMOTE HEADER', timeout=15000)
    assert snapshot() == before_stale
    api_json(admin, 'POST', base, '/auth/logout', csrf=csrf, expected=204)
    page.reload(wait_until='domcontentloaded')
    csrf = login_ui('library_admin', 'AdminPassword123!')
    api_json(admin, 'POST', base, f'/api/admin/template-overrides/{current["id"]}/reset',
             csrf=csrf, extra={'If-Match': f'"v{current["version"]}"'})
    assert get_header(admin, base) is None
    checks.append('D03_cross_account_login_clears_previous_preview_and_draft')
    return csrf


def run(binary: pathlib.Path, artifacts: pathlib.Path) -> dict:
    checks: list[str] = []
    errors: list[str] = []
    source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    with tempfile.TemporaryDirectory(prefix='opendash-m5-s12-t01-', ignore_cleanup_errors=True) as temp:
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
                    csrf = admin_login(admin, base, 'library_admin', 'AdminPassword123!', cms_locale="vi")
                    assert admin.request.get(base + '/api/admin/library').headers.get('cache-control') == 'no-store'
                    catalog = api_json(admin, 'GET', base, '/api/admin/library')
                    assert catalog['patterns'] == []
                    assert catalog['theme_state'] == 'active'
                    checks.append('catalog_authentication_and_empty_local_no_cloud_fallback')

                    source_id = create_ui_pattern(page, base, 'UI library banner', 'library-banner')
                    page.get_by_role('button', name='Mở Pattern Builder').click()
                    expect(page).to_have_url(base + f'/admin/#/studio/entry/pattern/{source_id}', timeout=15000)
                    expect(page.get_by_test_id('studio-mode2-sidebar')).to_be_visible(timeout=15000)
                    new_heading(page, 'LIBRARY COPY HEADING PRIVATE')
                    page.get_by_test_id('studio-save-btn').click()
                    expect(page.get_by_test_id('studio-dirty-badge')).to_have_count(0, timeout=15000)
                    source_path = f'/api/admin/entries/pattern/{source_id}/builder'
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
                        'props': {'media_id': image_id, 'alt': 'Private Pattern-only image'},
                    })
                    api_json(admin, 'PUT', base, source_path, source_document, csrf,
                             {'If-Match': f'"v{source_open["version"]}"'})
                    assert anon.request.get(base + f'/media/{image_id}/file').status == 404
                    assert anon.request.get(base + '/content/pattern/library-banner').status == 404
                    checks.append('UI_created_private_Pattern_with_image_never_public')

                    page.goto(base + '/admin/#/appearance/library', wait_until='domcontentloaded')
                    expect(page.get_by_test_id('local-template-library')).to_be_visible(timeout=15000)
                    expect(page.get_by_test_id('local-template-library').get_by_text('UI library banner')).to_be_visible(timeout=15000)
                    assert page.get_by_test_id('pending-library').count() == 0
                    assert api_json(admin, 'GET', base, '/api/admin/library')['patterns'][0]['id'] == source_id
                    assert 'document' not in api_json(admin, 'GET', base, '/api/admin/library')['patterns'][0]
                    page.screenshot(path=str(artifacts / 'library-list.png'), full_page=True)
                    checks.append('real_library_navigation_and_read_only_Pattern_metadata_no_AST_leak')

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
                    page.get_by_test_id('template-library-tabs-overrides').click()
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
                    page.get_by_test_id('template-library-tabs-theme').click()
                    expect(page.get_by_text('mac-proof-switch', exact=False).first).to_be_visible(timeout=15000)
                    page.get_by_test_id('template-library-tabs-overrides').click()
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
                                             '')
                    page_initial = api_json(admin, 'GET', base, page_scope(page_id))
                    page.get_by_test_id('open-ux-builder-toolbar').click()
                    expect(page).to_have_url(base + f'/admin/#/studio/entry/page/{page_id}', timeout=15000)
                    expect(page.get_by_test_id('studio-mode2-sidebar')).to_be_visible(timeout=15000)
                    footer = page.get_by_test_id('studio-sidebar-footer')
                    for _attempt in (1, 2):
                        footer.get_by_test_id('studio-open-pattern-library').click()
                        page.get_by_test_id('insert-theme-pattern-banner').click()
                        expect(page.get_by_test_id('studio-dirty-badge')).to_be_visible(timeout=15000)
                        assert api_json(admin, 'GET', base, page_scope(page_id))['document'] is None
                        assert api_json(admin, 'GET', base, source_path)['document'] == source_document
                    page.screenshot(path=str(artifacts / 'two-copies-unsaved.png'), full_page=True)
                    assert anon.request.get(base + f'/media/{image_id}/file').status == 404
                    assert anon.request.get(base + '/library-destination').status == 404
                    checks.append('two_unique_UI_copy_actions_unsaved_no_public_side_effects')

                    expect(page.frame_locator('#canvas').locator('body')).to_contain_text(
                        'THEME BANNER COPY', timeout=15000)
                    assert api_json(admin, 'GET', base, page_scope(page_id))['document'] is None
                    checks.append('authenticated_unsaved_copy_preview_no_persist_or_publish')
                    page.get_by_test_id('studio-save-draft-btn').click()
                    expect(page.get_by_test_id('studio-dirty-badge')).to_have_count(0, timeout=15000)
                    saved = api_json(admin, 'GET', base, page_scope(page_id))
                    assert saved['version'] == page_initial['version'] + 1 and saved['content_mode'] == 'builder'
                    copied = saved['document']['sections']
                    assert len(copied) == 2 and copied[0] != copied[1]
                    copied_ids = [node['id'] for sec in copied for node in [sec]
                                  + sec['children'] + [col for row in sec['children'] for col in row['children']]
                                  + [el for row in sec['children'] for col in row['children'] for el in col['children']]]
                    assert len(copied_ids) == len(set(copied_ids))
                    assert api_json(admin, 'GET', base, source_path)['document'] == source_document
                    assert anon.request.get(base + f'/media/{image_id}/file').status == 404
                    checks.append('saved_draft_contains_fresh_ID_tree_twice_source_preserved_media_private')

                    entry = api_json(admin, 'GET', base, f'/api/admin/content-types/page/entries/{page_id}')
                    api_json(admin, 'POST', base,
                             f'/api/admin/content-types/page/entries/{page_id}/publication',
                             {'status': 'published'}, csrf, {'If-Match': f'"v{entry["version"]}"'})
                    public = anon.request.get(base + '/library-destination')
                    assert public.status == 200, public.text()[:300]
                    assert public.text().count('THEME BANNER COPY') == 2
                    checks.append('only_explicit_published_destination_grants_public_output')

                    # Editor can open own Page Studio, cannot access shared overrides,
                    # global Studio, or Admin-only Pattern Studio.
                    api_json(admin, 'POST', base, '/api/admin/users', {
                        'username': 'library_editor', 'password': 'EditorPassword123!', 'role': 'editor',
                    }, csrf, expected=201)
                    editor = browser.new_context(viewport={'width': 390, 'height': 844})
                    editor_csrf = admin_login(editor, base, 'library_editor', 'EditorPassword123!')
                    editor_catalog = api_json(editor, 'GET', base, '/api/admin/library')
                    assert len(editor_catalog['patterns']) == 1
                    assert editor_catalog['theme_templates'] == [] and editor_catalog['theme_state'] == 'admin_only'
                    assert editor.request.get(base + '/api/admin/template-overrides').status == 403
                    assert editor.request.get(base + '/api/admin/studio/targets').status == 403
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
                    ep.goto(base + f'/admin/#/studio/entry/page/{page_id}',
                            wait_until='domcontentloaded')
                    expect(ep.get_by_test_id('studio-mode2-sidebar')).to_be_visible(timeout=15000)
                    ep.get_by_test_id('studio-open-pattern-library').click()
                    ep.get_by_test_id('insert-theme-pattern-banner').click()
                    expect(ep.get_by_test_id('studio-dirty-badge')).to_be_visible(timeout=15000)
                    ep.screenshot(path=str(artifacts / 'editor-copy-390.png'), full_page=True)
                    assert ep.evaluate('document.body.scrollWidth <= innerWidth + 2'), '390px horizontal overflow'
                    assert api_json(admin, 'GET', base, page_scope(page_id))['document'] == saved['document']
                    editor.close()
                    checks.append('Editor_allowed_scoped_copy_no_theme_authority_390px_unsaved')

                    # Switch away from the V3 provider to a theme without parts. The old header provider is withdrawn;
                    # no unpublished Pattern or Page AST may be mutated.
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
                        ('patterns', '#/patterns'),
                        ('local-library', '#/appearance/library'),
                        ('appearance-theme', '#/appearance/themes'),
                        ('extensions-plugins', '#/plugins'),
                        ('exact-page-builder', f'#/studio/entry/page/{page_id}'),
                    ]
                    for name, hash_route in routes:
                        page.goto(base + '/admin/' + hash_route, wait_until='domcontentloaded')
                        if name == 'post-table':
                            expect(page.locator('section[aria-label="Bài viết"]')).to_be_visible(timeout=16000)
                        elif name == 'page-table':
                            expect(page.locator('section[aria-label="Trang"]')).to_be_visible(timeout=16000)
                        elif name == 'local-library':
                            expect(page.get_by_test_id('local-template-library')).to_be_visible(timeout=16000)
                        elif name == 'patterns':
                            expect(page.get_by_test_id('pattern-workspace')).to_be_visible(timeout=16000)
                        elif name == 'exact-page-builder':
                            expect(page.get_by_test_id('studio-workspace')).to_be_visible(timeout=16000)
                            expect(page.get_by_test_id('studio-mode2-sidebar')).to_be_visible(timeout=16000)
                        else:
                            expect(page.locator('body')).to_be_visible(timeout=16000)
                        page.wait_for_load_state('networkidle', timeout=15000)
                        assert page.url.endswith(hash_route), (name, page.url)
                        if name == 'local-library':
                            expect(page.get_by_test_id('local-template-library').get_by_text('UI library banner')).to_be_visible(timeout=12000)
                        if name == 'patterns':
                            expect(page.get_by_test_id('pattern-workspace').get_by_text('UI library banner')).to_be_visible(timeout=12000)
                        page.screenshot(path=str(artifacts / f'owner-note-{name}.png'), full_page=True)
                    checks.append('owner_note_Post_Page_terms_media_tables_extensions_Studio_Pattern_library_routes_screenshots')

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
                    assert api_json(ctx, 'GET', base2, '/api/admin/library')['patterns'][0]['id'] == source_id
                    ctx.close()
                finally:
                    browser.close()
        finally:
            stop_server(resumed)
        checks.append('native_server_restart_preserves_local_catalog_and_page_copy_AST')
    return {'status': 'PASS', 'source_sha': source, 'binary_sha256': digest(binary),
            'platform': platform.platform(), 'checks': checks, 'chrome_js_errors': errors}


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
