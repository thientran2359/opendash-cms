/**
 * Deterministic offline public Tailwind v4 bundle. Run from any working
 * directory: npm --prefix cms/admin run build:public-tailwind
 */
import { readFile, readdir, mkdir, writeFile } from 'node:fs/promises';
import { dirname, extname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { compile } from 'tailwindcss';
import { optimize } from '@tailwindcss/node';
import { Scanner } from '@tailwindcss/oxide';

const adminDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const repoRoot = resolve(adminDir, '../..');
const source = join(adminDir, 'src/tailwind-public.css');
const target = join(repoRoot, 'cms/crates/opendash-server/src/builtin_themes/opendash-tailwind.css');
const scanRoots = [
  'cms/crates/opendash-server/src/builtin_blocks',
  'cms/crates/opendash-server/src/builtin_themes/opendash-starter',
  'cms/fixtures/demo-website',
];
const rustSources = [
  'cms/crates/opendash-server/src/content_render.rs',
  'cms/crates/opendash-server/src/page.rs',
  'cms/crates/opendash-server/src/routes/public.rs',
  'cms/crates/opendash-server/src/routes/public_content.rs',
];

async function scanDir(relative, files) {
  const location = join(repoRoot, relative);
  let entries;
  try {
    entries = await readdir(location, { withFileTypes: true });
  } catch (error) {
    if (error.code === 'ENOENT') return; // S02's starter theme is added after S01.
    throw error;
  }
  for (const entry of entries.sort((a, b) => a.name.localeCompare(b.name, 'en'))) {
    const path = join(relative, entry.name);
    if (entry.isDirectory()) await scanDir(path, files);
    else if (entry.isFile() && /\.(html|json)$/.test(entry.name)) files.push(join(repoRoot, path));
  }
}

async function loadStylesheet(id, base) {
  const filename = id === 'tailwindcss'
    ? join(adminDir, 'node_modules/tailwindcss/index.css')
    : id.startsWith('.')
      ? resolve(base, id)
      : null;
  if (!filename) throw new Error(`Unexpected public Tailwind import: ${id}`);
  return { content: await readFile(filename, 'utf8'), base: dirname(filename) };
}

async function main() {
  const files = [];
  for (const root of scanRoots) await scanDir(root, files);
  files.push(...rustSources.map(path => join(repoRoot, path)));
  files.push(join(adminDir, 'src/tailwind-public-safelist.txt'));
  const scanner = new Scanner({ sources: [] });
  const scanned = await Promise.all(files.map(async file => ({
    content: await readFile(file, 'utf8'),
    extension: extname(file).slice(1) || 'html',
  })));
  // Scan source contents explicitly so an unrelated checkout or node_modules
  // directory never leaks utilities into the committed single-binary asset.
  const candidates = [...new Set(scanner.scanFiles(scanned))].sort();
  const stylesheet = await compile(await readFile(source, 'utf8'), {
    base: adminDir,
    loadStylesheet,
  });
  const css = optimize(stylesheet.build(candidates), { minify: true }).code.trim() + '\n';
  const bytes = Buffer.byteLength(css);
  if (bytes >= 120 * 1024) throw new Error(`Public Tailwind bundle too large: ${bytes} bytes`);
  for (const required of ['.bg-od-primary', '.text-od-text', '.border-od-border', '.od-prose']) {
    if (!css.includes(required)) throw new Error(`Public Tailwind bundle missing ${required}`);
  }
  await mkdir(dirname(target), { recursive: true });
  await writeFile(target, css);
  process.stdout.write(`Built ${target} (${bytes} bytes, ${candidates.length} candidates)\n`);
}

await main();
