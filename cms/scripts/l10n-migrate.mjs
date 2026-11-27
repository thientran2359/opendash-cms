#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';

const scriptDir = path.dirname(decodeURIComponent(new URL(import.meta.url).pathname).replace(/^\/([A-Za-z]:\/)/, '$1'));
const cms = path.resolve(scriptDir, '..');
const args = new Set(process.argv.slice(2));
const applyAdmin = args.has('--apply-admin');

// 1. Parse i18n.rs EXACT_ERROR_ENTRIES & TEMPLATE_MATCHERS
const i18nRs = fs.readFileSync(path.join(cms, 'crates/opendash-server/src/i18n.rs'), 'utf8');

function extractBracketBlock(src, anchor) {
  const idx = src.indexOf(anchor);
  if (idx < 0) throw new Error(`Missing anchor: ${anchor}`);
  const eq = src.indexOf('= &[', idx + anchor.length);
  if (eq < 0) throw new Error(`Missing '= &[' for ${anchor}`);
  const openBracket = eq + 3;
  let depth = 0;
  let inStr = false;
  let esc = false;
  for (let i = openBracket; i < src.length; i++) {
    const ch = src[i];
    if (inStr) {
      if (esc) { esc = false; continue; }
      if (ch === '\\') { esc = true; continue; }
      if (ch === '"') inStr = false;
      continue;
    }
    if (ch === '"') { inStr = true; continue; }
    if (ch === '[') depth++;
    else if (ch === ']') {
      depth--;
      if (depth === 0) return src.slice(openBracket + 1, i);
    }
  }
  throw new Error(`Unclosed block for ${anchor}`);
}

function parseRustString(src, start) {
  if (src[start] !== '"') throw new Error(`Expected quote at ${start}`);
  let out = '';
  let i = start + 1;
  while (i < src.length) {
    const ch = src[i];
    if (ch === '\\') {
      const next = src[i + 1];
      if (next === 'n') out += '\n';
      else if (next === 't') out += '\t';
      else if (next === 'r') out += '\r';
      else if (next === '"') out += '"';
      else if (next === '\\') out += '\\';
      else out += next;
      i += 2;
      continue;
    }
    if (ch === '"') return { value: out, next: i + 1 };
    out += ch;
    i++;
  }
  throw new Error('Unterminated Rust string');
}

function parseExactEntries(block) {
  const entries = [];
  let i = 0;
  while (i < block.length) {
    if (block[i] !== '(') { i++; continue; }
    i++;
    const strs = [];
    while (i < block.length && block[i] !== ')') {
      if (block[i] === '"') {
        const parsed = parseRustString(block, i);
        strs.push(parsed.value);
        i = parsed.next;
      } else {
        i++;
      }
    }
    if (strs.length === 3) entries.push(strs); // [code, vi, en]
    i++;
  }
  return entries;
}

function parseTemplateMatchers(block) {
  const entries = [];
  let i = 0;
  while (i < block.length) {
    if (block[i] !== '(') { i++; continue; }
    i++;
    while (i < block.length && block[i] !== '"') i++;
    const codeParsed = parseRustString(block, i);
    i = codeParsed.next;
    const arrays = [];
    while (i < block.length && arrays.length < 2) {
      if (block[i] === '[') {
        i++;
        const parts = [];
        while (i < block.length && block[i] !== ']') {
          if (block[i] === '"') {
            const p = parseRustString(block, i);
            parts.push(p.value);
            i = p.next;
          } else {
            i++;
          }
        }
        arrays.push(parts);
        i++;
      } else {
        i++;
      }
    }
    if (arrays.length === 2) entries.push([codeParsed.value, arrays[0], arrays[1]]); // [code, vi_parts, en_parts]
  }
  return entries;
}

const exact = parseExactEntries(extractBracketBlock(i18nRs, 'const EXACT_ERROR_ENTRIES'));
const templates = parseTemplateMatchers(extractBracketBlock(i18nRs, 'const TEMPLATE_MATCHERS'));

// 2. Parse Admin locales/en.ts & vi.ts
function readTsCatalog(filePath, varName) {
  const src = fs.readFileSync(filePath, 'utf8');
  const match = src.match(new RegExp(`export const ${varName}(?:[^=\\n]*)=\\s*(\\{[\\s\\S]*?\\})\\s+as const;`));
  if (!match) throw new Error(`Cannot parse ${filePath}`);
  const result = new Map();
  const entryRe = /^\s*("(?:[^"\\]|\\.)*")\s*:\s*("(?:[^"\\]|\\.)*")\s*,?\s*$/;
  for (const line of match[1].slice(1, -1).split('\n')) {
    if (!line.trim()) continue;
    const m = line.match(entryRe);
    if (!m) throw new Error(`Invalid TS entry in ${filePath}: ${line}`);
    result.set(JSON.parse(m[1]), JSON.parse(m[2]));
  }
  return result;
}

const adminEnPath = path.join(cms, 'admin/src/i18n/locales/en.ts');
const adminViPath = path.join(cms, 'admin/src/i18n/locales/vi.ts');
const adminEn = fs.existsSync(adminEnPath) ? readTsCatalog(adminEnPath, 'en') : new Map();
const adminVi = fs.existsSync(adminViPath) ? readTsCatalog(adminViPath, 'vi') : new Map();

// Detect English collisions with distinct Vietnamese translations -> auto msgctxt
const enToViSet = new Map();
for (const [key, enText] of adminEn) {
  const viText = adminVi.get(key) || enText;
  if (!enToViSet.has(enText)) enToViSet.set(enText, new Set());
  enToViSet.get(enText).add(viText);
}

// Group _one / _other plural keys
const adminKeyMapping = new Map(); // key -> { fn, msgid, msgidPlural?, msgctxt? }
const handledPluralBase = new Set();

for (const [key, enText] of adminEn) {
  if (key.endsWith('_one') && adminEn.has(key.slice(0, -4) + '_other')) {
    const base = key.slice(0, -4);
    if (handledPluralBase.has(base)) continue;
    handledPluralBase.add(base);
    const otherText = adminEn.get(base + '_other');
    const viOther = adminVi.get(base + '_other') || adminVi.get(key) || otherText;
    const ctx = enToViSet.get(enText).size > 1 ? key.split('.')[0] : null;
    adminKeyMapping.set(base, {
      fn: ctx ? '_nx' : '_n',
      msgid: enText,
      msgidPlural: otherText,
      msgctxt: ctx,
      viPlural: [viOther],
    });
    continue;
  }
  if (key.endsWith('_other') && adminEn.has(key.slice(0, -6) + '_one')) {
    continue;
  }
  const viText = adminVi.get(key) || enText;
  const ctx = enToViSet.get(enText).size > 1 ? key.split('.').slice(0, 2).join('.') : null;
  adminKeyMapping.set(key, {
    fn: ctx ? '_x' : '__',
    msgid: enText,
    msgctxt: ctx,
    vi: viText,
  });
}

// 3. Collect unified catalog entries for .pot & vi.po
const catalogMap = new Map(); // `${ctx ?? ''}\u0004${msgid}` -> { msgctxt, msgid, msgidPlural, vi, viPlural, refs }
function addEntry({ msgctxt = null, msgid, msgidPlural = null, vi, viPlural = null, ref }) {
  if (!msgid) return;
  const k = `${msgctxt ?? ''}\u0004${msgid}`;
  if (!catalogMap.has(k)) {
    catalogMap.set(k, { msgctxt, msgid, msgidPlural, vi, viPlural, refs: new Set() });
  } else {
    const existing = catalogMap.get(k);
    if (msgidPlural && !existing.msgidPlural) {
      existing.msgidPlural = msgidPlural;
      existing.viPlural = viPlural;
    }
  }
  if (ref) catalogMap.get(k).refs.add(ref);
}

for (const [code, vi, en] of exact) {
  addEntry({ msgid: en, vi, ref: `server:error:${code}` });
}
for (const [code, viParts, enParts] of templates) {
  const toTemplate = parts => parts.map((p, idx) => idx < parts.length - 1 ? `${p}{${idx}}` : p).join('');
  addEntry({ msgid: toTemplate(enParts), vi: toTemplate(viParts), ref: `server:template:${code}` });
}

// Core L10n public fallback strings
const corePublic = [
  ['vi', 'en'],
  ['Có', 'Yes'],
  ['Không', 'No'],
  ['Chưa có nội dung.', 'No content yet.'],
  ['← Trang trước', '← Previous'],
  ['Trang sau →', 'Next →'],
  ['Nội dung mẫu không hợp lệ.', 'Invalid template content.'],
  ['Ảnh minh họa ', 'Illustration '],
  ['Danh sách chưa khả dụng.', 'List is currently unavailable.'],
  ['Trang này không có nội dung.', 'This page has no content.'],
  ['Về trang đầu', 'Back to first page'],
  ['Trang trước', 'Previous'],
  ['Trang sau', 'Next'],
  ['Biểu mẫu liên hệ hiện không khả dụng.', 'Contact form is currently unavailable.'],
  ['Pattern không còn tồn tại.', 'Pattern no longer exists.'],
  ['-- Chọn --', '-- Select --'],
  ['Để trống', 'Leave blank'],
  ['Chưa cấu hình trang chủ.', 'Homepage is not configured.'],
  ['404 — Không tìm thấy trang', '404 — Page not found'],
  ['Lỗi gửi biểu mẫu', 'Form submission error'],
  ['Không thể gửi biểu mẫu', 'Unable to submit form'],
  ['Đã nhận liên hệ', 'Message received'],
  ['Cảm ơn bạn đã liên hệ', 'Thank you for contacting us'],
  ['Thông tin của bạn đã được tiếp nhận.', 'Your message has been received.'],
  ['Trang chủ', 'Home'],
];
for (const [vi, en] of corePublic) {
  if (en === 'en') continue;
  addEntry({ msgid: en, vi, ref: 'server:core_l10n' });
}

for (const [key, item] of adminKeyMapping) {
  addEntry({
    msgctxt: item.msgctxt,
    msgid: item.msgid,
    msgidPlural: item.msgidPlural ?? null,
    vi: item.vi ?? null,
    viPlural: item.viPlural ?? null,
    ref: `admin:${key}`,
  });
}

function poQuote(str) {
  const escaped = str
    .replace(/\\/g, '\\\\')
    .replace(/"/g, '\\"')
    .replace(/\r/g, '\\r')
    .replace(/\t/g, '\\t')
    .replace(/\n/g, '\\n');
  return `"${escaped}"`;
}

function renderPo(isPot) {
  const lines = [
    isPot
      ? '# OpenDash CMS Translation Template.'
      : '# Vietnamese translation for OpenDash CMS.',
    'msgid ""',
    'msgstr ""',
    '"Project-Id-Version: OpenDash CMS 1.0.0\\n"',
    '"Content-Type: text/plain; charset=UTF-8\\n"',
    '"Content-Transfer-Encoding: 8bit\\n"',
    isPot
      ? '"Plural-Forms: nplurals=2; plural=(n != 1);\\n"'
      : '"Language: vi\\n"',
    ...(isPot ? [] : ['"Plural-Forms: nplurals=1; plural=0;\\n"']),
    '',
  ];

  const sorted = [...catalogMap.values()].sort((a, b) =>
    `${a.msgctxt ?? ''}\u0004${a.msgid}`.localeCompare(`${b.msgctxt ?? ''}\u0004${b.msgid}`)
  );

  for (const item of sorted) {
    if (item.refs.size > 0) {
      lines.push(`#: ${[...item.refs].slice(0, 5).join(' ')}`);
    }
    if (item.msgctxt !== null) {
      lines.push(`msgctxt ${poQuote(item.msgctxt)}`);
    }
    lines.push(`msgid ${poQuote(item.msgid)}`);
    if (item.msgidPlural) {
      lines.push(`msgid_plural ${poQuote(item.msgidPlural)}`);
      if (isPot) {
        lines.push('msgstr[0] ""');
        lines.push('msgstr[1] ""');
      } else {
        lines.push(`msgstr[0] ${poQuote(item.viPlural?.[0] ?? item.msgidPlural)}`);
      }
    } else {
      lines.push(`msgstr ${isPot ? '""' : poQuote(item.vi ?? item.msgid)}`);
    }
    lines.push('');
  }
  return lines.join('\n');
}

const l10nDir = path.join(cms, 'l10n');
fs.mkdirSync(l10nDir, { recursive: true });
fs.writeFileSync(path.join(l10nDir, 'opendash.pot'), renderPo(true), 'utf8');
fs.writeFileSync(path.join(l10nDir, 'vi.po'), renderPo(false), 'utf8');
fs.writeFileSync(
  path.join(l10nDir, 'error_codes.json'),
  JSON.stringify({ exact, templates }),
  'utf8',
);

console.log(`Generated opendash.pot (${catalogMap.size} entries), vi.po, error_codes.json (${exact.length} exact, ${templates.length} templates).`);

if (applyAdmin) {
  // Export mapping JSON for S03 Admin codemod
  const adminMapObj = {};
  for (const [k, v] of adminKeyMapping) adminMapObj[k] = v;
  fs.writeFileSync(path.join(l10nDir, 'admin-codemod-map.json'), JSON.stringify(adminMapObj, null, 2), 'utf8');
  console.log(`Exported admin-codemod-map.json (${Object.keys(adminMapObj).length} entries).`);
}
