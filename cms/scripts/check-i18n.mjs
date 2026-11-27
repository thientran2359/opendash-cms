#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';

const scriptDir = path.dirname(decodeURIComponent(new URL(import.meta.url).pathname).replace(/^\/([A-Za-z]:\/)/, '$1'));
const cms = path.resolve(scriptDir, '..');
const src = path.join(cms, 'admin', 'src');
const failures = [];

function walk(dir) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
    const file = path.join(dir, entry.name);
    return entry.isDirectory() ? walk(file) : /\.tsx?$/.test(entry.name) ? [file] : [];
  });
}

function withoutComments(source) {
  return source.replace(/"(?:\\[\s\S]|[^"\\])*"|'(?:\\[\s\S]|[^'\\])*'|`(?:\\[\s\S]|[^`\\])*`|\/\*[\s\S]*?\*\/|\/\/[^\r\n]*/g,
    match => match.startsWith('//') || match.startsWith('/*') ? match.replace(/[^\r\n]/g, ' ') : match);
}

const vietnameseRe = /[àáạảãâầấậẩẫăằắặẳẵèéẹẻẽêềếệểễìíịỉĩòóọỏõôồốộổỗơờớợởỡùúụủũưừứựửữỳýỵỷỹđÀÁẠẢÃÂẦẤẬẨẪĂẰẮẶẲẴÈÉẸẺẼÊỀẾỆỂỄÌÍỊỈĨÒÓỌỎÕÔỒỐỘỔỖƠỜỚỢỞỠÙÚỤỦŨƯỪỨỰỬỮỲÝỴỶỸĐ]/;
for (const file of walk(src)) {
  const rel = path.relative(src, file).replace(/\\/g, '/');
  const content = withoutComments(fs.readFileSync(file, 'utf8'));
  const lines = content.split(/\r?\n/);
  for (let i = 0; i < lines.length; i++) {
    if (vietnameseRe.test(lines[i])) {
      failures.push(`Hardcoded non-ASCII/Vietnamese literal in ${rel}:${i + 1}`);
    }
  }
}

const potPath = path.join(cms, 'l10n', 'opendash.pot');
const viPoPath = path.join(cms, 'l10n', 'vi.po');
if (!fs.existsSync(potPath) || !fs.existsSync(viPoPath)) {
  failures.push('Missing cms/l10n/opendash.pot or cms/l10n/vi.po');
}

if (failures.length > 0) {
  console.error(failures.slice(0, 30).join('\n'));
  process.exit(1);
}
console.log('PASS check-i18n: zero hardcoded Vietnamese in admin/src and .pot/.po present.');
