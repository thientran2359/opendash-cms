import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import { useI18n, tStatic } from '../../i18n/index.tsx';
import { ApiClientError } from '../../api/client.ts';
import {
  EMPTY_ADMIN_LOCALE_STATE,
  type AdminLocaleState,
} from '../../api/locales.ts';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';
import { LocaleTabs } from '../../components/LocaleTabs.tsx';
import { queryContentEntryTable } from '../../api/content.ts';
import {
  getReadingSettings,
  saveReadingSettings,
  type ReadingPageStatus,
  type ReadingSettings,
  type ReadingSettingsDraft,
} from '../../api/settings.ts';
import type { AuthUser } from '../../types.ts';

interface Props {
  user: AuthUser | null;
  siteInstanceId: string;
  csrfToken: string | null;
  localeState?: AdminLocaleState;
  onApiError?: (error: unknown) => void;
  onDirtyChange?: (dirty: boolean) => void;
}

type PageChoice = { id: number; title: string };
type PendingDraft = { baseVersion: number; values: ReadingSettingsDraft };

// Session-memory only. Never persist a draft belonging to another site/account.
const pendingDrafts = new Map<string, PendingDraft>();

export function discardReadingDraft(siteId: string, userId: number, locale?: string): void {
  if (locale) {
    pendingDrafts.delete(`${siteId}:${userId}:${locale}`);
    return;
  }
  const prefix = `${siteId}:${userId}:`;
  for (const key of pendingDrafts.keys()) {
    if (key.startsWith(prefix)) pendingDrafts.delete(key);
  }
  pendingDrafts.delete(`${siteId}:${userId}`);
}

export function clearReadingDrafts(): void {
  pendingDrafts.clear();
}

function snapshot(settings: ReadingSettings): ReadingSettingsDraft {
  return { homepage_page_id: settings.homepage_page_id, posts_page_id: settings.posts_page_id };
}

function sameDraft(a: ReadingSettingsDraft, b: ReadingSettingsDraft): boolean {
  return a.homepage_page_id === b.homepage_page_id && a.posts_page_id === b.posts_page_id;
}

function statusLabel(status: ReadingPageStatus): string {
  switch (status) {
    case 'published': return tStatic('reading.published');
    case 'draft': return tStatic('reading.draft');
    case 'deleted': return tStatic('reading.deleted');
    case 'unconfigured': return tStatic('reading.unconfigured');
  }
}

function pageId(value: string): number | null {
  if (value === '') return null;
  const id = Number(value);
  return Number.isSafeInteger(id) && id > 0 ? id : null;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : tStatic('reading.network_error');
}

export function ReadingSettingsPanel({
  user,
  siteInstanceId,
  csrfToken,
  localeState = EMPTY_ADMIN_LOCALE_STATE,
  onApiError,
  onDirtyChange,
}: Props) {
  const { t } = useI18n();
  const actorId = user?.id ?? null;
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
  const draftKey =
    actorId === null ? null : `${siteInstanceId}:${actorId}:${effectiveLocale ?? 'default'}`;
  const [saved, setSaved] = useState<ReadingSettings | null>(null);
  const [draft, setDraft] = useState<ReadingSettingsDraft | null>(null);
  const [baseVersion, setBaseVersion] = useState<number | null>(null);
  const [choices, setChoices] = useState<PageChoice[]>([]);
  const [pageOffset, setPageOffset] = useState(0);
  const [pageTotal, setPageTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [stale, setStale] = useState(false);
  const onErrorRef = useRef(onApiError);
  onErrorRef.current = onApiError;
  const onDirtyRef = useRef(onDirtyChange);
  onDirtyRef.current = onDirtyChange;
  const loadSequence = useRef(0);

  const dirty = Boolean(saved && draft && !sameDraft(draft, snapshot(saved)));

  useEffect(() => {
    onDirtyRef.current?.(dirty);
    if (draftKey && saved && draft && baseVersion !== null) {
      if (dirty) pendingDrafts.set(draftKey, { baseVersion, values: { ...draft } });
      else pendingDrafts.delete(draftKey);
    }
  }, [draftKey, saved, draft, baseVersion, dirty]);

  useEffect(() => () => { onDirtyRef.current?.(false); }, []);

  const load = useCallback(async (signal?: AbortSignal) => {
    if (!draftKey) return;
    const serial = ++loadSequence.current;
    setLoading(true);
    setError(null);
    setNotice(null);
    try {
      const [settings, firstPage] = await Promise.all([
        getReadingSettings(signal, effectiveLocale),
        queryContentEntryTable(
          'page',
          {
            status: 'published',
            ...(effectiveLocale ? { locale: effectiveLocale } : {}),
            sort: 'title',
            order: 'asc',
            limit: 100,
            offset: 0,
          },
          signal,
        ),
      ]);
      if (signal?.aborted || serial !== loadSequence.current) return;
      const pending = pendingDrafts.get(draftKey);
      setSaved(settings);
      setDraft(pending ? { ...pending.values } : snapshot(settings));
      setBaseVersion(pending?.baseVersion ?? settings.version);
      setStale(Boolean(pending && pending.baseVersion !== settings.version));
      setChoices(firstPage.items.map(({ id, title }) => ({ id, title })));
      setPageOffset(firstPage.items.length);
      setPageTotal(firstPage.total);
    } catch (reason) {
      if (!signal?.aborted && serial === loadSequence.current) {
        setError(errorMessage(reason));
        onErrorRef.current?.(reason);
      }
    } finally {
      if (!signal?.aborted && serial === loadSequence.current) setLoading(false);
    }
  }, [draftKey, effectiveLocale]);

  useEffect(() => {
    const controller = new AbortController();
    void load(controller.signal);
    return () => { controller.abort(); loadSequence.current += 1; };
  }, [load]);

  async function loadMorePages() {
    if (loadingMore || pageOffset >= pageTotal) return;
    setLoadingMore(true);
    try {
      const page = await queryContentEntryTable('page', {
        status: 'published',
        ...(effectiveLocale ? { locale: effectiveLocale } : {}),
        sort: 'title',
        order: 'asc',
        limit: 100,
        offset: pageOffset,
      });
      setChoices(previous => {
        const byId = new Map(previous.map(item => [item.id, item]));
        for (const item of page.items) byId.set(item.id, { id: item.id, title: item.title });
        return Array.from(byId.values());
      });
      setPageOffset(pageOffset + page.items.length);
      setPageTotal(page.total);
    } catch (reason) {
      setError(errorMessage(reason));
      onErrorRef.current?.(reason);
    } finally {
      setLoadingMore(false);
    }
  }

  function updateDraft(next: ReadingSettingsDraft) {
    setDraft(next);
    setError(null);
    setNotice(null);
    // This is recorded synchronously so a 401 on the next request cannot erase it.
    if (draftKey && baseVersion !== null) {
      if (saved && sameDraft(next, snapshot(saved))) pendingDrafts.delete(draftKey);
      else pendingDrafts.set(draftKey, { baseVersion, values: { ...next } });
    }
  }

  async function handleSave(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!draft || !saved || !csrfToken || baseVersion === null || saving || stale) return;
    if (draft.homepage_page_id !== null && draft.homepage_page_id === draft.posts_page_id) {
      setError(tStatic('reading.same_page_error'));
      return;
    }
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      const settings = await saveReadingSettings(draft, baseVersion, csrfToken, effectiveLocale);
      pendingDrafts.delete(draftKey ?? '');
      setSaved(settings);
      setDraft(snapshot(settings));
      setBaseVersion(settings.version);
      setStale(false);
      setNotice(tStatic('reading.saved'));
    } catch (reason) {
      if (reason instanceof ApiClientError && reason.status === 412) {
        setStale(true);
        setError(tStatic('reading.stale_error'));
      } else {
        setError(errorMessage(reason));
      }
      onErrorRef.current?.(reason);
    } finally {
      setSaving(false);
    }
  }

  function picker(
    field: keyof ReadingSettingsDraft,
    label: string,
    status: ReadingPageStatus,
    explanation: string,
  ) {
    if (!draft || !saved) return null;
    const selected = draft[field];
    const hasSelected = selected !== null && choices.some(item => item.id === selected);
    const savedId = saved[field];
    return <div className="space-y-1.5">
      <label htmlFor={`reading-${field}`} className="block text-sm font-medium text-zinc-900">{label}</label>
      <p className="text-xs text-zinc-500">{explanation}</p>
      <select
        id={`reading-${field}`}
        className="h-9 w-full max-w-xl rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-od-accent disabled:opacity-60"
        value={selected ?? ''}
        disabled={saving || loading}
        onChange={event => updateDraft({ ...draft, [field]: pageId(event.target.value) })}
      >
        <option value="">{t('reading.no_selection')}</option>
        {!hasSelected && selected !== null && <option value={selected}>{t('reading.missing_page', { id: selected })}</option>}
        {choices.map(item => <option key={item.id} value={item.id}>{item.title || t('reading.untitled')} · #{item.id}</option>)}
      </select>
      <p className="text-xs text-zinc-500" role="status">
        {t('reading.saved_status')} <strong className="text-zinc-800">{statusLabel(status)}</strong>{savedId !== null ? t('reading.saved_id', { id: savedId }) : ''}
      </p>
    </div>;
  }

  return <section className="min-w-0 space-y-5" data-testid="reading-settings" aria-labelledby="reading-heading">
    <div>
      <h2 id="reading-heading" className="text-base font-semibold text-zinc-950">{t('reading.heading')}</h2>
      <p className="mt-1 text-sm text-zinc-600">
        {t('reading.description')}
      </p>
    </div>
    <LocaleTabs
      localeState={localeState}
      activeLocale={effectiveLocale ?? defaultLocaleCode}
      onSelectLocale={setActiveLocale}
      testId="reading-locale-tabs"
    />
    {loading && <p role="status" className="text-sm text-zinc-500">{t('reading.loading_settings')}</p>}
    {error && <AdminAlert tone="error">{error}</AdminAlert>}
    {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
    {!loading && (!saved || !draft) && <AdminButton variant="primary" onClick={() => void load()}>{t('reading.reload')}</AdminButton>}
    {!loading && saved && draft && <form className="space-y-5" onSubmit={event => void handleSave(event)}>
      {picker('homepage_page_id', t('reading.homepage'), saved.homepage_status, t('reading.homepage_help'))}
      {picker('posts_page_id', t('reading.posts_page'), saved.posts_page_status, t('reading.posts_help'))}
      {pageOffset < pageTotal && <AdminButton variant="secondary" size="sm" disabled={loadingMore} onClick={() => void loadMorePages()}>
        {loadingMore ? t('reading.loading') : t('reading.load_more', { offset: pageOffset, total: pageTotal })}
      </AdminButton>}
      {stale && <AdminAlert tone="warning" className="max-w-xl space-y-2">
        <p>{t('reading.stale_warning')}</p>
        <AdminButton variant="secondary" size="sm"
          disabled={saving || saved.version === baseVersion}
          onClick={() => {
          setBaseVersion(saved.version);
          setStale(false);
          if (draftKey) pendingDrafts.set(draftKey, { baseVersion: saved.version, values: { ...draft } });
        }}>{t('reading.use_new_version', { version: saved.version })}</AdminButton>
      </AdminAlert>}
      {draft.homepage_page_id !== null && draft.homepage_page_id === draft.posts_page_id &&
        <AdminAlert tone="error" className="max-w-xl">{t('reading.same_page_warning')}</AdminAlert>}
      <div className="flex flex-wrap items-center gap-2.5 pt-1">
        <AdminButton type="submit" variant="primary"
          disabled={!dirty || !csrfToken || saving || stale || (draft.homepage_page_id !== null && draft.homepage_page_id === draft.posts_page_id)}>
          {saving ? t('reading.saving') : t('reading.save')}
        </AdminButton>
        <AdminButton variant="secondary" disabled={saving} onClick={() => void load()}>{t('reading.refresh')}</AdminButton>
        {dirty && <AdminButton variant="ghost" size="sm" disabled={saving}
          onClick={() => {
            if (!window.confirm(t('reading.discard_confirm'))) return;
            if (draftKey) pendingDrafts.delete(draftKey);
            setDraft(snapshot(saved));
            setBaseVersion(saved.version);
            setStale(false);
            setError(null);
          }}>{t('reading.discard')}</AdminButton>}
        {dirty && <span className="text-xs font-medium text-amber-800" role="status">{t('reading.unsaved')}</span>}
        <span className="text-xs text-zinc-500">{t('reading.version', { version: saved.version })}</span>
      </div>
    </form>}
  </section>;
}
