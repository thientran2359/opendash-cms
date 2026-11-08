"""Verify M2.5 through HTTP with temporary site data and real Wasm subscriptions."""
import argparse
from contextlib import closing
import io
import json
from pathlib import Path
import sqlite3
from tempfile import TemporaryDirectory
from zipfile import ZipFile, ZipInfo, ZIP_DEFLATED

from windows_smoke import running, request, sample_theme


def plugin_zip(plugin_id, subscribe=True, trap=False):
    sample = Path(__file__).resolve().parents[1] / 'examples/plugins/sample-plugin/sample-plugin.zip'
    with ZipFile(sample) as source:
        manifest = json.loads(source.read('manifest.json'))
        wasm = source.read('plugin.wasm')
    manifest['id'] = plugin_id
    if subscribe:
        manifest['hooks'] = ['page/after_save']
    # Minimal module: exported memory; opendash_run() -> i32 executes unreachable.
    if trap:
        wasm = bytes.fromhex('0061736d01000000 0105016000017f 03020100 0503010001 071902066d656d6f727902000c6f70656e646173685f72756e0000 0a05010300000b')
    output = io.BytesIO()
    with ZipFile(output, 'w', ZIP_DEFLATED) as archive:
        archive.writestr('manifest.json', json.dumps(manifest))
        archive.writestr('plugin.wasm', wasm)
    return output.getvalue()


def symlink_theme():
    output = io.BytesIO()
    with ZipFile(io.BytesIO(sample_theme())) as source, ZipFile(output, 'w') as archive:
        archive.writestr('block.json', source.read('block.json'))
        entry = ZipInfo('block.html')
        entry.create_system = 3
        entry.external_attr = 0o120777 << 16
        archive.writestr(entry, source.read('block.html'))
    return output.getvalue()


def verify(binary):
    with TemporaryDirectory(prefix='opendash-kernel-') as folder:
        db = Path(folder) / 'site.sqlite'
        credentials = json.dumps({'username': 'admin', 'password': 'kernel-password-123'}).encode()
        block = {'type': 'sample/banner', 'values': {'title': 'Kernel saved', 'note': 'User data'}}
        body = json.dumps(block).encode()
        def count(plugin_id):
            with closing(sqlite3.connect(db)) as connection:
                row = connection.execute("SELECT value_json FROM plugin_kv WHERE plugin_id=? AND key='run_count'", (plugin_id,)).fetchone()
                return int(row[0]) if row else 0
        with running(binary, db) as base:
            assert request(base, '/auth/setup', credentials, 'application/json')[0] == 201
            status, result, cookie = request(base, '/auth/login', credentials, 'application/json')
            assert status == 200
            headers = {'Cookie': cookie.split(';')[0], 'X-CSRF-Token': json.loads(result)['csrf_token']}
            def post(path, data=b'{}', content_type='application/json'):
                return request(base, path, data, content_type, headers)
            assert post('/theme', symlink_theme(), 'application/zip')[0] == 400
            assert post('/theme', sample_theme(), 'application/zip')[0] == 201
            registry = json.loads(request(base, '/registry', headers=headers)[1])
            for plugin_id, subscribe, trap in [('a-failing', True, True), ('b-listener', True, False), ('c-manual', False, False)]:
                status, result, _ = post('/api/admin/plugins', plugin_zip(plugin_id, subscribe, trap), 'application/zip')
                assert status == 201, result
            assert post('/preview', body)[0] == 200
            assert count('b-listener') == 0
            assert 'Kernel saved' not in request(base, '/')[1]
            assert post('/save', body)[0] == 200
            assert 'Kernel saved' in request(base, '/')[1]
            assert count('b-listener') == 1  # preceding failed hook did not stop this one
            assert count('c-manual') == 0  # old manifest did not subscribe implicitly
            with closing(sqlite3.connect(db)) as connection:
                assert connection.execute("SELECT last_error FROM plugins WHERE id='a-failing'").fetchone()[0]
                correlations = connection.execute("SELECT DISTINCT correlation_id FROM plugin_logs WHERE plugin_id IN ('a-failing','b-listener')").fetchall()
                assert len(correlations) == 1 and correlations[0][0].startswith('evt_'), correlations
            assert post('/api/admin/plugins/b-listener/disable')[0] == 200
            assert post('/save', body)[0] == 200
            assert count('b-listener') == 1
            assert post('/api/admin/plugins/b-listener/enable')[0] == 200
            assert post('/save', body)[0] == 200
            assert count('b-listener') == 2
            assert post('/theme', sample_theme(), 'application/zip')[0] == 409
            assert json.loads(request(base, '/page', headers=headers)[1]) == block
            # Manual input and output remain unchanged.
            status, response, _ = post('/api/admin/plugins/c-manual/run', b'{"text":"unchanged"}')
            assert status == 200 and json.loads(response)['data']['transformed'] == 'UNCHANGED'
        with running(binary, db) as base:
            assert json.loads(request(base, '/registry', headers=headers)[1]) == registry
            assert json.loads(request(base, '/page', headers=headers)[1]) == block
            assert 'Kernel saved' in request(base, '/')[1]
            assert request(base, '/save', body, 'application/json', headers)[0] == 200
            assert count('b-listener') == 3
            assert count('c-manual') == 1
    print('PASS: registry restart, symlink rejection, post-save hooks, failure isolation, disable/enable and manual compatibility')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('target/release/opendash.exe'))
    verify(parser.parse_args().binary.resolve())
