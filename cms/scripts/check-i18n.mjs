#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';

const scriptDir = path.dirname(decodeURIComponent(new URL(import.meta.url).pathname).replace(/^\/([A-Za-z]:\/)/, '$1'));
const cms = path.resolve(scriptDir, '..');
const src = path.join(cms, 'admin', 'src');
const mode = process.argv.slice(2);
if (mode.length !== 1 || !['--mode=s03', '--strict'].includes(mode[0])) {
  console.error('Usage: node cms/scripts/check-i18n.mjs --mode=s03 | --strict');
  process.exit(2);
}

const failures = [];
function readCatalog(name) {
  const file = path.join(src, 'i18n', 'locales', name + '.ts');
  const source = fs.readFileSync(file, 'utf8');
  const match = source.match(new RegExp('export const ' + name + '(?:[^=\\n]*)=\\s*(\\{[\\s\\S]*?\\})\\s+as const;'));
  if (!match) throw new Error(file + ': expected an exported flat object followed by as const');
  // Catalog entries are deterministic, JSON-compatible string literals.
  const body = match[1];
  const result = Object.create(null);
  const entry = /^\s*("(?:[^"\\]|\\.)*")\s*:\s*("(?:[^"\\]|\\.)*")\s*,?\s*$/;
  for (const line of body.slice(1, -1).split('\n')) {
    if (!line.trim()) continue;
    const fields = line.match(entry);
    if (!fields) throw new Error(file + ': invalid catalog entry: ' + line.trim());
    const key = JSON.parse(fields[1]);
    if (Object.hasOwn(result, key)) throw new Error(file + ': duplicate key ' + key);
    const value = JSON.parse(fields[2]);
    if (!value.trim()) throw new Error(file + ': empty translation ' + key);
    result[key] = value;
  }
  return result;
}

function placeholders(value) {
  return [...value.matchAll(/\{\{([a-zA-Z_][a-zA-Z0-9_]*)\}\}/g)].map(match => match[1]).sort().join(',');
}

const en = readCatalog('en');
const vi = readCatalog('vi');
for (const key of new Set([...Object.keys(en), ...Object.keys(vi)])) {
  if (!Object.hasOwn(en, key) || !Object.hasOwn(vi, key)) {
    failures.push('Catalog parity: ' + key);
  } else if (placeholders(en[key]) !== placeholders(vi[key])) {
    failures.push('Interpolation parity: ' + key);
  }
  if (key.endsWith('_one') && !Object.hasOwn(en, key.slice(0, -4) + '_other')) {
    failures.push('Missing plural form: ' + key.slice(0, -4) + '_other');
  }
  if (key.endsWith('_other') && !Object.hasOwn(en, key.slice(0, -6) + '_one')) {
    failures.push('Missing plural form: ' + key.slice(0, -6) + '_one');
  }
}

const s03 = [
  'App.tsx', 'types.ts',
  'i18n/index.tsx', 'i18n/format.ts', 'i18n/locales/en.ts',
  'api/client.ts', 'api/auth.ts', 'api/locales.ts', 'api/settings.ts',
  'shell/AdminShell.tsx', 'shell/AuthPanel.tsx', 'shell/MediaRoute.tsx', 'shell/router.ts',
  'components/AdminPrimitives.tsx', 'components/FormPrimitives.tsx', 'components/LocaleTabs.tsx',
  'features/system/GeneralSettingsPanel.tsx', 'features/system/SystemStatus.tsx',
  'features/settings/MultilingualSettingsPage.tsx', 'features/settings/ReadingSettingsPanel.tsx',
  'features/settings/PermalinkSettingsPage.tsx',
];

function walk(dir) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
    const file = path.join(dir, entry.name);
    return entry.isDirectory() ? walk(file) : /\.tsx?$/.test(entry.name) ? [file] : [];
  });
}

// Preserve quoted strings and template literals while masking source comments
// (including their newlines, so diagnostics point at the original lines).
function withoutComments(source) {
  return source.replace(/"(?:\\[\s\S]|[^"\\])*"|'(?:\\[\s\S]|[^'\\])*'|`(?:\\[\s\S]|[^`\\])*`|\/\*[\s\S]*?\*\/|\/\/[^\r\n]*/g,
    match => match.startsWith('//') || match.startsWith('/*') ? match.replace(/[^\r\n]/g, ' ') : match);
}

const vietnamese = /[\u00C0-\u00C3\u00C8-\u00CA\u00CC-\u00CD\u00D2-\u00D5\u00D9-\u00DA\u00DD\u00E0-\u00E3\u00E8-\u00EA\u00EC-\u00ED\u00F2-\u00F5\u00F9-\u00FA\u00FD\u0102\u0103\u0110\u0111\u0128\u0129\u0168\u0169\u01A0\u01A1\u01AF\u01B0\u1EA0-\u1EF9]/u;
const files = mode[0] === '--strict'
  ? walk(src).filter(file => file !== path.join(src, 'i18n', 'locales', 'vi.ts'))
  : [...s03.map(file => path.join(src, file)), path.join(cms, 'admin', 'index.html')];
for (const file of files) {
  const source = withoutComments(fs.readFileSync(file, 'utf8'));
  for (const [index, line] of source.split('\n').entries()) {
    if (vietnamese.test(line.normalize('NFC'))) failures.push(path.relative(cms, file) + ':' + (index + 1) + ': hardcoded Vietnamese: ' + line.trim());
  }
}

if (failures.length) {
  console.error(failures.join('\n'));
  process.exitCode = 1;
} else {
  console.log('i18n ' + mode[0] + ': ' + Object.keys(en).length + ' translation keys, ' + files.length + ' files checked.');
}
