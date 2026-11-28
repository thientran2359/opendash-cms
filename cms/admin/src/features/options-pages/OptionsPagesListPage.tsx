import React, { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import { ApiClientError } from '../../api/client.ts';
import {
  createOptionsPage,
  deleteOptionsPage,
  listOptionsPages,
  updateOptionsPage,
  type CustomOptionsPageDefinition,
  type CustomOptionsPageInput,
  type OptionsPageParentMenu,
} from '../../api/optionsPages.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';
import { CoreIconSvg } from '../../components/CoreIcon.tsx';
import { CORE_ICON_NAMES } from '../../fields/icons.ts';
import { __ } from '../../i18n/index.tsx';
import type { AuthUser } from '../../types.ts';
import type { FieldRole } from '../content/types.ts';

export interface OptionsPagesListPageProps {
  user: AuthUser | null;
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
  onDirtyChange?: (dirty: boolean) => void;
  onNavigateHash: (hash: string) => void;
  onChanged?: () => void;
}

interface PageFormState {
  page_key: string;
  title: string;
  menu_label: string;
  description: string;
  parent_menu: OptionsPageParentMenu;
  menu_icon: string;
  menu_position: number;
  min_edit_role: FieldRole;
}

const INPUT_CLASS =
  'mt-1 h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50';

function emptyForm(): PageFormState {
  return {
    page_key: '',
    title: '',
    menu_label: '',
    description: '',
    parent_menu: 'settings',
    menu_icon: 'settings',
    menu_position: 50,
    min_edit_role: 'admin',
  };
}

function fromDefinition(def: CustomOptionsPageDefinition): PageFormState {
  return {
    page_key: def.page_key,
    title: def.title,
    menu_label: def.menu_label,
    description: def.description,
    parent_menu: def.parent_menu,
    menu_icon: def.menu_icon ?? '',
    menu_position: def.menu_position,
    min_edit_role: def.min_edit_role,
  };
}

function formKeyFromTitle(title: string): string {
  return title
    .replace(/[\u0111\u0110]/g, 'd')
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 64) || 'options_page';
}

export const OptionsPagesListPage: React.FC<OptionsPagesListPageProps> = ({
  user,
  csrfToken,
  onApiError,
  onDirtyChange,
  onNavigateHash,
  onChanged,
}) => {
  const [pages, setPages] = useState<CustomOptionsPageDefinition[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [activeKey, setActiveKey] = useState<string>('');
  const [menuPosText, setMenuPosText] = useState('50');
  const [form, setForm] = useState<PageFormState>(emptyForm);
  const [baseline, setBaseline] = useState<PageFormState>(emptyForm);
  const [manualKey, setManualKey] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [deleteTarget, setDeleteTarget] = useState<CustomOptionsPageDefinition | null>(null);
  const [purgeValues, setPurgeValues] = useState(false);
  const dialogRef = useRef<HTMLDialogElement | null>(null);
  const cancelBtnRef = useRef<HTMLButtonElement | null>(null);

  const isNew = activeKey === '__new__';
  const activeDef = pages.find(p => p.page_key === activeKey) ?? null;
  const canEditForm =
    user?.role === 'admin' && (isNew || activeDef?.owner.kind === 'user');
  const dirty =
    (isNew && JSON.stringify(form) !== JSON.stringify(emptyForm())) ||
    (!isNew && activeDef !== null && JSON.stringify(form) !== JSON.stringify(baseline));

  useEffect(() => {
    onDirtyChange?.(dirty);
  }, [dirty, onDirtyChange]);
  useEffect(() => () => onDirtyChange?.(false), [onDirtyChange]);

  const reload = useCallback(async (signal?: AbortSignal) => {
    setLoading(true);
    try {
      const list = await listOptionsPages(signal);
      if (signal?.aborted) return;
      setPages(list);
      setError('');
    } catch (cause) {
      if (signal?.aborted) return;
      setError(cause instanceof Error ? cause.message : __("Could not load custom options pages."));
      onApiError(cause, __("Could not load custom options pages."));
    } finally {
      if (!signal?.aborted) setLoading(false);
    }
  }, [onApiError]);

  useEffect(() => {
    const controller = new AbortController();
    void reload(controller.signal);
    return () => controller.abort();
  }, [reload]);

  useEffect(() => {
    const dlg = dialogRef.current;
    if (!dlg) return;
    if (deleteTarget && !dlg.open) {
      dlg.showModal();
      cancelBtnRef.current?.focus();
    } else if (!deleteTarget && dlg.open) {
      dlg.close();
    }
  }, [deleteTarget]);

  const selectItem = (key: string) => {
    if (key === activeKey) return;
    if (dirty && !window.confirm(__("Discard unsaved changes to this options page?"))) return;
    setError('');
    setNotice('');
    setActiveKey(key);
    if (key === '__new__') {
      const fresh = emptyForm();
      setForm(fresh);
      setBaseline(fresh);
      setManualKey(false);
    } else {
      const found = pages.find(p => p.page_key === key);
      const next = found ? fromDefinition(found) : emptyForm();
      setForm(next);
      setBaseline(next);
      setManualKey(true);
    }
  };

  const handleSave = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!csrfToken || !canEditForm || busy) return;
    const key = form.page_key.trim();
    const title = form.title.trim();
    const menuLabel = form.menu_label.trim() || title;
    if (!/^[a-z][a-z0-9_]{0,63}$/.test(key)) {
      setError(__("Options page key must be snake_case (1 to 64 characters)."));
      return;
    }
    if (!title) {
      setError(__("Options page title is required."));
      return;
    }
    const payload: CustomOptionsPageInput = {
      page_key: key,
      title,
      menu_label: menuLabel,
      description: form.description.trim(),
      parent_menu: form.parent_menu,
      menu_icon: form.menu_icon.trim() || null,
      menu_position: Math.max(0, Math.min(1000, Math.round(form.menu_position || 50))),
      min_edit_role: form.min_edit_role,
    };

    setBusy(true);
    setError('');
    setNotice('');
    try {
      const saved = isNew
        ? await createOptionsPage(payload, csrfToken)
        : await updateOptionsPage(activeDef!.page_key, activeDef!.version, payload, csrfToken);
      const nextForm = fromDefinition(saved);
      setPages(prev => [...prev.filter(p => p.page_key !== saved.page_key), saved]);
      setActiveKey(saved.page_key);
      setForm(nextForm);
      setBaseline(nextForm);
      setManualKey(true);
      setNotice(__("Saved options page {{value1}} (v{{value2}}).", { value1: saved.page_key, value2: saved.version }));
      onChanged?.();
    } catch (cause) {
      const msg = cause instanceof ApiClientError && cause.status === 412
        ? __("Options page changed concurrently (412). Reload and try again.")
        : cause instanceof Error
          ? cause.message
          : __("Could not save options page.");
      setError(msg);
      onApiError(cause, __("Could not save options page."));
    } finally {
      setBusy(false);
    }
  };

  const confirmDelete = async () => {
    if (!deleteTarget || !csrfToken || busy) return;
    setBusy(true);
    setError('');
    try {
      await deleteOptionsPage(deleteTarget.page_key, deleteTarget.version, csrfToken, purgeValues);
      setPages(prev => prev.filter(p => p.page_key !== deleteTarget.page_key));
      if (activeKey === deleteTarget.page_key) {
        setActiveKey('');
        setForm(emptyForm());
        setBaseline(emptyForm());
      }
      setNotice(__("Deleted options page {{value1}}.", { value1: deleteTarget.page_key }));
      setDeleteTarget(null);
      setPurgeValues(false);
      onChanged?.();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : __("Could not delete options page."));
      onApiError(cause, __("Could not delete options page."));
      setDeleteTarget(null);
    } finally {
      setBusy(false);
    }
  };

  if (!user) return null;

  return (
    <section className="w-full min-w-0 space-y-5" data-testid="options-pages-list-page">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p className="text-xs font-semibold uppercase tracking-widest text-sky-700">
            {__("Content Structure / Options Pages")}
          </p>
          <h2 className="mt-1 text-2xl font-bold text-zinc-950">{__("Options pages")}</h2>
          <p className="text-sm text-zinc-600">
            {__("Create custom settings screens attached to Admin navigation and bind field groups via location rules.")}
          </p>
        </div>
        <div className="flex gap-2">
          <AdminButton variant="secondary" size="sm" disabled={busy || loading} onClick={() => void reload()}>
            {__("Reload")}
          </AdminButton>
          {user.role === 'admin' && (
            <AdminButton
              variant="primary"
              size="sm"
              disabled={busy || pages.length >= 64}
              data-testid="add-options-page-btn"
              onClick={() => selectItem('__new__')}
            >
              {__("+ Add options page")}
            </AdminButton>
          )}
        </div>
      </header>

      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}

      <div className="grid min-w-0 gap-5 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.25fr)]">
        <div className="space-y-2.5">
          {!loading && pages.length === 0 && (
            <p className="rounded-xl border border-zinc-200 bg-white p-4 text-sm text-zinc-500">
              {__("No custom options pages defined yet.")}
            </p>
          )}
          {pages.map(item => {
            const active = activeKey === item.page_key;
            return (
              <article
                key={item.page_key}
                data-options-page-key={item.page_key}
                className={`rounded-xl border p-3.5 transition-colors ${
                  active ? 'border-sky-500 bg-sky-50/40' : 'border-zinc-200 bg-white'
                }`}
              >
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0 space-y-1">
                    <div className="flex flex-wrap items-center gap-2">
                      {item.menu_icon && <CoreIconSvg name={item.menu_icon} size={16} className="text-zinc-600" />}
                      <h3 className="text-sm font-semibold text-zinc-950">{item.title}</h3>
                      <span className="font-mono text-xs text-zinc-500">custom:{item.page_key}</span>
                    </div>
                    <p className="text-xs text-zinc-500">
                      {__("Menu")}: {item.menu_label} · {__("Parent")}: {item.parent_menu} · {__("Role")}: {item.min_edit_role} · v{item.version}
                    </p>
                    <div className="flex flex-wrap gap-1.5">
                      <AdminBadge tone="neutral">{item.owner.kind}</AdminBadge>
                      {(item.provider_status !== 'active' || item.declaration_state !== 'declared') && (
                        <AdminBadge tone="warning">{item.provider_status}</AdminBadge>
                      )}
                    </div>
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    <AdminButton
                      variant="secondary"
                      size="sm"
                      disabled={busy}
                      onClick={() => onNavigateHash(
                        user.role === 'admin' && item.parent_menu === 'appearance' && item.provider_status === 'active' && item.declaration_state === 'declared'
                          ? `#/studio/options/custom/${encodeURIComponent(item.page_key)}`
                          : `#/options/${encodeURIComponent(item.page_key)}`,
                      )}
                    >
                      {user.role === 'admin' && item.parent_menu === 'appearance' && item.provider_status === 'active' && item.declaration_state === 'declared'
                        ? __("Open in Studio")
                        : __("Open screen")}
                    </AdminButton>
                    <AdminButton
                      variant="secondary"
                      size="sm"
                      disabled={busy}
                      onClick={() => selectItem(item.page_key)}
                    >
                      {__("Configure")}
                    </AdminButton>
                    {item.owner.kind === 'user' && user.role === 'admin' && (
                      <AdminButton
                        variant="danger"
                        size="sm"
                        disabled={busy}
                        onClick={() => {
                          setPurgeValues(false);
                          setDeleteTarget(item);
                        }}
                      >
                        {__("Delete")}
                      </AdminButton>
                    )}
                  </div>
                </div>
              </article>
            );
          })}
        </div>

        <div className="min-w-0">
          {(isNew || activeDef) && (
            <form
              onSubmit={e => void handleSave(e)}
              className="space-y-4 rounded-xl border border-zinc-200 bg-white p-5"
              data-testid="options-page-form"
            >
              <h3 className="text-base font-semibold text-zinc-950">
                {isNew ? __("Create options page") : __("Configure {{value1}}", { value1: activeDef!.page_key })}
              </h3>

              <div className="grid gap-3 sm:grid-cols-2">
                <label htmlFor="opt-page-title" className="block text-xs font-medium text-zinc-700">
                  <span>{__("Page title *")}</span>
                  <input
                    id="opt-page-title"
                    type="text"
                    required
                    maxLength={100}
                    disabled={!canEditForm || busy}
                    className={INPUT_CLASS}
                    value={form.title}
                    onChange={e => {
                      const title = e.target.value;
                      setForm(prev => ({
                        ...prev,
                        title,
                        menu_label: prev.menu_label === prev.title || !prev.menu_label ? title : prev.menu_label,
                        page_key: isNew && !manualKey ? formKeyFromTitle(title) : prev.page_key,
                      }));
                    }}
                  />
                </label>

                <label htmlFor="opt-page-key" className="block text-xs font-medium text-zinc-700">
                  <span>{__("Page key *")}</span>
                  <input
                    id="opt-page-key"
                    type="text"
                    required
                    maxLength={64}
                    pattern="[a-z][a-z0-9_]*"
                    autoCapitalize="none"
                    autoCorrect="off"
                    spellCheck={false}
                    disabled={!isNew || !canEditForm || busy}
                    className={`${INPUT_CLASS} font-mono text-xs`}
                    value={form.page_key}
                    onChange={e => {
                      setManualKey(true);
                      setForm(prev => ({ ...prev, page_key: e.target.value }));
                    }}
                  />
                </label>
              </div>

              <div className="grid gap-3 sm:grid-cols-2">
                <label htmlFor="opt-menu-label" className="block text-xs font-medium text-zinc-700">
                  <span>{__("Sidebar menu label *")}</span>
                  <input
                    id="opt-menu-label"
                    type="text"
                    required
                    maxLength={100}
                    disabled={!canEditForm || busy}
                    className={INPUT_CLASS}
                    value={form.menu_label}
                    onChange={e => setForm(prev => ({ ...prev, menu_label: e.target.value }))}
                  />
                </label>

                <label htmlFor="opt-parent-menu" className="block text-xs font-medium text-zinc-700">
                  <span>{__("Parent menu group")}</span>
                  <select
                    id="opt-parent-menu"
                    disabled={!canEditForm || busy}
                    className={INPUT_CLASS}
                    value={form.parent_menu}
                    onChange={e => setForm(prev => ({ ...prev, parent_menu: e.target.value as OptionsPageParentMenu }))}
                  >
                    <option value="settings">{__("System -> Settings")}</option>
                    <option value="content">{__("Content")}</option>
                    <option value="appearance">{__("Appearance -> Customize Studio")}</option>
                    <option value="system">{__("System")}</option>
                    <option value="top_level">{__("Top-level sidebar section")}</option>
                  </select>
                  {form.parent_menu === 'appearance' && (
                    <span className="mt-1 block text-[11px] font-normal text-zinc-500">
                      {__("Opens inside Customize Studio with live preview for Admins; Editors use the standalone form.")}
                    </span>
                  )}
                </label>
              </div>

              <div className="grid gap-3 sm:grid-cols-3">
                <label htmlFor="opt-min-role" className="block text-xs font-medium text-zinc-700">
                  <span>{__("Minimum edit role")}</span>
                  <select
                    id="opt-min-role"
                    disabled={!canEditForm || busy}
                    className={INPUT_CLASS}
                    value={form.min_edit_role}
                    onChange={e => setForm(prev => ({ ...prev, min_edit_role: e.target.value as FieldRole }))}
                  >
                    <option value="admin">{__("Admin only")}</option>
                    <option value="editor">{__("Editor & Admin")}</option>
                  </select>
                </label>

                <label htmlFor="opt-menu-icon" className="block text-xs font-medium text-zinc-700">
                  <span>{__("Menu icon")}</span>
                  <select
                    id="opt-menu-icon"
                    disabled={!canEditForm || busy}
                    className={INPUT_CLASS}
                    value={form.menu_icon}
                    onChange={e => setForm(prev => ({ ...prev, menu_icon: e.target.value }))}
                  >
                    <option value="">{__("Default")}</option>
                    {CORE_ICON_NAMES.map(icon => (
                      <option key={icon} value={icon}>{icon}</option>
                    ))}
                  </select>
                </label>

                <label htmlFor="opt-menu-pos" className="block text-xs font-medium text-zinc-700">
                  <span>{__("Menu position")}</span>
                  <input
                    id="opt-menu-pos"
                    type="number"
                    min={0}
                    max={1000}
                    disabled={!canEditForm || busy}
                    className={INPUT_CLASS}
                    value={menuPosText === '' ? '' : String(form.menu_position) === menuPosText ? menuPosText : String(form.menu_position)}
                    onChange={e => {
                      const raw = e.target.value;
                      setMenuPosText(raw);
                      if (raw.trim() !== '' && Number.isFinite(Number(raw))) {
                        setForm(prev => ({ ...prev, menu_position: Math.max(0, Math.min(1000, Math.trunc(Number(raw)))) }));
                      }
                    }}
                    onBlur={() => setMenuPosText(String(form.menu_position))}
                  />
                </label>
              </div>

              <label htmlFor="opt-description" className="block text-xs font-medium text-zinc-700">
                <span>{__("Description")}</span>
                <textarea
                  id="opt-description"
                  rows={2}
                  maxLength={400}
                  disabled={!canEditForm || busy}
                  className="mt-1 w-full rounded-md border border-zinc-300 bg-white px-2.5 py-1.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50"
                  value={form.description}
                  onChange={e => setForm(prev => ({ ...prev, description: e.target.value }))}
                />
              </label>

              {activeDef && activeDef.owner.kind !== 'user' && (
                <AdminAlert tone="warning">
                  {__("This options page is provider-managed. Only its assigned field group values can be edited.")}
                </AdminAlert>
              )}

              <div className="flex flex-wrap gap-2">
                <AdminButton
                  type="submit"
                  variant="primary"
                  disabled={!canEditForm || !csrfToken || busy || (!isNew && !dirty)}
                >
                  {busy ? __("Saving…") : __("Save options page")}
                </AdminButton>
                <AdminButton
                  type="button"
                  variant="secondary"
                  disabled={busy}
                  onClick={() => selectItem('')}
                >
                  {__("Close")}
                </AdminButton>
              </div>
            </form>
          )}
        </div>
      </div>

      <dialog
        ref={dialogRef}
        aria-labelledby="delete-options-page-title"
        className="fixed inset-0 m-auto w-full max-w-md rounded-xl border border-zinc-200 bg-white p-5 text-zinc-900 shadow-xl backdrop:bg-zinc-950/40 backdrop:overflow-hidden backdrop:[overscroll-behavior:contain] [overscroll-behavior:contain]"
        onCancel={e => {
          if (busy) {
            e.preventDefault();
            return;
          }
          setDeleteTarget(null);
        }}
      >
        {deleteTarget && (
          <div className="space-y-4">
            <h2 id="delete-options-page-title" className="text-base font-bold text-zinc-950">
              {__("Delete options page {{value1}}?", { value1: deleteTarget.title })}
            </h2>
            <p className="text-sm text-zinc-600">
              {__("Key")}: <code className="font-mono text-xs">{deleteTarget.page_key}</code>
            </p>
            <label className="flex items-start gap-2 rounded-lg border border-amber-200 bg-amber-50 p-3 text-xs text-amber-950">
              <input
                type="checkbox"
                checked={purgeValues}
                disabled={busy}
                onChange={e => setPurgeValues(e.target.checked)}
                className="mt-0.5 h-4 w-4 rounded border-zinc-300"
              />
              <span>{__("Also permanently purge all saved option values for this page")}</span>
            </label>
            <div className="flex justify-end gap-2">
              <AdminButton
                ref={cancelBtnRef}
                type="button"
                variant="secondary"
                disabled={busy}
                onClick={() => setDeleteTarget(null)}
              >
                {__("Cancel")}
              </AdminButton>
              <AdminButton
                type="button"
                variant="danger"
                disabled={busy || !csrfToken}
                onClick={() => void confirmDelete()}
              >
                {busy ? __("Deleting…") : __("Confirm delete")}
              </AdminButton>
            </div>
          </div>
        )}
      </dialog>
    </section>
  );
};
