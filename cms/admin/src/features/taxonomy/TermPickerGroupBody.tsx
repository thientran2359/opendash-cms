import React from 'react';
import type { TaxonomyDefinition, TaxonomyTerm } from '../../api/taxonomy.ts';
import type { TermPickerStyle } from '../../fields/types.ts';
import { __, _x } from '../../i18n/index.tsx';

export interface TermPickerGroupItem {
  kind: string;
  title: string;
  hierarchical: boolean;
  definition: TaxonomyDefinition;
  terms: Array<{ term: TaxonomyTerm; depth: number }>;
  count: number;
}

export interface TermPickerGroupBodyProps {
  group: TermPickerGroupItem;
  visible: Array<{ term: TaxonomyTerm; depth: number }>;
  pickedInGroup: Array<{ term: TaxonomyTerm; depth: number }>;
  selected: Set<number>;
  maxForKind: number;
  totalSelectedCount: number;
  searchQuery: string;
  loading: boolean;
  disabled?: boolean;
  hasError: boolean;
  primaryCategoryTermId?: number | null;
  onSearchChange: (value: string) => void;
  onToggleTerm: (termId: number, checked: boolean) => void;
  onSelectExclusiveTerm: (kind: string, termId: number | null) => void;
  onPrimaryCategoryChange?: (id: number | null) => void;
  onOpenInlineCreate?: () => void;
  canInlineCreate: boolean;
}

export const TermPickerGroupBody: React.FC<TermPickerGroupBodyProps> = ({
  group,
  visible,
  pickedInGroup,
  selected,
  maxForKind,
  totalSelectedCount,
  searchQuery,
  loading,
  disabled = false,
  hasError,
  primaryCategoryTermId = null,
  onSearchChange,
  onToggleTerm,
  onSelectExclusiveTerm,
  onPrimaryCategoryChange,
  onOpenInlineCreate,
  canInlineCreate,
}) => {
  const settings = group.definition.settings;
  const selectionMode = settings?.selection_mode ?? 'multiple';
  const style: TermPickerStyle = settings?.picker_style ?? 'auto';
  const effectiveStyle: Exclude<TermPickerStyle, 'auto'> =
    style !== 'auto'
      ? style
      : selectionMode === 'single'
        ? (group.hierarchical ? 'radio' : 'select')
        : group.kind === 'tag'
          ? 'tags'
          : 'checklist';

  const currentSingleId = pickedInGroup[0]?.term.id ?? null;

  return (
    <div className="mt-3 space-y-2.5">
      {group.terms.length > 5 && effectiveStyle !== 'select' && (
        <input
          type="search"
          aria-label={__("Search in {{value1}}", { value1: group.title })}
          value={searchQuery}
          onChange={e => onSearchChange(e.target.value)}
          placeholder={__("Search {{value1}}...", { value1: group.title.toLowerCase() })}
          disabled={disabled}
          className="h-8 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-2.5 text-xs text-zinc-900 placeholder:text-zinc-400 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50"
        />
      )}

      {(effectiveStyle === 'tags' || (!group.hierarchical && pickedInGroup.length > 0)) &&
        pickedInGroup.length > 0 && (
          <div className="flex flex-wrap gap-1.5" data-testid={`picked-chips-${group.kind}`}>
            {pickedInGroup.map(({ term }) => (
              <span
                key={term.id}
                className="inline-flex items-center gap-1 rounded-full bg-od-accent-soft px-2.5 py-0.5 text-xs font-medium text-od-accent-ink"
              >
                {term.name}
                <button
                  type="button"
                  aria-label={__("Remove {{value1}}", { value1: term.name })}
                  disabled={disabled}
                  className="rounded-full px-0.5 font-bold hover:bg-white/60 disabled:opacity-50"
                  onClick={() => onToggleTerm(term.id, false)}
                >
                  ×
                </button>
              </span>
            ))}
          </div>
        )}

      {effectiveStyle === 'select' && (
        <select
          aria-label={group.title}
          data-testid={`term-select-${group.kind}`}
          disabled={disabled || loading || hasError}
          value={
            selectionMode === 'single'
              ? (currentSingleId ? String(currentSingleId) : '')
              : ''
          }
          onChange={e => {
            const raw = e.target.value;
            if (selectionMode === 'single') {
              onSelectExclusiveTerm(group.kind, raw ? Number(raw) : null);
            } else if (raw) {
              onToggleTerm(Number(raw), true);
            }
          }}
          className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50"
        >
          <option value="">
            {selectionMode === 'single' ? __("— None —") : __("+ Choose term to add…")}
          </option>
          {group.terms.map(({ term, depth }) => (
            <option key={term.id} value={String(term.id)}>
              {depth > 0 ? `${'— '.repeat(Math.min(depth, 8))}${term.name}` : term.name}
              {selectionMode === 'multiple' && selected.has(term.id) ? ` (${__("selected")})` : ''}
            </option>
          ))}
        </select>
      )}

      {effectiveStyle === 'radio' && (
        <div className="space-y-1.5" data-testid={`term-radio-${group.kind}`}>
          {!settings?.required_on_publish && currentSingleId !== null && (
            <button
              type="button"
              disabled={disabled}
              onClick={() => onSelectExclusiveTerm(group.kind, null)}
              className="text-xs text-sky-700 hover:underline disabled:opacity-50"
            >
              {__("Clear selection")}
            </button>
          )}
          <div className="max-h-52 space-y-1 overflow-y-auto pr-1">
            {visible.map(({ term, depth }) => (
              <label
                key={term.id}
                className="flex min-w-0 cursor-pointer items-center gap-2 rounded-md px-1.5 py-1 text-sm text-zinc-800 hover:bg-zinc-50"
              >
                <input
                  type="radio"
                  name={`term-picker-radio-${group.kind}`}
                  checked={selected.has(term.id)}
                  disabled={disabled || loading || hasError}
                  onChange={() => onSelectExclusiveTerm(group.kind, term.id)}
                  className="shrink-0 accent-sky-600"
                />
                <span
                  className="min-w-0 break-words"
                  style={{ paddingLeft: `${Math.min(depth, 8) * 10}px` }}
                >
                  {term.name}
                </span>
              </label>
            ))}
          </div>
        </div>
      )}

      {(effectiveStyle === 'checklist' || effectiveStyle === 'tags') && (
        <div className="max-h-52 space-y-1 overflow-y-auto pr-1">
          {visible.map(({ term, depth }) => {
            const isChecked = selected.has(term.id);
            const isDisabled =
              disabled ||
              loading ||
              hasError ||
              (!isChecked &&
                ((selectionMode === 'multiple' && group.count >= maxForKind) ||
                  totalSelectedCount >= 96));
            return (
              <div
                key={term.id}
                className="flex min-w-0 items-center justify-between gap-2 rounded-md px-1.5 py-1 text-sm text-zinc-800 hover:bg-zinc-50"
              >
                <label className="flex min-w-0 flex-1 cursor-pointer items-center gap-2">
                  <input
                    type={selectionMode === 'single' ? 'radio' : 'checkbox'}
                    name={selectionMode === 'single' ? `term-picker-single-${group.kind}` : undefined}
                    className="shrink-0"
                    checked={isChecked}
                    onChange={e => {
                      if (selectionMode === 'single') {
                        onSelectExclusiveTerm(group.kind, e.target.checked ? term.id : null);
                      } else {
                        onToggleTerm(term.id, e.target.checked);
                      }
                    }}
                    disabled={isDisabled}
                  />
                  <span
                    className="min-w-0 break-words"
                    style={{ paddingLeft: `${Math.min(depth, 8) * 10}px` }}
                  >
                    {term.name}
                  </span>
                </label>
                {group.kind === 'category' && isChecked && onPrimaryCategoryChange && (
                  <button
                    type="button"
                    data-testid={`set-primary-term-${term.id}`}
                    aria-pressed={primaryCategoryTermId === term.id}
                    disabled={disabled || loading || hasError}
                    onClick={() => onPrimaryCategoryChange(term.id)}
                    className="shrink-0 rounded-md border border-zinc-200 px-2 py-0.5 text-[11px] font-medium text-od-accent hover:bg-od-accent-soft disabled:opacity-50"
                  >
                    {primaryCategoryTermId === term.id ? _x("Primary", "feature.taxonomy") : __("Set as primary")}
                  </button>
                )}
              </div>
            );
          })}
          {!loading && visible.length === 0 && (
            <p className="py-1 text-xs text-zinc-500">
              {searchQuery ? _x("No matching items.", "feature.taxonomy") : __("No items yet.")}
            </p>
          )}
        </div>
      )}

      {canInlineCreate && onOpenInlineCreate && (
        <div className="border-t border-zinc-100 pt-2">
          <button
            type="button"
            disabled={disabled || loading}
            data-testid={`inline-add-term-${group.kind}`}
            onClick={onOpenInlineCreate}
            className="text-xs font-medium text-sky-700 hover:underline disabled:opacity-50"
          >
            {__("+ Add {{value1}}", { value1: group.title })}
          </button>
        </div>
      )}
    </div>
  );
};
