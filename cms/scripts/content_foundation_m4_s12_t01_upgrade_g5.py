#!/usr/bin/env python3
"""#374 C2 G5 old(0038)->new(0039)->old(snapshot)->new upgrade test.

Old binary never executes on the 0039 schema. Restoring the *pre-0039*
SQLite/Uploads snapshot is the only rollback procedure; no down-migration.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import platform
import shutil
import sqlite3
import tempfile
import traceback

from m7_beta_menu_picker_e2e import launch_server, stop_server


def sha(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fingerprint(db: pathlib.Path) -> dict:
    with sqlite3.connect(db) as conn:
        conn.execute('PRAGMA foreign_keys=ON')
        assert conn.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
        assert conn.execute('PRAGMA foreign_key_check').fetchall() == []
        names = ('taxonomy_terms','entry_terms','term_extension_values',
                 'content_type_taxonomy_eligibility','content_entries')
        return {name: conn.execute(f'SELECT * FROM {name} ORDER BY 1').fetchall() for name in names}


def snapshot_db(path: pathlib.Path, target: pathlib.Path):
    target.parent.mkdir(parents=True, exist_ok=True)
    with sqlite3.connect(path) as src, sqlite3.connect(target) as dest:
        src.backup(dest)


def version(db: pathlib.Path) -> int:
    with sqlite3.connect(db) as conn:
        return conn.execute('SELECT MAX(version) FROM _sqlx_migrations WHERE success=1').fetchone()[0]


def run(old: pathlib.Path, new: pathlib.Path, artifacts: pathlib.Path):
    checks = []
    with tempfile.TemporaryDirectory(prefix='opendash-c2-upgrade-') as temp:
        root = pathlib.Path(temp).resolve() / 'site'
        root.mkdir(mode=0o700)
        db = root / 'database/opendash.sqlite'
        uploads = root / 'uploads'
        uploads.mkdir(mode=0o700)
        image = uploads / 'c2-g5-upload-retained.bin'
        image.write_bytes(b'isolated-C2-fixture-immutable-upload-v1')
        image_hash = sha(image)

        process, base = launch_server(old, root)
        assert ':18964' not in base
        stop_server(process)
        assert version(db) == 38, version(db)
        with sqlite3.connect(db) as conn:
            conn.execute('PRAGMA foreign_keys=ON')
            conn.execute("INSERT INTO taxonomy_terms(kind,name,slug) VALUES('category','Root','g5-root')")
            root_id = conn.execute("SELECT id FROM taxonomy_terms WHERE slug='g5-root'").fetchone()[0]
            conn.execute('INSERT INTO taxonomy_terms(kind,name,slug,parent_id) VALUES (?,?,?,?)',
                         ('category','Child','g5-child',root_id))
            child_id = conn.execute("SELECT id FROM taxonomy_terms WHERE slug='g5-child'").fetchone()[0]
            conn.execute("INSERT INTO taxonomy_terms(kind,name,slug) VALUES('tag','Tag','g5-tag')")
            tag_id = conn.execute("SELECT id FROM taxonomy_terms WHERE slug='g5-tag'").fetchone()[0]
            conn.execute("INSERT INTO taxonomy_terms(id,kind,name,slug) VALUES(1000,'tag','Deleted','g5-deleted')")
            conn.execute("DELETE FROM taxonomy_terms WHERE id=1000")
            assert conn.execute("SELECT seq FROM sqlite_sequence WHERE name='taxonomy_terms'").fetchone()[0] == 1000
            conn.execute("INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status) VALUES('post','g5-old','Keep old','{}','draft')")
            entry_id = conn.execute("SELECT id FROM content_entries WHERE slug='g5-old'").fetchone()[0]
            conn.execute('INSERT INTO entry_terms(entry_id,term_id) VALUES (?,?)',(entry_id,child_id))
            conn.execute('INSERT INTO entry_terms(entry_id,term_id) VALUES (?,?)',(entry_id,tag_id))
            conn.execute('INSERT INTO term_extension_values(term_id,owner_kind,owner_id,local_key,values_json) VALUES (?,?,?,?,?)',
                         (child_id,'plugin','sample-plugin','extra','{"marker":"preserve"}'))
            conn.commit()
        old_state = fingerprint(db)
        backup = pathlib.Path(temp).resolve() / 'snapshot'
        backup_db = backup / 'opendash.sqlite'
        snapshot_db(db, backup_db)
        shutil.copytree(uploads, backup / 'uploads')
        assert sha(backup / 'uploads' / image.name) == image_hash
        assert version(backup_db) == 38
        checks.append('old_0038_seed_category_tree_tag_membership_term_extension_and_backup')

        process, base = launch_server(new, root)
        assert ':18964' not in base
        stop_server(process)
        assert version(db) == 39
        upgraded_state = fingerprint(db)
        assert old_state == upgraded_state, {'old':old_state,'new':upgraded_state}
        assert sha(image) == image_hash
        with sqlite3.connect(db) as conn:
            assert conn.execute("SELECT seq FROM sqlite_sequence WHERE name='taxonomy_terms'").fetchone()[0] == 1000
            defs = conn.execute('SELECT key,hierarchical,public,owner_kind,version FROM taxonomy_definitions ORDER BY key').fetchall()
            assert defs == [('category',1,1,'core',1),('tag',0,1,'core',1)], defs
            conn.execute("INSERT INTO taxonomy_definitions(key,label,hierarchical,owner_kind) VALUES('genre','Genre',1,'user')")
            conn.execute("INSERT INTO taxonomy_terms(kind,name,slug) VALUES('genre','Genre','g5-root')")
            assert conn.execute("SELECT id FROM taxonomy_terms WHERE kind='genre'").fetchone()[0] == 1001
            conn.commit()
        checks.append('new_0039_forward_preserves_ids_fk_values_extensions_provider_and_media')
        checks.append('new_custom_taxonomy_same_slug_and_autoincrement_deleted_id_not_reused')

        # Deliberately never launch the old binary against 0039. Restore both
        # SQLite and uploads from the immutable pre-upgrade backup first.
        for suffix in ('','-wal','-shm'):
            candidate = pathlib.Path(str(db) + suffix)
            if candidate.exists(): candidate.unlink()
        shutil.copy2(backup_db, db)
        shutil.rmtree(uploads)
        shutil.copytree(backup / 'uploads', uploads)
        process, base = launch_server(old, root)
        assert ':18964' not in base
        stop_server(process)
        assert version(db) == 38
        assert fingerprint(db) == old_state
        assert sha(image) == image_hash
        checks.append('restored_pre0039_snapshot_old_binary_reads_exact_0038_data')

        process, base = launch_server(new, root)
        assert ':18964' not in base
        stop_server(process)
        assert version(db) == 39
        assert fingerprint(db) == old_state
        assert sha(image) == image_hash
        with sqlite3.connect(db) as conn:
            conn.execute("UPDATE taxonomy_terms SET name='Editable after restore' WHERE id=?",(child_id,))
            conn.commit()
            assert conn.execute('PRAGMA foreign_key_check').fetchall() == []
        checks.append('restored_snapshot_can_reupgrade_to_0039_and_reedit_terms')

    return {'result':'PASS','checks':checks,'old_schema':38,'new_schema':39,
            'rollback_method':'pre0039_SQLite_and_upload_snapshot_restore_not_schema_downgrade',
            'port_18964_used':False}


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--old-binary', type=pathlib.Path, required=True)
    p.add_argument('--new-binary', type=pathlib.Path, required=True)
    p.add_argument('--artifact-dir', type=pathlib.Path, required=True)
    args = p.parse_args()
    old, new = args.old_binary.resolve(strict=True), args.new_binary.resolve(strict=True)
    artifacts = args.artifact_dir.resolve()
    artifacts.mkdir(parents=True, exist_ok=True)
    root = pathlib.Path(__file__).resolve().parents[2]
    import subprocess
    receipt = {'source_sha':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),
               'source_worktree_clean':not subprocess.check_output(['git','status','--porcelain'],cwd=root).strip(),
               'source_changed_paths':subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True).splitlines(),
               'old_binary_sha256':sha(old),'new_binary_sha256':sha(new),
               'old_binary':str(old),'new_binary':str(new),
               'runner_sha256':sha(pathlib.Path(__file__).resolve()),
               'host':platform.platform(),'architecture':platform.machine()}
    try:
        receipt.update(run(old,new,artifacts))
    except Exception as exc:
        receipt.update({'result':'FAIL','error':str(exc),'traceback':traceback.format_exc()})
    (artifacts / 'upgrade_receipt.json').write_text(json.dumps(receipt,indent=2,ensure_ascii=False)+'\n')
    print(json.dumps(receipt,indent=2,ensure_ascii=False))
    if receipt['result'] != 'PASS': raise SystemExit(1)


if __name__ == '__main__': main()
