"""Check enforceable architecture boundaries; not a replacement for semantic review.

Uses only Python 3.11+ stdlib. --self-test proves violations fail in a temporary repo.
"""
import argparse
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import tomllib


def mask_literals(source):
    # Keep offsets/newlines for block extraction and diagnostics. Covers Rust raw
    # strings and ordinary TS/Rust strings; conservative around exotic syntax.
    pattern = r'r(#+)?"[\s\S]*?"\1|r"[\s\S]*?"|"(?:\\.|[^"\\])*"|\x27(?:\\.|[^\x27\\\n])*\x27|`(?:\\.|[^`\\])*`|//[^\n]*|/\*[\s\S]*?\*/'
    return re.sub(pattern, lambda m: re.sub(r'[^\n]', ' ', m[0]), source)


def block_end(source, start):
    depth = 0
    for index in range(start, len(source)):
        if source[index] == '{':
            depth += 1
        elif source[index] == '}':
            depth -= 1
            if depth == 0:
                return index + 1
    return len(source)


def production_source(source):
    masked = mask_literals(source)
    # Only inline cfg(test) modules are exempt. Separate files are exempt solely
    # when explicitly listed and their owning module remains cfg(test).
    for match in reversed(list(re.finditer(r'#\[cfg\(test\)\]\s*mod\s+\w+\s*\{', masked))):
        end = block_end(masked, masked.index('{', match.start()))
        masked = masked[:match.start()] + re.sub(r'[^\n]', ' ', masked[match.start():end]) + masked[end:]
    return masked


def check(root):
    errors = []
    core = root / 'cms/crates/opendash-core'
    server = root / 'cms/crates/opendash-server/src'
    admin = root / 'cms/admin/src'
    cargo = tomllib.loads((core / 'Cargo.toml').read_text(encoding='utf-8'))
    for name, spec in cargo.get('dependencies', {}).items():
        package = spec.get('package', name) if isinstance(spec, dict) else name
        if package not in {'serde', 'serde_json', 'url'}:
            errors.append(f'core dependency requires contract review: {name}')
    for path in (core / 'src').rglob('*.rs'):
        code = production_source(path.read_text(encoding='utf-8'))
        if re.search(r'\b(?:axum|sqlx|wasmi|tokio|reqwest)\s*::|\bstd\s*::\s*(?:fs|io|net|process|thread)\b|\bstd\s*::\s*\{[^}]*\b(?:fs|io|net|process|thread)\b', code):
            errors.append(f'core must not use framework/I/O: {path.relative_to(root)}')
    test_files = {'plugin/tests.rs': 'tests', 'plugin/review_regressions.rs': 'review_regressions'}
    module = (server / 'plugin/mod.rs').read_text(encoding='utf-8')
    for path in server.rglob('*.rs'):
        relative = path.relative_to(server).as_posix()
        if relative in test_files:
            if not re.search(r'#\[cfg\(test\)\]\s*mod\s+' + test_files[relative] + r'\s*;', module):
                errors.append(f'test SQL exemption requires cfg(test): {relative}')
            continue
        code = production_source(path.read_text(encoding='utf-8'))
        if not relative.startswith('storage/') and re.search(r'\bsqlx\s*::\s*(?:query\w*|raw_sql)\b|\buse\s+sqlx\s*::\s*\{[^}]*\b(?:query\w*|raw_sql)\b', code):
            errors.append(f'SQL must live in storage: {relative}')
        if relative.startswith('routes/') and re.search(r'\b(?:run_plugin|run_plugin_correlated)\s*\(', code):
            errors.append(f'route must invoke plugins through HookDispatcher: {relative}')
    for path in admin.rglob('*'):
        if path.suffix in {'.ts', '.tsx'} and path != admin / 'api/client.ts':
            if re.search(r'\bfetch\s*\(', mask_literals(path.read_text(encoding='utf-8'))):
                errors.append(f'HTTP must use api/client.ts: {path.relative_to(root)}')
    app = production_source((server / 'app.rs').read_text(encoding='utf-8'))
    mutations = set(re.findall(r'\b(?:post|put|patch|delete)\s*\(\s*(\w+)\s*\)', app))
    functions = {}
    for path in (server / 'routes').glob('*.rs'):
        code = production_source(path.read_text(encoding='utf-8'))
        for match in re.finditer(r'pub\s+async\s+fn\s+(\w+)\s*\(', code):
            start = code.find('{', match.end())
            functions[match[1]] = code[start:block_end(code, start)]
    for handler in mutations - {'auth_login', 'auth_setup'}:
        body = functions.get(handler, '')
        if not re.search(r'\brequire_(?:session|admin)_with_csrf\s*\(', body):
            errors.append(f'mutation requires CSRF guard: {handler}')
    return errors


def self_test():
    with tempfile.TemporaryDirectory(prefix='opendash-standards-') as folder:
        root = Path(folder)
        files = {
            'cms/crates/opendash-core/Cargo.toml': '[dependencies]\nserde = "1"\n',
            'cms/crates/opendash-core/src/lib.rs': '',
            'cms/crates/opendash-server/src/plugin/mod.rs': '#[cfg(test)]\nmod tests;\n#[cfg(test)]\nmod review_regressions;',
            'cms/crates/opendash-server/src/app.rs': 'post(save)',
            'cms/crates/opendash-server/src/routes/page.rs': 'pub async fn save() { require_session_with_csrf(); }',
            'cms/crates/opendash-server/src/storage/page.rs': 'sqlx::query("SELECT 1");',
            'cms/admin/src/api/client.ts': 'fetch("/api");',
        }
        for name, content in files.items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding='utf-8')
        command = [sys.executable, str(Path(__file__).resolve()), '--root', str(root)]
        def run(expected):
            result = subprocess.run(command, capture_output=True, text=True, encoding='utf-8')
            assert result.returncode == expected, result.stdout + result.stderr
        run(0)
        violations = {
            'cms/crates/opendash-server/src/routes/page.rs': 'pub async fn save() { sqlx::raw_sql("DELETE FROM users"); }',
            'cms/crates/opendash-server/src/plugin/mod.rs': 'sqlx::query_scalar("SELECT 1");',
            'cms/crates/opendash-core/src/lib.rs': 'use std::fs;',
            'cms/crates/opendash-core/Cargo.toml': '[dependencies]\naxum = "0.8"',
            'cms/admin/src/rogue.ts': 'fetch("/api");',
        }
        for name, content in violations.items():
            path = root / name
            old = path.read_text(encoding='utf-8') if path.exists() else None
            path.write_text(content, encoding='utf-8')
            run(1)
            if old is None:
                path.unlink()
            else:
                path.write_text(old, encoding='utf-8')
        run(0)
    print('PASS: standards check rejects SQL, I/O, dependency, guard and fetch violations')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    if args.self_test:
        self_test()
    else:
        errors = check(args.root.resolve())
        print('\n'.join(errors) if errors else 'PASS: core, storage, route and Admin boundaries')
        raise SystemExit(bool(errors))
