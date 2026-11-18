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
MUTATION_VERBS = {'post', 'put', 'patch', 'delete'}
ITEM = re.compile(
    rf'\bmod\s+({IDENT})\s*(?=[;{{])|'
    rf'\b(?:pub(?:\s*\([^)]*\))?\s+)?(?:async\s+)?fn\s+({IDENT})\s*\('
)
CSRF_GUARD = re.compile(r'\brequire_(?:session|admin)_with_csrf\s*\(')
BOOTSTRAP = {'crate::routes::auth::auth_login', 'crate::routes::auth::auth_setup'}
# These policy exceptions are tied to an exact resolved handler and a verified
# source shape. They must never be granted by a name-only match.
PUBLIC_ORIGIN = 'crate::routes::contact::public_submit'
PUBLIC_ORIGIN_HELPER = 'crate::routes::contact::check_strict_contact_origin'
DELEGATED_CSRF = 'crate::routes::operations::upload_restore_archive'
DELEGATED_CSRF_HELPER = 'crate::routes::operations::upload_restore_archive_with_timeout'


def strip_comments(source):
    """Retain literal values required by policy, removing comment spoofing."""
    pattern = r'r(#+)?"[\s\S]*?"\1|"(?:\\.|[^"\\])*"|//[^\n]*|/\*[\s\S]*?\*/'

    def replace(match):
        if match[0].startswith(('//', '/*')):
            return re.sub(r'[^\n]', ' ', match[0])
        return match[0]

    return re.sub(pattern, replace, source)


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
    """Expand top-level handler imports and routing-constructor aliases."""
    imports = {}
    constructors = {}
    errors = []

    def expand(tree, prefix='', location='app.rs:1'):
        tree = tree.strip()
        if not tree:
            return
        brace = tree.find('{')
        if brace >= 0:
            if not tree.endswith('}'):
                if prefix.startswith('axum::routing') or tree.startswith('axum::routing'):
                    errors.append(f'{location}: unsupported routing constructor import: {tree}')
                return
            base = tree[:brace].strip().removesuffix('::')
            path = '::'.join(part for part in (prefix, base) if part)
            for part in split_use_group(tree[brace + 1:-1]):
                expand(part, path, location)
            return
        match = re.fullmatch(rf'({IDENT}(?:::{IDENT})*)(?:\s+as\s+({IDENT}))?', tree)
        if not match:
            if prefix == 'axum::routing' or tree.startswith('axum::routing::'):
                errors.append(f'{location}: unsupported routing constructor import: {tree}')
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
        if canonical.startswith('axum::routing::'):
            verb = canonical.removeprefix('axum::routing::')
            if verb in MUTATION_VERBS:
                binding = alias or fragment.split('::')[-1]
                if binding in MUTATION_VERBS and binding != verb:
                    errors.append(
                        f'{location}: mutation constructor alias {binding} shadows '
                        f'the standard {binding} registration: {canonical}'
                    )
                constructors.setdefault(binding, set()).add(verb)

    # Top-level imports only; local use declarations inside function bodies do
    # not introduce bindings at the router's scope.
    for match in re.finditer(r'\buse\s+', app):
        depth = app[:match.start()].count('{') - app[:match.start()].count('}')
        if depth != 0:
            continue
        end = app.find(';', match.end())
        location = f'app.rs:{app.count(chr(10), 0, match.start()) + 1}'
        if end >= 0:
            expand(app[match.end():end], location=location)
        elif 'axum::routing' in app[match.end():]:
            errors.append(f'{location}: unterminated routing constructor import')
    return imports, constructors, errors


def route_functions(routes):
    """Index only functions reachable through real Rust module declarations."""
    functions = {}

    def visit(path, module, low=None, high=None, code=None, raw=None, module_dir=None):
        if raw is None:
            raw = path.read_text(encoding='utf-8')
        if code is None:
            code = production_source(raw)
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
                              code, raw, base_dir / name)
            elif match[2]:
                marker = code.find('{', match.end(), high)
                semicolon = code.find(';', match.end(), high)
                if marker < 0 or (semicolon >= 0 and semicolon < marker):
                    continue
                end = block_end(code, marker)
                if end <= high:
                    functions.setdefault(module + '::' + match[2], []).append(
                        (code[marker:end], strip_comments(raw[marker:end]))
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


def unique_body(functions, canonical):
    definitions = functions.get(canonical, [])
    return definitions[0] if len(definitions) == 1 else None


def public_origin_guarded(functions, handler):
    """Only the reviewed anonymous contact admission flow qualifies."""
    code, _raw = handler
    helper = unique_body(functions, PUBLIC_ORIGIN_HELPER)
    if helper is None:
        return f'public Origin helper missing/ambiguous: {PUBLIC_ORIGIN_HELPER}'
    helper_code, helper_raw = helper

    # The trusted-origin alternative is an actual equality check against the
    # runtime-configured Origin header, not a constant true or a loose match.
    trusted = re.search(
        r'\blet\s+trusted_origin_match\s*=\s*runtime_cfg\s*'
        r'\.as_ref\(\)\s*\.and_then\(\s*\|Extension\(cfg\)\|\s*'
        r'cfg\.trusted_origin_normalized\(\)\s*\)\s*'
        r'\.is_some_and\(\s*\|trusted\|\s*\{\s*'
        r'headers\s*\.get\(\s*header::ORIGIN\s*\)\s*'
        r'\.and_then\(\s*\|v\|\s*v\.to_str\(\)\.ok\(\)\s*\)\s*'
        r'\.is_some_and\(\s*\|o\|\s*o\s*==\s*trusted\s*\)\s*\}\s*\)\s*;',
        code,
    )
    if trusted is None:
        return 'public Origin trusted-origin comparison missing/changed'
    admitted = re.search(
        r'\bif\s*!\s*trusted_origin_match\s*&&\s*'
        r'!\s*check_strict_contact_origin\s*\(\s*&headers\s*,\s*&db\s*,\s*'
        r'&listener_origin\s*\)\s*\.await\s*\?\s*\{',
        code,
    )
    if admitted is None or admitted.start() <= trusted.end():
        return 'public Origin rejecting check/call/await/propagation missing'
    rejection_end = block_end(code, admitted.end() - 1)
    rejected = code[admitted.end():rejection_end]
    if not re.match(
        r'\s*return\s+Ok\s*\(\s*\(\s*StatusCode::FORBIDDEN\b',
        rejected,
    ):
        return 'public Origin failure is not rejected with FORBIDDEN'
    submit = re.search(r'\bsubmit\s*\(', code)
    if submit is None or submit.start() < rejection_end:
        return 'public Origin rejection must precede submission'

    missing = re.search(
        r'\blet\s+Some\(origin\)\s*=\s*headers\s*\.get\(\s*'
        r'header::ORIGIN\s*\)\s*\.and_then\(\s*\|v\|\s*v\.to_str\(\)\.ok\(\)'
        r'\s*\)\s*else\s*\{',
        helper_code,
    )
    if missing is None:
        return 'public Origin helper must reject missing Origin header'
    missing_end = block_end(helper_code, missing.end() - 1)
    if not re.search(r'\breturn\s+Ok\s*\(\s*false\s*\)\s*;', helper_code[missing.end():missing_end]):
        return 'public Origin helper missing-Header branch must return false'

    # Validate the literal "null" in its actual executable call, not a
    # string/comment elsewhere. The raw and masked bodies share offsets.
    null = re.search(
        r'\bif\s+origin\.is_empty\(\)\s*\|\|\s*'
        r'origin\.eq_ignore_ascii_case\s*\(\s*"null"\s*\)\s*\{',
        helper_raw,
    )
    if null is None or null.start() < missing_end or not re.match(
        r'\bif\s+origin\.is_empty\(\)\s*\|\|\s*'
        r'origin\.eq_ignore_ascii_case\s*\(\s*',
        helper_code[null.start():],
    ):
        return 'public Origin helper must reject empty/null Origin'
    null_end = block_end(helper_code, null.end() - 1)
    if not re.search(r'\breturn\s+Ok\s*\(\s*false\s*\)\s*;', helper_code[null.end():null_end]):
        return 'public Origin helper empty/null branch must return false'
    listener = re.search(
        r'\bif\s+origin\s*==\s*listener_origin\s*\{\s*'
        r'return\s+Ok\s*\(\s*true\s*\)\s*;\s*\}',
        helper_code,
    )
    configured = re.search(
        r'\blet\s+pub_settings\s*=\s*settings::get_public\s*\(\s*db\s*\)'
        r'\s*\.await\s*\?\s*;\s*Ok\s*\(\s*pub_settings\s*'
        r'\.public_origin\s*\.as_deref\(\)\s*'
        r'\.is_some_and\s*\(\s*\|configured\|\s*origin\s*==\s*configured'
        r'\s*\)\s*\)',
        helper_code,
    )
    if not listener or not configured or not (null_end <= listener.start() < configured.start()):
        return 'public Origin helper must compare listener/configured origins (reject foreign)'
    if len(re.findall(r'\breturn\s+Ok\s*\(\s*true\s*\)', helper_code)) != 1:
        return 'public Origin helper has unreviewed success path'
    if len(re.findall(r'\breturn\s+Ok\s*\(\s*false\s*\)', helper_code)) != 2:
        return 'public Origin helper has unreviewed failure path'
    return None


def delegated_csrf_guarded(functions, handler):
    """Only the exact restore wrapper/helper call chain is approved."""
    code, _raw = handler
    helper = unique_body(functions, DELEGATED_CSRF_HELPER)
    if helper is None:
        return f'delegated CSRF helper missing/ambiguous: {DELEGATED_CSRF_HELPER}'
    if not re.fullmatch(
        r'\{\s*upload_restore_archive_with_timeout\s*\(\s*'
        r'state\s*,\s*headers\s*,\s*body\s*,\s*'
        r'std::time::Duration::from_secs\s*\(\s*300\s*\)\s*,?\s*\)'
        r'\s*\.await\s*\}',
        code,
    ):
        return 'delegated CSRF wrapper must return awaited exact helper result without bypass'
    helper_code, _helper_raw = helper
    if not re.match(
        r'\{\s*(?:use\s+[^;]+;\s*)*'
        r'require_admin_with_csrf\s*\(\s*&headers\s*,\s*&state\.db\s*\)'
        r'\s*\.await\s*\?\s*;',
        helper_code,
    ):
        return 'delegated CSRF helper must propagate admin guard before side effects'
    return None


def mutation_handlers(app, imports, functions, constructors=None):
    errors = []
    constructors = constructors or {}
    registrations = checked = exemptions = found = public = delegated = 0
    seen = set()
    # Count ordinary method/constructor invocations and additional calls bound
    # by top-level use trees to axum::routing::{post,put,patch,delete}. A name
    # such as "submit" is not a mutation constructor without that import.
    sites = {match.start(): match for match in MUTATION.finditer(app)}
    for alias in constructors:
        if alias in MUTATION_VERBS:
            continue  # Already matched, including chained .post/.put/etc.
        pattern = re.compile(
            rf'(?<![\w.:]){re.escape(alias)}\s*(?:::\s*<[^()\n]*>)?\s*\('
        )
        for match in pattern.finditer(app):
            sites.setdefault(match.start(), match)
    for _, match in sorted(sites.items()):
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
        called = re.match(IDENT, match[0])[0]
        if called in constructors and len(constructors[called]) != 1:
            errors.append(
                f'{location}: ambiguous mutation constructor alias: {called} '
                f'(verbs={",".join(sorted(constructors[called]))})'
            )
            continue
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
        elif canonical == PUBLIC_ORIGIN:
            violation = public_origin_guarded(functions, bodies[0])
            if violation:
                errors.append(f'{location}: {canonical}: {violation}')
            else:
                public += 1
        elif canonical == DELEGATED_CSRF:
            violation = delegated_csrf_guarded(functions, bodies[0])
            if violation:
                errors.append(f'{location}: {canonical}: {violation}')
            else:
                delegated += 1
        elif direct_csrf_guard(bodies[0][0]):
            checked += 1
        else:
            errors.append(f'{location}: mutation requires CSRF guard: {canonical}')
    print(
        f'CSRF mutation inventory: registrations={registrations} '
        f'resolved_handlers={found} direct_guarded={checked} '
        f'delegated_csrf_guarded={delegated} public_origin_guarded={public} '
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
    imports, constructors, import_errors = app_imports(app)
    errors.extend(import_errors)
    errors.extend(mutation_handlers(app, imports, route_functions(server / 'routes'), constructors))
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
            ('post(crate::routes::page::save)', 0, 'direct_guarded=1'),
            ('put(crate::routes::nested::child::save)', 0, 'direct_guarded=1'),
            ('patch(crate::routes::nested::inline::save)', 0, 'direct_guarded=1'),
            ('use crate::routes::page::save as write;\npost(write)', 0, 'direct_guarded=1'),
            ('use crate::routes::page as handlers;\npost(handlers::save)', 0, 'direct_guarded=1'),
            ('use crate::routes as handlers;\npost(handlers::page::save)',
             0, 'direct_guarded=1'),
            ('use crate::{routes::{page::{save as write}}};\npost(write)', 0,
             'direct_guarded=1'),
            ('use crate::routes::page::save;\n'
             'post(save).put(save).patch(save).delete(save)\n'
             'axum::routing::delete(crate::routes::page::save)', 0,
             'registrations=5 resolved_handlers=1 direct_guarded=1'),
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
            # Aster P2 review on #389: aliased constructors used to disappear
            # from inventory, allowing an unguarded route to pass the full CLI.
            ('use axum::routing::post as submit;\n'
             'Router::new().route("/write", submit(crate::routes::page::unguarded))',
             1, 'mutation requires CSRF guard: crate::routes::page::unguarded'),
            ('use axum::routing::{post as submit, put as update};\n'
             'Router::new().route("/write", submit(crate::routes::page::missing))',
             1, 'mutation handler definition missing/ambiguous: crate::routes::page::missing'),
            ('use axum::{routing::{post as submit, patch as edit}};\n'
             'submit(crate::routes::page::unguarded)',
             1, 'mutation requires CSRF guard: crate::routes::page::unguarded'),
            ('use axum::routing::post as submit;\n'
             'Router::new().route("/write", submit(crate::routes::page::save))',
             0, 'registrations=1 resolved_handlers=1 direct_guarded=1'),
            ('use axum::routing::{post as submit, put as update};\n'
             'submit(crate::routes::page::save).put(crate::routes::page::save);\n'
             'update(crate::routes::page::save)',
             0, 'registrations=3 resolved_handlers=1 direct_guarded=1'),
            ('use axum::routing::{post as submit, put as update, '
             'patch as edit, delete as destroy};\n'
             'submit(crate::routes::page::save); update(crate::routes::page::save);\n'
             'edit(crate::routes::page::save); destroy(crate::routes::page::save)',
             0, 'registrations=4 resolved_handlers=1 direct_guarded=1'),
            ('use axum::routing::{post as submit, delete as destroy};\n'
             'destroy(crate::routes::other::save)',
             1, 'mutation requires CSRF guard: crate::routes::other::save'),
            ('use crate::routes::page::save as write;\n'
             'use axum::routing::post as submit;\nsubmit(write)',
             0, 'registrations=1 resolved_handlers=1 direct_guarded=1'),
            ('use axum::routing::{post as submit, put as submit};\n'
             'submit(crate::routes::page::save)',
             1, 'ambiguous mutation constructor alias: submit'),
            ('use axum::routing::put as post;\npost(crate::routes::page::save)',
             1, 'mutation constructor alias post shadows the standard post'),
            ('use axum::routing::post as r#submit;\n'
             'r#submit(crate::routes::page::unguarded)',
             1, 'unsupported routing constructor import'),
            ('use axum::routing::*;\npost(crate::routes::page::save)',
             1, 'unsupported routing constructor import'),
            ('use axum::routing::post as submit;\n'
             'let text = "submit(crate::routes::page::unguarded)";',
             0, 'registrations=0 resolved_handlers=0'),
            ('use axum::routing::post as submit;\n'
             'some_service.submit(crate::routes::page::unguarded)',
             0, 'registrations=0 resolved_handlers=0'),
        ]
        for app_source, expected, evidence in cases:
            app_path.write_text(app_source, encoding='utf-8')
            run(expected, evidence)

        # The only approved non-session policies live at these exact canonical
        # module paths. Test both the positive path and individual bypasses.
        contact_path = root / 'cms/crates/opendash-server/src/routes/contact.rs'
        operations_path = root / 'cms/crates/opendash-server/src/routes/operations.rs'
        public_fixture = """
async fn check_strict_contact_origin() -> ApiResult<bool> {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return Ok(false);
    };
    if origin.is_empty() || origin.eq_ignore_ascii_case("null") {
        return Ok(false);
    }
    if origin == listener_origin {
        return Ok(true);
    }
    let pub_settings = settings::get_public(db).await?;
    Ok(pub_settings.public_origin.as_deref().is_some_and(|configured| origin == configured))
}
pub async fn public_submit() -> ApiResult<Response> {
    let trusted_origin_match = runtime_cfg
        .as_ref()
        .and_then(|Extension(cfg)| cfg.trusted_origin_normalized())
        .is_some_and(|trusted| {
            headers.get(header::ORIGIN)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|o| o == trusted)
        });
    if !trusted_origin_match && !check_strict_contact_origin(&headers, &db, &listener_origin).await? {
        return Ok((StatusCode::FORBIDDEN, "bad origin").into_response());
    }
    let result = submit(&db).await?;
    Ok(result)
}
"""
        delegated_fixture = """
pub async fn upload_restore_archive() -> ApiResult<Response> {
    upload_restore_archive_with_timeout(
        state,
        headers,
        body,
        std::time::Duration::from_secs(300),
    ).await
}
pub async fn upload_restore_archive_with_timeout() -> ApiResult<Response> {
    use std::io::Write;
    use sha2::{Digest, Sha256};
    require_admin_with_csrf(&headers, &state.db).await?;
    do_upload().await?;
    Ok(done)
}
"""
        policy_app = ('post(crate::routes::contact::public_submit)'
                      '.post(crate::routes::operations::upload_restore_archive)')
        route_mod = root / 'cms/crates/opendash-server/src/routes/mod.rs'
        old_modules = route_mod.read_text(encoding='utf-8')
        route_mod.write_text(old_modules + '\npub mod contact;\npub mod operations;\n',
                             encoding='utf-8')
        app_path.write_text(policy_app, encoding='utf-8')
        contact_path.write_text(public_fixture, encoding='utf-8')
        operations_path.write_text(delegated_fixture, encoding='utf-8')
        run(0, 'registrations=2 resolved_handlers=2 direct_guarded=0 '
            'delegated_csrf_guarded=1 public_origin_guarded=1 bootstrap_exemptions=0')

        public_failures = [
            ('missing helper',
             public_fixture.replace('fn check_strict_contact_origin', 'fn missing_origin_helper'),
             'public Origin helper missing/ambiguous'),
            ('missing Origin',
             public_fixture.replace('headers.get(header::ORIGIN)',
                                    'headers.get(header::REFERER)', 1),
             'must reject missing Origin'),
            ('missing rejection',
             public_fixture.replace('return Ok(false);', 'return Ok(true);', 1),
             'missing-Header branch must return false'),
            ('null literal changed',
             public_fixture.replace('eq_ignore_ascii_case("null")',
                                    'eq_ignore_ascii_case("other")'),
             'must reject empty/null Origin'),
            ('empty/null branch accepts',
             public_fixture.replace('if origin.is_empty() || origin.eq_ignore_ascii_case("null") {\n'
                                    '        return Ok(false);',
                                    'if origin.is_empty() || origin.eq_ignore_ascii_case("null") {\n'
                                    '        return Ok(true);'),
             'empty/null branch must return false'),
            ('listener not compared',
             public_fixture.replace('origin == listener_origin', 'origin != listener_origin'),
             'must compare listener/configured origins'),
            ('configured foreign Origin accepted',
             public_fixture.replace('origin == configured', 'origin != configured'),
             'must compare listener/configured origins'),
            ('trusted origin forced true',
             public_fixture.replace('trusted_origin_match = runtime_cfg',
                                    'trusted_origin_match = true || runtime_cfg'),
             'trusted-origin comparison missing/changed'),
            ('trusted origin wrong header',
             public_fixture.replace('headers.get(header::ORIGIN)\n                .and_then',
                                    'headers.get(header::REFERER)\n                .and_then'),
             'trusted-origin comparison missing/changed'),
            ('public bypass',
             public_fixture.replace('if !trusted_origin_match && !check_strict_contact_origin',
                                    'if trusted_origin_match && !check_strict_contact_origin'),
             'rejecting check/call/await/propagation missing'),
            ('public helper result swallowed',
             public_fixture.replace(').await? {', ').await {'),
             'rejecting check/call/await/propagation missing'),
            ('public return success',
             public_fixture.replace('StatusCode::FORBIDDEN', 'StatusCode::OK'),
             'not rejected with FORBIDDEN'),
            ('public decoy forbidden',
             public_fixture.replace(
                 'return Ok((StatusCode::FORBIDDEN, "bad origin").into_response());',
                 'return Ok((StatusCode::OK, "bad origin").into_response());\n'
                 '        if false { return Ok((StatusCode::FORBIDDEN, "decoy").into_response()); }'),
             'not rejected with FORBIDDEN'),
            ('public submit before check',
             public_fixture.replace('    let trusted_origin_match = runtime_cfg',
                                    '    submit(&db).await?;\n    let trusted_origin_match = runtime_cfg'),
             'rejection must precede submission'),
            ('helper fast accepts foreign',
             public_fixture.replace('    let Some(origin) =',
                                    '    return Ok(true);\n    let Some(origin) ='),
             'unreviewed success path'),
            ('only comments claim guard',
             public_fixture.replace('&& !check_strict_contact_origin', '&& !other_helper')
                + '\n// && !check_strict_contact_origin(&headers, &db, &listener_origin).await?\n',
             'rejecting check/call/await/propagation missing'),
        ]
        for name, candidate, diagnostic in public_failures:
            contact_path.write_text(candidate, encoding='utf-8')
            run(1, diagnostic)
        contact_path.write_text(public_fixture, encoding='utf-8')

        delegated_failures = [
            ('missing exact helper',
             delegated_fixture.replace('fn upload_restore_archive_with_timeout',
                                       'fn renamed_upload_restore_archive_with_timeout'),
             'delegated CSRF helper missing/ambiguous'),
            ('different delegated function',
             delegated_fixture.replace('    upload_restore_archive_with_timeout(',
                                       '    other_upload_restore_archive_with_timeout('),
             'wrapper must return awaited exact helper result'),
            ('changed timeout arguments',
             delegated_fixture.replace('from_secs(300)', 'from_secs(301)'),
             'wrapper must return awaited exact helper result'),
            ('swallowed helper result',
             delegated_fixture.replace('    ).await\n}', '    ).await;\n    Ok(done)\n}'),
             'wrapper must return awaited exact helper result'),
            ('early successful bypass',
             delegated_fixture.replace('    upload_restore_archive_with_timeout(',
                                       '    if true { return Ok(done); }\n'
                                       '    upload_restore_archive_with_timeout('),
             'wrapper must return awaited exact helper result'),
            ('removed auth guard',
             delegated_fixture.replace('require_admin_with_csrf(&headers, &state.db).await?;',
                                       'do_nothing();'),
             'helper must propagate admin guard'),
            ('wrong guard',
             delegated_fixture.replace('require_admin_with_csrf', 'require_session_with_csrf'),
             'helper must propagate admin guard'),
            ('guard error swallowed',
             delegated_fixture.replace('require_admin_with_csrf(&headers, &state.db).await?;',
                                       'require_admin_with_csrf(&headers, &state.db).await;'),
             'helper must propagate admin guard'),
            ('guard moved after upload',
             delegated_fixture.replace(
                 '    require_admin_with_csrf(&headers, &state.db).await?;\n'
                 '    do_upload().await?;',
                 '    do_upload().await?;\n'
                 '    require_admin_with_csrf(&headers, &state.db).await?;'),
             'helper must propagate admin guard'),
            ('string guard spoofing',
             delegated_fixture.replace(
                 '    require_admin_with_csrf(&headers, &state.db).await?;',
                 '    let _s = "require_admin_with_csrf(&headers, &state.db).await?";'),
             'helper must propagate admin guard'),
        ]
        for name, candidate, diagnostic in delegated_failures:
            operations_path.write_text(candidate, encoding='utf-8')
            run(1, diagnostic)
        operations_path.write_text(delegated_fixture, encoding='utf-8')
        run(0, 'delegated_csrf_guarded=1 public_origin_guarded=1')
        # Remove synthetic policy routes before checking legacy regressions.
        app_path.write_text(files['cms/crates/opendash-server/src/app.rs'], encoding='utf-8')
        route_mod.write_text(old_modules, encoding='utf-8')
        contact_path.unlink()
        operations_path.unlink()
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
        f'{len(cases)} mutation + {len(public_failures)} public Origin + '
        f'{len(delegated_failures)} delegated CSRF negative regressions'
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
