import { evaluateConditions } from '../../fields/conditions.ts';
import { isUiOnlyFieldType } from '../../fields/helpers.ts';
import type { FieldSpec } from '../../fields/types.ts';
import type { BlockCatalogItemDto, BlockSchema, DefaultChildSpec } from '../../types.ts';
import {
  type BlockNode,
  type BuilderContextTarget,
  type BuilderDocument,
  type DragCompatibilityMask,
  allNodes,
  findActiveCatalogItem,
  findCatalogItem,
  locate,
  maxSubtreeDepth,
  object,
} from './types_and_tree.ts';
import {
  canDeleteInLoop,
  canInsertIntoLoop,
  canMoveWithLoopRules,
  isInsideLoopItem,
} from './loop_validation.ts';

export const MAX_BUILDER_DEPTH = 8;
export const MAX_BUILDER_NODES = 512;
export const MAX_BUILDER_BYTES = 256 * 1024;
export const MAX_BUILDER_LISTS = 8;
export const MAX_BUILDER_PATTERN_INSTANCES = 16;
export const MAX_BUILDER_CONTENT_SLOTS = 1;

const TEMPLATE_CONTEXT_KINDS = new Set<BuilderContextTarget['kind']>([
  'single',
  'single_custom',
  'singular',
  'page',
  'page_layout',
  'index',
  'archive',
  'taxonomy',
  'not_found',
  'search',
]);

export function parsePositiveInt(value: unknown): number | null {
  if (typeof value === 'number') {
    return Number.isSafeInteger(value) && value > 0 ? value : null;
  }
  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (!/^[1-9]\d{0,18}$/.test(trimmed)) return null;
    const parsed = Number(trimmed);
    return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : null;
  }
  return null;
}

/** Evaluate allow/deny pattern lists identically to Rust `matches_pattern_list`. */
export function matchesPatternList(
  patterns: readonly string[] | null | undefined,
  token: string,
): boolean {
  if (!patterns || patterns.length === 0) return true;
  if (patterns.some(entry => entry === `!${token}`)) return false;
  const positive = patterns.filter(entry => !entry.startsWith('!'));
  if (positive.length === 0) return true;
  return positive.includes('*') || positive.includes(token);
}

function matchesContextToken(target: BuilderContextTarget, token: string): boolean {
  if (token === '*') return true;
  if (token === 'loop_item') return target.kind === 'loop_item';
  if (target.kind === 'loop_item') return false;
  if (token === 'template') return TEMPLATE_CONTEXT_KINDS.has(target.kind);
  if (token === 'single') {
    return target.kind === 'single' || target.kind === 'single_custom' || target.kind === 'singular';
  }
  if (token.startsWith('entry_body:')) {
    return target.kind === 'entry_body' && target.type_key === token.slice('entry_body:'.length);
  }
  if (token.startsWith('part:')) {
    return target.kind === 'part' && target.part_key === token.slice('part:'.length);
  }
  if (token.startsWith('single:')) {
    const key = token.slice('single:'.length);
    return (target.kind === 'single' || target.kind === 'single_custom') && target.type_key === key;
  }
  if (token.startsWith('archive:')) {
    return target.kind === 'archive' && target.type_key === token.slice('archive:'.length);
  }
  if (token.startsWith('taxonomy:')) {
    return target.kind === 'taxonomy' && target.taxonomy_key === token.slice('taxonomy:'.length);
  }
  return target.kind === token;
}

export function matchesContextTarget(
  contextTarget: BuilderContextTarget | null | undefined,
  allowedContexts: readonly string[] | null | undefined,
): boolean {
  if (!contextTarget) return true;
  if (!allowedContexts || allowedContexts.length === 0) return true;
  if (allowedContexts.some(token => token.startsWith('!') && matchesContextToken(contextTarget, token.slice(1)))) {
    return false;
  }
  const positive = allowedContexts.filter(token => !token.startsWith('!'));
  if (positive.length === 0) return true;
  return positive.some(token => matchesContextToken(contextTarget, token));
}

export function isLockedNode(
  node: BlockNode,
  blockCatalog: readonly BlockCatalogItemDto[] = [],
): boolean {
  const validShape = typeof node.id === 'string'
    && typeof node.block_id === 'string'
    && (node.props === undefined || object(node.props))
    && (node.children === undefined || Array.isArray(node.children))
    && Object.keys(node).every(k => k === 'id' || k === 'block_id' || k === 'props' || k === 'meta' || k === 'children');
  if (!validShape) return true;
  return !findActiveCatalogItem(blockCatalog, node.block_id);
}

export function hasLockedAncestor(
  document: BuilderDocument,
  id: string,
  blockCatalog: readonly BlockCatalogItemDto[] = [],
): boolean {
  const entries = allNodes(document);
  const location = entries.find(entry => entry.node.id === id);
  if (!location || location.path.length <= 1) return false;
  for (let depth = 1; depth < location.path.length; depth += 1) {
    const ancestorPath = location.path.slice(0, depth);
    const ancestor = entries.find(
      entry => entry.path.length === ancestorPath.length
        && ancestorPath.every((step, idx) => entry.path[idx] === step),
    );
    if (ancestor && isLockedNode(ancestor.node, blockCatalog)) return true;
  }
  return false;
}

export function isProtectedNode(
  document: BuilderDocument,
  id: string,
  blockCatalog: readonly BlockCatalogItemDto[] = [],
): boolean {
  const entries = allNodes(document);
  const location = entries.find(entry => entry.node.id === id);
  if (!location) return false;
  if (hasLockedAncestor(document, id, blockCatalog)) return true;
  return entries.some(
    entry => entry.path.length >= location.path.length
      && location.path.every((step, index) => entry.path[index] === step)
      && isLockedNode(entry.node, blockCatalog),
  );
}

export function canParentAcceptChild(
  parentSchema: BlockSchema | null | undefined,
  childSchema: BlockSchema | null | undefined,
  parentId: string | null,
  childBlockId?: string,
): boolean {
  const resolvedChildId = childSchema?.id ?? childBlockId ?? '';
  if (!resolvedChildId) return false;
  if (parentId !== null) {
    if (!parentSchema || !parentSchema.supports_children) return false;
    if (!matchesPatternList(parentSchema.allowed_children, resolvedChildId)) return false;
  }
  if (!childSchema) return false;
  const parentToken = parentId === null ? 'root' : parentSchema?.id;
  if (!parentToken) return false;
  return matchesPatternList(childSchema.allowed_parents, parentToken);
}

export function countBlocksByType(
  document: BuilderDocument | null | undefined,
  blockId: string,
): number {
  return allNodes(document).filter(entry => entry.node.block_id === blockId).length;
}

export function countDescendantBlocksByType(
  document: BuilderDocument | null | undefined,
  rootId: string,
  blockId: string,
): number {
  const target = locate(document, rootId);
  if (!target) return 0;
  return allNodes(document).filter(
    entry => entry.path.length >= target.path.length
      && target.path.every((step, idx) => entry.path[idx] === step)
      && entry.node.block_id === blockId,
  ).length;
}

export function canDeleteNode(
  document: BuilderDocument,
  id: string,
  options?: {
    requiresContentSlot?: boolean;
    blockCatalog?: readonly BlockCatalogItemDto[];
  },
): boolean {
  const target = locate(document, id);
  if (!target) return false;
  if (options?.blockCatalog && hasLockedAncestor(document, id, options.blockCatalog)) {
    return false;
  }
  if (options?.requiresContentSlot) {
    const isEntryDisplay = (blockId: string) =>
      blockId === 'core/entry-content' || blockId.startsWith('core/entry-');
    const allEntryCount = allNodes(document).filter(item => isEntryDisplay(item.node.block_id)).length;
    const subtreeEntryCount = allNodes(document).filter(
      item =>
        item.path.length >= target.path.length
        && target.path.every((step, idx) => item.path[idx] === step)
        && isEntryDisplay(item.node.block_id),
    ).length;
    if (allEntryCount > 0 && subtreeEntryCount >= allEntryCount) return false;
  }
  return canDeleteInLoop(document, id);
}

export function withinBuilderLimits(document: BuilderDocument): boolean {
  return allNodes(document).length <= MAX_BUILDER_NODES
    && new TextEncoder().encode(JSON.stringify(document)).byteLength <= MAX_BUILDER_BYTES;
}

export function computeDragMask(
  document: BuilderDocument,
  sourceId: string,
  blockCatalog: readonly BlockCatalogItemDto[],
  contextTarget?: BuilderContextTarget | null,
): DragCompatibilityMask | null {
  const source = locate(document, sourceId);
  if (!source) return null;
  const sourceSubtreeHeight = maxSubtreeDepth(source.node);
  const validParentIds = new Set<string | null>();
  const validSiblingTargetIds = new Set<string>();

  if (hasLockedAncestor(document, sourceId, blockCatalog)) {
    return { sourceId, sourceSubtreeHeight, validParentIds, validSiblingTargetIds };
  }

  const entries = allNodes(document);
  const subtreeIds = new Set(
    entries
      .filter(
        entry => entry.path.length >= source.path.length
          && source.path.every((step, idx) => entry.path[idx] === step),
      )
      .map(entry => entry.node.id),
  );

  const sourceSchema = findActiveCatalogItem(blockCatalog, source.node.block_id)?.schema;
  const sourceLocked = isLockedNode(source.node, blockCatalog);

  if (sourceSubtreeHeight <= MAX_BUILDER_DEPTH && canMoveWithLoopRules(document, sourceId, null, blockCatalog)) {
    if (!sourceLocked && sourceSchema) {
      if (
        matchesContextTarget(contextTarget, sourceSchema.allowed_contexts)
        && canParentAcceptChild(null, sourceSchema, null)
      ) {
        validParentIds.add(null);
      }
    } else if (sourceLocked) {
      validParentIds.add(null);
    }
  }

  for (const entry of entries) {
    if (subtreeIds.has(entry.node.id)) continue;
    if (entry.depth + sourceSubtreeHeight > MAX_BUILDER_DEPTH) continue;
    if (!canMoveWithLoopRules(document, sourceId, entry.node.id, blockCatalog)) continue;
    if (isLockedNode(entry.node, blockCatalog) || hasLockedAncestor(document, entry.node.id, blockCatalog)) {
      continue;
    }
    const parentSchema = findActiveCatalogItem(blockCatalog, entry.node.block_id)?.schema;
    if (!parentSchema?.supports_children) continue;

    const effectiveCtx = isInsideLoopItem(document, entry.node.id)
      ? ({ kind: 'loop_item' } as const)
      : contextTarget;
    if (!sourceLocked && sourceSchema) {
      if (
        matchesContextTarget(effectiveCtx, sourceSchema.allowed_contexts)
        && canParentAcceptChild(parentSchema, sourceSchema, entry.node.id)
      ) {
        validParentIds.add(entry.node.id);
      }
    } else if (
      source.parentId === entry.node.id
      || (
        matchesPatternList(parentSchema.allowed_children, '*')
        && matchesPatternList(parentSchema.allowed_children, source.node.block_id)
      )
    ) {
      validParentIds.add(entry.node.id);
    }
  }

  for (const entry of entries) {
    if (subtreeIds.has(entry.node.id) || entry.node.id === sourceId) continue;
    if (validParentIds.has(entry.parentId)) {
      validSiblingTargetIds.add(entry.node.id);
    }
  }

  return { sourceId, sourceSubtreeHeight, validParentIds, validSiblingTargetIds };
}

function summarizeDefaultChildren(
  specs: readonly DefaultChildSpec[] | undefined,
): { height: number; nodes: number; lists: number; slots: number; patternRefs: number } {
  if (!specs || specs.length === 0) {
    return { height: 0, nodes: 0, lists: 0, slots: 0, patternRefs: 0 };
  }
  let maxH = 0;
  let nodes = 0;
  let lists = 0;
  let slots = 0;
  let patternRefs = 0;
  for (const spec of specs) {
    const child = summarizeDefaultChildren(spec.children);
    if (1 + child.height > maxH) maxH = 1 + child.height;
    nodes += 1 + child.nodes;
    lists += (spec.block_id === 'core/list' ? 1 : 0) + child.lists;
    slots += (spec.block_id === 'core/entry-content' ? 1 : 0) + child.slots;
    patternRefs += (spec.block_id === 'core/pattern-ref' ? 1 : 0) + child.patternRefs;
  }
  return { height: maxH, nodes, lists, slots, patternRefs };
}

export function canInsertBlockAt(
  document: BuilderDocument,
  schema: BlockSchema,
  parentId: string | null,
  blockCatalog: readonly BlockCatalogItemDto[],
  contextTarget?: BuilderContextTarget | null,
  presetKey?: string,
  childrenSpecsOverride?: readonly DefaultChildSpec[],
): boolean {
  const effectiveCtx = parentId !== null && isInsideLoopItem(document, parentId)
    ? ({ kind: 'loop_item' } as const)
    : contextTarget;
  if (!matchesContextTarget(effectiveCtx, schema.allowed_contexts)) return false;

  let parentDepth = 0;
  let parentSchema: BlockSchema | null = null;
  if (parentId !== null) {
    const parentLoc = locate(document, parentId);
    if (!parentLoc) return false;
    if (isLockedNode(parentLoc.node, blockCatalog) || hasLockedAncestor(document, parentId, blockCatalog)) {
      return false;
    }
    parentSchema = findActiveCatalogItem(blockCatalog, parentLoc.node.block_id)?.schema ?? null;
    if (!parentSchema?.supports_children) return false;
    parentDepth = parentLoc.depth;
  }

  if (!canParentAcceptChild(parentSchema, schema, parentId)) return false;

  const preset = presetKey ? schema.presets?.find(p => p.key === presetKey) : undefined;
  const childSpecs = childrenSpecsOverride?.length
    ? childrenSpecsOverride
    : preset?.children?.length
      ? preset.children
      : schema.default_children;
  const sub = summarizeDefaultChildren(childSpecs);
  const subtreeHeight = 1 + sub.height;
  if (parentDepth + subtreeHeight > MAX_BUILDER_DEPTH) return false;

  const nodesAdded = 1 + sub.nodes;
  if (allNodes(document).length + nodesAdded > MAX_BUILDER_NODES) return false;
  const listsAdded = (schema.id === 'core/list' ? 1 : 0) + sub.lists;
  if (countBlocksByType(document, 'core/list') + listsAdded > MAX_BUILDER_LISTS) return false;
  const slotsAdded = (schema.id === 'core/entry-content' ? 1 : 0) + sub.slots;
  if (countBlocksByType(document, 'core/entry-content') + slotsAdded > MAX_BUILDER_CONTENT_SLOTS) return false;
  const patternRefsAdded = (schema.id === 'core/pattern-ref' ? 1 : 0) + sub.patternRefs;
  if (countBlocksByType(document, 'core/pattern-ref') + patternRefsAdded > MAX_BUILDER_PATTERN_INSTANCES) {
    return false;
  }
  return canInsertIntoLoop(document, schema, parentId, blockCatalog, childSpecs);
}

/** Guarantee immutable/inactive nodes retain identity, props, and inactive container subtrees. */
export function preservesAnchors(
  before: BuilderDocument,
  after: BuilderDocument,
  deletedId?: string,
  blockCatalog: readonly BlockCatalogItemDto[] = [],
): boolean {
  const beforeEntries = allNodes(before);
  const afterEntries = allNodes(after);
  const deleted = deletedId ? locate(before, deletedId) : undefined;
  if (deletedId && deleted && hasLockedAncestor(before, deletedId, blockCatalog)) {
    return false;
  }
  const afterById = new Map(afterEntries.map(entry => [entry.node.id, entry]));
  for (const entry of beforeEntries) {
    if (!isLockedNode(entry.node, blockCatalog)) continue;
    if (
      deleted
      && entry.path.length >= deleted.path.length
      && deleted.path.every((step, idx) => entry.path[idx] === step)
    ) {
      continue;
    }
    const current = afterById.get(entry.node.id);
    if (!current || JSON.stringify(current.node) !== JSON.stringify(entry.node)) {
      return false;
    }
    if (current.parentId !== entry.parentId) {
      if (hasLockedAncestor(before, entry.node.id, blockCatalog)) return false;
      if (current.parentId !== null) {
        const nextParent = afterById.get(current.parentId);
        const parentSchema = nextParent ? findActiveCatalogItem(blockCatalog, nextParent.node.block_id)?.schema : undefined;
        if (
          !parentSchema
          || !matchesPatternList(parentSchema.allowed_children, '*')
          || !matchesPatternList(parentSchema.allowed_children, entry.node.block_id)
        ) {
          return false;
        }
      }
    }
  }
  return true;
}

export function declaredBlockKeys(fields: readonly FieldSpec[]): Set<string> {
  const keys = new Set<string>();
  for (const field of fields) {
    if (!isUiOnlyFieldType(field.type)) keys.add(field.key);
  }
  return keys;
}

export function pluginValuesReadyForFreshNode(
  values: Record<string, unknown>,
  schema: BlockSchema,
): boolean {
  if (Object.keys(values).length > 64 || new TextEncoder().encode(JSON.stringify(values)).byteLength > 64 * 1024) {
    return false;
  }
  const readyPositiveInt = (v: unknown) => parsePositiveInt(v) !== null;
  const readyField = (
    field: FieldSpec,
    data: Record<string, unknown>,
    scopeFields: readonly FieldSpec[],
    parentVisible: boolean,
  ): boolean => {
    if (isUiOnlyFieldType(field.type)) return true;
    const isVisible = parentVisible && evaluateConditions(
      field.conditions, scopeFields, data, schema.fields, values,
    );
    return readyValue(field, data[field.key], isVisible, field.required === true);
  };
  const readyValue = (
    field: FieldSpec,
    value: unknown,
    isVisible: boolean,
    checkRequired: boolean,
  ): boolean => {
    if (field.type === 'group') {
      if (value !== undefined && value !== null && !object(value)) return false;
      const groupObj = object(value) ? value : {};
      if (Object.keys(groupObj).length > 64) return false;
      const childFields = field.fields ?? [];
      return childFields.every(child => readyField(child, groupObj, childFields, isVisible));
    }
    if (field.type === 'repeater') {
      if (value === undefined || value === null) return !isVisible || (field.min_items ?? 0) === 0;
      if (!Array.isArray(value) || value.length > (field.max_items ?? 32)
          || (isVisible && value.length < (field.min_items ?? 0))) return false;
      const childFields = field.fields ?? [];
      return value.every(
        item => object(item) && Object.keys(item).length <= 64
          && childFields.every(child => readyField(child, item, childFields, isVisible)),
      );
    }
    if (value === undefined || value === null) return !isVisible || !checkRequired || !field.required;
    if (field.responsive && object(value)) {
      if (Object.keys(value).some(key => !['desktop', 'tablet', 'mobile'].includes(key))) return false;
      const entries = Object.values(value).filter(item => item !== null && item !== undefined);
      if (isVisible && checkRequired && field.required && entries.length === 0) return false;
      return entries.every(item => readyValue(field, item, isVisible, false));
    }
    if (field.type === 'number' || field.type === 'range') {
      return typeof value === 'number' && Number.isFinite(value)
        && (field.min === undefined || value >= field.min)
        && (field.max === undefined || value <= field.max);
    }
    if (field.type === 'boolean') return typeof value === 'boolean';
    if (field.type === 'select') {
      if (field.options_source) {
        if (field.multiple) return Array.isArray(value) && value.every(v => typeof v === 'string');
        return typeof value === 'string' && (!isVisible || !checkRequired || !field.required || value.trim().length > 0);
      }
      const allowed = new Set((field.options ?? []).map(o => o.value));
      if (field.multiple) return Array.isArray(value) && value.every(v => typeof v === 'string' && allowed.has(v));
      if (typeof value !== 'string') return false;
      if (value === '') return Boolean(field.allow_null || !isVisible || !checkRequired || !field.required);
      return allowed.has(value);
    }
    if (field.type === 'radio_buttons') {
      return typeof value === 'string' && (field.options ?? []).some(o => o.value === value);
    }
    if (field.type === 'checkbox') {
      if (!Array.isArray(value)) return false;
      const allowed = new Set((field.options ?? []).map(o => o.value));
      if (field.min_items !== undefined && isVisible && value.length < field.min_items) return false;
      if (field.max_items !== undefined && value.length > field.max_items) return false;
      return value.every(v => typeof v === 'string' && allowed.has(v));
    }
    if (field.type === 'spacing') {
      if (!object(value)) return false;
      const lo = field.min ?? (field.property === 'margin' ? -128 : 0);
      const hi = field.max ?? 256;
      return Object.entries(value).every(([k, v]) =>
        ['top', 'right', 'bottom', 'left'].includes(k)
        && (v === null || v === undefined || (typeof v === 'number' && Number.isFinite(v) && v >= lo && v <= hi)));
    }
    if (field.type === 'link') {
      if (!object(value) || typeof value.url !== 'string') return false;
      return !isVisible || !checkRequired || !field.required || value.url.trim().length > 0;
    }
    if (field.type === 'gallery') {
      if (!Array.isArray(value)) return false;
      if (field.min_items !== undefined && isVisible && value.length < field.min_items) return false;
      if (field.max_items !== undefined && value.length > field.max_items) return false;
      return value.every(v => typeof v === 'string' && v.trim().length > 0);
    }
    if (field.type === 'entry_ref' || field.type === 'term_ref') {
      if (field.multiple) {
        if (!Array.isArray(value)) return false;
        if (field.min_items !== undefined && isVisible && value.length < field.min_items) return false;
        if (field.max_items !== undefined && value.length > field.max_items) return false;
        return value.every(readyPositiveInt);
      }
      return readyPositiveInt(value);
    }
    return typeof value === 'string' && (!isVisible || !checkRequired || !field.required || value.trim().length > 0);
  };
  return schema.fields.every(field => readyField(field, values, schema.fields, true));
}

export function collectPatternSourceIds(
  fields: readonly FieldSpec[],
  data: Record<string, unknown>,
  out: Set<number>,
): void {
  for (const field of fields) {
    if (isUiOnlyFieldType(field.type)) continue;
    const raw = data[field.key];
    if (raw === undefined || raw === null) continue;
    if (field.type === 'group' && object(raw)) {
      collectPatternSourceIds(field.fields ?? [], raw, out);
    } else if (field.type === 'select' && field.options_source === 'patterns') {
      const items = Array.isArray(raw) ? raw : [raw];
      for (const item of items) {
        const parsed = parsePositiveInt(item);
        if (parsed !== null) out.add(parsed);
      }
    }
  }
}

export function distinctPatternIdCount(
  document: BuilderDocument,
  blockCatalog: readonly BlockCatalogItemDto[] = [],
): number {
  const ids = new Set<number>();
  for (const entry of allNodes(document)) {
    const props = entry.node.props;
    if (!object(props)) continue;
    if (entry.node.block_id === 'core/pattern-ref') {
      const parsed = parsePositiveInt(props.pattern_id);
      if (parsed !== null) ids.add(parsed);
    } else {
      const schema = findCatalogItem(blockCatalog, entry.node.block_id)?.schema;
      if (schema) collectPatternSourceIds(schema.fields, props, ids);
    }
  }
  return ids.size;
}
