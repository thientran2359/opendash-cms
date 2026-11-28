import type { BlockCatalogItemDto } from '../../types.ts';
import {
  type BlockNode,
  type BuilderContextTarget,
  type BuilderDocument,
  type NodeMeta,
  type NodeVisibility,
  allNodes,
  clone,
  locate,
  maxSubtreeDepth,
  newNodeId,
  object,
} from './types_and_tree.ts';
import {
  MAX_BUILDER_DEPTH,
  canDeleteNode,
  canParentAcceptChild,
  hasLockedAncestor,
  isLockedNode,
  matchesContextTarget,
  matchesPatternList,
  preservesAnchors,
  withinBuilderLimits,
} from './validation.ts';
import { isInsideLoopItem, validateLoopInvariants } from './loop_validation.ts';
import { findActiveCatalogItem } from './types_and_tree.ts';

function nodeArrayForParent(document: BuilderDocument, parentId: string | null): BlockNode[] | null {
  if (parentId === null) return document.blocks;
  const parentLoc = locate(document, parentId);
  if (!parentLoc) return null;
  if (!Array.isArray(parentLoc.node.children)) parentLoc.node.children = [];
  return parentLoc.node.children;
}

export function createStarterTemplateDocument(options?: {
  includeContentSlot?: boolean;
  includeHeroHeading?: boolean;
  headingText?: string;
}): BuilderDocument {
  const includeContentSlot = options?.includeContentSlot ?? false;
  const includeHeroHeading = options?.includeHeroHeading ?? true;
  const doc: BuilderDocument = { schema_version: 1, blocks: [] };
  const colChildren: BlockNode[] = [];
  if (includeHeroHeading) {
    colChildren.push({
      id: newNodeId('core/heading', doc),
      block_id: 'core/heading',
      props: { level: 'h1', text: options?.headingText ?? 'Page title' },
    });
  }
  if (includeContentSlot) {
    colChildren.push({
      id: newNodeId('core/entry-content', doc),
      block_id: 'core/entry-content',
      props: {},
    });
  }
  const colNode: BlockNode = {
    id: newNodeId('core/column', doc),
    block_id: 'core/column',
    children: colChildren,
  };
  const rowNode: BlockNode = {
    id: newNodeId('core/row', doc),
    block_id: 'core/row',
    props: { layout_mode: 'equal', columns: { desktop: 1 } },
    children: [colNode],
  };
  const secNode: BlockNode = {
    id: newNodeId('core/section', doc),
    block_id: 'core/section',
    props: { content_width: 'boxed', text_scheme: 'auto' },
    children: [rowNode],
  };
  doc.blocks.push(secNode);
  return doc;
}

export function setNodeProps(
  document: BuilderDocument,
  id: string,
  nextProps: Record<string, unknown>,
  blockCatalog: readonly BlockCatalogItemDto[] = [],
): BuilderDocument | null {
  if (!object(nextProps)) return null;
  const next = clone(document);
  const target = locate(next, id);
  if (!target || isLockedNode(target.node, blockCatalog) || hasLockedAncestor(next, id, blockCatalog)) {
    return null;
  }
  const cleaned: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(clone(nextProps))) {
    if (value !== undefined) cleaned[key] = value;
  }
  if (Object.keys(cleaned).length === 0) {
    delete target.node.props;
  } else {
    target.node.props = cleaned;
  }
  return withinBuilderLimits(next) && preservesAnchors(document, next, undefined, blockCatalog)
    ? next
    : null;
}

export function patchNodeProps(
  document: BuilderDocument,
  id: string,
  patch: Record<string, unknown>,
  blockCatalog: readonly BlockCatalogItemDto[] = [],
): BuilderDocument | null {
  if (!object(patch)) return null;
  const current = locate(document, id);
  if (!current) return null;
  const merged = { ...(current.node.props ?? {}), ...clone(patch) };
  return setNodeProps(document, id, merged, blockCatalog);
}

export function setNodeMeta(
  document: BuilderDocument,
  id: string,
  patch: Partial<NodeMeta>,
  blockCatalog: readonly BlockCatalogItemDto[] = [],
): BuilderDocument | null {
  const next = clone(document);
  const target = locate(next, id);
  if (!target || isLockedNode(target.node, blockCatalog) || hasLockedAncestor(next, id, blockCatalog)) {
    return null;
  }
  const merged: NodeMeta = { ...(target.node.meta ?? {}) };
  if ('name' in patch) {
    const val = (patch.name ?? '').trim();
    if (!val) {
      delete merged.name;
    } else {
      if (val.length > 80 || [...val].some(ch => ch.charCodeAt(0) < 32)) return null;
      merged.name = val;
    }
  }
  if ('html_id' in patch) {
    const val = (patch.html_id ?? '').trim();
    if (!val) {
      delete merged.html_id;
    } else if (isInsideLoopItem(next, id) || !/^[a-zA-Z0-9_-]{1,64}$/.test(val)) {
      return null;
    } else {
      merged.html_id = val;
    }
  }
  if ('class_name' in patch) {
    const val = (patch.class_name ?? '').trim();
    if (!val) {
      delete merged.class_name;
    } else if (!/^[a-zA-Z0-9_ -]{1,256}$/.test(val)) {
      return null;
    } else {
      merged.class_name = val;
    }
  }
  if ('visibility' in patch) {
    const vis = patch.visibility;
    const normalized: NodeVisibility = {};
    if (vis?.hide_desktop) normalized.hide_desktop = true;
    if (vis?.hide_tablet) normalized.hide_tablet = true;
    if (vis?.hide_mobile) normalized.hide_mobile = true;
    if (Object.keys(normalized).length === 0) delete merged.visibility;
    else merged.visibility = normalized;
  }
  if (Object.keys(merged).length === 0) {
    delete target.node.meta;
  } else {
    target.node.meta = merged;
  }
  return withinBuilderLimits(next) && preservesAnchors(document, next, undefined, blockCatalog)
    ? next
    : null;
}

export function removeNode(
  document: BuilderDocument,
  id: string,
  options?: {
    requiresContentSlot?: boolean;
    blockCatalog?: readonly BlockCatalogItemDto[];
  },
): BuilderDocument | null {
  if (!canDeleteNode(document, id, options)) return null;
  const next = clone(document);
  const target = locate(next, id);
  if (!target) return null;
  const siblings = nodeArrayForParent(next, target.parentId);
  if (!siblings) return null;
  const index = target.path[target.path.length - 1];
  siblings.splice(index, 1);
  return withinBuilderLimits(next) && preservesAnchors(document, next, id, options?.blockCatalog ?? [])
    ? next
    : null;
}

/** Reorder or reparent a block subtree across compatible containers up to max depth 8. */
export function moveNode(
  document: BuilderDocument,
  sourceId: string,
  targetParentId: string | null,
  targetIndex: number,
  blockCatalog: readonly BlockCatalogItemDto[] = [],
  contextTarget?: BuilderContextTarget | null,
): BuilderDocument | null {
  if (sourceId === targetParentId || !Number.isInteger(targetIndex) || targetIndex < 0) return null;
  const source = locate(document, sourceId);
  if (!source || hasLockedAncestor(document, sourceId, blockCatalog)) return null;

  const targetParent = targetParentId ? locate(document, targetParentId) : undefined;
  if (targetParentId !== null) {
    if (!targetParent) return null;
    const insideSource = targetParent.path.length >= source.path.length
      && source.path.every((step, idx) => targetParent.path[idx] === step);
    if (insideSource) return null;
    if (isLockedNode(targetParent.node, blockCatalog) || hasLockedAncestor(document, targetParentId, blockCatalog)) {
      return null;
    }
  }

  const targetParentDepth = targetParent ? targetParent.depth : 0;
  if (targetParentDepth + maxSubtreeDepth(source.node) > MAX_BUILDER_DEPTH) return null;

  const sourceLocked = isLockedNode(source.node, blockCatalog);
  const sourceSchema = findActiveCatalogItem(blockCatalog, source.node.block_id)?.schema;
  const targetParentSchema = targetParent
    ? findActiveCatalogItem(blockCatalog, targetParent.node.block_id)?.schema
    : null;

  const effectiveCtx = targetParentId !== null && isInsideLoopItem(document, targetParentId)
    ? ({ kind: 'loop_item' } as const)
    : contextTarget;
  if (!sourceLocked && sourceSchema) {
    if (
      !matchesContextTarget(effectiveCtx, sourceSchema.allowed_contexts)
      || !canParentAcceptChild(targetParentSchema, sourceSchema, targetParentId)
    ) {
      return null;
    }
  } else if (sourceLocked && targetParentId !== null && source.parentId !== targetParentId) {
    if (
      !targetParentSchema
      || !matchesPatternList(targetParentSchema.allowed_children, '*')
      || !matchesPatternList(targetParentSchema.allowed_children, source.node.block_id)
    ) {
      return null;
    }
  }

  const next = clone(document);
  const sourceSiblings = nodeArrayForParent(next, source.parentId);
  const targetSiblings = nodeArrayForParent(next, targetParentId);
  if (!sourceSiblings || !targetSiblings) return null;

  const sourceIndex = source.path[source.path.length - 1];
  const [moving] = sourceSiblings.splice(sourceIndex, 1);
  if (!moving) return null;

  const rawAdjusted = source.parentId === targetParentId && targetIndex > sourceIndex
    ? targetIndex - 1
    : targetIndex;
  const insertIndex = Math.max(0, Math.min(rawAdjusted, targetSiblings.length));
  if (source.parentId === targetParentId && insertIndex === sourceIndex) return null;

  targetSiblings.splice(insertIndex, 0, moving);
  return withinBuilderLimits(next)
    && preservesAnchors(document, next, undefined, blockCatalog)
    && validateLoopInvariants(next, blockCatalog)
    ? next
    : null;
}

export function moveNodeSibling(
  document: BuilderDocument,
  id: string,
  direction: 'up' | 'down',
  blockCatalog: readonly BlockCatalogItemDto[] = [],
  contextTarget?: BuilderContextTarget | null,
): BuilderDocument | null {
  const source = locate(document, id);
  if (!source) return null;
  const siblings = source.parentId === null
    ? document.blocks
    : (locate(document, source.parentId)?.node.children ?? []);
  const index = source.path[source.path.length - 1];
  if (direction === 'up') {
    if (index <= 0) return null;
    return moveNode(document, id, source.parentId, index - 1, blockCatalog, contextTarget);
  }
  if (index >= siblings.length - 1) return null;
  return moveNode(document, id, source.parentId, index + 2, blockCatalog, contextTarget);
}

export { allNodes };
