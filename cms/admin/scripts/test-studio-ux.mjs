/** Node-only Studio AST regressions. esbuild is already installed by Vite. */
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build, transform } from 'esbuild';

const { outputFiles: [{ text: code }] } = await build({
  entryPoints: [fileURLToPath(new URL('../src/builder/document.ts', import.meta.url))],
  bundle: true, write: false, platform: 'node', format: 'esm', target: 'es2022',
});
const {
  allNodes, appendFreeformElement, appendRowWithColumns, canPasteOptions, copyNodeOptions, duplicateNode,
  isLockedElement, locate, nodeTitle, pasteNodeOptions, patchNodeFields, preservesAnchors, removeNode,
  reorderSiblingNode, setContainerBoxSpacing, setNodeMeta, setResponsiveField, setSectionStyle,
  syncContainerResponsiveToDesktop, visibleNodesInTree,
} = await import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);

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

// Compile the actual TS bridge literal to JS so the regression hashes the exact
// bytes that preparePreviewMarkup injects into the CSP-protected iframe.
const frameSource = await readFile(new URL('../src/builder/BuilderPreviewFrame.tsx', import.meta.url), 'utf8');
const bridgeLiteral = frameSource.match(/const BRIDGE_SCRIPT = (\[[\s\S]*?\])\.join\('\\n'\);/);
assert.ok(bridgeLiteral, 'BRIDGE_SCRIPT array must remain statically extractable');
const { code: bridgeEvaluator } = await transform(
  "globalThis.bridgeScript = " + bridgeLiteral[1] + ".join('\\n');",
  { loader: 'ts', format: 'iife', target: 'es2022' },
);
const bridgeScope = {};
runInNewContext(bridgeEvaluator, bridgeScope);
const bridgeScript = bridgeScope.bridgeScript;

test('iframe script SHA-256 matches CSP and its DOM creation never uses innerHTML', () => {
  const shaLiteral = frameSource.match(/const BRIDGE_SCRIPT_SHA256 = '([^']+)'/);
  assert.ok(shaLiteral, 'CSP hash must be present');
  assert.equal(createHash('sha256').update(bridgeScript, 'utf8').digest('base64'), shaLiteral[1]);
  assert.doesNotMatch(bridgeScript, /\binnerHTML\b|\binsertAdjacentHTML\b/);
  assert.match(frameSource, /sandbox=\{interactive \? 'allow-scripts' : ''\}/);
});

test('canvas bridge mounts only Section/non-replaced Element gear; gear click and dblclick have separate menu modes', () => {
  class Node {
    constructor(tag, id, kind) {
      this.tagName = tag.toUpperCase();
      this.attributes = new Map();
      this.children = [];
      this.parentNode = null;
      if (id) {
        this.setAttribute('data-od-editable', '1');
        this.setAttribute('data-node-id', id);
        this.setAttribute('data-od-kind', kind);
      }
    }
    getAttribute(key) { return this.attributes.get(key) ?? null; }
    setAttribute(key, value) { this.attributes.set(key, String(value)); }
    removeAttribute(key) { this.attributes.delete(key); }
    appendChild(node) { node.parentNode = this; this.children.push(node); return node; }
    closest(selector) {
      for (let current = this; current; current = current.parentNode) {
        if (selector === '[data-od-gear]' && current.getAttribute('data-od-gear') !== null) return current;
        if (selector.includes('[data-od-editable=') && current.getAttribute('data-od-editable') === '1'
          && current.getAttribute('data-node-id')) return current;
        if (selector === 'a,button,input,select,textarea,form'
          && ['A', 'BUTTON', 'INPUT', 'SELECT', 'TEXTAREA', 'FORM'].includes(current.tagName)) return current;
      }
      return null;
    }
    getBoundingClientRect() { return { left: 10, top: 20, right: 34, bottom: 44 }; }
  }
  const section = new Node('section', 'section-id', 'section');
  const row = new Node('div', 'row-id', 'row');
  const column = new Node('div', 'column-id', 'column');
  const element = new Node('div', 'element-id', 'element');
  const image = new Node('img', 'image-id', 'element');
  const video = new Node('video', 'video-id', 'element');
  const nodes = [section, row, column, element, image, video];
  const listeners = new Map();
  const messages = [];
  const document = {
    querySelector(selector) {
      if (selector === 'meta[name="od-studio-nonce"]') return { getAttribute: () => 'test-nonce' };
      return null;
    },
    querySelectorAll(selector) {
      if (selector.includes('[data-node-id]')) return nodes;
      if (selector.includes('[data-od-selected]')) return nodes.filter(n => n.getAttribute('data-od-selected') === '1');
      if (selector.includes('[data-od-hover]')) return nodes.filter(n => n.getAttribute('data-od-hover') === '1');
      return [];
    },
    createElement: tag => new Node(tag),
    createElementNS: (_namespace, tag) => new Node(tag),
    addEventListener: (type, callback) => { listeners.set(type, callback); },
  };
  const window = {
    parent: { postMessage: message => messages.push(message) },
    addEventListener() {},
    scrollTo() {},
    scrollX: 0,
    scrollY: 0,
  };
  runInNewContext(bridgeScript, { document, window, setTimeout, clearTimeout });

  assert.equal(section.children.length, 1);
  assert.equal(element.children.length, 1);
  assert.equal(section.children[0].getAttribute('data-od-gear'), 'section-id');
  assert.equal(element.children[0].getAttribute('data-od-gear'), 'element-id');
  assert.equal(section.children[0].tagName, 'BUTTON');
  assert.equal(element.children[0].children[0].tagName, 'SVG');
  for (const node of [row, column, image, video]) assert.equal(node.children.length, 0);
  assert.deepEqual(nodes.flatMap(node => node.children.map(child => child.getAttribute('data-od-gear'))),
    ['section-id', 'element-id'], 'bridge must only add the two gear controls');

  function dispatch(type, target, coords = { clientX: 0, clientY: 0 }) {
    const state = { prevented: false, stopped: false };
    listeners.get(type)({
      target,
      clientX: coords.clientX,
      clientY: coords.clientY,
      preventDefault() { state.prevented = true; },
      stopPropagation() { state.stopped = true; },
    });
    return state;
  }
  // The click may start on the gear's nested SVG path rather than the button.
  const gearPath = element.children[0].children[0].children[0];
  assert.deepEqual(dispatch('click', gearPath), { prevented: true, stopped: true });
  assert.deepEqual(JSON.parse(JSON.stringify(messages)), [{
    type: 'od-studio-menu', nonce: 'test-nonce', nodeId: 'element-id', mode: 'toggle',
    rect: { left: 34, top: 20 },
  }]);
  assert.equal(element.getAttribute('data-od-selected'), '1');
  messages.length = 0;
  dispatch('dblclick', gearPath);
  assert.equal(messages.length, 0, 'double-click on gear must not dispatch an edit after toggle');
  assert.deepEqual(dispatch('dblclick', section), { prevented: true, stopped: true });
  assert.equal(messages.length, 1);
  assert.equal(messages[0].type, 'od-studio-edit');
  assert.equal(messages[0].nodeId, 'section-id');
  assert.equal(section.getAttribute('data-od-selected'), '1');
  assert.equal(element.getAttribute('data-od-selected'), null);
  messages.length = 0;
  dispatch('click', row);
  assert.equal(messages.length, 1);
  assert.equal(messages[0].type, 'od-studio-pick', 'ordinary canvas selection must remain intact');
  messages.length = 0;
  assert.deepEqual(dispatch('contextmenu', row, { clientX: 120, clientY: 240 }), { prevented: true, stopped: true });
  assert.deepEqual(JSON.parse(JSON.stringify(messages)), [{
    type: 'od-studio-menu', nonce: 'test-nonce', nodeId: 'row-id', mode: 'open',
    rect: { left: 120, top: 240 },
  }]);
  assert.equal(row.getAttribute('data-od-selected'), '1');
});

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
  const next = duplicateNode(original, 'row-1', catalog);
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
  assert.equal(duplicateNode(doc, 'lock', inactive), null);
  assert.equal(duplicateNode(doc, 'column-1', inactive), null);
  assert.equal(duplicateNode(doc, 'first', inactive), null);
  assert.ok(duplicateNode(doc, 'third', inactive));
  assert.deepEqual(doc, unchanged);
  assert.equal(duplicateNode(makeDoc([element('slot', 'core/content_slot', {})]), 'slot'), null);
  assert.equal(duplicateNode(makeDoc([element('slot', 'core/content_slot', {})]), 'row-1'), null);
});

test('historical unresolved core props remain immutable anchors for duplicate, reorder and paste', () => {
  const legacy = element('historical', 'core/text', { text: 'Saved', unsupported_legacy: true });
  const doc = makeDoc([element('first'), legacy, element('last')]);
  assert.equal(isLockedElement(legacy, catalog), true);
  assert.equal(copyNodeOptions(doc, 'historical', catalog), null);
  assert.equal(duplicateNode(doc, 'historical', catalog), null);
  assert.equal(duplicateNode(doc, 'column-1', catalog), null);
  assert.equal(duplicateNode(doc, 'first', catalog), null);
  assert.equal(reorderSiblingNode(doc, 'first', 'last', 'after', catalog), null);
  const source = copyNodeOptions(doc, 'first', catalog);
  assert.equal(pasteNodeOptions(doc, 'historical', source), null);
  assert.ok(duplicateNode(doc, 'last', catalog));
  assert.deepEqual(doc.sections[0].children[0].children[0].children.map(n => n.id),
    ['first', 'historical', 'last']);
  // The server also rejects unexpected element-level fields and binding on non-bindable types.
  assert.equal(isLockedElement({ ...element('extra'), unexpected: true }, catalog), true);
  const legacyBinding = element('old-binding', 'core/text', {
    text: 'Saved', binding: {
      type_key: 'page', entry_slug: 'home', target: 'text',
      source: { kind: 'title', unexpected_legacy: true },
    },
  });
  assert.equal(isLockedElement(legacyBinding, catalog), true);
  assert.equal(pasteNodeOptions(makeDoc([legacyBinding]), 'old-binding', source), null);
  assert.equal(isLockedElement(element('video', 'core/video', {
    media_id: '', controls: true, muted: false, binding: { kind: 'title' },
  }), catalog), true);
});

test('duplicateNode enforces list count, 512-node bound, and UTF-8 256KiB bound', () => {
  const lists = Array.from({ length: 8 }, (_, n) => element(`list-${n}`, 'core/list', {
    type_key: 'post', include_descendants: false, sort: 'id_desc', page_size: 6,
  }));
  assert.equal(duplicateNode(makeDoc(lists), 'list-0'), null);
  assert.equal(duplicateNode(makeDoc(Array.from({ length: 509 }, (_, n) => element(`el-${n}`))), 'el-0'), null);
  const unicode = makeDoc([element('large', 'core/text', { text: '界'.repeat(85_000) })]);
  assert.equal(duplicateNode(unicode, 'large'), null);
  const patterns = makeDoc(Array.from({ length: 16 }, (_, n) =>
    element(`pattern-${n}`, 'core/pattern_ref', { pattern_id: n + 1 })));
  assert.ok(duplicateNode(patterns, 'pattern-0'));
});

test('schema evolution: fresh plugin copies must satisfy newly required visible fields', () => {
  const evolvedSchema = { ...schema, fields: [
    { type: 'boolean', key: 'show_more', label: 'Show more', default: false },
    { type: 'text', key: 'required_new', label: 'Required', required: true },
    { type: 'group', key: 'details', label: 'Details', conditions: [[{ field: 'show_more', op: 'eq', value: true }]],
      fields: [{ type: 'text', key: 'conditional', label: 'Conditional', required: true }] },
    { type: 'repeater', key: 'items', label: 'Items', min_items: 1, fields: [
      { type: 'text', key: 'label', label: 'Label', required: true },
    ] },
  ] };
  const evolvedCatalog = [{ ...activeBlock, schema: evolvedSchema }];
  const old = makeDoc([element('old', 'plugin/block', { block_id: 'plugin/hero', values: {
    show_more: false, items: [{ label: 'Good' }],
  } })]);
  assert.equal(duplicateNode(old, 'old', evolvedCatalog), null);
  assert.equal(duplicateNode(old, 'row-1', evolvedCatalog), null);
  assert.equal(copyNodeOptions(old, 'old', evolvedCatalog), null);
  const ready = copy(old);
  ready.sections[0].children[0].children[0].children[0].props.values.required_new = 'Ready';
  assert.ok(duplicateNode(ready, 'old', evolvedCatalog));
  assert.ok(copyNodeOptions(ready, 'old', evolvedCatalog));
  ready.sections[0].children[0].children[0].children[0].props.values.show_more = true;
  assert.equal(duplicateNode(ready, 'old', evolvedCatalog), null);
  ready.sections[0].children[0].children[0].children[0].props.values.details = { conditional: 'Visible' };
  assert.ok(duplicateNode(ready, 'old', evolvedCatalog));
  ready.sections[0].children[0].children[0].children[0].props.values.items = [{ label: '' }];
  assert.equal(duplicateNode(ready, 'old', evolvedCatalog), null);
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
  const selected = copyNodeOptions(doc, 'section-1');
  assert.deepEqual(selected, { kind: 'section', responsive: doc.sections[0].responsive });
  const pasted = pasteNodeOptions(doc, 'section-1', selected);
  assert.ok(pasted);
  pasted.sections[0].responsive.desktop.padding_px = 42;
  assert.equal(doc.sections[0].responsive.desktop.padding_px, 20);

  const clear = copyNodeOptions(makeDoc(), 'section-1');
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
  const clipboard = copyNodeOptions(doc, 'source');
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
  const clipboard = copyNodeOptions(foreign, 'different', catalog);
  assert.ok(clipboard);
  assert.equal(pasteNodeOptions(patterns, 'pattern-16', clipboard, catalog), null);
  const same = copyNodeOptions(patterns, 'pattern-1', catalog);
  const next = pasteNodeOptions(patterns, 'pattern-16', same, catalog);
  assert.ok(next);
  assert.equal(locate(next, 'pattern-16').node.props.pattern_id, 2);
  assert.equal(locate(patterns, 'pattern-16').node.props.pattern_id, 1);
});

test('plugin copy drops unknown values, preserves nested group keys, and never adds top-level binding', () => {
  const values = {
    headline_group: { headline: 'Headline' }, items: [{ label: 'one' }], enabled: true,
    unknown_legacy: { private: true }, in_flight_draft: undefined,
  };
  const doc = makeDoc([
    element('p1', 'plugin/block', { block_id: 'plugin/hero', values }),
    element('p2', 'plugin/block', { block_id: 'plugin/hero', values: { headline_group: { headline: 'before' } } }),
    element('p3', 'plugin/block', { block_id: 'plugin/other', values: {} }),
  ]);
  const clipboard = copyNodeOptions(doc, 'p1', catalog);
  assert.deepEqual(clipboard.props, { values: { headline_group: { headline: 'Headline' }, items: [{ label: 'one' }], enabled: true } });
  assert.equal(clipboard.blockId, 'plugin/hero');
  assert.equal(canPasteOptions(clipboard, locate(doc, 'p2'), false), true);
  assert.equal(canPasteOptions(clipboard, locate(doc, 'p3'), false), false);
  const pasted = pasteNodeOptions(doc, 'p2', clipboard, catalog);
  assert.deepEqual(locate(pasted, 'p2').node.props, { block_id: 'plugin/hero', values: clipboard.props.values });
  assert.deepEqual(locate(doc, 'p1').node.props.values, values);
  assert.equal(copyNodeOptions(doc, 'p1', [{ ...activeBlock, declaration_state: 'withdrawn' }]), null);
  assert.equal(pasteNodeOptions(doc, 'p3', clipboard, catalog), null);
});

test('legacy/block retired elements remain immutable locked anchors (D3)', () => {
  const doc = makeDoc([
    element('legacy-source', 'legacy/block', { block_id: 'theme/banner', values: { title: 'Welcome' } }),
    element('legacy-target', 'legacy/block', { block_id: 'theme/banner', values: { title: 'Old' } }),
  ]);
  assert.equal(isLockedElement(locate(doc, 'legacy-source').node, catalog), true);
  assert.equal(copyNodeOptions(doc, 'legacy-source', catalog), null);
  assert.equal(duplicateNode(doc, 'legacy-source', catalog), null);
  assert.equal(canPasteOptions({ kind: 'element', elementType: 'legacy/block', blockId: 'theme/banner', props: { values: { title: 'Welcome' } } }, locate(doc, 'legacy-target'), true), false);
});

test('reorderSiblingNode supports before/after, rejects cross-parent and no-op', () => {
  const doc = makeDoc([element('a'), element('b'), element('c')]);
  const after = reorderSiblingNode(doc, 'a', 'c', 'after');
  assert.deepEqual(locate(after, 'column-1').node.children.map(n => n.id), ['b', 'c', 'a']);
  const before = reorderSiblingNode(doc, 'c', 'a', 'before');
  assert.deepEqual(locate(before, 'column-1').node.children.map(n => n.id), ['c', 'a', 'b']);
  assert.equal(reorderSiblingNode(doc, 'a', 'a', 'after'), null);
  assert.equal(reorderSiblingNode(doc, 'a', 'b', 'before'), null);
  assert.equal(reorderSiblingNode(doc, 'a', 'row-1', 'after'), null);
  assert.deepEqual(locate(doc, 'column-1').node.children.map(n => n.id), ['a', 'b', 'c']);
});

test('reorderSiblingNode preserves immutable anchor position and ancestor IDs', () => {
  const unknown = element('locked', 'plugin/block', { block_id: 'plugin/hero', values: {} });
  const doc = makeDoc([element('before-1'), element('before-2'), unknown, element('after-1'), element('after-2')]);
  const inactive = [{ ...activeBlock, status: 'provider_inactive' }];
  assert.equal(reorderSiblingNode(doc, 'locked', 'after-1', 'after', inactive), null);
  assert.equal(reorderSiblingNode(doc, 'before-1', 'after-1', 'after', inactive), null);
  assert.equal(reorderSiblingNode(doc, 'after-1', 'before-2', 'before', inactive), null);
  const valid = reorderSiblingNode(doc, 'after-2', 'after-1', 'before', inactive);
  assert.ok(valid);
  assert.equal(preservesAnchors(doc, valid, undefined, inactive), true);
  assert.deepEqual(locate(valid, 'column-1').node.children.map(n => n.id),
    ['before-1', 'before-2', 'locked', 'after-2', 'after-1']);
});

test('appendFreeformElement, visibleNodesInTree, setNodeMeta, and transparent carrier operations work end-to-end', () => {
  let doc = { schema_version: 1, sections: [] };
  const h1 = appendFreeformElement(doc, null, 'core/heading');
  assert.ok(h1);
  doc = h1.document;
  const b1 = appendFreeformElement(doc, null, 'core/button');
  assert.ok(b1);
  doc = b1.document;

  const visible = visibleNodesInTree(doc);
  assert.deepEqual(visible.map(n => n.node.id), [h1.id, b1.id]);
  assert.equal(visible[0].visualParentId, null);
  assert.equal(visible[1].visualParentId, null);

  doc = setNodeMeta(doc, h1.id, {
    name: 'Hero Heading',
    html_id: 'hero-title',
    class_name: 'text-3xl font-bold',
    visibility: { hide_mobile: true },
  });
  assert.ok(doc);
  const h1Loc = locate(doc, h1.id);
  assert.equal(nodeTitle(h1Loc), 'Hero Heading');
  assert.equal(isLockedElement(h1Loc.node, catalog), false);

  const reordered = reorderSiblingNode(doc, b1.id, h1.id, 'before', catalog);
  assert.ok(reordered);
  assert.deepEqual(visibleNodesInTree(reordered).map(n => n.node.id), [b1.id, h1.id]);

  const removed = removeNode(reordered, h1.id);
  assert.ok(removed);
  assert.deepEqual(visibleNodesInTree(removed).map(n => n.node.id), [b1.id]);
});

test('Wave 4 schema types (spacing, radio_buttons, link) initialize, validate, and copy/paste cleanly', () => {
  const wave4Schema = {
    schema_version: 1,
    id: 'plugin/wave4',
    title: 'Wave 4 Card',
    category: 'content',
    render: 'card.html',
    fields: [
      { type: 'spacing', key: 'box_pad', label: 'Padding', default: { top: 16, right: 24, bottom: 16, left: 24 } },
      {
        type: 'radio_buttons', key: 'align', label: 'Align', default: 'center',
        options: [{ value: 'left', label: 'Left' }, { value: 'center', label: 'Center' }],
      },
      {
        type: 'link', key: 'cta', label: 'CTA', required: true,
        default: { url: '/about', new_tab: true, nofollow: false },
      },
    ],
  };
  const wave4Block = {
    block_id: 'plugin/wave4', owner_kind: 'plugin', owner_id: 'wave4',
    status: 'active', declaration_state: 'declared', schema: wave4Schema,
  };
  const wave4Catalog = [wave4Block];

  // initialBlockValues via appendFreeformElement initializes spacing, radio_buttons, and link defaults.
  const appended = appendFreeformElement({ schema_version: 1, sections: [] }, null, 'plugin/block', { blockSchema: wave4Schema });
  assert.ok(appended);
  const createdLoc = locate(appended.document, appended.id);
  assert.deepEqual(createdLoc.node.props.values, {
    box_pad: { top: 16, right: 24, bottom: 16, left: 24 },
    align: 'center',
    cta: { url: '/about', new_tab: true, nofollow: false },
  });

  // copyNodeOptions / duplicateNode succeed when required link.url is non-empty, and reject when empty.
  const validClipboard = copyNodeOptions(appended.document, appended.id, wave4Catalog);
  assert.ok(validClipboard);
  assert.deepEqual(validClipboard.props.values, createdLoc.node.props.values);

  const emptyUrlDoc = makeDoc([
    element('p-empty', 'plugin/block', {
      block_id: 'plugin/wave4',
      values: { box_pad: { top: 8 }, align: 'left', cta: { url: '   ', new_tab: false, nofollow: true } },
    }),
    element('p-target', 'plugin/block', {
      block_id: 'plugin/wave4',
      values: { box_pad: { top: 4 }, align: 'center', cta: { url: '/old', new_tab: false, nofollow: false } },
    }),
  ]);
  assert.equal(copyNodeOptions(emptyUrlDoc, 'p-empty', wave4Catalog), null);
  assert.equal(duplicateNode(emptyUrlDoc, 'p-empty', wave4Catalog), null);

  const pasted = pasteNodeOptions(emptyUrlDoc, 'p-target', validClipboard, wave4Catalog);
  assert.ok(pasted);
  assert.deepEqual(locate(pasted, 'p-target').node.props.values, validClipboard.props.values);
});

test('setSectionStyle, setContainerBoxSpacing, patchNodeFields, rich core/text, and button flags stay unlocked', () => {
  let doc = makeDoc([
    element('txt-1', 'core/text', { text: 'Plain' }),
    element('btn-1', 'core/button', { label: 'Read more', url: '/blog' }),
  ]);

  // Section style + 4-side padding/margin + justify alignment round-trip cleanly.
  doc = setSectionStyle(doc, 'section-1', {
    background_color: '#0f172a',
    background_media_id: '018f0000-0000-7000-8000-000000000001',
    content_width: 'wide',
  });
  doc = setContainerBoxSpacing(doc, 'section-1', 'desktop', 'padding', { top: 48, right: 24, bottom: 48, left: 24 });
  doc = setContainerBoxSpacing(doc, 'section-1', 'mobile', 'margin', { top: 12, bottom: 12 });
  assert.deepEqual(doc.sections[0].style, {
    background_color: '#0f172a',
    background_media_id: '018f0000-0000-7000-8000-000000000001',
    content_width: 'wide',
  });
  assert.deepEqual(doc.sections[0].responsive.desktop.padding, { top: 48, right: 24, bottom: 48, left: 24 });
  assert.deepEqual(doc.sections[0].responsive.mobile.margin, { top: 12, bottom: 12 });

  // Copy/paste section options preserves section.style and deep-clones box spacing.
  const secClip = copyNodeOptions(doc, 'section-1', catalog);
  assert.ok(secClip);
  const targetDoc = makeDoc();
  const secPasted = pasteNodeOptions(targetDoc, 'section-1', secClip);
  assert.ok(secPasted);
  assert.deepEqual(secPasted.sections[0].style, doc.sections[0].style);
  assert.deepEqual(secPasted.sections[0].responsive, doc.sections[0].responsive);

  // Clearing section style removes empty style key cleanly.
  const clearedStyleDoc = setSectionStyle(doc, 'section-1', {
    background_color: undefined,
    background_media_id: undefined,
    content_width: undefined,
  });
  assert.equal(Object.hasOwn(clearedStyleDoc.sections[0], 'style'), false);

  // Core elements remain unlocked when using Wave 4 fields (rich text & button flags).
  doc = patchNodeFields(doc, 'txt-1', { text: '<p>Xin <strong>chào</strong></p>', rich: true });
  doc = patchNodeFields(doc, 'btn-1', { url: 'https://example.com', new_tab: true, nofollow: true });
  const txtNode = locate(doc, 'txt-1').node;
  const btnNode = locate(doc, 'btn-1').node;
  assert.equal(txtNode.props.rich, true);
  assert.equal(txtNode.props.text, '<p>Xin <strong>chào</strong></p>');
  assert.equal(btnNode.props.new_tab, true);
  assert.equal(btnNode.props.nofollow, true);
  assert.equal(isLockedElement(txtNode, catalog), false);
  assert.equal(isLockedElement(btnNode, catalog), false);
});

test('syncContainerResponsiveToDesktop clears tablet/mobile overrides for columns, padding(+padding_px), and margin', () => {
  let doc = makeDoc();
  doc.sections[0].responsive = {
    desktop: { padding_px: 32 },
    tablet: { padding_px: 16, margin: { top: 8 } },
    mobile: { padding: { top: 12, bottom: 12 }, margin: { top: -16 } },
  };
  doc = setResponsiveField(doc, 'row-1', 'desktop', 'columns_per_row', 3);
  doc = setResponsiveField(doc, 'row-1', 'tablet', 'columns_per_row', 2);
  doc = setResponsiveField(doc, 'row-1', 'mobile', 'columns_per_row', 1);
  doc = setResponsiveField(doc, 'row-1', 'mobile', 'gap_px', 16);

  // Syncing columns drops tablet & mobile columns_per_row while preserving gap_px on mobile.
  doc = syncContainerResponsiveToDesktop(doc, 'row-1', 'columns_per_row');
  assert.deepEqual(locate(doc, 'row-1').node.responsive, {
    desktop: { columns_per_row: 3 },
    mobile: { gap_px: 16 },
  });

  // Syncing padding drops tablet.padding_px and mobile.padding while keeping tablet.margin & mobile.margin.
  doc = syncContainerResponsiveToDesktop(doc, 'section-1', 'padding');
  assert.deepEqual(doc.sections[0].responsive, {
    desktop: { padding_px: 32 },
    tablet: { margin: { top: 8 } },
    mobile: { margin: { top: -16 } },
  });

  // Syncing margin prunes now-empty tablet & mobile viewport keys cleanly.
  doc = syncContainerResponsiveToDesktop(doc, 'section-1', 'margin');
  assert.deepEqual(doc.sections[0].responsive, {
    desktop: { padding_px: 32 },
  });
});

test('unified appearance routes redirect #/appearance/options and #/appearance/menus into Studio panels', async () => {
  const routerSource = await readFile(new URL('../src/shell/router.ts', import.meta.url), 'utf8');
  const { code: routerCode } = await transform(routerSource, { loader: 'ts', format: 'esm', target: 'es2022' });
  const { parseAdminRoute } = await import(`data:text/javascript;base64,${Buffer.from(routerCode).toString('base64')}`);

  const optsRoute = parseAdminRoute('#/appearance/options');
  assert.equal(optsRoute.view, 'studio');
  assert.equal(optsRoute.studioPanel, 'theme-options');
  assert.equal(optsRoute.redirectHash, '#/studio?panel=theme-options');

  const menusRoute = parseAdminRoute('#/appearance/menus');
  assert.equal(menusRoute.view, 'studio');
  assert.equal(menusRoute.studioPanel, 'menus');
  assert.equal(menusRoute.redirectHash, '#/studio?panel=menus');
});
