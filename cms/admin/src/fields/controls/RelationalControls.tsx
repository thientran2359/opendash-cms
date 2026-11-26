import React, { useEffect, useId, useMemo, useRef, useState } from 'react';
import { queryContentEntryTable } from '../../api/content.ts';
import {
  getTaxonomyDefinition,
  listAllTaxonomyTerms,
  type TaxonomyTerm,
} from '../../api/taxonomy.ts';
import { STUDIO_INPUT_CLASS } from '../../builder/StudioFieldControls.tsx';
import { AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';
import type { EntryTableRow } from '../../features/content/types.ts';
import { getEffectiveBcp47, useI18n } from '../../i18n/index.tsx';
import { fetchOptionsSource, resolveEntriesByIds } from '../optionsSource.ts';
import type { DynamicSelectOption, FieldSpec, ResolvedEntrySummary } from '../types.ts';
import { FieldLabelHeader } from './BasicControls.tsx';

export const EntryRefControl: React.FC<{
  field: FieldSpec;
  value: unknown;
  locale?: string | null;
  disabled?: boolean;
  onChange: (next: number | number[] | null) => void;
  onApiError: (error: unknown, fallback: string) => void;
}> = ({ field, value, locale, disabled = false, onChange, onApiError }) => {
  const { t } = useI18n();
  const id = useId();
  const isWildcard = field.target_types?.length === 1 && field.target_types[0] === '*';
  const multiple = Boolean(field.multiple);
  const maxItems = Math.max(1, Math.min(50, field.max_items ?? 50));

  const selectedIds = useMemo(() => {
    if (multiple) {
      return Array.isArray(value)
        ? value.filter((n): n is number => typeof n === 'number' && Number.isSafeInteger(n) && n > 0)
        : [];
    }
    return typeof value === 'number' && Number.isSafeInteger(value) && value > 0 ? [value] : [];
  }, [multiple, value]);

  const [dynamicCpts, setDynamicCpts] = useState<DynamicSelectOption[]>([]);
  const [activeCpt, setActiveCpt] = useState<string>(() =>
    !isWildcard ? (field.target_types?.[0] ?? 'post') : '',
  );
  const [searchOpen, setSearchOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [searchRows, setSearchRows] = useState<EntryTableRow[]>([]);
  const [searching, setSearching] = useState(false);
  const [resolvedMap, setResolvedMap] = useState<Record<number, ResolvedEntrySummary>>({});

  useEffect(() => {
    if (!isWildcard) {
      setActiveCpt(prev =>
        field.target_types?.includes(prev) ? prev : (field.target_types?.[0] ?? 'post'),
      );
      return;
    }
    const controller = new AbortController();
    fetchOptionsSource('content_types', { signal: controller.signal })
      .then(list => {
        if (controller.signal.aborted) return;
        setDynamicCpts(list);
        setActiveCpt(prev =>
          prev && list.some(c => c.value === prev) ? prev : (list[0]?.value ?? ''),
        );
      })
      .catch(err => {
        if (!controller.signal.aborted) {
          onApiError(err, t('fields.errors.load_options_source'));
        }
      });
    return () => controller.abort();
  }, [isWildcard, field.target_types, onApiError, t]);

  const selectedIdsKey = selectedIds.join(',');
  useEffect(() => {
    if (!selectedIdsKey) {
      return;
    }
    const missing = selectedIds.filter(eid => !resolvedMap[eid]);
    if (missing.length === 0) return;
    const controller = new AbortController();
    resolveEntriesByIds(selectedIds, controller.signal)
      .then(items => {
        if (controller.signal.aborted) return;
        setResolvedMap(prev => {
          const next = { ...prev };
          for (const item of items) next[item.id] = item;
          return next;
        });
      })
      .catch(() => {
        // Non-fatal; fallback badge renders `#id`
      });
    return () => controller.abort();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedIdsKey]);

  useEffect(() => {
    if (!searchOpen || !activeCpt) return;
    const controller = new AbortController();
    setSearching(true);
    queryContentEntryTable(
      activeCpt,
      {
        q: query.trim() || undefined,
        locale: locale ?? undefined,
        limit: 20,
      },
      controller.signal,
    )
      .then(res => {
        if (!controller.signal.aborted) setSearchRows(res.items);
      })
      .catch(err => {
        if (!controller.signal.aborted) {
          onApiError(err, t('fields.errors.search_entries'));
        }
      })
      .finally(() => {
        if (!controller.signal.aborted) setSearching(false);
      });
    return () => controller.abort();
  }, [searchOpen, activeCpt, query, locale, onApiError, t]);

  const cptOptions: DynamicSelectOption[] = isWildcard
    ? dynamicCpts
    : (field.target_types ?? []).map(tk => ({ value: tk, label: tk }));

  const addEntry = (row: EntryTableRow) => {
    setResolvedMap(prev => ({
      ...prev,
      [row.id]: {
        id: row.id,
        content_type_key: row.type_key,
        title: row.title,
        slug: row.slug,
        publication_status: row.publication_status,
        locale: row.locale ?? locale ?? '',
      },
    }));
    if (multiple) {
      if (selectedIds.includes(row.id) || selectedIds.length >= maxItems) return;
      onChange([...selectedIds, row.id]);
    } else {
      onChange(row.id);
      setSearchOpen(false);
    }
  };

  const removeEntry = (entryId: number) => {
    if (multiple) {
      onChange(selectedIds.filter(idItem => idItem !== entryId));
    } else {
      onChange(null);
    }
  };

  const moveEntry = (idx: number, delta: -1 | 1) => {
    if (!multiple) return;
    const nextIdx = idx + delta;
    if (nextIdx < 0 || nextIdx >= selectedIds.length) return;
    const copy = [...selectedIds];
    [copy[idx], copy[nextIdx]] = [copy[nextIdx], copy[idx]];
    onChange(copy);
  };

  return (
    <div className="space-y-2" data-testid={`field-entry-ref-${field.key}`}>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <FieldLabelHeader htmlFor={id} label={field.label} required={field.required} />
        <AdminButton
          size="sm"
          variant="secondary"
          disabled={disabled}
          onClick={() => setSearchOpen(v => !v)}
        >
          {searchOpen ? t('fields.common.close') : t('fields.entry_ref.pick')}
        </AdminButton>
      </div>

      {selectedIds.length === 0 ? (
        <p className="text-xs italic text-zinc-400">{t('fields.entry_ref.none_selected')}</p>
      ) : (
        <ul className="space-y-1">
          {selectedIds.map((entryId, idx) => {
            const summary = resolvedMap[entryId];
            return (
              <li
                key={entryId}
                className="flex items-center justify-between gap-2 rounded-md border border-zinc-200 bg-white px-2.5 py-1.5 text-xs"
              >
                <div className="min-w-0 flex-1">
                  <span className="font-medium text-zinc-900">
                    {summary ? summary.title : `#${entryId}`}
                  </span>
                  <div className="mt-0.5 flex flex-wrap items-center gap-1">
                    {summary ? (
                      <>
                        <AdminBadge tone="neutral">{summary.content_type_key}</AdminBadge>
                        <AdminBadge tone={summary.publication_status === 'published' ? 'success' : 'warning'}>
                          {summary.publication_status}
                        </AdminBadge>
                        {summary.locale && <AdminBadge tone="neutral">{summary.locale}</AdminBadge>}
                      </>
                    ) : (
                      <span className="text-[11px] text-zinc-400">ID #{entryId}</span>
                    )}
                  </div>
                </div>
                <div className="flex items-center gap-1">
                  {multiple && (
                    <>
                      <button
                        type="button"
                        disabled={disabled || idx === 0}
                        aria-label={t('fields.common.move_up')}
                        onClick={() => moveEntry(idx, -1)}
                        className="rounded px-1 text-zinc-600 hover:bg-zinc-100 disabled:opacity-35"
                      >
                        ↑
                      </button>
                      <button
                        type="button"
                        disabled={disabled || idx === selectedIds.length - 1}
                        aria-label={t('fields.common.move_down')}
                        onClick={() => moveEntry(idx, 1)}
                        className="rounded px-1 text-zinc-600 hover:bg-zinc-100 disabled:opacity-35"
                      >
                        ↓
                      </button>
                    </>
                  )}
                  <button
                    type="button"
                    disabled={disabled}
                    aria-label={t('fields.common.remove')}
                    onClick={() => removeEntry(entryId)}
                    className="rounded px-1 text-rose-600 hover:bg-rose-50 disabled:opacity-35"
                  >
                    ✕
                  </button>
                </div>
              </li>
            );
          })}
        </ul>
      )}

      {searchOpen && (
        <div className="space-y-2 rounded-lg border border-zinc-200 bg-zinc-50/80 p-2.5">
          <div className="flex flex-wrap items-center gap-2">
            {cptOptions.length > 1 && (
              <select
                aria-label={t('fields.entry_ref.filter_type')}
                disabled={disabled}
                value={activeCpt}
                onChange={e => setActiveCpt(e.target.value)}
                className="h-7 rounded border border-zinc-300 bg-white px-2 text-xs text-zinc-900"
              >
                {cptOptions.map(opt => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))}
              </select>
            )}
            <input
              id={id}
              type="search"
              value={query}
              disabled={disabled}
              placeholder={t('fields.entry_ref.search_placeholder')}
              onChange={e => setQuery(e.target.value)}
              className={`${STUDIO_INPUT_CLASS} flex-1`}
            />
          </div>

          {searching ? (
            <p className="text-xs text-zinc-500">{t('fields.common.loading')}</p>
          ) : searchRows.length === 0 ? (
            <p className="text-xs italic text-zinc-400">{t('fields.entry_ref.no_results')}</p>
          ) : (
            <ul className="max-h-48 space-y-1 overflow-y-auto">
              {searchRows.map(row => {
                const picked = selectedIds.includes(row.id);
                const limitHit = multiple && selectedIds.length >= maxItems && !picked;
                return (
                  <li key={row.id}>
                    <button
                      type="button"
                      disabled={disabled || picked || limitHit}
                      onClick={() => addEntry(row)}
                      className="flex w-full items-center justify-between gap-2 rounded border border-zinc-200 bg-white px-2.5 py-1.5 text-left text-xs hover:bg-sky-50 disabled:opacity-50"
                    >
                      <span className="truncate font-medium text-zinc-800">{row.title}</span>
                      <span className="shrink-0 font-mono text-[10px] text-zinc-500">
                        {picked ? t('fields.common.selected') : `#${row.id} · ${row.publication_status}`}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      )}
    </div>
  );
};

interface TermTreeItem {
  term: TaxonomyTerm;
  depth: number;
}

function buildTermTreeOrder(terms: TaxonomyTerm[]): TermTreeItem[] {
  const byParent = new Map<number | null, TaxonomyTerm[]>();
  const validIds = new Set(terms.map(t => t.id));
  for (const term of terms) {
    const parent = term.parent_id && validIds.has(term.parent_id) ? term.parent_id : null;
    const list = byParent.get(parent) ?? [];
    list.push(term);
    byParent.set(parent, list);
  }
  const out: TermTreeItem[] = [];
  const visited = new Set<number>();
  const walk = (parentId: number | null, depth: number) => {
    for (const term of byParent.get(parentId) ?? []) {
      if (visited.has(term.id)) continue;
      visited.add(term.id);
      out.push({ term, depth });
      walk(term.id, Math.min(8, depth + 1));
    }
  };
  walk(null, 0);
  return out;
}

export const TermRefControl: React.FC<{
  field: FieldSpec;
  value: unknown;
  scopeValues: Record<string, unknown>;
  rootValues: Record<string, unknown>;
  scopeSpecs: readonly FieldSpec[];
  rootSpecs: readonly FieldSpec[];
  locale?: string | null;
  disabled?: boolean;
  onChange: (next: number | number[] | null) => void;
  onApiError: (error: unknown, fallback: string) => void;
}> = ({
  field,
  value,
  scopeValues,
  rootValues,
  scopeSpecs,
  rootSpecs,
  locale,
  disabled = false,
  onChange,
  onApiError,
}) => {
  const { t } = useI18n();
  const id = useId();
  const multiple = Boolean(field.multiple);
  const maxItems = Math.max(1, Math.min(50, field.max_items ?? 50));

  const dynamicTaxKey = useMemo(() => {
    if (!field.taxonomy_from_field) return field.taxonomy ?? '';
    const refKey = field.taxonomy_from_field;
    const spec = scopeSpecs.find(s => s.key === refKey) ?? rootSpecs.find(s => s.key === refKey);
    const raw = Object.prototype.hasOwnProperty.call(scopeValues, refKey)
      ? scopeValues[refKey]
      : Object.prototype.hasOwnProperty.call(rootValues, refKey)
        ? rootValues[refKey]
        : spec?.default;
    return typeof raw === 'string' ? raw.trim() : '';
  }, [field.taxonomy, field.taxonomy_from_field, scopeValues, rootValues, scopeSpecs, rootSpecs]);

  const selectedIds = useMemo(() => {
    if (multiple) {
      return Array.isArray(value)
        ? value.filter((n): n is number => typeof n === 'number' && Number.isSafeInteger(n) && n > 0)
        : [];
    }
    return typeof value === 'number' && Number.isSafeInteger(value) && value > 0 ? [value] : [];
  }, [multiple, value]);

  const [terms, setTerms] = useState<TaxonomyTerm[]>([]);
  const [loading, setLoading] = useState(false);
  const [hierarchical, setHierarchical] = useState(false);
  const [search, setSearch] = useState('');
  const prevTaxRef = useRef<string>(dynamicTaxKey);

  useEffect(() => {
    const prevTax = prevTaxRef.current;
    prevTaxRef.current = dynamicTaxKey;
    if (!dynamicTaxKey) {
      setTerms([]);
      if (field.taxonomy_from_field && prevTax !== '' && selectedIds.length > 0) {
        onChange(multiple ? [] : null);
      }
      return;
    }

    const controller = new AbortController();
    setLoading(true);
    Promise.all([
      listAllTaxonomyTerms(dynamicTaxKey, controller.signal, locale),
      getTaxonomyDefinition(dynamicTaxKey, controller.signal).catch(() => null),
    ])
      .then(([loadedTerms, def]) => {
        if (controller.signal.aborted) return;
        setTerms(loadedTerms);
        setHierarchical(Boolean(def?.hierarchical));
        if (field.taxonomy_from_field && prevTax && prevTax !== dynamicTaxKey) {
          const validSet = new Set(loadedTerms.map(item => item.id));
          if (multiple) {
            const kept = selectedIds.filter(tid => validSet.has(tid));
            if (kept.length !== selectedIds.length) onChange(kept);
          } else if (selectedIds[0] !== undefined && !validSet.has(selectedIds[0])) {
            onChange(null);
          }
        }
      })
      .catch(err => {
        if (!controller.signal.aborted) {
          onApiError(err, t('fields.errors.load_terms'));
        }
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => controller.abort();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dynamicTaxKey, locale]);

  if (field.taxonomy_from_field && !dynamicTaxKey) {
    return (
      <div className="space-y-1" data-testid={`field-term-ref-${field.key}`}>
        <FieldLabelHeader label={field.label} required={field.required} />
        <p className="rounded border border-dashed border-zinc-300 bg-zinc-50 px-2.5 py-2 text-xs text-zinc-500">
          {t('fields.term_ref.select_taxonomy_first')}
        </p>
      </div>
    );
  }

  const orderedTerms = hierarchical
    ? buildTermTreeOrder(terms)
    : terms.map(term => ({ term, depth: 0 }));
  const bcp47 = getEffectiveBcp47();
  const q = search.trim().toLocaleLowerCase(bcp47);
  const filteredTerms = q
    ? orderedTerms.filter(item => item.term.name.toLocaleLowerCase(bcp47).includes(q))
    : orderedTerms;

  const knownTermMap = new Map(terms.map(item => [item.id, item]));
  const unknownIds = selectedIds.filter(tid => !knownTermMap.has(tid));
  const style = field.picker_style ?? 'auto';

  const toggleMulti = (termId: number) => {
    if (selectedIds.includes(termId)) {
      onChange(selectedIds.filter(idItem => idItem !== termId));
    } else if (selectedIds.length < maxItems) {
      onChange([...selectedIds, termId]);
    }
  };

  if (!multiple && (style === 'select' || (style === 'auto' && !hierarchical))) {
    const curVal = selectedIds[0] ? String(selectedIds[0]) : '';
    return (
      <div className="space-y-1" data-testid={`field-term-ref-${field.key}`}>
        <FieldLabelHeader htmlFor={id} label={field.label} required={field.required} />
        <select
          id={id}
          disabled={disabled || loading}
          value={curVal}
          onChange={e => onChange(e.target.value ? Number(e.target.value) : null)}
          className={STUDIO_INPUT_CLASS}
        >
          <option value="">{loading ? t('fields.common.loading') : t('fields.select.empty_option')}</option>
          {unknownIds.map(uid => (
            <option key={`unknown-${uid}`} value={String(uid)}>
              #{uid}
            </option>
          ))}
          {orderedTerms.map(({ term, depth }) => (
            <option key={term.id} value={String(term.id)}>
              {depth > 0 ? `${'— '.repeat(depth)}${term.name}` : term.name}
            </option>
          ))}
        </select>
      </div>
    );
  }

  if (!multiple && style === 'radio') {
    const curId = selectedIds[0] ?? null;
    return (
      <div className="space-y-1.5" data-testid={`field-term-ref-${field.key}`}>
        <FieldLabelHeader label={field.label} required={field.required} />
        {!field.required && curId !== null && (
          <button
            type="button"
            disabled={disabled}
            onClick={() => onChange(null)}
            className="text-[11px] text-sky-700 hover:underline"
          >
            {t('fields.common.clear')}
          </button>
        )}
        <div className="max-h-48 space-y-1 overflow-y-auto rounded-md border border-zinc-200 bg-white p-2">
          {orderedTerms.map(({ term, depth }) => (
            <label
              key={term.id}
              style={depth > 0 ? { paddingLeft: `${depth * 12}px` } : undefined}
              className="flex cursor-pointer items-center gap-2 text-xs text-zinc-800"
            >
              <input
                type="radio"
                name={`term-${field.key}`}
                checked={curId === term.id}
                disabled={disabled}
                onChange={() => onChange(term.id)}
                className="h-3.5 w-3.5 border-zinc-300 text-sky-600 focus:ring-sky-600"
              />
              <span>{term.name}</span>
            </label>
          ))}
        </div>
      </div>
    );
  }

  if (multiple && style === 'tags') {
    return (
      <div className="space-y-2" data-testid={`field-term-ref-${field.key}`}>
        <FieldLabelHeader htmlFor={id} label={field.label} required={field.required} />
        {selectedIds.length > 0 && (
          <div className="flex flex-wrap gap-1">
            {selectedIds.map(tid => (
              <span
                key={tid}
                className="inline-flex items-center gap-1 rounded-full border border-sky-200 bg-sky-50 px-2 py-0.5 text-xs text-sky-900"
              >
                <span>{knownTermMap.get(tid)?.name ?? `#${tid}`}</span>
                <button
                  type="button"
                  disabled={disabled}
                  onClick={() => toggleMulti(tid)}
                  className="text-sky-700 hover:text-rose-700"
                  aria-label={t('fields.common.remove')}
                >
                  ×
                </button>
              </span>
            ))}
          </div>
        )}
        <input
          id={id}
          type="search"
          value={search}
          disabled={disabled}
          placeholder={t('fields.term_ref.search_placeholder')}
          onChange={e => setSearch(e.target.value)}
          className={STUDIO_INPUT_CLASS}
        />
        {!loading && filteredTerms.length > 0 && (
          <div className="flex max-h-36 flex-wrap gap-1 overflow-y-auto rounded border border-zinc-200 bg-white p-1.5">
            {filteredTerms.map(({ term }) => {
              const active = selectedIds.includes(term.id);
              return (
                <button
                  key={term.id}
                  type="button"
                  disabled={disabled || (!active && selectedIds.length >= maxItems)}
                  onClick={() => toggleMulti(term.id)}
                  className={
                    'rounded border px-2 py-0.5 text-xs ' +
                    (active
                      ? 'border-sky-600 bg-sky-600 text-white'
                      : 'border-zinc-200 bg-zinc-50 text-zinc-700 hover:bg-zinc-100')
                  }
                >
                  {term.name}
                </button>
              );
            })}
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="space-y-1.5" data-testid={`field-term-ref-${field.key}`}>
      <FieldLabelHeader htmlFor={id} label={field.label} required={field.required} />
      {unknownIds.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {unknownIds.map(uid => (
            <button
              key={uid}
              type="button"
              disabled={disabled}
              onClick={() => onChange(multiple ? selectedIds.filter(x => x !== uid) : null)}
              className="rounded border border-amber-300 bg-amber-50 px-2 py-0.5 text-[11px] text-amber-900"
            >
              #{uid} ×
            </button>
          ))}
        </div>
      )}
      <input
        id={id}
        type="search"
        value={search}
        disabled={disabled}
        placeholder={t('fields.term_ref.search_placeholder')}
        onChange={e => setSearch(e.target.value)}
        className={STUDIO_INPUT_CLASS}
      />
      <div className="max-h-48 space-y-1 overflow-y-auto rounded-md border border-zinc-200 bg-white p-2">
        {loading ? (
          <p className="text-xs text-zinc-500">{t('fields.common.loading')}</p>
        ) : filteredTerms.length === 0 ? (
          <p className="text-xs italic text-zinc-400">{t('fields.term_ref.empty')}</p>
        ) : (
          filteredTerms.map(({ term, depth }) => {
            const checked = selectedIds.includes(term.id);
            return (
              <label
                key={term.id}
                style={depth > 0 ? { paddingLeft: `${depth * 12}px` } : undefined}
                className="flex cursor-pointer items-center gap-2 text-xs text-zinc-800"
              >
                <input
                  type={multiple ? 'checkbox' : 'radio'}
                  name={multiple ? undefined : `term-check-${field.key}`}
                  checked={checked}
                  disabled={disabled || (multiple && !checked && selectedIds.length >= maxItems)}
                  onChange={() => {
                    if (multiple) {
                      toggleMulti(term.id);
                    } else {
                      onChange(checked ? null : term.id);
                    }
                  }}
                  className="h-3.5 w-3.5 rounded border-zinc-300 text-sky-600 focus:ring-sky-600"
                />
                <span>{term.name}</span>
              </label>
            );
          })
        )}
      </div>
    </div>
  );
};
