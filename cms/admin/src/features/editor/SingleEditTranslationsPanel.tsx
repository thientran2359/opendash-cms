import React, { useState } from 'react';
import { __ } from '../../i18n/index.tsx';
import { ApiClientError } from '../../api/client.ts';
import { createEntryTranslation } from '../../api/content.ts';
import type { AdminLocaleState } from '../../api/locales.ts';
import type { TranslationLinkSummary } from '../content/types.ts';
import { SingleEditTranslationModal } from './SingleEditTranslationModal.tsx';

export interface SingleEditTranslationsPanelProps {
  typeKey: string;
  entryId: number | null;
  expectedVersion: number | null;
  slug: string;
  title: string;
  entryLocale: string;
  defaultLocaleCode: string;
  translations: TranslationLinkSummary[];
  localeState: AdminLocaleState;
  listHashPrefix: string;
  saving: boolean;
  csrfToken: string | null;
  onLocaleChange: (nextLocale: string) => void;
  onApiError: (error: unknown, fallback: string) => void;
  onNavigateHash: (hash: string) => void;
}

export const SingleEditTranslationsPanel: React.FC<SingleEditTranslationsPanelProps> = ({
  typeKey,
  entryId,
  expectedVersion,
  slug,
  title,
  entryLocale,
  defaultLocaleCode,
  translations,
  localeState,
  listHashPrefix,
  saving,
  csrfToken,
  onLocaleChange,
  onApiError,
  onNavigateHash,
}) => {
  const [translationModalLocale, setTranslationModalLocale] = useState<string | null>(null);
  const [translationTitle, setTranslationTitle] = useState('');
  const [translationSlug, setTranslationSlug] = useState('');
  const [creatingTranslation, setCreatingTranslation] = useState(false);
  const [translationError, setTranslationError] = useState<string | null>(null);

  const missingTranslationLocales = localeState.allLocales.filter(
    loc => loc.code !== entryLocale && !translations.some(tr => tr.locale === loc.code),
  );

  const handleCreateTranslation = async () => {
    if (!csrfToken || entryId === null || !translationModalLocale) return;
    if (expectedVersion === undefined || expectedVersion === null) return;
    setCreatingTranslation(true);
    setTranslationError(null);
    try {
      const created = await createEntryTranslation(
        typeKey,
        entryId,
        {
          target_locale: translationModalLocale,
          title: translationTitle.trim() || undefined,
          slug: translationSlug.trim() || slug || `${typeKey}-${translationModalLocale}`,
          auto_unique_slug: !translationSlug.trim(),
        },
        expectedVersion,
        csrfToken,
      );
      setTranslationModalLocale(null);
      onNavigateHash(`${listHashPrefix}/${created.id}`);
    } catch (error) {
      if (error instanceof ApiClientError) {
        setTranslationError(error.message);
      } else {
        setTranslationError(__("Could not create translation."));
      }
      onApiError(error, __("Could not create translation"));
    } finally {
      setCreatingTranslation(false);
    }
  };

  return (
    <>
      <div className="space-y-2.5" data-testid="entry-translations-panel">
        {entryId === null ? (
          <label
            className="flex flex-col gap-1 text-xs font-medium text-zinc-700"
            htmlFor="entry-locale-select"
          >
            <span>{__("Content language")}</span>
            <select
              id="entry-locale-select"
              data-testid="entry-locale-select"
              className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
              value={entryLocale || defaultLocaleCode}
              disabled={saving || !csrfToken}
              onChange={event => onLocaleChange(event.target.value)}
            >
              {localeState.allLocales.map(loc => (
                <option key={loc.code} value={loc.code}>
                  {loc.label} ({loc.code})
                  {loc.is_default ? __(" — Default") : ''}
                </option>
              ))}
            </select>
          </label>
        ) : (
          <>
            <div className="flex items-center justify-between text-xs">
              <span className="font-medium text-zinc-700">{__("Language")}</span>
              <span
                className="rounded bg-zinc-100 px-2 py-0.5 font-mono text-zinc-800"
                data-testid="entry-locked-locale"
              >
                {entryLocale || defaultLocaleCode}
              </span>
            </div>
            {translations.length > 0 && (
              <div className="space-y-1">
                <p className="text-xs font-medium text-zinc-600">{__("Linked translations")}</p>
                <ul className="space-y-1">
                  {translations.map(tr => (
                    <li key={tr.id} className="flex items-center justify-between gap-2 text-xs">
                      <a
                        href={`${listHashPrefix}/${tr.id}`}
                        onClick={e => {
                          e.preventDefault();
                          onNavigateHash(`${listHashPrefix}/${tr.id}`);
                        }}
                        className="truncate font-medium text-od-accent hover:underline"
                      >
                        [{tr.locale}] {tr.title || `#${tr.id}`}
                      </a>
                      <span className="shrink-0 text-[11px] text-zinc-500">
                        {tr.publication_status === 'published'
                          ? __("published")
                          : tr.publication_status === 'draft'
                            ? __("draft")
                            : tr.publication_status}
                      </span>
                    </li>
                  ))}
                </ul>
              </div>
            )}
            {missingTranslationLocales.length > 0 && (
              <div className="space-y-1 pt-1">
                <p className="text-xs font-medium text-zinc-600">{__("Create translation")}</p>
                <div className="flex flex-wrap gap-1.5">
                  {missingTranslationLocales.map(loc => (
                    <button
                      key={loc.code}
                      type="button"
                      disabled={saving || !csrfToken}
                      onClick={() => {
                        setTranslationModalLocale(loc.code);
                        setTranslationTitle('');
                        setTranslationSlug('');
                        setTranslationError(null);
                      }}
                      data-testid={`create-translation-${loc.code}`}
                      className="inline-flex h-7 items-center rounded border border-zinc-300 bg-white px-2 text-xs font-medium text-zinc-800 hover:bg-zinc-50 disabled:opacity-50"
                    >
                      {__("+ Create translation (")}
                      {loc.code})
                    </button>
                  ))}
                </div>
              </div>
            )}
          </>
        )}
      </div>
      <SingleEditTranslationModal
        targetLocale={translationModalLocale}
        sourceTitle={title}
        translationTitle={translationTitle}
        translationSlug={translationSlug}
        creating={creatingTranslation}
        error={translationError}
        canCreate={Boolean(csrfToken)}
        onTitleChange={setTranslationTitle}
        onSlugChange={setTranslationSlug}
        onCancel={() => setTranslationModalLocale(null)}
        onConfirm={() => void handleCreateTranslation()}
      />
    </>
  );
};
