"""M4-S08-T02 List Block HTTP/SQLite acceptance on an isolated site.

Run after building the embedded Admin:
    python cms/scripts/builder_list_smoke.py --binary <opendash>
No production site or user uploads are touched.
"""
from __future__ import annotations

import argparse
import html
import json
import re
import sqlite3
from contextlib import closing
from pathlib import Path
from tempfile import TemporaryDirectory

from builder_homepage_smoke import checked, element, get, preview, save, tree
from content_entries_smoke import api
from content_types_smoke import login, request, running
from entry_terms_smoke import add_term
from media_smoke import raw_request


def create(base, auth, type_key, slug, title, terms=(), *, values=None):
    return json.loads(checked(api(base, 'POST', f'/api/admin/content-types/{type_key}/entries', {
        'slug': slug,
        'title': title,
        'values': values if values is not None else ({} if type_key != 'projects' else {'summary': 'fixture'}),
        'term_ids': list(terms),
    }, auth), 201, f'create {type_key}/{slug}')[1])


def set_publication(base, auth, item, type_key, status):
    return json.loads(checked(api(
        base, 'POST',
        f'/api/admin/content-types/{type_key}/entries/{item["id"]}/publication',
        {'status': status},
        {**auth, 'If-Match': f'"v{item["version"]}"'},
    ), 200, f'{status} {item["slug"]}')[1])


def list_node(node_id, type_key='post', **options):
    return element('core/list', {'type_key': type_key, 'sort': 'id_desc',
                                 'page_size': 1, 'include_descendants': False, **options}, node_id)


def fragment(page_html, node_id):
    match = re.search(
        rf'<div class="builder-element" data-node-id="{re.escape(node_id)}">(.*?)</div></div>',
        page_html, re.DOTALL,
    )
    assert match, (node_id, page_html[:600])
    return match.group(1)


def public(base, path='/'):
    code, body, headers = raw_request(base, 'GET', path)
    assert code == 200, (path, code, body[:250])
    return body.decode('utf-8')


def test(binary: Path):
    with TemporaryDirectory(prefix='opendash-m4-s08-t02-list-') as temp:
        database = Path(temp) / 'site' / 'database' / 'opendash.sqlite'
        with running(binary, database) as base:
            checked(request(base, '/auth/setup', {'username': 'listadmin', 'password': 'password123'}),
                    201, 'setup')
            admin = login(base, 'listadmin', 'password123')
            checked(api(base, 'POST', '/api/admin/users', {
                'username': 'listeditor', 'password': 'password123', 'role': 'editor',
            }, admin), 201, 'editor')
            editor = login(base, 'listeditor', 'password123')
            checked(api(base, 'POST', '/api/admin/content-types', {
                'key': 'projects', 'label': 'Projects',
                'fields': [{'key': 'summary', 'label': 'Summary', 'type': 'short_text'}],
            }, admin), 201, 'CPT')
            root = add_term(base, admin, 'category', 'Root', 'root')
            child = add_term(base, admin, 'category', 'Child', 'child', root['id'])
            tag = add_term(base, admin, 'tag', 'Featured', 'featured')
            other_tag = add_term(base, admin, 'tag', 'Other', 'other')
            ghost = add_term(base, admin, 'category', 'Ghost', 'ghost')
            post1 = create(base, admin, 'post', 'post-one', 'Public & One', [root['id'], tag['id']])
            post2 = create(base, admin, 'post', 'post-two', '<script>unsafe</script>', [child['id'], tag['id']])
            post3 = create(base, admin, 'post', 'post-three', 'Third', [child['id'], tag['id']])
            exclude = create(base, admin, 'post', 'excluded', 'Exclude other tag', [child['id'], other_tag['id']])
            draft = create(base, admin, 'post', 'secret', 'SECRET DRAFT', [child['id'], tag['id']])
            post1 = set_publication(base, admin, post1, 'post', 'published')
            post2 = set_publication(base, admin, post2, 'post', 'published')
            post3 = set_publication(base, admin, post3, 'post', 'published')
            exclude = set_publication(base, admin, exclude, 'post', 'published')
            page_draft = create(base, admin, 'page', 'page-draft', 'PRIVATE PAGE')
            project1 = create(base, admin, 'projects', 'proj-one', 'Project One')
            project2 = create(base, admin, 'projects', 'proj-two', 'Project Two')
            assert project1['publication_status'] == project2['publication_status'] == 'published'
            assert draft['publication_status'] == 'draft'
            assert public(base).find('SECRET DRAFT') == -1

            a = list_node('list-a', category_id=root['id'], tag_id=tag['id'],
                          include_descendants=True)
            b = list_node('list-b', 'projects', sort='id_asc')
            layout = tree(a, b)
            before = get(base, admin)
            assert before['version'] == 1
            checked(preview(base, {}, layout), 401, 'unauthenticated Preview')
            checked(preview(base, editor, layout), 200, 'editor Preview')
            inspected = json.loads(checked(preview(base, admin, layout), 200, 'list Preview')[1])
            assert inspected['diagnostics'] == []
            assert 'Third' in fragment(inspected['html'], 'list-a')
            assert 'Project One' in fragment(inspected['html'], 'list-b')
            assert 'SECRET DRAFT' not in inspected['html']
            assert 'builder-list-next' not in inspected['html'], 'Preview must always show first page without pagination'
            assert get(base, admin)['version'] == 1
            checked(save(base, admin, layout, 1), 200, 'List Save')
            checked(save(base, editor, layout, 1), 412, 'If-Match stale')
            saved = get(base, editor)
            assert saved['version'] == 2 and saved['document'] == layout
            public_html = public(base)
            assert 'Third' in fragment(public_html, 'list-a')
            assert 'Project One' in fragment(public_html, 'list-b')
            assert 'SECRET DRAFT' not in public_html
            assert 'Exclude other tag' not in public_html
            assert 'builder-list-next' in fragment(public_html, 'list-a')
            assert 'href="/content/post/post-three"' in public_html
            assert 'href="/content/projects/proj-one"' in public_html
            page_layout = tree(list_node('list-page', 'page', page_size=6))
            draft_only = json.loads(checked(preview(base, editor, page_layout), 200, 'Page draft list')[1])
            assert 'PRIVATE PAGE' not in draft_only['html']
            published_page = set_publication(base, admin, page_draft, 'page', 'published')
            allowed_page = json.loads(checked(preview(base, editor, page_layout), 200, 'Page published list')[1])
            assert 'PRIVATE PAGE' in allowed_page['html']
            set_publication(base, admin, published_page, 'page', 'draft')
            assert 'PRIVATE PAGE' not in json.loads(checked(
                preview(base, editor, page_layout), 200, 'Page unpublished list',
            )[1])['html']

            independent = public(base, '/?list_page_list-a=2&list_page_list-b=2&junk=ignore')
            assert '&lt;script&gt;unsafe&lt;/script&gt;' in fragment(independent, 'list-a')
            assert '<script>unsafe</script>' not in independent
            assert 'Project Two' in fragment(independent, 'list-b')
            assert 'href="/?list_page_list-a=3&amp;list_page_list-b=2"' in fragment(independent, 'list-a')
            assert 'href="/?list_page_list-a=2"' in fragment(independent, 'list-b')
            assert 'junk=' not in independent
            duplicate = public(base, '/?list_page_list-a=2&list_page_list-a=3&list_page_list-b=2&extra=4')
            assert 'Third' in fragment(duplicate, 'list-a'), 'duplicate must reset A to first page'
            assert 'Project Two' in fragment(duplicate, 'list-b'), 'B page remains independent'
            assert 'extra=' not in duplicate
            for invalid in ('0', '-1', '101', '9999999999999999', '02', 'x', '1%20OR%201'):
                malformed = public(base, '/?list_page_list-a=' + invalid)
                assert 'Third' in fragment(malformed, 'list-a'), invalid
            unknown = public(base, '/?list_page_unknown=2&list_page_list-b=2')
            assert 'list_page_unknown' not in unknown
            assert 'Project Two' in fragment(unknown, 'list-b')
            out = public(base, '/?list_page_list-a=99&list_page_list-b=2')
            assert 'Trang này không có nội dung' in fragment(out, 'list-a')
            assert 'href="/?list_page_list-b=2"' in fragment(out, 'list-a')
            assert 'Project Two' in fragment(out, 'list-b')
            post3 = set_publication(base, admin, post3, 'post', 'draft')
            assert 'Third' not in fragment(public(base), 'list-a')
            assert '&lt;script&gt;unsafe&lt;/script&gt;' in fragment(public(base), 'list-a')
            post3 = set_publication(base, admin, post3, 'post', 'published')
            assert 'Third' in fragment(public(base), 'list-a')

            # Exact category excludes children; AND filtering happens before pagination.
            exact_layout = tree(list_node('list-exact', category_id=root['id'],
                                          tag_id=tag['id'], page_size=20))
            exact_result = json.loads(checked(preview(base, admin, exact_layout), 200,
                                               'exact category')[1])
            assert 'Public &amp; One' in exact_result['html']
            assert 'Third' not in exact_result['html']
            # Missing category/tag/type are not errors or lost data. Builder
            # must display an empty state + non-sensitive diagnostic and round-trip.
            orphan_layout = tree(
                list_node('list-missing-cpt', 'deleted_provider'),
                list_node('list-missing-cat', category_id=99999999),
                list_node('list-missing-tag', tag_id=root['id']),  # wrong kind
            )
            missing = json.loads(checked(preview(base, admin, orphan_layout), 200,
                                         'missing list references')[1])
            assert {d['node_id'] for d in missing['diagnostics']} == {
                'list-missing-cpt', 'list-missing-cat', 'list-missing-tag',
            }
            assert all(d['code'] == 'list_config_unavailable' for d in missing['diagnostics'])
            assert missing['html'].count('Danh sách chưa khả dụng') == 3
            checked(save(base, admin, orphan_layout, 2), 200, 'Save orphan configs intact')
            assert get(base, admin)['document'] == orphan_layout
            assert get(base, admin)['version'] == 3

            # Deleting a truly unlinked category after save yields a diagnostic.
            ghost_layout = tree(list_node('list-ghost', category_id=ghost['id']))
            checked(save(base, admin, ghost_layout, 3), 200, 'save term reference')
            checked(api(base, 'DELETE', f'/api/admin/taxonomy/terms/{ghost["id"]}',
                        headers={**admin, 'If-Match': f'"v{ghost["version"]}"'}),
                    204, 'delete unlinked category')
            assert 'Danh sách chưa khả dụng' in public(base)
            assert get(base, editor)['document'] == ghost_layout

            # Reject invalid config, bad identifiers, ninth list and out-of-bounds sizes
            # without changing saved ETag, even when prior config references are missing.
            for invalid in (
                {'page_size': 0}, {'page_size': 21}, {'page_size': -1},
                {'page_size': '6'}, {'sort': 'title_asc'},
                {'type_key': '/wrong'}, {'category_id': -1},
                {'include_descendants': True}, {'tag_id': 0},
            ):
                props = {'type_key': 'post', 'page_size': 6, 'sort': 'id_desc',
                         'include_descendants': False, **invalid}
                checked(preview(base, admin, tree(element('core/list', props, 'bad-list'))),
                        400, str(invalid))
            checked(preview(base, admin, tree(*[
                list_node(f'list-{index}') for index in range(9)
            ])), 400, 'more than eight list blocks')
            assert get(base, admin)['version'] == 4
            with closing(sqlite3.connect(database)) as conn:
                assert conn.execute('PRAGMA integrity_check').fetchone() == ('ok',)
                assert conn.execute('PRAGMA foreign_key_check').fetchall() == []
                assert conn.execute('SELECT MAX(version) FROM _sqlx_migrations').fetchone()[0] == 14
                assert conn.execute('SELECT COUNT(*) FROM builder_homepage_media').fetchone()[0] == 0

        with running(binary, database) as base:
            auth = login(base, 'listadmin', 'password123')
            assert get(base, auth)['version'] == 4
            assert 'Danh sách chưa khả dụng' in public(base)

    print('PASS #106: published-only Post/Page/CPT, two independent lists, taxonomy AND/descendants, canonical pagination, invalid refs, 8/20 limits, ETag, restart')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    args = parser.parse_args()
    test(args.binary.resolve())
