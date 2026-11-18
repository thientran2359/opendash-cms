#!/usr/bin/env python3
"""#374/C2 native isolated admin/API/public acceptance (no owner site access).

python3 cms/scripts/content_foundation_m4_s12_t01_e2e.py \
  --binary cms/target/debug/opendash --artifact-dir .g5-checkouts/evidence/c2-acceptance
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import platform
import sqlite3
import tempfile
import traceback
from concurrent.futures import ThreadPoolExecutor
from threading import Barrier
from urllib.error import HTTPError
from urllib.request import Request, urlopen

from playwright.sync_api import sync_playwright
from m7_beta_menu_picker_e2e import api_json, admin_login, launch_server, stop_server


def sha(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fetch(ctx, base, method, path, expected, payload=None, csrf=None, version=None):
    headers = {'Origin': base} if method != 'GET' else {}
    if csrf:
        headers['X-CSRF-Token'] = csrf
    if version is not None:
        headers['If-Match'] = f'"v{version}"'
    if payload is not None:
        headers['Content-Type'] = 'application/json'
    result = ctx.request.fetch(base + path, method=method, headers=headers,
                               data=json.dumps(payload) if payload is not None else None,
                               timeout=20000)
    assert result.status == expected, (method, path, expected, result.status, result.text()[:500])
    return result


def api(ctx, base, method, path, expected=200, data=None, csrf=None, version=None):
    response = fetch(ctx, base, method, path, expected, data, csrf, version)
    return response.json() if response.body() else None


def concurrent_patches(base, session_cookie, csrf, key, version):
    gate = Barrier(2)

    def worker(i):
        command = Request(base + f'/api/admin/taxonomy/definitions/{key}',
                          method='PATCH', data=json.dumps({'label': f'Genre version {i}'}).encode(),
                          headers={'Origin': base, 'Cookie': session_cookie,
                                   'X-CSRF-Token': csrf, 'If-Match': f'"v{version}"',
                                   'Content-Type': 'application/json'})
        gate.wait(timeout=8)
        try:
            with urlopen(command, timeout=30) as res:
                return res.status
        except HTTPError as exc:
            return exc.code

    with ThreadPoolExecutor(max_workers=2) as pool:
        return sorted(pool.map(worker, [1, 2]))


def run(binary: pathlib.Path, artifacts: pathlib.Path) -> dict:
    checks = []
    with tempfile.TemporaryDirectory(prefix='opendash-c2-isolated-') as temp:
        site_root = pathlib.Path(temp).resolve() / 'site'
        site_root.mkdir(mode=0o700)
        server, base = launch_server(binary, site_root)
        assert ':18964' not in base and base.startswith('http://127.0.0.1:'), base
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel='chrome', headless=True)
                chrome_version = browser.version
                try:
                    ctx = browser.new_context(viewport={'width': 1280, 'height': 800})
                    password = 'C2-Test-Only-Isolated-Password!'
                    api(ctx, base, 'POST', '/auth/setup', 201,
                        {'username':'c2_admin','password': password})
                    csrf = admin_login(ctx, base, 'c2_admin', password)
                    public_settings = api(ctx, base, 'GET', '/api/admin/settings/public')
                    api(ctx, base, 'PUT', '/api/admin/settings/public', 200,
                        {'public_origin':base, 'content_language':'vi',
                         'indexing_enabled':True}, csrf, public_settings['version'])
                    defs = api(ctx, base, 'GET', '/api/admin/taxonomy/definitions')
                    assert {d['key'] for d in defs} == {'category','tag'}
                    assert next(d for d in defs if d['key']=='category')['hierarchical']
                    assert not next(d for d in defs if d['key']=='tag')['hierarchical']
                    checks.append('core_category_tag_seeded_with_hierarchy')

                    guest = browser.new_context()
                    fetch(guest, base, 'GET', '/api/admin/taxonomy/definitions', 401)
                    guest.close()
                    api(ctx, base, 'POST', '/api/admin/users', 201,
                        {'username':'c2_editor', 'password':'C2-Only-Editor-Password!', 'role':'editor'}, csrf)
                    editor = browser.new_context()
                    editor_csrf = admin_login(editor, base, 'c2_editor', 'C2-Only-Editor-Password!')
                    fetch(editor, base, 'POST', '/api/admin/taxonomy/definitions', 403,
                          {'key':'forbidden','label':'Forbidden'}, editor_csrf)
                    fetch(editor, base, 'PUT', '/api/admin/taxonomy/eligibility/post', 403,
                          {'taxonomies': [], 'expected_version': 1}, editor_csrf, 1)
                    editor.close()
                    fetch(ctx, base, 'POST', '/api/admin/taxonomy/definitions', 403,
                          {'key':'missing_csrf','label':'No CSRF'})
                    checks.append('guest_401_editor_403_and_csrf_enforced')

                    genre = api(ctx, base, 'POST', '/api/admin/taxonomy/definitions', 201,
                                {'key':'genre','label':'Genre','hierarchical': True}, csrf)
                    brand = api(ctx, base, 'POST', '/api/admin/taxonomy/definitions', 201,
                                {'key':'brand','label':'Brand'}, csrf)
                    assert genre['public'] and genre['hierarchical'] and not brand['hierarchical']
                    fetch(ctx, base, 'POST', '/api/admin/taxonomy/definitions', 409,
                          {'key':'genre','label':'Genre twice'}, csrf)
                    fetch(ctx, base, 'POST', '/api/admin/taxonomy/definitions', 400,
                          {'key':'category','label':'Reserved'}, csrf)
                    checks.append('custom_genre_brand_defaults_reserved_and_duplicate')

                    root = api(ctx, base, 'POST', '/api/admin/taxonomy/terms', 201,
                               {'kind':'genre','name':'Root','slug':'same-slug'}, csrf)
                    child = api(ctx, base, 'POST', '/api/admin/taxonomy/terms', 201,
                                {'kind':'genre','name':'Child','slug':'child','parent_id':root['id']}, csrf)
                    flat = api(ctx, base, 'POST', '/api/admin/taxonomy/terms', 201,
                               {'kind':'brand','name':'Brand','slug':'same-slug'}, csrf)
                    fetch(ctx, base, 'POST', '/api/admin/taxonomy/terms', 409,
                          {'kind':'genre','name':'Collision','slug':'same-slug'}, csrf)
                    fetch(ctx, base, 'POST', '/api/admin/taxonomy/terms', 400,
                          {'kind':'brand','name':'Bad parent','slug':'bad','parent_id':root['id']}, csrf)
                    fetch(ctx, base, 'POST', '/api/admin/taxonomy/terms', 400,
                          {'kind':'genre','name':'Wrong parent','slug':'wrong','parent_id':flat['id']}, csrf)
                    fetch(ctx, base, 'PATCH', f'/api/admin/taxonomy/terms/{root["id"]}', 409,
                          {'parent_id':child['id']}, csrf, root['version'])
                    checks.append('term_cross_taxonomy_slug_collision_parent_and_cycle')

                    last = child
                    for depth in range(3, 9):
                        last = api(ctx, base, 'POST', '/api/admin/taxonomy/terms', 201,
                                   {'kind':'genre','name':f'Depth {depth}', 'slug':f'depth-{depth}',
                                    'parent_id':last['id']}, csrf)
                    fetch(ctx, base, 'POST', '/api/admin/taxonomy/terms', 409,
                          {'kind':'genre','name':'Too deep','slug':'depth-nine','parent_id':last['id']}, csrf)
                    fetch(ctx, base, 'PATCH', '/api/admin/taxonomy/definitions/genre', 409,
                          {'hierarchical':False}, csrf, genre['version'])
                    checks.append('hierarchy_depth_eight_and_toggle_denied_with_children')

                    fetch(ctx, base, 'PATCH', '/api/admin/taxonomy/definitions/genre', 428,
                          {'label':'Missing CAS'}, csrf)
                    genre = api(ctx, base, 'PATCH', '/api/admin/taxonomy/definitions/genre', 200,
                                {'label':'Genre Updated'}, csrf, genre['version'])
                    fetch(ctx, base, 'PATCH', '/api/admin/taxonomy/definitions/genre', 412,
                          {'label':'Stale'}, csrf, 1)
                    statuses = concurrent_patches(base,
                        '; '.join(f'{c["name"]}={c["value"]}' for c in ctx.cookies(base)),
                        csrf, 'genre', genre['version'])
                    assert statuses == [200, 412], statuses
                    genre = api(ctx, base, 'GET', '/api/admin/taxonomy/definitions/genre')
                    checks.append('definitions_patch_missing_stale_and_parallel_cas')

                    custom = api(ctx, base, 'POST', '/api/admin/content-types', 201,
                                 {'key':'book_c2','label':'Book C2','fields':[]}, csrf)
                    post_schema = next(d for d in api(ctx, base, 'GET', '/api/admin/content-types')
                                       if d['descriptor']['key']=='post')
                    assert api(ctx, base, 'GET', '/api/admin/taxonomy/eligibility/page/effective') == []
                    fetch(ctx, base, 'PUT', '/api/admin/taxonomy/eligibility/section', 400,
                          {'taxonomies':['genre'],'expected_version':1}, csrf, 1)
                    fetch(ctx, base, 'PUT', '/api/admin/taxonomy/eligibility/book_c2', 428,
                          {'taxonomies':['genre'],'expected_version':1}, csrf)
                    custom = api(ctx, base, 'PUT', '/api/admin/taxonomy/eligibility/book_c2', 200,
                                 {'taxonomies':['genre','brand'],'expected_version':custom['version']},
                                 csrf, custom['version'])
                    fetch(ctx, base, 'PUT', '/api/admin/taxonomy/eligibility/book_c2', 412,
                          {'taxonomies':['brand'],'expected_version':1}, csrf, 1)
                    assert api(ctx, base, 'GET', '/api/admin/taxonomy/eligibility/book_c2/effective') == ['brand','genre']
                    checks.append('cpt_eligibility_overrides_defaults_cas_and_section_private')

                    entry = api(ctx, base, 'POST', '/api/admin/content-types/book_c2/entries', 201,
                                {'title':'C2 Book','slug':'c2-book','values':{},
                                 'term_ids':[child['id'],flat['id']]}, csrf)
                    assert set(entry['term_ids']) == {child['id'],flat['id']}
                    wrong = api(ctx, base, 'POST', '/api/admin/taxonomy/terms', 201,
                                {'kind':'tag','name':'Not for book','slug':'c2-wrong'}, csrf)
                    fetch(ctx, base, 'POST', '/api/admin/content-types/book_c2/entries', 400,
                          {'title':'Bad term','slug':'bad-term','values':{},'term_ids':[wrong['id']]}, csrf)
                    before_links = entry['term_ids'][:]
                    custom = api(ctx, base, 'PUT', '/api/admin/taxonomy/eligibility/book_c2', 200,
                                 {'taxonomies':['brand'],'expected_version':custom['version']},
                                 csrf, custom['version'])
                    assert api(ctx, base, 'GET', '/api/admin/taxonomy/eligibility/book_c2/effective') == ['brand']
                    fetch(ctx, base, 'DELETE', '/api/admin/taxonomy/definitions/genre', 409,
                          csrf=csrf, version=genre['version'])
                    unchanged = api(ctx, base, 'GET', f'/api/admin/content-types/book_c2/entries/{entry["id"]}')
                    assert unchanged['term_ids'] == before_links
                    edited = api(ctx, base, 'PUT', f'/api/admin/content-types/book_c2/entries/{entry["id"]}', 200,
                                 {'title':'C2 Book Updated','slug':'c2-book','values':{}},
                                 csrf, unchanged['version'])
                    assert edited['term_ids'] == before_links
                    fetch(ctx, base, 'PUT', f'/api/admin/content-types/book_c2/entries/{entry["id"]}', 400,
                          {'title':'Blocked re-link','slug':'c2-book','values':{},
                           'term_ids':[child['id']]}, csrf, edited['version'])
                    checks.append('omitted_links_preserved_explicit_links_validated_and_delete_409')

                    archive = '/content/book_c2'
                    fetch(ctx, base, 'GET', archive + f'?taxonomy_key=genre&term_id={child["id"]}', 400)
                    fetch(ctx, base, 'GET', archive + '?taxonomy_key=genre', 400)
                    fetch(ctx, base, 'GET', archive + f'?term_id={child["id"]}', 400)
                    fetch(ctx, base, 'GET', archive + f'?taxonomy_key=brand&term_id={flat["id"]}&category_id=1', 400)
                    assert 'C2 Book Updated' in fetch(ctx, base, 'GET',
                        archive + f'?taxonomy_key=brand&term_id={flat["id"]}', 200).text()
                    fetch(ctx, base, 'GET', archive + f'?taxonomy_key=brand&term_id={flat["id"]}&include_descendants=true', 400)
                    table = api(ctx, base, 'GET',
                        f'/api/admin/content-types/book_c2/entry-table?taxonomy_key=brand&term_id={flat["id"]}')
                    assert table['total'] == 1 and table['items'][0]['id'] == entry['id']
                    for invalid_id in ('0', f'0{flat["id"]}', f'%2B{flat["id"]}'):
                        fetch(ctx, base, 'GET',
                            f'/api/admin/content-types/book_c2/entry-table?taxonomy_key=brand&term_id={invalid_id}', 400)
                        fetch(ctx, base, 'GET',
                            archive + f'?taxonomy_key=brand&term_id={invalid_id}', 400)
                    checks.append('generic_public_and_table_filters_validate_pairs_and_association')

                    # Cross-archive generic filters: the CTE must seed from
                    # taxonomy_key, not from the category/tag archive's kind.
                    arc_category = api(ctx, base, 'POST', '/api/admin/taxonomy/terms', 201,
                        {'kind':'category','name':'C2 Archive','slug':'c2-archive'}, csrf)
                    arc_tag = api(ctx, base, 'POST', '/api/admin/taxonomy/terms', 201,
                        {'kind':'tag','name':'C2 Archive Tag','slug':'c2-archive-tag'}, csrf)
                    custom = api(ctx, base, 'PUT', '/api/admin/taxonomy/eligibility/book_c2', 200,
                        {'taxonomies':['brand','category','tag'],
                         'expected_version':custom['version']}, csrf, custom['version'])
                    edited = api(ctx, base, 'PUT',
                        f'/api/admin/content-types/book_c2/entries/{entry["id"]}', 200,
                        {'title':'C2 Book Updated','slug':'c2-book','values':{},
                         'term_ids':[flat['id'],arc_category['id'],arc_tag['id']]},
                        csrf, edited['version'])
                    for kind, slug in [('category','c2-archive'),('tag','c2-archive-tag')]:
                        html = fetch(ctx, base, 'GET',
                            f'/taxonomy/{kind}/{slug}?taxonomy_key=brand&term_id={flat["id"]}', 200).text()
                        assert 'C2 Book Updated' in html, (kind, html[:400])
                    checks.append('generic_filter_on_category_and_tag_archives_cross_kind')

                    brand = api(ctx, base, 'PATCH', '/api/admin/taxonomy/definitions/brand', 200,
                                {'public':False}, csrf, brand['version'])
                    fetch(ctx, base, 'GET', archive + f'?taxonomy_key=brand&term_id={flat["id"]}', 400)
                    assert api(ctx, base, 'GET', '/api/admin/taxonomy/definitions/brand')['public'] is False
                    checks.append('private_taxonomy_not_filterable_publicly')

                    fetch(ctx, base, 'GET', '/sitemap.xml', 200)
                    sitemap = fetch(ctx, base, 'GET', '/sitemaps/1.xml', 200).text()
                    assert '/taxonomy/tag/same-slug' not in sitemap, sitemap[:700]
                    assert '/taxonomy/category/c2-archive' in sitemap, sitemap[:700]
                    custom = api(ctx, base, 'PUT', '/api/admin/taxonomy/eligibility/book_c2', 200,
                        {'taxonomies':['brand','tag'], 'expected_version':custom['version']},
                        csrf, custom['version'])
                    sitemap = fetch(ctx, base, 'GET', '/sitemaps/1.xml', 200).text()
                    assert '/taxonomy/category/c2-archive' not in sitemap
                    checks.append('sitemap_never_leaks_custom_private_or_removed_category_association')

                    post_item = api(ctx, base, 'POST', '/api/admin/content-types/post/entries', 201,
                        {'title':'C2 Builder List Entry','slug':'c2-builder-list','values':{},
                         'term_ids':[arc_category['id']]}, csrf)
                    api(ctx, base, 'POST',
                        f'/api/admin/content-types/post/entries/{post_item["id"]}/publication', 200,
                        {'status':'published'}, csrf, post_item['version'])
                    home = api(ctx, base, 'GET', '/api/admin/builder/homepage')
                    list_doc = {'schema_version':1,'sections':[
                        {'id':'c2-section','children':[
                            {'id':'c2-row','children':[
                                {'id':'c2-column','children':[
                                    {'id':'c2-list-node','type':'core/list',
                                     'props':{'type_key':'post','category_id':arc_category['id'],
                                              'page_size':6,'sort':'id_desc'}}
                                ]}
                            ]}
                        ]}
                    ]}
                    api(ctx, base, 'PUT', '/api/admin/builder/homepage', 200,
                        list_doc, csrf, home['version'])
                    assert 'C2 Builder List Entry' in fetch(ctx, base, 'GET', '/', 200).text()
                    api(ctx, base, 'PUT', '/api/admin/taxonomy/eligibility/post', 200,
                        {'taxonomies':[],'expected_version':post_schema['version']},
                        csrf, post_schema['version'])
                    new_home = fetch(ctx, base, 'GET', '/', 200).text()
                    assert 'C2 Builder List Entry' not in new_home
                    checks.append('saved_builder_list_survives_removed_category_without_homepage_400')

                    # Browser smoke confirms these endpoints are mounted in
                    # the real application and do not break Admin/390px layout.
                    desktop = ctx.new_page()
                    desktop.goto(base + '/admin/#/posts', wait_until='domcontentloaded')
                    desktop.screenshot(path=str(artifacts / 'c2-desktop.png'), full_page=True)
                    desktop.set_viewport_size({'width': 390, 'height': 844})
                    desktop.screenshot(path=str(artifacts / 'c2-mobile-390.png'), full_page=True)
                    assert desktop.evaluate('document.documentElement.scrollWidth <= window.innerWidth')
                    desktop.close()
                    checks.append('real_chrome_admin_desktop_and_390px_no_overflow')

                    # Persist and restart on this exact disposable site.
                    db_path = site_root / 'database/opendash.sqlite'
                    with sqlite3.connect(db_path) as conn:
                        conn.execute('PRAGMA foreign_keys=ON')
                        assert conn.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
                        assert conn.execute('PRAGMA foreign_key_check').fetchall() == []
                        assert conn.execute('SELECT COUNT(*) FROM entry_terms WHERE entry_id=?',
                                            (entry['id'],)).fetchone()[0] == 3
                    checks.append('sqlite_fk_integrity_and_removed_association_preserves_membership')
                finally:
                    browser.close()
        finally:
            stop_server(server)
        server, base = launch_server(binary, site_root)
        try:
            with sqlite3.connect(site_root / 'database/opendash.sqlite') as conn:
                assert conn.execute("SELECT COUNT(*) FROM taxonomy_definitions WHERE key IN ('genre','brand')").fetchone()[0] == 2
                assert conn.execute("SELECT taxonomies_json FROM content_type_taxonomy_overrides WHERE content_type_key='book_c2'").fetchone()[0] == '["brand","tag"]'
            checks.append('native_restart_retains_definitions_overrides_and_terms')
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
    source = pathlib.Path(__file__).resolve().parents[2]
    receipt = {
        'source_sha': __import__('subprocess').check_output(['git','rev-parse','HEAD'], cwd=source, text=True).strip(),
        'source_worktree_clean': not __import__('subprocess').check_output(['git','status','--porcelain'], cwd=source).strip(),
        'source_changed_paths': __import__('subprocess').check_output(['git','status','--porcelain'], cwd=source, text=True).splitlines(),
        'binary_sha256': sha(binary), 'binary_path': str(binary),
        'runner_sha256': sha(pathlib.Path(__file__).resolve()),
        'migration_0039_sha256': sha(source / 'cms/migrations/0039_taxonomy_definitions.sql'),
        'platform': platform.platform(), 'architecture': platform.machine(),
    }
    try:
        receipt.update(run(binary, artifacts))
        receipt['screenshot_sha256'] = {p.name:sha(p) for p in artifacts.glob('*.png')}
    except Exception as exc:
        receipt.update({'result':'FAIL', 'error':str(exc), 'traceback':traceback.format_exc()})
    (artifacts / 'receipt.json').write_text(json.dumps(receipt, indent=2, ensure_ascii=False)+'\n')
    print(json.dumps(receipt, indent=2, ensure_ascii=False))
    if receipt['result'] != 'PASS':
        raise SystemExit(1)


if __name__ == '__main__':
    main()
