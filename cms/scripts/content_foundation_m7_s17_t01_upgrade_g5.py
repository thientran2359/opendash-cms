#!/usr/bin/env python3
"""C3 isolated native 0039 -> 0040 -> restored 0039 -> 0040 rehearsal.

Never run the old executable against the migrated database. Always restore
both pre-upgrade SQLite snapshot and uploads before booting the old release.
"""
from __future__ import annotations

import argparse
import hashlib
import http.client
import json
import pathlib
import platform
import shutil
import sqlite3
import subprocess
import tempfile
import traceback
from urllib.parse import urlsplit

from m7_beta_menu_picker_e2e import launch_server, stop_server


def sha(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check(value: bool, description: str) -> None:
    if not value:
        raise AssertionError(description)


def version(db: pathlib.Path) -> int:
    with sqlite3.connect(db) as conn:
        return int(conn.execute(
            'SELECT MAX(version) FROM _sqlx_migrations WHERE success=1'
        ).fetchone()[0])


def inspect(db: pathlib.Path) -> dict:
    with sqlite3.connect(db) as conn:
        conn.execute('PRAGMA foreign_keys=ON')
        check(conn.execute('PRAGMA integrity_check').fetchone() == ('ok',),
              'sqlite_integrity_check')
        check(not conn.execute('PRAGMA foreign_key_check').fetchall(),
              'sqlite_foreign_key_check')
        tables = ('content_entries', 'content_types', 'taxonomy_definitions',
                  'taxonomy_terms', 'entry_public_paths', 'entry_redirects')
        return {table: conn.execute(f'SELECT * FROM {table} ORDER BY 1').fetchall()
                for table in tables}


def status(base: str, path: str) -> tuple[int, str | None]:
    target = urlsplit(base)
    connection = http.client.HTTPConnection(target.hostname, target.port, timeout=20)
    try:
        connection.request('GET', path)
        resp = connection.getresponse()
        resp.read()
        return resp.status, resp.getheader('Location')
    finally:
        connection.close()


def require_http(base: str, path: str, code: int, location: str | None = None) -> None:
    actual_code, actual_location = status(base, path)
    check(actual_code == code, f'{path} got {actual_code}, expected {code}')
    if location is not None:
        check(actual_location == location,
              f'{path} redirect {actual_location!r}, expected {location!r}')


def seed_0039(db: pathlib.Path) -> None:
    with sqlite3.connect(db) as conn:
        conn.execute('PRAGMA foreign_keys=ON')
        post_json = json.loads(conn.execute(
            "SELECT definition_json FROM content_types WHERE key='post'"
        ).fetchone()[0])
        post_json['key'] = 'books_c3'
        post_json['label'] = 'C3 Books'
        conn.execute(
            "INSERT INTO content_types(key,definition_json,owner_kind,provider_status) "
            "VALUES('books_c3',?,'user','active')",
            (json.dumps(post_json, ensure_ascii=False),)
        )
        conn.execute(
            "INSERT INTO taxonomy_definitions(key,label,hierarchical,public,owner_kind) "
            "VALUES('genre_c3','Genre C3',0,1,'user')"
        )
        conn.execute(
            "INSERT INTO taxonomy_terms(kind,name,slug) "
            "VALUES('genre_c3','Fiction','fiction')"
        )
        for kind, slug, title in (
            ('post', 'upgrade-post', 'Existing C3 Post'),
            ('page', 'upgrade-page', 'Existing C3 Page'),
            ('books_c3', 'upgrade-book', 'Existing C3 Book'),
        ):
            conn.execute(
                'INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status) '
                "VALUES(?,?,?,'{}','draft')",
                (kind, slug, title)
            )
            conn.execute(
                "UPDATE content_entries SET publication_status='published' "
                'WHERE content_type_key=? AND slug=?', (kind, slug)
            )
        post_id = conn.execute(
            "SELECT id FROM content_entries WHERE slug='upgrade-post'"
        ).fetchone()[0]
        page_id = conn.execute(
            "SELECT id FROM content_entries WHERE slug='upgrade-page'"
        ).fetchone()[0]
        book_id = conn.execute(
            "SELECT id FROM content_entries WHERE slug='upgrade-book'"
        ).fetchone()[0]
        conn.execute(
            "INSERT INTO entry_redirects(source_path,entry_id) VALUES('/old-upgrade-post',?)",
            (post_id,)
        )
        conn.execute(
            "INSERT INTO entry_public_paths(entry_id,path) VALUES(?,'/old-page-custom')",
            (page_id,)
        )
        conn.execute(
            "INSERT INTO entry_public_paths(entry_id,path) VALUES(?,'/old-book-custom')",
            (book_id,)
        )
        conn.commit()


def snapshot_sqlite(db: pathlib.Path, target: pathlib.Path) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    with sqlite3.connect(db) as source, sqlite3.connect(target) as destination:
        source.backup(destination)


def restore(db: pathlib.Path, uploads: pathlib.Path,
            snapshot_db: pathlib.Path, snapshot_uploads: pathlib.Path) -> None:
    for suffix in ('', '-wal', '-shm'):
        path = pathlib.Path(str(db) + suffix)
        if path.exists():
            path.unlink()
    shutil.copy2(snapshot_db, db)
    if uploads.exists():
        shutil.rmtree(uploads)
    shutil.copytree(snapshot_uploads, uploads)


def run(old: pathlib.Path, new: pathlib.Path, artifacts: pathlib.Path) -> dict:
    checks = []
    with tempfile.TemporaryDirectory(prefix='c3-upgrade-', dir=artifacts) as temp:
        root = pathlib.Path(temp) / 'site'
        root.mkdir(mode=0o700)
        db = root / 'database/opendash.sqlite'
        uploads = root / 'uploads'
        uploads.mkdir()
        media_path = uploads / 'c3-preserve-media.bin'
        media_path.write_bytes(b'c3-snapshot-restored-site-owned-upload')
        media_hash = sha(media_path)

        proc, base = launch_server(old, root)
        stop_server(proc)
        check(version(db) == 39, 'old binary must create schema 0039')
        seed_0039(db)
        before = inspect(db)
        checks.append('old_0039_cpt_terms_posts_page_and_redirects_seeded_integrity')

        snapshot = pathlib.Path(temp) / 'pre-upgrade'
        original = snapshot / 'site.sqlite'
        snapshot_sqlite(db, original)
        shutil.copytree(uploads, snapshot / 'uploads')
        check(inspect(original) == before, 'immutable SQLite snapshot matches')
        checks.append('sqlite_and_uploads_snapshot_before_upgrading')

        proc, base = launch_server(old, root)
        try:
            require_http(base, '/content/post/upgrade-post', 200)
            require_http(base, '/old-upgrade-post', 301, '/content/post/upgrade-post')
            require_http(base, '/old-page-custom', 200)
            require_http(base, '/content/page/upgrade-page', 301, '/old-page-custom')
            require_http(base, '/old-book-custom', 200)
            require_http(base, '/content/books_c3/upgrade-book', 301, '/old-book-custom')
        finally:
            stop_server(proc)
        checks.append('old_0039_legacy_public_routes_and_redirects_work')

        for stage in (1, 2):
            proc, base = launch_server(new, root)
            try:
                check(version(db) == 40, 'new binary must apply migration0040')
                check(inspect(db) == before, 'site-owned original rows must survive migration')
                check(sha(media_path) == media_hash, 'media unchanged after upgrade')
                require_http(base, '/content/post/upgrade-post', 200)
                require_http(base, '/old-upgrade-post', 301, '/content/post/upgrade-post')
                require_http(base, '/old-page-custom', 200)
                require_http(base, '/content/page/upgrade-page', 301, '/old-page-custom')
                require_http(base, '/old-book-custom', 200)
                require_http(base, '/content/books_c3/upgrade-book', 301, '/old-book-custom')
                require_http(base, '/taxonomy/genre_c3/fiction', 200)
                with sqlite3.connect(db) as conn:
                    check(conn.execute(
                        'SELECT COUNT(*) FROM permalink_route_claims'
                    ).fetchone()[0] >= 7, 'legacy canonical claims seeded')
                    check(conn.execute(
                        'SELECT target_kind FROM permalink_route_redirects '
                        "WHERE source_path='/old-upgrade-post'"
                    ).fetchone() == ('entry',), 'legacy redirect maps stable ID')
            finally:
                stop_server(proc)
            checks.append(f'new_0040_stage_{stage}_preserves_data_media_legacy_urls_and_claims')
            if stage == 1:
                restore(db, uploads, original, snapshot / 'uploads')
                check(version(db) == 39, 'restored snapshot is old schema')
                check(inspect(db) == before, 'old SQLite restored byte-equivalent records')
                check(sha(media_path) == media_hash, 'old uploads restored')
                proc, base = launch_server(old, root)
                try:
                    require_http(base, '/content/post/upgrade-post', 200)
                    require_http(base, '/old-upgrade-post', 301, '/content/post/upgrade-post')
                    check(version(db) == 39, 'older binary stays on 0039')
                finally:
                    stop_server(proc)
                checks.append('snapshot_restored_0039_then_old_release_boots_without_downgrade')

    return {'result': 'PASS', 'checks': checks, 'port_18964_used': False}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('--old-binary', required=True, type=pathlib.Path)
    parser.add_argument('--new-binary', required=True, type=pathlib.Path)
    parser.add_argument('--artifact-dir', required=True, type=pathlib.Path)
    args = parser.parse_args()
    old = args.old_binary.resolve(strict=True)
    new = args.new_binary.resolve(strict=True)
    artifacts = args.artifact_dir.resolve()
    artifacts.mkdir(parents=True, exist_ok=True)
    repo = pathlib.Path(__file__).resolve().parents[2]
    receipt = {
        'source_sha': subprocess.check_output(
            ['git', 'rev-parse', 'HEAD'], cwd=repo, text=True
        ).strip(),
        'dirty_paths': subprocess.check_output(
            ['git', 'status', '--porcelain'], cwd=repo, text=True
        ).splitlines(),
        'old_sha256': sha(old), 'new_sha256': sha(new),
        'runner_sha256': sha(pathlib.Path(__file__).resolve()),
        'platform': platform.platform(), 'arch': platform.machine(),
    }
    try:
        receipt.update(run(old, new, artifacts))
    except Exception as error:
        receipt.update({'result': 'FAIL', 'error': str(error),
                        'traceback': traceback.format_exc()})
    (artifacts / 'upgrade-receipt.json').write_text(
        json.dumps(receipt, ensure_ascii=False, indent=2) + '\n'
    )
    print(json.dumps(receipt, ensure_ascii=False, indent=2))
    if receipt['result'] != 'PASS':
        raise SystemExit(1)


if __name__ == '__main__':
    main()
