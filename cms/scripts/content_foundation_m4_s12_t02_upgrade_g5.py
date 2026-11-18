#!/usr/bin/env python3
"""#375 native 0039 old -> exact merged new -> snapshot-restored old -> new.

This is a task-specific G5 test. It never uses the owner's site and never
downgrades schema in place. Earlier #374 release is the old compatible 0039
binary; both SQLite and uploads are snapshotted/restored before rollback.
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
import sys
import tempfile
import traceback


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def migration_version(db: pathlib.Path) -> int:
    with sqlite3.connect(db) as conn:
        return int(conn.execute('SELECT MAX(version) FROM _sqlx_migrations WHERE success=1').fetchone()[0])


def fingerprint(db: pathlib.Path) -> dict:
    tables = ('taxonomy_definitions', 'taxonomy_terms', 'content_types',
              'content_type_taxonomy_eligibility', 'content_type_taxonomy_overrides',
              'content_entries', 'entry_terms', 'term_extension_values')
    with sqlite3.connect(db) as conn:
        conn.execute('PRAGMA foreign_keys=ON')
        assert conn.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
        assert conn.execute('PRAGMA foreign_key_check').fetchall() == []
        return {table: conn.execute(f'SELECT * FROM {table} ORDER BY 1').fetchall()
                for table in tables}


def backup_sqlite(db: pathlib.Path, destination: pathlib.Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    with sqlite3.connect(db) as source, sqlite3.connect(destination) as target:
        source.backup(target)


def assert_old_new_data(db: pathlib.Path, expected_title: str, expected_label: str) -> None:
    with sqlite3.connect(db) as conn:
        conn.execute('PRAGMA foreign_keys=ON')
        assert conn.execute('SELECT title FROM content_entries WHERE slug=?',
                            ('g5-book-entry',)).fetchone() == (expected_title,)
        assert conn.execute('SELECT label FROM taxonomy_definitions WHERE key=?',
                            ('genre_g5',)).fetchone() == (expected_label,)
        assert conn.execute('SELECT COUNT(*) FROM entry_terms et JOIN taxonomy_terms t ON t.id=et.term_id '
                            'WHERE t.kind=?', ('genre_g5',)).fetchone() == (1,)
        assert conn.execute('PRAGMA integrity_check').fetchone() == ('ok',)
        assert conn.execute('PRAGMA foreign_key_check').fetchall() == []


def run(old: pathlib.Path, new: pathlib.Path, artifacts: pathlib.Path,
        launch_server, stop_server) -> dict:
    checks = []
    with tempfile.TemporaryDirectory(prefix='opendash-c2-t02-g5-', dir=artifacts) as temp:
        root = pathlib.Path(temp).resolve() / 'site'
        root.mkdir(mode=0o700)
        db = root / 'database/opendash.sqlite'
        uploads = root / 'uploads'
        uploads.mkdir(mode=0o700)
        image = uploads / 'g5-t02-keep-upload.bin'
        image.write_bytes(b'g5-t02-site-owned-media-preserve-through-rollback-v1')
        upload_sha = digest(image)

        process, base = launch_server(old, root)
        assert base.startswith('http://127.0.0.1:') and ':18964' not in base
        stop_server(process)
        assert migration_version(db) == 39
        checks.append('old_exact_374_release_boots_existing_0039_schema')

        # Seed site-owned CPT and custom taxonomy data in a disposable DB
        # belonging to the old release, without touching any production site.
        with sqlite3.connect(db) as conn:
            conn.execute('PRAGMA foreign_keys=ON')
            post = json.loads(conn.execute("SELECT definition_json FROM content_types WHERE key='post'").fetchone()[0])
            post['key'] = 'g5books'
            post['label'] = 'G5 Books'
            conn.execute('INSERT INTO content_types(key,definition_json,owner_kind) VALUES(?,?,?)',
                         ('g5books', json.dumps(post, ensure_ascii=False), 'user'))
            conn.execute("INSERT INTO taxonomy_definitions(key,label,hierarchical,public,owner_kind) "
                         "VALUES('genre_g5','Genre G5',1,1,'user')")
            conn.execute("INSERT INTO taxonomy_definitions(key,label,hierarchical,public,owner_kind) "
                         "VALUES('label_g5','Label G5',0,1,'user')")
            conn.execute("INSERT INTO taxonomy_terms(kind,name,slug) VALUES('genre_g5','G5 Root','g5-root')")
            parent_id = conn.execute("SELECT id FROM taxonomy_terms WHERE kind='genre_g5' AND slug='g5-root'").fetchone()[0]
            conn.execute('INSERT INTO taxonomy_terms(kind,name,slug,parent_id) VALUES (?,?,?,?)',
                         ('genre_g5','G5 Child','g5-child',parent_id))
            child_id = conn.execute("SELECT id FROM taxonomy_terms WHERE kind='genre_g5' AND slug='g5-child'").fetchone()[0]
            conn.execute("INSERT INTO taxonomy_terms(kind,name,slug) VALUES('label_g5','G5 Label','g5-label')")
            label_id = conn.execute("SELECT id FROM taxonomy_terms WHERE kind='label_g5'").fetchone()[0]
            conn.execute("INSERT INTO content_type_taxonomy_overrides(content_type_key,taxonomies_json) "
                         "VALUES('g5books',?)", (json.dumps(['genre_g5','label_g5']),))
            conn.execute("INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status) "
                         "VALUES('g5books','g5-book-entry','G5 Original','{}','draft')")
            entry_id = conn.execute("SELECT id FROM content_entries WHERE slug='g5-book-entry'").fetchone()[0]
            conn.executemany('INSERT INTO entry_terms(entry_id,term_id) VALUES(?,?)',
                             [(entry_id,child_id),(entry_id,label_id)])
            conn.execute('INSERT INTO term_extension_values(term_id,owner_kind,owner_id,local_key,values_json) '
                         'VALUES(?,?,?,?,?)',
                         (child_id,'plugin','g5-fixture','marker','{"preserved":true}'))
            conn.commit()
        original = fingerprint(db)
        assert_old_new_data(db, 'G5 Original', 'Genre G5')
        checks.append('old_site_cpt_custom_hierarchy_memberships_override_extensions_integrity')

        snapshot = pathlib.Path(temp) / 'immutable-preupgrade-snapshot'
        snapshot_db = snapshot / 'opendash.sqlite'
        backup_sqlite(db, snapshot_db)
        shutil.copytree(uploads, snapshot / 'uploads')
        assert fingerprint(snapshot_db) == original
        assert digest(snapshot / 'uploads' / image.name) == upload_sha
        checks.append('backup_sqlite_and_media_before_new_release')

        process, base = launch_server(new, root)
        assert ':18964' not in base
        stop_server(process)
        assert migration_version(db) == 39
        assert fingerprint(db) == original
        assert digest(image) == upload_sha
        checks.append('new_exact_390_release_preserves_existing_0039_schema_rows_and_media')

        with sqlite3.connect(db) as conn:
            conn.execute("UPDATE taxonomy_definitions SET label='Genre G5 New',version=version+1 WHERE key='genre_g5'")
            conn.execute("UPDATE content_entries SET title='G5 Updated',version=version+1 WHERE slug='g5-book-entry'")
            conn.execute("INSERT INTO taxonomy_terms(kind,name,slug) VALUES('label_g5','New-only','g5-new-only')")
            conn.commit()
        process, base = launch_server(new, root)
        assert ':18964' not in base
        stop_server(process)
        assert_old_new_data(db, 'G5 Updated', 'Genre G5 New')
        assert digest(image) == upload_sha
        checks.append('new_release_restart_persists_edit_and_extra_term_without_fk_failure')

        # Never run the older executable against the newer stage. Restore
        # pristine pre-upgrade SQLite and uploads together before rollback.
        for suffix in ('','-wal','-shm'):
            target = pathlib.Path(str(db) + suffix)
            if target.exists(): target.unlink()
        shutil.copy2(snapshot_db, db)
        shutil.rmtree(uploads)
        shutil.copytree(snapshot / 'uploads', uploads)
        process, base = launch_server(old, root)
        assert ':18964' not in base
        stop_server(process)
        assert migration_version(db) == 39
        assert fingerprint(db) == original
        assert digest(image) == upload_sha
        assert_old_new_data(db, 'G5 Original', 'Genre G5')
        checks.append('restored_snapshot_old_release_reads_exact_original_no_new_only_data')

        process, base = launch_server(new, root)
        assert ':18964' not in base
        stop_server(process)
        assert fingerprint(db) == original
        assert digest(image) == upload_sha
        with sqlite3.connect(db) as conn:
            conn.execute("UPDATE taxonomy_terms SET name='Reedited after restore' WHERE slug='g5-child'")
            conn.commit()
            assert conn.execute('PRAGMA foreign_key_check').fetchall() == []
        checks.append('restored_snapshot_new_release_can_reopen_and_reedit_without_data_loss')

    return {'result':'PASS','checks':checks,'old_schema':39,'new_schema':39,
            'rollback_method':'SQLite_backup_and_uploads_snapshot_restore_before_old_binary',
            'port_18964_used':False}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('--source-dir', type=pathlib.Path, required=True)
    parser.add_argument('--old-binary', type=pathlib.Path, required=True)
    parser.add_argument('--new-binary', type=pathlib.Path, required=True)
    parser.add_argument('--artifact-dir', type=pathlib.Path, required=True)
    args = parser.parse_args()
    source = args.source_dir.resolve(strict=True)
    old = args.old_binary.resolve(strict=True)
    new = args.new_binary.resolve(strict=True)
    artifacts = args.artifact_dir.resolve()
    artifacts.mkdir(parents=True, exist_ok=True)
    sys.path.insert(0, str(source / 'cms/scripts'))
    from m7_beta_menu_picker_e2e import launch_server, stop_server
    receipt = {
        'source_sha':subprocess.check_output(['git','rev-parse','HEAD'],cwd=source,text=True).strip(),
        'source_tree':subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=source,text=True).strip(),
        'source_changed_paths':subprocess.check_output(['git','status','--porcelain'],cwd=source,text=True).splitlines(),
        'old_binary_sha256':digest(old), 'new_binary_sha256':digest(new),
        'runner_sha256':digest(pathlib.Path(__file__).resolve()),
        'old_binary':str(old), 'new_binary':str(new),
        'host':platform.platform(), 'architecture':platform.machine(),
    }
    try:
        receipt.update(run(old,new,artifacts,launch_server,stop_server))
    except Exception as error:
        receipt.update({'result':'FAIL','error':str(error),'traceback':traceback.format_exc()})
    (artifacts / 'upgrade_receipt.json').write_text(json.dumps(receipt,indent=2,ensure_ascii=False)+'\n')
    print(json.dumps(receipt,indent=2,ensure_ascii=False))
    if receipt['result'] != 'PASS': raise SystemExit(1)


if __name__ == '__main__': main()
