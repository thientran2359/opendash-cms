import { tStatic } from '../i18n/index.tsx';
import { evaluateConditions } from '../fields/conditions.ts';
import { isUiOnlyFieldType } from '../fields/helpers.ts';
import type { FieldSpec } from '../fields/types.ts';
import type { BlockCatalogItemDto, BlockPropValue, BlockSchemaV1 } from '../types.ts';

/** Homepage AST v1. The server validates every Save; this client preserves
 * unknown JSON properties so the Editor never strips historical/bound data. */
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
  synthetic?: boolean;
}
export interface BuilderElement {
  id: string;
  type: string;
  props: Record<string, unknown>;
  meta?: NodeMeta;
}
/** ADR-0009: one typed source reference per eligible core element. */
export type BindingTarget = 'text' | 'media_id' | 'alt' | 'label' | 'url';
export type BindingSource =
  | { kind: 'title' }
  | { kind: 'field'; key: string }
  | { kind: 'route_context'; field_key: string };
export interface ElementBinding {
  type_key: string;
  entry_slug: string;
  entry_locale?: string | null;
  source: BindingSource;
  target: BindingTarget;
}
export type ListSort = 'id_desc' | 'id_asc';
export interface ListProps {
  type_key: string;
  category_id?: number;
  tag_id?: number;
  /** C2 generic taxonomy filter. A valid List requires both fields together. */
  taxonomy_key?: string;
  term_id?: number;
  include_descendants: boolean;
  sort: ListSort;
  page_size: number;
}
export type BuilderViewport = 'desktop' | 'tablet' | 'mobile';
export type ResponsiveField = 'padding_px' | 'gap_px' | 'columns_per_row' | 'text_align';
export type ResponsiveAlign = 'start' | 'center' | 'end' | 'justify';
export interface BoxSpacing {
  top?: number;
  right?: number;
  bottom?: number;
  left?: number;
}
export interface ResponsiveStyle {
  padding_px?: number;
  padding?: BoxSpacing;
  margin?: BoxSpacing;
  gap_px?: number;
  columns_per_row?: number;
  text_align?: ResponsiveAlign;
}
export type BuilderResponsive = Partial<Record<BuilderViewport, ResponsiveStyle>>;
export type SectionWidth = 'full' | 'boxed' | 'wide';
export interface SectionStyle {
  background_color?: string;
  background_media_id?: string;
  content_width?: SectionWidth;
}
export interface ResponsiveContainer { responsive?: BuilderResponsive; meta?: NodeMeta }
export interface BuilderColumn extends ResponsiveContainer { id: string; children: BuilderElement[] }
export interface BuilderRow extends ResponsiveContainer { id: string; children: BuilderColumn[] }
export interface BuilderSection extends ResponsiveContainer { id: string; style?: SectionStyle; children: BuilderRow[] }
export interface BuilderDocument { schema_version: number; sections: BuilderSection[] }
export type BuilderNode = BuilderSection | BuilderRow | BuilderColumn | BuilderElement;
export type NodeKind = 'section' | 'row' | 'column' | 'element';
/** ElementType is the legacy binding/field registry's closed set. PatternRef
 * is an insertable core block with its own typed Inspector, no field binding. */
export type ElementType = 'core/heading' | 'core/text' | 'core/image' | 'core/video' | 'core/button' | 'core/list' | 'core/contact_form';
export type TemplateElementType = 'core/content_slot' | 'core/template_part';
/** Declarative schema IDs are selected in the palette, but their AST type is
 * always plugin/block. Keep ElementType closed for legacy binding controls. */
export type InsertableElementType = ElementType | 'core/pattern_ref' | TemplateElementType
  | 'plugin/block' | (string & {});
export const ELEMENT_TYPES: InsertableElementType[] = [
  'core/heading',
  'core/text',
  'core/image',
  'core/video',
  'core/button',
  'core/list',
  'core/contact_form',
  'core/pattern_ref',
  'core/content_slot',
  'core/template_part',
];
export const VALID_TEMPLATE_PART_KEY = /^[a-z][a-z0-9-]{0,63}$/;
export const EMPTY_DOCUMENT: BuilderDocument = { schema_version: 1, sections: [] };
export interface NodeLocation {
  node: BuilderNode;
  kind: NodeKind;
  parentId: string | null;
  path: number[];
}

/** Copy only editable configuration, never a node ID or an element binding. */
export interface StudioBuilderClipboard {
  kind: NodeKind;
  elementType?: string;
  blockId?: string;
  responsive?: BuilderResponsive;
  sectionStyle?: SectionStyle;
  props?: Record<string, unknown>;
  class_name?: string;
  visibility?: NodeVisibility;
}

export function allNodes(document: BuilderDocument): NodeLocation[] {
  const found: NodeLocation[] = [];
  document.sections.forEach((section, s) => {
    found.push({ node: section, kind: 'section', parentId: null, path: [s] });
    section.children.forEach((row, r) => {
      found.push({ node: row, kind: 'row', parentId: section.id, path: [s, r] });
      row.children.forEach((column, c) => {
        found.push({ node: column, kind: 'column', parentId: row.id, path: [s, r, c] });
        column.children.forEach((element, e) => {
          found.push({ node: element, kind: 'element', parentId: column.id, path: [s, r, c, e] });
        });
      });
    });
  });
  return found;
}

export function locate(document: BuilderDocument, id: string | null): NodeLocation | undefined {
  return id ? allNodes(document).find(entry => entry.node.id === id) : undefined;
}

export function isElement(node: BuilderNode): node is BuilderElement {
  return 'type' in node;
}

export const VIEWPORT_WIDTHS: Record<BuilderViewport, number> = {
  desktop: 1280, tablet: 834, mobile: 390,
};
export const RESPONSIVE_FIELDS: Record<'section' | 'row' | 'column', ResponsiveField[]> = {
  section: ['padding_px'],
  row: ['padding_px', 'gap_px', 'columns_per_row'],
  column: ['padding_px', 'text_align'],
};

/** Desktop → Tablet → Mobile property-wise cascade, including explicit zero. */
export function effectiveResponsive(
  node: ResponsiveContainer, viewport: BuilderViewport,
): ResponsiveStyle {
  const styles = node.responsive ?? {};
  const result: ResponsiveStyle = { ...styles.desktop };
  if (viewport !== 'desktop') Object.assign(result, styles.tablet);
  if (viewport === 'mobile') Object.assign(result, styles.mobile);
  return result;
}

/** Remove an unset viewport and parent object to keep v1 JSON sparse. */
export function setResponsiveField(
  document: BuilderDocument, id: string, viewport: BuilderViewport,
  field: ResponsiveField, value: number | ResponsiveAlign | null,
): BuilderDocument | null {
  const next = clone(document);
  const location = locate(next, id);
  if (!location || location.kind === 'element'
      || !RESPONSIVE_FIELDS[location.kind].includes(field)) return null;
  if (value !== null) {
    if (field === 'text_align') {
      if (!['start', 'center', 'end', 'justify'].includes(String(value))) return null;
    } else if (typeof value !== 'number' || !Number.isInteger(value)
        || value < (field === 'columns_per_row' ? 1 : 0)
        || value > (field === 'columns_per_row' ? 6 : 64)) return null;
  }
  const node = location.node as ResponsiveContainer;
  const responsive: BuilderResponsive = { ...node.responsive };
  const style: ResponsiveStyle = { ...responsive[viewport] };
  if (value === null) delete style[field];
  else (style as Record<string, number | string>)[field] = value;
  if (Object.keys(style).length === 0) delete responsive[viewport];
  else responsive[viewport] = style;
  if (Object.keys(responsive).length === 0) delete node.responsive;
  else node.responsive = responsive;
  return next;
}

function normalizeBoxSpacing(
  raw: BoxSpacing | null, min: number, max: number,
): BoxSpacing | undefined | null {
  if (raw === null) return undefined;
  const out: BoxSpacing = {};
  for (const side of ['top', 'right', 'bottom', 'left'] as const) {
    const val = raw[side];
    if (val === undefined || val === null) continue;
    if (typeof val !== 'number' || !Number.isInteger(val) || val < min || val > max) return null;
    out[side] = val;
  }
  return Object.keys(out).length ? out : undefined;
}

export function setContainerBoxSpacing(
  document: BuilderDocument, id: string, viewport: BuilderViewport,
  kind: 'padding' | 'margin', spacing: BoxSpacing | null,
): BuilderDocument | null {
  const min = kind === 'padding' ? 0 : -128;
  const normalized = normalizeBoxSpacing(spacing, min, 256);
  if (normalized === null) return null;
  const next = clone(document);
  const location = locate(next, id);
  if (!location || location.kind === 'element' || isSyntheticNode(location.node)) return null;
  const node = location.node as ResponsiveContainer;
  const responsive: BuilderResponsive = { ...node.responsive };
  const style: ResponsiveStyle = { ...responsive[viewport] };
  if (normalized === undefined) {
    delete style[kind];
  } else {
    style[kind] = normalized;
    if (kind === 'padding') delete style.padding_px;
  }
  if (Object.keys(style).length === 0) delete responsive[viewport];
  else responsive[viewport] = style;
  if (Object.keys(responsive).length === 0) delete node.responsive;
  else node.responsive = responsive;
  return next;
}

export function setSectionStyle(
  document: BuilderDocument, id: string, patch: Partial<SectionStyle>,
): BuilderDocument | null {
  const next = clone(document);
  const location = locate(next, id);
  if (!location || location.kind !== 'section' || isSyntheticNode(location.node)) return null;
  const section = location.node as BuilderSection;
  const merged: SectionStyle = { ...(section.style ?? {}) };
  if ('background_color' in patch) {
    const val = (patch.background_color ?? '').trim();
    if (!val) delete merged.background_color;
    else merged.background_color = val;
  }
  if ('background_media_id' in patch) {
    const val = (patch.background_media_id ?? '').trim();
    if (!val) delete merged.background_media_id;
    else if (!/^[a-zA-Z0-9_-]{1,128}$/.test(val)) return null;
    else merged.background_media_id = val;
  }
  if ('content_width' in patch) {
    if (!patch.content_width) delete merged.content_width;
    else merged.content_width = patch.content_width;
  }
  if (Object.keys(merged).length === 0) delete section.style;
  else section.style = merged;
  return next;
}

/** Drops tablet/mobile overrides for one responsive property so all 3 viewports match Desktop. */
export function syncContainerResponsiveToDesktop(
  document: BuilderDocument, id: string, property: ResponsiveField | 'padding' | 'margin',
): BuilderDocument | null {
  const next = clone(document);
  const location = locate(next, id);
  if (!location || location.kind === 'element' || isSyntheticNode(location.node)) return null;
  const node = location.node as ResponsiveContainer;
  if (!node.responsive) return next;
  const responsive: BuilderResponsive = { ...node.responsive };
  for (const bp of ['tablet', 'mobile'] as const) {
    if (!responsive[bp]) continue;
    const style: ResponsiveStyle = { ...responsive[bp] };
    delete (style as Record<string, unknown>)[property];
    if (property === 'padding') delete style.padding_px;
    if (Object.keys(style).length === 0) delete responsive[bp];
    else responsive[bp] = style;
  }
  if (Object.keys(responsive).length === 0) delete node.responsive;
  else node.responsive = responsive;
  return next;
}

/** IDs obey the core valid_package_id alphabet and length contract. */
export function newNodeId(kind: NodeKind, document: BuilderDocument): string {
  const used = new Set(allNodes(document).map(item => item.node.id));
  let id: string;
  do {
    // getRandomValues also works on HTTP LAN access where randomUUID may be
    // unavailable because the page is outside a secure browser context.
    const bytes = new Uint8Array(16);
    crypto.getRandomValues(bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    const hex = Array.from(bytes, value => value.toString(16).padStart(2, '0')).join('');
    id = kind + '-' + [
      hex.slice(0, 8), hex.slice(8, 12), hex.slice(12, 16),
      hex.slice(16, 20), hex.slice(20),
    ].join('-');
  } while (used.has(id));
  return id;
}

export function isSyntheticNode(node: BuilderNode): boolean {
  return Boolean(node.meta?.synthetic);
}

export interface VisibleNodeLocation extends NodeLocation {
  visualParentId: string | null;
}

/** Flatten document nodes while skipping transparent synthetic wrapper containers. */
export function visibleNodesInTree(document: BuilderDocument): VisibleNodeLocation[] {
  const entries = allNodes(document);
  const byId = new Map(entries.map(entry => [entry.node.id, entry]));
  const result: VisibleNodeLocation[] = [];
  for (const entry of entries) {
    if (isSyntheticNode(entry.node)) continue;
    let p = entry.parentId;
    while (p) {
      const parentEntry = byId.get(p);
      if (!parentEntry || !isSyntheticNode(parentEntry.node)) break;
      p = parentEntry.parentId;
    }
    result.push({ ...entry, visualParentId: p });
  }
  return result;
}

/** Lift a node up through any single-child synthetic wrappers so delete/move/duplicate/reorder act cleanly on the full freeform block carrier. */
export function carrierLocation(document: BuilderDocument, id: string): NodeLocation | undefined {
  let loc = locate(document, id);
  while (loc && loc.parentId) {
    const parentLoc = locate(document, loc.parentId);
    if (!parentLoc || isElement(parentLoc.node)
        || !isSyntheticNode(parentLoc.node)
        || parentLoc.node.children.length !== 1) break;
    loc = parentLoc;
  }
  return loc;
}

export function nodeTypeTitle(location: NodeLocation, blockCatalog: BlockCatalogItemDto[] = []): string {
  if (location.kind !== 'element') {
    return ({ section: 'Section', row: 'Row', column: 'Column' } as Record<string, string>)[location.kind];
  }
  const element = location.node as BuilderElement;
  if (element.type === 'core/template_part') {
    const key = typeof element.props?.part_key === 'string' && element.props.part_key
      ? element.props.part_key : 'part';
    return tStatic('builder.document.template_part', { key });
  }
  if (element.type === 'plugin/block') {
    const blockId = typeof element.props?.block_id === 'string' ? element.props.block_id : '';
    const schema = blockCatalog.find(item => item.block_id === blockId);
    return schema?.schema.title || (blockId ? tStatic('builder.document.block', { id: blockId }) : tStatic('builder.document.unknown'));
  }
  return ({
    'core/heading': tStatic('builder.document.heading'), 'core/text': tStatic('builder.document.text'),
    'core/image': tStatic('builder.document.image'), 'core/video': tStatic('builder.document.video'), 'core/button': tStatic('builder.document.button'), 'core/list': tStatic('builder.document.list'), 'core/contact_form': tStatic('builder.document.contact'),
    'core/pattern_ref': tStatic('builder.document.pattern'),
    'core/content_slot': tStatic('builder.document.content_slot'),
    'legacy/block': tStatic('builder.document.theme'),
  } as Record<string, string>)[element.type] || tStatic('builder.document.unknown');
}

export function nodeTitle(location: NodeLocation, blockCatalog: BlockCatalogItemDto[] = []): string {
  const customName = location.node.meta?.name?.trim();
  return customName || nodeTypeTitle(location, blockCatalog);
}

function object(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

/** Match the fields accepted by Rust's KnownElement / Props
 * #[serde(deny_unknown_fields)]. A historic element carrying an extra field
 * is Unresolved on the server and must remain an immutable anchor. */
const ELEMENT_PROP_KEYS: Record<string, readonly string[]> = {
  'core/heading': ['level', 'text', 'binding'],
  'core/text': ['text', 'rich', 'binding'],
  'core/image': ['media_id', 'alt', 'binding'],
  'core/video': ['media_id', 'poster_image_id', 'controls', 'muted'],
  'core/button': ['label', 'url', 'new_tab', 'nofollow', 'binding'],
  'core/list': ['type_key', 'category_id', 'tag_id', 'taxonomy_key', 'term_id',
    'include_descendants', 'sort', 'page_size'],
  'core/contact_form': ['form_id', 'submit_label'],
  'core/pattern_ref': ['pattern_id'],
  'core/content_slot': [],
  'core/template_part': ['part_key'],
  'legacy/block': ['block_id', 'values'],
  'plugin/block': ['block_id', 'values'],
};

function hasUnexpectedElementFields(element: BuilderElement): boolean {
  const allowed = ELEMENT_PROP_KEYS[element.type];
  return !allowed
    || Object.keys(element).some(key => key !== 'id' && key !== 'type' && key !== 'props' && key !== 'meta')
    || !object(element.props)
    || Object.keys(element.props).some(key => !allowed.includes(key));
}

function hasInvalidCoreBinding(element: BuilderElement): boolean {
  const binding = element.props.binding;
  if (binding === undefined) return false;
  if (!object(binding) || typeof binding.type_key !== 'string'
      || typeof binding.entry_slug !== 'string'
      || !['text', 'media_id', 'alt', 'label', 'url'].includes(String(binding.target))
      || Object.keys(binding).some(key => !['type_key', 'entry_slug', 'source', 'target'].includes(key))
      || !object(binding.source)) return true;
  const source = binding.source;
  const sourceKeys = source.kind === 'title' ? ['kind']
    : source.kind === 'field' ? ['kind', 'key']
      : source.kind === 'route_context' ? ['kind', 'field_key'] : null;
  return !sourceKeys || Object.keys(source).some(key => !sourceKeys.includes(key))
    || (source.kind === 'field' && typeof source.key !== 'string')
    || (source.kind === 'route_context' && typeof source.field_key !== 'string');
}

/** Mirrors the core/list validation contract for both editor updates and
 * locked historical nodes. Category + tag may coexist; generic filters
 * cannot be mixed with those legacy aliases.
 */
function validListTaxonomyFilter(props: {
  category_id?: unknown;
  tag_id?: unknown;
  taxonomy_key?: unknown;
  term_id?: unknown;
  include_descendants?: unknown;
}): boolean {
  const category = props.category_id !== undefined;
  const tag = props.tag_id !== undefined;
  const taxonomy = props.taxonomy_key !== undefined;
  const term = props.term_id !== undefined;
  return !(category && (!Number.isSafeInteger(props.category_id) || Number(props.category_id) <= 0))
    && !(tag && (!Number.isSafeInteger(props.tag_id) || Number(props.tag_id) <= 0))
    && !(taxonomy && (typeof props.taxonomy_key !== 'string'
      || !/^[a-z][a-z0-9_]{0,63}$/.test(props.taxonomy_key)))
    && !(term && (!Number.isSafeInteger(props.term_id) || Number(props.term_id) <= 0))
    && taxonomy === term
    && !(taxonomy && (category || tag))
    && (props.include_descendants === undefined || typeof props.include_descendants === 'boolean')
    && !(props.include_descendants === true && !category && !taxonomy);
}

/** Unknown, malformed, or inactive-provider elements are immutable anchors.
 * The server's is_preserved validator is the final authority. */
export function isLockedElement(
  element: BuilderElement,
  blockCatalog: BlockCatalogItemDto[] = [],
): boolean {
  const props = element.props;
  if (hasUnexpectedElementFields(element) || hasInvalidCoreBinding(element)) return true;
  switch (element.type) {
    case 'core/heading': return !Number.isInteger(props.level) || Number(props.level) < 1
      || Number(props.level) > 6 || typeof props.text !== 'string';
    case 'core/text': return typeof props.text !== 'string'
      || (props.rich !== undefined && typeof props.rich !== 'boolean');
    case 'core/image': return typeof props.media_id !== 'string' || typeof props.alt !== 'string';
    case 'core/video': return typeof props.media_id !== 'string';
    case 'core/button': return typeof props.label !== 'string' || typeof props.url !== 'string'
      || (props.new_tab !== undefined && typeof props.new_tab !== 'boolean')
      || (props.nofollow !== undefined && typeof props.nofollow !== 'boolean');
    case 'core/contact_form': return typeof props.form_id !== 'number' || props.form_id <= 0 || typeof props.submit_label !== 'string' || !props.submit_label.trim();
    case 'core/list': return typeof props.type_key !== 'string'
      || !/^[a-z][a-z0-9_]{0,63}$/.test(props.type_key)
      || !validListTaxonomyFilter(props)
      || (props.sort !== undefined && !['id_desc', 'id_asc'].includes(String(props.sort)))
      || (props.page_size !== undefined && (!Number.isInteger(props.page_size)
        || Number(props.page_size) < 1 || Number(props.page_size) > 20));
    // 0 is the intentional client-side placeholder while selecting a Pattern.
    // Server validation rejects it on Save; retain the Inspector so the user
    // can select a valid Pattern without throwing away the new block.
    case 'core/pattern_ref': return !Number.isSafeInteger(props.pattern_id) || Number(props.pattern_id) < 0;
    case 'core/content_slot': return Object.keys(props).length > 0;
    case 'core/template_part': return typeof props.part_key !== 'string';
    case 'legacy/block':
      // Retired M1–M4 theme block nodes remain immutable read-only anchors (D3).
      return true;
    case 'plugin/block': {
      // Rust PluginBlockProps has deny_unknown_fields. An extra top-level
      // property (including a seemingly valid binding) makes the entire node
      // unresolved; never offer mutable controls for that preserved payload.
      if (typeof props.block_id !== 'string' || !object(props.values)
          || Object.keys(props).some(key => key !== 'block_id' && key !== 'values')) return true;
      const descriptor = blockCatalog.find(item => item.block_id === props.block_id);
      return !descriptor || descriptor.status !== 'active' || descriptor.declaration_state !== 'declared';
    }
    default: return true;
  }
}

export function isProtectedNode(
  document: BuilderDocument, id: string,
  blockCatalog: BlockCatalogItemDto[] = [],
): boolean {
  const entries = allNodes(document);
  const location = entries.find(entry => entry.node.id === id);
  if (!location) return false;
  return entries.some(entry =>
    entry.kind === 'element'
    && isLockedElement(entry.node as BuilderElement, blockCatalog)
    && entry.path.slice(0, location.path.length).every((step, index) => step === location.path[index])
  );
}

/** Preserve the absolute index and ancestor IDs of every unresolvable node,
 * even when other nodes are added/moved. Mirrors server's is_preserved guard. */
export function preservesAnchors(
  before: BuilderDocument, after: BuilderDocument,
  deletedId?: string,
  blockCatalog: BlockCatalogItemDto[] = [],
): boolean {
  const deleted = deletedId ? locate(before, deletedId) : undefined;
  const afterEntries = allNodes(after);
  const beforeEntries = allNodes(before);
  return beforeEntries.every(entry => {
    if (entry.kind !== 'element' || !isLockedElement(entry.node as BuilderElement, blockCatalog)) return true;
    if (deleted && deleted.path.every((step, index) => entry.path[index] === step)) return true;
    const current = afterEntries.find(item => item.node.id === entry.node.id);
    return !!current && JSON.stringify(current.node) === JSON.stringify(entry.node)
      && JSON.stringify(current.path) === JSON.stringify(entry.path)
      && entry.path.every((_, depth) => {
        const oldAncestor = beforeEntries.find(item => JSON.stringify(item.path) === JSON.stringify(entry.path.slice(0, depth + 1)));
        const newAncestor = afterEntries.find(item => JSON.stringify(item.path) === JSON.stringify(entry.path.slice(0, depth + 1)));
        return oldAncestor?.node.id === newAncestor?.node.id;
      });
  });
}

function clone<T>(document: T): T {
  return JSON.parse(JSON.stringify(document)) as T;
}

/** Import theme AST as an independent copy. Generate an ID for every level,
 * including containers and locked/unknown elements, without mutating the
 * immutable theme source or touching the existing target document.
 */
export function cloneWithFreshNodeIds(
  source: BuilderDocument, existing?: BuilderDocument,
): BuilderDocument {
  const copy = clone(source);
  const reservedSections = [...copy.sections, ...(existing?.sections ?? [])];
  const reserved = { schema_version: copy.schema_version, sections: reservedSections };
  for (const entry of allNodes(copy)) {
    entry.node.id = newNodeId(entry.kind, reserved);
  }
  return copy;
}

function childrenAt(document: BuilderDocument, location: NodeLocation | undefined): BuilderNode[] | undefined {
  if (!location) return document.sections;
  if (location.kind === 'element') return undefined;
  const current = locate(document, location.node.id);
  return current && !isElement(current.node) ? current.node.children : undefined;
}

/** Group fields hold nested objects `{ ... }`; UI-only fields (`tab`, `accordion`, `message`)
 * emit no keys; repeater items hold their own object values; responsive defaults start at desktop. */
function initialBlockValues(fields: readonly FieldSpec[]): Record<string, unknown> {
  const values: Record<string, unknown> = {};
  for (const field of fields) {
    if (isUiOnlyFieldType(field.type)) continue;
    if (field.type === 'group') {
      values[field.key] = initialBlockValues(field.fields ?? []);
    } else if (field.type === 'repeater') {
      values[field.key] = Array.from({ length: field.min_items ?? 0 },
        () => initialBlockValues(field.fields ?? []));
    } else if (field.default !== undefined && field.default !== null) {
      const defCopy = typeof field.default === 'object' ? clone(field.default) : field.default;
      values[field.key] = field.responsive ? { desktop: defCopy } : defCopy;
    }
  }
  return values;
}

export function appendNode(
  document: BuilderDocument, parentId: string | null, kind: NodeKind, elementType: InsertableElementType,
  options?: { blockSchema?: BlockSchemaV1 },
): { document: BuilderDocument; id: string } | null {
  const parent = locate(document, parentId);
  if ((kind === 'section' && parentId !== null)
      || (kind === 'row' && parent?.kind !== 'section')
      || (kind === 'column' && parent?.kind !== 'row')
      || (kind === 'element' && parent?.kind !== 'column')) return null;
  const next = clone(document);
  const children = childrenAt(next, parent);
  if (!children) return null;
  if (kind === 'element' && elementType === 'core/list'
      && allNodes(document).filter(item => item.kind === 'element'
        && (item.node as BuilderElement).type === 'core/list').length >= 8) return null;
  const blockSchema = options?.blockSchema;
  const declarative = kind === 'element'
    && (elementType === 'plugin/block' || blockSchema?.id === elementType);
  if (declarative && (!blockSchema || blockSchema.schema_version !== 1)) return null;
  const id = newNodeId(kind, next);
  const defaults: Record<string, Record<string, unknown>> = {
    'core/heading': { level: 2, text: tStatic('builder.document.new_heading') },
    'core/text': { text: tStatic('builder.document.new_content') },
    'core/image': { media_id: '', alt: '' },
    'core/video': { media_id: '', controls: true, muted: false },
    'core/button': { label: tStatic('builder.document.new_button'), url: '/' },
    'core/list': { type_key: 'post', include_descendants: false, sort: 'id_desc', page_size: 6 },
    'core/contact_form': { form_id: 1, submit_label: tStatic('builder.document.submit') },
    'core/pattern_ref': { pattern_id: 0 },
    'core/content_slot': {},
    'core/template_part': { part_key: 'header' },
  };
  if (kind === 'element' && !declarative && !Object.hasOwn(defaults, elementType)) return null;
  const node: BuilderNode = kind === 'element'
    ? declarative && blockSchema
      ? { id, type: 'plugin/block', props: { block_id: blockSchema.id, values: initialBlockValues(blockSchema.fields) } }
      : { id, type: elementType, props: { ...defaults[elementType] } }
    : { id, children: [] };
  children.push(node);
  return { document: next, id };
}

/** The server rejects documents beyond either bound. Count UTF-8 bytes, not
 * JavaScript string units, because Builder content can contain Unicode. */
function withinBuilderLimits(document: BuilderDocument): boolean {
  return allNodes(document).length <= 512
    && new TextEncoder().encode(JSON.stringify(document)).byteLength <= 256 * 1024;
}

/** Duplicate an editable subtree without moving any immutable anchor. */
export function duplicateNode(
  document: BuilderDocument, id: string,
  blockCatalog: BlockCatalogItemDto[] = [],
): { document: BuilderDocument; id: string } | null {
  const leafLoc = locate(document, id);
  const source = carrierLocation(document, id);
  if (!leafLoc || !source || isProtectedNode(document, source.node.id, blockCatalog)
      || countDescendantElementsByType(document, source.node.id, 'core/content_slot') > 0
      || countElementsByType(document, 'core/list')
        + countDescendantElementsByType(document, source.node.id, 'core/list') > 8) return null;

  const sourcePath = source.path;
  for (const entry of allNodes(document)) {
    if (entry.kind !== 'element'
        || !sourcePath.every((step, depth) => entry.path[depth] === step)) continue;
    const element = entry.node as BuilderElement;
    if (element.type !== 'plugin/block') continue;
    const descriptor = blockCatalog.find(item => item.block_id === element.props.block_id
      && item.status === 'active' && item.declaration_state === 'declared');
    if (!descriptor || !object(element.props.values)
        || !pluginValuesReadyForFreshNode(element.props.values, descriptor.schema)) return null;
  }

  const next = clone(document);
  const siblings = childrenAt(next, source.parentId ? locate(next, source.parentId) : undefined);
  if (!siblings) return null;
  const index = source.path[source.path.length - 1];
  siblings.splice(index + 1, 0, clone(siblings[index]));
  const duplicatePath = [...source.path.slice(0, -1), index + 1];
  const inserted = allNodes(next).filter(entry =>
    duplicatePath.every((step, depth) => entry.path[depth] === step));
  for (const entry of inserted) entry.node.id = newNodeId(entry.kind, next);

  if (distinctPatternIdCount(next, blockCatalog) > 16 || !withinBuilderLimits(next)
      || !preservesAnchors(document, next, undefined, blockCatalog)) return null;
  const targetRelPath = [...duplicatePath, ...leafLoc.path.slice(source.path.length)];
  const returnEntry = inserted.find(entry =>
    entry.path.length === targetRelPath.length
    && targetRelPath.every((step, idx) => entry.path[idx] === step)) ?? inserted[0];
  return { document: next, id: returnEntry.node.id };
}

/** Creates one row with its columns as a single undoable document mutation; when parentId is null, wraps in a transparent synthetic Section. */
export function appendRowWithColumns(
  document: BuilderDocument, parentId: string | null, columnCount: 1 | 2 | 3,
): { document: BuilderDocument; id: string } | null {
  if (![1, 2, 3].includes(columnCount)) return null;
  let working = document;
  let sectionId = parentId;
  if (parentId === null) {
    const sec = appendNode(working, null, 'section', 'core/heading');
    if (!sec) return null;
    working = sec.document;
    const secLoc = locate(working, sec.id);
    if (secLoc) secLoc.node.meta = { synthetic: true };
    sectionId = sec.id;
  } else if (locate(working, parentId)?.kind !== 'section') {
    return null;
  }
  const row = appendNode(working, sectionId, 'row', 'core/heading');
  if (!row) return null;
  let next = row.document;
  for (let index = 0; index < columnCount; index += 1) {
    const column = appendNode(next, row.id, 'column', 'core/heading');
    if (!column) return null;
    next = column.document;
  }
  const addedRow = locate(next, row.id)?.node;
  if (!addedRow || isElement(addedRow)) return null;
  if (columnCount > 1) addedRow.responsive = { desktop: { columns_per_row: columnCount } };
  return withinBuilderLimits(next) ? { document: next, id: row.id } : null;
}

/** Inserts an element at root (null), directly inside a Section, or inside a Column, creating transparent synthetic wrappers where needed. */
export function appendFreeformElement(
  document: BuilderDocument,
  parentId: string | null,
  elementType: InsertableElementType,
  options?: { blockSchema?: BlockSchemaV1 },
): { document: BuilderDocument; id: string } | null {
  const parentLoc = locate(document, parentId);
  if (parentId === null) {
    const sec = appendNode(document, null, 'section', 'core/heading');
    if (!sec) return null;
    const secLoc = locate(sec.document, sec.id);
    if (secLoc) secLoc.node.meta = { synthetic: true };
    const row = appendNode(sec.document, sec.id, 'row', 'core/heading');
    if (!row) return null;
    const rowLoc = locate(row.document, row.id);
    if (rowLoc) rowLoc.node.meta = { synthetic: true };
    const col = appendNode(row.document, row.id, 'column', 'core/heading');
    if (!col) return null;
    const colLoc = locate(col.document, col.id);
    if (colLoc) colLoc.node.meta = { synthetic: true };
    const el = appendNode(col.document, col.id, 'element', elementType, options);
    return el && withinBuilderLimits(el.document) ? el : null;
  }
  if (parentLoc?.kind === 'section') {
    const row = appendNode(document, parentId, 'row', 'core/heading');
    if (!row) return null;
    const rowLoc = locate(row.document, row.id);
    if (rowLoc) rowLoc.node.meta = { synthetic: true };
    const col = appendNode(row.document, row.id, 'column', 'core/heading');
    if (!col) return null;
    const colLoc = locate(col.document, col.id);
    if (colLoc) colLoc.node.meta = { synthetic: true };
    const el = appendNode(col.document, col.id, 'element', elementType, options);
    return el && withinBuilderLimits(el.document) ? el : null;
  }
  if (parentLoc?.kind === 'column') {
    const el = appendNode(document, parentId, 'element', elementType, options);
    return el && withinBuilderLimits(el.document) ? el : null;
  }
  return null;
}

/** Top-level persisted value keys for a unified V1 block schema (excluding UI-only fields).
 * Groups retain nested object values under their own field key. */
function declaredBlockKeys(fields: readonly FieldSpec[]): Set<string> {
  const keys = new Set<string>();
  for (const field of fields) {
    if (!isUiOnlyFieldType(field.type)) keys.add(field.key);
  }
  return keys;
}

/** Check the constraints that require previous_values=None in the Rust
 * BlockSchemaV1 validator: newly required fields, visible DNF conditions,
 * nested groups, responsive required values, and repeater min_items/subfields. */
function pluginValuesReadyForFreshNode(
  values: Record<string, unknown>, schema: BlockSchemaV1,
): boolean {
  if (Object.keys(values).length > 64
      || new TextEncoder().encode(JSON.stringify(values)).byteLength > 64 * 1024) return false;
  const readyPositiveInt = (v: unknown) => typeof v === 'number' && Number.isInteger(v) && v > 0;
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
      return value.every(item => object(item)
        && Object.keys(item).length <= 64
        && childFields.every(child => readyField(child, item, childFields, isVisible)));
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
      if (value === '') return Boolean(field.allow_null || (!isVisible || !checkRequired || !field.required));
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
    return typeof value === 'string'
      && (!isVisible || !checkRequired || !field.required || value.trim().length > 0);
  };
  return schema.fields.every(field => readyField(field, values, schema.fields, true));
}

function collectPatternSourceIds(
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
        if (typeof item === 'string' && /^\d+$/.test(item.trim())) {
          const parsed = Number(item.trim());
          if (parsed > 0) out.add(parsed);
        }
      }
    }
  }
}

function distinctPatternIdCount(
  document: BuilderDocument,
  blockCatalog: BlockCatalogItemDto[] = [],
): number {
  const ids = new Set<number>();
  for (const entry of allNodes(document)) {
    if (entry.kind !== 'element') continue;
    const el = entry.node as BuilderElement;
    if (el.type === 'core/pattern_ref') {
      const pid = el.props.pattern_id;
      if (typeof pid === 'number' && pid > 0) ids.add(pid);
    } else if (el.type === 'plugin/block' && object(el.props.values)) {
      const descriptor = blockCatalog.find(item => item.block_id === el.props.block_id);
      if (descriptor) collectPatternSourceIds(descriptor.schema.fields, el.props.values, ids);
    }
  }
  return ids.size;
}

export function copyNodeOptions(
  document: BuilderDocument, id: string,
  blockCatalog: BlockCatalogItemDto[] = [],
): StudioBuilderClipboard | null {
  const item = locate(document, id);
  if (!item) return null;
  const attachSharedStyle = (target: StudioBuilderClipboard): StudioBuilderClipboard => {
    const className = item.node.meta?.class_name?.trim();
    if (className) target.class_name = className;
    const vis = item.node.meta?.visibility;
    if (vis && (vis.hide_desktop || vis.hide_tablet || vis.hide_mobile)) {
      target.visibility = clone(vis);
    }
    return target;
  };
  if (!isElement(item.node)) {
    const responsive = item.node.responsive;
    const base: StudioBuilderClipboard = {
      kind: item.kind,
      responsive: responsive && Object.keys(responsive).length ? clone(responsive) : undefined,
    };
    if (item.kind === 'section') {
      const style = (item.node as BuilderSection).style;
      if (style && (style.background_color || style.background_media_id || style.content_width)) {
        base.sectionStyle = clone(style);
      }
    }
    return attachSharedStyle(base);
  }

  const element = item.node;
  if (element.type === 'core/content_slot' || isLockedElement(element, blockCatalog)) return null;
  const clipboard: StudioBuilderClipboard = attachSharedStyle({
    kind: 'element',
    elementType: element.type,
  });
  if (element.type === 'plugin/block' || element.type === 'legacy/block') {
    clipboard.blockId = element.props.block_id as string;
    const values = element.props.values as Record<string, unknown>;
    if (element.type === 'plugin/block') {
      const active = blockCatalog.find(item => item.block_id === clipboard.blockId
        && item.status === 'active' && item.declaration_state === 'declared');
      if (!active) return null;
      const keys = declaredBlockKeys(active.schema.fields);
      // Clone the entire JSON payload first: optional draft fields may be
      // undefined, and cloning an individual undefined value would throw.
      clipboard.props = { values: Object.fromEntries(
        Object.entries(clone(values)).filter(([key]) => keys.has(key)),
      ) };
      if (!pluginValuesReadyForFreshNode(
        clipboard.props.values as Record<string, unknown>, active.schema,
      )) return null;
    } else {
      clipboard.props = { values: clone(values) };
    }
  } else {
    clipboard.props = clone(element.props);
    delete clipboard.props.binding;
  }
  return clipboard;
}

export function canPasteOptions(
  clipboard: StudioBuilderClipboard | null, target: NodeLocation, locked: boolean,
): boolean {
  if (!clipboard || locked || clipboard.kind !== target.kind) return false;
  if (target.kind !== 'element') return true;
  if (!isElement(target.node) || target.node.type !== clipboard.elementType
      || target.node.type === 'core/content_slot') return false;
  return (target.node.type !== 'plugin/block' && target.node.type !== 'legacy/block')
    || target.node.props.block_id === clipboard.blockId;
}

/** Never transfer binding references or a block_id to the target node. */
export function pasteNodeOptions(
  document: BuilderDocument, id: string, clipboard: StudioBuilderClipboard,
  blockCatalog: BlockCatalogItemDto[] = [],
): BuilderDocument | null {
  const next = clone(document);
  const target = locate(next, id);
  if (!target || !canPasteOptions(clipboard, target, false)) return null;
  if (isElement(target.node)
      && (hasUnexpectedElementFields(target.node) || hasInvalidCoreBinding(target.node))) return null;
  if (!isElement(target.node)) {
    if (clipboard.responsive && Object.keys(clipboard.responsive).length) {
      target.node.responsive = clone(clipboard.responsive);
    } else {
      delete target.node.responsive;
    }
    if (target.kind === 'section') {
      const sec = target.node as BuilderSection;
      if (clipboard.sectionStyle && Object.keys(clipboard.sectionStyle).length) {
        sec.style = clone(clipboard.sectionStyle);
      } else {
        delete sec.style;
      }
    }
  } else if (target.node.type === 'plugin/block' || target.node.type === 'legacy/block') {
    if (!object(clipboard.props?.values) || !object(target.node.props.values)) return null;
    target.node.props.values = clone(clipboard.props.values);
  } else {
    if (!object(clipboard.props)) return null;
    const props = clone(clipboard.props);
    delete props.binding;
    if (target.node.props.binding !== undefined) props.binding = target.node.props.binding;
    target.node.props = props;
  }
  const mergedMeta: NodeMeta = { ...(target.node.meta ?? {}) };
  if (clipboard.class_name) mergedMeta.class_name = clipboard.class_name;
  else delete mergedMeta.class_name;
  if (clipboard.visibility && (clipboard.visibility.hide_desktop || clipboard.visibility.hide_tablet || clipboard.visibility.hide_mobile)) {
    mergedMeta.visibility = clone(clipboard.visibility);
  } else {
    delete mergedMeta.visibility;
  }
  if (Object.keys(mergedMeta).length === 0) delete target.node.meta;
  else target.node.meta = mergedMeta;
  return distinctPatternIdCount(next, blockCatalog) <= 16 && withinBuilderLimits(next) ? next : null;
}

/** Drag/drop may reorder siblings only if preserved nodes retain their exact
 * index, ancestor IDs, and serialized element content. Supports transparent carriers across freeform blocks. */
export function reorderSiblingNode(
  document: BuilderDocument, sourceId: string, targetId: string,
  position: 'before' | 'after',
  blockCatalog: BlockCatalogItemDto[] = [],
): BuilderDocument | null {
  if (sourceId === targetId || (position !== 'before' && position !== 'after')) return null;
  const source = carrierLocation(document, sourceId);
  const target = carrierLocation(document, targetId);
  if (!source || !target || source.node.id === target.node.id
      || source.kind !== target.kind || source.parentId !== target.parentId
      || isProtectedNode(document, source.node.id, blockCatalog)) return null;
  const next = clone(document);
  const siblings = childrenAt(next, source.parentId ? locate(next, source.parentId) : undefined);
  if (!siblings) return null;
  const sourceIndex = source.path[source.path.length - 1];
  const [moving] = siblings.splice(sourceIndex, 1);
  const targetIndex = siblings.findIndex(node => node.id === target.node.id);
  if (!moving || targetIndex === -1) return null;
  const insertIndex = targetIndex + (position === 'after' ? 1 : 0);
  if (insertIndex === sourceIndex) return null;
  siblings.splice(insertIndex, 0, moving);
  return preservesAnchors(document, next, undefined, blockCatalog) ? next : null;
}

export function countElementsByType(document: BuilderDocument, elementType: string): number {
  return allNodes(document).filter(
    item => item.kind === 'element' && (item.node as BuilderElement).type === elementType,
  ).length;
}

export function countDescendantElementsByType(
  document: BuilderDocument,
  nodeId: string,
  elementType: string,
): number {
  const target = locate(document, nodeId);
  if (!target) return 0;
  return allNodes(document).filter(
    item =>
      item.kind === 'element' &&
      (item.node as BuilderElement).type === elementType &&
      item.path.length >= target.path.length &&
      target.path.every((step, idx) => item.path[idx] === step),
  ).length;
}

/** Honors TemplateCandidate slot cardinality: never remove the last core/content_slot
 * when requiresContentSlot is true; core/template_part is always removable. */
export function canDeleteElement(
  document: BuilderDocument,
  nodeId: string,
  options?: { requiresContentSlot?: boolean },
): boolean {
  const target = locate(document, nodeId);
  if (!target) return false;
  if (!options?.requiresContentSlot) return true;
  const totalSlots = countElementsByType(document, 'core/content_slot');
  const removedSlots = countDescendantElementsByType(document, nodeId, 'core/content_slot');
  return totalSlots - removedSlots >= 1;
}

export function createStarterTemplateDocument(requiresContentSlot: boolean): BuilderDocument {
  let doc: BuilderDocument = { schema_version: 1, sections: [] };
  const sec = appendNode(doc, null, 'section', 'core/heading');
  if (!sec) return doc;
  doc = sec.document;
  const row = appendNode(doc, sec.id, 'row', 'core/heading');
  if (!row) return doc;
  doc = row.document;
  const col = appendNode(doc, row.id, 'column', 'core/heading');
  if (!col) return doc;
  doc = col.document;
  const el = appendNode(doc, col.id, 'element', requiresContentSlot ? 'core/content_slot' : 'core/heading');
  return el ? el.document : doc;
}

/** Typed list updates; never encode filters/booleans as static string props. */
export function setListProps(document: BuilderDocument, id: string, props: ListProps): BuilderDocument | null {
  if (!props.type_key || !/^[a-z][a-z0-9_]{0,63}$/.test(props.type_key)
      || !Number.isInteger(props.page_size) || props.page_size < 1 || props.page_size > 20
      || !['id_desc', 'id_asc'].includes(props.sort)
      || !validListTaxonomyFilter(props)) return null;
  const next = clone(document);
  const element = locate(next, id)?.node;
  if (!element || !isElement(element) || element.type !== 'core/list') return null;
  element.props = { ...props };
  return next;
}

export function removeNode(document: BuilderDocument, id: string): BuilderDocument | null {
  const location = carrierLocation(document, id);
  if (!location) return null;
  const next = clone(document);
  const siblings = childrenAt(next, location.parentId ? locate(next, location.parentId) : undefined);
  if (!siblings) return null;
  siblings.splice(location.path[location.path.length - 1], 1);
  return next;
}

export function moveNode(document: BuilderDocument, id: string, direction: -1 | 1): BuilderDocument | null {
  const location = carrierLocation(document, id);
  if (!location) return null;
  const next = clone(document);
  const siblings = childrenAt(next, location.parentId ? locate(next, location.parentId) : undefined);
  if (!siblings) return null;
  const index = location.path[location.path.length - 1];
  const target = index + direction;
  if (target < 0 || target >= siblings.length) return null;
  [siblings[index], siblings[target]] = [siblings[target], siblings[index]];
  return next;
}

export function setNodeMeta(
  document: BuilderDocument, id: string, patch: Partial<NodeMeta>,
): BuilderDocument | null {
  const next = clone(document);
  const location = locate(next, id);
  if (!location || isSyntheticNode(location.node)) return null;
  const current: NodeMeta = { ...(location.node.meta ?? {}) };
  if ('name' in patch) {
    const val = patch.name ?? '';
    if (val.length > 80) return null;
    if (!val.trim()) delete current.name;
    else current.name = val;
  }
  if ('html_id' in patch) {
    const val = (patch.html_id ?? '').trim();
    if (!val) delete current.html_id;
    else if (!/^[a-zA-Z0-9_-]{1,64}$/.test(val)) return null;
    else current.html_id = val;
  }
  if ('class_name' in patch) {
    const val = patch.class_name ?? '';
    if (!val.trim()) delete current.class_name;
    else if (val.length > 256 || !/^[a-zA-Z0-9 _:/.\[\]%#-]{1,256}$/.test(val)) return null;
    else current.class_name = val;
  }
  if ('visibility' in patch) {
    const v = patch.visibility ?? {};
    const normalized: NodeVisibility = {};
    if (v.hide_desktop) normalized.hide_desktop = true;
    if (v.hide_tablet) normalized.hide_tablet = true;
    if (v.hide_mobile) normalized.hide_mobile = true;
    if (Object.keys(normalized).length === 0) delete current.visibility;
    else current.visibility = normalized;
  }
  if (Object.keys(current).length === 0) delete location.node.meta;
  else location.node.meta = current;
  return next;
}

export function setNodeField(
  document: BuilderDocument, id: string, field: string, value: BlockPropValue | undefined,
): BuilderDocument | null {
  return patchNodeFields(document, id, { [field]: value });
}

export function patchNodeFields(
  document: BuilderDocument, id: string, patch: Record<string, BlockPropValue | undefined>,
): BuilderDocument | null {
  const next = clone(document);
  const location = locate(next, id);
  if (!location || !isElement(location.node)) return null;
  const element = location.node;
  if (element.type === 'plugin/block' && Object.hasOwn(patch, 'values')) {
    const value = patch.values;
    if (!object(value) || !object(element.props.values)) return null;
    element.props = { ...element.props, values: { ...value } };
  } else if (element.type === 'legacy/block' || element.type === 'plugin/block') {
    const values = element.props.values;
    if (!object(values)) return null;
    const updated = { ...values };
    for (const [field, value] of Object.entries(patch)) {
      if (value === undefined) delete updated[field];
      else updated[field] = value;
    }
    element.props = { ...element.props, values: updated };
  } else {
    const updated = { ...element.props };
    for (const [field, value] of Object.entries(patch)) {
      if (value === undefined) delete updated[field];
      else updated[field] = value;
    }
    element.props = updated;
  }
  return next;
}

/** Typed binding updates are kept separate from editing static props.
 * Unbind removes only binding, preserving static values and responsive metadata.
 */
export function setElementBinding(
  document: BuilderDocument, id: string, binding: ElementBinding | null,
): BuilderDocument | null {
  const next = clone(document);
  const item = locate(next, id);
  if (!item || !isElement(item.node) || !ELEMENT_TYPES.includes(item.node.type as ElementType)) return null;
  const props = { ...item.node.props };
  if (binding === null) delete props.binding;
  else props.binding = { ...binding, source: { ...binding.source } };
  item.node.props = props;
  return next;
}
