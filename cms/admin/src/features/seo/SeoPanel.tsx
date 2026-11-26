import { tStatic, useI18n } from '../../i18n/index.tsx';
import React, { useEffect, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import { getHomeSeo, putHomeSeo } from '../../api/seo.ts';
import {
  EMPTY_ADMIN_LOCALE_STATE,
  type AdminLocaleState,
} from '../../api/locales.ts';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';
import { LocaleTabs } from '../../components/LocaleTabs.tsx';

interface SeoPanelProps {
  csrfToken: string | null;
  onApiError: (error: unknown) => void;
  localeState?: AdminLocaleState;
}

export function SeoPanel({
  csrfToken,
  onApiError,
  localeState = EMPTY_ADMIN_LOCALE_STATE,
}: SeoPanelProps) {
  const { t: translate } = useI18n();
  const defaultLocaleCode = localeState.defaultLocale?.code ?? '';
  const [activeLocale, setActiveLocale] = useState<string>(defaultLocaleCode);
  useEffect(() => {
    if (!localeState.hasMultipleLocales) {
      setActiveLocale(defaultLocaleCode);
      return;
    }
    if (!activeLocale || !localeState.allLocales.some((l) => l.code === activeLocale)) {
      setActiveLocale(defaultLocaleCode);
    }
  }, [localeState, activeLocale, defaultLocaleCode]);

  const effectiveLocale =
    localeState.hasMultipleLocales && activeLocale ? activeLocale : undefined;

  const [version, setVersion] = useState<number>(1);
  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [ogImageId, setOgImageId] = useState('');
  const [canonicalUrl, setCanonicalUrl] = useState('');
  const [noindex, setNoindex] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [staleConflict, setStaleConflict] = useState(false);

  const load = async () => {
    setLoading(true);
    setStaleConflict(false);
    try {
      const snap = await getHomeSeo(effectiveLocale);
      setVersion(snap.version);
      setTitle(snap.title ?? '');
      setDescription(snap.description ?? '');
      setOgImageId(snap.og_image_id ?? '');
      setCanonicalUrl(snap.canonical_url ?? '');
      setNoindex(snap.noindex);
    } catch (error) {
      onApiError(error);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, [effectiveLocale]);

  const handleSave = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!csrfToken) return;
    setSaving(true);
    setStatus(null);
    setStaleConflict(false);
    try {
      const saved = await putHomeSeo(
        version,
        {
          title: title.trim() || null,
          description: description.trim() || null,
          og_image_id: ogImageId.trim() || null,
          canonical_url: canonicalUrl.trim() || null,
          noindex,
        },
        csrfToken,
        effectiveLocale,
      );
      setVersion(saved.version);
      setStatus(tStatic('feature.seo.seopanel.homepage_seo_saved_v_value1', { value1: saved.version }));
    } catch (error) {
      if (error instanceof ApiClientError && error.status === 412) {
        setStaleConflict(true);
        setStatus(tStatic('feature.seo.seopanel.the_homepage_seo_version_changed_your_draft_has_been_retained'));
      } else {
        onApiError(error);
      }
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="space-y-4" aria-label={translate('feature.seo.seopanel.homepage_seo')}>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-base font-semibold text-zinc-950">{translate('feature.seo.seopanel.homepage_seo')}</h2>
          <p className="mt-0.5 text-sm text-zinc-600">{translate('feature.seo.seopanel.public_metadata_for_the_homepage')}<code className="text-xs">v{version}</code>).
          </p>
        </div>
        <AdminButton variant="secondary" size="sm" disabled={loading || saving} onClick={load}>{translate('multilingual.reload')}</AdminButton>
      </div>
      <LocaleTabs
        localeState={localeState}
        activeLocale={effectiveLocale ?? defaultLocaleCode}
        onSelectLocale={setActiveLocale}
        testId="seo-locale-tabs"
      />
      {status && (
        <AdminAlert tone={staleConflict ? 'error' : 'success'}>
          {status}
        </AdminAlert>
      )}
      <form className="max-w-2xl space-y-4" onSubmit={handleSave}>
        <label className="flex flex-col gap-1 text-sm">
          <span className="font-medium text-zinc-800">{translate('feature.seo.seopanel.seo_title_up_to_120_characters')}</span>
          <input
            type="text"
            className="h-9 rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-od-accent"
            value={title}
            maxLength={120}
            disabled={loading || saving}
            onChange={(e) => setTitle(e.target.value)}
          />
        </label>
        <label className="flex flex-col gap-1 text-sm">
          <span className="font-medium text-zinc-800">{translate('feature.seo.seopanel.seo_description_up_to_320_characters')}</span>
          <textarea
            className="rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-od-accent"
            rows={2}
            value={description}
            maxLength={320}
            disabled={loading || saving}
            onChange={(e) => setDescription(e.target.value)}
          />
        </label>
        <div className="grid gap-3 md:grid-cols-2">
          <label className="flex flex-col gap-1 text-sm">
            <span className="font-medium text-zinc-800">{translate('feature.seo.seopanel.og_image_media_id_sha_256')}</span>
            <input
              type="text"
              className="h-9 rounded-md border border-zinc-300 bg-white px-3 font-mono text-xs text-zinc-900 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-od-accent"
              value={ogImageId}
              disabled={loading || saving}
              onChange={(e) => setOgImageId(e.target.value)}
            />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="font-medium text-zinc-800">{translate('feature.seo.seopanel.custom_canonical_url')}</span>
            <input
              type="url"
              className="h-9 rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-od-accent"
              value={canonicalUrl}
              disabled={loading || saving}
              onChange={(e) => setCanonicalUrl(e.target.value)}
            />
          </label>
        </div>
        <label className="flex items-center gap-2 text-sm text-zinc-800">
          <input
            type="checkbox"
            checked={noindex}
            disabled={loading || saving}
            onChange={(e) => setNoindex(e.target.checked)}
          />
          <span>{translate('feature.seo.seopanel.prevent_search_engines_from_indexing_noindex')}</span>
        </label>
        <div className="pt-1">
          <AdminButton type="submit" variant="primary" disabled={loading || saving || !csrfToken}>
            {saving ? translate('general.saving') : translate('feature.seo.seopanel.save_homepage_seo')}
          </AdminButton>
        </div>
      </form>
    </section>
  );
}
