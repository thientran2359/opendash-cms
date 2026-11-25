import { useCallback, useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react';
import { ApiClientError, type PermalinkConflictItem } from '../../api/client.ts';
import {
  createLocale, deleteLocale, listLocales, listOrphanedLocales, purgeOrphanedLocale,
  setDefaultLocale, setMultilingualEnabled, updateLocale,
  type AdminLocaleState, type LocaleDirection, type SiteLocale, type SiteLocalesSnapshot,
} from '../../api/locales.ts';
import {
  AdminAlert,
  AdminBadge,
  AdminButton,
  AdminTabPanel,
  AdminTabs,
} from '../../components/AdminPrimitives.tsx';

type MultilingualTabId = 'languages' | 'orphans';
import { useI18n, tStatic, getEffectiveBcp47 } from '../../i18n/index.tsx';

interface Props {
  csrfToken: string | null;
  localeState: AdminLocaleState;
  onLocalesChanged: () => Promise<void>;
  onApiError: (error: unknown) => void;
}
type Orphans = Awaited<ReturnType<typeof listOrphanedLocales>>;
type Modal = { kind: 'disable' } | { kind: 'delete' | 'purge'; code: string };
type Edit = { code: string; label: string; direction: LocaleDirection };
const FIELD = 'h-9 rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 disabled:opacity-50';
const CELL = 'px-3 py-2';

function messageFor(error: unknown) {
  return error instanceof Error ? error.message : tStatic('multilingual.error');
}

function ConfirmDialog({ testId, title, busy, onCancel, children }: {
  testId: string; title: string; busy: boolean; onCancel: () => void; children: ReactNode;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const dialog = ref.current;
    dialog?.showModal();
    return () => { if (dialog?.open) dialog.close(); };
  }, []);
  return (
    <dialog ref={ref} data-testid={testId} aria-labelledby="locale-confirm-title"
      className="m-auto max-w-md rounded-lg border border-zinc-200 bg-white p-5 shadow-xl backdrop:bg-black/40"
      onCancel={(event) => { event.preventDefault(); if (!busy) onCancel(); }}>
      <h3 id="locale-confirm-title" className="mb-3 text-base font-semibold">{title}</h3>
      {children}
    </dialog>
  );
}

export function MultilingualSettingsPage({ csrfToken, localeState, onLocalesChanged, onApiError }: Props) {
  const { t } = useI18n();
  const [snap, setSnap] = useState<SiteLocalesSnapshot | null>(null);
  const [orphans, setOrphans] = useState<Orphans | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [stale, setStale] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [conflicts, setConflicts] = useState<PermalinkConflictItem[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const [showAdd, setShowAdd] = useState(false);
  const [newCode, setNewCode] = useState('');
  const [newLabel, setNewLabel] = useState('');
  const [newDirection, setNewDirection] = useState<LocaleDirection>('ltr');
  const [edit, setEdit] = useState<Edit | null>(null);
  const [modal, setModal] = useState<Modal | null>(null);
  const [confirmCode, setConfirmCode] = useState('');
  const [activeTab, setActiveTab] = useState<MultilingualTabId>('languages');
  const onApiErrorRef = useRef(onApiError);
  onApiErrorRef.current = onApiError;
  const onLocalesChangedRef = useRef(onLocalesChanged);
  onLocalesChangedRef.current = onLocalesChanged;
  const mutationRef = useRef(false);
  const mountedRef = useRef(false);

  const loadOrphans = useCallback(async (signal?: AbortSignal) => {
    const data = await listOrphanedLocales(signal);
    if (mountedRef.current && !signal?.aborted) setOrphans(data);
    return data;
  }, []);

  const load = useCallback(async (signal?: AbortSignal, syncParent = false) => {
    const reportError = onApiErrorRef.current;
    const refreshParent = onLocalesChangedRef.current;
    setLoading(true);
    setError(null);
    setConflicts([]);
    try {
      const [locales, orphanData] = await Promise.all([listLocales(signal), loadOrphans(signal)]);
      if (!mountedRef.current || signal?.aborted) return;
      setSnap(locales);
      if (syncParent) await refreshParent();
      if (!mountedRef.current || signal?.aborted) return;
      setStale(locales.version !== orphanData.version);
    } catch (reason) {
      if (mountedRef.current && !signal?.aborted) {
        setStale(true);
        setError(messageFor(reason));
        reportError(reason);
      }
    } finally {
      if (mountedRef.current && !signal?.aborted) setLoading(false);
    }
  }, [loadOrphans]);

  useEffect(() => {
    mountedRef.current = true;
    const controller = new AbortController();
    void load(controller.signal);
    return () => { mountedRef.current = false; controller.abort(); };
  }, [load]);

  const enabled = snap?.multilingual_enabled ?? localeState.multilingualEnabled;
  const editUnavailable = !!edit && (!enabled || !snap?.items.some((item) => item.code === edit.code));
  const addUnavailable = showAdd && !enabled;
  const blocked = !csrfToken || !snap || !orphans || busy || loading || stale || snap.version !== orphans.version;
  const formatCount = (count: number) => new Intl.NumberFormat(getEffectiveBcp47()).format(count);

  // All three blocks share site_locales_state.version. Serialize writes and
  // refresh both snapshots and the shell before accepting another mutation.
  async function mutate(run: (version: number, token: string) => Promise<void>) {
    if (blocked || mutationRef.current || !snap || !csrfToken) return;
    const reportError = onApiErrorRef.current;
    const refreshParent = onLocalesChangedRef.current;
    mutationRef.current = true;
    setBusy(true);
    setError(null);
    setConflicts([]);
    setNotice(null);
    try {
      await run(snap.version, csrfToken);
      // Even if the user navigated away, the committed mutation must refresh
      // the parent locale state for the originating authenticated session.
      if (!mountedRef.current) { await refreshParent(); return; }
      try {
        const [locales, orphanData] = await Promise.all([
          listLocales(), loadOrphans(), refreshParent(),
        ]);
        if (!mountedRef.current) return;
        setSnap(locales);
        setStale(locales.version !== orphanData.version);
      } catch (reason) {
        if (!mountedRef.current) return;
        setStale(true);
        setError(messageFor(reason));
        reportError(reason);
      }
    } catch (reason) {
      if (!mountedRef.current) return;
      if (reason instanceof ApiClientError) {
        if (reason.status === 412) setStale(true);
        if (reason.conflicts?.length) setConflicts(reason.conflicts);
      }
      setError(messageFor(reason));
      reportError(reason);
    } finally {
      mutationRef.current = false;
      if (mountedRef.current) setBusy(false);
    }
  }

  function openModal(next: Modal) {
    setError(null);
    setConflicts([]);
    setConfirmCode('');
    setModal(next);
  }
  function closeModal() {
    setModal(null);
    setConfirmCode('');
  }
  function toggle(next: boolean) {
    if (!next) { openModal({ kind: 'disable' }); return; }
    void mutate(async (version, token) => {
      setSnap(await setMultilingualEnabled(true, version, token));
      setNotice(t('multilingual.enabled_notice'));
    });
  }
  function add(event: FormEvent) {
    event.preventDefault();
    if (!enabled) return;
    void mutate(async (version, token) => {
      setSnap(await createLocale({ code: newCode.trim(), label: newLabel.trim(), direction: newDirection }, version, token));
      setShowAdd(false);
      setNewCode('');
      setNewLabel('');
      setNewDirection('ltr');
      setNotice(t('multilingual.added'));
    });
  }
  function saveEdit(item: SiteLocale) {
    if (!enabled || edit?.code !== item.code || !edit.label.trim()) return;
    void mutate(async (version, token) => {
      setSnap(await updateLocale(item.code, { label: edit.label.trim(), direction: edit.direction, enabled: item.enabled, position: item.position }, version, token));
      setEdit(null);
      setNotice(t('multilingual.updated'));
    });
  }
  function toggleLocale(item: SiteLocale) {
    if (!enabled || item.is_default) return;
    void mutate(async (version, token) => {
      setSnap(await updateLocale(item.code, { label: item.label, direction: item.direction, enabled: !item.enabled, position: item.position }, version, token));
      setNotice(t('multilingual.updated'));
    });
  }
  function makeDefault(item: SiteLocale) {
    if (!enabled || item.is_default || !item.enabled) return;
    void mutate(async (version, token) => {
      setSnap(await setDefaultLocale(item.code, version, token));
      setNotice(t('multilingual.updated'));
    });
  }
  function confirmModal() {
    if (!modal) return;
    const target = modal;
    const normalizedConfirm = confirmCode.trim().toLowerCase();
    if (target.kind === 'purge' && normalizedConfirm !== target.code) return;
    void mutate(async (version, token) => {
      if (target.kind === 'disable') {
        setSnap(await setMultilingualEnabled(false, version, token));
        setNotice(t('multilingual.disabled_notice'));
      } else if (target.kind === 'delete') {
        const result = await deleteLocale(target.code, version, token);
        setSnap(result.snapshot);
        setNotice(result.soft_deleted
          ? t('multilingual.soft_deleted', { code: result.deleted_code, count: result.orphan_total })
          : t('multilingual.deleted', { code: result.deleted_code }));
      } else {
        const result = await purgeOrphanedLocale(target.code, normalizedConfirm, version, token);
        setOrphans(result.snapshot);
        setNotice(t('multilingual.purged', { code: result.purged_code }));
      }
      closeModal();
    });
  }

  const conflictPanel = conflicts.length > 0 && (
    <AdminAlert tone="error" data-testid="locale-conflicts">
      <p className="font-semibold">{t('multilingual.conflicts')}</p>
      <table className="mt-2 w-full text-left text-xs">
        <thead><tr><th>{t('multilingual.path')}</th><th>{t('multilingual.new_target')}</th><th>{t('multilingual.existing_target')}</th></tr></thead>
        <tbody>{conflicts.map((item, i) => (
          <tr key={item.path + '-' + i}><td><code>{item.path}</code></td><td>{item.new_target}</td><td>{item.existing_target}</td></tr>
        ))}</tbody>
      </table>
    </AdminAlert>
  );
  const stalePanel = stale && (
    <AdminAlert tone="warning" data-testid="locale-stale">
      {t('multilingual.stale')}{' '}
      <AdminButton size="sm" disabled={busy || loading} onClick={() => { closeModal(); void load(undefined, true); }}>{t('multilingual.reload')}</AdminButton>
    </AdminAlert>
  );

  const orphanCount = orphans?.items.length ?? 0;

  return (
    <section className="min-w-0 space-y-6">
      <div>
        <h2 className="text-base font-semibold text-zinc-950">{t('multilingual.heading')}</h2>
        <p className="mt-1 text-sm text-zinc-600">{t('multilingual.description')}</p>
      </div>
      {loading && <p role="status" className="text-sm text-zinc-500">{t('multilingual.loading')}</p>}
      {!modal && error && <AdminAlert tone="error">{error}</AdminAlert>}
      {!modal && conflictPanel}
      {!modal && stalePanel}
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
      <AdminTabs
        tabs={[
          {
            id: 'languages',
            label: t('multilingual.tab_languages'),
            badge: snap ? snap.items.length : undefined,
            dirty: Boolean(edit || showAdd),
          },
          {
            id: 'orphans',
            label: t('multilingual.tab_orphans'),
            badge: orphans ? orphanCount : undefined,
            badgeTone: orphanCount > 0 ? 'warning' : 'neutral',
          },
        ]}
        activeId={activeTab}
        onChange={setActiveTab}
        ariaLabel={t('multilingual.tabs_label')}
        testId="multilingual-tabs"
        idPrefix="multilingual"
      />
      <AdminTabPanel
        active={activeTab === 'languages'}
        id="multilingual-panel-languages"
        labelledBy="multilingual-tab-languages"
        className="space-y-6"
      >
        <div data-testid="multilingual-toggle-section" className="space-y-3">
          <h3 className="text-lg font-semibold">{t('multilingual.support')}</h3>
          <label className="flex items-center gap-2 text-sm font-medium">
            <input type="checkbox" data-testid="multilingual-enabled-checkbox" checked={enabled}
              disabled={blocked || !!edit || showAdd} onChange={(event) => toggle(event.target.checked)} />
            {t('multilingual.enable')}
          </label>
          <p className="text-sm text-zinc-600">{t('multilingual.toggle_help')}</p>
        </div>
        <div data-testid="content-locales-section" className="space-y-4 border-t border-zinc-200 pt-6">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <h3 className="text-lg font-semibold">{t('multilingual.locales')}</h3>
              <p className="mt-1 text-sm text-zinc-600">{t('multilingual.locales_help', { max: snap?.max_locales ?? localeState.maxLocales })}</p>
            </div>
            {enabled && snap && snap.items.length < snap.max_locales && !showAdd && (
              <AdminButton size="sm" data-testid="add-locale-btn" disabled={blocked || !!edit} onClick={() => setShowAdd(true)}>
                {t('multilingual.add')}
              </AdminButton>
            )}
          </div>
          {!enabled && <AdminAlert tone="info">{t('multilingual.disabled_help')}</AdminAlert>}
          {(editUnavailable || addUnavailable) && (
            <AdminAlert tone="warning">
              <p>{t('multilingual.draft_unavailable')}</p>
              <p className="mt-1 break-all">{editUnavailable ? edit?.label : newCode + ' / ' + newLabel}</p>
              <AdminButton size="sm" disabled={busy} onClick={() => { setEdit(null); setShowAdd(false); }}>{t('multilingual.cancel')}</AdminButton>
            </AdminAlert>
          )}
          {snap && (
            <div className="overflow-x-auto rounded-lg border border-zinc-200">
              <table className="w-full text-left text-sm">
                <thead className="border-b border-zinc-200 bg-zinc-50 text-xs font-semibold text-zinc-600">
                  <tr><th className={CELL}>{t('multilingual.label')}</th><th className={CELL}>{t('multilingual.code')}</th>
                    <th className={CELL}>{t('multilingual.direction')}</th><th className={CELL}>{t('multilingual.status')}</th><th className={CELL}>{t('multilingual.actions')}</th></tr>
                </thead>
                <tbody className="divide-y divide-zinc-200">
                  {snap.items.map((item) => (
                    <tr key={item.code} data-testid={'locale-row-' + item.code}>
                      <td className={CELL}>{edit?.code === item.code && enabled ? (
                        <input className={FIELD} aria-label={t('multilingual.label')} data-testid={'edit-locale-label-' + item.code}
                          disabled={busy} required maxLength={80} value={edit.label} onChange={(event) => setEdit({ ...edit, label: event.target.value })} />
                      ) : <>{item.label}{' '}{item.is_default && <AdminBadge tone="success">{t('multilingual.default')}</AdminBadge>}</>}</td>
                      <td className={CELL}><code>{item.bcp47_tag}</code></td>
                      <td className={CELL}>{edit?.code === item.code && enabled ? (
                        <select className={FIELD} aria-label={t('multilingual.direction')} data-testid={'edit-locale-direction-' + item.code}
                          disabled={busy} value={edit.direction} onChange={(event) => setEdit({ ...edit, direction: event.target.value as LocaleDirection })}>
                          <option value="ltr">{t('multilingual.ltr')}</option><option value="rtl">{t('multilingual.rtl')}</option>
                        </select>
                      ) : item.direction === 'ltr' ? t('multilingual.ltr') : t('multilingual.rtl')}</td>
                      <td className={CELL}><AdminBadge tone={item.enabled ? 'success' : 'neutral'}>{item.enabled ? t('multilingual.active') : t('multilingual.inactive')}</AdminBadge></td>
                      <td className={CELL}><div className="flex flex-wrap gap-1.5">
                        {edit?.code === item.code && enabled ? (
                          <>
                            <AdminButton size="sm" variant="primary" data-testid={'save-locale-' + item.code} disabled={blocked || !edit.label.trim()} onClick={() => saveEdit(item)}>{t('multilingual.save')}</AdminButton>
                            <AdminButton size="sm" data-testid={'cancel-edit-locale-' + item.code} disabled={busy} onClick={() => setEdit(null)}>{t('multilingual.cancel')}</AdminButton>
                          </>
                        ) : (
                          <>
                            {enabled && <AdminButton size="sm" data-testid={'edit-locale-' + item.code} disabled={blocked || !!edit || showAdd} onClick={() => setEdit({ code: item.code, label: item.label, direction: item.direction })}>{t('multilingual.edit')}</AdminButton>}
                            {enabled && !item.is_default && item.enabled && <AdminButton size="sm" data-testid={'set-default-locale-' + item.code} disabled={blocked || !!edit || showAdd} onClick={() => makeDefault(item)}>{t('multilingual.set_default')}</AdminButton>}
                            {enabled && !item.is_default && <AdminButton size="sm" data-testid={'toggle-locale-' + item.code} disabled={blocked || !!edit || showAdd} onClick={() => toggleLocale(item)}>{item.enabled ? t('multilingual.disable') : t('multilingual.reenable')}</AdminButton>}
                            {!item.is_default && <AdminButton variant="danger" size="sm" data-testid={'delete-locale-' + item.code} disabled={blocked || !!edit || showAdd} onClick={() => openModal({ kind: 'delete', code: item.code })}>{t('multilingual.delete')}</AdminButton>}
                          </>
                        )}
                      </div></td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          {showAdd && enabled && (
            <form data-testid="add-locale-form" className="max-w-xl space-y-3 rounded-lg border border-zinc-200 bg-zinc-50 p-4" onSubmit={add}>
              <h4 className="text-sm font-semibold">{t('multilingual.add')}</h4>
              <div className="grid gap-3 sm:grid-cols-3">
                <label className="flex flex-col gap-1 text-xs">{t('multilingual.code')}
                  <input className={FIELD} data-testid="new-locale-code" required maxLength={35} disabled={busy} value={newCode} onChange={(event) => setNewCode(event.target.value)} />
                </label>
                <label className="flex flex-col gap-1 text-xs">{t('multilingual.label')}
                  <input className={FIELD} data-testid="new-locale-label" required maxLength={80} disabled={busy} value={newLabel} onChange={(event) => setNewLabel(event.target.value)} />
                </label>
                <label className="flex flex-col gap-1 text-xs">{t('multilingual.direction')}
                  <select className={FIELD} data-testid="new-locale-direction" disabled={busy} value={newDirection} onChange={(event) => setNewDirection(event.target.value as LocaleDirection)}>
                    <option value="ltr">{t('multilingual.ltr')}</option><option value="rtl">{t('multilingual.rtl')}</option>
                  </select>
                </label>
              </div>
              <div className="flex gap-2">
                <AdminButton size="sm" variant="primary" type="submit" disabled={blocked}>{t('multilingual.add')}</AdminButton>
                <AdminButton size="sm" disabled={busy} onClick={() => setShowAdd(false)}>{t('multilingual.cancel')}</AdminButton>
              </div>
            </form>
          )}
        </div>
      </AdminTabPanel>
      <AdminTabPanel
        active={activeTab === 'orphans'}
        id="multilingual-panel-orphans"
        labelledBy="multilingual-tab-orphans"
        className="space-y-4"
      >
        <div data-testid="orphaned-locales-section" className="space-y-4">
          <h3 className="text-lg font-semibold">{t('multilingual.orphans')}</h3>
          <p className="text-sm text-zinc-600">{t('multilingual.orphans_help')}</p>
          {orphans && !orphans.items.length && <p className="text-sm text-zinc-500">{t('multilingual.no_orphans')}</p>}
          {!!orphans?.items.length && (
            <div className="overflow-x-auto rounded-lg border border-zinc-200">
              <table className="w-full text-left text-sm">
                <thead className="border-b border-zinc-200 bg-zinc-50 text-xs font-semibold text-zinc-600">
                  <tr><th className={CELL}>{t('multilingual.code')}</th><th className={CELL}>{t('multilingual.entries')}</th>
                    <th className={CELL}>{t('multilingual.terms')}</th><th className={CELL}>{t('multilingual.settings')}</th>
                    <th className={CELL}>{t('multilingual.menu_slots')}</th><th className={CELL}>{t('multilingual.routes')}</th>
                    <th className={CELL}>{t('multilingual.projections')}</th><th className={CELL}>{t('multilingual.media_seo_refs')}</th>
                    <th className={CELL}>{t('multilingual.total')}</th><th className={CELL}>{t('multilingual.actions')}</th></tr>
                </thead>
                <tbody className="divide-y divide-zinc-200">
                  {orphans.items.map((item) => (
                    <tr key={item.code} data-testid={'orphan-locale-row-' + item.code}>
                      <td className={CELL}>{item.label} (<code>{item.code}</code>)</td>
                      <td className={CELL}>{formatCount(item.counts.entries)}</td><td className={CELL}>{formatCount(item.counts.terms)}</td>
                      <td className={CELL}>{formatCount(item.counts.settings)}</td><td className={CELL}>{formatCount(item.counts.menu_slots)}</td>
                      <td className={CELL}>{formatCount(item.counts.routes)}</td><td className={CELL}>{formatCount(item.counts.projections)}</td>
                      <td className={CELL}>{formatCount(item.counts.media_seo_refs)}</td><td className={CELL}>{formatCount(item.counts.total)}</td>
                      <td className={CELL}><AdminButton variant="danger" size="sm" data-testid={'purge-orphan-' + item.code}
                        disabled={blocked || !!edit || showAdd} onClick={() => openModal({ kind: 'purge', code: item.code })}>{t('multilingual.purge')}</AdminButton></td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </AdminTabPanel>
      {modal && (
        <ConfirmDialog testId={modal.kind === 'disable' ? 'disable-multilingual-modal' : modal.kind === 'delete' ? 'delete-locale-modal' : 'purge-orphan-modal'}
          title={modal.kind === 'disable' ? t('multilingual.confirm_disable') : modal.kind === 'delete' ? t('multilingual.confirm_delete') : t('multilingual.confirm_purge')}
          busy={busy} onCancel={closeModal}>
          <div className="space-y-3">
            <p className="text-sm text-zinc-700">{modal.kind === 'disable' ? t('multilingual.disable_help') : modal.kind === 'delete'
              ? t('multilingual.delete_help', { code: modal.code }) : t('multilingual.purge_help', { code: modal.code })}</p>
            {error && <AdminAlert tone="error">{error}</AdminAlert>}
            {conflictPanel}
            {stalePanel}
            {modal.kind === 'purge' && <label className="flex flex-col gap-1 text-sm">{t('multilingual.type_code', { code: modal.code })}
              <input className={FIELD} data-testid="purge-confirm-input" autoComplete="off" disabled={busy} value={confirmCode} onChange={(event) => setConfirmCode(event.target.value)} />
            </label>}
            <div className="flex flex-wrap justify-end gap-2">
              <AdminButton autoFocus disabled={busy} data-testid={modal.kind === 'disable' ? 'cancel-disable-multilingual' : modal.kind === 'delete' ? 'cancel-delete-locale' : 'cancel-purge-orphan'} onClick={closeModal}>{t('multilingual.cancel')}</AdminButton>
              <AdminButton variant="danger" disabled={blocked || (modal.kind === 'purge' && confirmCode.trim().toLowerCase() !== modal.code)}
                data-testid={modal.kind === 'disable' ? 'confirm-disable-multilingual' : modal.kind === 'delete' ? 'confirm-delete-locale' : 'purge-confirm-submit'} onClick={confirmModal}>
                {modal.kind === 'disable' ? t('multilingual.disable') : modal.kind === 'delete' ? t('multilingual.delete') : t('multilingual.purge')}
              </AdminButton>
            </div>
          </div>
        </ConfirmDialog>
      )}
    </section>
  );
}
