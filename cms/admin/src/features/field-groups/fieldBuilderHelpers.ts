import type { FieldSpec, FieldType, SelectOption } from '../../fields/types.ts';
import type {
  FieldGroupDomain,
  FieldGroupPresentation,
  LocationRuleGroups,
} from '../content/types.ts';

export const DATA_FIELD_TYPES: ReadonlyArray<{ type: FieldType; label: string; group: string }> = [
  { type: 'text', label: 'Text', group: 'Basic' },
  { type: 'textarea', label: 'Text Area', group: 'Basic' },
  { type: 'number', label: 'Number', group: 'Basic' },
  { type: 'range', label: 'Range Slider', group: 'Basic' },
  { type: 'email', label: 'Email', group: 'Basic' },
  { type: 'url', label: 'URL', group: 'Basic' },
  { type: 'secret', label: 'Secret / Password', group: 'Basic' },
  { type: 'richtext', label: 'Rich Text Editor', group: 'Content' },
  { type: 'media', label: 'Media / Image / File', group: 'Content' },
  { type: 'gallery', label: 'Media Gallery', group: 'Content' },
  { type: 'link', label: 'Link (URL + Label)', group: 'Content' },
  { type: 'select', label: 'Select Dropdown', group: 'Choice' },
  { type: 'checkbox', label: 'Checkbox Group', group: 'Choice' },
  { type: 'radio_buttons', label: 'Radio Buttons', group: 'Choice' },
  { type: 'boolean', label: 'True / False Toggle', group: 'Choice' },
  { type: 'color', label: 'Color Picker', group: 'Visual' },
  { type: 'icon', label: 'Icon Picker', group: 'Visual' },
  { type: 'spacing', label: 'Spacing Box', group: 'Visual' },
  { type: 'date', label: 'Date', group: 'DateTime' },
  { type: 'datetime', label: 'Date & Time', group: 'DateTime' },
  { type: 'time', label: 'Time', group: 'DateTime' },
  { type: 'entry_ref', label: 'Entry Reference', group: 'Relational' },
  { type: 'term_ref', label: 'Taxonomy Term Reference', group: 'Relational' },
  { type: 'group', label: 'Field Group (Nested)', group: 'Structural' },
  { type: 'repeater', label: 'Repeater (Rows)', group: 'Structural' },
  { type: 'tab', label: 'Layout Tab', group: 'Layout' },
  { type: 'accordion', label: 'Layout Accordion', group: 'Layout' },
  { type: 'message', label: 'Info Message', group: 'Layout' },
];

export const DEFAULT_PRESENTATION: FieldGroupPresentation = {
  region: 'main_bottom',
  order: 50,
  collapsible: true,
  collapsed_by_default: false,
  style: 'card',
  label_placement: 'top',
};

export function snakeKeyFromLabel(raw: string): string {
  const clean = raw
    .toLowerCase()
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 64);
  return clean && /^[a-z]/.test(clean) ? clean : `field_${clean || 'item'}`.slice(0, 64);
}

export function uniqueFieldKey(baseKey: string, siblings: readonly FieldSpec[]): string {
  const existing = new Set(siblings.map(f => f.key));
  if (!existing.has(baseKey)) return baseKey;
  let idx = 2;
  while (existing.has(`${baseKey}_${idx}`)) idx += 1;
  return `${baseKey}_${idx}`.slice(0, 64);
}

export function createBlankFieldSpec(
  siblings: readonly FieldSpec[],
  adminOnlyLocation = false,
  type: FieldType = 'text',
): FieldSpec {
  const key = uniqueFieldKey(type === 'tab' || type === 'accordion' || type === 'message' ? `ui_${type}` : 'new_field', siblings);
  return {
    type,
    key,
    label: 'New Field',
    required: false,
    width: 100,
    ...(type === 'select' || type === 'checkbox' || type === 'radio_buttons'
      ? { options: [{ value: 'option_1', label: 'Option 1' }] }
      : {}),
    ...(type === 'entry_ref' ? { target_types: ['post'] } : {}),
    ...(type === 'term_ref' ? { taxonomy: 'category' } : {}),
    ...(type === 'range' ? { min: 0, max: 100, step: 1 } : {}),
    ...(type === 'group' || type === 'repeater' ? { fields: [] } : {}),
    acl: {
      min_view_role: adminOnlyLocation ? 'admin' : 'editor',
      min_edit_role: adminOnlyLocation ? 'admin' : 'editor',
      public_exposure: adminOnlyLocation ? 'private' : 'public',
    },
  };
}

export function serializeSelectOptions(options?: readonly SelectOption[]): string {
  return (options ?? [])
    .map(opt => (opt.icon ? `${opt.value}|${opt.label}|${opt.icon}` : `${opt.value}|${opt.label}`))
    .join('\n');
}

export function parseSelectOptions(raw: string): SelectOption[] {
  const out: SelectOption[] = [];
  for (const line of raw.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const parts = trimmed.split('|');
    const value = (parts[0] ?? '').trim();
    const label = (parts[1] ?? value).trim();
    const icon = (parts[2] ?? '').trim();
    if (!value) continue;
    out.push(icon ? { value, label, icon } : { value, label });
  }
  return out;
}

export function inferDomainFromLocation(groups: LocationRuleGroups): FieldGroupDomain {
  for (const branch of groups) {
    for (const rule of branch) {
      if (rule.param === 'taxonomy_term') return 'term';
      if (rule.param === 'options_page') return 'options_page';
    }
  }
  return 'content';
}

export function locationHasEntryTaxonomy(groups: LocationRuleGroups): boolean {
  return groups.some(branch => branch.some(rule => rule.param === 'entry_taxonomy'));
}

export function isLocationRestrictedToAdminOnly(groups: LocationRuleGroups): boolean {
  if (groups.length === 0) return false;
  return groups.every(branch =>
    branch.some(rule => rule.param === 'user_role' && rule.op === 'eq' && rule.value === 'admin') ||
    branch.some(rule => rule.param === 'user_role' && rule.op === 'neq' && rule.value === 'editor'),
  );
}

export function hasAnyPublicFieldRecursive(fields: readonly FieldSpec[]): boolean {
  return fields.some(spec => {
    if (spec.type === 'tab' || spec.type === 'accordion' || spec.type === 'message') {
      return false;
    }
    if (spec.public || spec.acl?.public_exposure !== 'private') return true;
    return spec.fields ? hasAnyPublicFieldRecursive(spec.fields) : false;
  });
}

export function countDataFields(fields: readonly FieldSpec[]): number {
  return fields.filter(f => f.type !== 'tab' && f.type !== 'accordion' && f.type !== 'message').length;
}

export function moveArrayItem<T>(list: readonly T[], fromIdx: number, toIdx: number): T[] {
  if (fromIdx < 0 || fromIdx >= list.length || toIdx < 0 || toIdx >= list.length || fromIdx === toIdx) {
    return [...list];
  }
  const next = [...list];
  const [moved] = next.splice(fromIdx, 1);
  next.splice(toIdx, 0, moved);
  return next;
}
