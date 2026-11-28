/** Node-only Unified Recursive AST Studio regressions. */
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
  EMPTY_DOCUMENT, allNodes, canDeleteNode, canInsertBlockAt, canParentAcceptChild, canPasteOptions,
  cloneWithFreshNodeIds, computeDragMask, copyNodeOptions, createStarterTemplateDocument,
  duplicateNode, getLockedCompoundChild, hasLockedAncestor, insertBlock, isContainerBlock,
  isLockedCompoundChildSchema, isLockedNode, locate, matchesContextTarget, matchesPatternList,
  moveNode, moveNodeSibling, nodeTitle, pasteNodeOptions, patchNodeProps, preservesAnchors,
  removeNode, setNodeMeta, setNodeProps, visibleNodesInTree,
} = await import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);

const schemaSection = {
  schema_version: 1, id: 'core/section', title: 'Section', category: 'layout', render: 'native',
  supports_children: true, allowed_parents: ['root'], allowed_children: ['*', '!core/section'],
  fields: [{ type: 'text', key: 'label', label: 'Label' }],
};
const schemaRow = {
  schema_version: 1, id: 'core/row', title: 'Row', category: 'layout', render: 'native',
  supports_children: true, allowed_children: ['core/column'],
  default_children: [{ block_id: 'core/column' }, { block_id: 'core/column' }],
  presets: [{
    key: 'grid_8_4', title: '8 / 4', props: { layout_mode: 'grid_12' },
    children: [
      { block_id: 'core/column', props: { span: { desktop: 8, mobile: 12 } } },
      { block_id: 'core/column', props: { span: { desktop: 4, mobile: 12 } } },
    ],
  }],
  fields: [{ type: 'text', key: 'layout_mode', label: 'Layout', default: 'equal' }],
};
const schemaColumn = {
  schema_version: 1, id: 'core/column', title: 'Column', category: 'layout', render: 'native',
  supports_children: true, allowed_parents: ['core/row'], allowed_children: ['*', '!core/section'],
  fields: [{ type: 'text', key: 'align', label: 'Align' }],
};
const schemaBox = {
  schema_version: 1, id: 'test/box', title: 'Box', category: 'layout', render: 'render.html',
  supports_children: true, allowed_children: ['*', '!core/section'], fields: [],
};
const schemaAccordion = {
  schema_version: 1, id: 'core/accordion', title: 'Accordion', category: 'interactive', render: 'render.html',
  supports_children: true, allowed_children: ['core/accordion-item'],
  default_children: [
    { block_id: 'core/accordion-item', props: { title: 'Q1' }, children: [{ block_id: 'core/text', props: { content: 'A1' } }] },
    { block_id: 'core/accordion-item', props: { title: 'Q2' }, children: [{ block_id: 'core/text', props: { content: 'A2' } }] },
  ],
  fields: [],
};
const schemaAccordionItem = {
  schema_version: 1, id: 'core/accordion-item', title: 'Accordion Item', category: 'interactive', render: 'render.html',
  supports_children: true, allowed_parents: ['core/accordion'], allowed_children: ['*', '!core/section'],
  fields: [{ type: 'text', key: 'title', label: 'Title', required: true, default: 'Item' }],
};
const schemaText = {
  schema_version: 1, id: 'core/text', title: 'Text', category: 'content', render: 'native',
  fields: [{ type: 'text', key: 'content', label: 'Content', default: '<p>Hi</p>' }],
};
const schemaList = {
  schema_version: 1, id: 'core/list', title: 'List', category: 'content', render: 'native',
  allowed_contexts: ['*', '!pattern', '!theme_pattern'],
  fields: [{ type: 'text', key: 'type_key', label: 'Type', default: 'post', required: true }],
};
const schemaPatternRef = {
  schema_version: 1, id: 'core/pattern-ref', title: 'Pattern', category: 'content', render: 'native',
  allowed_contexts: ['entry_body'],
  fields: [{ type: 'select', key: 'pattern_id', label: 'Pattern', options_source: 'patterns', required: true }],
};
const schemaSlot = {
  schema_version: 1, id: 'core/content-slot', title: 'Slot', category: 'content', render: 'native',
  allowed_contexts: ['single', 'page', 'home'], fields: [],
};
const schemaHero = {
  schema_version: 1, id: 'plugin/hero', title: 'Hero', category: 'content', render: 'index.html',
  fields: [
    { type: 'group', key: 'headline_group', label: 'Headline', fields: [{ type: 'text', key: 'headline', label: 'Headline' }] },
    { type: 'repeater', key: 'items', label: 'Items', fields: [{ type: 'text', key: 'label', label: 'Label' }] },
    { type: 'boolean', key: 'enabled', label: 'Enabled' },
  ],
};

const toCatalogItem = schema => ({
  block_id: schema.id,
  owner_kind: schema.id.startsWith('core/') ? 'core' : 'plugin',
  owner_id: schema.id.split('/')[0],
  status: 'active',
  declaration_state: 'declared',
  schema,
});
const catalog = [
  schemaSection, schemaRow, schemaColumn, schemaBox, schemaAccordion,
  schemaAccordionItem, schemaText, schemaList, schemaPatternRef, schemaSlot, schemaHero,
].map(toCatalogItem);

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

function makeDoc(colChildren = [{ id: 't1', block_id: 'core/text', props: { content: 'First' } }, { id: 't2', block_id: 'core/text', props: { content: 'Second' } }]) {
  return {
    schema_version: 1,
    blocks: [{
      id: 'sec-1', block_id: 'core/section',
      children: [{ id: 'row-1', block_id: 'core/row', children: [{ id: 'col-1', block_id: 'core/column', children: colChildren }] }],
    }],
  };
}

test('iframe script SHA-256 matches CSP and its DOM creation never uses innerHTML', () => {
  const shaLiteral = frameSource.match(/const BRIDGE_SCRIPT_SHA256 = '([^']+)'/);
  assert.ok(shaLiteral, 'CSP hash must be present');
  assert.equal(createHash('sha256').update(bridgeScript, 'utf8').digest('base64'), shaLiteral[1]);
  assert.doesNotMatch(bridgeScript, /\binnerHTML\b|\binsertAdjacentHTML\b/);
  assert.match(frameSource, /sandbox=\{interactive \? 'allow-scripts' : ''\}/);
});

test('canvas bridge mounts gears on section/container/block and auto-opens enclosing <details> on select', () => {
  class Node {
    constructor(tag, id, kind) {
      this.tagName = tag.toUpperCase();
      this.attributes = new Map();
      this.children = [];
      this.parentElement = null;
      this.open = false;
      if (id) {
        this.setAttribute('data-od-editable', '1');
        this.setAttribute('data-node-id', id);
        this.setAttribute('data-od-kind', kind);
      }
    }
    getAttribute(key) { return this.attributes.get(key) ?? null; }
    setAttribute(key, value) { this.attributes.set(key, String(value)); }
    removeAttribute(key) { this.attributes.delete(key); }
    appendChild(node) { node.parentElement = this; this.children.push(node); return node; }
    closest(selector) {
      for (let cur = this; cur; cur = cur.parentElement) {
        if (selector === '[data-od-gear]' && cur.getAttribute('data-od-gear') !== null) return cur;
        if (selector.includes('[data-od-editable=') && cur.getAttribute('data-od-editable') === '1'
          && cur.getAttribute('data-node-id')) return cur;
        if (selector === 'a,button,input,select,textarea,form'
          && ['A', 'BUTTON', 'INPUT', 'SELECT', 'TEXTAREA', 'FORM'].includes(cur.tagName)) return cur;
      }
      return null;
    }
    getBoundingClientRect() { return { left: 10, top: 20, right: 34, bottom: 44 }; }
  }

  const section = new Node('section', 'section-id', 'section');
  const container = new Node('div', 'row-id', 'container');
  const details = new Node('details', 'acc-item-id', 'container');
  const nestedBlock = new Node('div', 'nested-txt-id', 'block');
  details.appendChild(nestedBlock);
  const img = new Node('img', 'img-id', 'block');
  const video = new Node('video', 'vid-id', 'block');
  const nodes = [section, container, details, nestedBlock, img, video];
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
    createElementNS: (_ns, tag) => new Node(tag),
    addEventListener: (type, cb) => { listeners.set(type, cb); },
  };
  const window = { parent: { postMessage: m => messages.push(m) }, addEventListener() {}, scrollTo() {}, scrollX: 0, scrollY: 0 };
  runInNewContext(bridgeScript, { document, window, setTimeout, clearTimeout });

  assert.equal(section.children[0].getAttribute('data-od-gear'), 'section-id');
  assert.equal(container.children[0].getAttribute('data-od-gear'), 'row-id');
  assert.equal(details.children[1].getAttribute('data-od-gear'), 'acc-item-id');
  assert.equal(nestedBlock.children[0].getAttribute('data-od-gear'), 'nested-txt-id');
  assert.equal(img.children.length, 0);
  assert.equal(video.children.length, 0);

  assert.equal(details.open, false);
  listeners.get('click')({ target: nestedBlock, preventDefault() {}, stopPropagation() {} });
  assert.equal(nestedBlock.getAttribute('data-od-selected'), '1');
  assert.equal(details.open, true);
});

test('insertBlock instantiates presets and recursive default_children accurately', () => {
  const sec = insertBlock(EMPTY_DOCUMENT, 'core/section', null, undefined, catalog);
  assert.ok(sec);
  const rowPreset = insertBlock(sec.document, 'core/row', sec.id, undefined, catalog, { presetKey: 'grid_8_4' });
  assert.ok(rowPreset);
  const rowNode = locate(rowPreset.document, rowPreset.id).node;
  assert.equal(rowNode.props.layout_mode, 'grid_12');
  assert.equal(rowNode.children.length, 2);
  assert.deepEqual(rowNode.children.map(c => c.props.span), [
    { desktop: 8, mobile: 12 }, { desktop: 4, mobile: 12 },
  ]);

  const acc = insertBlock(rowPreset.document, 'core/accordion', rowNode.children[0].id, undefined, catalog);
  assert.ok(acc);
  const accNode = locate(acc.document, acc.id).node;
  assert.equal(accNode.children.length, 2);
  assert.equal(accNode.children[0].block_id, 'core/accordion-item');
  assert.equal(accNode.children[0].children[0].block_id, 'core/text');
  assert.equal(accNode.children[0].children[0].props.content, 'A1');
  const ids = allNodes(acc.document).map(e => e.node.id);
  assert.equal(new Set(ids).size, ids.length);
});

test('matchesPatternList and matchesContextTarget mirror Rust matching rules', () => {
  assert.equal(matchesPatternList(null, 'core/text'), true);
  assert.equal(matchesPatternList(['*', '!core/section'], 'core/text'), true);
  assert.equal(matchesPatternList(['*', '!core/section'], 'core/section'), false);
  assert.equal(matchesPatternList(['!core/section'], 'core/row'), true);

  assert.equal(matchesContextTarget({ kind: 'loop_item' }, ['*']), false);
  assert.equal(matchesContextTarget({ kind: 'loop_item' }, null), false);
  assert.equal(matchesContextTarget({ kind: 'single', type_key: 'post' }, ['template']), true);
  assert.equal(matchesContextTarget({ kind: 'single', type_key: 'post' }, ['single:post']), true);
  assert.equal(matchesContextTarget({ kind: 'single', type_key: 'post' }, ['single:product']), false);
  assert.equal(matchesContextTarget({ kind: 'pattern' }, ['*', '!pattern']), false);
  assert.equal(matchesContextTarget({ kind: 'entry_body', type_key: 'page' }, ['entry_body']), true);
});

test('computeDragMask enforces depth 8, ACLs, compound helpers, and never blocks existing list/slot reorder', () => {
  assert.equal(isLockedCompoundChildSchema(schemaColumn), true);
  assert.equal(isLockedCompoundChildSchema(schemaSection), false);
  assert.equal(getLockedCompoundChild(schemaRow, catalog)?.block_id, 'core/column');
  assert.equal(getLockedCompoundChild(schemaSection, catalog), null);

  // Depth 7 chain: b1..b7 (root b1 at depth 1 ... b7 at depth 7) + outside leaf + outside 2-level container.
  let chain = { id: 'b7', block_id: 'test/box', children: [] };
  for (let d = 6; d >= 1; d -= 1) chain = { id: `b${d}`, block_id: 'test/box', children: [chain] };
  const depthDoc = {
    schema_version: 1,
    blocks: [
      chain,
      { id: 'leaf', block_id: 'core/text', props: { content: 'Leaf' } },
      { id: 'pair', block_id: 'test/box', children: [{ id: 'pair-c', block_id: 'core/text', props: { content: 'C' } }] },
    ],
  };
  const leafMask = computeDragMask(depthDoc, 'leaf', catalog);
  assert.equal(leafMask.validParentIds.has('b7'), true);
  const pairMask = computeDragMask(depthDoc, 'pair', catalog);
  assert.equal(pairMask.validParentIds.has('b7'), false);
  assert.equal(pairMask.validParentIds.has('b6'), true);
  assert.equal(pairMask.validParentIds.has('pair'), false);
  assert.equal(pairMask.validParentIds.has('pair-c'), false);

  // Maxed list/slot counts block new insertions but still allow moving existing list/slot nodes.
  const lists = Array.from({ length: 8 }, (_, i) => ({ id: `list-${i}`, block_id: 'core/list', props: { type_key: 'post' } }));
  const fullDoc = {
    schema_version: 1,
    blocks: [{
      id: 'sec-1', block_id: 'core/section',
      children: [{
        id: 'row-1', block_id: 'core/row',
        children: [
          { id: 'col-1', block_id: 'core/column', children: [{ id: 'slot-1', block_id: 'core/content-slot', props: {} }, ...lists] },
          { id: 'col-2', block_id: 'core/column', children: [] },
        ],
      }],
    }],
  };
  assert.equal(canInsertBlockAt(fullDoc, schemaList, 'col-2', catalog, { kind: 'page' }), false);
  assert.equal(canInsertBlockAt(fullDoc, schemaSlot, 'col-2', catalog, { kind: 'page' }), false);
  assert.equal(computeDragMask(fullDoc, 'list-0', catalog, { kind: 'page' }).validParentIds.has('col-2'), true);
  assert.equal(computeDragMask(fullDoc, 'slot-1', catalog, { kind: 'page' }).validParentIds.has('col-2'), true);
  assert.equal(canParentAcceptChild(null, schemaColumn, null), false);
  assert.equal(canParentAcceptChild(schemaSection, schemaSection, 'sec-1'), false);
});

test('moveNode and moveNodeSibling handle reorder, reparent, cycle guard, and depth bounds', () => {
  const doc = {
    schema_version: 1,
    blocks: [{
      id: 'sec-1', block_id: 'core/section',
      children: [{
        id: 'row-1', block_id: 'core/row',
        children: [
          { id: 'col-1', block_id: 'core/column', children: [{ id: 'a', block_id: 'core/text', props: { content: 'A' } }, { id: 'b', block_id: 'core/text', props: { content: 'B' } }] },
          { id: 'col-2', block_id: 'core/column', children: [{ id: 'c', block_id: 'core/text', props: { content: 'C' } }] },
        ],
      }],
    }],
  };
  const swapped = moveNodeSibling(doc, 'a', 'down', catalog);
  assert.deepEqual(locate(swapped, 'col-1').node.children.map(n => n.id), ['b', 'a']);
  const movedAcross = moveNode(doc, 'a', 'col-2', 0, catalog);
  assert.deepEqual(locate(movedAcross, 'col-1').node.children.map(n => n.id), ['b']);
  assert.deepEqual(locate(movedAcross, 'col-2').node.children.map(n => n.id), ['a', 'c']);
  assert.equal(moveNode(doc, 'sec-1', 'col-1', 0, catalog), null);
  assert.equal(moveNode(doc, 'col-1', 'col-2', 0, catalog), null);
});

test('duplicateNode, copyNodeOptions, pasteNodeOptions enforce limits and schema evolution', () => {
  const base = makeDoc();
  const dup = duplicateNode(base, 'row-1', catalog);
  assert.ok(dup);
  assert.equal(locate(dup.document, 'sec-1').node.children.length, 2);
  const allIds = allNodes(dup.document).map(e => e.node.id);
  assert.equal(new Set(allIds).size, allIds.length);

  assert.equal(duplicateNode(makeDoc([{ id: 's', block_id: 'core/content-slot', props: {} }]), 's', catalog), null);
  assert.equal(duplicateNode(makeDoc(Array.from({ length: 8 }, (_, i) => ({ id: `l${i}`, block_id: 'core/list', props: { type_key: 'post' } }))), 'l0', catalog), null);
  assert.equal(duplicateNode(makeDoc(Array.from({ length: 509 }, (_, i) => ({ id: `n${i}`, block_id: 'core/text', props: { content: 'x' } }))), 'n0', catalog), null);
  assert.equal(duplicateNode(makeDoc([{ id: 'big', block_id: 'core/text', props: { content: '界'.repeat(85_000) } }]), 'big', catalog), null);

  const evolvedCatalog = catalog.map(c => c.block_id !== 'plugin/hero' ? c : {
    ...c,
    schema: { ...c.schema, fields: [...c.schema.fields, { type: 'text', key: 'req_new', label: 'Req', required: true }] },
  });
  const badHeroDoc = makeDoc([
    { id: 'h1', block_id: 'plugin/hero', props: { enabled: true } },
    { id: 'h2', block_id: 'plugin/hero', props: { req_new: 'ok', enabled: false, stale_key: 1 } },
    { id: 't1', block_id: 'core/text', props: { content: 'hi' } },
  ]);
  assert.equal(duplicateNode(badHeroDoc, 'h1', evolvedCatalog), null);
  assert.equal(copyNodeOptions(badHeroDoc, 'h1', evolvedCatalog), null);
  const clip = copyNodeOptions(badHeroDoc, 'h2', catalog);
  assert.ok(clip);
  assert.equal(Object.hasOwn(clip.props, 'stale_key'), false);
  assert.equal(canPasteOptions(clip, locate(badHeroDoc, 't1'), catalog), false);
  assert.equal(canPasteOptions(clip, locate(badHeroDoc, 'h1'), catalog), true);
  assert.ok(pasteNodeOptions(badHeroDoc, 'h1', clip, catalog));
});

test('hasLockedAncestor and preservesAnchors isolate inactive containers cleanly', () => {
  const inactiveCat = catalog.map(c => c.block_id === 'test/box' ? { ...c, status: 'provider_inactive' } : c);
  const doc = {
    schema_version: 1,
    blocks: [
      { id: 'sec-1', block_id: 'core/section', children: [] },
      { id: 'locked-box', block_id: 'test/box', children: [{ id: 'inner', block_id: 'core/text', props: { content: 'Keep' } }] },
      { id: 'sec-2', block_id: 'core/section', children: [] },
    ],
  };
  assert.equal(isLockedNode(locate(doc, 'locked-box').node, inactiveCat), true);
  assert.equal(hasLockedAncestor(doc, 'inner', inactiveCat), true);
  assert.equal(setNodeProps(doc, 'inner', { content: 'Mutate' }, inactiveCat), null);
  assert.equal(removeNode(doc, 'inner', { blockCatalog: inactiveCat }), null);
  assert.equal(insertBlock(doc, 'core/text', 'locked-box', undefined, inactiveCat), null);

  const tampered = structuredClone(doc);
  tampered.blocks[1].children[0].props.content = 'Tampered';
  assert.equal(preservesAnchors(doc, tampered, undefined, inactiveCat), false);

  const reordered = moveNodeSibling(doc, 'sec-2', 'up', inactiveCat);
  assert.ok(reordered);
  assert.deepEqual(reordered.blocks.map(b => b.id), ['sec-1', 'sec-2', 'locked-box']);

  assert.equal(canDeleteNode(doc, 'locked-box', { blockCatalog: inactiveCat }), true);
  const deletedTop = removeNode(doc, 'locked-box', { blockCatalog: inactiveCat });
  assert.ok(deletedTop);
  assert.equal(deletedTop.blocks.length, 2);
});

test('setNodeMeta, cloneWithFreshNodeIds, and createStarterTemplateDocument round-trip', () => {
  const starter = createStarterTemplateDocument({ includeContentSlot: true, headingText: 'Home' });
  assert.equal(starter.schema_version, 1);
  assert.equal(isContainerBlock(starter.blocks[0], catalog), true);
  assert.equal(visibleNodesInTree(starter).length, 5);

  const cloned = cloneWithFreshNodeIds(starter, starter);
  const combined = [...allNodes(starter), ...allNodes(cloned)].map(e => e.node.id);
  assert.equal(new Set(combined).size, combined.length);

  const withMeta = setNodeMeta(starter, starter.blocks[0].id, {
    name: 'Hero Section', html_id: 'hero', class_name: 'bg-white', visibility: { hide_mobile: true },
  }, catalog);
  assert.ok(withMeta);
  assert.equal(nodeTitle(locate(withMeta, withMeta.blocks[0].id), catalog), 'Hero Section');
  const clearedMeta = setNodeMeta(withMeta, withMeta.blocks[0].id, { name: '', html_id: '', class_name: '', visibility: {} }, catalog);
  assert.equal(Object.hasOwn(clearedMeta.blocks[0], 'meta'), false);
});

test('unified appearance routes redirect #/appearance/options and #/appearance/menus into canonical Studio sub-routes', async () => {
  const { outputFiles: [routerOut] } = await build({
    entryPoints: [fileURLToPath(new URL('../src/shell/router.ts', import.meta.url))],
    bundle: true,
    write: false,
    platform: 'node',
    format: 'esm',
    target: 'es2022',
  });
  const { parseAdminRoute } = await import(`data:text/javascript;base64,${Buffer.from(routerOut.text).toString('base64')}`);

  const optsRoute = parseAdminRoute('#/appearance/options');
  assert.equal(optsRoute.view, 'studio');
  assert.equal(optsRoute.studioPanel, 'options');
  assert.deepEqual(optsRoute.studioOptionsRoute, { kind: 'index' });
  assert.equal(optsRoute.redirectHash, '#/studio/options');

  const menusRoute = parseAdminRoute('#/appearance/menus');
  assert.equal(menusRoute.view, 'studio');
  assert.equal(menusRoute.studioPanel, 'menus');
  assert.equal(menusRoute.redirectHash, '#/studio/menus');
});

test('partitionFieldSections, stripNonPublicPreviewValues, buildStudioHash, and resolveStudioBackTarget work cleanly', async () => {
  const { outputFiles: [secOut] } = await build({
    entryPoints: [fileURLToPath(new URL('../src/studio/studioOptionsSections.ts', import.meta.url))],
    bundle: true,
    write: false,
    platform: 'node',
    format: 'esm',
    target: 'es2022',
  });
  const {
    partitionFieldSections,
    stripNonPublicPreviewValues,
    buildStudioHash,
    resolveStudioBackTarget,
  } = await import(`data:text/javascript;base64,${Buffer.from(secOut.text).toString('base64')}`);

  const globalFields = [
    { type: 'accordion', key: 'acc_social_links', label: 'Social links' },
    { type: 'url', key: 'social_facebook', label: 'Facebook', public: true },
    { type: 'accordion', key: 'acc_brand_contact', label: 'Brand & Contact' },
    { type: 'text', key: 'contact_phone', label: 'Phone', public: true },
  ];
  const sections = partitionFieldSections(globalFields, 'Global');
  assert.equal(sections.length, 2);
  assert.deepEqual(sections.map(s => s.key), ['acc_social_links', 'acc_brand_contact']);
  assert.equal(sections[0].fields.some(f => f.type === 'accordion'), false);
  assert.equal(sections[1].fields[0].key, 'contact_phone');

  // Nested private/secret fields inside public Group/Repeater are stripped recursively
  const specs = [
    { type: 'text', key: 'headline', label: 'Headline', public: true },
    { type: 'secret', key: 'api_key', label: 'API Key', public: true },
    { type: 'text', key: 'private_note', label: 'Private', public: false },
    {
      type: 'group',
      key: 'cta',
      label: 'CTA',
      public: true,
      fields: [
        { type: 'text', key: 'label', label: 'Label' },
        { type: 'text', key: 'admin_token', label: 'Token', acl: { min_view_role: 'admin', public_exposure: 'private' } },
      ],
    },
  ];
  const safe = stripNonPublicPreviewValues(specs, {
    headline: 'Hello',
    api_key: 'sk-secret',
    private_note: 'hidden',
    cta: { label: 'Click', admin_token: 'leak' },
  });
  assert.deepEqual(safe, { headline: 'Hello', cta: { label: 'Click' } });

  assert.equal(buildStudioHash('options', { kind: 'custom', pageKey: 'optheme_options', groupId: 'site:g1' }), '#/studio/options/custom/optheme_options/site%3Ag1');
  assert.deepEqual(
    resolveStudioBackTarget('options', { kind: 'custom', pageKey: 'optheme_options', groupId: 'site:g1' }, null, { themeSectionCount: 1, customGroupCount: 2 }),
    { kind: 'custom_page', hash: '#/studio/options/custom/optheme_options' },
  );
  assert.deepEqual(
    resolveStudioBackTarget('options', { kind: 'global', section: 'acc_social_links' }, null),
    { kind: 'options', hash: '#/studio/options' },
  );
});
