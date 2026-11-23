import type { BlockCatalogItemDto, BlockPropValue, BlockSchemaV1 } from '../types.ts';

/** Homepage AST v1. The server validates every Save; this client preserves
 * unknown JSON properties so the Editor never strips historical/bound data. */
export interface BuilderElement {
  id: string;
  type: string;
  props: Record<string, unknown>;
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
export type ResponsiveAlign = 'start' | 'center' | 'end';
export interface ResponsiveStyle {
  padding_px?: number;
  gap_px?: number;
  columns_per_row?: number;
  text_align?: ResponsiveAlign;
}
export type BuilderResponsive = Partial<Record<BuilderViewport, ResponsiveStyle>>;
export interface ResponsiveContainer { responsive?: BuilderResponsive }
export interface BuilderColumn extends ResponsiveContainer { id: string; children: BuilderElement[] }
export interface BuilderRow extends ResponsiveContainer { id: string; children: BuilderColumn[] }
export interface BuilderSection extends ResponsiveContainer { id: string; children: BuilderRow[] }
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
  props?: Record<string, unknown>;
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
export const VIEWPORT_LABELS: Record<BuilderViewport, string> = {
  desktop: 'Desktop', tablet: 'Tablet', mobile: 'Mobile',
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

export function rowUsesGrid(row: BuilderRow): boolean {
  return Object.values(row.responsive ?? {}).some(style =>
    style?.gap_px !== undefined || style?.columns_per_row !== undefined
  );
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
      if (!['start', 'center', 'end'].includes(String(value))) return null;
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

export function nodeTitle(location: NodeLocation, blockCatalog: BlockCatalogItemDto[] = []): string {
  if (location.kind !== 'element') {
    return ({ section: 'Section', row: 'Row', column: 'Column' } as Record<string, string>)[location.kind];
  }
  const element = location.node as BuilderElement;
  if (element.type === 'core/template_part') {
    const key = typeof element.props?.part_key === 'string' && element.props.part_key
      ? element.props.part_key : 'part';
    return `Template Part (${key})`;
  }
  if (element.type === 'plugin/block') {
    const blockId = typeof element.props?.block_id === 'string' ? element.props.block_id : '';
    const schema = blockCatalog.find(item => item.block_id === blockId);
    return schema?.schema.title || (blockId ? `Block (${blockId})` : 'Block chưa nhận diện');
  }
  return ({
    'core/heading': 'Tiêu đề', 'core/text': 'Văn bản',
    'core/image': 'Hình ảnh', 'core/video': 'Video', 'core/button': 'Nút', 'core/list': 'Danh sách', 'core/contact_form': 'Form liên hệ',
    'core/pattern_ref': 'Reusable Pattern',
    'core/content_slot': 'Khung nội dung (Content Slot)',
    'legacy/block': 'Block theme',
  } as Record<string, string>)[element.type] || 'Block chưa nhận diện';
}

function object(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

/** Match the fields accepted by Rust's KnownElement / Props
 * #[serde(deny_unknown_fields)]. A historic element carrying an extra field
 * is Unresolved on the server and must remain an immutable anchor. */
const ELEMENT_PROP_KEYS: Record<string, readonly string[]> = {
  'core/heading': ['level', 'text', 'binding'],
  'core/text': ['text', 'binding'],
  'core/image': ['media_id', 'alt', 'binding'],
  'core/video': ['media_id', 'poster_image_id', 'controls', 'muted'],
  'core/button': ['label', 'url', 'binding'],
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
    || Object.keys(element).some(key => key !== 'id' && key !== 'type' && key !== 'props')
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
  registry: Record<string, Record<string, string>>,
  blockCatalog: BlockCatalogItemDto[] = [],
): boolean {
  const props = element.props;
  if (hasUnexpectedElementFields(element) || hasInvalidCoreBinding(element)) return true;
  switch (element.type) {
    case 'core/heading': return !Number.isInteger(props.level) || Number(props.level) < 1
      || Number(props.level) > 6 || typeof props.text !== 'string';
    case 'core/text': return typeof props.text !== 'string';
    case 'core/image': return typeof props.media_id !== 'string' || typeof props.alt !== 'string';
    case 'core/video': return typeof props.media_id !== 'string';
    case 'core/button': return typeof props.label !== 'string' || typeof props.url !== 'string';
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
      return typeof props.block_id !== 'string' || !object(props.values)
        || !Object.hasOwn(registry, props.block_id);
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
  document: BuilderDocument, id: string, registry: Record<string, Record<string, string>>,
  blockCatalog: BlockCatalogItemDto[] = [],
): boolean {
  const entries = allNodes(document);
  const location = entries.find(entry => entry.node.id === id);
  if (!location) return false;
  return entries.some(entry =>
    entry.kind === 'element'
    && isLockedElement(entry.node as BuilderElement, registry, blockCatalog)
    && entry.path.slice(0, location.path.length).every((step, index) => step === location.path[index])
  );
}

/** Preserve the absolute index and ancestor IDs of every unresolvable node,
 * even when other nodes are added/moved. Mirrors server's is_preserved guard. */
export function preservesAnchors(
  before: BuilderDocument, after: BuilderDocument,
  registry: Record<string, Record<string, string>>,
  deletedId?: string,
  blockCatalog: BlockCatalogItemDto[] = [],
): boolean {
  const deleted = deletedId ? locate(before, deletedId) : undefined;
  const afterEntries = allNodes(after);
  const beforeEntries = allNodes(before);
  return beforeEntries.every(entry => {
    if (entry.kind !== 'element' || !isLockedElement(entry.node as BuilderElement, registry, blockCatalog)) return true;
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

/** Group fields are flattened in the server schema. Repeater items hold their
 * own object values; responsive defaults start at desktop and cascade in UI. */
function initialBlockValues(fields: BlockSchemaV1['fields']): Record<string, unknown> {
  const values: Record<string, unknown> = {};
  for (const field of fields) {
    if (field.type === 'group') {
      Object.assign(values, initialBlockValues(field.fields));
    } else if (field.type === 'repeater') {
      values[field.key] = Array.from({ length: field.min_items ?? 0 },
        () => initialBlockValues(field.fields));
    } else if ('default' in field && field.default !== undefined && field.default !== null) {
      values[field.key] = 'responsive' in field && field.responsive
        ? { desktop: field.default } : field.default;
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
    'core/heading': { level: 2, text: 'Tiêu đề mới' },
    'core/text': { text: 'Nội dung mới' },
    'core/image': { media_id: '', alt: '' },
    'core/video': { media_id: '', controls: true, muted: false },
    'core/button': { label: 'Nút mới', url: '/' },
    'core/list': { type_key: 'post', include_descendants: false, sort: 'id_desc', page_size: 6 },
    'core/contact_form': { form_id: 1, submit_label: 'Gửi liên hệ' },
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
  registry: Record<string, Record<string, string>>,
  blockCatalog: BlockCatalogItemDto[] = [],
): { document: BuilderDocument; id: string } | null {
  const source = locate(document, id);
  if (!source || isProtectedNode(document, id, registry, blockCatalog)
      || countDescendantElementsByType(document, id, 'core/content_slot') > 0
      || countElementsByType(document, 'core/list')
        + countDescendantElementsByType(document, id, 'core/list') > 8) return null;

  // Server validation grants historical schema-upgrade exemptions only to
  // the original (element.id, block_id). Every clone has a fresh ID, so all
  // declarative blocks in the copied subtree must pass fresh-node checks.
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
  // The duplicate is already in next, so every generated ID is unique even
  // when the cloned subtree contains several nested nodes of the same kind.
  const duplicatePath = [...source.path.slice(0, -1), index + 1];
  const inserted = allNodes(next).filter(entry =>
    duplicatePath.every((step, depth) => entry.path[depth] === step));
  for (const entry of inserted) entry.node.id = newNodeId(entry.kind, next);

  if (distinctPatternIdCount(next) > 16 || !withinBuilderLimits(next)
      || !preservesAnchors(document, next, registry, undefined, blockCatalog)) return null;
  return { document: next, id: inserted[0].node.id };
}

/** Creates one row with its columns as a single undoable document mutation. */
export function appendRowWithColumns(
  document: BuilderDocument, sectionId: string, columnCount: 1 | 2 | 3,
): { document: BuilderDocument; id: string } | null {
  if (![1, 2, 3].includes(columnCount) || locate(document, sectionId)?.kind !== 'section') return null;
  const row = appendNode(document, sectionId, 'row', 'core/heading');
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

/** Group fields are flattened in PluginBlockProps.values; repeater values
 * belong to their declared top-level repeater field. */
function declaredBlockKeys(fields: BlockSchemaV1['fields']): Set<string> {
  const keys = new Set<string>();
  for (const field of fields) {
    if (field.type === 'group') {
      for (const key of declaredBlockKeys(field.fields)) keys.add(key);
    } else {
      keys.add(field.key);
    }
  }
  return keys;
}

/** Check the constraints that require previous_values=None in the Rust
 * BlockSchemaV1 validator: newly required fields, visible group/field rules,
 * responsive required values, and repeater min_items/subfields. */
function pluginValuesReadyForFreshNode(
  values: Record<string, unknown>, schema: BlockSchemaV1,
): boolean {
  if (Object.keys(values).length > 64
      || new TextEncoder().encode(JSON.stringify(values)).byteLength > 64 * 1024) return false;
  const fields = schema.fields.flatMap(field => field.type === 'group' ? field.fields : [field]);
  const defaults = new Map(fields.map(field =>
    [field.key, 'default' in field ? field.default : undefined]));
  const visible = (rule: { field: string; equals: string | number | boolean | null } | null | undefined) => {
    if (!rule) return true;
    const value = values[rule.field] ?? defaults.get(rule.field) ?? null;
    return value === rule.equals;
  };
  type Field = BlockSchemaV1['fields'][number];
  const readyLeaf = (field: Field, value: unknown, required: boolean): boolean => {
    if (value === undefined || value === null) return !required || !('required' in field && field.required);
    if (field.type === 'group' || field.type === 'repeater') return false;
    const responsive = 'responsive' in field && field.responsive;
    if (responsive && object(value)) {
      if (Object.keys(value).some(key => !['desktop', 'tablet', 'mobile'].includes(key))) return false;
      const entries = Object.values(value).filter(item => item !== null && item !== undefined);
      if (required && 'required' in field && field.required && entries.length === 0) return false;
      return entries.every(item => readyLeaf(field, item, false));
    }
    if (field.type === 'number' || field.type === 'range') {
      return typeof value === 'number' && Number.isFinite(value)
        && (field.min === undefined || field.min === null || value >= field.min)
        && (field.max === undefined || field.max === null || value <= field.max);
    }
    if (field.type === 'boolean') return typeof value === 'boolean';
    if (field.type === 'select') return typeof value === 'string'
      && field.options.some(option => option.value === value);
    return typeof value === 'string'
      && (!required || !('required' in field && field.required) || value.trim().length > 0);
  };
  const readyField = (field: Field, data: Record<string, unknown>, isVisible: boolean): boolean => {
    const current = data[field.key];
    if (field.type !== 'repeater') return readyLeaf(field, current, isVisible);
    if (current === undefined || current === null) return !isVisible || (field.min_items ?? 0) === 0;
    if (!Array.isArray(current) || current.length > (field.max_items ?? 32)
        || (isVisible && current.length < (field.min_items ?? 0))) return false;
    return current.every(item => object(item)
      && Object.keys(item).length <= 64
      && field.fields.every(child => readyLeaf(child, item[child.key],
        isVisible && visible(child.show_if))));
  };
  return schema.fields.every(field => field.type === 'group'
    ? field.fields.every(child => readyField(child, values,
      visible(field.show_if) && visible(child.show_if)))
    : readyField(field, values, visible(field.show_if)));
}

function distinctPatternIdCount(document: BuilderDocument): number {
  return new Set(allNodes(document)
    .filter(entry => entry.kind === 'element'
      && (entry.node as BuilderElement).type === 'core/pattern_ref')
    .map(entry => (entry.node as BuilderElement).props.pattern_id)).size;
}

export function copyNodeOptions(
  document: BuilderDocument, id: string,
  registry: Record<string, Record<string, string>>,
  blockCatalog: BlockCatalogItemDto[] = [],
): StudioBuilderClipboard | null {
  const item = locate(document, id);
  if (!item) return null;
  if (!isElement(item.node)) {
    const responsive = item.node.responsive;
    return { kind: item.kind, responsive: responsive && Object.keys(responsive).length
      ? clone(responsive) : undefined };
  }

  const element = item.node;
  if (element.type === 'core/content_slot' || isLockedElement(element, registry, blockCatalog)) return null;
  const clipboard: StudioBuilderClipboard = { kind: 'element', elementType: element.type };
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
  return distinctPatternIdCount(next) <= 16 && withinBuilderLimits(next) ? next : null;
}

/** Drag/drop may reorder siblings only if preserved nodes retain their exact
 * index, ancestor IDs, and serialized element content. */
export function reorderSiblingNode(
  document: BuilderDocument, sourceId: string, targetId: string,
  position: 'before' | 'after', registry: Record<string, Record<string, string>>,
  blockCatalog: BlockCatalogItemDto[] = [],
): BuilderDocument | null {
  if (sourceId === targetId || (position !== 'before' && position !== 'after')) return null;
  const source = locate(document, sourceId);
  const target = locate(document, targetId);
  if (!source || !target || source.kind !== target.kind || source.parentId !== target.parentId
      || isProtectedNode(document, sourceId, registry, blockCatalog)) return null;
  const next = clone(document);
  const siblings = childrenAt(next, source.parentId ? locate(next, source.parentId) : undefined);
  if (!siblings) return null;
  const sourceIndex = source.path[source.path.length - 1];
  const [moving] = siblings.splice(sourceIndex, 1);
  const targetIndex = siblings.findIndex(node => node.id === targetId);
  if (!moving || targetIndex === -1) return null;
  const insertIndex = targetIndex + (position === 'after' ? 1 : 0);
  if (insertIndex === sourceIndex) return null;
  siblings.splice(insertIndex, 0, moving);
  return preservesAnchors(document, next, registry, undefined, blockCatalog) ? next : null;
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
  const location = locate(document, id);
  if (!location) return null;
  const next = clone(document);
  const siblings = childrenAt(next, location.parentId ? locate(next, location.parentId) : undefined);
  if (!siblings) return null;
  siblings.splice(location.path[location.path.length - 1], 1);
  return next;
}

export function moveNode(document: BuilderDocument, id: string, direction: -1 | 1): BuilderDocument | null {
  const location = locate(document, id);
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

export function setNodeField(
  document: BuilderDocument, id: string, field: string, value: BlockPropValue | undefined,
): BuilderDocument | null {
  const next = clone(document);
  const location = locate(next, id);
  if (!location || !isElement(location.node)) return null;
  const element = location.node;
  if (element.type === 'plugin/block' && field === 'values') {
    // The schema form supplies a complete values object. Replace it as one
    // draft mutation so repeater reorder and responsive edits remain atomic.
    if (!object(value) || !object(element.props.values)) return null;
    element.props = { ...element.props, values: { ...value } };
  } else if (element.type === 'legacy/block' || element.type === 'plugin/block') {
    const values = element.props.values;
    if (!object(values)) return null;
    const updated = { ...values };
    if (value === undefined) delete updated[field];
    else updated[field] = value;
    element.props = { ...element.props, values: updated };
  } else {
    if (value === undefined) {
      const rest = { ...element.props };
      delete rest[field];
      element.props = rest;
    } else {
      element.props = { ...element.props, [field]: value };
    }
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
