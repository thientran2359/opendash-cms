import { isUiOnlyFieldType } from '../../fields/helpers.ts';
import type { FieldSpec } from '../../fields/types.ts';
import type { BlockCatalogItemDto, BlockSchema, DefaultChildSpec } from '../../types.ts';
import {
  type BlockNode,
  type BuilderContextTarget,
  type BuilderDocument,
  type StudioBuilderClipboard,
  allNodes,
  assignFreshSubtreeIds,
  clone,
  findActiveCatalogItem,
  locate,
  maxSubtreeDepth,
  newNodeId,
  object,
} from './types_and_tree.ts';
import {
  MAX_BUILDER_CONTENT_SLOTS,
  MAX_BUILDER_DEPTH,
  MAX_BUILDER_LISTS,
  MAX_BUILDER_PATTERN_INSTANCES,
  canInsertBlockAt,
  countBlocksByType,
  countDescendantBlocksByType,
  declaredBlockKeys,
  distinctPatternIdCount,
  hasLockedAncestor,
  isLockedNode,
  isProtectedNode,
  pluginValuesReadyForFreshNode,
  preservesAnchors,
  withinBuilderLimits,
} from './validation.ts';

/** Build initial props from FieldSpec defaults, skipping UI-only layout fields. */
export function initialBlockProps(fields: readonly FieldSpec[]): Record<string, unknown> {
  const props: Record<string, unknown> = {};
  for (const field of fields) {
    if (isUiOnlyFieldType(field.type)) continue;
    if (field.type === 'group') {
      props[field.key] = initialBlockProps(field.fields ?? []);
    } else if (field.type === 'repeater') {
      props[field.key] = Array.from(
        { length: field.min_items ?? 0 },
        () => initialBlockProps(field.fields ?? []),
      );
    } else if (field.default !== undefined && field.default !== null) {
      const defCopy = typeof field.default === 'object' ? clone(field.default) : field.default;
      props[field.key] = field.responsive ? { desktop: defCopy } : defCopy;
    }
  }
  return props;
}

function buildDefaultChildNode(
  spec: DefaultChildSpec,
  blockCatalog: readonly BlockCatalogItemDto[],
  usedIds: Set<string>,
): BlockNode {
  const childSchema = findActiveCatalogItem(blockCatalog, spec.block_id)?.schema;
  const baseProps = childSchema ? initialBlockProps(childSchema.fields) : {};
  const mergedProps = { ...baseProps, ...(spec.props ? clone(spec.props) : {}) };
  const id = newNodeId(spec.block_id, usedIds);
  usedIds.add(id);

  const nestedSpecs = spec.children?.length ? spec.children : childSchema?.default_children;
  const children = nestedSpecs?.length
    ? nestedSpecs.map(childSpec => buildDefaultChildNode(childSpec, blockCatalog, usedIds))
    : undefined;

  const node: BlockNode = { id, block_id: spec.block_id };
  if (Object.keys(mergedProps).length > 0) node.props = mergedProps;
  if (children && children.length > 0) node.children = children;
  else if (childSchema?.supports_children) node.children = [];
  return node;
}

export function instantiateBlockNode(
  schema: BlockSchema,
  document: BuilderDocument,
  blockCatalog: readonly BlockCatalogItemDto[] = [],
  options?: {
    presetKey?: string;
    propsOverride?: Record<string, unknown>;
    childrenSpecs?: readonly DefaultChildSpec[];
  },
): BlockNode {
  const usedIds = new Set(allNodes(document).map(entry => entry.node.id));
  const preset = options?.presetKey
    ? schema.presets?.find(entry => entry.key === options.presetKey)
    : undefined;

  const baseProps = initialBlockProps(schema.fields);
  const presetProps = preset?.props ? clone(preset.props) : {};
  const overrideProps = options?.propsOverride ? clone(options.propsOverride) : {};
  const mergedProps = { ...baseProps, ...presetProps, ...overrideProps };

  const id = newNodeId(schema.id, usedIds);
  usedIds.add(id);

  const childSpecs = options?.childrenSpecs?.length
    ? options.childrenSpecs
    : preset?.children?.length
      ? preset.children
      : schema.default_children;
  const children = childSpecs?.length
    ? childSpecs.map(spec => buildDefaultChildNode(spec, blockCatalog, usedIds))
    : undefined;

  const node: BlockNode = { id, block_id: schema.id };
  if (Object.keys(mergedProps).length > 0) node.props = mergedProps;
  if (children && children.length > 0) node.children = children;
  else if (schema.supports_children) node.children = [];
  return node;
}

export function insertBlock(
  document: BuilderDocument,
  blockId: string,
  parentId: string | null,
  index: number | undefined,
  blockCatalog: readonly BlockCatalogItemDto[],
  options?: {
    contextTarget?: BuilderContextTarget | null;
    presetKey?: string;
    propsOverride?: Record<string, unknown>;
  },
): { document: BuilderDocument; id: string } | null {
  const item = findActiveCatalogItem(blockCatalog, blockId);
  if (!item) return null;
  if (
    !canInsertBlockAt(
      document,
      item.schema,
      parentId,
      blockCatalog,
      options?.contextTarget,
      options?.presetKey,
    )
  ) {
    return null;
  }

  const next = clone(document);
  const parentLoc = parentId !== null ? locate(next, parentId) : undefined;
  const parentDepth = parentLoc ? parentLoc.depth : 0;
  const node = instantiateBlockNode(item.schema, next, blockCatalog, options);
  if (parentDepth + maxSubtreeDepth(node) > MAX_BUILDER_DEPTH) return null;

  if (parentLoc) {
    if (!Array.isArray(parentLoc.node.children)) parentLoc.node.children = [];
    const targetList = parentLoc.node.children;
    const insertAt = index === undefined
      ? targetList.length
      : Math.max(0, Math.min(index, targetList.length));
    targetList.splice(insertAt, 0, node);
  } else {
    const insertAt = index === undefined
      ? next.blocks.length
      : Math.max(0, Math.min(index, next.blocks.length));
    next.blocks.splice(insertAt, 0, node);
  }

  const ok = withinBuilderLimits(next)
    && countBlocksByType(next, 'core/list') <= MAX_BUILDER_LISTS
    && countBlocksByType(next, 'core/entry-content') <= MAX_BUILDER_CONTENT_SLOTS
    && countBlocksByType(next, 'core/pattern-ref') <= MAX_BUILDER_PATTERN_INSTANCES
    && distinctPatternIdCount(next, blockCatalog) <= MAX_BUILDER_PATTERN_INSTANCES
    && preservesAnchors(document, next, undefined, blockCatalog);
  return ok ? { document: next, id: node.id } : null;
}

export function duplicateNode(
  document: BuilderDocument,
  id: string,
  blockCatalog: readonly BlockCatalogItemDto[] = [],
): { document: BuilderDocument; id: string } | null {
  const source = locate(document, id);
  if (
    !source
    || hasLockedAncestor(document, id, blockCatalog)
    || isProtectedNode(document, id, blockCatalog)
    || countDescendantBlocksByType(document, id, 'core/entry-content') > 0
  ) {
    return null;
  }
  if (
    countBlocksByType(document, 'core/list') + countDescendantBlocksByType(document, id, 'core/list')
    > MAX_BUILDER_LISTS
  ) {
    return null;
  }
  if (
    countBlocksByType(document, 'core/pattern-ref')
      + countDescendantBlocksByType(document, id, 'core/pattern-ref')
    > MAX_BUILDER_PATTERN_INSTANCES
  ) {
    return null;
  }

  for (const entry of allNodes(document)) {
    const inSubtree = entry.path.length >= source.path.length
      && source.path.every((step, idx) => entry.path[idx] === step);
    if (!inSubtree) continue;
    const active = findActiveCatalogItem(blockCatalog, entry.node.block_id);
    if (!active || !pluginValuesReadyForFreshNode(entry.node.props ?? {}, active.schema)) {
      return null;
    }
  }

  const next = clone(document);
  const parentLoc = source.parentId !== null ? locate(next, source.parentId) : undefined;
  const siblings = parentLoc ? parentLoc.node.children : next.blocks;
  if (!siblings) return null;

  const sourceIndex = source.path[source.path.length - 1];
  const copy = clone(siblings[sourceIndex]);
  if (!copy) return null;

  const usedIds = new Set(allNodes(next).map(entry => entry.node.id));
  assignFreshSubtreeIds([copy], usedIds);
  siblings.splice(sourceIndex + 1, 0, copy);

  const ok = withinBuilderLimits(next)
    && distinctPatternIdCount(next, blockCatalog) <= MAX_BUILDER_PATTERN_INSTANCES
    && preservesAnchors(document, next, undefined, blockCatalog);
  return ok ? { document: next, id: copy.id } : null;
}

export function copyNodeOptions(
  document: BuilderDocument,
  id: string,
  blockCatalog: readonly BlockCatalogItemDto[] = [],
): StudioBuilderClipboard | null {
  const item = locate(document, id);
  if (
    !item
    || isLockedNode(item.node, blockCatalog)
    || hasLockedAncestor(document, id, blockCatalog)
    || item.node.block_id === 'core/entry-content'
  ) {
    return null;
  }
  const active = findActiveCatalogItem(blockCatalog, item.node.block_id);
  if (!active) return null;

  const allowedKeys = declaredBlockKeys(active.schema.fields);
  const rawProps = clone(item.node.props ?? {});
  const filteredProps = Object.fromEntries(
    Object.entries(rawProps).filter(([key]) => allowedKeys.has(key)),
  );
  if (!pluginValuesReadyForFreshNode(filteredProps, active.schema)) return null;

  const clipboard: StudioBuilderClipboard = {
    block_id: item.node.block_id,
    props: filteredProps,
  };
  const className = item.node.meta?.class_name?.trim();
  if (className) clipboard.class_name = className;
  const vis = item.node.meta?.visibility;
  if (vis && (vis.hide_desktop || vis.hide_tablet || vis.hide_mobile)) {
    clipboard.visibility = clone(vis);
  }
  return clipboard;
}

export function canPasteOptions(
  clipboard: StudioBuilderClipboard | null,
  target: BlockNode | { node: BlockNode },
  lockedOrCatalog: boolean | readonly BlockCatalogItemDto[] = false,
): boolean {
  if (!clipboard) return false;
  const node = 'node' in target ? target.node : target;
  const locked = typeof lockedOrCatalog === 'boolean'
    ? lockedOrCatalog
    : isLockedNode(node, lockedOrCatalog);
  if (locked || node.block_id === 'core/entry-content') return false;
  return node.block_id === clipboard.block_id;
}

export function pasteNodeOptions(
  document: BuilderDocument,
  id: string,
  clipboard: StudioBuilderClipboard,
  blockCatalog: readonly BlockCatalogItemDto[] = [],
): BuilderDocument | null {
  const next = clone(document);
  const target = locate(next, id);
  if (!target || hasLockedAncestor(next, id, blockCatalog)) return null;
  if (!canPasteOptions(clipboard, target, blockCatalog)) return null;

  const active = findActiveCatalogItem(blockCatalog, target.node.block_id);
  if (!active || (clipboard.props !== undefined && !object(clipboard.props))) return null;

  const nextProps = clone(clipboard.props ?? {});
  if (Object.keys(nextProps).length === 0) delete target.node.props;
  else target.node.props = nextProps;

  const mergedMeta = { ...(target.node.meta ?? {}) };
  if (clipboard.class_name) mergedMeta.class_name = clipboard.class_name;
  else delete mergedMeta.class_name;
  if (
    clipboard.visibility
    && (clipboard.visibility.hide_desktop || clipboard.visibility.hide_tablet || clipboard.visibility.hide_mobile)
  ) {
    mergedMeta.visibility = clone(clipboard.visibility);
  } else {
    delete mergedMeta.visibility;
  }
  if (Object.keys(mergedMeta).length === 0) delete target.node.meta;
  else target.node.meta = mergedMeta;

  return distinctPatternIdCount(next, blockCatalog) <= MAX_BUILDER_PATTERN_INSTANCES
    && withinBuilderLimits(next)
    && preservesAnchors(document, next, undefined, blockCatalog)
    ? next
    : null;
}
