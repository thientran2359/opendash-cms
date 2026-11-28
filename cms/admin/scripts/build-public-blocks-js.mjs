/**
 * Deterministic offline build & security/size verifier for /assets/opendash-blocks.js.
 * Run from any working directory: npm --prefix cms/admin run build:public-blocks-js
 */
import { readdir, readFile, mkdir, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import esbuild from 'esbuild';

const adminDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const repoRoot = resolve(adminDir, '../..');
const sourceDir = join(adminDir, 'src/public-blocks');
const source = join(sourceDir, 'opendash-blocks.js');
const target = join(repoRoot, 'cms/crates/opendash-server/src/builtin_themes/opendash-blocks.js');

const FORBIDDEN_PATTERNS = [
  { name: 'innerHTML', re: /\binnerHTML\b/ },
  { name: 'outerHTML', re: /\bouterHTML\b/ },
  { name: 'insertAdjacentHTML', re: /\binsertAdjacentHTML\b/ },
  { name: 'document.write', re: /\bdocument\.write\b/ },
  { name: 'eval(', re: /\beval\s*\(/ },
  { name: 'new Function', re: /\bnew\s+Function\b/ },
  { name: 'http:// literal', re: /http:\/\// },
  { name: 'https:// literal', re: /https:\/\// },
];

async function main() {
  const entries = (await readdir(sourceDir)).filter((f) => f.endsWith('.js')).sort();
  let totalSourceLoc = 0;
  for (const file of entries) {
    const raw = await readFile(join(sourceDir, file), 'utf8');
    const lineCount = raw.split('\n').length;
    totalSourceLoc += lineCount;
    if (lineCount > 600) {
      throw new Error(`${file} exceeds 600 LOC (${lineCount})`);
    }
    for (const check of FORBIDDEN_PATTERNS) {
      if (check.re.test(raw)) {
        throw new Error(`Forbidden pattern in ${file}: ${check.name}`);
      }
    }
  }

  const result = await esbuild.build({
    entryPoints: [source],
    bundle: true,
    minify: true,
    format: 'iife',
    target: 'es2020',
    write: false,
  });

  const code = result.outputFiles[0].text.trim() + '\n';
  const bytes = Buffer.byteLength(code, 'utf8');
  if (bytes >= 12 * 1024) {
    throw new Error(`Public blocks JS too large: ${bytes} bytes (limit < 12288)`);
  }

  for (const check of FORBIDDEN_PATTERNS) {
    if (check.re.test(code)) {
      throw new Error(`Forbidden pattern in bundled opendash-blocks.js: ${check.name}`);
    }
  }

  await mkdir(dirname(target), { recursive: true });
  await writeFile(target, code, 'utf8');
  process.stdout.write(`Built ${target} (${bytes} bytes, ${entries.length} modules, ${totalSourceLoc} total source LOC)\n`);
}

await main();
