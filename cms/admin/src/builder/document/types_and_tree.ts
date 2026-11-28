import { __ } from '../../i18n/index.tsx';
import type { BlockCatalogItemDto, BlockSchema } from '../../types.ts';

export interface NodeVisibility {
  hide_desktop?: boolean;
  hide_tablet?: boolean;
  hide_mobile?: boolean;
}

export interface NodeMeta {
  name?: string;
  html_id?: string;
  class_name?: string;
  visibility?: NodeVisibility;
}

export interface BlockNode {
  id: string;
  block_id: string;
  props?: Record<string, unknown>;
  meta?: NodeMeta;
  children?: BlockNode[];
}

export interface BuilderDocument {
  schema_version: number;
  blocks: BlockNode[];
}

export type BuilderViewport = 'desktop' | 'tablet' | 'mobile';

export interface BoxSpacing {
  top?: number;
  right?: number;
  bottom?: number;
  left?: number;
}

export const VIEWPORT_WIDTHS: Record<BuilderViewport, number> = {
  desktop: 1280,
  tablet: 834,
  mobile: 390,
};

export const VALID_TEMPLATE_PART_KEY = /^[a-z][a-z0-9-]{0,63}$/;

export const EMPTY_DOCUMENT: BuilderDocument = {
  schema_version: 1,
  blocks: [],
};

export interface NodeLocation {
  node: BlockNode;
  parentId: string | null;
  path: number[];
  /** 1-based tree depth (root blocks sit at depth 1; max depth is 8). */
  depth: number;
}

export type BuilderContextTarget =
  | { kind: 'entry_body'; type_key?: string | null }
  | { kind: 'pattern' }
  | { kind: 'theme_pattern' }
  | { kind: 'part'; part_key?: string | null }
  | { kind: 'single'; type_key?: string | null }
  | { kind: 'single_custom'; type_key?: string | null }
  | { kind: 'singular' }
  | { kind: 'page' }
  | { kind: 'page_layout' }
  | { kind: 'index' }
  | { kind: 'archive'; type_key?: string | null }
  | { kind: 'taxonomy'; taxonomy_key?: string | null }
  | { kind: 'not_found' }
    | { kind: 'search' }
  | { kind: 'loop_item' };

export interface DragCompatibilityMask {
  sourceId: string;
  sourceSubtreeHeight: number;
  /** Container node IDs (`null` for document root) able to receive the dragged subtree. */
  validParentIds: Set<string | null>;
  /** Existing node IDs whose parent slot can receive the dragged subtree as a sibling. */
  validSiblingTargetIds: Set<string>;
}

/** Copy only editable props and presentation metadata, never node IDs or HTML anchors. */
export interface StudioBuilderClipboard {
  block_id: string;
  props?: Record<string, unknown>;
  class_name?: string;
  visibility?: NodeVisibility;
}

export function object(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

export function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function collectNodes(
  nodes: readonly BlockNode[],
  parentId: string | null,
  parentPath: number[],
  depth: number,
  out: NodeLocation[],
): void {
  nodes.forEach((node, index) => {
    const path = [...parentPath, index];
    out.push({ node, parentId, path, depth });
    if (Array.isArray(node.children) && node.children.length > 0) {
      collectNodes(node.children, node.id, path, depth + 1, out);
    }
  });
}

export function allNodes(document: BuilderDocument | null | undefined): NodeLocation[] {
  if (!document || !Array.isArray(document.blocks)) return [];
  const out: NodeLocation[] = [];
  collectNodes(document.blocks, null, [], 1, out);
  return out;
}

export function locate(
  document: BuilderDocument | null | undefined,
  id: string | null | undefined,
): NodeLocation | undefined {
  if (!id) return undefined;
  return allNodes(document).find(entry => entry.node.id === id);
}

export function findNode(
  document: BuilderDocument | null | undefined,
  id: string | null | undefined,
): BlockNode | undefined {
  return locate(document, id)?.node;
}

export function visibleNodesInTree(document: BuilderDocument | null | undefined): NodeLocation[] {
  return allNodes(document);
}

export function maxSubtreeDepth(node: BlockNode): number {
  const children = Array.isArray(node.children) ? node.children : [];
  if (children.length === 0) return 1;
  let deepest = 0;
  for (const child of children) {
    const childDepth = maxSubtreeDepth(child);
    if (childDepth > deepest) deepest = childDepth;
  }
  return 1 + deepest;
}

function sanitizeIdPrefix(blockId: string): string {
  const segment = blockId.split('/').pop() ?? blockId;
  const normalized = segment.toLowerCase().replace(/[^a-z0-9-]+/g, '-').replace(/^-+|-+$/g, '');
  if (!normalized || !/^[a-z0-9]/.test(normalized)) return 'blk';
  return normalized.slice(0, 20);
}

/** Generate a valid package_id-compliant unique node ID. */
export function newNodeId(
  blockIdOrPrefix: string,
  usedIdsOrDocument: BuilderDocument | ReadonlySet<string>,
): string {
  const used = usedIdsOrDocument instanceof Set
    ? usedIdsOrDocument
    : new Set(allNodes(usedIdsOrDocument as BuilderDocument).map(entry => entry.node.id));
  const prefix = sanitizeIdPrefix(blockIdOrPrefix);
  let id: string;
  do {
    const bytes = new Uint8Array(16);
    crypto.getRandomValues(bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    const hex = Array.from(bytes, value => value.toString(16).padStart(2, '0')).join('');
    id = `${prefix}-${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}`;
  } while (used.has(id));
  return id.slice(0, 64);
}

export function findCatalogItem(
  blockCatalog: readonly BlockCatalogItemDto[],
  blockId: string,
): BlockCatalogItemDto | undefined {
  return blockCatalog.find(item => item.block_id === blockId);
}

export function findActiveCatalogItem(
  blockCatalog: readonly BlockCatalogItemDto[],
  blockId: string,
): BlockCatalogItemDto | undefined {
  return blockCatalog.find(
    item => item.block_id === blockId && item.status === 'active' && item.declaration_state === 'declared',
  );
}

export function findBlockSchema(
  blockCatalog: readonly BlockCatalogItemDto[],
  blockId: string,
  activeOnly = false,
): BlockSchema | undefined {
  const item = activeOnly ? findActiveCatalogItem(blockCatalog, blockId) : findCatalogItem(blockCatalog, blockId);
  return item?.schema;
}

export function isContainerBlock(
  node: BlockNode,
  blockCatalog: readonly BlockCatalogItemDto[] = [],
): boolean {
  const schema = findBlockSchema(blockCatalog, node.block_id);
  if (schema?.supports_children !== undefined) return schema.supports_children;
  return Array.isArray(node.children) && node.children.length > 0;
}

/** True when a schema restricts allowed_parents to specific parent container block ID(s). */
export function isLockedCompoundChildSchema(schema: BlockSchema | null | undefined): boolean {
  const parents = schema?.allowed_parents;
  if (!Array.isArray(parents) || parents.length === 0) return false;
  const positive = parents.filter(token => !token.startsWith('!'));
  if (positive.length === 0) return false;
  return !positive.includes('*') && !positive.includes('root');
}

/** Return the single locked compound child catalog item (e.g. core/column for core/row). */
export function getLockedCompoundChild(
  parentSchema: BlockSchema | null | undefined,
  blockCatalog: readonly BlockCatalogItemDto[],
): BlockCatalogItemDto | null {
  if (!parentSchema?.supports_children) return null;
  const children = parentSchema.allowed_children;
  if (!Array.isArray(children) || children.length === 0) return null;
  const positive = children.filter(token => !token.startsWith('!'));
  if (positive.length === 0 || positive.includes('*')) return null;
  for (const childBlockId of positive) {
    const item = findActiveCatalogItem(blockCatalog, childBlockId);
    if (item && isLockedCompoundChildSchema(item.schema)) {
      return item;
    }
  }
  return null;
}

export function nodeTypeTitle(
  nodeOrLoc: BlockNode | NodeLocation,
  blockCatalog: readonly BlockCatalogItemDto[] = [],
): string {
  const node = 'node' in nodeOrLoc ? nodeOrLoc.node : nodeOrLoc;
  const schema = findBlockSchema(blockCatalog, node.block_id);
  if (node.block_id === 'core/template-part') {
    const partKey = typeof node.props?.part_key === 'string' && node.props.part_key.trim()
      ? node.props.part_key.trim()
      : 'part';
    return schema?.title
      ? `${schema.title} (${partKey})`
      : __("Template Part ({{key}})", { key: partKey });
  }
  if (schema?.title) return schema.title;
  return node.block_id ? __("Block ({{id}})", { id: node.block_id }) : __("Unrecognized block");
}

export function nodeTitle(
  nodeOrLoc: BlockNode | NodeLocation,
  blockCatalog: readonly BlockCatalogItemDto[] = [],
): string {
  const node = 'node' in nodeOrLoc ? nodeOrLoc.node : nodeOrLoc;
  const customName = node.meta?.name?.trim();
  return customName || nodeTypeTitle(node, blockCatalog);
}

export function assignFreshSubtreeIds(
  nodes: BlockNode[],
  usedIds: Set<string>,
): void {
  for (const node of nodes) {
    const fresh = newNodeId(node.block_id, usedIds);
    usedIds.add(fresh);
    node.id = fresh;
    if (Array.isArray(node.children) && node.children.length > 0) {
      assignFreshSubtreeIds(node.children, usedIds);
    }
  }
}

export function cloneWithFreshNodeIds(
  source: BuilderDocument,
  existing?: BuilderDocument,
): BuilderDocument {
  const copy = clone(source);
  const usedIds = new Set(allNodes(existing).map(entry => entry.node.id));
  if (Array.isArray(copy.blocks)) {
    assignFreshSubtreeIds(copy.blocks, usedIds);
  } else {
    copy.blocks = [];
  }
  copy.schema_version = 1;
  return copy;
}
