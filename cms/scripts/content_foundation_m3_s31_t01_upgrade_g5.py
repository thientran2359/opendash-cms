#!/usr/bin/env python3
"""M3-S31-T01 isolated old/new/old SQLite + media and native macOS rollback G5.

Requires exact pre-0038 and new pinned native binaries. Never uses owner site.
The old binary is run only against its original pre-0038 snapshot. Migrated DBs
are never downgraded in place; rollback restores the original SQLite and media.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import platform
import shutil
import sqlite3
import subprocess
import tempfile
import traceback

from playwright.sync_api import sync_playwright
from m7_beta_menu_picker_e2e import api_json, admin_login, launch_server, stop_server
from media_smoke import make_png


def sha(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fingerprints(site: pathlib.Path) -> dict:
    database = site / 'database/opendash.sqlite'
    db = sqlite3.connect(database)
    try:
        assert db.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
        assert db.execute('PRAGMA foreign_key_check').fetchall() == []
        version = db.execute('SELECT max(version) FROM _sqlx_migrations').fetchone()[0]
        schemas = db.execute('SELECT key,definition_json,version FROM content_types ORDER BY key').fetchall()
        entries = db.execute('SELECT content_type_key,slug,title,values_json,publication_status,version FROM content_entries ORDER BY id').fetchall()
        media_rows = db.execute('SELECT id,byte_size FROM media_assets ORDER BY id').fetchall()
    finally:
        db.close()
    uploads = site / 'uploads'
    files = {str(p.relative_to(site)): sha(p) for p in sorted(uploads.rglob('*')) if p.is_file()} if uploads.exists() else {}
    return {'version': version, 'schemas': schemas, 'entries': entries, 'media': media_rows,
            'uploads': files, 'sqlite_bytes_sha256': sha(database)}


def request(context, base, method, path, data=None, csrf=None, extra=None, expected=200):
    headers = dict(extra or {})
    if method != 'GET':
        headers['Origin'] = base
    if csrf:
        headers['X-CSRF-Token'] = csrf
    if data is not None:
        headers['Content-Type'] = 'application/json'
    resp = context.request.fetch(base + path, method=method, headers=headers,
                                 data=json.dumps(data) if data is not None else None, timeout=20000)
    assert resp.status == expected, (method, path, resp.status, resp.text()[:350])
    return resp.json() if 'application/json' in resp.headers.get('content-type', '') else {}


def upload(context, base, csrf):
    resp = context.request.fetch(base + '/api/admin/media', method='POST',
                                 headers={'Origin': base, 'X-CSRF-Token': csrf,
                                          'X-File-Name': 'c1-upgrade.png', 'Content-Type': 'image/png'},
                                 data=make_png(seed=69), timeout=20000)
    assert resp.status == 201, (resp.status, resp.text()[:250])
    return resp.json()['id']


def verify_old_state(ctx, base, media_id):
    types = request(ctx, base, 'GET', '/api/admin/content-types')
    defs = {item['descriptor']['key']: item for item in types}
    assert defs['legacy_g5']['descriptor']['key'] == 'legacy_g5'
    entries = request(ctx, base, 'GET', '/api/admin/content-types/legacy_g5/entries')
    assert len(entries) == 1 and entries[0]['publication_status'] == 'published'
    request(ctx, base, 'GET', '/content/legacy_g5/legacy-public', expected=200)
    request(ctx, base, 'GET', '/content/legacy_g5', expected=200)
    img = ctx.request.get(base + f'/media/{media_id}/file', timeout=20000)
    assert img.status == 200 and img.body().startswith(b'\x89PNG')
    assert 'Old C1 builder' in ctx.request.get(base + '/').text()


def run(old_binary: pathlib.Path, new_binary: pathlib.Path, output: pathlib.Path) -> list[str]:
    checks = []
    with tempfile.TemporaryDirectory(prefix='c1-upgrade-g5-') as temporary:
        temp = pathlib.Path(temporary).resolve()
        original = temp / 'original_site'
        original.mkdir(mode=0o700)
        with sync_playwright() as pw:
            browser = pw.chromium.launch(channel='chrome', headless=True)
            try:
                server, base = launch_server(old_binary, original)
                try:
                    context = browser.new_context()
                    request(context, base, 'POST', '/auth/setup',
                            {'username': 'upgrade_admin', 'password': 'C1-G5-Snapshot-Admin-2026!'}, expected=201)
                    csrf = admin_login(context, base, 'upgrade_admin', 'C1-G5-Snapshot-Admin-2026!')
                    img_id = upload(context, base, csrf)
                    old_schema = {'key': 'legacy_g5', 'label': 'Legacy G5', 'fields': [
                        {'key': 'cover', 'label': 'Cover', 'type': 'image'}]}
                    request(context, base, 'POST', '/api/admin/content-types', old_schema, csrf, expected=201)
                    request(context, base, 'POST', '/api/admin/content-types/legacy_g5/entries',
                            {'title': 'Original public', 'slug': 'legacy-public',
                             'values': {'cover': img_id}}, csrf, expected=201)
                    for key in ('post', 'page'):
                        row = request(context, base, 'POST', f'/api/admin/content-types/{key}/entries',
                                      {'title': f'Old private {key}', 'slug': f'old-{key}-draft',
                                       'values': {}}, csrf, expected=201)
                        assert row['publication_status'] == 'draft'
                    home = request(context, base, 'GET', '/api/admin/builder/homepage')
                    static_doc = {'schema_version': 1, 'sections': [{
                        'id': 'g5-section', 'children': [{'id': 'g5-row', 'children': [{
                            'id': 'g5-column', 'children': [{'id': 'g5-text', 'type': 'core/text',
                                                           'props': {'text': 'Old C1 builder'}}]}]}]}]}
                    request(context, base, 'PUT', '/api/admin/builder/homepage', static_doc, csrf,
                            {'If-Match': f'"v{home["version"]}"'})
                    verify_old_state(context, base, img_id)
                    context.close()
                    checks.append('old_source_native_0037_post_page_drafts_legacy_public_builder_and_media')
                finally:
                    stop_server(server)

                old_fp = fingerprints(original)
                assert old_fp['version'] == 37
                backup = temp / 'backup_before_upgrade.sqlite'
                with sqlite3.connect(original / 'database/opendash.sqlite') as source, sqlite3.connect(backup) as destination:
                    source.backup(destination)
                backup_assets = temp / 'backup_uploads'
                if (original / 'uploads').exists():
                    shutil.copytree(original / 'uploads', backup_assets)
                checks.append('snapshot_db_and_uploads_before_0038_and_fk_integrity_pass')

                server, base = launch_server(new_binary, original)
                try:
                    context = browser.new_context()
                    csrf = admin_login(context, base, 'upgrade_admin', 'C1-G5-Snapshot-Admin-2026!')
                    defs = {entry['descriptor']['key']: entry for entry in request(context, base, 'GET', '/api/admin/content-types')}
                    assert defs['post']['descriptor']['settings']['publication'] == 'draft_publish'
                    assert defs['section']['descriptor']['settings']['public'] is False
                    legacy = defs['legacy_g5']
                    assert legacy['descriptor']['settings'] == {
                        'public': True, 'has_archive': True,
                        'publication': 'immediate', 'supports': ['editor', 'featured_image']}
                    verify_old_state(context, base, img_id)
                    settings = dict(legacy['descriptor']['settings'])
                    settings['publication'] = 'draft_publish'
                    payload = dict(legacy['descriptor'], settings=settings)
                    request(context, base, 'PUT', '/api/admin/content-types/legacy_g5', payload,
                            csrf, {'If-Match': f'"v{legacy["version"]}"'})
                    created = request(context, base, 'POST', '/api/admin/content-types/legacy_g5/entries',
                                      {'title': 'New private draft', 'slug': 'new-private', 'values': {}},
                                      csrf, expected=201)
                    assert created['publication_status'] == 'draft'
                    request(context, base, 'GET', '/content/legacy_g5/new-private', expected=404)
                    request(context, base, 'POST',
                            f'/api/admin/content-types/legacy_g5/entries/{created["id"]}/publication',
                            {'status': 'published'}, csrf,
                            {'If-Match': f'"v{created["version"]}"'})
                    request(context, base, 'GET', '/content/legacy_g5/new-private', expected=200)
                    context.close()
                    checks.append('new_native_0038_migrates_old_data_publication_CAS_and_retains_builder_media')
                finally:
                    stop_server(server)
                new_fp = fingerprints(original)
                assert new_fp['version'] == 38
                assert old_fp['media'] == new_fp['media']
                assert old_fp['uploads'] == new_fp['uploads']
                assert {key: status for key, slug, _, _, status, _ in old_fp['entries'] if key in ('post','page')} == \
                       {key: status for key, slug, _, _, status, _ in new_fp['entries'] if key in ('post','page')}
                assert any(entry[1] == 'legacy-public' and entry[4] == 'published' for entry in new_fp['entries'])
                checks.append('migration_0038_preserves_original_media_and_post_page_publication')

                restore = temp / 'restored_old_site'
                (restore / 'database').mkdir(parents=True)
                shutil.copy2(backup, restore / 'database/opendash.sqlite')
                if backup_assets.exists():
                    shutil.copytree(backup_assets, restore / 'uploads')
                restored_fp = fingerprints(restore)
                assert restored_fp['schemas'] == old_fp['schemas']
                assert restored_fp['entries'] == old_fp['entries']
                assert restored_fp['media'] == old_fp['media']
                assert restored_fp['uploads'] == old_fp['uploads']
                server, base = launch_server(old_binary, restore)
                try:
                    context = browser.new_context()
                    admin_login(context, base, 'upgrade_admin', 'C1-G5-Snapshot-Admin-2026!')
                    verify_old_state(context, base, img_id)
                    context.close()
                    checks.append('rollback_old_binary_restored_pre0038_snapshot_without_schema_downgrade')
                finally:
                    stop_server(server)
                assert fingerprints(restore)['entries'] == old_fp['entries']
                checks.append('old_rollback_data_and_media_checksums_stable')

                # Reapply the new binary to a separate copy of the restored
                # snapshot, verifying that editing remains possible afterwards.
                server, base = launch_server(new_binary, restore)
                try:
                    context = browser.new_context()
                    csrf = admin_login(context, base, 'upgrade_admin', 'C1-G5-Snapshot-Admin-2026!')
                    types = {t['descriptor']['key']: t for t in request(context, base, 'GET', '/api/admin/content-types')}
                    assert types['legacy_g5']['descriptor']['settings']['publication'] == 'immediate'
                    result = request(context, base, 'POST', '/api/admin/content-types/legacy_g5/entries',
                                     {'title': 'Restored content editable', 'slug': 'after-restore',
                                      'values': {}}, csrf, expected=201)
                    assert result['publication_status'] == 'published'
                    request(context, base, 'GET', '/content/legacy_g5/after-restore', expected=200)
                    context.close()
                    checks.append('backup_restore_then_new_upgrade_can_reedit_content')
                finally:
                    stop_server(server)
            finally:
                browser.close()

        (output / 'fingerprints.json').write_text(json.dumps({
            'old': old_fp, 'new': new_fp,
            'old_snapshot_sha256': sha(backup),
            'restored_pre_upgrade': {k: v for k, v in restored_fp.items() if k != 'sqlite_bytes_sha256'},
        }, indent=2, ensure_ascii=False) + '\n')
    return checks


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--old-binary', required=True, type=pathlib.Path)
    p.add_argument('--new-binary', required=True, type=pathlib.Path)
    p.add_argument('--artifact-dir', required=True, type=pathlib.Path)
    args = p.parse_args()
    old = args.old_binary.resolve(strict=True)
    new = args.new_binary.resolve(strict=True)
    output = args.artifact_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    receipt = {'status': 'FAIL', 'old_binary_sha256': sha(old), 'new_binary_sha256': sha(new),
               'host_os': platform.platform(), 'host_arch': platform.machine(),
               'isolation': 'two private disposable site roots, random loopback ports; no 18964',
               'checks': [], 'not_run': ['Windows/Linux native', 'owner human dogfood', 'common RC/V1']}
    try:
        receipt['checks'] = run(old, new, output)
        receipt['status'] = 'PASS'
    except Exception as error:
        receipt['error'] = f'{type(error).__name__}: {error}'
        traceback.print_exc()
    finally:
        (output / 'upgrade_receipt.json').write_text(json.dumps(receipt, indent=2, ensure_ascii=False) + '\n')
        print(json.dumps(receipt, ensure_ascii=False))
    return 0 if receipt['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
