import type { BlockPropValue } from '../../types.ts';
import { type NodeVisibility, type NodeMeta, type BuilderElement, type ElementBinding, type ListProps, type BuilderDocument, type ElementType, ELEMENT_TYPES, allNodes, locate, isElement, isSyntheticNode, carrierLocation, clone } from './types_and_tree.ts';
import { validListTaxonomyFilter } from './validation.ts';
import { childrenAt, appendNode } from './clipboard_and_insert.ts';

/** Homepage AST v1. The server validates every Save; this client preserves
 * unknown JSON properties so the Editor never strips historical/bound data. */

export function object(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

/** Match the fields accepted by Rust's KnownElement / Props
 * #[serde(deny_unknown_fields)]. A historic element carrying an extra field
 * is Unresolved on the server and must remain an immutable anchor. */

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
