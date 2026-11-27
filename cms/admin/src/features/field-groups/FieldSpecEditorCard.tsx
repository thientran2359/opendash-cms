import React, { useState } from 'react';
import type {
  FieldAcl,
  FieldSpec,
  FieldType,
  MessageTone,
  RichTextToolbar,
  SelectOptionsSource,
  TermPickerStyle,
} from '../../fields/types.ts';
import type { FieldGroupDomain, LocationRuleGroups } from '../content/types.ts';
import { __, useI18n } from '../../i18n/index.tsx';
import { FieldConditionalLogicEditor } from './FieldConditionalLogicEditor.tsx';
import {
  createBlankFieldSpec,
  DATA_FIELD_TYPES,
  locationHasEntryTaxonomy,
  moveArrayItem,
  parseSelectOptions,
  serializeSelectOptions,
  snakeKeyFromLabel,
} from './fieldBuilderHelpers.ts';

type FieldTab = 'general' | 'validation' | 'presentation' | 'conditions';

const INPUT_CLASS =
  'h-8 w-full min-w-0 rounded border border-zinc-300 bg-white px-2.5 text-xs text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50';

interface FieldSpecEditorCardProps {
  field: FieldSpec;
  index: number;
  total: number;
  siblings: readonly FieldSpec[];
  rootFields: readonly FieldSpec[];
  domain: FieldGroupDomain;
  location: LocationRuleGroups;
  adminOnlyLocation: boolean;
  depth?: number;
  disabled?: boolean;
  onChange: (next: FieldSpec) => void;
  onMove: (delta: -1 | 1) => void;
  onDuplicate: () => void;
  onRemove: () => void;
  onDragStart?: () => void;
  onDragOver?: (e: React.DragEvent) => void;
  onDrop?: (e: React.DragEvent) => void;
}

export const FieldSpecEditorCard: React.FC<FieldSpecEditorCardProps> = ({
  field,
  index,
  total,
  siblings,
  rootFields,
  domain,
  location,
  adminOnlyLocation,
  depth = 0,
  disabled = false,
  onChange,
  onMove,
  onDuplicate,
  onRemove,
  onDragStart,
  onDragOver,
  onDrop,
}) => {
  useI18n();
  const [expanded, setExpanded] = useState(index === 0 && depth === 0);
  const [tab, setTab] = useState<FieldTab>('general');
  const [keyManuallyEdited, setKeyManuallyEdited] = useState(Boolean(field.key && field.key !== 'new_field'));
  const [optionsText, setOptionsText] = useState(() => serializeSelectOptions(field.options));
  const [liveStatus, setLiveStatus] = useState('');

  const isUiOnly = field.type === 'tab' || field.type === 'accordion' || field.type === 'message';
  const hasOptions = field.type === 'select' || field.type === 'checkbox' || field.type === 'radio_buttons';
  const isContainer = field.type === 'group' || field.type === 'repeater';
  const canSyncEntryTerms =
    domain === 'content' && depth === 0 && field.type === 'term_ref' && !locationHasEntryTaxonomy(location);

  const update = (patch: Partial<FieldSpec>) => onChange({ ...field, ...patch });
  const updateAcl = (patch: Partial<FieldAcl>) =>
    update({ acl: { min_view_role: 'editor', min_edit_role: 'editor', public_exposure: 'public', ...field.acl, ...patch } });

  const subfields = field.fields ?? [];
  const candidateConditions = Array.from(
    new Map([...rootFields, ...siblings].filter(s => s.key !== field.key).map(s => [s.key, s])).values(),
  );

  return (
    <article
      draggable={Boolean(!disabled && onDragStart)}
      onDragStart={onDragStart}
      onDragOver={onDragOver}
      onDrop={onDrop}
      data-testid={`field-builder-card-${field.key}`}
      className="rounded-lg border border-zinc-200 bg-white shadow-2xs"
    >
      <div className="flex flex-wrap items-center justify-between gap-2 px-3 py-2.5">
        <div className="flex min-w-0 items-center gap-2">
          {onDragStart && <span className="cursor-grab text-zinc-400 select-none" aria-hidden="true">⋮⋮</span>}
          <button
            type="button"
            aria-expanded={expanded}
            onClick={() => setExpanded(v => !v)}
            className="flex min-w-0 items-center gap-2 text-left text-xs font-semibold text-zinc-900 hover:text-od-accent"
          >
            <span>{field.label || __("Untitled field")}</span>
            <code className="rounded bg-zinc-100 px-1.5 py-0.5 font-mono text-[11px] font-normal text-zinc-600">{field.key}</code>
            <span className="rounded bg-sky-50 px-1.5 py-0.5 text-[11px] font-medium text-sky-800">{field.type}</span>
            {field.required && <span className="text-rose-600">*</span>}
          </button>
        </div>

        <div className="flex flex-wrap items-center gap-1">
          <button type="button" disabled={disabled || index === 0}
            aria-label={__("Move field {{value1}} up", { value1: field.label })}
            onClick={() => onMove(-1)}
            className="h-7 w-7 rounded border border-zinc-300 bg-white text-xs text-zinc-700 hover:bg-zinc-50 disabled:opacity-40">↑</button>
          <button type="button" disabled={disabled || index === total - 1}
            aria-label={__("Move field {{value1}} down", { value1: field.label })}
            onClick={() => onMove(1)}
            className="h-7 w-7 rounded border border-zinc-300 bg-white text-xs text-zinc-700 hover:bg-zinc-50 disabled:opacity-40">↓</button>
          <button type="button" disabled={disabled} onClick={onDuplicate}
            className="h-7 rounded border border-zinc-300 bg-white px-2 text-[11px] font-medium text-zinc-700 hover:bg-zinc-50">{__("Duplicate")}</button>
          <button type="button" disabled={disabled} onClick={onRemove}
            className="h-7 rounded border border-rose-200 bg-white px-2 text-[11px] font-medium text-rose-600 hover:bg-rose-50">{__("Delete")}</button>
        </div>
      </div>

      <span className="sr-only" aria-live="polite">{liveStatus}</span>

      {expanded && (
        <div className="space-y-3 border-t border-zinc-100 bg-zinc-50/40 p-3">
          <div role="tablist" aria-label={__("Field setting sections")} className="flex flex-wrap gap-1.5 border-b border-zinc-200 pb-2">
            {([
              ['general', __("General")],
              ['validation', __("Validation")],
              ['presentation', __("Presentation & Access")],
              ['conditions', __("Conditional Logic")],
            ] as const).map(([id, label]) => (
              <button
                key={id}
                type="button"
                role="tab"
                aria-selected={tab === id}
                onClick={() => setTab(id)}
                className={
                  'h-7 rounded px-2.5 text-xs font-medium ' +
                  (tab === id ? 'bg-od-accent text-white' : 'border border-zinc-300 bg-white text-zinc-700 hover:bg-zinc-50')
                }
              >
                {label}
              </button>
            ))}
          </div>

          {tab === 'general' && (
            <div className="space-y-3">
              <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-3">
                <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                  <span>{__("Field label *")}</span>
                  <input type="text" className={INPUT_CLASS} value={field.label} disabled={disabled} maxLength={100}
                    onChange={e => {
                      const label = e.target.value;
                      if (!keyManuallyEdited && label.trim()) {
                        update({ label, key: snakeKeyFromLabel(label) });
                      } else {
                        update({ label });
                      }
                    }} />
                </label>
                <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                  <span>{__("Field key *")}</span>
                  <input type="text" className={`${INPUT_CLASS} font-mono`} value={field.key} disabled={disabled} maxLength={64}
                    onChange={e => { setKeyManuallyEdited(true); update({ key: e.target.value }); }} />
                </label>
                <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                  <span>{__("Field type")}</span>
                  <select className={INPUT_CLASS} value={field.type} disabled={disabled}
                    onChange={e => {
                      const nextType = e.target.value as FieldType;
                      const fresh = createBlankFieldSpec(siblings, adminOnlyLocation, nextType, depth === 0 ? domain : 'content');
                      const nextUiOnly = nextType === 'tab' || nextType === 'accordion' || nextType === 'message';
                      onChange({
                        ...fresh,
                        key: field.key,
                        label: field.label,
                        width: field.width,
                        ...(nextUiOnly
                          ? {}
                          : {
                              acl: field.acl ?? fresh.acl,
                              ...(domain === 'options_page' && depth === 0
                                ? { public: field.public ?? fresh.public ?? false }
                                : {}),
                            }),
                      });
                      if (nextType === 'select' || nextType === 'checkbox' || nextType === 'radio_buttons') {
                        setOptionsText(serializeSelectOptions(fresh.options));
                      }
                    }}>
                    {DATA_FIELD_TYPES.map(ft => (
                      <option key={ft.type} value={ft.type}>{ft.group}: {ft.label} ({ft.type})</option>
                    ))}
                  </select>
                </label>
              </div>

              {!isUiOnly && (
                <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-3">
                  <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                    <span>{__("Instructions")}</span>
                    <input type="text" className={INPUT_CLASS} value={field.instructions ?? ''} disabled={disabled} maxLength={300}
                      onChange={e => update({ instructions: e.target.value || undefined })} />
                  </label>
                  <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                    <span>{__("Placeholder")}</span>
                    <input type="text" className={INPUT_CLASS} value={field.placeholder ?? ''} disabled={disabled} maxLength={120}
                      onChange={e => update({ placeholder: e.target.value || undefined })} />
                  </label>
                  <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                    <span>{__("Default value (string or JSON)")}</span>
                    <input type="text" className={`${INPUT_CLASS} font-mono`}
                      value={field.default === undefined ? '' : typeof field.default === 'string' ? field.default : JSON.stringify(field.default)}
                      disabled={disabled}
                      onChange={e => {
                        const raw = e.target.value;
                        if (raw === '') { update({ default: undefined }); return; }
                        if (field.type === 'boolean') { update({ default: raw === 'true' }); return; }
                        if (field.type === 'number' || field.type === 'range') {
                          const num = Number(raw);
                          update({ default: Number.isFinite(num) ? num : raw });
                          return;
                        }
                        try {
                          if ((raw.startsWith('{') && raw.endsWith('}')) || (raw.startsWith('[') && raw.endsWith(']'))) {
                            update({ default: JSON.parse(raw) });
                            return;
                          }
                        } catch {}
                        update({ default: raw });
                      }} />
                  </label>
                </div>
              )}

              {hasOptions && (
                <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
                  <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                    <span>{__("Choices (value|Label or value|Label|icon per line)")}</span>
                    <textarea rows={3} className="w-full rounded border border-zinc-300 bg-white p-2 font-mono text-xs"
                      value={optionsText} disabled={disabled}
                      onChange={e => {
                        setOptionsText(e.target.value);
                        update({ options: parseSelectOptions(e.target.value) });
                      }} />
                  </label>
                  <div className="space-y-2">
                    <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                      <span>{__("Dynamic options source")}</span>
                      <select className={INPUT_CLASS} value={field.options_source ?? ''} disabled={disabled}
                        onChange={e => update({ options_source: (e.target.value || undefined) as SelectOptionsSource | undefined })}>
                        <option value="">{__("Static choices above")}</option>
                        {(['menus', 'menu_slots', 'contact_forms', 'content_types', 'taxonomies', 'template_parts', 'patterns'] as const).map(src => (
                          <option key={src} value={src}>{src}</option>
                        ))}
                      </select>
                    </label>
                    {field.type === 'select' && (
                      <div className="flex flex-wrap gap-3 text-xs text-zinc-700">
                        <label className="inline-flex items-center gap-1.5">
                          <input type="checkbox" checked={Boolean(field.multiple)} disabled={disabled}
                            onChange={e => update({ multiple: e.target.checked })} />
                          <span>{__("Multiple select")}</span>
                        </label>
                        <label className="inline-flex items-center gap-1.5">
                          <input type="checkbox" checked={Boolean(field.allow_null)} disabled={disabled}
                            onChange={e => update({ allow_null: e.target.checked })} />
                          <span>{__("Allow empty option")}</span>
                        </label>
                      </div>
                    )}
                  </div>
                </div>
              )}

              {field.type === 'richtext' && (
                <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                  <span>{__("Rich text toolbar")}</span>
                  <select className={INPUT_CLASS} value={field.toolbar ?? 'standard'} disabled={disabled}
                    onChange={e => update({ toolbar: e.target.value as RichTextToolbar })}>
                    <option value="standard">{__("Standard")}</option>
                    <option value="minimal">{__("Minimal")}</option>
                  </select>
                </label>
              )}

              {field.type === 'entry_ref' && (
                <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
                  <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                    <span>{__("Target content types (comma-separated)")}</span>
                    <input type="text" className={`${INPUT_CLASS} font-mono`} value={(field.target_types ?? []).join(', ')} disabled={disabled}
                      onChange={e => update({ target_types: e.target.value.split(',').map(v => v.trim()).filter(Boolean) })} />
                  </label>
                  <label className="flex items-center gap-2 self-end pb-1 text-xs text-zinc-700">
                    <input type="checkbox" checked={Boolean(field.multiple)} disabled={disabled}
                      onChange={e => update({ multiple: e.target.checked })} />
                    <span>{__("Allow multiple entries")}</span>
                  </label>
                </div>
              )}

              {field.type === 'term_ref' && (
                <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-4">
                  <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                    <span>{__("Taxonomy key")}</span>
                    <input type="text" className={`${INPUT_CLASS} font-mono`} value={field.taxonomy ?? 'category'} disabled={disabled}
                      onChange={e => update({ taxonomy: e.target.value.trim() || undefined })} />
                  </label>
                  <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                    <span>{__("Picker UI style")}</span>
                    <select className={INPUT_CLASS} value={field.picker_style ?? 'auto'} disabled={disabled}
                      onChange={e => update({ picker_style: e.target.value as TermPickerStyle })}>
                      <option value="auto">{__("Auto (follows taxonomy)")}</option>
                      <option value="checklist">{__("Checklist")}</option>
                      <option value="tags">{__("Tag input")}</option>
                      <option value="select">{__("Dropdown select")}</option>
                      <option value="radio">{__("Radio buttons")}</option>
                    </select>
                  </label>
                  <label className="flex items-center gap-1.5 self-end pb-1 text-xs text-zinc-700">
                    <input type="checkbox" checked={Boolean(field.multiple)} disabled={disabled}
                      onChange={e => update({ multiple: e.target.checked })} />
                    <span>{__("Multiple terms")}</span>
                  </label>
                  <label className="flex items-center gap-1.5 self-end pb-1 text-xs text-zinc-700">
                    <input type="checkbox" checked={Boolean(field.sync_entry_terms)}
                      disabled={disabled || !canSyncEntryTerms}
                      onChange={e => update({ sync_entry_terms: e.target.checked })} />
                    <span>{__("Sync to entry taxonomy")}</span>
                  </label>
                </div>
              )}

              {field.type === 'boolean' && (
                <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
                  <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                    <span>{__("True label")}</span>
                    <input type="text" className={INPUT_CLASS} value={field.on_label ?? ''} disabled={disabled}
                      onChange={e => update({ on_label: e.target.value || undefined })} />
                  </label>
                  <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                    <span>{__("False label")}</span>
                    <input type="text" className={INPUT_CLASS} value={field.off_label ?? ''} disabled={disabled}
                      onChange={e => update({ off_label: e.target.value || undefined })} />
                  </label>
                </div>
              )}

              {field.type === 'message' && (
                <div className="grid grid-cols-1 gap-2 sm:grid-cols-[140px_1fr]">
                  <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                    <span>{__("Tone")}</span>
                    <select className={INPUT_CLASS} value={field.tone ?? 'info'} disabled={disabled}
                      onChange={e => update({ tone: e.target.value as MessageTone })}>
                      <option value="info">info</option>
                      <option value="warning">warning</option>
                      <option value="neutral">neutral</option>
                    </select>
                  </label>
                  <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                    <span>{__("Message text")}</span>
                    <input type="text" className={INPUT_CLASS} value={field.body ?? ''} disabled={disabled}
                      onChange={e => update({ body: e.target.value || undefined })} />
                  </label>
                </div>
              )}

              {isContainer && depth < 4 && (
                <div className="space-y-2 rounded border border-zinc-200 bg-white p-2.5">
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-xs font-semibold text-zinc-800">
                      {__("Nested sub-fields")} ({subfields.length})
                    </span>
                    <button
                      type="button"
                      disabled={disabled}
                      onClick={() => {
                        const child = createBlankFieldSpec(subfields, adminOnlyLocation);
                        update({ fields: [...subfields, child] });
                        setLiveStatus(__("Added sub-field."));
                      }}
                      className="rounded border border-zinc-300 bg-zinc-50 px-2 py-1 text-xs font-medium text-zinc-700 hover:bg-zinc-100"
                    >
                      {__("+ Add sub-field")}
                    </button>
                  </div>
                  <div className="space-y-2">
                    {subfields.map((sub, subIdx) => (
                      <FieldSpecEditorCard
                        key={`subfield-${subIdx}`}
                        field={sub}
                        index={subIdx}
                        total={subfields.length}
                        siblings={subfields}
                        rootFields={rootFields}
                        domain={domain}
                        location={location}
                        adminOnlyLocation={adminOnlyLocation}
                        depth={depth + 1}
                        disabled={disabled}
                        onChange={nextSub => update({ fields: subfields.map((s, i) => (i === subIdx ? nextSub : s)) })}
                        onMove={d => update({ fields: moveArrayItem(subfields, subIdx, subIdx + d) })}
                        onDuplicate={() => {
                          const clone = structuredClone(sub);
                          clone.key = `${sub.key}_copy`.slice(0, 64);
                          update({ fields: [...subfields, clone] });
                        }}
                        onRemove={() => update({ fields: subfields.filter((_, i) => i !== subIdx) })}
                      />
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}

          {tab === 'validation' && !isUiOnly && (
            <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-4">
              <label className="flex items-center gap-2 text-xs font-medium text-zinc-800 sm:col-span-4">
                <input type="checkbox" checked={Boolean(field.required)} disabled={disabled}
                  onChange={e => update({ required: e.target.checked })} />
                <span>{__("Required")}</span>
              </label>
              <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                <span>{__("Min chars")}</span>
                <input type="number" className={INPUT_CLASS} value={field.min_chars ?? ''} disabled={disabled}
                  onChange={e => update({ min_chars: e.target.value ? Number(e.target.value) : undefined })} />
              </label>
              <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                <span>{__("Max chars")}</span>
                <input type="number" className={INPUT_CLASS} value={field.max_chars ?? ''} disabled={disabled}
                  onChange={e => update({ max_chars: e.target.value ? Number(e.target.value) : undefined })} />
              </label>
              <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                <span>{__("Min value")}</span>
                <input type="number" className={INPUT_CLASS} value={field.min ?? ''} disabled={disabled}
                  onChange={e => update({ min: e.target.value ? Number(e.target.value) : undefined })} />
              </label>
              <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                <span>{__("Max value")}</span>
                <input type="number" className={INPUT_CLASS} value={field.max ?? ''} disabled={disabled}
                  onChange={e => update({ max: e.target.value ? Number(e.target.value) : undefined })} />
              </label>
            </div>
          )}

          {tab === 'validation' && isUiOnly && (
            <p className="text-xs text-zinc-500">{__("UI layout elements do not store data or run validation.")}</p>
          )}

          {tab === 'presentation' && (
            <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-3">
              <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                <span>{__("Grid width (%)")}</span>
                <select className={INPUT_CLASS} value={field.width ?? 100} disabled={disabled}
                  onChange={e => update({ width: Number(e.target.value) as FieldSpec['width'] })}>
                  {[100, 75, 66, 50, 33, 25].map(w => <option key={w} value={w}>{w}%</option>)}
                </select>
              </label>
              {!isUiOnly && (
                <>
                  <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                    <span>{__("CSS custom var")}</span>
                    <input type="text" className={`${INPUT_CLASS} font-mono`} value={field.css_var ?? ''} disabled={disabled}
                      onChange={e => update({ css_var: e.target.value || undefined })} />
                  </label>
                  <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                    <span>{__("Role access (View / Edit)")}</span>
                    <select className={INPUT_CLASS}
                      value={`${field.acl?.min_view_role ?? 'editor'}:${field.acl?.min_edit_role ?? 'editor'}`}
                      disabled={disabled}
                      onChange={e => {
                        const [v, ed] = e.target.value.split(':') as ['editor' | 'admin', 'editor' | 'admin'];
                        if (v === 'admin') {
                          update({
                            ...(domain === 'options_page' && depth === 0 ? { public: false } : {}),
                            acl: { min_view_role: v, min_edit_role: ed, public_exposure: 'private' },
                          });
                        } else {
                          updateAcl({ min_view_role: v, min_edit_role: ed });
                        }
                      }}>
                      <option value="editor:editor">{__("Editor views & edits")}</option>
                      <option value="editor:admin">{__("Editor views, Admin edits")}</option>
                      <option value="admin:admin">{__("Admin only")}</option>
                    </select>
                  </label>
                  <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                    <span>{__("Public hydration exposure")}</span>
                    <select className={INPUT_CLASS}
                      value={
                        domain === 'options_page' && depth === 0
                          ? (field.public ? 'public' : 'private')
                          : (field.acl?.public_exposure ?? 'public')
                      }
                      disabled={disabled || adminOnlyLocation}
                      onChange={e => {
                        const exp = e.target.value as 'public' | 'private';
                        if (domain === 'options_page' && depth === 0) {
                          update({
                            public: exp === 'public',
                            acl: {
                              min_view_role: exp === 'public' ? 'editor' : (field.acl?.min_view_role ?? 'editor'),
                              min_edit_role: field.acl?.min_edit_role ?? 'editor',
                              public_exposure: exp,
                            },
                          });
                        } else {
                          updateAcl({ public_exposure: exp });
                        }
                      }}>
                      <option value="public">{__("Public (rendered in themes)")}</option>
                      <option value="private">{__("Private (Admin/Editor only)")}</option>
                    </select>
                  </label>
                  <div className="flex flex-wrap items-center gap-3 self-end pb-1 text-xs text-zinc-700 sm:col-span-2">
                    <label className="inline-flex items-center gap-1.5">
                      <input type="checkbox" checked={Boolean(field.show_in_list)}
                        disabled={disabled || domain !== 'content' || depth > 0}
                        onChange={e => update({ show_in_list: e.target.checked })} />
                      <span>{__("Show in list column")}</span>
                    </label>
                    <label className="inline-flex items-center gap-1.5">
                      <input type="checkbox" checked={Boolean(field.responsive)} disabled={disabled}
                        onChange={e => update({ responsive: e.target.checked })} />
                      <span>{__("Responsive")}</span>
                    </label>
                    <label className="inline-flex items-center gap-1.5">
                      <input type="checkbox" checked={Boolean(field.translatable)}
                        disabled={disabled || domain !== 'options_page' || depth > 0}
                        onChange={e => update({ translatable: e.target.checked })} />
                      <span>{__("Translatable (multilingual)")}</span>
                    </label>
                  </div>
                </>
              )}
            </div>
          )}

          {tab === 'conditions' && (
            <FieldConditionalLogicEditor
              conditions={field.conditions}
              candidateFields={candidateConditions}
              disabled={disabled}
              onChange={next => update({ conditions: next })}
            />
          )}
        </div>
      )}
    </article>
  );
};
