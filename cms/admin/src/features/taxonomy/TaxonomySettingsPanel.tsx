import React from 'react';
import type { OwnedContentTypeWithSettings } from '../../api/content.ts';
import type { TaxonomySelectionMode, TaxonomyTerm } from '../../api/taxonomy.ts';
import type { TermPickerStyle } from '../../fields/types.ts';
import { __ } from '../../i18n/index.tsx';
import type { FormValues } from './taxonomyDefinitionsHelpers.ts';

export interface TaxonomySettingsPanelProps {
  isNew: boolean;
  form: FormValues;
  allContentTypes: OwnedContentTypeWithSettings[];
  termsForDefaultSelect: TaxonomyTerm[];
  disabled: boolean;
  onPatch: (patch: Partial<FormValues>) => void;
}

const INPUT_CLASS =
  'mt-1 h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50';
const CHECK_CLASS = 'mt-0.5 h-4 w-4 rounded border-zinc-300 accent-sky-600';

export const TaxonomySettingsPanel: React.FC<TaxonomySettingsPanelProps> = ({
  isNew,
  form,
  allContentTypes,
  termsForDefaultSelect,
  disabled,
  onPatch,
}) => {
  const attachableCpts = allContentTypes.filter(
    ct => ct.descriptor.key !== 'section' && ct.descriptor.key !== 'pattern',
  );

  const toggleCpt = (typeKey: string, checked: boolean) => {
    const next = checked
      ? [...new Set([...form.content_types, typeKey])]
      : form.content_types.filter(k => k !== typeKey);
    onPatch({ content_types: next });
  };

  return (
    <div className="space-y-4 border-t border-zinc-200 pt-4" data-testid="taxonomy-settings-panel">
      <div className="grid gap-3 sm:grid-cols-2">
        <label htmlFor="tax-singular-label" className="block text-xs font-medium text-zinc-700">
          <span>{__("Singular label")}</span>
          <input
            id="tax-singular-label"
            type="text"
            className={INPUT_CLASS}
            maxLength={100}
            disabled={disabled}
            value={form.singular_label}
            placeholder={__("Optional singular name")}
            onChange={e => onPatch({ singular_label: e.target.value })}
          />
        </label>

        <label htmlFor="tax-rewrite-base" className="block text-xs font-medium text-zinc-700">
          <span>{__("URL rewrite base")}</span>
          <input
            id="tax-rewrite-base"
            type="text"
            className={`${INPUT_CLASS} font-mono text-xs`}
            maxLength={64}
            pattern="[a-z][a-z0-9-]*"
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
            disabled={disabled}
            value={form.rewrite_base}
            placeholder={form.key || 'category'}
            aria-describedby="tax-rewrite-base-help"
            onChange={e => onPatch({ rewrite_base: e.target.value })}
          />
          <span id="tax-rewrite-base-help" className="mt-1 block text-[11px] font-normal text-zinc-500">
            {__("Lowercase kebab-case archive prefix. Leave empty to use the taxonomy key.")}
          </span>
        </label>
      </div>

      <label htmlFor="tax-description" className="block text-xs font-medium text-zinc-700">
        <span>{__("Description")}</span>
        <textarea
          id="tax-description"
          rows={2}
          maxLength={400}
          disabled={disabled}
          className="mt-1 w-full rounded-md border border-zinc-300 bg-white px-2.5 py-1.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50"
          value={form.description}
          onChange={e => onPatch({ description: e.target.value })}
        />
      </label>

      <fieldset className="space-y-3 rounded-lg border border-zinc-200 bg-zinc-50/50 p-3">
        <legend className="px-1 text-xs font-semibold text-zinc-800">{__("Editor & picker behavior")}</legend>
        <div className="grid gap-3 sm:grid-cols-2">
          <label htmlFor="tax-selection-mode" className="block text-xs font-medium text-zinc-700">
            <span>{__("Selection mode")}</span>
            <select
              id="tax-selection-mode"
              className={INPUT_CLASS}
              disabled={disabled}
              value={form.selection_mode}
              onChange={e => onPatch({ selection_mode: e.target.value as TaxonomySelectionMode })}
            >
              <option value="multiple">{__("Multiple terms")}</option>
              <option value="single">{__("Single term only")}</option>
            </select>
          </label>

          <label htmlFor="tax-picker-style" className="block text-xs font-medium text-zinc-700">
            <span>{__("Picker style")}</span>
            <select
              id="tax-picker-style"
              className={INPUT_CLASS}
              disabled={disabled}
              value={form.picker_style}
              onChange={e => onPatch({ picker_style: e.target.value as TermPickerStyle })}
            >
              <option value="auto">{__("Auto (by hierarchy)")}</option>
              <option value="checklist">{__("Checklist")}</option>
              <option value="tags">{__("Tags chips")}</option>
              <option value="select">{__("Dropdown select")}</option>
              <option value="radio">{__("Radio buttons")}</option>
            </select>
          </label>
        </div>

        {!isNew && (
          <label htmlFor="tax-default-term" className="block text-xs font-medium text-zinc-700">
            <span>{__("Default fallback term on publish")}</span>
            <select
              id="tax-default-term"
              className={INPUT_CLASS}
              disabled={disabled}
              value={form.default_term_id ? String(form.default_term_id) : ''}
              onChange={e =>
                onPatch({
                  default_term_id: e.target.value ? Number(e.target.value) : null,
                })
              }
            >
              <option value="">{__("None (no automatic default)")}</option>
              {form.default_term_id &&
                !termsForDefaultSelect.some(t => t.id === form.default_term_id) && (
                  <option value={String(form.default_term_id)}>
                    {__("Term #{{value1}} (not loaded)", { value1: form.default_term_id })}
                  </option>
                )}
              {termsForDefaultSelect.map(term => (
                <option key={term.id} value={String(term.id)}>
                  {term.name} (/{term.slug} · #{term.id})
                </option>
              ))}
            </select>
          </label>
        )}

        <div className="grid gap-2 sm:grid-cols-2">
          <label className="flex items-start gap-2 text-xs text-zinc-800">
            <input
              type="checkbox"
              className={CHECK_CLASS}
              checked={form.required_on_publish}
              disabled={disabled}
              onChange={e => onPatch({ required_on_publish: e.target.checked })}
            />
            <span>{__("Require at least one term when publishing")}</span>
          </label>

          <label className="flex items-start gap-2 text-xs text-zinc-800">
            <input
              type="checkbox"
              className={CHECK_CLASS}
              checked={form.editor_can_create_terms}
              disabled={disabled}
              onChange={e => onPatch({ editor_can_create_terms: e.target.checked })}
            />
            <span>{__("Allow Editors to create new terms")}</span>
          </label>

          <label className="flex items-start gap-2 text-xs text-zinc-800">
            <input
              type="checkbox"
              className={CHECK_CLASS}
              checked={form.show_in_menu}
              disabled={disabled}
              onChange={e => onPatch({ show_in_menu: e.target.checked })}
            />
            <span>{__("Show taxonomy submenu in Admin sidebar")}</span>
          </label>

          <label className="flex items-start gap-2 text-xs text-zinc-800">
            <input
              type="checkbox"
              className={CHECK_CLASS}
              checked={form.show_admin_column}
              disabled={disabled}
              onChange={e => onPatch({ show_admin_column: e.target.checked })}
            />
            <span>{__("Show column by default in entry list table")}</span>
          </label>
        </div>
      </fieldset>

      <fieldset className="space-y-2 rounded-lg border border-zinc-200 bg-zinc-50/50 p-3">
        <legend className="px-1 text-xs font-semibold text-zinc-800">{__("Attach to content types")}</legend>
        <div className="grid gap-2 sm:grid-cols-2">
          {attachableCpts.map(ct => (
            <label key={ct.descriptor.key} className="flex items-start gap-2 text-xs text-zinc-800">
              <input
                type="checkbox"
                className={CHECK_CLASS}
                checked={form.content_types.includes(ct.descriptor.key)}
                disabled={disabled}
                onChange={e => toggleCpt(ct.descriptor.key, e.target.checked)}
              />
              <span>
                {ct.descriptor.label}{' '}
                <span className="font-mono text-[11px] text-zinc-500">({ct.descriptor.key})</span>
              </span>
            </label>
          ))}
        </div>
      </fieldset>
    </div>
  );
};
