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


IDENT = r'[A-Za-z_][A-Za-z_0-9]*'
PATH = re.compile(rf'{IDENT}(?:::{IDENT})*\Z')
MUTATION = re.compile(r'\b(?:post|put|patch|delete)\s*(?:::\s*<[^()\n]*>)?\s*\(')
ITEM = re.compile(
    rf'\bmod\s+({IDENT})\s*(?=[;{{])|'
    rf'\b(?:pub(?:\s*\([^)]*\))?\s+)?(?:async\s+)?fn\s+({IDENT})\s*\('
)
CSRF_GUARD = re.compile(r'\brequire_(?:session|admin)_with_csrf\s*\(')
BOOTSTRAP = {'crate::routes::auth::auth_login', 'crate::routes::auth::auth_setup'}


def split_use_group(group):
    """Split a Rust use tree at commas outside nested groups."""
    parts, start, depth = [], 0, 0
    for index, char in enumerate(group):
        if char == '{':
            depth += 1
        elif char == '}':
            depth -= 1
        elif char == ',' and depth == 0:
            parts.append(group[start:index])
            start = index + 1
    parts.append(group[start:])
    return parts


def app_imports(app):
    """Expand the use trees that introduce a route handler or module in app.rs."""
    imports = {}

    def expand(tree, prefix=''):
        tree = tree.strip()
        if not tree:
            return
        brace = tree.find('{')
        if brace >= 0:
            if not tree.endswith('}'):
                return
            base = tree[:brace].strip().removesuffix('::')
            path = '::'.join(part for part in (prefix, base) if part)
            for part in split_use_group(tree[brace + 1:-1]):
                expand(part, path)
            return
        match = re.fullmatch(rf'({IDENT}(?:::{IDENT})*)(?:\s+as\s+({IDENT}))?', tree)
        if not match:
            return
        fragment, alias = match.groups()
        if fragment == 'self':
            canonical = prefix
        elif fragment.startswith('self::'):
            canonical = prefix + fragment[4:]
        else:
            canonical = '::'.join(part for part in (prefix, fragment) if part)
        if canonical == 'crate::routes' or canonical.startswith('crate::routes::'):
            imports.setdefault(alias or fragment.split('::')[-1], set()).add(canonical)

    # Top-level imports only; local use declarations inside function bodies do
    # not introduce bindings at the router's scope.
    depth = 0
    for match in re.finditer(r'\buse\s+', app):
        depth = app[:match.start()].count('{') - app[:match.start()].count('}')
        if depth != 0:
            continue
        end = app.find(';', match.end())
        if end >= 0:
            expand(app[match.end():end])
    return imports


def route_functions(routes):
    """Index only functions reachable through real Rust module declarations."""
    functions = {}

    def visit(path, module, low=None, high=None, code=None, module_dir=None):
        if code is None:
            code = production_source(path.read_text(encoding='utf-8'))
        low = 0 if low is None else low
        high = len(code) if high is None else high
        levels, depth = [0] * (len(code) + 1), 0
        for index, char in enumerate(code):
            levels[index] = depth
            if char == '{':
                depth += 1
            elif char == '}':
                depth -= 1
        levels[len(code)] = depth
        level = levels[low]
        base_dir = module_dir or (path.parent if path.stem == 'mod' else path.with_suffix(''))

        for match in ITEM.finditer(code, low, high):
            if levels[match.start()] != level:
                continue
            if match[1]:
                name = match[1]
                marker = code.find('{', match.end(), high)
                semicolon = code.find(';', match.end(), high)
                if semicolon >= 0 and (marker < 0 or semicolon < marker):
                    candidates = [base_dir / (name + '.rs'), base_dir / name / 'mod.rs']
                    existing = [file for file in candidates if file.is_file()]
                    if len(existing) == 1:
                        visit(existing[0], module + '::' + name)
                elif marker >= 0:
                    end = block_end(code, marker)
                    if end <= high:
                        visit(path, module + '::' + name, marker + 1, end - 1,
                              code, base_dir / name)
            elif match[2]:
                marker = code.find('{', match.end(), high)
                semicolon = code.find(';', match.end(), high)
                if marker < 0 or (semicolon >= 0 and semicolon < marker):
                    continue
                end = block_end(code, marker)
                if end <= high:
                    functions.setdefault(module + '::' + match[2], []).append(
                        code[marker:end]
                    )

    module_file = routes / 'mod.rs'
    if module_file.is_file():
        visit(module_file, 'crate::routes')
    return functions


def direct_csrf_guard(body):
    # Guard calls in local functions are not run by the registered handler.
    without_local_functions = body
    for match in reversed(list(re.finditer(rf'\b(?:async\s+)?fn\s+{IDENT}\s*\(', body))):
        start = body.find('{', match.end())
        semicolon = body.find(';', match.end())
        if start >= 0 and (semicolon < 0 or start < semicolon):
            end = block_end(body, start)
            without_local_functions = (
                without_local_functions[:match.start()]
                + re.sub(r'[^\n]', ' ', body[match.start():end])
                + without_local_functions[end:]
            )
    return bool(CSRF_GUARD.search(without_local_functions))


def mutation_handlers(app, imports, functions):
    errors = []
    registrations = checked = exemptions = found = 0
    seen = set()
    for match in MUTATION.finditer(app):
        registrations += 1
        start = match.end() - 1
        end = start + 1
        depth = 1
        while end < len(app) and depth:
            if app[end] == '(':
                depth += 1
            elif app[end] == ')':
                depth -= 1
            end += 1
        raw = app[start + 1:end - 1].strip() if depth == 0 else ''
        location = f'app.rs:{app.count(chr(10), 0, match.start()) + 1}'
        if '<' in match[0] or not PATH.fullmatch(raw):
            errors.append(f'{location}: unsupported mutation registration: {match[0].strip()}({raw})')
            continue
        parts = raw.split('::')
        if raw.startswith('crate::'):
            candidates = {raw}
        elif parts[0] in imports:
            candidates = {base + ('::' + '::'.join(parts[1:]) if len(parts) > 1 else '')
                          for base in imports[parts[0]]}
        else:
            candidates = set()
        if len(candidates) != 1:
            errors.append(f'{location}: unresolved or ambiguous mutation handler: {raw}')
            continue
        canonical = candidates.pop()
        if canonical in seen:
            continue
        seen.add(canonical)
        bodies = functions.get(canonical, [])
        if len(bodies) != 1:
            errors.append(
                f'{location}: mutation handler definition missing/ambiguous: {canonical} '
                f'(definitions={len(bodies)})'
            )
            continue
        found += 1
        if canonical in BOOTSTRAP:
            exemptions += 1
        elif direct_csrf_guard(bodies[0]):
            checked += 1
        else:
            errors.append(f'{location}: mutation requires CSRF guard: {canonical}')
    print(
        f'CSRF mutation inventory: registrations={registrations} '
        f'resolved_handlers={found} checked={checked} '
        f'bootstrap_exemptions={exemptions}'
    )
    return errors


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
    errors.extend(mutation_handlers(app, app_imports(app), route_functions(server / 'routes')))
    return errors


def self_test():
    with tempfile.TemporaryDirectory(prefix='opendash-standards-') as folder:
        root = Path(folder)
        files = {
            'cms/crates/opendash-core/Cargo.toml': '[dependencies]\nserde = "1"\n',
            'cms/crates/opendash-core/src/lib.rs': '',
            'cms/crates/opendash-server/src/plugin/mod.rs': '#[cfg(test)]\nmod tests;\n#[cfg(test)]\nmod review_regressions;',
            'cms/crates/opendash-server/src/app.rs': 'use crate::routes::page::save;\npost(save)',
            'cms/crates/opendash-server/src/routes/mod.rs':
                'pub mod page;\npub mod other;\npub mod nested;\npub mod auth;',
            'cms/crates/opendash-server/src/routes/page.rs':
                'pub async fn save() { require_session_with_csrf(); }\n'
                'pub async fn unguarded() {}\n'
                'pub async fn string_only() { let s = "require_session_with_csrf()"; }\n'
                'pub async fn comment_only() { /* require_admin_with_csrf(); */ }\n'
                'pub async fn nested_only() { fn inner() { require_admin_with_csrf(); } }',
            'cms/crates/opendash-server/src/routes/other.rs':
                'pub async fn save() {}\npub async fn auth_login() {}',
            'cms/crates/opendash-server/src/routes/nested.rs':
                'pub mod child;\nmod inline { pub async fn save() { require_session_with_csrf(); } }',
            'cms/crates/opendash-server/src/routes/nested/child.rs':
                'pub async fn save() { require_admin_with_csrf(); }',
            'cms/crates/opendash-server/src/routes/auth.rs':
                'pub async fn auth_login() {}\npub async fn auth_setup() {}',
            'cms/crates/opendash-server/src/storage/page.rs': 'sqlx::query("SELECT 1");',
            'cms/admin/src/api/client.ts': 'fetch("/api");',
        }
        for name, content in files.items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding='utf-8')
        command = [sys.executable, str(Path(__file__).resolve()), '--root', str(root)]
        def run(expected, evidence=None):
            result = subprocess.run(command, capture_output=True, text=True, encoding='utf-8')
            assert result.returncode == expected, result.stdout + result.stderr
            if evidence:
                assert evidence in result.stdout, result.stdout + result.stderr
        run(0)
        app_path = root / 'cms/crates/opendash-server/src/app.rs'
        cases = [
            ('post(crate::routes::missing_unguarded_handler)', 1,
             'definition missing/ambiguous: crate::routes::missing_unguarded_handler'),
            ('post(crate::routes::page::save)', 0, 'checked=1'),
            ('put(crate::routes::nested::child::save)', 0, 'checked=1'),
            ('patch(crate::routes::nested::inline::save)', 0, 'checked=1'),
            ('use crate::routes::page::save as write;\npost(write)', 0, 'checked=1'),
            ('use crate::routes::page as handlers;\npost(handlers::save)', 0, 'checked=1'),
            ('use crate::routes as handlers;\npost(handlers::page::save)',
             0, 'checked=1'),
            ('use crate::{routes::{page::{save as write}}};\npost(write)', 0,
             'checked=1'),
            ('use crate::routes::page::save;\n'
             'post(save).put(save).patch(save).delete(save)\n'
             'axum::routing::delete(crate::routes::page::save)', 0,
             'registrations=5 resolved_handlers=1 checked=1'),
            ('post(crate::routes::page::save).put(crate::routes::other::save)', 1,
             'mutation requires CSRF guard: crate::routes::other::save'),
            ('use crate::routes::page::save as same;\n'
             'use crate::routes::other::save as same;\npost(same)', 1,
             'unresolved or ambiguous mutation handler: same'),
            ('post(crate::routes::page::unguarded)', 1,
             'mutation requires CSRF guard: crate::routes::page::unguarded'),
            ('post(crate::routes::page::string_only)', 1,
             'mutation requires CSRF guard: crate::routes::page::string_only'),
            ('post(crate::routes::page::comment_only)', 1,
             'mutation requires CSRF guard: crate::routes::page::comment_only'),
            ('post(crate::routes::page::nested_only)', 1,
             'mutation requires CSRF guard: crate::routes::page::nested_only'),
            ('post(crate::routes::page::missing)', 1,
             'mutation handler definition missing/ambiguous: crate::routes::page::missing'),
            ('post(|| save)', 1, 'unsupported mutation registration'),
            ('post::<State>(crate::routes::page::save)', 1,
             'unsupported mutation registration'),
            ('post(missing)', 1, 'unresolved or ambiguous mutation handler: missing'),
            ('post(crate::routes::auth::auth_login).post(crate::routes::auth::auth_setup)',
             0, 'bootstrap_exemptions=2'),
            ('post(crate::routes::other::auth_login)', 1,
             'mutation requires CSRF guard: crate::routes::other::auth_login'),
        ]
        for app_source, expected, evidence in cases:
            app_path.write_text(app_source, encoding='utf-8')
            run(expected, evidence)
        app_path.write_text(files['cms/crates/opendash-server/src/app.rs'], encoding='utf-8')
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
    print(
        f'PASS: standards check rejects SQL, I/O, dependency, fetch and '
        f'{len(cases)} mutation coverage regressions'
    )


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
