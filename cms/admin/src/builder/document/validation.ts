import { evaluateConditions } from '../../fields/conditions.ts';
import { isUiOnlyFieldType } from '../../fields/helpers.ts';
import type { FieldSpec } from '../../fields/types.ts';
import type { BlockCatalogItemDto } from '../../types.ts';
import type { BlockSchemaV1 } from '../../types.ts';
import { type BuilderElement, type BuilderDocument, allNodes, locate } from './types_and_tree.ts';
import { object } from './mutations.ts';

export const ELEMENT_PROP_KEYS: Record<string, readonly string[]> = {
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


export function hasUnexpectedElementFields(element: BuilderElement): boolean {
  const allowed = ELEMENT_PROP_KEYS[element.type];
  return !allowed
    || Object.keys(element).some(key => key !== 'id' && key !== 'type' && key !== 'props' && key !== 'meta')
    || !object(element.props)
    || Object.keys(element.props).some(key => !allowed.includes(key));
}


export function hasInvalidCoreBinding(element: BuilderElement): boolean {
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

export function validListTaxonomyFilter(props: {
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


export function withinBuilderLimits(document: BuilderDocument): boolean {
  return allNodes(document).length <= 512
    && new TextEncoder().encode(JSON.stringify(document)).byteLength <= 256 * 1024;
}

/** Duplicate an editable subtree without moving any immutable anchor. */

export function declaredBlockKeys(fields: readonly FieldSpec[]): Set<string> {
  const keys = new Set<string>();
  for (const field of fields) {
    if (!isUiOnlyFieldType(field.type)) keys.add(field.key);
  }
  return keys;
}

/** Check the constraints that require previous_values=None in the Rust
 * BlockSchemaV1 validator: newly required fields, visible DNF conditions,
 * nested groups, responsive required values, and repeater min_items/subfields. */

export function pluginValuesReadyForFreshNode(
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
        if (typeof item === 'string' && /^\d+$/.test(item.trim())) {
          const parsed = Number(item.trim());
          if (parsed > 0) out.add(parsed);
        }
      }
    }
  }
}


export function distinctPatternIdCount(
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
