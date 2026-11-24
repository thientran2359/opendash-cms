import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import {
  getPublicSiteSettings,
  savePublicSiteSettings,
  type PublicSiteSettings,
  type PublicSiteSettingsDraft,
} from '../../api/settings.ts';
import { ApiClientError } from '../../api/client.ts';
import { EMPTY_ADMIN_LOCALE_STATE, type AdminLocaleState } from '../../api/locales.ts';
import { useI18n, tStatic, getEffectiveBcp47 } from '../../i18n/index.tsx';
import {
  getGeneralSettings,
  getTimezoneCatalog,
  saveGeneralSettings,
  type GeneralSettings,
  type GeneralSettingsDraft,
  type TimezoneCatalog,
} from '../../api/settings.ts';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';
import { LocaleTabs } from '../../components/LocaleTabs.tsx';

const FIELD_CLASS =
  'h-9 w-full max-w-xl rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 ' +
  'focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-od-accent';
const TEXTAREA_CLASS =
  'w-full max-w-xl rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm text-zinc-900 ' +
  'focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-od-accent';

interface Props {
  csrfToken: string | null;
  onApiError?: (error: unknown) => void;
  localeState?: AdminLocaleState;
  onLocalesChanged?: () => void | Promise<void>;
}

const WINTER_SAMPLE = new Date('2026-01-15T12:00:00Z');
const SUMMER_SAMPLE = new Date('2026-07-15T12:00:00Z');

function previewTime(timezone: string, at: Date): { text: string; fallback: boolean } {
  const format = (zone: string) => new Intl.DateTimeFormat(getEffectiveBcp47(), {
    timeZone: zone,
    year: 'numeric',
    month: 'long',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    timeZoneName: 'short',
  }).format(at);
  try {
    return { text: format(timezone), fallback: false };
  } catch {
    // Display fallback does not change the saved or pending timezone.
    return { text: format('UTC'), fallback: true };
  }
}

function messageFor(error: unknown): string {
  return error instanceof Error ? error.message : tStatic('general.error');
}

export function GeneralSettingsPanel({
  csrfToken,
  onApiError,
  localeState = EMPTY_ADMIN_LOCALE_STATE,
  onLocalesChanged,
}: Props) {
  const { t, bootstrap, refreshBootstrap, uiLocale } = useI18n();
  // The AdminShell callback is recreated by the parent on render. Keep the
  // loader stable so an unrelated parent update never reloads and erases a draft.
  const onApiErrorRef = useRef(onApiError);
  onApiErrorRef.current = onApiError;
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
  const isSecondaryLocale = Boolean(
    localeState.hasMultipleLocales &&
      effectiveLocale &&
      defaultLocaleCode &&
      effectiveLocale !== defaultLocaleCode,
  );

  const [saved, setSaved] = useState<GeneralSettings | null>(null);
  const [draft, setDraft] = useState<GeneralSettingsDraft | null>(null);
  const [catalog, setCatalog] = useState<TimezoneCatalog | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [stale, setStale] = useState(false);

  const load = useCallback(async (signal?: AbortSignal) => {
    setLoading(true);
    setError(null);
    setNotice(null);
    try {
      const [settings, timezones] = await Promise.all([
        getGeneralSettings(signal, effectiveLocale),
        getTimezoneCatalog(signal),
      ]);
      if (signal?.aborted) return;
      setSaved(settings);
      setDraft({
        site_title: settings.site_title,
        tagline: settings.tagline,
        timezone: settings.timezone,
        cms_locale: settings.cms_locale,
      });
      setCatalog(timezones);
      setStale(false);
    } catch (reason) {
      if (!signal?.aborted) {
        setError(messageFor(reason));
        onApiErrorRef.current?.(reason);
      }
    } finally {
      if (!signal?.aborted) setLoading(false);
    }
  }, [effectiveLocale]);

  useEffect(() => {
    const controller = new AbortController();
    void load(controller.signal);
    return () => controller.abort();
  }, [load]);

  const dirty = Boolean(saved && draft && (
    draft.site_title !== saved.site_title ||
    draft.tagline !== saved.tagline ||
    draft.timezone !== saved.timezone ||
    draft.cms_locale !== saved.cms_locale
  ));
  const hasLegacyTimezone = Boolean(draft && catalog && !catalog.items.includes(draft.timezone));
  const preview = useMemo(() => {
    if (!draft) return null;
    return {
      winter: previewTime(draft.timezone, WINTER_SAMPLE),
      summer: previewTime(draft.timezone, SUMMER_SAMPLE),
    };
  }, [draft?.timezone, uiLocale]);

  async function handleSave(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!saved || !draft || !csrfToken || saving) return;
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      const updated = await saveGeneralSettings(
        isSecondaryLocale ? { site_title: draft.site_title, tagline: draft.tagline, timezone: draft.timezone } : draft,
        saved.version,
        csrfToken,
        effectiveLocale,
      );
      setSaved(updated);
      setDraft({ site_title: updated.site_title, tagline: updated.tagline, timezone: updated.timezone, cms_locale: updated.cms_locale });
      setStale(false);
      await refreshBootstrap();
      setNotice('saved');
    } catch (reason) {
      if (reason instanceof ApiClientError && reason.status === 412) {
        setStale(true);
        setError(t('general.stale'));
      } else {
        setError(messageFor(reason));
      }
      onApiError?.(reason);
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className="min-w-0 space-y-6" aria-labelledby="general-settings-heading" data-testid="general-settings">
      <div>
        <h2 id="general-settings-heading" className="text-base font-semibold text-zinc-950">{t('general.heading')}</h2>
        <p className="mt-1 text-sm text-zinc-600">
          {t('general.description')}
        </p>
      </div>
      <LocaleTabs
        localeState={localeState}
        activeLocale={effectiveLocale ?? defaultLocaleCode}
        onSelectLocale={(code) => { if (!saving) setActiveLocale(code); }}
        testId="general-locale-tabs"
      />
      {loading && <p role="status" className="text-sm text-zinc-500">{t('general.loading')}</p>}
      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      {notice && <AdminAlert tone="success">{t('general.saved')}</AdminAlert>}
      {stale && <p className="text-sm text-zinc-700">{t('general.reload_hint')}</p>}
      {!loading && (!draft || !saved || !catalog) && (
        <AdminButton variant="primary" onClick={() => void load()}>
          {t('general.retry')}
        </AdminButton>
      )}
      {!loading && draft && saved && catalog && (
        <form className="space-y-4" onSubmit={(event) => void handleSave(event)}>
          <div>
            <label htmlFor="od-site-title" className="mb-1 block text-sm font-medium text-zinc-800">{t('general.site_title')}</label>
            <input
              id="od-site-title" name="site_title" autoComplete="organization" required disabled={saving}
              className={FIELD_CLASS}
              value={draft.site_title}
              onChange={(event) => setDraft({ ...draft, site_title: event.target.value })}
              aria-describedby="od-site-title-help"
            />
            <p id="od-site-title-help" className="mt-1 text-xs text-zinc-500">{t('general.site_title_help')}</p>
          </div>
          <div>
            <label htmlFor="od-tagline" className="mb-1 block text-sm font-medium text-zinc-800">{t('general.tagline')}</label>
            <textarea
              id="od-tagline" name="tagline" rows={3} disabled={saving}
              className={TEXTAREA_CLASS}
              value={draft.tagline}
              onChange={(event) => setDraft({ ...draft, tagline: event.target.value })}
              aria-describedby="od-tagline-help"
            />
            <p id="od-tagline-help" className="mt-1 text-xs text-zinc-500">{t('general.tagline_help')}</p>
          </div>
          <div>
            <label htmlFor="od-timezone" className="mb-1 block text-sm font-medium text-zinc-800">{t('general.timezone')}</label>
            <select
              id="od-timezone" name="timezone"
              className={FIELD_CLASS}
              value={draft.timezone}
              disabled={isSecondaryLocale || saving}
              onChange={(event) => setDraft({ ...draft, timezone: event.target.value })}
              aria-describedby="od-timezone-help"
            >
              {hasLegacyTimezone && <option value={draft.timezone}>{t('general.timezone_legacy', { timezone: draft.timezone })}</option>}
              {catalog.items.map((timezone) => <option key={timezone} value={timezone}>{timezone}</option>)}
            </select>
            <p id="od-timezone-help" className="mt-1 text-xs text-zinc-500">
              {isSecondaryLocale
                ? t('general.timezone_secondary')
                : t('general.timezone_catalog', { version: catalog.version })}
            </p>
          </div>
          <div>
            <label htmlFor="od-cms-locale" className="mb-1 block text-sm font-medium text-zinc-800">{t('general.cms_locale')}</label>
            <select id="od-cms-locale" data-testid="cms-locale-select" className={FIELD_CLASS}
              value={draft.cms_locale} disabled={isSecondaryLocale || saving}
              onChange={(event) => setDraft({ ...draft, cms_locale: event.target.value })}
              aria-describedby="od-cms-locale-help">
              {draft.cms_locale && !bootstrap.available_locales.some((option) => option.code === draft.cms_locale) && (
                <option value={draft.cms_locale}>{draft.cms_locale}</option>
              )}
              {bootstrap.available_locales.map((option) => (
                <option key={option.code} value={option.code}>{option.native_name} — {option.english_name} ({option.code})</option>
              ))}
            </select>
            <p id="od-cms-locale-help" className="mt-1 text-xs text-zinc-500">{isSecondaryLocale ? t('general.cms_secondary') : t('general.cms_help')}</p>
            {bootstrap.available_locales.some((option) => option.code === draft.cms_locale && !option.installed) && (
              <AdminAlert tone="warning" className="mt-2 max-w-xl" data-testid="cms-locale-fallback">{t('general.cms_fallback')}</AdminAlert>
            )}
          </div>
          {preview && (
            <div className="max-w-xl rounded-lg border border-zinc-200 bg-zinc-50 p-3.5 text-sm text-zinc-700" data-testid="timezone-preview">
              <h3 className="text-xs font-semibold uppercase tracking-wider text-zinc-500">{t('general.timezone_preview')}</h3>
              <p className="mt-1.5">{t('general.winter', { time: preview.winter.text })}</p>
              <p className="mt-1">{t('general.summer', { time: preview.summer.text })}</p>
              {(preview.winter.fallback || preview.summer.fallback) && (
                <p role="status" className="mt-2 text-amber-800">
                  {t('general.timezone_fallback')}
                </p>
              )}
            </div>
          )}
          <div className="flex flex-wrap items-center gap-2.5 pt-1">
            <AdminButton
              variant="primary"
              type="submit"
              disabled={!dirty || !csrfToken || saving || stale}
            >
              {saving ? t('general.saving') : t('general.save')}
            </AdminButton>
            <AdminButton variant="secondary" disabled={saving} onClick={() => void load()}>
              {t('general.reload')}
            </AdminButton>
            {dirty && <span className="text-xs font-medium text-amber-800" role="status">{t('general.dirty')}</span>}
          </div>
        </form>
      )}
      <PublicSiteSettingsSection
        csrfToken={csrfToken}
        onApiError={onApiError}
        localeState={localeState}
        onLocalesChanged={onLocalesChanged}
      />
      <a href="#/settings/multilingual" className="text-sm text-od-accent underline">{t('general.multilingual_link')}</a>
    </section>
  );
}

function PublicSiteSettingsSection({
  csrfToken,
  onApiError,
  localeState = EMPTY_ADMIN_LOCALE_STATE,
  onLocalesChanged,
}: Props) {
  const { t } = useI18n();
  const contentLanguageLocked = localeState.multilingualEnabled || localeState.hasStoredSecondaryLocales;
  const onApiErrorRef = useRef(onApiError);
  onApiErrorRef.current = onApiError;
  const [saved, setSaved] = useState<PublicSiteSettings | null>(null);
  const [draft, setDraft] = useState<PublicSiteSettingsDraft | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [stale, setStale] = useState(false);

  const load = useCallback(async (signal?: AbortSignal) => {
    setLoading(true);
    setError(null);
    setNotice(null);
    try {
      const settings = await getPublicSiteSettings(signal);
      if (signal?.aborted) return;
      setSaved(settings);
      setDraft({
        public_origin: settings.public_origin,
        content_language: settings.content_language,
        indexing_enabled: settings.indexing_enabled,
      });
      setStale(false);
    } catch (reason) {
      if (!signal?.aborted) {
        setError(messageFor(reason));
        onApiErrorRef.current?.(reason);
      }
    } finally {
      if (!signal?.aborted) setLoading(false);
    }
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    void load(controller.signal);
    return () => controller.abort();
  }, [load]);

  const normalizedDraftOrigin = draft?.public_origin?.trim() ? draft.public_origin.trim() : null;
  const dirty = Boolean(saved && draft && (
    normalizedDraftOrigin !== saved.public_origin ||
    draft.content_language !== saved.content_language ||
    draft.indexing_enabled !== saved.indexing_enabled
  ));

  async function handleSave(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!saved || !draft || !csrfToken || saving) return;
    setSaving(true);
    setError(null);
    setNotice(null);
    const payload: PublicSiteSettingsDraft = {
      public_origin: draft.public_origin === null || draft.public_origin === '' ? null : draft.public_origin,
      content_language: draft.content_language,
      indexing_enabled: draft.indexing_enabled,
    };
    try {
      const updated = await savePublicSiteSettings(payload, saved.version, csrfToken);
      setSaved(updated);
      setDraft({
        public_origin: updated.public_origin,
        content_language: updated.content_language,
        indexing_enabled: updated.indexing_enabled,
      });
      setStale(false);
      setNotice('saved');
      if (!contentLanguageLocked) {
        await onLocalesChanged?.();
      }
    } catch (reason) {
      if (reason instanceof ApiClientError && reason.status === 412) {
        setStale(true);
        setError(t('general.public_stale'));
      } else {
        setError(messageFor(reason));
      }
      onApiErrorRef.current?.(reason);
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="border-t border-slate-200 pt-6 space-y-5" data-testid="public-settings">
      <div>
        <h3 className="text-lg font-semibold text-slate-900">{t('general.public_heading')}</h3>
        <p className="mt-1 text-sm text-slate-600">
          {t('general.public_description')}
        </p>
      </div>
      {loading && <p role="status" className="text-sm text-zinc-500">{t('general.public_loading')}</p>}
      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      {notice && <AdminAlert tone="success">{t('general.public_saved')}</AdminAlert>}
      {!loading && (!draft || !saved) && (
        <AdminButton variant="primary" onClick={() => void load()}>
          {t('general.public_retry')}
        </AdminButton>
      )}
      {!loading && draft && saved && (
        <form className="space-y-4" onSubmit={(event) => void handleSave(event)}>
          {!saved.public_origin && (
            <AdminAlert tone="warning" className="max-w-xl" data-testid="missing-origin-prompt">
              {t('general.origin_missing')}
            </AdminAlert>
          )}
          <div>
            <label htmlFor="od-public-origin" className="mb-1 block text-sm font-medium text-zinc-800">{t('general.origin')}</label>
            <input
              id="od-public-origin" name="public_origin" type="text" placeholder="https://example.com" disabled={saving}
              className={FIELD_CLASS}
              value={draft.public_origin ?? ''}
              onChange={(event) => setDraft({ ...draft, public_origin: event.target.value === '' ? null : event.target.value })}
              aria-describedby="od-public-origin-help"
            />
            <p id="od-public-origin-help" className="mt-1 text-xs text-zinc-500">{t('general.origin_help')}</p>
          </div>
          <div>
            <label htmlFor="od-content-language" className="mb-1 block text-sm font-medium text-zinc-800">{t('general.content_language')}</label>
            <input
              id="od-content-language" name="content_language" type="text" required maxLength={35}
              className={FIELD_CLASS}
              disabled={contentLanguageLocked || saving}
              value={draft.content_language}
              onChange={(event) => setDraft({ ...draft, content_language: event.target.value })}
              aria-describedby="od-content-language-help"
            />
            <p id="od-content-language-help" className="mt-1 text-xs text-zinc-500">
              {contentLanguageLocked
                ? t('general.content_locked')
                : t('general.content_help')}
            </p>
          </div>
          <div className="flex items-start gap-2.5">
            <input
              id="od-indexing-enabled" name="indexing_enabled" type="checkbox" disabled={saving}
              className="mt-0.5 h-4 w-4 rounded border-zinc-300"
              checked={draft.indexing_enabled}
              onChange={(event) => setDraft({ ...draft, indexing_enabled: event.target.checked })}
            />
            <div>
              <label htmlFor="od-indexing-enabled" className="text-sm font-medium text-zinc-800">{t('general.indexing')}</label>
              <p className="text-xs text-zinc-500">{t('general.indexing_help')}</p>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2.5 pt-1">
            <AdminButton
              variant="primary"
              type="submit"
              disabled={!dirty || !csrfToken || saving || stale}
            >
              {saving ? t('general.saving') : t('general.public_save')}
            </AdminButton>
            <AdminButton variant="secondary" disabled={saving} onClick={() => void load()}>
              {t('general.public_reload')}
            </AdminButton>
            {dirty && <span className="text-xs font-medium text-amber-800" role="status">{t('general.dirty')}</span>}
          </div>
        </form>
      )}
    </div>
  );
}
