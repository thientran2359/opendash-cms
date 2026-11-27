import { __ } from '../../i18n/index.tsx';
import { isUiOnlyFieldType } from '../../fields/helpers.ts';
import type { FieldSpec } from '../../fields/types.ts';
import type { BlockCatalogItemDto } from '../../types.ts';
import type { BlockSchemaV1 } from '../../types.ts';
import { type NodeMeta, type BuilderElement, type BuilderSection, type BuilderDocument, type BuilderNode, type NodeKind, type InsertableElementType, type NodeLocation, type StudioBuilderClipboard, allNodes, locate, isElement, newNodeId, carrierLocation, clone } from './types_and_tree.ts';
import { hasUnexpectedElementFields, hasInvalidCoreBinding, isLockedElement, isProtectedNode, preservesAnchors, withinBuilderLimits, declaredBlockKeys, pluginValuesReadyForFreshNode, distinctPatternIdCount } from './validation.ts';
import { object, countElementsByType, countDescendantElementsByType } from './mutations.ts';

export function childrenAt(document: BuilderDocument, location: NodeLocation | undefined): BuilderNode[] | undefined {
  if (!location) return document.sections;
  if (location.kind === 'element') return undefined;
  const current = locate(document, location.node.id);
  return current && !isElement(current.node) ? current.node.children : undefined;
}

/** Group fields hold nested objects `{ ... }`; UI-only fields (`tab`, `accordion`, `message`)
 * emit no keys; repeater items hold their own object values; responsive defaults start at desktop. */

export function initialBlockValues(fields: readonly FieldSpec[]): Record<string, unknown> {
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
    'core/heading': { level: 2, text: __("New heading") },
    'core/text': { text: __("New content") },
    'core/image': { media_id: '', alt: '' },
    'core/video': { media_id: '', controls: true, muted: false },
    'core/button': { label: __("New button"), url: '/' },
    'core/list': { type_key: 'post', include_descendants: false, sort: 'id_desc', page_size: 6 },
    'core/contact_form': { form_id: 1, submit_label: __("Send message") },
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
