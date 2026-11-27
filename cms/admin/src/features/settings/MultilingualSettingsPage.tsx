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

import { LanguagePacksPanel } from './LanguagePacksPanel.tsx';

type MultilingualTabId = 'languages' | 'orphans' | 'packs';
import { useI18n, __, _x, getEffectiveBcp47 } from '../../i18n/index.tsx';

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
  return error instanceof Error ? error.message : _x("Unable to connect to the server.", "multilingual.error");
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
  const { __, _x } = useI18n();
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
      setNotice(__("Multilingual support enabled."));
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
      setNotice(__("Content language added."));
    });
  }
  function saveEdit(item: SiteLocale) {
    if (!enabled || edit?.code !== item.code || !edit.label.trim()) return;
    void mutate(async (version, token) => {
      setSnap(await updateLocale(item.code, { label: edit.label.trim(), direction: edit.direction, enabled: item.enabled, position: item.position }, version, token));
      setEdit(null);
      setNotice(__("Content languages updated."));
    });
  }
  function toggleLocale(item: SiteLocale) {
    if (!enabled || item.is_default) return;
    void mutate(async (version, token) => {
      setSnap(await updateLocale(item.code, { label: item.label, direction: item.direction, enabled: !item.enabled, position: item.position }, version, token));
      setNotice(__("Content languages updated."));
    });
  }
  function makeDefault(item: SiteLocale) {
    if (!enabled || item.is_default || !item.enabled) return;
    void mutate(async (version, token) => {
      setSnap(await setDefaultLocale(item.code, version, token));
      setNotice(__("Content languages updated."));
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
        setNotice(__("Multilingual support disabled. Existing translations are preserved."));
      } else if (target.kind === 'delete') {
        const result = await deleteLocale(target.code, version, token);
        setSnap(result.snapshot);
        setNotice(result.soft_deleted
          ? __("Removed {{code}}; {{count}} data records are retained for recovery.", { code: result.deleted_code, count: result.orphan_total })
          : __("Removed {{code}}.", { code: result.deleted_code }));
      } else {
        const result = await purgeOrphanedLocale(target.code, normalizedConfirm, version, token);
        setOrphans(result.snapshot);
        setNotice(__("Permanently deleted retained data for {{code}}.", { code: result.purged_code }));
      }
      closeModal();
    });
  }

  const conflictPanel = conflicts.length > 0 && (
    <AdminAlert tone="error" data-testid="locale-conflicts">
      <p className="font-semibold">{__("Public URL conflicts")}</p>
      <table className="mt-2 w-full text-left text-xs">
        <thead><tr><th>{__("Path")}</th><th>{__("New target")}</th><th>{__("Existing target")}</th></tr></thead>
        <tbody>{conflicts.map((item, i) => (
          <tr key={item.path + '-' + i}><td><code>{item.path}</code></td><td>{item.new_target}</td><td>{item.existing_target}</td></tr>
        ))}</tbody>
      </table>
    </AdminAlert>
  );
  const stalePanel = stale && (
    <AdminAlert tone="warning" data-testid="locale-stale">
      {__("Language settings changed. Your inputs are preserved. Reload the latest settings before trying again.")}{' '}
      <AdminButton size="sm" disabled={busy || loading} onClick={() => { closeModal(); void load(undefined, true); }}>{__("Reload")}</AdminButton>
    </AdminAlert>
  );

  const orphanCount = orphans?.items.length ?? 0;

  return (
    <section className="min-w-0 space-y-6">
      <div>
        <h2 className="text-base font-semibold text-zinc-950">{__("Multilingual settings")}</h2>
        <p className="mt-1 text-sm text-zinc-600">{__("Manage public content languages and retained translations.")}</p>
      </div>
      {loading && <p role="status" className="text-sm text-zinc-500">{__("Loading languages…")}</p>}
      {!modal && error && <AdminAlert tone="error">{error}</AdminAlert>}
      {!modal && conflictPanel}
      {!modal && stalePanel}
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
      <AdminTabs
        tabs={[
          {
            id: 'languages',
            label: __("Languages & support"),
            badge: snap ? snap.items.length : undefined,
            dirty: Boolean(edit || showAdd),
          },
          {
            id: 'orphans',
            label: __("Orphaned data"),
            badge: orphans ? orphanCount : undefined,
            badgeTone: orphanCount > 0 ? 'warning' : 'neutral',
          },
          {
            id: 'packs',
            label: __("Language packs"),
          },
        ]}
        activeId={activeTab}
        onChange={setActiveTab}
        ariaLabel={__("Multilingual sections")}
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
          <h3 className="text-lg font-semibold">{__("Multilingual support")}</h3>
          <label className="flex items-center gap-2 text-sm font-medium">
            <input type="checkbox" data-testid="multilingual-enabled-checkbox" checked={enabled}
              disabled={blocked || !!edit || showAdd} onChange={(event) => toggle(event.target.checked)} />
            {__("Enable multilingual content support")}
          </label>
          <p className="text-sm text-zinc-600">{__("When disabled, the site uses only the default content language. Existing translations are retained.")}</p>
        </div>
        <div data-testid="content-locales-section" className="space-y-4 border-t border-zinc-200 pt-6">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <h3 className="text-lg font-semibold">{__("Content languages")}</h3>
              <p className="mt-1 text-sm text-zinc-600">{__("The default language uses the root URL (/); secondary languages use a language prefix. Maximum: {{max}} languages.", { max: snap?.max_locales ?? localeState.maxLocales })}</p>
            </div>
            {enabled && snap && snap.items.length < snap.max_locales && !showAdd && (
              <AdminButton size="sm" data-testid="add-locale-btn" disabled={blocked || !!edit} onClick={() => setShowAdd(true)}>
                {__("Add language")}
              </AdminButton>
            )}
          </div>
          {!enabled && <AdminAlert tone="info">{__("Enable multilingual support to add or edit languages, change the default, or enable and disable individual languages. Secondary languages can still be deleted.")}</AdminAlert>}
          {(editUnavailable || addUnavailable) && (
            <AdminAlert tone="warning">
              <p>{__("This draft can no longer be saved because the language was removed or multilingual support was disabled. Copy your text before closing it.")}</p>
              <p className="mt-1 break-all">{editUnavailable ? edit?.label : newCode + ' / ' + newLabel}</p>
              <AdminButton size="sm" disabled={busy} onClick={() => { setEdit(null); setShowAdd(false); }}>{_x("Cancel", "multilingual.cancel")}</AdminButton>
            </AdminAlert>
          )}
          {snap && (
            <div className="overflow-x-auto rounded-lg border border-zinc-200">
              <table className="w-full text-left text-sm">
                <thead className="border-b border-zinc-200 bg-zinc-50 text-xs font-semibold text-zinc-600">
                  <tr><th className={CELL}>{__("Display name")}</th><th className={CELL}>{__("BCP47 code")}</th>
                    <th className={CELL}>{__("Direction")}</th><th className={CELL}>{__("Status")}</th><th className={CELL}>{__("Actions")}</th></tr>
                </thead>
                <tbody className="divide-y divide-zinc-200">
                  {snap.items.map((item) => (
                    <tr key={item.code} data-testid={'locale-row-' + item.code}>
                      <td className={CELL}>{edit?.code === item.code && enabled ? (
                        <input className={FIELD} aria-label={__("Display name")} data-testid={'edit-locale-label-' + item.code}
                          disabled={busy} required maxLength={80} value={edit.label} onChange={(event) => setEdit({ ...edit, label: event.target.value })} />
                      ) : <>{item.label}{' '}{item.is_default && <AdminBadge tone="success">{__("Default")}</AdminBadge>}</>}</td>
                      <td className={CELL}><code>{item.bcp47_tag}</code></td>
                      <td className={CELL}>{edit?.code === item.code && enabled ? (
                        <select className={FIELD} aria-label={__("Direction")} data-testid={'edit-locale-direction-' + item.code}
                          disabled={busy} value={edit.direction} onChange={(event) => setEdit({ ...edit, direction: event.target.value as LocaleDirection })}>
                          <option value="ltr">{__("Left to right (LTR)")}</option><option value="rtl">{__("Right to left (RTL)")}</option>
                        </select>
                      ) : item.direction === 'ltr' ? __("Left to right (LTR)") : __("Right to left (RTL)")}</td>
                      <td className={CELL}><AdminBadge tone={item.enabled ? 'success' : 'neutral'}>{item.enabled ? __("Enabled") : _x("Disabled", "multilingual.inactive")}</AdminBadge></td>
                      <td className={CELL}><div className="flex flex-wrap gap-1.5">
                        {edit?.code === item.code && enabled ? (
                          <>
                            <AdminButton size="sm" variant="primary" data-testid={'save-locale-' + item.code} disabled={blocked || !edit.label.trim()} onClick={() => saveEdit(item)}>{__("Save")}</AdminButton>
                            <AdminButton size="sm" data-testid={'cancel-edit-locale-' + item.code} disabled={busy} onClick={() => setEdit(null)}>{_x("Cancel", "multilingual.cancel")}</AdminButton>
                          </>
                        ) : (
                          <>
                            {enabled && <AdminButton size="sm" data-testid={'edit-locale-' + item.code} disabled={blocked || !!edit || showAdd} onClick={() => setEdit({ code: item.code, label: item.label, direction: item.direction })}>{_x("Edit", "multilingual.edit")}</AdminButton>}
                            {enabled && !item.is_default && item.enabled && <AdminButton size="sm" data-testid={'set-default-locale-' + item.code} disabled={blocked || !!edit || showAdd} onClick={() => makeDefault(item)}>{__("Set as default")}</AdminButton>}
                            {enabled && !item.is_default && <AdminButton size="sm" data-testid={'toggle-locale-' + item.code} disabled={blocked || !!edit || showAdd} onClick={() => toggleLocale(item)}>{item.enabled ? __("Disable") : __("Enable")}</AdminButton>}
                            {!item.is_default && <AdminButton variant="danger" size="sm" data-testid={'delete-locale-' + item.code} disabled={blocked || !!edit || showAdd} onClick={() => openModal({ kind: 'delete', code: item.code })}>{__("Delete")}</AdminButton>}
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
              <h4 className="text-sm font-semibold">{__("Add language")}</h4>
              <div className="grid gap-3 sm:grid-cols-3">
                <label className="flex flex-col gap-1 text-xs">{__("BCP47 code")}
                  <input className={FIELD} data-testid="new-locale-code" required maxLength={35} disabled={busy} value={newCode} onChange={(event) => setNewCode(event.target.value)} />
                </label>
                <label className="flex flex-col gap-1 text-xs">{__("Display name")}
                  <input className={FIELD} data-testid="new-locale-label" required maxLength={80} disabled={busy} value={newLabel} onChange={(event) => setNewLabel(event.target.value)} />
                </label>
                <label className="flex flex-col gap-1 text-xs">{__("Direction")}
                  <select className={FIELD} data-testid="new-locale-direction" disabled={busy} value={newDirection} onChange={(event) => setNewDirection(event.target.value as LocaleDirection)}>
                    <option value="ltr">{__("Left to right (LTR)")}</option><option value="rtl">{__("Right to left (RTL)")}</option>
                  </select>
                </label>
              </div>
              <div className="flex gap-2">
                <AdminButton size="sm" variant="primary" type="submit" disabled={blocked}>{__("Add language")}</AdminButton>
                <AdminButton size="sm" disabled={busy} onClick={() => setShowAdd(false)}>{_x("Cancel", "multilingual.cancel")}</AdminButton>
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
          <h3 className="text-lg font-semibold">{__("Orphaned locale data")}</h3>
          <p className="text-sm text-zinc-600">{__("Deleted languages with retained data can be restored by adding the same code when multilingual support is enabled. Purging permanently removes the retained data.")}</p>
          {orphans && !orphans.items.length && <p className="text-sm text-zinc-500">{__("No orphaned languages.")}</p>}
          {!!orphans?.items.length && (
            <div className="overflow-x-auto rounded-lg border border-zinc-200">
              <table className="w-full text-left text-sm">
                <thead className="border-b border-zinc-200 bg-zinc-50 text-xs font-semibold text-zinc-600">
                  <tr><th className={CELL}>{__("BCP47 code")}</th><th className={CELL}>{__("Entries")}</th>
                    <th className={CELL}>{_x("Terms", "multilingual.terms")}</th><th className={CELL}>{__("Settings")}</th>
                    <th className={CELL}>{__("Menu slots")}</th><th className={CELL}>{__("Routes")}</th>
                    <th className={CELL}>{__("Projections")}</th><th className={CELL}>{__("Media / SEO references")}</th>
                    <th className={CELL}>{__("Total")}</th><th className={CELL}>{__("Actions")}</th></tr>
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
                        disabled={blocked || !!edit || showAdd} onClick={() => openModal({ kind: 'purge', code: item.code })}>{__("Purge permanently")}</AdminButton></td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </AdminTabPanel>
      <AdminTabPanel
        active={activeTab === 'packs'}
        id="multilingual-panel-packs"
        labelledBy="multilingual-tab-packs"
        className="space-y-4"
      >
        <LanguagePacksPanel csrfToken={csrfToken} onApiError={onApiError} />
      </AdminTabPanel>
      {modal && (
        <ConfirmDialog testId={modal.kind === 'disable' ? 'disable-multilingual-modal' : modal.kind === 'delete' ? 'delete-locale-modal' : 'purge-orphan-modal'}
          title={modal.kind === 'disable' ? __("Disable multilingual support?") : modal.kind === 'delete' ? __("Delete content language?") : __("Permanently purge retained data?")}
          busy={busy} onCancel={closeModal}>
          <div className="space-y-3">
            <p className="text-sm text-zinc-700">{modal.kind === 'disable' ? __("Secondary content languages will no longer be available publicly. Their data will be retained and can be used again when multilingual support is enabled.") : modal.kind === 'delete'
              ? __("Remove {{code}} from the active languages. Existing locale data, if any, will be retained as orphaned data and can be recovered by adding the same code.", { code: modal.code }) : __("This permanently deletes retained data for {{code}}. This action cannot be undone.", { code: modal.code })}</p>
            {error && <AdminAlert tone="error">{error}</AdminAlert>}
            {conflictPanel}
            {stalePanel}
            {modal.kind === 'purge' && <label className="flex flex-col gap-1 text-sm">{__("Type {{code}} to confirm.", { code: modal.code })}
              <input className={FIELD} data-testid="purge-confirm-input" autoComplete="off" disabled={busy} value={confirmCode} onChange={(event) => setConfirmCode(event.target.value)} />
            </label>}
            <div className="flex flex-wrap justify-end gap-2">
              <AdminButton autoFocus disabled={busy} data-testid={modal.kind === 'disable' ? 'cancel-disable-multilingual' : modal.kind === 'delete' ? 'cancel-delete-locale' : 'cancel-purge-orphan'} onClick={closeModal}>{_x("Cancel", "multilingual.cancel")}</AdminButton>
              <AdminButton variant="danger" disabled={blocked || (modal.kind === 'purge' && confirmCode.trim().toLowerCase() !== modal.code)}
                data-testid={modal.kind === 'disable' ? 'confirm-disable-multilingual' : modal.kind === 'delete' ? 'confirm-delete-locale' : 'purge-confirm-submit'} onClick={confirmModal}>
                {modal.kind === 'disable' ? __("Disable") : modal.kind === 'delete' ? __("Delete") : __("Purge permanently")}
              </AdminButton>
            </div>
          </div>
        </ConfirmDialog>
      )}
    </section>
  );
}
