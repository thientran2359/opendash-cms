/** Homepage AST v1. The server validates every Save; this client preserves
 * unknown JSON properties so the Editor never strips historical/bound data. */
export interface BuilderElement {
  id: string;
  type: string;
  props: Record<string, unknown>;
}
export interface BuilderColumn { id: string; children: BuilderElement[] }
export interface BuilderRow { id: string; children: BuilderColumn[] }
export interface BuilderSection { id: string; children: BuilderRow[] }
export interface BuilderDocument { schema_version: number; sections: BuilderSection[] }
export type BuilderNode = BuilderSection | BuilderRow | BuilderColumn | BuilderElement;
export type NodeKind = 'section' | 'row' | 'column' | 'element';
export type ElementType = 'core/heading' | 'core/text' | 'core/image' | 'core/button';
export const ELEMENT_TYPES: ElementType[] = ['core/heading', 'core/text', 'core/image', 'core/button'];
export const EMPTY_DOCUMENT: BuilderDocument = { schema_version: 1, sections: [] };
export interface NodeLocation {
  node: BuilderNode;
  kind: NodeKind;
  parentId: string | null;
  path: number[];
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

export function nodeTitle(location: NodeLocation): string {
  if (location.kind !== 'element') {
    return ({ section: 'Section', row: 'Row', column: 'Column' } as Record<string, string>)[location.kind];
  }
  const element = location.node as BuilderElement;
  return ({
    'core/heading': 'Tiêu đề', 'core/text': 'Văn bản',
    'core/image': 'Hình ảnh', 'core/button': 'Nút',
    'legacy/block': 'Block theme',
  } as Record<string, string>)[element.type] || 'Block chưa nhận diện';
}

function object(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

/** Unknown, malformed, or inactive-provider elements are immutable anchors.
 * The server's is_preserved validator is the final authority. */
export function isLockedElement(element: BuilderElement, registry: Record<string, Record<string, string>>): boolean {
  const props = element.props;
  if (!object(props)) return true;
  const binding = props.binding;
  if (binding !== undefined && (!object(binding) || typeof binding.type_key !== 'string'
      || typeof binding.entry_slug !== 'string' || !object(binding.source)
      || typeof binding.target !== 'string')) return true;
  switch (element.type) {
    case 'core/heading': return !Number.isInteger(props.level) || Number(props.level) < 1
      || Number(props.level) > 6 || typeof props.text !== 'string';
    case 'core/text': return typeof props.text !== 'string';
    case 'core/image': return typeof props.media_id !== 'string' || typeof props.alt !== 'string';
    case 'core/button': return typeof props.label !== 'string' || typeof props.url !== 'string';
    case 'legacy/block':
      return typeof props.block_id !== 'string' || !object(props.values)
        || !Object.hasOwn(registry, props.block_id);
    default: return true;
  }
}

export function isProtectedNode(
  document: BuilderDocument, id: string, registry: Record<string, Record<string, string>>,
): boolean {
  const entries = allNodes(document);
  const location = entries.find(entry => entry.node.id === id);
  if (!location) return false;
  return entries.some(entry =>
    entry.kind === 'element'
    && isLockedElement(entry.node as BuilderElement, registry)
    && entry.path.slice(0, location.path.length).every((step, index) => step === location.path[index])
  );
}

/** Preserve the absolute index and ancestor IDs of every unresolvable node,
 * even when other nodes are added/moved. Mirrors server's is_preserved guard. */
export function preservesAnchors(
  before: BuilderDocument, after: BuilderDocument,
  registry: Record<string, Record<string, string>>,
  deletedId?: string,
): boolean {
  const deleted = deletedId ? locate(before, deletedId) : undefined;
  const afterEntries = allNodes(after);
  const beforeEntries = allNodes(before);
  return beforeEntries.every(entry => {
    if (entry.kind !== 'element' || !isLockedElement(entry.node as BuilderElement, registry)) return true;
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

function clone(document: BuilderDocument): BuilderDocument {
  return JSON.parse(JSON.stringify(document)) as BuilderDocument;
}

function childrenAt(document: BuilderDocument, location: NodeLocation | undefined): BuilderNode[] | undefined {
  if (!location) return document.sections;
  if (location.kind === 'element') return undefined;
  const current = locate(document, location.node.id);
  return current && !isElement(current.node) ? current.node.children : undefined;
}

export function appendNode(
  document: BuilderDocument, parentId: string | null, kind: NodeKind, elementType: ElementType,
): { document: BuilderDocument; id: string } | null {
  const parent = locate(document, parentId);
  if ((kind === 'section' && parentId !== null)
      || (kind === 'row' && parent?.kind !== 'section')
      || (kind === 'column' && parent?.kind !== 'row')
      || (kind === 'element' && parent?.kind !== 'column')) return null;
  const next = clone(document);
  const children = childrenAt(next, parent);
  if (!children) return null;
  const id = newNodeId(kind, next);
  const defaults: Record<ElementType, Record<string, unknown>> = {
    'core/heading': { level: 2, text: 'Tiêu đề mới' },
    'core/text': { text: 'Nội dung mới' },
    'core/image': { media_id: '', alt: '' },
    'core/button': { label: 'Nút mới', url: '/' },
  };
  const node: BuilderNode = kind === 'element' ? { id, type: elementType, props: { ...defaults[elementType] } }
    : { id, children: [] };
  children.push(node);
  return { document: next, id };
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
  document: BuilderDocument, id: string, field: string, value: string | number,
): BuilderDocument | null {
  const next = clone(document);
  const location = locate(next, id);
  if (!location || !isElement(location.node)) return null;
  const element = location.node;
  if (element.type === 'legacy/block') {
    const values = element.props.values;
    if (!object(values)) return null;
    element.props = { ...element.props, values: { ...values, [field]: value } };
  } else {
    element.props = { ...element.props, [field]: value };
  }
  return next;
}
