import { __, useI18n } from '../../i18n/index.tsx';
import React, { useEffect, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import { getHomeSeo, putHomeSeo } from '../../api/seo.ts';
import {
  EMPTY_ADMIN_LOCALE_STATE,
  type AdminLocaleState,
} from '../../api/locales.ts';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';
import { LocaleTabs } from '../../components/LocaleTabs.tsx';
import { mediaThumbnailUrl } from '../../api/media.ts';
import { MediaPicker } from '../media/MediaPicker.tsx';

interface SeoPanelProps {
  csrfToken: string | null;
  onApiError: (error: unknown) => void;
  onDirtyChange?: (dirty: boolean) => void;
  localeState?: AdminLocaleState;
}

export function SeoPanel({
  csrfToken,
  onApiError,
  onDirtyChange,
  localeState = EMPTY_ADMIN_LOCALE_STATE,
}: SeoPanelProps) {
  const { __ } = useI18n();
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
  const [savedSnapshot, setSavedSnapshot] = useState({
    title: '', description: '', ogImageId: '', canonicalUrl: '', noindex: false,
  });
  const [pickerOpen, setPickerOpen] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [staleConflict, setStaleConflict] = useState(false);

  const dirty = !loading && (
    title !== savedSnapshot.title ||
    description !== savedSnapshot.description ||
    ogImageId !== savedSnapshot.ogImageId ||
    canonicalUrl !== savedSnapshot.canonicalUrl ||
    noindex !== savedSnapshot.noindex
  );

  useEffect(() => {
    onDirtyChange?.(dirty);
  }, [dirty, onDirtyChange]);

  useEffect(() => () => { onDirtyChange?.(false); }, [onDirtyChange]);

  const load = async () => {
    setLoading(true);
    setStaleConflict(false);
    try {
      const snap = await getHomeSeo(effectiveLocale);
      const next = {
        title: snap.title ?? '',
        description: snap.description ?? '',
        ogImageId: snap.og_image_id ?? '',
        canonicalUrl: snap.canonical_url ?? '',
        noindex: snap.noindex,
      };
      setVersion(snap.version);
      setTitle(next.title);
      setDescription(next.description);
      setOgImageId(next.ogImageId);
      setCanonicalUrl(next.canonicalUrl);
      setNoindex(next.noindex);
      setSavedSnapshot(next);
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
      setSavedSnapshot({
        title: title.trim(),
        description: description.trim(),
        ogImageId: ogImageId.trim(),
        canonicalUrl: canonicalUrl.trim(),
        noindex,
      });
      setStatus(__("Homepage SEO saved (v{{value1}}).", { value1: saved.version }));
    } catch (error) {
      if (error instanceof ApiClientError && error.status === 412) {
        setStaleConflict(true);
        setStatus(__("The homepage SEO version changed. Your draft has been retained."));
      } else {
        onApiError(error);
      }
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="space-y-4" aria-label={__("Homepage SEO")}>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-base font-semibold text-zinc-950">{__("Homepage SEO")}</h2>
          <p className="mt-0.5 text-sm text-zinc-600">
            {__("Public metadata for the homepage (v{{version}}).", { version })}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <a
            href="#/tools/redirects"
            data-testid="seo-redirects-link"
            className="inline-flex h-8 items-center rounded-md border border-zinc-300 bg-white px-2.5 text-xs font-medium text-zinc-800 no-underline hover:bg-zinc-50"
          >{__("Redirects & 404")}</a>
          <AdminButton variant="secondary" size="sm" disabled={loading || saving} onClick={load}>{__("Reload")}</AdminButton>
        </div>
      </div>
      <LocaleTabs
        localeState={localeState}
        activeLocale={effectiveLocale ?? defaultLocaleCode}
        onSelectLocale={(code) => {
          if (code === (effectiveLocale ?? defaultLocaleCode) || saving) return;
          if (dirty && !window.confirm(__("Switch language and discard unsaved changes in this tab?"))) return;
          setActiveLocale(code);
        }}
        testId="seo-locale-tabs"
      />
      {status && (
        <AdminAlert tone={staleConflict ? 'error' : 'success'}>
          {status}
        </AdminAlert>
      )}
      <form className="max-w-2xl space-y-4" onSubmit={handleSave}>
        <label className="flex flex-col gap-1 text-sm">
          <span className="font-medium text-zinc-800">{__("SEO title (up to 120 characters)")}</span>
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
          <span className="font-medium text-zinc-800">{__("SEO description (up to 320 characters)")}</span>
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
          <div className="flex flex-col gap-1.5 text-sm">
            <label htmlFor="seo-og-image-input" className="font-medium text-zinc-800">
              {__("OG image")}
            </label>
            <div className="flex flex-wrap items-center gap-2">
              <AdminButton
                variant="secondary"
                size="sm"
                disabled={loading || saving}
                data-testid="seo-choose-og-image"
                onClick={() => setPickerOpen(true)}
              >
                {__("Choose OG image")}
              </AdminButton>
              {ogImageId && (
                <AdminButton
                  variant="ghost"
                  size="sm"
                  disabled={loading || saving}
                  onClick={() => setOgImageId('')}
                >
                  {__("Clear")}
                </AdminButton>
              )}
            </div>
            {ogImageId && /^[0-9a-f]{64}$/i.test(ogImageId.trim()) && (
              <img
                src={mediaThumbnailUrl(ogImageId.trim())}
                alt={__("OG image preview")}
                className="h-16 w-28 rounded-md border border-zinc-200 bg-zinc-100 object-cover"
              />
            )}
            <input
              id="seo-og-image-input"
              type="text"
              placeholder={__("SHA-256 media ID (optional)")}
              className="h-9 rounded-md border border-zinc-300 bg-white px-3 font-mono text-xs text-zinc-900 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-od-accent"
              value={ogImageId}
              disabled={loading || saving}
              onChange={(e) => setOgImageId(e.target.value)}
            />
          </div>
          <label className="flex flex-col gap-1 text-sm">
            <span className="font-medium text-zinc-800">{__("Custom canonical URL")}</span>
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
          <span>{__("Prevent search engines from indexing (noindex)")}</span>
        </label>
        <div className="pt-1">
          <AdminButton type="submit" variant="primary" disabled={loading || saving || !csrfToken}>
            {saving ? __("Saving…") : __("Save homepage SEO")}
          </AdminButton>
        </div>
      </form>
      {pickerOpen && (
        <MediaPicker
          mode="image"
          acceptKind="image"
          selectedIds={ogImageId ? [ogImageId] : []}
          csrfToken={csrfToken}
          onApply={(ids) => {
            setOgImageId(ids[0] ?? '');
            setPickerOpen(false);
          }}
          onClose={() => setPickerOpen(false)}
          onError={(err) => onApiError(err)}
        />
      )}
    </section>
  );
}
