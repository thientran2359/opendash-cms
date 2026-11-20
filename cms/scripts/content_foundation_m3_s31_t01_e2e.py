#!/usr/bin/env python3
"""M3-S31-T01 native macOS G5: CPT policies on an isolated temporary site.

Usage: python3 cms/scripts/content_foundation_m3_s31_t01_e2e.py \
    --binary cms/target/debug/opendash --artifact-dir /private/path

The owner review site at port 18964 is never used or modified.
"""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import pathlib
import platform
import sqlite3
import subprocess
import tempfile
import threading
import traceback
from urllib.error import HTTPError
from urllib.request import Request, urlopen

from playwright.sync_api import expect, sync_playwright
from m7_beta_menu_picker_e2e import api_json, admin_login, launch_server, stop_server
from media_smoke import make_png


def sha(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def request(ctx, base, method, path, payload=None, csrf=None, version=None):
    headers = {}
    if method != 'GET':
        headers['Origin'] = base
    if csrf:
        headers['X-CSRF-Token'] = csrf
    if version is not None:
        headers['If-Match'] = f'"v{version}"'
    if payload is not None:
        headers['Content-Type'] = 'application/json'
    return ctx.request.fetch(base + path, method=method, headers=headers,
                             data=json.dumps(payload) if payload is not None else None, timeout=20000)


def checked_status(ctx, base, method, path, status, payload=None, csrf=None, version=None):
    response = request(ctx, base, method, path, payload, csrf, version)
    assert response.status == status, (method, path, response.status, response.text()[:400])
    return response


def descriptor(key, label, *, settings=None, fields=None):
    result = {'key': key, 'label': label, 'fields': fields or []}
    if settings is not None:
        result['settings'] = settings
    return result


def policy(public=True, archive=True, publication='immediate', supports=None):
    return {'public': public, 'has_archive': archive, 'publication': publication,
            'supports': ['editor', 'featured_image'] if supports is None else supports}


def concurrent_puts(base, cookie, csrf, commands):
    """Real competing HTTP requests using separate sockets and the same CAS."""
    barrier = threading.Barrier(len(commands))

    def put(command):
        path, payload, version = command
        headers = {'Content-Type': 'application/json', 'Origin': base,
                   'Cookie': cookie, 'X-CSRF-Token': csrf}
        if version is not None:
            headers['If-Match'] = f'"v{version}"'
        req = Request(base + path, data=json.dumps(payload).encode('utf-8'),
                      method='PUT', headers=headers)
        barrier.wait(timeout=5)
        try:
            with urlopen(req, timeout=20) as response:
                return response.status
        except HTTPError as error:
            with error:
                return error.code

    with ThreadPoolExecutor(max_workers=len(commands)) as pool:
        return list(pool.map(put, commands))


def run(binary: pathlib.Path, artifact_dir: pathlib.Path) -> list[str]:
    checks = []
    with tempfile.TemporaryDirectory(prefix='opendash-c1-g5-') as tmp:
        # macOS /var is a symlink to /private/var; the runtime requires a
        # fully resolved root to reject malicious symlink traversal safely.
        root = pathlib.Path(tmp).resolve() / 'site'
        root.mkdir(mode=0o700)
        server, base = launch_server(binary, root)
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel='chrome', headless=True)
                try:
                    ctx = browser.new_context(viewport={'width': 1440, 'height': 900})
                    password = 'C1-Local-Only-Admin-Password-2026!'
                    api_json(ctx, 'POST', base, '/auth/setup',
                             {'username': 'c1_admin', 'password': password}, expected=201)
                    csrf = admin_login(ctx, base, 'c1_admin', password)
                    types = api_json(ctx, 'GET', base, '/api/admin/content-types')
                    schema = {entry['descriptor']['key']: entry for entry in types}
                    assert schema['post']['descriptor']['settings']['publication'] == 'draft_publish'
                    assert schema['page']['descriptor']['settings']['has_archive'] is True
                    assert schema['section']['descriptor']['settings']['public'] is False
                    checks.append('builtin_post_page_section_effective_settings')

                    guest = browser.new_context()
                    checked_status(guest, base, 'GET', '/api/admin/content-types', 401)
                    checked_status(guest, base, 'GET',
                                   '/api/admin/content-types/post/entries/1/preview', 401)
                    guest.close()
                    api_json(ctx, 'POST', base, '/api/admin/users',
                             {'username': 'c1_editor', 'password': 'C1-Editor-Test-Only-2026!', 'role': 'editor'},
                             csrf, expected=201)
                    editor = browser.new_context()
                    editor_csrf = admin_login(editor, base, 'c1_editor', 'C1-Editor-Test-Only-2026!')
                    checked_status(editor, base, 'PUT', '/api/admin/content-types/post', 403,
                                   schema['post']['descriptor'], editor_csrf, version=schema['post']['version'])
                    editor.close()
                    checks.append('guest_401_editor_forbidden_schema_write_403')

                    # Real Admin interaction: create and configure a draft CPT,
                    # save/publish through the editor rather than only API calls.
                    ui = ctx.new_page()
                    ui_errors = []
                    ui.on('pageerror', lambda error: ui_errors.append(str(error)))
                    ui.goto(base + '/admin/#/settings/content-types', wait_until='domcontentloaded')
                    expect(ui.get_by_test_id('content-type-schema-settings')).to_be_visible(timeout=20000)
                    ui.get_by_role('button', name='+ Tạo loại nội dung mới').click()
                    ui.locator('#content-type-key').fill('ui_c1')
                    ui.locator('#content-type-label').fill('UI CPT C1')
                    ui.locator('#new-cpt-publication').select_option('draft_publish')
                    ui.locator('#new-cpt-archive').uncheck()
                    ui.get_by_role('button', name='Tạo loại nội dung', exact=True).click()
                    expect(ui.get_by_role('button', name='UI CPT C1 (ui_c1)')).to_be_visible(timeout=20000)
                    created_ui = next(t for t in api_json(ctx, 'GET', base, '/api/admin/content-types')
                                      if t['descriptor']['key'] == 'ui_c1')
                    assert created_ui['descriptor']['settings']['publication'] == 'draft_publish'
                    assert created_ui['descriptor']['settings']['has_archive'] is False
                    checks.append('chrome_admin_create_draft_publish_cpt_with_archive_disabled')

                    ui.goto(base + '/admin/#/cpt/ui_c1/new', wait_until='domcontentloaded')
                    expect(ui.locator('#entry-title-input')).to_be_visible(timeout=20000)
                    ui.locator('#entry-title-input').fill('C1 UI Draft Article')
                    ui.locator('#entry-slug-input').fill('c1-ui-article')
                    ui.get_by_role('group', name='Lưu nháp và xuất bản').get_by_role(
                        'button', name='Lưu nháp').click()
                    expect(ui).to_have_url(__import__('re').compile(r'/cpt/ui_c1/\d+/edit'), timeout=20000)
                    ui_items = api_json(ctx, 'GET', base, '/api/admin/content-types/ui_c1/entries')
                    ui_entry = next(entry for entry in ui_items if entry['slug'] == 'c1-ui-article')
                    assert ui_entry['publication_status'] == 'draft'
                    checked_status(ctx, base, 'GET', '/content/ui_c1/c1-ui-article', 404)
                    assert ui.get_by_role('link', name='Xem nội dung').count() == 0
                    ui.get_by_role('group', name='Lưu nháp và xuất bản').get_by_role(
                        'button', name='Đăng bài').click()
                    expect(ui.get_by_role('group', name='Lưu nháp và xuất bản').get_by_text(
                        'Đã xuất bản')).to_be_visible(timeout=20000)
                    assert ui.get_by_role('link', name='Xem nội dung').count() >= 1
                    checked_status(ctx, base, 'GET', '/content/ui_c1/c1-ui-article', 200)
                    checked_status(ctx, base, 'GET', '/content/ui_c1', 404)
                    checks.append('chrome_admin_editor_save_draft_publish_and_public_view')

                    ui.goto(base + '/admin/#/settings/content-types', wait_until='domcontentloaded')
                    ui.get_by_role('button', name='UI CPT C1 (ui_c1)').click()
                    form = ui.get_by_role('form', name='Cấu hình loại nội dung')
                    expect(form.get_by_label('Cơ chế xuất bản')).to_have_value('draft_publish')
                    form.get_by_label('Công khai nội dung (public)').uncheck()
                    form.get_by_role('button', name='Lưu cấu hình').click()
                    expect(form.get_by_label('Công khai nội dung (public)')).not_to_be_checked(timeout=20000)
                    checked_status(ctx, base, 'GET', '/content/ui_c1/c1-ui-article', 404)
                    ui.set_viewport_size({'width': 390, 'height': 844})
                    ui.screenshot(path=str(artifact_dir / 'c1-ui-settings-390.png'), full_page=True)
                    assert ui.evaluate('document.documentElement.scrollWidth <= window.innerWidth')
                    assert not ui_errors, ui_errors
                    checks.append('chrome_admin_settings_cas_public_toggle_390px_no_overflow')
                    ui.close()

                    # Post/Page keep their fixed schema and draft policy, while
                    # administrators may change C1 visibility and support flags.
                    post = schema['post']
                    post_entry = api_json(ctx, 'POST', base, '/api/admin/content-types/post/entries',
                                          {'title': 'Supported body', 'slug': 'supported-body',
                                           'values': {'body': '<p>Keep me</p>'}}, csrf, expected=201)
                    assert post_entry['publication_status'] == 'draft'
                    removed_editor = {**post['descriptor']['settings'], 'supports': ['featured_image']}
                    post_updated = api_json(ctx, 'PUT', base, '/api/admin/content-types/post',
                                            {**post['descriptor'], 'settings': removed_editor}, csrf,
                                            {'If-Match': f'"v{post["version"]}"'})
                    checked_status(ctx, base, 'POST', '/api/admin/content-types/post/entries',
                                   422, {'title': 'Blocked body', 'slug': 'blocked-body',
                                         'values': {'body': 'Must reject'}}, csrf)
                    no_editor_post = api_json(ctx, 'GET', base,
                                              f'/api/admin/content-types/post/entries/{post_entry["id"]}')
                    assert no_editor_post['values']['body'] == '<p>Keep me</p>'
                    effective = api_json(ctx, 'GET', base,
                                         '/api/admin/content-types/post/effective-editor-config')
                    updated_post = api_json(ctx, 'PUT', base,
                                            f'/api/admin/content-types/post/entries/{post_entry["id"]}/composite',
                                            {'expected_version': no_editor_post['version'],
                                             'expected_schema_token': effective['schema_token'],
                                             'title': 'Title kept body', 'slug': 'supported-body',
                                             'values': {}, 'publication_status': 'draft'},
                                            csrf)
                    assert updated_post['entry']['values']['body'] == '<p>Keep me</p>'
                    restored_editor = api_json(ctx, 'PUT', base, '/api/admin/content-types/post',
                                               {**post['descriptor'], 'settings': post['descriptor']['settings']},
                                               csrf, {'If-Match': f'"v{post_updated["version"]}"'})
                    assert restored_editor['descriptor']['settings']['supports'] == ['editor', 'featured_image']
                    checked_status(ctx, base, 'PUT', '/api/admin/content-types/post', 400,
                                   {**post['descriptor'], 'settings':
                                    {**post['descriptor']['settings'], 'publication': 'immediate'}},
                                   csrf, version=restored_editor['version'])
                    checks.append('post_immutable_publication_and_disabled_support_preserves_body')

                    page_type = schema['page']
                    page_no_archive = api_json(ctx, 'PUT', base, '/api/admin/content-types/page',
                                               {**page_type['descriptor'], 'settings':
                                                {**page_type['descriptor']['settings'], 'has_archive': False}},
                                               csrf, {'If-Match': f'"v{page_type["version"]}"'})
                    checked_status(ctx, base, 'GET', '/content/page', 404)
                    api_json(ctx, 'PUT', base, '/api/admin/content-types/page',
                             {**page_type['descriptor'], 'settings': page_type['descriptor']['settings']},
                             csrf, {'If-Match': f'"v{page_no_archive["version"]}"'})
                    checked_status(ctx, base, 'GET', '/content/page', 200)
                    checks.append('legacy_page_archive_remains_until_explicitly_disabled')

                    legacy_schema = descriptor('legacy_c1', 'Legacy C1')
                    legacy = api_json(ctx, 'POST', base, '/api/admin/content-types', legacy_schema, csrf, expected=201)
                    assert legacy['descriptor']['settings'] == policy()
                    old = api_json(ctx, 'POST', base, '/api/admin/content-types/legacy_c1/entries',
                                   {'title': 'Legacy kept public', 'slug': 'legacy-kept', 'values': {}},
                                   csrf, expected=201)
                    assert old['publication_status'] == 'published'
                    checked_status(ctx, base, 'GET', '/content/legacy_c1/legacy-kept', 200)
                    checked_status(ctx, base, 'GET', '/content/legacy_c1', 200)
                    checks.append('legacy_create_immediate_public_and_archive')

                    # A user CPT may declare a custom field with the same key
                    # as Post/Page's Core body. `supports=[]` must not suppress
                    # or reject this unrelated custom field.
                    custom = descriptor('custombody_c1', 'Custom Body C1',
                                        fields=[{'key': 'body', 'label': 'Custom Body', 'type': 'short_text'}],
                                        settings=policy(supports=[]))
                    api_json(ctx, 'POST', base, '/api/admin/content-types', custom, csrf, expected=201)
                    entry_custom = api_json(ctx, 'POST', base,
                                            '/api/admin/content-types/custombody_c1/entries',
                                            {'title': 'Custom field owned', 'slug': 'custom-field-owned',
                                             'values': {'body': 'User defined body field'}},
                                            csrf, expected=201)
                    assert entry_custom['values']['body'] == 'User defined body field'
                    checked_status(ctx, base, 'GET', '/content/custombody_c1/custom-field-owned', 200)
                    custom_page = ctx.new_page()
                    custom_page.goto(base + '/admin/#/cpt/custombody_c1/new', wait_until='domcontentloaded')
                    expect(custom_page.locator('#content-field-body')).to_be_visible(timeout=20000)
                    custom_page.close()
                    checks.append('custom_body_field_is_not_hidden_by_core_support_settings')

                    # Real media ACL; publishing an entry does not make its
                    # attachment public while the CPT policy is private.
                    upload_resp = ctx.request.fetch(base + '/api/admin/media', method='POST',
                        headers={'Origin': base, 'X-CSRF-Token': csrf,
                                 'Content-Type': 'image/png', 'X-File-Name': 'c1-private.png'},
                        data=make_png(seed=53), timeout=20000)
                    assert upload_resp.status == 201, (upload_resp.status, upload_resp.text()[:250])
                    image_id = upload_resp.json()['id']
                    private_media = descriptor('privatemedia_c1', 'Private Media C1',
                        fields=[{'key': 'hero', 'label': 'Hero', 'type': 'image'}],
                        settings=policy(False, False, 'draft_publish', []))
                    media_type = api_json(ctx, 'POST', base, '/api/admin/content-types',
                                          private_media, csrf, expected=201)
                    attached = api_json(ctx, 'POST', base, '/api/admin/content-types/privatemedia_c1/entries',
                                        {'title': 'Media hidden', 'slug': 'media-hidden',
                                         'values': {'hero': image_id}}, csrf, expected=201)
                    api_json(ctx, 'POST', base,
                        f'/api/admin/content-types/privatemedia_c1/entries/{attached["id"]}/publication',
                        {'status': 'published'}, csrf, {'If-Match': f'"v{attached["version"]}"'})
                    checked_status(ctx, base, 'GET', f'/media/{image_id}/file', 404)
                    image_public = dict(private_media)
                    image_public['settings'] = policy(True, False, 'draft_publish', [])
                    media_type = api_json(ctx, 'PUT', base, '/api/admin/content-types/privatemedia_c1',
                                          image_public, csrf,
                                          {'If-Match': f'"v{media_type["version"]}"'})
                    image_response = ctx.request.get(base + f'/media/{image_id}/file')
                    assert image_response.status == 200 and image_response.body().startswith(b'\x89PNG')
                    api_json(ctx, 'PUT', base, '/api/admin/content-types/privatemedia_c1',
                             private_media, csrf, {'If-Match': f'"v{media_type["version"]}"'})
                    checked_status(ctx, base, 'GET', f'/media/{image_id}/file', 404)
                    checked_status(ctx, base, 'GET', f'/api/admin/media/{image_id}/file', 200)
                    checks.append('media_acl_revoked_and_restored_by_cpt_public_setting')

                    private_config = policy(False, False, 'draft_publish', [])
                    private_schema = descriptor('private_c1', 'Private C1', settings=private_config)
                    private = api_json(ctx, 'POST', base, '/api/admin/content-types',
                                       private_schema, csrf, expected=201)
                    assert private['descriptor']['settings'] == private_config
                    first = api_json(ctx, 'POST', base, '/api/admin/content-types/private_c1/entries',
                                     {'title': 'Draft hidden', 'slug': 'draft-hidden', 'values': {}},
                                     csrf, expected=201)
                    assert first['publication_status'] == 'draft'
                    checked_status(ctx, base, 'GET', '/content/private_c1/draft-hidden', 404)
                    checked_status(ctx, base, 'GET', '/content/private_c1', 404)
                    preview = checked_status(ctx, base, 'GET',
                                             f'/api/admin/content-types/private_c1/entries/{first["id"]}/preview', 200)
                    assert 'noindex' in preview.text().lower()
                    checks.append('draft_create_private_public_404_authenticated_noindex_preview')

                    changed = api_json(ctx, 'POST', base,
                                       f'/api/admin/content-types/private_c1/entries/{first["id"]}/publication',
                                       {'status': 'published'}, csrf,
                                       {'If-Match': f'"v{first["version"]}"'})
                    assert changed['publication_status'] == 'published'
                    checked_status(ctx, base, 'GET', '/content/private_c1/draft-hidden', 404)
                    checks.append('private_type_stays_hidden_after_publish')

                    # The saved Builder can hold an exact-entry binding. It
                    # must not retrieve a published entry of a private CPT.
                    home = api_json(ctx, 'GET', base, '/api/admin/builder/homepage')
                    bound_doc = {
                        'schema_version': 1,
                        'sections': [{'id': 'c1-section', 'children': [
                            {'id': 'c1-row', 'children': [
                                {'id': 'c1-col', 'children': [
                                    {'id': 'c1-bound-title', 'type': 'core/text', 'props': {
                                        'text': '',
                                        'binding': {
                                            'type_key': 'private_c1', 'entry_slug': 'draft-hidden',
                                            'source': {'kind': 'title'}, 'target': 'text',
                                        },
                                    }},
                                ]},
                            ]},
                        ]}],
                    }
                    api_json(ctx, 'PUT', base, '/api/admin/builder/homepage',
                             bound_doc, csrf, {'If-Match': f'"v{home["version"]}"'})
                    hidden_home = checked_status(ctx, base, 'GET', '/', 200).text()
                    assert 'Draft hidden' not in hidden_home
                    checks.append('saved_builder_binding_cannot_leak_private_published_cpt')

                    exposed_config = policy(True, False, 'draft_publish', [])
                    private_schema['settings'] = exposed_config
                    private = api_json(ctx, 'PUT', base, '/api/admin/content-types/private_c1',
                                       private_schema, csrf,
                                       {'If-Match': f'"v{private["version"]}"'})
                    checked_status(ctx, base, 'GET', '/content/private_c1/draft-hidden', 200)
                    checked_status(ctx, base, 'GET', '/content/private_c1', 404)
                    public_home = checked_status(ctx, base, 'GET', '/', 200).text()
                    assert 'Draft hidden' in public_home
                    checks.append('public_true_exposes_single_has_archive_false_hides_archive')

                    # C1 updates must preserve policy when old API callers omit settings.
                    private = api_json(ctx, 'PUT', base, '/api/admin/content-types/private_c1',
                                       descriptor('private_c1', 'Private renamed'), csrf,
                                       {'If-Match': f'"v{private["version"]}"'})
                    assert private['descriptor']['settings'] == exposed_config
                    stale = checked_status(ctx, base, 'PUT', '/api/admin/content-types/private_c1',
                                           412, private_schema, csrf, version=private['version'] - 1)
                    assert stale.status == 412
                    checks.append('omitted_settings_preserved_and_stale_schema_cas_412')

                    second = api_json(ctx, 'POST', base, '/api/admin/content-types/private_c1/entries',
                                      {'title': 'Hold draft', 'slug': 'hold-draft', 'values': {}},
                                      csrf, expected=201)
                    assert second['publication_status'] == 'draft'
                    reverse_config = policy(True, False, 'immediate', [])
                    reverse_schema = descriptor('private_c1', 'Private renamed', settings=reverse_config)
                    checked_status(ctx, base, 'PUT', '/api/admin/content-types/private_c1',
                                   409, reverse_schema, csrf, version=private['version'])
                    checked_status(ctx, base, 'GET', '/content/private_c1/hold-draft', 404)
                    changed_second = api_json(ctx, 'POST', base,
                                              f'/api/admin/content-types/private_c1/entries/{second["id"]}/publication',
                                              {'status': 'published'}, csrf,
                                              {'If-Match': f'"v{second["version"]}"'})
                    assert changed_second['publication_status'] == 'published'
                    private = api_json(ctx, 'PUT', base, '/api/admin/content-types/private_c1',
                                       reverse_schema, csrf,
                                       {'If-Match': f'"v{private["version"]}"'})
                    assert private['descriptor']['settings']['publication'] == 'immediate'
                    checked_status(ctx, base, 'POST',
                                   f'/api/admin/content-types/private_c1/entries/{second["id"]}/publication',
                                   400, {'status': 'draft'}, csrf, version=changed_second['version'])
                    checks.append('draft_to_immediate_blocked_409_until_all_drafts_published')

                    # Parallel settings CAS and settings-vs-save must serialize
                    # without committing a stale schema or a partial payload.
                    cookie = '; '.join(f'{c["name"]}={c["value"]}' for c in ctx.cookies(base))
                    competing_a = descriptor('private_c1', 'Private renamed',
                                             settings=policy(False, False, 'immediate', []))
                    competing_b = descriptor('private_c1', 'Private renamed',
                                             settings=policy(True, True, 'immediate', []))
                    before_race_version = private['version']
                    result = concurrent_puts(base, cookie, csrf, [
                        ('/api/admin/content-types/private_c1', competing_a, private['version']),
                        ('/api/admin/content-types/private_c1', competing_b, private['version']),
                    ])
                    assert sorted(result) == [200, 412], result
                    private = next(t for t in api_json(ctx, 'GET', base, '/api/admin/content-types')
                                   if t['descriptor']['key'] == 'private_c1')
                    assert private['version'] == before_race_version + 1, private['version']
                    checks.append('parallel_settings_cas_has_exactly_one_200_and_one_412')

                    latest_entry = api_json(ctx, 'GET', base,
                        f'/api/admin/content-types/private_c1/entries/{second["id"]}')
                    token = api_json(ctx, 'GET', base,
                        '/api/admin/content-types/private_c1/effective-editor-config')['schema_token']
                    toggle_archive = descriptor('private_c1', 'Private renamed',
                        settings={**private['descriptor']['settings'],
                                  'has_archive': not private['descriptor']['settings']['has_archive']})
                    composite = {'expected_version': latest_entry['version'],
                                 'expected_schema_token': token, 'title': 'Concurrent entry update',
                                 'slug': latest_entry['slug'], 'values': {},
                                 'publication_status': 'published'}
                    result = concurrent_puts(base, cookie, csrf, [
                        ('/api/admin/content-types/private_c1', toggle_archive, private['version']),
                        (f'/api/admin/content-types/private_c1/entries/{second["id"]}/composite',
                         composite, None),
                    ])
                    assert result[0] == 200 and result[1] in (200, 409), result
                    final_entry = api_json(ctx, 'GET', base,
                        f'/api/admin/content-types/private_c1/entries/{second["id"]}')
                    assert final_entry['title'] == ('Concurrent entry update' if result[1] == 200 else 'Hold draft')
                    private = next(t for t in api_json(ctx, 'GET', base, '/api/admin/content-types')
                                   if t['descriptor']['key'] == 'private_c1')
                    restore_settings = descriptor('private_c1', 'Private renamed',
                        settings=policy(True, False, 'immediate', []))
                    private = api_json(ctx, 'PUT', base, '/api/admin/content-types/private_c1',
                        restore_settings, csrf, {'If-Match': f'"v{private["version"]}"'})
                    checks.append('parallel_settings_and_composite_save_atomic_schema_fence')

                    malformed = descriptor('broken_c1', 'Broken',
                                           settings={**policy(), 'invalid': True})
                    checked_status(ctx, base, 'POST', '/api/admin/content-types', 422,
                                   malformed, csrf)
                    checked_status(ctx, base, 'POST', '/api/admin/content-types', 400,
                                   descriptor('duplicate_c1', 'Duplicate', settings=policy(
                                       supports=['editor', 'editor'])), csrf)
                    checked_status(ctx, base, 'PUT', '/api/admin/content-types/section',
                                   403, descriptor('section', 'Section', settings=policy()),
                                   csrf, version=schema['section']['version'])
                    checked_status(ctx, base, 'PUT', '/api/admin/content-types/private_c1',
                                   403, reverse_schema, csrf=None, version=private['version'])
                    checks.append('malformed_unknown_duplicate_section_and_csrf_negative_cases')

                    page = ctx.new_page()
                    browser_errors = []
                    page.on('pageerror', lambda error: browser_errors.append(str(error)))
                    page.goto(base + '/admin/#/settings/content-types', wait_until='domcontentloaded')
                    expect(page.get_by_test_id('content-type-schema-settings')).to_be_visible(timeout=20000)
                    page.screenshot(path=str(artifact_dir / 'c1-desktop.png'), full_page=True)
                    page.set_viewport_size({'width': 390, 'height': 844})
                    page.screenshot(path=str(artifact_dir / 'c1-mobile-390.png'), full_page=True)
                    assert not browser_errors, browser_errors
                    checks.append('native_chrome_desktop_and_390px_no_js_errors')

                    db_path = root / 'database/opendash.sqlite'
                    db = sqlite3.connect(db_path)
                    try:
                        assert db.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
                        assert db.execute('PRAGMA foreign_key_check').fetchall() == []
                        assert db.execute('SELECT COUNT(*) FROM content_entries WHERE content_type_key=?',
                                          ('private_c1',)).fetchone()[0] == 2
                    finally:
                        db.close()
                    checks.append('sqlite_integrity_fk_and_entry_preservation')
                    ctx.close()
                    checks.append('chrome_' + browser.version)
                finally:
                    browser.close()
        finally:
            stop_server(server)
        # A fresh native process must read persisted schema settings after restart.
        server2, base2 = launch_server(binary, root)
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel='chrome', headless=True)
                try:
                    ctx = browser.new_context()
                    csrf = admin_login(ctx, base2, 'c1_admin', 'C1-Local-Only-Admin-Password-2026!')
                    types = api_json(ctx, 'GET', base2, '/api/admin/content-types')
                    state = next(item for item in types if item['descriptor']['key'] == 'private_c1')
                    assert state['descriptor']['settings']['publication'] == 'immediate'
                    assert state['descriptor']['settings']['has_archive'] is False
                    checked_status(ctx, base2, 'GET', '/content/private_c1/draft-hidden', 200)
                    checked_status(ctx, base2, 'GET', '/content/private_c1', 404)
                    ctx.close()
                    checks.append('native_restart_persists_cpt_publication_and_archive_policy')
                finally:
                    browser.close()
        finally:
            stop_server(server2)
    return checks


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=pathlib.Path, required=True)
    parser.add_argument('--artifact-dir', type=pathlib.Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    artifact_dir = args.artifact_dir.resolve()
    artifact_dir.mkdir(parents=True, exist_ok=True)
    repo_root = pathlib.Path(__file__).resolve().parents[2]
    def git(*args: str) -> str:
        return subprocess.check_output(['git', *args], cwd=repo_root, text=True).strip()
    changed_paths = git('status', '--porcelain=v1', '--untracked-files=all')
    diff_bytes = subprocess.check_output(['git', 'diff', '--binary'], cwd=repo_root)
    admin_root = repo_root / 'cms/admin/dist'
    admin_hashes = {str(p.relative_to(admin_root)): sha(p)
                    for p in sorted(admin_root.rglob('*')) if p.is_file()} if admin_root.exists() else {}
    fixture_paths = [repo_root / 'cms/migrations/0038_content_type_settings.sql',
                     pathlib.Path(__file__).resolve()]
    receipt = {
        'status': 'FAIL',
        'source_sha': git('rev-parse', 'HEAD'),
        'source_base_sha': git('merge-base', 'HEAD', 'origin/main'),
        'source_worktree_clean': not bool(changed_paths),
        'source_changed_paths': changed_paths.splitlines(),
        'source_worktree_diff_sha256': hashlib.sha256(diff_bytes).hexdigest(),
        'binary_sha256': sha(binary),
        'admin_build_files_sha256': admin_hashes,
        'fixtures_sha256': {str(path.relative_to(repo_root)): sha(path) for path in fixture_paths},
        'host_os': platform.platform(),
        'host_arch': platform.machine(),
        'isolation': 'temporary site root, ephemeral 127.0.0.1 port; never port 18964',
        'checks': [],
        'not_run': ['Windows native G5', 'Linux native G5', 'owner human dogfood', 'common RC/V1'],
    }
    try:
        receipt['checks'] = run(binary, artifact_dir)
        receipt['status'] = 'PASS'
    except Exception as error:
        receipt['error'] = f'{type(error).__name__}: {error}'
        traceback.print_exc()
    finally:
        receipt['screenshots_sha256'] = {
            p.name: sha(p) for p in artifact_dir.glob('*.png')
        }
        (artifact_dir / 'receipt.json').write_text(
            json.dumps(receipt, indent=2, ensure_ascii=False) + '\n'
        )
        print(json.dumps(receipt, ensure_ascii=False))
    return 0 if receipt['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
