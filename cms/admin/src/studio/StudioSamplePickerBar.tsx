import React, { useEffect, useState } from 'react';
import type { TemplatePreviewEntry, TemplatePreviewTerm } from '../api/builder.ts';
import { queryContentEntryTable } from '../api/content.ts';
import type { StudioTargetItem } from '../api/studio.ts';
import { listTaxonomyTerms, type TaxonomyTerm } from '../api/taxonomy.ts';
import type { EntryTableRow } from '../features/content/types.ts';
import { __ } from '../i18n/index.tsx';

interface StudioSamplePickerBarProps {
  target: StudioTargetItem | null;
  previewLocale: string;
  previewEntry: TemplatePreviewEntry | null;
  previewTerm: TemplatePreviewTerm | null;
  onChangePreviewEntry: (next: TemplatePreviewEntry | null) => void;
  onChangePreviewTerm: (next: TemplatePreviewTerm | null) => void;
  disabled?: boolean;
}

const ENTRY_SAMPLE_KINDS = new Set(['single', 'single_custom', 'singular', 'page', 'page_layout']);

export const StudioSamplePickerBar: React.FC<StudioSamplePickerBarProps> = ({
  target,
  previewLocale,
  previewEntry,
  previewTerm,
  onChangePreviewEntry,
  onChangePreviewTerm,
  disabled = false,
}) => {
  const [entries, setEntries] = useState<EntryTableRow[]>([]);
  const [terms, setTerms] = useState<TaxonomyTerm[]>([]);
  const [loadingSamples, setLoadingSamples] = useState(false);

  const kind = target?.target.kind ?? null;
  const isEntryTarget = Boolean(kind && ENTRY_SAMPLE_KINDS.has(kind));
  const isTaxonomyTarget = kind === 'taxonomy';

  const resolvedEntryTypeKey = target
    ? (target.target.content_type
      ?? (target.target.kind === 'page' || target.target.kind === 'page_layout' ? 'page' : 'post'))
    : 'post';
  const resolvedTaxonomyKey = target?.target.taxonomy?.trim() || 'category';

  useEffect(() => {
    if (!target || (!isEntryTarget && !isTaxonomyTarget)) {
      setEntries([]);
      setTerms([]);
      setLoadingSamples(false);
      return;
    }

    const controller = new AbortController();
    setLoadingSamples(true);

    if (isEntryTarget) {
      setTerms([]);
      void queryContentEntryTable(
        resolvedEntryTypeKey,
        {
          status: 'all',
          locale: previewLocale || undefined,
          limit: 50,
          sort: 'updated_at',
          order: 'desc',
        },
        controller.signal,
      )
        .then(res => {
          if (!controller.signal.aborted) setEntries(res.items);
        })
        .catch(() => {
          if (!controller.signal.aborted) setEntries([]);
        })
        .finally(() => {
          if (!controller.signal.aborted) setLoadingSamples(false);
        });
    } else if (isTaxonomyTarget) {
      setEntries([]);
      void listTaxonomyTerms(
        resolvedTaxonomyKey,
        100,
        0,
        controller.signal,
        previewLocale || undefined,
      )
        .then(res => {
          if (!controller.signal.aborted) setTerms(res.items);
        })
        .catch(() => {
          if (!controller.signal.aborted) setTerms([]);
        })
        .finally(() => {
          if (!controller.signal.aborted) setLoadingSamples(false);
        });
    }

    return () => controller.abort();
  }, [
    target?.canonical_target_key,
    isEntryTarget,
    isTaxonomyTarget,
    resolvedEntryTypeKey,
    resolvedTaxonomyKey,
    previewLocale,
  ]);

  if (!target || (!isEntryTarget && !isTaxonomyTarget)) return null;

  return (
    <div
      className="flex flex-wrap items-center justify-between gap-2 border-b border-zinc-200 bg-zinc-50 px-4 py-2 text-xs"
      data-testid="studio-sample-picker-bar"
    >
      <div className="flex min-w-0 items-center gap-2">
        <span className="shrink-0 text-[11px] font-semibold uppercase tracking-wide text-zinc-600">
          {isEntryTarget ? __("Preview entry") : __("Preview term")}
        </span>
        {isEntryTarget ? (
          <select
            aria-label={__("Preview entry")}
            data-testid="studio-sample-entry-select"
            disabled={disabled || loadingSamples}
            value={previewEntry ? String(previewEntry.entry_id) : ''}
            onChange={event => {
              const raw = event.target.value;
              if (!raw) {
                onChangePreviewEntry(null);
                return;
              }
              const entryId = Number(raw);
              const found = entries.find(item => item.id === entryId);
              if (found) {
                onChangePreviewEntry({ type_key: found.type_key || resolvedEntryTypeKey, entry_id: found.id });
              }
            }}
            className="h-7 min-w-[220px] max-w-[360px] rounded border border-zinc-300 bg-white px-2 text-xs text-zinc-900"
          >
            <option value="">{loadingSamples ? __("Loading samples…") : __("Auto (latest)")}</option>
            {entries.map(item => (
              <option key={item.id} value={String(item.id)}>
                {item.title} [{item.publication_status === 'draft' ? __("Draft") : __("Published")}] (#{item.id})
              </option>
            ))}
          </select>
        ) : (
          <select
            aria-label={__("Preview term")}
            data-testid="studio-sample-term-select"
            disabled={disabled || loadingSamples}
            value={previewTerm ? String(previewTerm.term_id) : ''}
            onChange={event => {
              const raw = event.target.value;
              if (!raw) {
                onChangePreviewTerm(null);
                return;
              }
              const termId = Number(raw);
              const found = terms.find(item => item.id === termId);
              if (found) {
                onChangePreviewTerm({ taxonomy_key: found.kind || resolvedTaxonomyKey, term_id: found.id });
              }
            }}
            className="h-7 min-w-[220px] max-w-[360px] rounded border border-zinc-300 bg-white px-2 text-xs text-zinc-900"
          >
            <option value="">{loadingSamples ? __("Loading samples…") : __("Auto (latest)")}</option>
            {terms.map(item => (
              <option key={item.id} value={String(item.id)}>
                {item.name} ({item.slug}) #{item.id}
              </option>
            ))}
          </select>
        )}
      </div>
      {loadingSamples && <span className="text-[11px] text-zinc-500">{__("Loading samples…")}</span>}
    </div>
  );
};
