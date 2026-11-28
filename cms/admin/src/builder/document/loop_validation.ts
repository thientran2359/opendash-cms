import type { BlockCatalogItemDto, BlockSchema, DefaultChildSpec } from '../../types.ts';
import {
  type BlockNode,
  type BuilderDocument,
  type NodeLocation,
  allNodes,
  findActiveCatalogItem,
  locate,
  maxSubtreeDepth,
} from './types_and_tree.ts';

export const MAX_LOOP_ITEM_NODES = 16;
export const MAX_LOOP_ITEM_REL_DEPTH = 4;
export const MAX_THIRD_PARTY_BLOCKS_IN_LOOP = 2;

export const LOOP_PARENT_CARDINALITY: Readonly<Record<string, { min: number; max: number }>> = {
  'core/archive-loop': { min: 1, max: 1 },
  'core/entry-related': { min: 1, max: 1 },
  'core/list': { min: 0, max: 1 },
};

export const LOOP_ITEM_CORE_ALLOWLIST: ReadonlySet<string> = new Set([
  'core/entry-title',
  'core/entry-featured-image',
  'core/entry-meta',
  'core/entry-excerpt',
  'core/entry-terms',
  'core/entry-field',
  'core/row',
  'core/column',
  'core/stack',
  'core/card',
  'core/banner',
  'core/heading',
  'core/text',
  'core/image',
  'core/button',
  'core/divider',
  'core/spacer',
]);

const DISALLOWED_LOOP_FIELD_TYPES = new Set([
  'repeater',
  'richtext',
  'gallery',
  'entry_ref',
  'term_ref',
]);

function hasDisallowedLoopField(fields: readonly BlockSchema['fields'][number][] | undefined): boolean {
  if (!fields || fields.length === 0) return false;
  return fields.some(field =>
    DISALLOWED_LOOP_FIELD_TYPES.has(field.type)
    || Boolean(field.options_source)
    || hasDisallowedLoopField(field.fields),
  );
}

export function isLoopSchemaCompatible(schema: BlockSchema): boolean {
  if (schema.id.startsWith('core/')) {
    return LOOP_ITEM_CORE_ALLOWLIST.has(schema.id);
  }
  const contexts = schema.allowed_contexts ?? [];
  const explicitLoop = contexts.includes('loop_item') && !contexts.includes('!loop_item');
  return explicitLoop && !schema.supports_children && !hasDisallowedLoopField(schema.fields);
}

function ancestorChain(document: BuilderDocument, id: string | null | undefined): NodeLocation[] {
  if (!id) return [];
  const entries = allNodes(document);
  const target = entries.find(entry => entry.node.id === id);
  if (!target) return [];
  const chain: NodeLocation[] = [];
  for (let len = 1; len <= target.path.length; len += 1) {
    const prefix = target.path.slice(0, len);
    const match = entries.find(
      entry => entry.path.length === len && prefix.every((step, idx) => entry.path[idx] === step),
    );
    if (match) chain.push(match);
  }
  return chain;
}

/** Nearest strict ancestor `core/loop-item` around `id`. */
export function findEnclosingLoopItem(
  document: BuilderDocument | null | undefined,
  id: string | null | undefined,
): NodeLocation | null {
  if (!document || !id) return null;
  const chain = ancestorChain(document, id);
  for (let i = chain.length - 2; i >= 0; i -= 1) {
    if (chain[i].node.block_id === 'core/loop-item') return chain[i];
  }
  return null;
}

/** True when `id` itself is `core/loop-item` or sits inside one (matching Rust html_id scope). */
export function isInsideLoopItem(
  document: BuilderDocument | null | undefined,
  id: string | null | undefined,
): boolean {
  if (!document || !id) return false;
  return ancestorChain(document, id).some(entry => entry.node.block_id === 'core/loop-item');
}

/** Nearest strict ancestor `core/list` around `id`. */
export function findEnclosingList(
  document: BuilderDocument | null | undefined,
  id: string | null | undefined,
): NodeLocation | null {
  if (!document || !id) return null;
  const chain = ancestorChain(document, id);
  for (let i = chain.length - 2; i >= 0; i -= 1) {
    if (chain[i].node.block_id === 'core/list') return chain[i];
  }
  return null;
}

function findLoopScopeForContainer(
  document: BuilderDocument,
  parentId: string | null,
): { loopLoc: NodeLocation; parentRelDepth: number } | null {
  if (parentId === null) return null;
  const chain = ancestorChain(document, parentId);
  if (chain.length === 0) return null;
  const parentLoc = chain[chain.length - 1];
  for (let i = chain.length - 1; i >= 0; i -= 1) {
    if (chain[i].node.block_id === 'core/loop-item') {
      return {
        loopLoc: chain[i],
        parentRelDepth: parentLoc.depth - chain[i].depth,
      };
    }
  }
  return null;
}

function countLoopSubtreeStats(
  nodes: readonly BlockNode[],
): { nodes: number; thirdParty: number } {
  let count = 0;
  let thirdParty = 0;
  for (const node of nodes) {
    count += 1;
    if (!node.block_id.startsWith('core/')) thirdParty += 1;
    if (node.children?.length) {
      const childStats = countLoopSubtreeStats(node.children);
      count += childStats.nodes;
      thirdParty += childStats.thirdParty;
    }
  }
  return { nodes: count, thirdParty };
}

function checkPlacedSubtreeInLoop(
  node: BlockNode,
  blockCatalog: readonly BlockCatalogItemDto[],
): { ok: boolean; nodes: number; thirdParty: number } {
  if (node.meta?.html_id?.trim()) return { ok: false, nodes: 0, thirdParty: 0 };
  if (node.block_id.startsWith('core/')) {
    if (!LOOP_ITEM_CORE_ALLOWLIST.has(node.block_id)) return { ok: false, nodes: 0, thirdParty: 0 };
  } else {
    const active = findActiveCatalogItem(blockCatalog, node.block_id);
    if (active && !isLoopSchemaCompatible(active.schema)) return { ok: false, nodes: 0, thirdParty: 0 };
  }
  let nodes = 1;
  let thirdParty = node.block_id.startsWith('core/') ? 0 : 1;
  for (const child of node.children ?? []) {
    const sub = checkPlacedSubtreeInLoop(child, blockCatalog);
    if (!sub.ok) return { ok: false, nodes: 0, thirdParty: 0 };
    nodes += sub.nodes;
    thirdParty += sub.thirdParty;
  }
  return { ok: true, nodes, thirdParty };
}

function checkSpecsInLoop(
  specs: readonly DefaultChildSpec[] | undefined,
  blockCatalog: readonly BlockCatalogItemDto[],
): { ok: boolean; nodes: number; thirdParty: number; height: number } {
  if (!specs || specs.length === 0) return { ok: true, nodes: 0, thirdParty: 0, height: 0 };
  let nodes = 0;
  let thirdParty = 0;
  let maxH = 0;
  for (const spec of specs) {
    if (spec.block_id.startsWith('core/')) {
      if (!LOOP_ITEM_CORE_ALLOWLIST.has(spec.block_id)) {
        return { ok: false, nodes: 0, thirdParty: 0, height: 0 };
      }
    } else {
      const schema = findActiveCatalogItem(blockCatalog, spec.block_id)?.schema;
      if (!schema || !isLoopSchemaCompatible(schema)) {
        return { ok: false, nodes: 0, thirdParty: 0, height: 0 };
      }
      thirdParty += 1;
    }
    const childSchema = findActiveCatalogItem(blockCatalog, spec.block_id)?.schema;
    const nested = spec.children?.length ? spec.children : childSchema?.default_children;
    const sub = checkSpecsInLoop(nested, blockCatalog);
    if (!sub.ok) return sub;
    nodes += 1 + sub.nodes;
    thirdParty += sub.thirdParty;
    if (1 + sub.height > maxH) maxH = 1 + sub.height;
  }
  return { ok: true, nodes, thirdParty, height: maxH };
}

export function canDeleteInLoop(document: BuilderDocument, id: string): boolean {
  const target = locate(document, id);
  if (!target) return false;
  if (target.node.block_id === 'core/loop-item' && target.parentId !== null) {
    const parentLoc = locate(document, target.parentId);
    const card = parentLoc ? LOOP_PARENT_CARDINALITY[parentLoc.node.block_id] : undefined;
    if (card) {
      const loopCount = (parentLoc?.node.children ?? []).filter(c => c.block_id === 'core/loop-item').length;
      if (loopCount - 1 < card.min) return false;
    }
  }
  return true;
}

export function canInsertIntoLoop(
  document: BuilderDocument,
  schema: BlockSchema,
  parentId: string | null,
  blockCatalog: readonly BlockCatalogItemDto[],
  childSpecs?: readonly DefaultChildSpec[],
): boolean {
  if (parentId !== null) {
    const parentLoc = locate(document, parentId);
    if (!parentLoc) return false;
    const card = LOOP_PARENT_CARDINALITY[parentLoc.node.block_id];
    if (card && schema.id === 'core/loop-item') {
      const currentCount = (parentLoc.node.children ?? []).filter(c => c.block_id === 'core/loop-item').length;
      if (currentCount + 1 > card.max) return false;
    }
    const scope = findLoopScopeForContainer(document, parentId);
    if (scope) {
      if (!isLoopSchemaCompatible(schema)) return false;
      const specCheck = checkSpecsInLoop(childSpecs, blockCatalog);
      if (!specCheck.ok) return false;
      const existing = countLoopSubtreeStats(scope.loopLoc.node.children ?? []);
      const addedNodes = 1 + specCheck.nodes;
      const addedThirdParty = (schema.id.startsWith('core/') ? 0 : 1) + specCheck.thirdParty;
      if (existing.nodes + addedNodes > MAX_LOOP_ITEM_NODES) return false;
      if (existing.thirdParty + addedThirdParty > MAX_THIRD_PARTY_BLOCKS_IN_LOOP) return false;
      if (scope.parentRelDepth + 1 + specCheck.height > MAX_LOOP_ITEM_REL_DEPTH) return false;
    }
  }
  return true;
}

export function canMoveWithLoopRules(
  document: BuilderDocument,
  sourceId: string,
  targetParentId: string | null,
  blockCatalog: readonly BlockCatalogItemDto[],
): boolean {
  const source = locate(document, sourceId);
  if (!source) return false;
  if (source.node.block_id === 'core/loop-item' && source.parentId !== targetParentId) {
    if (!canDeleteInLoop(document, sourceId)) return false;
    if (targetParentId !== null) {
      const targetParent = locate(document, targetParentId);
      const card = targetParent ? LOOP_PARENT_CARDINALITY[targetParent.node.block_id] : undefined;
      if (card) {
        const count = (targetParent?.node.children ?? []).filter(c => c.block_id === 'core/loop-item').length;
        if (count + 1 > card.max) return false;
      }
    }
  }
  const targetScope = findLoopScopeForContainer(document, targetParentId);
  if (!targetScope) return true;
  if (targetScope.parentRelDepth + maxSubtreeDepth(source.node) > MAX_LOOP_ITEM_REL_DEPTH) return false;
  const subCheck = checkPlacedSubtreeInLoop(source.node, blockCatalog);
  if (!subCheck.ok) return false;
  const sourceLoop = findEnclosingLoopItem(document, sourceId);
  if (sourceLoop && sourceLoop.node.id === targetScope.loopLoc.node.id) {
    return true;
  }
  const existing = countLoopSubtreeStats(targetScope.loopLoc.node.children ?? []);
  return (
    existing.nodes + subCheck.nodes <= MAX_LOOP_ITEM_NODES
    && existing.thirdParty + subCheck.thirdParty <= MAX_THIRD_PARTY_BLOCKS_IN_LOOP
  );
}

/** Validates loop cardinality, allowlist, rel depth (<=4), node cap (<=16), 3P cap (<=2), and no html_id. */
export function validateLoopInvariants(
  document: BuilderDocument,
  blockCatalog: readonly BlockCatalogItemDto[] = [],
): boolean {
  const visit = (
    nodes: readonly BlockNode[],
    counters: { nodes: number; thirdParty: number } | null,
    relDepth: number,
  ): boolean => {
    for (const node of nodes) {
      if ((node.block_id === 'core/loop-item' || counters !== null) && node.meta?.html_id?.trim()) {
        return false;
      }
      if (counters !== null) {
        if (relDepth > MAX_LOOP_ITEM_REL_DEPTH) return false;
        counters.nodes += 1;
        if (counters.nodes > MAX_LOOP_ITEM_NODES) return false;
        if (node.block_id.startsWith('core/')) {
          if (!LOOP_ITEM_CORE_ALLOWLIST.has(node.block_id)) return false;
        } else {
          const active = findActiveCatalogItem(blockCatalog, node.block_id);
          if (active && !isLoopSchemaCompatible(active.schema)) return false;
          counters.thirdParty += 1;
          if (counters.thirdParty > MAX_THIRD_PARTY_BLOCKS_IN_LOOP) return false;
        }
      }
      const card = LOOP_PARENT_CARDINALITY[node.block_id];
      if (card) {
        const loopCnt = (node.children ?? []).filter(c => c.block_id === 'core/loop-item').length;
        if (loopCnt < card.min || loopCnt > card.max) return false;
      }
      if (node.children?.length) {
        if (node.block_id === 'core/loop-item') {
          if (!visit(node.children, { nodes: 0, thirdParty: 0 }, 1)) return false;
        } else if (!visit(node.children, counters, counters ? relDepth + 1 : 0)) {
          return false;
        }
      }
    }
    return true;
  };
  return visit(document.blocks, null, 0);
}
