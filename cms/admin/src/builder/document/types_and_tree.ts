import { __, _x } from '../../i18n/index.tsx';
import type { BlockCatalogItemDto } from '../../types.ts';

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

export type ElementType = 'core/heading' | 'core/text' | 'core/image' | 'core/video' | 'core/button' | 'core/list' | 'core/contact_form' | 'core/search_form' | 'core/search_results';

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
  'core/search_form',
  'core/search_results',
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


export function normalizeBoxSpacing(
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
    return __("Template Part ({{key}})", { key });
  }
  if (element.type === 'plugin/block') {
    const blockId = typeof element.props?.block_id === 'string' ? element.props.block_id : '';
    const schema = blockCatalog.find(item => item.block_id === blockId);
    return schema?.schema.title || (blockId ? __("Block ({{id}})", { id: blockId }) : __("Unrecognized block"));
  }
  return ({
    'core/heading': __("Heading"), 'core/text': _x("Text", "builder.document"),
    'core/image': __("Image"), 'core/video': __("Video"), 'core/button': __("Button"), 'core/list': __("List"), 'core/contact_form': __("Contact form"),
    'core/search_form': __("Search form"), 'core/search_results': __("Search results"),
    'core/pattern_ref': _x("Reusable Pattern", "builder.document"),
    'core/content_slot': _x("Content Slot", "builder.document"),
    'legacy/block': __("Theme block"),
  } as Record<string, string>)[element.type] || __("Unrecognized block");
}


export function nodeTitle(location: NodeLocation, blockCatalog: BlockCatalogItemDto[] = []): string {
  const customName = location.node.meta?.name?.trim();
  return customName || nodeTypeTitle(location, blockCatalog);
}


export function clone<T>(document: T): T {
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
