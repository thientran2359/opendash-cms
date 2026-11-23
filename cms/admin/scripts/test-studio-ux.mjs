/** Node-only Studio AST regressions. esbuild is already installed by Vite. */
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { transform } from 'esbuild';

const source = await readFile(new URL('../src/builder/document.ts', import.meta.url), 'utf8');
const { code } = await transform(source, { loader: 'ts', format: 'esm', target: 'es2022' });
const {
  allNodes, appendRowWithColumns, canPasteOptions, copyNodeOptions, duplicateNode,
  isLockedElement, locate, pasteNodeOptions, preservesAnchors, reorderSiblingNode,
} = await import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);

const registry = { 'theme/banner': { title: 'text' } };
const schema = {
  schema_version: 1, id: 'plugin/hero', title: 'Hero', category: 'content', render: 'index.html',
  fields: [
    { type: 'group', key: 'headline_group', label: 'Headline', fields: [
      { type: 'text', key: 'headline', label: 'Headline' },
    ] },
    { type: 'repeater', key: 'items', label: 'Items', fields: [
      { type: 'text', key: 'label', label: 'Label' },
    ] },
    { type: 'boolean', key: 'enabled', label: 'Enabled' },
  ],
};
const activeBlock = { block_id: 'plugin/hero', owner_kind: 'plugin', owner_id: 'hero',
  status: 'active', declaration_state: 'declared', schema };
const catalog = [activeBlock];

function element(id, type = 'core/text', props = { text: id }) {
  return { id, type, props };
}
function makeDoc(elements = [element('first'), element('second')]) {
  return { schema_version: 1, sections: [
    { id: 'section-1', children: [
      { id: 'row-1', children: [{ id: 'column-1', children: elements }] },
    ] },
  ] };
}
function copy(doc) { return structuredClone(doc); }

test('duplicateNode deep-clones a subtree, assigns unique IDs, and inserts after source', () => {
  const original = makeDoc();
  const next = duplicateNode(original, 'row-1', registry, catalog);
  assert.ok(next);
  assert.equal(original.sections[0].children.length, 1);
  assert.equal(next.document.sections[0].children.length, 2);
  assert.equal(next.id, next.document.sections[0].children[1].id);
  assert.deepEqual(next.document.sections[0].children[1].children[0].children.map(n => n.props.text), ['first', 'second']);
  const ids = allNodes(next.document).map(item => item.node.id);
  assert.equal(new Set(ids).size, ids.length);
  next.document.sections[0].children[1].children[0].children[0].props.text = 'changed';
  assert.equal(original.sections[0].children[0].children[0].children[0].props.text, 'first');
});

test('duplicateNode rejects locked subtree, content slot, and an anchor shifted by insertion', () => {
  const locked = element('lock', 'plugin/block', { block_id: 'plugin/hero', values: {} });
  const doc = makeDoc([element('first'), locked, element('third')]);
  const inactive = [{ ...activeBlock, status: 'provider_inactive' }];
  const unchanged = copy(doc);
  assert.equal(duplicateNode(doc, 'lock', registry, inactive), null);
  assert.equal(duplicateNode(doc, 'column-1', registry, inactive), null);
  assert.equal(duplicateNode(doc, 'first', registry, inactive), null);
  assert.ok(duplicateNode(doc, 'third', registry, inactive));
  assert.deepEqual(doc, unchanged);
  assert.equal(duplicateNode(makeDoc([element('slot', 'core/content_slot', {})]), 'slot', registry), null);
  assert.equal(duplicateNode(makeDoc([element('slot', 'core/content_slot', {})]), 'row-1', registry), null);
});

test('historical unresolved core props remain immutable anchors for duplicate, reorder and paste', () => {
  const legacy = element('historical', 'core/text', { text: 'Saved', unsupported_legacy: true });
  const doc = makeDoc([element('first'), legacy, element('last')]);
  assert.equal(isLockedElement(legacy, registry, catalog), true);
  assert.equal(copyNodeOptions(doc, 'historical', registry, catalog), null);
  assert.equal(duplicateNode(doc, 'historical', registry, catalog), null);
  assert.equal(duplicateNode(doc, 'column-1', registry, catalog), null);
  assert.equal(duplicateNode(doc, 'first', registry, catalog), null);
  assert.equal(reorderSiblingNode(doc, 'first', 'last', 'after', registry, catalog), null);
  const source = copyNodeOptions(doc, 'first', registry, catalog);
  assert.equal(pasteNodeOptions(doc, 'historical', source), null);
  assert.ok(duplicateNode(doc, 'last', registry, catalog));
  assert.deepEqual(doc.sections[0].children[0].children[0].children.map(n => n.id),
    ['first', 'historical', 'last']);
  // The server also rejects unexpected element-level fields and binding on non-bindable types.
  assert.equal(isLockedElement({ ...element('extra'), unexpected: true }, registry, catalog), true);
  const legacyBinding = element('old-binding', 'core/text', {
    text: 'Saved', binding: {
      type_key: 'page', entry_slug: 'home', target: 'text',
      source: { kind: 'title', unexpected_legacy: true },
    },
  });
  assert.equal(isLockedElement(legacyBinding, registry, catalog), true);
  assert.equal(pasteNodeOptions(makeDoc([legacyBinding]), 'old-binding', source), null);
  assert.equal(isLockedElement(element('video', 'core/video', {
    media_id: '', controls: true, muted: false, binding: { kind: 'title' },
  }), registry, catalog), true);
});

test('duplicateNode enforces list count, 512-node bound, and UTF-8 256KiB bound', () => {
  const lists = Array.from({ length: 8 }, (_, n) => element(`list-${n}`, 'core/list', {
    type_key: 'post', include_descendants: false, sort: 'id_desc', page_size: 6,
  }));
  assert.equal(duplicateNode(makeDoc(lists), 'list-0', registry), null);
  assert.equal(duplicateNode(makeDoc(Array.from({ length: 509 }, (_, n) => element(`el-${n}`))), 'el-0', registry), null);
  const unicode = makeDoc([element('large', 'core/text', { text: '界'.repeat(85_000) })]);
  assert.equal(duplicateNode(unicode, 'large', registry), null);
  const patterns = makeDoc(Array.from({ length: 16 }, (_, n) =>
    element(`pattern-${n}`, 'core/pattern_ref', { pattern_id: n + 1 })));
  assert.ok(duplicateNode(patterns, 'pattern-0', registry));
});

test('schema evolution: fresh plugin copies must satisfy newly required visible fields', () => {
  const evolvedSchema = { ...schema, fields: [
    { type: 'boolean', key: 'show_more', label: 'Show more', default: false },
    { type: 'text', key: 'required_new', label: 'Required', required: true },
    { type: 'group', key: 'details', label: 'Details', show_if: { field: 'show_more', equals: true },
      fields: [{ type: 'text', key: 'conditional', label: 'Conditional', required: true }] },
    { type: 'repeater', key: 'items', label: 'Items', min_items: 1, fields: [
      { type: 'text', key: 'label', label: 'Label', required: true },
    ] },
  ] };
  const evolvedCatalog = [{ ...activeBlock, schema: evolvedSchema }];
  const old = makeDoc([element('old', 'plugin/block', { block_id: 'plugin/hero', values: {
    show_more: false, items: [{ label: 'Good' }],
  } })]);
  assert.equal(duplicateNode(old, 'old', registry, evolvedCatalog), null);
  assert.equal(duplicateNode(old, 'row-1', registry, evolvedCatalog), null);
  assert.equal(copyNodeOptions(old, 'old', registry, evolvedCatalog), null);
  const ready = copy(old);
  ready.sections[0].children[0].children[0].children[0].props.values.required_new = 'Ready';
  assert.ok(duplicateNode(ready, 'old', registry, evolvedCatalog));
  assert.ok(copyNodeOptions(ready, 'old', registry, evolvedCatalog));
  ready.sections[0].children[0].children[0].children[0].props.values.show_more = true;
  assert.equal(duplicateNode(ready, 'old', registry, evolvedCatalog), null);
  ready.sections[0].children[0].children[0].children[0].props.values.conditional = 'Visible';
  assert.ok(duplicateNode(ready, 'old', registry, evolvedCatalog));
  ready.sections[0].children[0].children[0].children[0].props.values.items = [{ label: '' }];
  assert.equal(duplicateNode(ready, 'old', registry, evolvedCatalog), null);
});

test('appendRowWithColumns creates an atomic 1/2/3-column grid and stays sparse for single column', () => {
  for (const count of [1, 2, 3]) {
    const doc = makeDoc();
    const result = appendRowWithColumns(doc, 'section-1', count);
    assert.ok(result);
    assert.equal(doc.sections[0].children.length, 1);
    const row = result.document.sections[0].children[1];
    assert.equal(row.id, result.id);
    assert.equal(row.children.length, count);
    assert.ok(row.children.every(column => column.children.length === 0));
    assert.equal(new Set(allNodes(result.document).map(entry => entry.node.id)).size, allNodes(result.document).length);
    if (count === 1) assert.equal(Object.hasOwn(row, 'responsive'), false);
    else assert.deepEqual(row.responsive, { desktop: { columns_per_row: count } });
  }
  assert.equal(appendRowWithColumns(makeDoc(), 'first', 2), null);
  assert.equal(appendRowWithColumns(makeDoc(), 'section-1', 4), null);
  assert.equal(appendRowWithColumns(makeDoc(Array.from({ length: 508 }, (_, n) => element(`el-${n}`))), 'section-1', 1), null);
});

test('copy/paste container options deep-clones responsive and deletes empty overrides', () => {
  const doc = makeDoc();
  doc.sections[0].responsive = { desktop: { padding_px: 20 }, mobile: { padding_px: 0 } };
  const selected = copyNodeOptions(doc, 'section-1', registry);
  assert.deepEqual(selected, { kind: 'section', responsive: doc.sections[0].responsive });
  const pasted = pasteNodeOptions(doc, 'section-1', selected);
  assert.ok(pasted);
  pasted.sections[0].responsive.desktop.padding_px = 42;
  assert.equal(doc.sections[0].responsive.desktop.padding_px, 20);

  const clear = copyNodeOptions(makeDoc(), 'section-1', registry);
  assert.equal(clear.responsive, undefined);
  const cleared = pasteNodeOptions(doc, 'section-1', clear);
  assert.equal(Object.hasOwn(cleared.sections[0], 'responsive'), false);
  assert.equal(canPasteOptions(clear, locate(doc, 'row-1'), false), false);
  assert.equal(canPasteOptions(clear, locate(doc, 'section-1'), true), false);
});

test('core options omit source binding and preserve the target binding', () => {
  const binding = { type_key: 'page', entry_slug: 'home', source: { kind: 'title' }, target: 'text' };
  const doc = makeDoc([
    element('source', 'core/text', { text: 'From source', binding }),
    element('target', 'core/text', { text: 'Original', binding: { ...binding, entry_slug: 'target' } }),
    element('other', 'core/button', { label: 'Press', url: '/' }),
  ]);
  const clipboard = copyNodeOptions(doc, 'source', registry);
  assert.ok(clipboard);
  assert.equal(Object.hasOwn(clipboard.props, 'binding'), false);
  assert.equal(canPasteOptions(clipboard, locate(doc, 'other'), false), false);
  assert.equal(canPasteOptions(clipboard, locate(doc, 'target'), false), true);
  const pasted = pasteNodeOptions(doc, 'target', clipboard);
  assert.equal(locate(pasted, 'target').node.props.text, 'From source');
  assert.equal(locate(pasted, 'target').node.props.binding.entry_slug, 'target');
  assert.equal(locate(doc, 'target').node.props.text, 'Original');
  assert.equal(pasteNodeOptions(doc, 'other', clipboard), null);
});

test('pasting a Pattern reference cannot introduce a seventeenth distinct ID', () => {
  const patterns = makeDoc(Array.from({ length: 17 }, (_, i) =>
    element(`pattern-${i}`, 'core/pattern_ref', { pattern_id: i === 16 ? 1 : i + 1 })));
  const foreign = makeDoc([element('different', 'core/pattern_ref', { pattern_id: 17 })]);
  const clipboard = copyNodeOptions(foreign, 'different', registry, catalog);
  assert.ok(clipboard);
  assert.equal(pasteNodeOptions(patterns, 'pattern-16', clipboard), null);
  const same = copyNodeOptions(patterns, 'pattern-1', registry, catalog);
  const next = pasteNodeOptions(patterns, 'pattern-16', same);
  assert.ok(next);
  assert.equal(locate(next, 'pattern-16').node.props.pattern_id, 2);
  assert.equal(locate(patterns, 'pattern-16').node.props.pattern_id, 1);
});

test('plugin copy drops unknown values, flattens group keys, and never adds top-level binding', () => {
  const values = { headline: 'Headline', items: [{ label: 'one' }], enabled: true,
    unknown_legacy: { private: true }, in_flight_draft: undefined };
  const doc = makeDoc([
    element('p1', 'plugin/block', { block_id: 'plugin/hero', values }),
    element('p2', 'plugin/block', { block_id: 'plugin/hero', values: { headline: 'before' } }),
    element('p3', 'plugin/block', { block_id: 'plugin/other', values: {} }),
  ]);
  const clipboard = copyNodeOptions(doc, 'p1', registry, catalog);
  assert.deepEqual(clipboard.props, { values: { headline: 'Headline', items: [{ label: 'one' }], enabled: true } });
  assert.equal(clipboard.blockId, 'plugin/hero');
  assert.equal(canPasteOptions(clipboard, locate(doc, 'p2'), false), true);
  assert.equal(canPasteOptions(clipboard, locate(doc, 'p3'), false), false);
  const pasted = pasteNodeOptions(doc, 'p2', clipboard);
  assert.deepEqual(locate(pasted, 'p2').node.props, { block_id: 'plugin/hero', values: clipboard.props.values });
  assert.deepEqual(locate(doc, 'p1').node.props.values, values);
  assert.equal(copyNodeOptions(doc, 'p1', registry, [{ ...activeBlock, declaration_state: 'withdrawn' }]), null);
  assert.equal(pasteNodeOptions(doc, 'p3', clipboard), null);
});

test('legacy options update values but preserve block identity', () => {
  const doc = makeDoc([
    element('legacy-source', 'legacy/block', { block_id: 'theme/banner', values: { title: 'Welcome' } }),
    element('legacy-target', 'legacy/block', { block_id: 'theme/banner', values: { title: 'Old' } }),
  ]);
  const clipboard = copyNodeOptions(doc, 'legacy-source', registry);
  assert.ok(clipboard);
  assert.equal(clipboard.blockId, 'theme/banner');
  const next = pasteNodeOptions(doc, 'legacy-target', clipboard);
  assert.deepEqual(locate(next, 'legacy-target').node.props, { block_id: 'theme/banner', values: { title: 'Welcome' } });
});

test('reorderSiblingNode supports before/after, rejects cross-parent and no-op', () => {
  const doc = makeDoc([element('a'), element('b'), element('c')]);
  const after = reorderSiblingNode(doc, 'a', 'c', 'after', registry);
  assert.deepEqual(locate(after, 'column-1').node.children.map(n => n.id), ['b', 'c', 'a']);
  const before = reorderSiblingNode(doc, 'c', 'a', 'before', registry);
  assert.deepEqual(locate(before, 'column-1').node.children.map(n => n.id), ['c', 'a', 'b']);
  assert.equal(reorderSiblingNode(doc, 'a', 'a', 'after', registry), null);
  assert.equal(reorderSiblingNode(doc, 'a', 'b', 'before', registry), null);
  assert.equal(reorderSiblingNode(doc, 'a', 'row-1', 'after', registry), null);
  assert.deepEqual(locate(doc, 'column-1').node.children.map(n => n.id), ['a', 'b', 'c']);
});

test('reorderSiblingNode preserves immutable anchor position and ancestor IDs', () => {
  const unknown = element('locked', 'plugin/block', { block_id: 'plugin/hero', values: {} });
  const doc = makeDoc([element('before-1'), element('before-2'), unknown, element('after-1'), element('after-2')]);
  const inactive = [{ ...activeBlock, status: 'provider_inactive' }];
  assert.equal(reorderSiblingNode(doc, 'locked', 'after-1', 'after', registry, inactive), null);
  assert.equal(reorderSiblingNode(doc, 'before-1', 'after-1', 'after', registry, inactive), null);
  assert.equal(reorderSiblingNode(doc, 'after-1', 'before-2', 'before', registry, inactive), null);
  const valid = reorderSiblingNode(doc, 'after-2', 'after-1', 'before', registry, inactive);
  assert.ok(valid);
  assert.equal(preservesAnchors(doc, valid, registry, undefined, inactive), true);
  assert.deepEqual(locate(valid, 'column-1').node.children.map(n => n.id),
    ['before-1', 'before-2', 'locked', 'after-2', 'after-1']);
});
